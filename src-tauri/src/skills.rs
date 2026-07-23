//! Skill loader — discovers SKILL.md files and formats them for system prompt.
//!
//! Adapted from pi-agent-rust (src/resources.rs).
//! Loads skills from three layers (priority: project > user > builtin):
//! - .zcode/skills/*/SKILL.md (project-level)
//! - ~/.config/zcode/skills/*/SKILL.md (user-level)
//! - Bundled at compile time (builtin)

use serde::{Deserialize, Serialize};
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

/// User-level skill roots. Includes both platform config and documented
/// `~/.config/zcode` locations because they differ on macOS.
pub fn user_skill_roots(user_config_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(config_dir) = user_config_dir {
        roots.push(config_dir.join("skills"));
    }
    if user_config_dir.is_some() {
        if let Some(home) = dirs::home_dir() {
            let documented = home.join(".config").join("zcode").join("skills");
            if !roots.contains(&documented) {
                // Loaded last so the documented/default install location wins.
                roots.push(documented);
            }
        }
    }
    roots
}

/// Skill roots allowed for direct file access.
pub fn trusted_skill_roots(cwd: &Path) -> Vec<PathBuf> {
    let mut roots = vec![cwd.join(".zcode").join("skills")];
    let platform_config = dirs::config_dir().map(|dir| dir.join("zcode"));
    roots.extend(user_skill_roots(platform_config.as_deref()));
    roots
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillPathSource {
    Project,
    User,
}

/// Identify whether `path` belongs to a project- or user-level skill root.
pub fn skill_path_source(path: &Path, cwd: &Path) -> Option<SkillPathSource> {
    let candidate = comparable_path(path);
    let project_root = comparable_path(&cwd.join(".zcode").join("skills"));
    if candidate.starts_with(project_root) {
        return Some(SkillPathSource::Project);
    }

    let platform_config = dirs::config_dir().map(|dir| dir.join("zcode"));
    user_skill_roots(platform_config.as_deref())
        .into_iter()
        .map(|root| comparable_path(&root))
        .any(|root| candidate.starts_with(root))
        .then_some(SkillPathSource::User)
}

/// Return whether `path` is contained by a project- or user-level skill root.
pub fn is_trusted_skill_path(path: &Path, cwd: &Path) -> bool {
    skill_path_source(path, cwd).is_some()
}

fn comparable_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    // Normalize `.` and `..` before prefix matching. This also keeps a new,
    // not-yet-created file from escaping a trusted root via path traversal.
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    if let Ok(canonical) = dunce::canonicalize(&normalized) {
        return canonical;
    }

    // New files cannot be canonicalized. Resolve the closest existing parent
    // so symlinked skill roots still compare correctly.
    let mut ancestor = normalized.as_path();
    let mut missing = Vec::new();
    while let Some(name) = ancestor.file_name() {
        missing.push(name.to_os_string());
        let Some(parent) = ancestor.parent() else {
            break;
        };
        ancestor = parent;
        if let Ok(mut canonical) = dunce::canonicalize(ancestor) {
            for part in missing.iter().rev() {
                canonical.push(part);
            }
            return canonical;
        }
    }

    normalized
}

// ============================================================================
// Built-in skills (compiled into the binary)
// ============================================================================

/// Add new built-in skills here. Each entry is (dir_name, SKILL.md_content).
/// The file lives under `src-tauri/skills/<dir_name>/SKILL.md`.
const BUILTIN_SKILLS: &[(&str, &str)] = &[(
    "skill-creator",
    include_str!("../skills/skill-creator/SKILL.md"),
)];

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

    // 2. User-level. On macOS, scan both the platform config directory and
    // ~/.config/zcode so skills installed by the cross-platform scripts work.
    for user_skills_dir in user_skill_roots(user_config_dir) {
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
                insert_skill(&mut skill_map, &mut diagnostics, skill, path);
            }
        }
    }

    let mut skills: Vec<Skill> = skill_map.into_values().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    (skills, diagnostics)
}

fn insert_skill(
    skill_map: &mut HashMap<String, Skill>,
    diagnostics: &mut Vec<String>,
    skill: Skill,
    path: &Path,
) {
    let name = skill.name.clone();
    if let Some(previous) = skill_map.insert(name.clone(), skill) {
        diagnostics.push(format!(
            "Skill '{name}' from {} overrides {} source {}",
            path.display(),
            previous.source,
            previous.file_path.display()
        ));
    }
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
                    insert_skill(skill_map, diagnostics, skill, &skill_file);
                }
            }
        } else if path.extension().is_some_and(|ext| ext == "md") {
            // Top-level .md files also treated as skills
            if let Some(skill) = load_skill_from_file(&path, source) {
                insert_skill(skill_map, diagnostics, skill, &path);
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

    #[test]
    fn test_project_skill_paths_are_trusted() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_file = tmp
            .path()
            .join(".zcode/skills/my-skill/references/guide.md");

        assert!(is_trusted_skill_path(&skill_file, tmp.path()));
        assert_eq!(
            skill_path_source(&skill_file, tmp.path()),
            Some(SkillPathSource::Project)
        );
        assert!(!is_trusted_skill_path(
            &tmp.path().join("ordinary-project-file.md"),
            tmp.path()
        ));
        assert!(!is_trusted_skill_path(
            &tmp.path().join(".zcode/skills/../outside.md"),
            tmp.path()
        ));
    }

    #[test]
    fn test_user_skill_paths_are_trusted() {
        let tmp = tempfile::tempdir().unwrap();
        let user_root = trusted_skill_roots(tmp.path())
            .into_iter()
            .find(|root| !root.starts_with(tmp.path()))
            .expect("a user-level skill root should exist");

        assert!(is_trusted_skill_path(
            &user_root.join("my-skill/SKILL.md"),
            tmp.path()
        ));
        assert_eq!(
            skill_path_source(&user_root.join("my-skill/SKILL.md"), tmp.path()),
            Some(SkillPathSource::User)
        );
    }

    #[test]
    fn test_project_skill_overrides_user_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let user_config = tmp.path().join("user-config");
        let name = "zcode-priority-test-skill";
        let user_skill = user_config.join("skills").join(name);
        let project_skill = tmp.path().join(".zcode").join("skills").join(name);
        std::fs::create_dir_all(&user_skill).unwrap();
        std::fs::create_dir_all(&project_skill).unwrap();
        std::fs::write(
            user_skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: user version\n---\n"),
        )
        .unwrap();
        std::fs::write(
            project_skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: project version\n---\n"),
        )
        .unwrap();

        let (skills, _) = load_skills(tmp.path(), Some(&user_config), &[]);
        let loaded = skills.iter().find(|skill| skill.name == name).unwrap();
        assert_eq!(loaded.source, "project");
        assert_eq!(loaded.description, "project version");
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
