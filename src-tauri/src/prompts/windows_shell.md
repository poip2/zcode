The `shell` tool runs commands through **Windows PowerShell** (pwsh or powershell.exe),
NOT bash/sh. Write PowerShell syntax, not POSIX shell syntax.

When executing commands from a skill or script document written for bash, you MUST
mentally translate them to PowerShell using this table:

| Bash / POSIX | PowerShell | Notes |
|---|---|---|
| `rm -rf <path>` | `Remove-Item -Recurse -Force <path>` | ⚠️ DANGER on Windows: if `<path>` contains an NTFS junction or symlink (e.g. a `node_modules` tree with pnpm/npm links), `-Recurse -Force` can follow the link and delete the REAL target directory's contents instead of just removing the link — this has caused real, permanent data loss. Never run this on a directory you haven't inspected; if in doubt, list contents first and ask before deleting, per the File Safety rule above. |
| `cmd1 && cmd2` | `cmd1; if ($LASTEXITCODE -eq 0) { cmd2 }` | PowerShell 5.1 does NOT support `&&` / `\|\|` chaining (parse error). Use `$LASTEXITCODE`, not `$?` — for native/external executables `$?` can be unreliable (e.g. some tools writing to stderr, or a later command overwriting `$?`, can make it read `False` even though the exit code was 0). `$LASTEXITCODE` holds the actual numeric exit code of the last native command and is not affected by that. |
| `export VAR=value` | `$env:VAR = "value"` | Only affects current process and its children |
| `test -f file && echo yes` | `if (Test-Path file -PathType Leaf) { echo yes }` | Plain `Test-Path file` matches files AND directories (like `test -e`). Use `-PathType Leaf` for "is a file" (`test -f`) and `-PathType Container` for "is a directory" (`test -d`). |
| `which <cmd>` | `Get-Command <cmd> -ErrorAction SilentlyContinue` | Without `-ErrorAction SilentlyContinue`, a missing command prints a noisy red error to the output instead of just returning nothing. |
| `find . -name "*.md"` | `Get-ChildItem -Recurse -Filter *.md` | |
| `$VAR` (env ref) | `$env:VAR` | Bare `$VAR` in PowerShell is a regular variable, not an environment variable |
| `cat a \| grep b` | `Get-Content a \| Select-String b` | `cat` / `grep` exist as aliases but parameter semantics differ — prefer native cmdlets |

### File Safety: Text Encoding (writing files)

PowerShell's `-Encoding UTF8` parameter (in both `Set-Content` and `Out-File`) means
**UTF-8 WITH BOM**, on both Windows PowerShell 5.1 and PowerShell 7+. This is a
historical quirk, not a bug that got fixed in pwsh core.

Any file that will be parsed by a non-Windows-native tool — YAML frontmatter,
JSON, shell scripts, Python, anything read on Linux/macOS, or files fetched from
GitHub and rewritten to disk — MUST be written without a BOM:

| Task | ❌ Adds BOM | ✅ No BOM |
|---|---|---|
| Write string to file | `Set-Content -Encoding UTF8` | `Set-Content -Encoding utf8NoBOM` |
| Write string to file (alt) | `Out-File -Encoding UTF8` | `Out-File -Encoding utf8NoBOM` |
| Byte-level write (safest) | — | `[System.IO.File]::WriteAllText($path, $content, [System.Text.UTF8Encoding]::new($false))` |

Default rule: whenever downloading/copying a file that already has known-good
content (e.g. `Invoke-RestMethod` result, a script from a repo), prefer the
byte-level `WriteAllText`/`WriteAllBytes` form over `Set-Content`/`Out-File`
entirely, since it skips PowerShell's encoding guesswork.

After writing any such file, verify: first 3 bytes must NOT be `EF BB BF`.
