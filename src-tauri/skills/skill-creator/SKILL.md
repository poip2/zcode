---
name: skill-creator
description: >
  MANDATORY — invoke when user wants to create/make/build/write/design/install/
  download/add/pull a skill or SKILL.md from GitHub or anywhere. Also trigger on:
  帮我写skill 创建skill 加个skill 安装skill 下载skill install from github.
  Trigger when user describes something they want AI to remember and follow
  in future, even without saying "skill".
  NEVER create a skill file without reading this skill first.
  NEVER use HTTP (curl/iwr) to fetch from GitHub — always git clone --depth 1
  with sparse-checkout. This rule applies to ALL GitHub repo access, not just skills.
---

# Skill Creator & Installer for zcode

This skill covers TWO workflows:

| You want to... | Go to |
|----------------|-------|
| Create a new skill from scratch | [Creating skills](#creating-skills) |
| Install an existing skill from GitHub | [Installing skills from GitHub](#installing-skills-from-github) |

---

## Where skills live

Skills are discovered from TWO locations (both checked automatically):

| Location | Scope | Used for |
|----------|-------|----------|
| `.zcode/skills/<name>/SKILL.md` | Project only | Skills specific to this project |
| `~/.config/zcode/skills/<name>/SKILL.md` | All projects | Globally useful skills (like this one) |

---

## Skill file format

Every skill MUST have YAML frontmatter:

```markdown
---
name: my-skill
description: What this skill does and when to trigger it
disable-model-invocation: false  # optional, defaults to false
---

# Skill Title

Instructions go here...
```

Required fields:
- **`name`**: Unique identifier (kebab-case recommended, e.g. `fix-grammar`)
- **`description`**: When to trigger + what it does. This is the PRIMARY trigger — include specific contexts and keywords. Be a little "pushy" to avoid under-triggering.

Optional fields:
- **`disable-model-invocation`**: Set to `true` to hide the skill from the AI (user can still enable/disable from Settings UI)

---

# Creating skills

## Workflow

### 1. Capture intent
Ask the user:
- What should this skill do?
- When should it trigger? (what user phrases/contexts)
- What's the expected output?

### 2. Write the skill
- Create the directory: `.zcode/skills/<name>/` (project) or `~/.config/zcode/skills/<name>/` (global)
- Write `SKILL.md` with frontmatter + instructions
- Keep instructions clear and specific. Use imperative mood. Include examples.
- Explain *why* rather than just commanding — models respond better to understanding than rigid ALWAYS/NEVER rules.

### 3. Test
- Write the skill file using the `write` tool
- Then send a test message to verify the model picks up the skill.
- Ask the user if the behavior matches expectations.

### 4. Iterate
- Refine based on user feedback

---

# Installing skills from GitHub

## Key rule

**NEVER use Invoke-WebRequest / Invoke-RestMethod / curl to fetch individual files
from GitHub.** GitHub API rate limits unauthenticated requests, and page downloads
pull 130KB+ of HTML for every file.

**ALWAYS use `git clone --depth 1 --no-checkout` + `git sparse-checkout` instead.**
This pulls only the needed directory in a single network round-trip (~3–10s).

## Install locations

| Location | Scope | Flag |
|----------|-------|------|
| `.zcode/skills/<name>/` | Project only | `--project` (default) |
| `~/.config/zcode/skills/<name>/` | All projects (global) | `--global` |
| `~/.agents/skills/<name>/` | pi agent skills | `--agents` |

## Workflow (agent-driven)

### Step 1: Determine parameters from URL (minimize questions)

Users paste GitHub URLs at varying depths. Parse accordingly:

**URL patterns and extraction:**

| URL pattern | Example | Extraction |
|---|---|---|
| Repo root | `github.com/user/repo` | ❓ Ask which skill |
| Directory (tree) | `github.com/user/repo/tree/main/skills/ppt-master` | Repo + skill from path |
| File (blob) | `github.com/user/repo/blob/main/skills/ppt-master/SKILL.md` | Repo + skill = parent dir of SKILL.md |

**Parsing rules:**

1. **Repo URL**: `https://github.com/{owner}/{repo}.git` — strip everything after the repo name, add `.git` if not already present.
2. **In-repo path**: the portion after `tree/{branch}/` or `blob/{branch}/`.
3. **Skill name**: if the in-repo path ends with `SKILL.md`, the skill is its parent directory.
   Otherwise, the skill is the last path segment.
4. **Sparse-checkout path**: the in-repo path up to and including the skill directory
   (i.e., strip `/SKILL.md` if present).

**Examples:**

```
URL: https://github.com/hugohe3/ppt-master/blob/main/skills/ppt-master/SKILL.md
→ Repo:   https://github.com/hugohe3/ppt-master.git
→ Path:   skills/ppt-master/SKILL.md
→ Skill:  ppt-master  (parent of SKILL.md)
→ Sparse: skills/ppt-master

URL: https://github.com/anthropics/skills/tree/main/skills/xlsx
→ Repo:   https://github.com/anthropics/skills.git
→ Path:   skills/xlsx
→ Skill:  xlsx  (last segment)
→ Sparse: skills/xlsx

URL: https://github.com/user/myskills/tree/main/rust
→ Repo:   https://github.com/user/myskills.git
→ Path:   rust  (no skills/ prefix — skill at repo root)
→ Skill:  rust
→ Sparse: rust
```

Only ask the user if the URL is just a repo root (no path beyond `github.com/user/repo`).
Default scope to `--project` (`.zcode/skills/`) unless user says "global", "all projects", or "agents".

### Step 2: Install with git sparse-checkout

**IMPORTANT**: git clone goes over the network and can take 30-120s. Always use
`timeout: 60` (or omit timeout to get the 120s default). Never set timeout < 60.

Split into two calls so the slow network step doesn't get killed:

**Call 1 — Clone (timeout: 60):**

Mac / Linux:
```bash
TMP=$(mktemp -d)
git clone --depth 1 --no-checkout REPO_URL "$TMP"
cd "$TMP" && git sparse-checkout set "SPARSE_PATH" && git checkout
echo "$TMP"
```

Windows PowerShell:
```powershell
$tmp = Join-Path $env:TEMP "skill_$(Get-Random)"
git clone --depth 1 --no-checkout REPO_URL $tmp
Push-Location $tmp
git sparse-checkout set "SPARSE_PATH"
git checkout
Pop-Location
Write-Output $tmp
```

**Call 2 — Copy to target (no timeout needed):**

Replace `TMP` with the path printed by Call 1.

Mac / Linux:
```bash
mkdir -p TARGET_DIR && cp -r "TMP/SPARSE_PATH"/. TARGET_DIR/ && rm -rf "TMP"
```

Windows PowerShell:
```powershell
New-Item -ItemType Directory -Path "TARGET_DIR" -Force | Out-Null
Copy-Item -Recurse "TMP\SPARSE_PATH\*" "TARGET_DIR"
Remove-Item -Recurse -Force "TMP"
```

TARGET_DIR mapping:
- **project** (default): `$(pwd)/.zcode/skills/SKILL_NAME`
- **global**: `~/.config/zcode/skills/SKILL_NAME`
- **pi agent**: `~/.agents/skills/SKILL_NAME`

### Step 3: Verify
Use the `read` tool to check `TARGET_DIR/SKILL.md` has valid YAML frontmatter
(`name` + `description` fields).

### Updating an already-installed skill
Same workflow — just overwrite the target directory. No extra steps needed.

## Prerequisites

- **Git** must be installed and on PATH. Verify: `git --version`

## Performance

| Method | Time | Issues |
|--------|------|--------|
| HTTP file-by-file (curl/iwr) | ~20s+ | API rate limit, 130KB HTML per request |
| `git clone --depth 1` + sparse-checkout | ~3–10s | One round-trip, no rate limits |

## Notes

- The sparse-checkout path is directly extracted from the URL — no guesswork needed
- The temporary clone directory is fully cleaned up after install (via `rm -rf` / `Remove-Item`)

---

# Tips

- **Descriptions matter most.** The description determines whether the skill gets triggered. Describe both what the skill does AND when to use it.
- **Keep skills focused.** One skill = one clear purpose. Don't cram unrelated instructions into one skill.
- **Project vs global:** Use project-level (`.zcode/skills/`) for project-specific workflows (coding conventions, domain knowledge). Use global (`~/.config/zcode/skills/`) for reusable skills that apply across projects.
- **Test with the actual agent.** After creating a skill, ask the user to send a message that should trigger it, and verify the agent follows the instructions.
- **Workspace four-folder layout:** When creating skills that generate files, respect the workspace convention:
  - Skill instructions → `.zcode/skills/<name>/` or `~/.config/zcode/skills/<name>/`
  - Scripts → `scripts/` directory (alongside pin)
  - Generated output → `output/` directory (alongside pin)
  - Source files to modify → `sources/` directory (alongside pin)
  All four directories are writable via `read`/`write`/`edit` tools.
