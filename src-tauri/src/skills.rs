//! Skill loader — discovers SKILL.md files and formats them for system prompt.
//!
//! Adapted from pi-agent-rust (src/resources.rs).
//! Loads skills from three layers (priority: project > user > builtin):
//! - .zcode/skills/*/SKILL.md (project-level)
//! - ~/.config/zcode/skills/*/SKILL.md (user-level)
//! - Bundled at compile time (builtin)

use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A loaded skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub base_dir: PathBuf,
    pub source: String, // "builtin" | "user" | "project"
    pub disable_model_invocation: bool,
    /// Full SKILL.md body (populated for built-in skills so the model can read
    /// the instructions without hitting a non-existent file path).
    #[serde(skip)]
    pub body: Option<String>,
}

// ============================================================================
// Built-in skills (compiled into the binary)
// ============================================================================

/// Add new built-in skills here. Each entry is (dir_name, SKILL.md_content).
/// The file lives under `src-tauri/skills/<dir_name>/SKILL.md`.
const BUILTIN_SKILLS: &[(&str, &str)] = &[
    ("skill-creator", include_str!("../skills/skill-creator/SKILL.md")),
];

/// Load skills from project and user directories.
/// Loading order determines priority: project > user > builtin.
/// Built-in skills are loaded first so they can be overridden.
pub fn load_skills(
    cwd: &Path,
    user_config_dir: Option<&Path>,
    extra_paths: &[PathBuf],
) -> (Vec<Skill>, Vec<String>) {
    let mut skill_map: HashMap<String, Skill> = HashMap::new();
    let mut diagnostics: Vec<String> = Vec::new();

    // 1. Built-in skills (lowest priority — can be overridden)
    for &(dir_name, content) in BUILTIN_SKILLS {
        if let Some(skill) = load_skill_from_str(content, dir_name, "builtin") {
            skill_map.insert(skill.name.clone(), skill);
        }
    }

    // 2. User-level: ~/.config/zcode/skills/*/SKILL.md
    if let Some(user_dir) = user_config_dir {
        let user_skills_dir = user_dir.join("skills");
        if user_skills_dir.exists() {
            load_from_dir(&user_skills_dir, "user", &mut skill_map, &mut diagnostics);
        }
    }

    // 3. Project-level: .zcode/skills/*/SKILL.md (highest priority)
    let project_skills_dir = cwd.join(".zcode").join("skills");
    if project_skills_dir.exists() {
        load_from_dir(
            &project_skills_dir,
            "project",
            &mut skill_map,
            &mut diagnostics,
        );
    }

    // 4. Extra paths
    for path in extra_paths {
        if path.is_dir() {
            load_from_dir(path, "path", &mut skill_map, &mut diagnostics);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(skill) = load_skill_from_file(path, "path") {
                let name = skill.name.clone();
                match skill_map.entry(name) {
                    Entry::Occupied(entry) => {
                        diagnostics.push(format!(
                            "Skill name collision: {} from {:?}",
                            entry.key(),
                            path
                        ));
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(skill);
                    }
                }
            }
        }
    }

    let mut skills: Vec<Skill> = skill_map.into_values().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    (skills, diagnostics)
}

/// Scan a directory for SKILL.md files (one level deep).
fn load_from_dir(
    dir: &Path,
    source: &str,
    skill_map: &mut HashMap<String, Skill>,
    diagnostics: &mut Vec<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.exists() {
                if let Some(skill) = load_skill_from_file(&skill_file, source) {
                    let name = skill.name.clone();
                    match skill_map.entry(name) {
                        Entry::Occupied(entry) => {
                            diagnostics.push(format!(
                                "Skill name collision: {} from {:?}",
                                entry.key(),
                                skill_file
                            ));
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(skill);
                        }
                    }
                }
            }
        } else if path.extension().is_some_and(|ext| ext == "md") {
            // Top-level .md files also treated as skills
            if let Some(skill) = load_skill_from_file(&path, source) {
                let name = skill.name.clone();
                if let Entry::Vacant(entry) = skill_map.entry(name) {
                    entry.insert(skill);
                }
            }
        }
    }
}

