The `shell` tool runs commands through `bash` on this Unix-like system. Prefer portable
commands; Bash syntax is available when needed for NUL-safe filename handling.

Use `shell` for directory inspection and all path/content searches. Search programs use exit
code 1 for a valid “no matches” result, so pass `successExitCodes: [0, 1]` in shell tool calls.
Use `set -o pipefail` so upstream failures survive pipelines. Bound displayed output with POSIX
`awk 'NR <= limit'`; unlike `head`, it drains input instead of causing an expected SIGPIPE.

```bash
# Inspect a directory
ls -la

# Find files. Include hidden files, preserve ignore rules in Git worktrees, cap output.
set -o pipefail
if command -v rg >/dev/null 2>&1; then
  rg --files --hidden -g '*.md' -g '!.git' | awk 'NR <= 200'
elif git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git -c core.quotepath=false ls-files -co --exclude-standard -- '*.md' | awk 'NR <= 200'
else
  find . -type d \( -name .git -o -name node_modules -o -name target \) -prune \
    -o -type f -name '*.md' -print | awk 'NR <= 200'
  code=${PIPESTATUS[0]}
  [ "$code" -eq 0 ] || exit 2
fi

# Search content. Git fallback searches tracked and untracked non-ignored files.
search_git_files() {
  local candidates file code found=1
  candidates=$(mktemp) || return 2
  git ls-files -co --exclude-standard -z >"$candidates"
  code=$?
  if [ "$code" -ne 0 ]; then
    rm -f "$candidates"
    return 2
  fi
  while IFS= read -r -d '' file; do
    grep -nH -- 'TODO' "$file"
    code=$?
    case "$code" in
      0) found=0 ;;
      1) ;;
      *) rm -f "$candidates"; return "$code" ;;
    esac
  done <"$candidates"
  rm -f "$candidates"
  return "$found"
}

search_filesystem() {
  local candidates file code found=1
  candidates=$(mktemp) || return 2
  find . -type d \( -name .git -o -name node_modules -o -name target \) -prune \
    -o -type f -print0 >"$candidates"
  code=$?
  if [ "$code" -ne 0 ]; then
    rm -f "$candidates"
    return 2
  fi
  while IFS= read -r -d '' file; do
    grep -nH -- 'TODO' "$file"
    code=$?
    case "$code" in
      0) found=0 ;;
      1) ;;
      *) rm -f "$candidates"; return "$code" ;;
    esac
  done <"$candidates"
  rm -f "$candidates"
  return "$found"
}

set -o pipefail
if command -v rg >/dev/null 2>&1; then
  rg -n --hidden -g '!.git' -- 'TODO' . | awk 'NR <= 200'
elif git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  search_git_files | awk 'NR <= 200'
else
  search_filesystem | awk 'NR <= 200'
fi
```

`rg` and `fd` are optional and may be absent. Do not ask user to install them merely to search
files. macOS ships BSD `find` and `grep`, not GNU variants: avoid GNU-only options such as
`find -printf` and `grep -P`. Keep searches narrow and bounded.
