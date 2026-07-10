//! Verification: model reads skill from real .zcode/skills/test-skill/SKILL.md
//! and follows the "喵" protocol.
//!
//! Run: cargo test --test meow_e2e -- --nocapture

use futures::StreamExt;
use std::path::Path;
use zcode_lib::model::{Message, StreamEvent, UserContent, UserMessage};
use zcode_lib::provider::{Context, Provider, StreamOptions};
use zcode_lib::providers::OpenAIProvider;
use zcode_lib::skills;

const MODEL: &str = "deepseek-v4-flash";
const BASE_URL: &str = "https://api.deepseek.com/v1/chat/completions";

fn get_api_key() -> String {
    std::env::var("ZCODE_TEST_API_KEY").expect("ZCODE_TEST_API_KEY env var must be set")
}

#[tokio::test]
async fn test_meow_skill_injection() -> zcode_lib::error::Result<()> {
    // 1. Load skills from the real project dir (where .zcode/skills/ lives)
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let user_cfg = dirs::config_dir().map(|d| d.join("zcode"));

    let (all_skills, diags) = skills::load_skills(project_root, user_cfg.as_deref(), &[]);
    eprintln!("Diagnostics: {:?}", diags);
    eprintln!(
        "Found {} skill(s): {:?}",
        all_skills.len(),
        all_skills.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    assert!(
        all_skills.iter().any(|s| s.name == "test-skill"),
        "test-skill not found in discovered skills"
    );

    let skills_xml = skills::format_skills_for_prompt(&all_skills);
    assert!(!skills_xml.is_empty());
    // The SKILL.md content includes "喵" in the description and body
    assert!(
        skills_xml.contains("喵"),
        "Skills XML should contain 喵 from our SKILL.md"
    );
    eprintln!(
        "Skills XML (first 300 chars): {}",
        &skills_xml[..300.min(skills_xml.len())]
    );

    // 2. Build system prompt (mimicking build_system_prompt)
    let system_prompt = format!(
        "You are an AI assistant embedded in zcode, a Markdown editor.\n\
         Working directory: {}\n\n\
         {skills_xml}\n\n\
         Always respond in the same language as the user's message.\n",
        project_root.display()
    );

    // 3. Send message to model
    let provider = OpenAIProvider::new("deepseek", MODEL, None::<String>, Some(BASE_URL))?;

    let messages = vec![Message::User(UserMessage {
        content: UserContent::Text("你好，你是谁？".to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    })];

    let context = Context {
        system_prompt: Some(&system_prompt),
        messages: &messages,
        tools: &[],
    };

    let mut opts = StreamOptions {
        max_tokens: Some(150),
        ..Default::default()
    };
    opts.headers
        .insert("Authorization".into(), format!("Bearer {}", get_api_key()));

    eprintln!("\n--- Streaming with '喵' skill injected ---");
    let mut stream = provider.stream(&context, &opts).await?;
    let mut full = String::new();

    while let Some(e) = stream.next().await {
        match e {
            Ok(StreamEvent::TextDelta { delta, .. }) => {
                print!("{delta}");
                full.push_str(&delta);
            }
            Ok(StreamEvent::Done { .. }) => eprintln!("\n[DONE]"),
            Ok(StreamEvent::Error { error, .. }) => {
                eprintln!("\n[ERR] {:?}", error.error_message)
            }
            Err(e) => eprintln!("\n[STREAM ERR] {e}"),
            _ => {}
        }
    }

    eprintln!("\n\n=== Result ===");
    eprintln!("Full response: {full}");

    // 4. Verify model started with "喵"
    assert!(
        full.trim_start().starts_with("喵"),
        "Model should start with '喵' per skill instruction.\nGot: {full}"
    );

    eprintln!("\n✅ PASS: Model followed '喵' skill instruction!");
    Ok(())
}
