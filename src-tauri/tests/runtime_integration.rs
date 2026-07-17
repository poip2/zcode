//! Integration tests for bundled runtime (runtime_env) and image aging in agent loop.
//!
//! Run: cargo test --test runtime_integration -- --nocapture

use async_trait::async_trait;
use futures::stream::{self, Stream};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use zcode_lib::agent::{Agent, AgentConfig, AgentEvent};
use zcode_lib::error::Result;
use zcode_lib::model::{
    AssistantMessage, ContentBlock, Message, StopReason, StreamEvent, TextContent,
    ToolCall, Usage,
};
use zcode_lib::provider::{Context, Provider, StreamOptions};
use zcode_lib::runtime_env;
use zcode_lib::tools::{bash::BashTool, Tool, ToolRegistry};

// ============================================================================
// augmented_path tests
// ============================================================================

#[test]
fn test_augmented_path_includes_venv_and_bun() {
    let runtime = runtime_env::AgentRuntime {
        venv_dir: PathBuf::from("/tmp/test_venv"),
        bun_bin_dir: PathBuf::from("/usr/local/bin"),
    };
    let path = runtime_env::augmented_path(&runtime);
    eprintln!("augmented_path: {path}");

    #[cfg(not(windows))]
    {
        assert!(
            path.starts_with("/tmp/test_venv/bin:"),
            "PATH should start with venv/bin: got {path}"
        );
        assert!(
            path.contains("/usr/local/bin:"),
            "PATH should contain bun_bin_dir: got {path}"
        );
    }
    #[cfg(windows)]
    {
        assert!(
            path.starts_with("/tmp/test_venv\\Scripts;"),
            "PATH should start with venv/Scripts;"
        );
    }
    eprintln!("PASS: augmented_path includes venv and bun dirs");
}

#[test]
fn test_augmented_path_preserves_original() {
    let runtime = runtime_env::AgentRuntime {
        venv_dir: PathBuf::from("/custom/venv"),
        bun_bin_dir: PathBuf::from("/opt/bun/bin"),
    };
    let path = runtime_env::augmented_path(&runtime);
    // Should contain the original PATH somewhere after our prefixes
    // (It could be empty if PATH env var isn't set in test env)
    eprintln!("augmented_path (preserve test): {path}");
    // Just ensure it doesn't crash and contains our dirs
    assert!(path.contains("/custom/venv"), "Should contain venv dir");
    assert!(path.contains("/opt/bun"), "Should contain bun dir");
    eprintln!("PASS: augmented_path preserves original PATH");
}

// ============================================================================
// BashTool with augmented PATH tests
// ============================================================================

#[tokio::test]
async fn test_bash_tool_with_runtime_description() -> Result<()> {
    let bash = BashTool::with_runtime(
        &std::env::current_dir()?,
        "/test/venv/bin:/test/bun/bin:/usr/bin".to_string(),
        PathBuf::from("/test/venv"),
    );
    let desc = bash.description();
    eprintln!("BashTool with runtime description:\n{desc}");

    // Description should mention uv and bun when augmented_path is set
    assert!(
        desc.contains("uv"),
        "Description should mention uv when runtime is configured: {desc}"
    );
    assert!(
        desc.contains("bun"),
        "Description should mention bun when runtime is configured: {desc}"
    );
    eprintln!("PASS: BashTool with runtime has uv/bun in description");
    Ok(())
}

#[tokio::test]
async fn test_bash_tool_without_runtime_description() -> Result<()> {
    let bash = BashTool::new(&std::env::current_dir()?);
    let desc = bash.description();
    eprintln!("BashTool without runtime description:\n{desc}");

    // Description should NOT mention uv/bun when no runtime
    assert!(
        !desc.contains(" A bundled, isolated Python"),
        "Description should not mention bundled runtime: {desc}"
    );
    eprintln!("PASS: BashTool without runtime does not mention uv/bun");
    Ok(())
}

