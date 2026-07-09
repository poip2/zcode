//! Integration tests for the provider implementations.
//!
//! Run with: cargo test --test provider_smoke -- --nocapture

use futures::StreamExt;
use zcode_lib::error::Result;
use zcode_lib::model::{Message, UserContent, UserMessage};
use zcode_lib::provider::{Context, Provider, StreamOptions};
use zcode_lib::providers::{AnthropicProvider, OpenAIProvider};

fn print_stream_event(event: &zcode_lib::model::StreamEvent) {
    match event {
        zcode_lib::model::StreamEvent::Start { .. } => println!("[START]"),
        zcode_lib::model::StreamEvent::TextDelta { delta, .. } => print!("{delta}"),
        zcode_lib::model::StreamEvent::TextEnd { content, .. } => {
            println!("\n[TEXT END] {content}")
        }
        zcode_lib::model::StreamEvent::ThinkingStart { .. } => print!("[THINK] "),
        zcode_lib::model::StreamEvent::ThinkingDelta { delta, .. } => print!("{delta}"),
        zcode_lib::model::StreamEvent::ThinkingEnd { content, .. } => {
            println!("\n[THINK END] {content}")
        }
        zcode_lib::model::StreamEvent::Done { reason, message } => {
            println!("\n[DONE] stop_reason={reason:?}");
            println!(
                "Usage: input={} output={} total={}",
                message.usage.input, message.usage.output, message.usage.total_tokens,
            );
        }
        zcode_lib::model::StreamEvent::Error { error, .. } => {
            eprintln!("\n[ERROR] {:?}", error.error_message);
        }
        e => println!("\n[{e:?}]"),
    }
}

fn get_deepseek_base_url() -> String {
    dotenvy::dotenv().ok();
    std::env::var("ZCODE_DEEPSEEK_BASE_URL")
        .unwrap_or_else(|_| panic!("ZCODE_DEEPSEEK_BASE_URL env var must be set"))
}

fn get_deepseek_model() -> String {
    dotenvy::dotenv().ok();
    std::env::var("ZCODE_DEEPSEEK_MODEL")
        .unwrap_or_else(|_| panic!("ZCODE_DEEPSEEK_MODEL env var must be set"))
}

fn get_api_key() -> String {
    dotenvy::dotenv().ok();
    std::env::var("ZCODE_TEST_API_KEY")
        .unwrap_or_else(|_| panic!("ZCODE_TEST_API_KEY env var must be set"))
}

#[tokio::test]
async fn test_openai_deepseek() -> Result<()> {
    let model = get_deepseek_model();
    let provider = OpenAIProvider::new(
        "deepseek",
        &model,
        None::<String>,
        Some("https://api.deepseek.com/v1/chat/completions"),
    )?;

    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text("Say hello in exactly 3 words.".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];
    let context = Context {
        system_prompt: Some("Be brief."),
        messages: &messages,
        tools: &[],
    };
    let mut opts = StreamOptions {
        max_tokens: Some(100),
        ..Default::default()
    };
    opts.headers
        .insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    println!("\n=== OpenAI/DeepSeek (model={model}) ===");
    let mut stream = provider.stream(&context, &opts).await?;

    let mut response_text = String::new();
    let mut stream_error: Option<String> = None;
    let mut done_stop_reason: Option<zcode_lib::model::StopReason> = None;

    while let Some(e) = stream.next().await {
        match e {
            Ok(ev) => {
                print_stream_event(&ev);
                if let zcode_lib::model::StreamEvent::TextDelta { delta, .. } = &ev {
                    response_text.push_str(delta);
                }
                if let zcode_lib::model::StreamEvent::Done { reason, .. } = &ev {
                    done_stop_reason = Some(*reason);
                }
                if let zcode_lib::model::StreamEvent::Error { error, .. } = &ev {
                    stream_error = error.error_message.clone();
                }
            }
            Err(e) => {
                eprintln!("Stream error: {e}");
                stream_error = Some(e.to_string());
            }
        }
    }

    // Assertions
    assert!(
        stream_error.is_none(),
        "Stream error: {}",
        stream_error.unwrap_or_default()
    );
    assert!(
        !response_text.is_empty(),
        "Response text is empty — model returned no content"
    );
    assert!(
        done_stop_reason != Some(zcode_lib::model::StopReason::Error),
        "Done event indicates an error"
    );
    Ok(())
}

#[tokio::test]
async fn test_anthropic_deepseek() -> Result<()> {
    let model = get_deepseek_model();
    let base_url = get_deepseek_base_url();
    // AnthropicProvider does NOT auto-append /v1/messages; POST goes to
    // the exact URL given. DeepSeek's Anthropic-compatible endpoint lives
    // at /anthropic/v1/messages, so we append the suffix here.
    let base_url = if base_url.ends_with("/v1/messages") {
        base_url
    } else {
        format!("{base_url}/v1/messages")
    };

    let provider = AnthropicProvider::new(&model, None::<String>, Some(&base_url))?;

    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text("Say hello in exactly 3 words. No thinking needed.".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];
    let context = Context {
        system_prompt: Some("Be brief."),
        messages: &messages,
        tools: &[],
    };
    let mut opts = StreamOptions {
        max_tokens: Some(200),
        ..Default::default()
    };
    opts.headers
        .insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    println!("\n=== Anthropic/DeepSeek (model={model}, url={base_url}) ===");
    let mut stream = provider.stream(&context, &opts).await?;

    let mut response_text = String::new();
    let mut stream_error: Option<String> = None;
    let mut done_stop_reason: Option<zcode_lib::model::StopReason> = None;

    while let Some(e) = stream.next().await {
        match e {
            Ok(ev) => {
                print_stream_event(&ev);
                if let zcode_lib::model::StreamEvent::TextDelta { delta, .. } = &ev {
                    response_text.push_str(delta);
                }
                if let zcode_lib::model::StreamEvent::Done { reason, .. } = &ev {
                    done_stop_reason = Some(*reason);
                }
                if let zcode_lib::model::StreamEvent::Error { error, .. } = &ev {
                    stream_error = error.error_message.clone();
                }
            }
            Err(e) => {
                eprintln!("Stream error: {e}");
                stream_error = Some(e.to_string());
            }
        }
    }

    // Assertions
    assert!(
        stream_error.is_none(),
        "Stream error: {}",
        stream_error.unwrap_or_default()
    );
    assert!(
        !response_text.is_empty(),
        "Response text is empty — model returned no content"
    );
    assert!(
        done_stop_reason != Some(zcode_lib::model::StopReason::Error),
        "Done event indicates an error"
    );
    Ok(())
}