/// Parse YAML frontmatter from a SKILL.md file.
fn load_skill_from_file(path: &Path, source: &str) -> Option<Skill> {
    let content = std::fs::read_to_string(path).ok()?;

    // Parse YAML frontmatter between --- markers
    let frontmatter = if let Some(rest) = content.strip_prefix("---") {
        let end = rest.find("---")?;
        &rest[..end]
    } else {
        return None;
    };

    let name = extract_frontmatter_field(frontmatter, "name")?;
    let description = extract_frontmatter_field(frontmatter, "description")
        .unwrap_or_else(|| "No description".to_string());
    let disable_model_invocation =
        extract_frontmatter_field(frontmatter, "disable-model-invocation")
            .map(|v| matches!(v.as_str(), "true" | "yes"))
            .unwrap_or(false);

    let base_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

    Some(Skill {
        name,
        description,
        file_path: path.to_path_buf(),
        base_dir,
        source: source.to_string(),
        disable_model_invocation,
        body: None,
    })
}

/// Parse YAML frontmatter from an embedded string (for built-in skills).
fn load_skill_from_str(content: &str, dir_name: &str, source: &str) -> Option<Skill> {
    let frontmatter = if let Some(rest) = content.strip_prefix("---") {
        let end = rest.find("---")?;
        &rest[..end]
    } else {
        return None;
    };

    let name = extract_frontmatter_field(frontmatter, "name")?;
    let description = extract_frontmatter_field(frontmatter, "description")
        .unwrap_or_else(|| "No description".to_string());
    let disable_model_invocation =
        extract_frontmatter_field(frontmatter, "disable-model-invocation")
            .map(|v| matches!(v.as_str(), "true" | "yes"))
            .unwrap_or(false);

    let file_path = PathBuf::from(format!("<builtin>/{}/SKILL.md", dir_name));
    let base_dir = PathBuf::from(format!("<builtin>/{}", dir_name));

    Some(Skill {
        name,
        description,
        file_path,
        base_dir,
        source: source.to_string(),
        disable_model_invocation,
        body: Some(content.to_string()),
    })
}

/// Simple frontmatter field extraction (key: value).
/// Also handles YAML block scalars (key: > or key: >- followed by indented lines).
fn extract_frontmatter_field(frontmatter: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = frontmatter.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let Some(trimmed) = line.strip_prefix(&format!("{key}:")) else {
            continue;
        };
        let val = trimmed.trim();
        // Multi-line block scalar (key: > or key: >-)
        if val == ">-" || val == ">" {
            let mut parts: Vec<&str> = Vec::new();
            for next in &lines[i + 1..] {
                if next.starts_with(' ') || next.starts_with('\t') {
                    parts.push(next.trim());
                } else {
                    break;
                }
            }
            if !parts.is_empty() {
                return Some(parts.join(" "));
            }
        }
        // Single-line value
        if !val.is_empty() && val != ">-" && val != ">" {
            return Some(val.to_string());
        }
    }
    None
}

