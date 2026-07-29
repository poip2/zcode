---
name: skill-creator
description: >
  MANDATORY — invoke whenever the user wants to create, write, design, install,
  download, add, update, or pull a skill or SKILL.md from GitHub or elsewhere.
  Trigger on phrases such as 帮我写skill, 创建skill, 加个skill, 安装skill,
  下载skill, and install from GitHub. Also trigger when the user describes behavior
  they want the AI to remember and follow later, even without saying "skill".
  Never create a skill file without reading these instructions first. When a GitHub
  skill install needs Git but git is missing, stay inside this skill and use its
  embedded Git bootstrap route; never create a separate Git helper skill.
---

# Skill Creator and Installer

This built-in skill owns two workflows and one internal fallback:

| Request | Route |
|---|---|
| Create a skill from instructions | [Workflow A — Create](#workflow-a--create-a-skill) |
| Install or update a skill from GitHub | [Workflow B — Install](#workflow-b--install-or-update-from-github) |
| Workflow B discovers that Git is missing | [Git bootstrap](#git-bootstrap-route-no-separate-skill), then return to Workflow B |

## Non-negotiable rules

1. One user intent produces one focused skill. Do not create a Git installer/helper
   skill when Git is merely a prerequisite of installation.
2. Never fetch GitHub repository content file-by-file with `curl`,
   `Invoke-WebRequest`, or `Invoke-RestMethod`. Use shallow Git sparse-checkout.
3. HTTP is allowed only inside the documented Git bootstrap, where Git does not yet
   exist and official bootstrap artifacts must be downloaded.
4. Default installs to user scope unless the user explicitly requests project or pi
   agent scope.
5. Verify every written or installed `SKILL.md`. Do not report success before
   verification passes.
6. Respect zcode tool approvals and filesystem safety rules. “Silent installation”
   means no operating-system GUI wizard; it does not bypass zcode approval.

---

# Shared conventions

## Skill locations

| Scope | Directory | Selection rule |
|---|---|---|
| Project | `.zcode/skills/<name>/SKILL.md` | User says project/current project |
| User/global | `~/.config/zcode/skills/<name>/SKILL.md` | Default |
| pi agent | `~/.agents/skills/<name>/SKILL.md` | User says agents/pi agent |

## Required file format

Every skill has YAML frontmatter followed by Markdown instructions:

```markdown
---
name: my-skill
description: What this skill does and exactly when it should trigger
# disable-model-invocation: false  # optional; defaults to false
---

# Skill title

Instructions...
```

- `name`: unique identifier; kebab-case preferred.
- `description`: primary trigger contract. Include actions, contexts, and useful trigger
  phrases. Be explicit enough to avoid under-triggering.
- `disable-model-invocation`: optional. `true` hides the skill from model invocation;
  Settings can still enable or disable it.

## Quality bar

- Use imperative, concrete instructions.
- Explain why a constraint exists when that improves compliance.
- Include only examples that clarify behavior.
- Keep one skill focused on one responsibility.
- Put supporting material beneath the same skill directory rather than creating
  unrelated top-level files.
- Respect zcode workspace folders when generated work needs scripts, sources, or output.

---

# Workflow A — Create a skill

## A1. Capture intent

Determine:

- desired behavior;
- trigger phrases and contexts;
- expected output or side effects;
- project, user, or pi-agent scope.

Ask only for information that cannot be safely inferred. If the request already answers
these points, do not repeat questions.

## A2. Choose name and target

Use a concise kebab-case name. Resolve the destination from the shared location table;
default to user/global scope when scope is omitted.

Before writing, check whether the target already exists. If it exists, treat the task as
an update and preserve useful existing instructions unless the user explicitly asks for
a replacement.

## A3. Write

Create `<target>/<name>/SKILL.md` with valid frontmatter and focused instructions. Use
the `write` tool for a new file and `edit` for targeted changes to an existing file.
Do not create variant files such as `SKILL-v2.md`.

## A4. Verify

Read the resulting `SKILL.md` and confirm:

- frontmatter delimiters are present;
- `name` and `description` are non-empty;
- description names realistic trigger conditions;
- body contains actionable instructions;
- destination matches requested scope.

Then ask the user to test with a message that should trigger the skill. Iterate from
observed behavior rather than creating another skill.

---

# Workflow B — Install or update from GitHub

## B1. Parse the URL

Users may provide a repository root, directory URL, or direct `SKILL.md` URL.

| URL shape | Action |
|---|---|
| `github.com/owner/repo` | Ask which skill/directory |
| `github.com/owner/repo/tree/BRANCH/path/to/skill` | Derive repo, branch path, skill name |
| `github.com/owner/repo/blob/BRANCH/path/to/skill/SKILL.md` | Use parent directory as skill |

Derive:

1. **Repo URL**: `https://github.com/OWNER/REPO.git`.
2. **Branch**: branch named by `tree/BRANCH/` or `blob/BRANCH/`; omit only for a
   repository-root URL so Git uses its default branch.
3. **In-repo path**: portion after the branch.
4. **Sparse path**: skill directory; remove trailing `/SKILL.md`.
5. **Skill name**: final sparse-path segment.

Examples:

```text
https://github.com/hugohe3/ppt-master/blob/main/skills/ppt-master/SKILL.md
→ repo:   https://github.com/hugohe3/ppt-master.git
→ sparse: skills/ppt-master
→ name:   ppt-master

https://github.com/anthropics/skills/tree/main/skills/xlsx
→ repo:   https://github.com/anthropics/skills.git
→ sparse: skills/xlsx
→ name:   xlsx

https://github.com/user/myskills/tree/main/rust
→ repo:   https://github.com/user/myskills.git
→ sparse: rust
→ name:   rust
```

Only ask which skill when the URL contains no in-repo path. Do not guess among several
repository skills.

## B2. Resolve destination

Map scope using the shared location table. Default:

```text
~/.config/zcode/skills/SKILL_NAME
```

## B3. Git gate

Normally run:

```bash
git --version
```

macOS exception: Apple's `/usr/bin/git` shim can open the Command Line Tools installer
when tools are absent. Avoid triggering that GUI. On Darwin, preflight first:

```bash
if [ "$(command -v git 2>/dev/null)" = "/usr/bin/git" ] && ! xcode-select -p >/dev/null 2>&1; then
  echo "Git unavailable: Apple Command Line Tools are not installed" >&2
  exit 127
fi
git --version
```

Interpret carefully:

- Version prints successfully → continue to B4.
- Executable is unavailable (`command not found`, PowerShell “not recognized”, or the
  macOS preflight exits 127) → run [Git bootstrap](#git-bootstrap-route-no-separate-skill),
  verify, then return to B4.
- Git runs but another operation failed (`not a git repository`, authentication,
  network, merge conflict) → report that actual error. Do not reinstall Git.

A failed `git status` alone never proves that Git is missing; the version gate above is
canonical.

## B4. Sparse clone

Git network operations may take 30–120 seconds. Use a timeout of at least 120 seconds,
or omit timeout only when tool default is at least 120 seconds. Keep clone and copy in
separate tool calls. When B1 provided a branch, add `--branch "BRANCH"` before
`--no-checkout`; otherwise omit that argument.

macOS/Linux clone call:

```bash
TMP=$(mktemp -d)
git clone --depth 1 --no-checkout REPO_URL "$TMP"
cd "$TMP" && git sparse-checkout set "SPARSE_PATH" && git checkout
echo "$TMP"
```

Windows PowerShell clone call:

```powershell
$tmp = Join-Path $env:TEMP "skill_$(Get-Random)"
git clone --depth 1 --no-checkout REPO_URL $tmp
Push-Location $tmp
git sparse-checkout set "SPARSE_PATH"
git checkout
Pop-Location
Write-Output $tmp
```

Do not continue when clone, sparse-checkout, or checkout fails.

## B5. Copy and clean up

Replace `TMP`, `SPARSE_PATH`, and `TARGET_DIR` with resolved values.

macOS/Linux:

```bash
mkdir -p "TARGET_DIR"
cp -r "TMP/SPARSE_PATH"/. "TARGET_DIR"/
rm -rf "TMP"
```

Windows PowerShell:

```powershell
New-Item -ItemType Directory -Path "TARGET_DIR" -Force | Out-Null
Copy-Item -Recurse -Force "TMP\SPARSE_PATH\*" "TARGET_DIR"
Remove-Item -Recurse -Force "TMP"
```

Delete only the temporary directory created by B4. Never broaden cleanup to its parent
or an unresolved variable.

For updates, copy into the same target. Preserve target scope and re-run verification;
do not create a second directory with a version suffix.

## B6. Verify installation

Use `read` on `TARGET_DIR/SKILL.md`. Confirm:

- file exists;
- frontmatter parses;
- `name` and `description` exist;
- installed name and requested skill agree;
- no clone failure was hidden by a later successful command.

Only then report successful installation.

---

# Git bootstrap route (no separate skill)

This is an internal prerequisite branch of Workflow B. Do not create a
Git helper skill or another `SKILL.md`.

Route:

```text
detect missing executable
→ identify supported platform
→ install without GUI interaction
→ refresh PATH if needed
→ re-run git --version
→ notify user
→ return to B4
```

## C1. Trigger boundaries

Enter this branch only when `git --version` confirms that the executable is missing.
Typical signals:

- macOS: `command not found: git` or B3 detects an unusable Apple Git shim;
- Windows PowerShell: `git is not recognized`;
- prerequisite checklist failed, followed by failed `git --version`.

Do not enter for normal Git errors.

This bootstrap supports macOS and Windows only. Detect the actual OS; bash/zsh does not
prove macOS. On Linux or another platform, stop and report that automatic bootstrap is
unsupported.

If tools execute in a sandbox/container rather than on the user's machine, state which
environment was checked. Do not claim the user's computer was modified.

No graphical installer or command that opens one is allowed. In particular, never use:

```bash
xcode-select --install
```

## C2. macOS

First verify `uname -s` returns `Darwin`. Use the B3 preflight instead of invoking an
unconfigured `/usr/bin/git` shim.

When Homebrew exists:

```bash
command -v brew
brew install git
hash -r
git --version
```

When Homebrew is absent:

```bash
NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
elif [ -x /usr/local/bin/brew ]; then
  eval "$(/usr/local/bin/brew shellenv)"
else
  echo "Homebrew installation finished but brew was not found" >&2
  exit 1
fi
brew install git
hash -r
git --version
```

Use at least a five-minute timeout. `curl` is allowed here only for Homebrew's official
bootstrap because Git is unavailable. `NONINTERACTIVE=1` suppresses prompts; if the
operation fails because Command Line Tools, privileges, or network access are missing,
do not fall back to a GUI. Report the exact error and provide:

<https://git-scm.com/download/mac>

## C3. Windows default — MinGit

Use official MinGit unless the user explicitly requests Git Bash, Git GUI, or full Git
for Windows. MinGit supplies command-line `git.exe` without an installer wizard,
administrator rights, Git Bash, Git GUI, or shell menus.

Run in PowerShell:

```powershell
$ErrorActionPreference = "Stop"

$release = Invoke-RestMethod `
  -Uri "https://api.github.com/repos/git-for-windows/git/releases/latest" `
  -Headers @{ "User-Agent" = "zcode-skill-creator" }

$pattern = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
  "MinGit-*-arm64.zip"
} else {
  "MinGit-*-64-bit.zip"
}
$asset = $release.assets |
  Where-Object { $_.name -like $pattern -and $_.name -notlike "*-busybox-*" } |
  Select-Object -First 1
if (-not $asset) {
  throw "No matching MinGit asset found for $env:PROCESSOR_ARCHITECTURE"
}

$zipPath = Join-Path $env:TEMP "MinGit.zip"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\MinGit"
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath
New-Item -ItemType Directory -Path $installDir -Force | Out-Null
Expand-Archive -Path $zipPath -DestinationPath $installDir -Force
Remove-Item -Path $zipPath -Force -ErrorAction SilentlyContinue

$gitCmdDir = Join-Path $installDir "cmd"
$gitExe = Join-Path $gitCmdDir "git.exe"
if (-not (Test-Path -LiteralPath $gitExe -PathType Leaf)) {
  throw "MinGit archive extracted, but git.exe was not found at $gitExe"
}

$userPath = [System.Environment]::GetEnvironmentVariable("Path", "User")
$userEntries = @($userPath -split ";" | Where-Object { $_ })
if ($userEntries -notcontains $gitCmdDir) {
  $newUserPath = (@($userEntries) + $gitCmdDir) -join ";"
  [System.Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
}
if (($env:Path -split ";") -notcontains $gitCmdDir) {
  $env:Path += ";$gitCmdDir"
}

git --version
```

The GitHub API and MinGit ZIP are allowed HTTP bootstrap exceptions because Git cannot
clone its own prerequisite.

If extraction succeeds but command lookup still fails, refresh PATH once and verify:

```powershell
$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
  [System.Environment]::GetEnvironmentVariable("Path", "User")
git --version
```

## C4. Windows full Git — explicit request only

When the user explicitly needs Git Bash, Git GUI, or full Git for Windows:

```powershell
winget install --id Git.Git -e --source winget --silent --accept-package-agreements --accept-source-agreements
$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
  [System.Environment]::GetEnvironmentVariable("Path", "User")
git --version
```

If `winget` is unavailable, do not launch an interactive installer. Mention Chocolatey
or Scoop only when already installed. Otherwise provide:

<https://git-scm.com/download/win>

## C5. Verification and notification

Every installation branch must end with a new `git --version` call. Success requires a
printed version such as:

```text
git version 2.54.0
git version 2.54.0.windows.1
```

Notify according to verified state:

- **Verified**: Git was missing, installation succeeded, include exact version, continue
  to B4.
- **Still installing**: say it may take a few minutes; do not imply completion.
- **Full path works but PATH lookup fails**: explain that a new terminal may be needed.
- **Failed**: include actual error and platform-specific manual URL.

Never say Git is installed successfully before verification succeeds.

---

# Final checklist

## Created skill

- Correct scope and path
- Valid `name` and trigger-rich `description`
- Focused instructions
- Result re-read
- Test prompt suggested

## Installed skill

- URL parsed without guessing
- Scope resolved
- Git gate passed or bootstrap verified
- Shallow sparse-checkout used
- Temporary clone cleaned safely
- Installed `SKILL.md` re-read and validated

## Never do

- Create a separate Git helper skill
- Fetch repository skill files one-by-one over HTTP
- Treat `git status` failure as proof Git is absent
- Use a GUI installer in automatic bootstrap
- Claim success before verification
