//! Integration tests for the provider implementations.
//!
//! Run with: cargo test --test provider_smoke -- --nocapture

use zcode_lib::error::Result;
use zcode_lib::model::{Message, UserContent, UserMessage};
use zcode_lib::provider::{Context, Provider, StreamOptions};
use zcode_lib::providers::{AnthropicProvider, OpenAIProvider};
use futures::StreamExt;

fn print_stream_event(event: &zcode_lib::model::StreamEvent) {
    match event {
        zcode_lib::model::StreamEvent::Start { .. } => println!("[START]"),
        zcode_lib::model::StreamEvent::TextDelta { delta, .. } => print!("{delta}"),
        zcode_lib::model::StreamEvent::TextEnd { content, .. } => println!("\n[TEXT END] {content}"),
        zcode_lib::model::StreamEvent::ThinkingStart { .. } => print!("[THINK] "),
        zcode_lib::model::StreamEvent::ThinkingDelta { delta, .. } => print!("{delta}"),
        zcode_lib::model::StreamEvent::ThinkingEnd { content, .. } => println!("\n[THINK END] {content}"),
        zcode_lib::model::StreamEvent::Done { reason, message } => {
            println!("\n[DONE] stop_reason={reason:?}");
            println!(
                "Usage: input={} output={} total={}",
                message.usage.input,
                message.usage.output,
                message.usage.total_tokens,
            );
        }
        zcode_lib::model::StreamEvent::Error { error, .. } => {
            eprintln!("\n[ERROR] {:?}", error.error_message);
        }
        e => println!("\n[{e:?}]"),
    }
}

const MODEL: &str = "deepseek-v4-flash";

fn get_api_key() -> String {
    std::env::var("ZCODE_TEST_API_KEY")
        .unwrap_or_else(|_| panic!("ZCODE_TEST_API_KEY env var must be set"))
}

#[tokio::test]
async fn test_openai_deepseek() -> Result<()> {
    let provider = OpenAIProvider::new(
        "deepseek",
        MODEL,
        None::<String>,
        Some("https://api.deepseek.com/v1/chat/completions"),
    )?;

    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text("Say hello in exactly 3 words.".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];
    let context = Context { system_prompt: Some("Be brief."), messages: &messages, tools: &[] };
    let mut opts = StreamOptions { max_tokens: Some(100), ..Default::default() };
    opts.headers.insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    println!("\n=== OpenAI/DeepSeek ===");
    let mut stream = provider.stream(&context, &opts).await?;
    while let Some(e) = stream.next().await {
        match e {
            Ok(ev) => print_stream_event(&ev),
            Err(e) => eprintln!("Error: {e}"),
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_anthropic_deepseek() -> Result<()> {
    let provider = AnthropicProvider::new(
        MODEL,
        None::<String>,
        Some("https://api.deepseek.com/anthropic/v1/messages"),
    )?;

    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text("Say hello in exactly 3 words. No thinking needed.".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];
    let context = Context { system_prompt: Some("Be brief."), messages: &messages, tools: &[] };
    let mut opts = StreamOptions { max_tokens: Some(200), ..Default::default() };
    opts.headers.insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    println!("\n=== Anthropic/DeepSeek ===");
    let mut stream = provider.stream(&context, &opts).await?;
    while let Some(e) = stream.next().await {
        match e {
            Ok(ev) => print_stream_event(&ev),
            Err(e) => eprintln!("Error: {e}"),
        }
    }
    Ok(())
}
