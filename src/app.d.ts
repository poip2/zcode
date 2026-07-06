declare module "markdown-it-task-lists" {
  import type MarkdownIt from "markdown-it";
  function taskLists(md: MarkdownIt, options?: { enabled?: boolean; label?: boolean }): void;
  export default taskLists;
}

declare module "markdown-it-texmath" {
  import type MarkdownIt from "markdown-it";
  function texmath(md: MarkdownIt, options?: { engine: unknown; delimiters?: string }): void;
  export default texmath;
}
