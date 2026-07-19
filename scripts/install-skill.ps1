# =============================================================================
# install-skill.ps1 — Install a zcode skill from any GitHub repo
# =============================================================================
# Uses git clone --depth 1 + sparse-checkout for maximum speed.
# No rate limits, no 130KB HTML downloads, just the files you need in ~3-10s.
#
# Usage:
#   .\install-skill.ps1 <repo-url> <skill-name> [--global|--project|--agents]
#
# Examples:
#   .\install-skill.ps1 https://github.com/anthropics/skills.git xlsx --global
#   .\install-skill.ps1 https://github.com/user/my-skills.git my-skill --project
#   .\install-skill.ps1 https://github.com/user/skills.git rust --agents
# =============================================================================
param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$RepoUrl,

    [Parameter(Mandatory=$true, Position=1)]
    [string]$SkillName,

    [Parameter(Mandatory=$false, Position=2)]
    [ValidateSet("--global", "--project", "--agents")]
    [string]$Scope = "--project"
)

$ErrorActionPreference = "Stop"

# --- Prerequisites check ---
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "✗ Git is not installed or not on PATH." -ForegroundColor Red
    Write-Host "  Install it from: https://git-scm.com/downloads" -ForegroundColor Yellow
    exit 1
}

# --- Resolve target directory ---
switch ($Scope) {
    "--global"  { $TargetDir = Join-Path $env:USERPROFILE ".config\zcode\skills\$SkillName" }
    "--agents"  { $TargetDir = Join-Path $env:USERPROFILE ".agents\skills\$SkillName" }
    "--project" { $TargetDir = Join-Path (Get-Location) ".zcode\skills\$SkillName" }
    default     { $TargetDir = Join-Path (Get-Location) ".zcode\skills\$SkillName" }
}

$TmpDir = Join-Path $env:TEMP "skill_install_$(Get-Random)"

try {
    Write-Host "→ Cloning $RepoUrl (depth 1, no checkout)..." -ForegroundColor Cyan

    git clone --depth 1 --no-checkout $RepoUrl $TmpDir 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "✗ Clone failed. Check the repo URL and your network connection." -ForegroundColor Red
        exit 1
    }

    # Try skills/<name> first (standard layout)
    $SparsePath = "skills/$SkillName"
    Write-Host "→ Sparse-checkout: $SparsePath" -ForegroundColor Cyan

    Push-Location $TmpDir
    git sparse-checkout set $SparsePath 2>&1 | Out-Null
    git checkout 2>&1 | Out-Null
    Pop-Location

    $Src = Join-Path $TmpDir $SparsePath

    # Fallback: try without skills/ prefix
    if (-not (Test-Path $Src)) {
        Write-Host "→ 'skills/$SkillName' not found, trying '$SkillName' at repo root..." -ForegroundColor Yellow
        Push-Location $TmpDir
        git sparse-checkout set $SkillName 2>&1 | Out-Null
        git checkout 2>&1 | Out-Null
        Pop-Location
        $Src = Join-Path $TmpDir $SkillName
    }

    if (-not (Test-Path $Src)) {
        Write-Host "✗ Skill directory '$SkillName' not found in repo." -ForegroundColor Red
        Write-Host "  Available directories (top-level):" -ForegroundColor Yellow
        Get-ChildItem $TmpDir -Directory | ForEach-Object { Write-Host "    $($_.Name)" }
        $skillsDir = Join-Path $TmpDir "skills"
        if (Test-Path $skillsDir) {
            Write-Host "  Available skills/:" -ForegroundColor Yellow
            Get-ChildItem $skillsDir -Directory | ForEach-Object { Write-Host "    $($_.Name)" }
        }
        exit 1
    }

    # --- Install ---
    Write-Host "→ Installing to $TargetDir" -ForegroundColor Cyan
    if (-not (Test-Path $TargetDir)) {
        New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
    }

    Copy-Item -Recurse -Force "$Src\*" $TargetDir

    # Verify SKILL.md exists
    $skillMd = Join-Path $TargetDir "SKILL.md"
    if (Test-Path $skillMd) {
        $fileCount = (Get-ChildItem $TargetDir -Recurse -File).Count
        Write-Host "✓ Installed $SkillName → $TargetDir ($fileCount files)" -ForegroundColor Green
    } else {
        Write-Host "⚠ Installed files but no SKILL.md found in $TargetDir" -ForegroundColor Yellow
        Write-Host "  The skill may not be recognized by zcode." -ForegroundColor Yellow
    }

} finally {
    if (Test-Path $TmpDir) {
        Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
    }
}
