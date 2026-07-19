//! End-to-end agent integration test.
//!
//! Run: cargo test --test agent_e2e -- --nocapture

use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use zcode_lib::agent::{Agent, AgentConfig, AgentEvent};
use zcode_lib::error::Result;
use zcode_lib::provider::StreamOptions;
use zcode_lib::providers::OpenAIProvider;
use zcode_lib::tools::ToolRegistry;

const MODEL: &str = "deepseek-v4-flash";

fn get_api_key() -> String {
    std::env::var("ZCODE_TEST_API_KEY")
        .unwrap_or_else(|_| panic!("ZCODE_TEST_API_KEY env var must be set"))
}

#[tokio::test]
async fn test_agent_basic_chat() -> Result<()> {
    eprintln!("=== Agent Basic Chat ===");

    let provider = Arc::new(OpenAIProvider::new(
        "deepseek",
        MODEL,
        None::<String>,
        Some("https://api.deepseek.com/v1/chat/completions"),
    )?);

    let tools = ToolRegistry::new(&[], Path::new("."));

    let mut stream_opts = StreamOptions {
        max_tokens: Some(100),
        temperature: Some(0.7),
        ..Default::default()
    };
    stream_opts
        .headers
        .insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    let config = AgentConfig {
        system_prompt: Some("You are a helpful assistant. Be brief.".into()),
        max_tool_iterations: 50,
        stream_options: stream_opts,
    };

    let mut agent = Agent::new(provider, tools, config);

    let result = agent
        .run("Say hello in exactly 3 words.", move |ev| {
            let label = match &ev {
                AgentEvent::AgentStart { .. } => "[AgentStart]".into(),
                AgentEvent::AgentEnd { error, .. } => format!("[AgentEnd error={:?}]", error),
                AgentEvent::TurnStart { turn_index, .. } => format!("[TurnStart #{turn_index}]"),
                AgentEvent::TurnEnd { .. } => "[TurnEnd]".into(),
                AgentEvent::MessageUpdate { delta, .. } => delta.clone(),
                AgentEvent::ToolStart { tool_name, .. } => format!("[Tool: {tool_name}]"),
                AgentEvent::ToolEnd {
                    tool_name,
                    is_error,
                    ..
                } => {
                    format!("[ToolEnd: {tool_name} err={is_error}]")
                }
                _ => format!("[{:?}]", std::mem::discriminant(&ev)),
            };
            if !label.is_empty() && !label.starts_with('[') {
                print!("{label}");
            } else {
                eprintln!("{label}");
            }
        }, CancellationToken::new())
        .await;

    match &result {
        Ok(msg) => {
            let text: String = msg
                .content
                .iter()
                .filter_map(|b| {
                    if let zcode_lib::model::ContentBlock::Text(tc) = b {
                        Some(tc.text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            eprintln!("\nFinal response: {text}");
            eprintln!(
                "Usage: input={} output={} total={}",
                msg.usage.input, msg.usage.output, msg.usage.total_tokens
            );
            eprintln!("PASS: Agent basic chat works");
        }
        Err(e) => eprintln!("Agent error: {e}"),
    }
    Ok(())
}

#[tokio::test]
async fn test_agent_with_tools() -> Result<()> {
    eprintln!("\n=== Agent With Tools ===");

    // Create temp working dir
    let tmp = tempfile::tempdir()?;
    let cwd = tmp.path();
    std::fs::write(
        cwd.join("hello.txt"),
        "Hello, world!\nThis is a test file.\n",
    )?;

    let provider = Arc::new(OpenAIProvider::new(
        "deepseek",
        MODEL,
        None::<String>,
        Some("https://api.deepseek.com/v1/chat/completions"),
    )?);

    let tools = ToolRegistry::new(&["read", "ls"], cwd);

    let mut stream_opts = StreamOptions {
        max_tokens: Some(200),
        ..Default::default()
    };
    stream_opts
        .headers
        .insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    let config = AgentConfig {
        system_prompt: Some(format!(
            "You are an expert coding assistant operating inside zcode. \
             You help users by reading files and executing commands.\n\
             Available tools: read, ls\n\
             Working directory: {}",
            cwd.display()
        )),
        max_tool_iterations: 10,
        stream_options: stream_opts,
    };

    let mut agent = Agent::new(provider, tools, config);

    let result = agent
        .run(
            "Use the ls tool to list the current directory, then use read on whatever files you find.",
            move |ev| {
                match &ev {
                    AgentEvent::TurnStart { turn_index, .. } => eprintln!("--- Turn #{turn_index} ---"),
                    AgentEvent::ToolStart { tool_name, .. } => eprintln!("  [Tool: {tool_name}]"),
                    AgentEvent::ToolEnd { tool_name, is_error, .. } => {
                        eprintln!("  [ToolEnd: {tool_name} error={is_error}]")
                    }
                    AgentEvent::MessageUpdate { delta, .. } => print!("{delta}"),
                    AgentEvent::AgentEnd { .. } => eprintln!("\n[AgentEnd]"),
                    _ => {}
                }
            },
            CancellationToken::new(),
        )
        .await;

    match &result {
        Ok(_) => eprintln!("PASS: Agent with tools works"),
        Err(e) => eprintln!("Agent error: {e}"),
    }
    Ok(())
}
