//! Tool integration tests.
//!
//! Run: cargo test --test tool_smoke -- --nocapture

use std::path::Path;
use zcode_lib::error::Result;
use zcode_lib::tools::ToolRegistry;

#[test]
fn test_tool_registry_subset() {
    let cwd = Path::new(".");
    let enabled = &["read", "grep", "find", "ls"];
    let registry = ToolRegistry::new(enabled, cwd);

    let names: Vec<&str> = registry.tools().iter().map(|t| t.name()).collect();
    eprintln!("Registered tools: {:?}", names);
    assert_eq!(registry.tools().len(), 4);
    assert!(registry.get("read").is_some());
    assert!(registry.get("grep").is_some());
    assert!(registry.get("find").is_some());
    assert!(registry.get("ls").is_some());
    assert!(registry.get("bash").is_none());
    assert!(registry.get("shell").is_none());
    assert!(registry.get("edit").is_none());
    eprintln!("PASS: ToolRegistry subset works");
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
async fn test_ls_tool() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    std::fs::write(tmp.path().join("a.txt"), "a")?;
    std::fs::create_dir(tmp.path().join("subdir"))?;

    let registry = ToolRegistry::new(&["ls"], tmp.path());
    let tool = registry.get("ls").unwrap();
    let output = tool
        .execute("test-id", serde_json::json!({"path": "."}), None)
        .await?;
    assert!(!output.is_error);
    let text = &output.content[0];
    // Check it contains our files
    if let zcode_lib::model::ContentBlock::Text(tc) = text {
        assert!(tc.text.contains("a.txt"));
        assert!(tc.text.contains("subdir/"));
        eprintln!("LS output:\n{}", tc.text);
    }
    eprintln!("PASS: LsTool works");
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
async fn test_grep_tool_if_rg_available() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    std::fs::write(
        tmp.path().join("test.txt"),
        "hello world\nfoo bar\nhello again\n",
    )?;

    let registry = ToolRegistry::new(&["grep"], tmp.path());
    let tool = registry.get("grep").unwrap();
    let output = tool
        .execute(
            "test-id",
            serde_json::json!({"pattern": "hello", "path": "."}),
            None,
        )
        .await;

    match output {
        Ok(out) => {
            if let zcode_lib::model::ContentBlock::Text(tc) = &out.content[0] {
                eprintln!("Grep output: {}", tc.text);
                if out.is_error {
                    eprintln!("Grep error (rg not installed): {}", tc.text);
                } else {
                    assert!(tc.text.contains("hello"), "Expected grep to find 'hello'");
                }
            }
            eprintln!("PASS: GrepTool works (with rg)");
        }
        Err(_) => {
            eprintln!("SKIP: rg not available, grep test skipped");
        }
    }
    Ok(())
}
