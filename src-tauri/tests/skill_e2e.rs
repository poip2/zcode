//! Verification 3: End-to-end skills injection + model recognition.
//!
//! Run: cargo test --test skill_e2e -- --nocapture
//! Requires: ZCODE_TEST_API_KEY env var

use zcode_lib::error::Result;
use zcode_lib::model::{Message, UserContent, UserMessage};
use zcode_lib::provider::{Context, Provider, StreamOptions};
use zcode_lib::providers::OpenAIProvider;
use zcode_lib::skills;
use futures::StreamExt;
use std::io::Write;
use std::path::PathBuf;

const MODEL: &str = "deepseek-v4-flash";

fn get_api_key() -> String {
    std::env::var("ZCODE_TEST_API_KEY")
        .unwrap_or_else(|_| panic!("ZCODE_TEST_API_KEY env var must be set"))
}

#[tokio::test]
async fn test_skill_injection_model_sees_skill() -> Result<()> {
    // 1. Create a test SKILL.md
    let tmp = tempfile::tempdir()?;
    let skill_dir = tmp.path().join(".zcode").join("skills").join("test-skill");
    std::fs::create_dir_all(&skill_dir)?;

    let skill_content = r#"---
name: test-skill
description: Provides a specific greeting protocol for the user. When asked to greet, you MUST respond with "GREETINGS HUMAN" in all caps.
---

# Test Skill: Greeting Protocol

## Instructions
When the user asks you to say hello or greet them, you must follow the protocol:
1. Say "GREETINGS HUMAN"
2. Then ask if they need any assistance
"#;
    std::fs::write(skill_dir.join("SKILL.md"), skill_content)?;

    // 2. Load skills
    let (loaded_skills, diags) = skills::load_skills(tmp.path(), None, &[]);
    eprintln!("Diagnostics: {:?}", diags);
    assert_eq!(loaded_skills.len(), 1);
    assert_eq!(loaded_skills[0].name, "test-skill");
    eprintln!("Skill loaded: {} - {}", loaded_skills[0].name, loaded_skills[0].description);

    // 3. Format as XML
    let skills_xml = skills::format_skills_for_prompt(&loaded_skills);
    assert!(!skills_xml.is_empty());
    eprintln!("Skills XML (first 200 chars): {}", &skills_xml[..200.min(skills_xml.len())]);

    // 4. Build system prompt with skills injected
    let system_prompt = format!(
        "You are a helpful assistant. Follow all skill instructions when applicable.\n{skills_xml}"
    );

    // 5. Create provider and send prompt
    let provider = OpenAIProvider::new(
        "deepseek",
        MODEL,
        None::<String>,
        Some("https://api.deepseek.com/v1/chat/completions"),
    )?;

    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text("Please greet me.".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];

    let context = Context {
        system_prompt: Some(&system_prompt),
        messages: &messages,
        tools: &[],
    };

    let mut opts = StreamOptions {
        max_tokens: Some(100),
        ..Default::default()
    };
    opts.headers.insert(
        "Authorization".into(),
        format!("Bearer {}", get_api_key()),
    );

    eprintln!("\n--- Streaming with skill-injected prompt ---");
    let mut stream = provider.stream(&context, &opts).await?;
    let mut full_response = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(zcode_lib::model::StreamEvent::TextDelta { delta, .. }) => {
                print!("{delta}");
                full_response.push_str(&delta);
            }
            Ok(zcode_lib::model::StreamEvent::Done { reason, message }) => {
                eprintln!("\n\n[DONE] reason={reason:?}");
                eprintln!("Input tokens: {}, Output tokens: {}",
                    message.usage.input, message.usage.output);
            }
            Ok(zcode_lib::model::StreamEvent::Error { error, .. }) => {
                eprintln!("\n[ERROR] {:?}", error.error_message);
            }
            Err(e) => eprintln!("\n[STREAM ERR] {e}"),
            _ => {}
        }
    }

    eprintln!("Full response: {full_response}");

    // 6. Verify model followed the skill instruction
    let upper = full_response.to_uppercase();
    assert!(
        upper.contains("GREETINGS HUMAN"),
        "Model should have followed skill instruction to say GREETINGS HUMAN. Got: {full_response}"
    );
    eprintln!("\nPASS: Model successfully followed skill instruction.");

    Ok(())
}
