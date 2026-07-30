//! Tool integration tests.
//!
//! Run: cargo test --test tool_smoke -- --nocapture

use std::path::Path;
use zcode_lib::error::Result;
use zcode_lib::tools::ToolRegistry;

#[test]
fn test_tool_registry_contains_only_core_tools() {
    let cwd = Path::new(".");
    let enabled = &["read", "write", "edit", "shell", "grep", "find", "ls"];
    let registry = ToolRegistry::new(enabled, cwd);

    let names: Vec<&str> = registry.tools().iter().map(|t| t.name()).collect();
    eprintln!("Registered tools: {names:?}");
    assert_eq!(names, vec!["read", "write", "edit", "shell"]);
    assert!(registry.get("grep").is_none());
    assert!(registry.get("find").is_none());
    assert!(registry.get("ls").is_none());
    assert!(registry.get("bash").is_none());
    eprintln!("PASS: ToolRegistry exposes only core tools");
}

#[tokio::test]
async fn test_read_tool() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let file = tmp.path().join("hello.txt");
    std::fs::write(&file, "line1\nline2\nline3\n")?;

    let registry = ToolRegistry::new(&["read"], tmp.path());
    let tool = registry.get("read").unwrap();
    let output = tool
        .execute("test-id", serde_json::json!({"path": "hello.txt"}), None)
        .await?;
    assert!(!output.is_error);
    let text = &output.content[0];
    eprintln!("Read output: {:?}", text);
    eprintln!("PASS: ReadTool works");
    Ok(())
}

#[tokio::test]
async fn test_write_tool() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let registry = ToolRegistry::new(&["write"], tmp.path());
    let tool = registry.get("write").unwrap();
    let output = tool
        .execute(
            "test-id",
            serde_json::json!({"path": "newfile.txt", "content": "Hello, world!"}),
            None,
        )
        .await?;
    assert!(!output.is_error);
    let written = std::fs::read_to_string(tmp.path().join("newfile.txt"))?;
    assert_eq!(written, "Hello, world!");
    eprintln!("PASS: WriteTool works");
    Ok(())
}

#[tokio::test]
async fn test_edit_tool() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let file = tmp.path().join("editme.txt");
    std::fs::write(&file, "Hello, world!\n")?;

    let registry = ToolRegistry::new(&["edit"], tmp.path());
    let tool = registry.get("edit").unwrap();
    let output = tool
        .execute(
            "test-id",
            serde_json::json!({
                "path": "editme.txt",
                "oldText": "Hello",
                "newText": "Goodbye"
            }),
            None,
        )
        .await?;
    assert!(!output.is_error);
    let content = std::fs::read_to_string(&file)?;
    assert_eq!(content, "Goodbye, world!\n");
    eprintln!("PASS: EditTool works");
    Ok(())
}

#[tokio::test]
async fn test_shell_tool() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let registry = ToolRegistry::new(&["shell"], tmp.path());
    let tool = registry.get("shell").unwrap();
    let output = tool
        .execute(
            "test-id",
            serde_json::json!({"command": "echo hello world"}),
            None,
        )
        .await?;
    assert!(!output.is_error);
    if let zcode_lib::model::ContentBlock::Text(tc) = &output.content[0] {
        assert!(tc.text.contains("hello world"));
        eprintln!("Shell output: {}", tc.text);
    }
    eprintln!("PASS: ShellTool works");
    Ok(())
}

#[cfg(not(windows))]
#[tokio::test]
async fn test_shell_search_with_portable_fallbacks() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    std::fs::write(
        tmp.path().join("test.txt"),
        "hello world\nfoo bar\nhello again\n",
    )?;

    let registry = ToolRegistry::new(&["shell"], tmp.path());
    let tool = registry.get("shell").unwrap();
    let output = tool
        .execute(
            "test-id",
            serde_json::json!({
                "command": "find . -type f -name '*.txt' -print; grep -R -n -- 'hello' .",
                "successExitCodes": [0, 1]
            }),
            None,
        )
        .await?;

    assert!(!output.is_error);
    if let zcode_lib::model::ContentBlock::Text(tc) = &output.content[0] {
        eprintln!("Shell search output: {}", tc.text);
        assert!(tc.text.contains("test.txt"));
        assert!(tc.text.contains("hello"));
    }

    let no_matches = tool
        .execute(
            "test-id-no-matches",
            serde_json::json!({
                "command": "grep -R -n -- 'absent-pattern' .",
                "successExitCodes": [0, 1]
            }),
            None,
        )
        .await?;
    assert!(!no_matches.is_error, "exit code 1 must mean no matches");
    assert_eq!(
        no_matches
            .details
            .as_ref()
            .and_then(|v| v["exitCode"].as_i64()),
        Some(1)
    );

    let default_nonzero = tool
        .execute(
            "test-id-default-error",
            serde_json::json!({"command": "exit 1"}),
            None,
        )
        .await?;
    assert!(
        default_nonzero.is_error,
        "non-search commands still reject exit 1"
    );

    eprintln!("PASS: shell searches and preserves no-match semantics");
    Ok(())
}

#[cfg(not(windows))]
#[tokio::test]
async fn test_shell_output_is_bounded_by_lines_and_bytes() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let registry = ToolRegistry::new(&["shell"], tmp.path());
    let tool = registry.get("shell").unwrap();

    let many_lines = tool
        .execute(
            "test-id-line-limit",
            serde_json::json!({
                "command": "awk 'BEGIN { for (i = 1; i <= 400; i++) print \"line-\" i }'"
            }),
            None,
        )
        .await?;
    if let zcode_lib::model::ContentBlock::Text(tc) = &many_lines.content[0] {
        assert!(tc.text.lines().count() <= 201);
        assert!(tc.text.contains("lines omitted"));
    }

    let many_bytes = tool
        .execute(
            "test-id-byte-limit",
            serde_json::json!({
                "command": "awk 'BEGIN { for (i = 1; i <= 400; i++) { for (j = 1; j <= 200; j++) printf \"x\"; print \"\" } }'"
            }),
            None,
        )
        .await?;
    if let zcode_lib::model::ContentBlock::Text(tc) = &many_bytes.content[0] {
        assert!(tc.text.len() <= 30_000);
        assert!(tc.text.contains("bytes omitted"));
    }

    eprintln!("PASS: shell output stays within line and byte limits");
    Ok(())
}

#[cfg(not(windows))]
#[tokio::test]
async fn test_shell_git_fallback_includes_untracked_files() -> Result<()> {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("SKIP: git not available");
        return Ok(());
    }

    let tmp = tempfile::tempdir()?;
    std::fs::write(tmp.path().join("tracked.txt"), "tracked content\n")?;
    std::fs::write(tmp.path().join("untracked.txt"), "untracked needle\n")?;

    let registry = ToolRegistry::new(&["shell"], tmp.path());
    let tool = registry.get("shell").unwrap();
    let output = tool
        .execute(
            "test-id-git-untracked",
            serde_json::json!({
                "command": "git init -q && git add tracked.txt && while IFS= read -r -d '' file; do grep -nH -- 'needle' \"$file\"; code=$?; case \"$code\" in 0|1) ;; *) exit \"$code\" ;; esac; done < <(git ls-files -co --exclude-standard -z)",
                "successExitCodes": [0, 1]
            }),
            None,
        )
        .await?;

    assert!(!output.is_error);
    if let zcode_lib::model::ContentBlock::Text(tc) = &output.content[0] {
        assert!(tc.text.contains("untracked.txt"));
        assert!(tc.text.contains("untracked needle"));
    }
    eprintln!("PASS: git fallback searches untracked non-ignored files");
    Ok(())
}
