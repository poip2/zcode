/** Match Rust MARKDOWN_EXTS in commands.rs. */
export function isMarkdownExt(name: string): boolean {
  const lower = name.toLowerCase();
  return (
    lower.endsWith(".md") ||
    lower.endsWith(".markdown") ||
    lower.endsWith(".mdown") ||
    lower.endsWith(".mkd")
  );
}