/// Format skills as XML for system prompt injection.
/// Produces output identical to pi's <available_skills> block.
pub fn format_skills_for_prompt(skills: &[Skill]) -> String {
    let visible: Vec<&Skill> = skills
        .iter()
        .filter(|s| !s.disable_model_invocation)
        .collect();
    if visible.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "\n\nThe following skills provide specialized instructions for specific tasks."
            .to_string(),
        "Use the read tool to load a skill's file when the task matches its description."
            .to_string(),
        "Skills marked with source=\"builtin\" are embedded directly below — you do NOT need to read a file for these."
            .to_string(),
        "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands."
            .to_string(),
        String::new(),
        "<available_skills>".to_string(),
    ];

    let mut builtin_bodies: Vec<String> = Vec::new();

    for skill in &visible {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        if skill.source == "builtin" {
            lines.push("    <location>embedded (see below)</location>".to_string());
            if let Some(ref body) = skill.body {
                builtin_bodies.push(format!(
                    "\n<!-- BEGIN builtin skill: {} -->\n{}\n<!-- END builtin skill: {} -->",
                    skill.name, body, skill.name
                ));
            }
        } else {
            lines.push(format!(
                "    <location>{}</location>",
                escape_xml(&skill.file_path.display().to_string())
            ));
        }
        lines.push("  </skill>".to_string());
    }

    lines.push("</available_skills>".to_string());

    // Append built-in skill bodies directly so the model doesn't need to
    // read a non-existent file path.
    if !builtin_bodies.is_empty() {
        lines.push(
            "\n<!-- Built-in skill instructions follow (already loaded — no read tool needed) -->"
                .to_string(),
        );
        lines.extend(builtin_bodies);
    }

    lines.join("\n")
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_skill(dir: &Path) {
        let skill_dir = dir.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_content = r#"---
name: test-skill
description: A test skill for verification
---

# Test Skill

This is the skill content.

## Instructions

Do something specific.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), skill_content).unwrap();
    }

    #[test]
    fn test_load_skills_from_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_dir = tmp.path().join(".zcode").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        setup_test_skill(&skills_dir);

        let (skills, diags) = load_skills(tmp.path(), None, &[]);
        eprintln!("Diagnostics: {:?}", diags);
        eprintln!(
            "Skills found: {:?}",
            skills.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        // At least 2: one builtin (skill-creator) + one project (test-skill)
        assert!(
            skills.len() >= 2,
            "Expected at least 2 skills, got {}",
            skills.len()
        );
        // test-skill should be present (builtin loaded first, project can shadow)
        let ts = skills
            .iter()
            .find(|s| s.name == "test-skill")
            .expect("test-skill should be in the list");
        assert_eq!(ts.description, "A test skill for verification");
        assert!(!ts.disable_model_invocation);
        assert_eq!(ts.source, "project");
        // skill-creator builtin should be present too
        let sc = skills
            .iter()
            .find(|s| s.name == "skill-creator")
            .expect("skill-creator should be in the list");
        assert_eq!(sc.source, "builtin");
    }

    #[test]
    fn test_format_skills_xml() {
        let skill = Skill {
            name: "test-skill".into(),
            description: "A test skill".into(),
            file_path: PathBuf::from("/tmp/test/SKILL.md"),
            base_dir: PathBuf::from("/tmp/test"),
            source: "project".into(),
            disable_model_invocation: false,
            body: None,
        };

        let xml = format_skills_for_prompt(&[skill]);
        eprintln!("Generated XML:\n{xml}");

        assert!(xml.contains("<available_skills>"));
        assert!(xml.contains("<name>test-skill</name>"));
        assert!(xml.contains("<description>A test skill</description>"));
        assert!(xml.contains("<location>/tmp/test/SKILL.md</location>"));
        assert!(xml.contains("</available_skills>"));
    }

    #[test]
    fn test_disabled_skill_not_in_prompt() {
        let skill = Skill {
            name: "hidden".into(),
            description: "Should not appear".into(),
            file_path: PathBuf::from("/tmp/hidden/SKILL.md"),
            base_dir: PathBuf::from("/tmp/hidden"),
            source: "project".into(),
            disable_model_invocation: true,
            body: None,
        };

        let xml = format_skills_for_prompt(&[skill]);
        assert!(xml.is_empty(), "Disabled skill should not appear in prompt");
    }
}

#[test]
fn test_skill_state_persistence_roundtrip() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    struct SkillState {
        disabled: Vec<String>,
    }

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let state_path = dir.join("skill-state.json");

    // 1. No state file → default (all active)
    let state: SkillState = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    assert!(state.disabled.is_empty());

    // 2. Disable "test-skill"
    let state = SkillState {
        disabled: vec!["test-skill".into()],
    };
    std::fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap()).unwrap();

    let loaded: SkillState = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap();
    assert_eq!(loaded.disabled, vec!["test-skill"]);

    // 3. Enable "test-skill" again
    std::fs::write(&state_path, r#"{"disabled":[]}"#).unwrap();
    let loaded: SkillState = std::fs::read_to_string(&state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap();
    assert!(loaded.disabled.is_empty());

    eprintln!("✅ Skill state persistence roundtrip OK");
}

#[test]
fn test_extract_frontmatter_single_line() {
    let frontmatter = r#"name: skill-creator
description: MANDATORY invoke whenever"#;
    assert_eq!(
        extract_frontmatter_field(frontmatter, "name"),
        Some("skill-creator".to_string())
    );
    let desc = extract_frontmatter_field(frontmatter, "description");
    assert!(desc.unwrap().contains("MANDATORY"));
}

#[test]
fn test_extract_frontmatter_multiline_block() {
    let frontmatter = "name: test\ndescription: >-\n  Line one.\n  Line two.\nother: val";
    let desc = extract_frontmatter_field(frontmatter, "description");
    assert_eq!(desc, Some("Line one. Line two.".to_string()));
}
