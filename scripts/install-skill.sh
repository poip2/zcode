#!/usr/bin/env bash
# =============================================================================
# install-skill.sh — Install a zcode skill from any GitHub repo
# =============================================================================
# Uses git clone --depth 1 + sparse-checkout for maximum speed.
# No rate limits, no 130KB HTML downloads, just the files you need in ~3-10s.
#
# Usage:
#   ./install-skill.sh <repo-url> <skill-name> [--global|--project|--agents]
#
# Examples:
#   ./install-skill.sh https://github.com/anthropics/skills.git xlsx --global
#   ./install-skill.sh https://github.com/user/my-skills.git my-skill --project
#   ./install-skill.sh https://github.com/user/skills.git rust --agents
# =============================================================================
set -euo pipefail

# --- Help ---
if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ] || [ $# -lt 2 ]; then
  cat << 'EOF'
Usage: install-skill.sh <repo-url> <skill-name> [--global|--project|--agents]

Install a zcode skill from a GitHub repository using git sparse-checkout.

Arguments:
  repo-url     Git clone URL (e.g. https://github.com/anthropics/skills.git)
  skill-name   Skill directory name (e.g. xlsx, rust, skill-creator)
  --project    Install to .zcode/skills/ (default, project-scoped)
  --global     Install to ~/.config/zcode/skills/ (all projects)
  --agents     Install to ~/.agents/skills/ (pi agent skills)

Examples:
  ./install-skill.sh https://github.com/anthropics/skills.git xlsx --global
  ./install-skill.sh https://github.com/user/my-skills.git my-skill --project

Performance: ~3-10s (vs ~20s+ for HTTP file-by-file)
EOF
  exit 0
fi

REPO_URL="$1"
SKILL_NAME="$2"
SCOPE="${3:---project}"

# --- Prerequisites check ---
if ! command -v git &> /dev/null; then
  echo "✗ Git is not installed or not on PATH." >&2
  echo "  Install it from: https://git-scm.com/downloads" >&2
  exit 1
fi

# --- Resolve target directory ---
case "$SCOPE" in
  --global)
    TARGET_DIR="${HOME}/.config/zcode/skills/${SKILL_NAME}"
    ;;
  --agents)
    TARGET_DIR="${HOME}/.agents/skills/${SKILL_NAME}"
    ;;
  --project|*)
    TARGET_DIR="$(pwd)/.zcode/skills/${SKILL_NAME}"
    ;;
esac

# --- Detect if skill path has "skills/" prefix ---
# Most repos use skills/<name>/ layout; some put skills at root.
# Try "skills/<name>" first; fall back to just "<name>".
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "→ Cloning ${REPO_URL} (depth 1, no checkout)..."

if ! git clone --depth 1 --no-checkout "$REPO_URL" "$TMP_DIR" 2>&1; then
  echo "✗ Clone failed. Check the repo URL and your network connection." >&2
  exit 1
fi

# Try skills/<name> first
SPARSE_PATH="skills/${SKILL_NAME}"
echo "→ Sparse-checkout: ${SPARSE_PATH}"

git -C "$TMP_DIR" sparse-checkout set "$SPARSE_PATH" 2>/dev/null
git -C "$TMP_DIR" checkout 2>/dev/null

SRC="${TMP_DIR}/${SPARSE_PATH}"

# Fallback: try without skills/ prefix
if [ ! -d "$SRC" ]; then
  echo "→ 'skills/${SKILL_NAME}' not found, trying '${SKILL_NAME}' at repo root..."
  git -C "$TMP_DIR" sparse-checkout set "$SKILL_NAME"
  git -C "$TMP_DIR" checkout
  SRC="${TMP_DIR}/${SKILL_NAME}"
fi

if [ ! -d "$SRC" ]; then
  echo "✗ Skill directory '${SKILL_NAME}' not found in repo." >&2
  echo "  Available directories (top-level):" >&2
  ls -1 "$TMP_DIR/" 2>/dev/null || echo "  (empty)" >&2
  if [ -d "${TMP_DIR}/skills" ]; then
    echo "  Available skills/:" >&2
    ls -1 "${TMP_DIR}/skills/" 2>/dev/null || echo "  (empty)" >&2
  fi
  exit 1
fi

# --- Install ---
echo "→ Installing to ${TARGET_DIR}"
mkdir -p "$TARGET_DIR"

# Copy all files including dotfiles, preserving structure
cp -r "${SRC}/." "$TARGET_DIR/"

# Verify SKILL.md exists
if [ -f "${TARGET_DIR}/SKILL.md" ]; then
  SKILL_COUNT=$(find "$TARGET_DIR" -type f | wc -l | tr -d ' ')
  echo "✓ Installed ${SKILL_NAME} → ${TARGET_DIR} (${SKILL_COUNT} files)"
else
  echo "⚠ Installed files but no SKILL.md found in ${TARGET_DIR}" >&2
  echo "  The skill may not be recognized by zcode." >&2
fi
