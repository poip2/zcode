# zcode Skills System — Developer Guide

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Discovery (Rust)                     │
│  skills::load_skills(cwd, user_config_dir, extra_paths) │
│  → Scans .zcode/skills/<name>/SKILL.md (project)       │
│  → Scans ~/.config/zcode/skills/<name>/SKILL.md (user) │
│  → Returns Vec<Skill> with name/description/file_path  │
├─────────────────────────────────────────────────────────┤
│                    Persistence                          │
│  ~/.config/zcode/skill-state.json                      │
│  → {"disabled": ["skill-name", ...]}                   │
│  → Opt-out: all skills enabled by default              │
├─────────────────────────────────────────────────────────┤
│                    Injection                           │
│  build_system_prompt() → filter by state               │
│  skills::format_skills_for_prompt() → XML              │
│  → Injects <available_skills> block into system prompt │
├─────────────────────────────────────────────────────────┤
│                    Filesystem Watching                  │
│  SkillWatcherState → notify watcher on both dirs       │
│  → emits "skills-changed" event to frontend            │
│  → frontend reloads via list_skills command            │
├─────────────────────────────────────────────────────────┤
│                    Frontend UI                          │
│  skills.svelte.ts store → list_skills / set_skill_active│
│  SettingsDialog → dynamic toggle list                  │
│  AgentPanel → enabled skills tag bar                   │
│  +layout.svelte → global watcher + reload              │
└─────────────────────────────────────────────────────────┘
```

## Key files

| File | Role |
|------|------|
| `src-tauri/src/skills.rs` | SKILL.md discovery + YAML parsing + XML formatting |
| `src-tauri/src/agent_command.rs:build_system_prompt()` | Filter by state + inject into system prompt |
| `src-tauri/src/agent_command.rs:list_skills` | Tauri command: return all skills with enabled/disabled |
| `src-tauri/src/agent_command.rs:set_skill_active` | Tauri command: toggle skill on/off, persist to state file |
| `src-tauri/src/watcher.rs:SkillWatcherState` | `notify`-based filesystem watcher for skill dirs |
| `src/lib/stores/skills.svelte.ts` | Frontend store: lists, toggles, reloads |
| `src/lib/components/SettingsDialog.svelte` | Skills tab UI with dynamic toggle list |
| `src/routes/+layout.svelte` | Global watcher mount + cwd derivation |
| `~/.config/zcode/skill-state.json` | Persisted disabled skill names (opt-out) |

## SKILL.md format

```markdown
---
name: my-skill           # required: unique identifier
description: ...          # required: what + when to trigger
disable-model-invocation: false  # optional: hide from model
---

# Body (optional)
Instructions that get loaded `read` by the model when the skill triggers.
```

- `name` and `description` are REQUIRED. Missing either → skill silently skipped.
- `description` is the **primary trigger** — the model sees `<name>` + `<description>` and decides whether to read the full body.
- `disable-model-invocation: true` means the skill still appears in UI but is never injected into the prompt.

## Discovery locations

| Path | `source` field |
|------|---------------|
| `<cwd>/.zcode/skills/<name>/SKILL.md` | `"project"` |
| `~/.config/zcode/skills/<name>/SKILL.md` | `"user"` |

Project-level skills take precedence on name collision.

On macOS, zcode also scans the platform config directory (`~/Library/Application Support/zcode/skills`) so both native and cross-platform install locations work.

Project- and user-level skill directories are allowed file roots. User-installed skills receive an automatic per-turn approval pass after `SKILL.md` loads. Project skills require approval for their first dangerous action each turn, then later actions skip repeated approval. This prevents a cloned repository from silently executing commands.

## State persistence

`~/.config/zcode/skill-state.json`:
```json
{
  "disabled": ["skill-a", "skill-b"]
}
```

- **Opt-out model**: all discovered skills are enabled by default.
- User toggles off in Settings → name goes into `disabled` list.
- Toggle back on → name removed from list.
- Persisted by `set_skill_active` command.

## cwd derivation

The "current working directory" for skill discovery is determined by:

1. **Open file's parent directory** (e.g., editing `/a/b/README.md` → `/a/b`)
2. **Pinned folder** (fallback if no file is open)
3. **`"."`** (fallback if nothing pinned)

This logic lives in `+layout.svelte:deriveCwd()` and `SettingsDialog.svelte:deriveCwd()`.

## Events

| Event | Source | Payload |
|-------|--------|---------|
| `skills-changed` | `SkillWatcherState` (Rust) | `()` |

Triggered when any `SKILL.md` file is created/modified/deleted in either watched directory.
Frontend reacts: `skillsStore.reload(cwd)`.

## Creating skills for users

When your agent creates skills for the user, follow these rules:

1. **Use `write` tool** to create `.zcode/skills/<name>/SKILL.md` (project) or `~/.config/zcode/skills/<name>/SKILL.md` (global).
2. **Always include YAML frontmatter** with at minimum `name` and `description`.
3. **Use kebab-case** for skill names.
4. **Write clear descriptions** — they determine triggering, so mention the domain, actions, and trigger phrases.
5. **Keep skill bodies focused** — under 500 lines ideally. One skill = one purpose.

See the `skill-creator` skill (`~/.config/zcode/skills/skill-creator/SKILL.md`) for the agent-side guide.

## Built-in skills

zcode ships with one built-in skill compiled into the binary:

| Skill | Description |
|-------|-------------|
| `skill-creator` | Guide for creating new skills + installing skills from GitHub repos |

To add a new built-in skill:
1. Create `src-tauri/skills/<name>/SKILL.md`
2. Add to `BUILTIN_SKILLS` array in `src-tauri/src/skills.rs`

## Installing skills from GitHub

Use `scripts/install-skill.sh` (Mac/Linux) or `scripts/install-skill.ps1` (Windows). Omitting scope installs to user-level `~/.config/zcode/skills` by default:

```bash
# Install a skill for the current user (default)
./scripts/install-skill.sh https://github.com/anthropics/skills.git xlsx

# Install to current project
./scripts/install-skill.sh https://github.com/anthropics/skills.git xlsx --project

# Install to pi agent skills
./scripts/install-skill.sh https://github.com/anthropics/skills.git xlsx --agents
```

The script uses `git clone --depth 1` + `sparse-checkout` (~3-10s) instead of
slow HTTP per-file downloads (~20s+ with API rate limits).
