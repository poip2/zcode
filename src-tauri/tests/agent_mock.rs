//! Mock-based agent loop test — demonstrates the agent orchestration
//! (provider → tool → message loop) without requiring a real LLM API key.
//!
//! Run: cargo test --test agent_mock -- --nocapture

use async_trait::async_trait;
use futures::stream::{self, Stream};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;
use zcode_lib::agent::{Agent, AgentConfig, AgentEvent};
use zcode_lib::error::Result;
use zcode_lib::model::{
    AssistantMessage, ContentBlock, Message, StopReason, StreamEvent, TextContent, ToolCall, Usage,
};
use zcode_lib::provider::{Context, Provider, StreamOptions};
use zcode_lib::tools::ToolRegistry;

/// A mock provider that returns a tool call followed by a final text response.
struct MockProvider {
    responses: Vec<Vec<ContentBlock>>,
    call_count: std::sync::atomic::AtomicUsize,
}

impl MockProvider {
    fn new(responses: Vec<Vec<ContentBlock>>) -> Self {
        Self {
            responses,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> &str {
        "mock"
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
            .unwrap_or_else(|| vec![ContentBlock::Text(TextContent::new("Fallback response"))]);

        // Build text deltas from content blocks
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
async fn test_agent_loop_with_mock_provider() -> Result<()> {
    eprintln!("=== Agent Loop Mock Test ===");

    // Set up working dir with a test file
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path();
    std::fs::write(cwd.join("hello.txt"), "Hello from mock test!\n")?;

    // Mock: first call returns a tool call to read "hello.txt",
    // second call returns the final text response.
    let mock = Arc::new(MockProvider::new(vec![
        // Turn 1: tool call → read hello.txt
        vec![ContentBlock::ToolCall(ToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path": "hello.txt"}),
            thought_signature: None,
        })],
        // Turn 2: final text response
        vec![ContentBlock::Text(TextContent::new(
            "The file contains: Hello from mock test!",
        ))],
    ]));

    let tools = ToolRegistry::new(&["read"], cwd);

    let config = AgentConfig {
        system_prompt: Some("You are a helpful assistant.".into()),
        max_tool_iterations: 5,
        ..Default::default()
    };

    let mut agent = Agent::new(mock, tools, config);
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let events_clone = Arc::clone(&events);

    let result = agent
        .run(
            "What's in hello.txt?",
            move |ev| {
                let label = match &ev {
                    AgentEvent::AgentStart { .. } => "[AgentStart]".into(),
                    AgentEvent::AgentEnd { error, .. } => format!("[AgentEnd error={:?}]", error),
                    AgentEvent::TurnStart { turn_index, .. } => {
                        format!("[TurnStart #{turn_index}]")
                    }
                    AgentEvent::TurnEnd { .. } => "[TurnEnd]".into(),
                    AgentEvent::MessageUpdate { delta, .. } => delta.clone(),
                    AgentEvent::MessageEnd { message } => {
                        if let Message::Assistant(ref m) = message {
                            let has_tool = m
                                .content
                                .iter()
                                .any(|b| matches!(b, ContentBlock::ToolCall(_)));
                            format!("[MessageEnd has_tool_call={has_tool}]")
                        } else {
                            "[MessageEnd]".into()
                        }
                    }
                    AgentEvent::ToolStart { tool_name, .. } => format!("[ToolStart: {tool_name}]"),
                    AgentEvent::ToolEnd {
                        tool_name,
                        is_error,
                        ..
                    } => {
                        format!("[ToolEnd: {tool_name} error={is_error}]")
                    }
                    _ => format!("[{:?}]", std::mem::discriminant(&ev)),
                };
                eprintln!("{label}");
                events_clone.lock().unwrap().push(label);
            },
            CancellationToken::new(),
        )
        .await;

    let msg = result?;

    // Verify final text
    let final_text: String = msg
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
    eprintln!("\nFinal response: {final_text}");
    assert!(
        final_text.contains("Hello from mock test!"),
        "Expected final response to contain file content"
    );

    // Verify the tool was called
    let events = events.lock().unwrap();
    let tool_starts: Vec<_> = events
        .iter()
        .filter(|e| e.starts_with("[ToolStart"))
        .collect();
    assert!(!tool_starts.is_empty(), "Expected at least one tool call");

    // Verify two turns happened
    let turn_starts: Vec<_> = events
        .iter()
        .filter(|e| e.starts_with("[TurnStart"))
        .collect();
    assert_eq!(
        turn_starts.len(),
        2,
        "Expected 2 turns (tool call + final response)"
    );

    eprintln!("\nPASS: Agent loop with mock provider works correctly");
    eprintln!("  - Tool call was issued and executed");
    eprintln!("  - Final response contains expected content");
    eprintln!("  - Two turns completed");
    Ok(())
}