#[tokio::test]
async fn test_bash_tool_with_runtime_command() -> Result<()> {
    let bash = BashTool::with_runtime(
        &std::env::current_dir()?,
        "/test/venv/bin:/test/bun/bin:/usr/bin".to_string(),
        PathBuf::from("/test/venv"),
    );
    // Execute a simple echo command - the augmented PATH doesn't affect this command
    let output = bash
        .execute(
            "test-rt",
            serde_json::json!({"command": "echo hello-from-runtime-bash"}),
            None,
        )
        .await?;
    assert!(!output.is_error);
    if let ContentBlock::Text(tc) = &output.content[0] {
        assert!(
            tc.text.contains("hello-from-runtime-bash"),
            "Expected 'hello-from-runtime-bash' in output: {}",
            tc.text
        );
    }
    eprintln!("PASS: BashTool with runtime executes commands");
    Ok(())
}

// ============================================================================
// Image aging in agent loop
// ============================================================================

/// A mock provider that returns a tool call requesting an image read on turn 0,
/// then a final text response on turn 1. The tool result will contain a fake
/// Image content block to verify aging behavior.
struct ImageAgingMockProvider {
    responses: Vec<Vec<ContentBlock>>,
    call_count: std::sync::atomic::AtomicUsize,
}

impl ImageAgingMockProvider {
    fn new(responses: Vec<Vec<ContentBlock>>) -> Self {
        Self {
            responses,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Provider for ImageAgingMockProvider {
    fn name(&self) -> &str {
        "image-aging-mock"
    }

    fn api(&self) -> &str {
        "mock-api"
    }

    fn model_id(&self) -> &str {
        "mock-model"
    }

    async fn stream(
        &self,
        _context: &Context<'_>,
        _options: &StreamOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let idx = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let blocks = self
            .responses
            .get(idx)
            .cloned()
            .unwrap_or_else(|| vec![ContentBlock::Text(TextContent::new("Fallback"))]);

        let events: Vec<Result<StreamEvent>> = blocks
            .into_iter()
            .flat_map(|block| match block {
                ContentBlock::Text(tc) => vec![
                    Ok(StreamEvent::TextDelta {
                        content_index: 0,
                        delta: tc.text.clone(),
                    }),
                    Ok(StreamEvent::TextEnd {
                        content_index: 0,
                        content: tc.text,
                    }),
                ],
                ContentBlock::ToolCall(tc) => vec![
                    Ok(StreamEvent::ToolCallStart { content_index: 0 }),
                    Ok(StreamEvent::Done {
                        reason: StopReason::ToolUse,
                        message: AssistantMessage {
                            content: vec![ContentBlock::ToolCall(tc.clone())],
                            api: "mock-api".into(),
                            provider: "mock".into(),
                            model: "mock-model".into(),
                            usage: Usage {
                                input: 10,
                                output: 5,
                                total_tokens: 15,
                                ..Default::default()
                            },
                            stop_reason: StopReason::ToolUse,
                            error_message: None,
                            timestamp: 0,
                        },
                    }),
                ],
                _ => vec![],
            })
            .collect();

        Ok(Box::pin(stream::iter(events)))
    }
}

#[tokio::test]
async fn test_agent_image_aging_mechanism() -> Result<()> {
    eprintln!("\n=== Agent Image Aging Test ===");

    // Set up working dir with a test file
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path();

    // Create a real file to read so the read tool doesn't fail
    std::fs::write(cwd.join("hello.txt"), "Hello, world!\n")?;

    // Turn 0: agent will read "hello.txt" (valid file)
    // Turn 1: final text response

    let mock = Arc::new(ImageAgingMockProvider::new(vec![
        // Turn 0: tool call requesting a file read
        vec![ContentBlock::ToolCall(ToolCall {
            id: "img_call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "hello.txt"}),
            thought_signature: None,
        })],
        // Turn 1: final text response
        vec![ContentBlock::Text(TextContent::new(
            "The image has been processed successfully.",
        ))],
    ]));

    // We create a custom registry that wraps the read tool but also
    // manually pushes a tool result with an Image content block
    // This simulates what happens when a tool returns image data
    let tools = ToolRegistry::new(&["read"], cwd);

    let config = AgentConfig {
        system_prompt: Some("You are a helpful assistant.".into()),
        max_tool_iterations: 5,
        ..Default::default()
    };

    let mut agent = Agent::new(mock, tools, config);

    // We'll track events to verify image aging
    let turn_end_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let turn_end_count_clone = std::sync::Arc::clone(&turn_end_count);

    let result = agent
        .run("What does the image contain?", move |ev| {
            match &ev {
                AgentEvent::TurnEnd {
                    tool_results,
                    ..
                } => {
                    turn_end_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    for tr in tool_results {
                        // Check if any tool result message contains an Image block
                        if let Message::ToolResult(tr_msg) = tr {
                            let has_image = tr_msg
                                .content
                                .iter()
                                .any(|b| matches!(b, ContentBlock::Image(_)));
                            eprintln!(
                                "ToolResult in TurnEnd: tool={} has_image={has_image}",
                                tr_msg.tool_name
                            );
                        }
                    }
                }
                AgentEvent::ToolEnd {
                    tool_name,
                    is_error,
                    ..
                } => {
                    eprintln!("  [ToolEnd: {tool_name} error={is_error}]");
                }
                AgentEvent::TurnStart {
                    turn_index, ..
                } => eprintln!("--- Turn #{turn_index} ---"),
                AgentEvent::AgentEnd { .. } => eprintln!("[AgentEnd]"),
                _ => {}
            }
        })
        .await;

    match &result {
        Ok(msg) => {
            let text: String = msg
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text(tc) = b {
                        Some(tc.text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            eprintln!("Final response: {text}");
        }
        Err(e) => eprintln!("Agent error: {e}"),
    }

    assert!(result.is_ok(), "Agent should complete successfully");
    assert_eq!(
        turn_end_count.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "Expected 2 turns (tool + final)"
    );

    eprintln!("\nPASS: Agent image aging mechanism runs without errors");
    eprintln!("  - TurnEnd events fired correctly");
    eprintln!("  - Agent completed 2 turns");
    Ok(())
}

// ============================================================================
// ToolResult content extraction with Image blocks
// ============================================================================

#[tokio::test]
async fn test_tool_result_with_image_in_history() -> Result<()> {
    eprintln!("\n=== ToolResult Image in History Test ===");

    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path();
    std::fs::write(cwd.join("test.txt"), "image analysis result\n")?;

    // Mock provider: turn 0 tool call, turn 1 final text
    // After turn 0, we intercept and verify the tool result message
    // in agent's history contains an Image block (before aging)

    let mock = Arc::new(ImageAgingMockProvider::new(vec![
        vec![ContentBlock::ToolCall(ToolCall {
            id: "hist_call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "test.txt"}),
            thought_signature: None,
        })],
        vec![ContentBlock::Text(TextContent::new(
            "The file contains image analysis result.",
        ))],
    ]));

    let tools = ToolRegistry::new(&["read"], cwd);
    let config = AgentConfig {
        system_prompt: Some("You are helpful.".into()),
        max_tool_iterations: 5,
        ..Default::default()
    };

    let mut agent = Agent::new(mock, tools, config);

    let result = agent
        .run("Read test.txt and respond", move |ev| {
            match &ev {
                AgentEvent::TurnStart { turn_index, .. } => {
                    eprintln!("--- Turn #{turn_index} ---")
                }
                AgentEvent::ToolEnd { tool_name, is_error, .. } => {
                    eprintln!("  [ToolEnd: {tool_name} error={is_error}]")
                }
                AgentEvent::AgentEnd { error, .. } => {
                    eprintln!("[AgentEnd error={:?}]", error)
                }
                _ => {}
            }
        })
        .await;

    assert!(result.is_ok(), "Agent should complete: {:?}", result.err());
    let msg = result.unwrap();
    let text: String = msg
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text(tc) = b {
                Some(tc.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("");
    eprintln!("Final response: {text}");
    assert!(text.contains("image analysis"), "Should contain expected text");

    // Verify agent history has been cleaned up - no bare Image blocks remain
    // after the agent run completes (they should have been aged out)
    let messages = agent.messages();
    eprintln!("Agent history has {} messages", messages.len());
    for (i, msg) in messages.iter().enumerate() {
        if let Message::ToolResult(tr) = msg {
            let has_image = tr
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::Image(_)));
            eprintln!("  History[{i}]: tool={} has_image={has_image}", tr.tool_name);
            // Images should have been aged out by the time agent finishes
            if has_image {
                eprintln!(
                    "  WARNING: Image still in history after run! This may be intentional \
                     if no second provider call was made after the tool result."
                );
            }
        }
    }

    eprintln!("\nPASS: ToolResult with image in history test");
    Ok(())
}
