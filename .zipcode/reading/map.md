# zcode — 极简 Markdown 编辑器 + AI Agent

一个 Tauri v2 桌面应用。暖白单色 UI，侧边栏文件树，AI Agent 编程代理（读/写/执行/搜索工具链），四文件夹工作区约定，原生拖放导入。技术栈：SvelteKit 5 + Tauri v2 + Rust 后端，markdown-it 渲染，Anthropic/OpenAI 双协议 LLM 支持。

<!-- node:app.entry status:unvisited -->
### 应用入口
```python
def main():
    tauri_builder = TauriBuilder.default()
    tauri_builder.plugin(opener_plugin)
    tauri_builder.plugin(dialog_plugin)
    tauri_builder.plugin(store_plugin)
    tauri_builder.setup(migrate_legacy_api_key())
    tauri_builder.manage(SessionManager.new())
    tauri_builder.manage(RuntimeState.default())
    tauri_builder.manage(WatcherState.default())
    tauri_builder.manage(SkillWatcherState.default())
    tauri_builder.invoke_handler([
        read_markdown_file, write_markdown_file, resolve_path,
        read_dir_tree, create_markdown_file, create_folder,
        start_agent_turn, approve_tool_call,
        list_skills, set_skill_active,
        list_sessions, close_session, clear_session,
        watch_files, watch_skills,
        check_api_key, save_api_key, call_ai_provider,
        ...
    ])
    tauri_builder.run()
```
<!-- /node -->

入口启动 Tauri 应用，注册所有命令和插件，初始化托管状态（会话管理器、运行时环境、文件监听器）。

<!-- node:app.commands status:unvisited -->
### 文件操作命令
```python
def read_markdown_file(path: str) -> str:       # 读取文件内容
def write_markdown_file(path: str, content: str): # 写入文件
def resolve_path(path: str) -> str:              # 解析为绝对路径
def read_dir_tree(root: str) -> DirNode:         # 递归读取目录树（depth≤3，跳过隐藏/无用目录）
def create_markdown_file(dir: str, name: str):   # 创建新 .md 文件
def create_folder(dir: str, name: str):          # 创建新文件夹
def get_default_data_dir(app) -> str:            # 获取默认数据目录
def list_folder_flat(folder: str) -> Vec<DirNode>: # 平铺列出目录中所有可见文件
def copy_file_to_folder(src: str, dst_dir: str):  # 复制文件到目标文件夹（同名自动加 (1)(2) 后缀）
def open_in_shell(path: str):                     # 在系统文件管理器中打开
```
<!-- /node -->

文件操作层。目录树最多 3 层，跳过 node_modules/target/dist 等非内容目录和隐藏项。文件复制永不覆盖——同名文件自动递增编号。

<!-- node:app.keychain status:unvisited -->
### API Key 管理
```python
def check_api_key() -> ApiKeyStatus:       # 查询 keychain 中是否有 key  ⚠ 未确认返回格式
def save_api_key(api_key: str) -> Option[str]:  # 存储或删除（空串）key  ⚠ 未确认 warning 内容
# API key 存在 OS keychain（macOS Keychain/Windows Credential Manager/Linux secret-service）
# 前端 store 只存 masked 版本（如 sk-5d70d***5c60）
```
<!-- /node -->

API key 通过 OS 级 keyring 存储，不写入明文配置。Linux 需要 gnome-keyring 或 kwallet 等 D-Bus secret-service 守护进程。

<!-- node:app.sidebar_files status:unvisited -->
### 侧边栏文件树
```python
# 前端 Svelte 组件 + tauri 命令 read_dir_tree
# 显示当前工作目录中的可读文件树（深度≤3）
# 显示文件扩展名: .md .docx .xlsx .pdf .csv .txt .json .yaml .html 等
# 排序: 目录 > 文件，各自按字母序
# 隐藏 .dotfiles 和 node_modules/target/dist 等目录
```
<!-- /node -->

<!-- node:editor.markdown_render status:unvisited -->
### Markdown 渲染管线
```python
# 前端 Svelte 组件 MarkdownRenderer + markdown-it
# 插件: markdown-it-anchor（标题锚点）, katex（LaTeX）, highlight.js（代码高亮）, markdown-it-task-lists（任务列表）
# 额外插件: markdown-it-texmath（$...$ 数学公式）, dompurify（XSS 防护）
def renderFull(text: str) -> str:
    md = MarkdownIt({html: true, linkify: true, typographer: true})
    md.use(anchor)
    md.use(texmath, {engine: katex})
    md.use(task_lists)
    rendered = md.render(text)
    return DOMPurify.sanitize(rendered)
```
<!-- /node -->

markdown-it 渲染 + DOMPurify 安全过滤，支持 LaTeX、代码高亮、任务列表。

<!-- node:editor.editor status:unvisited -->
### 编辑器组件
```python
# Svelte 组件 Editor.svelte
# 编辑/预览切换（isEditing 状态）
# 脏状态追踪（dirty flag）
# 保存时调 saveFile() → 调后端 write_markdown_file
# 拖放导入文件（自动复制到 sources 文件夹）
# 文件变更监听（watch file changes → reload）
```
<!-- /node -->

<!-- node:agent.core_loop status:unvisited -->
### Agent 核心循环
```python
def agent_run(session_id, user_input, tools, provider, token):
    emit(AgentStart)
    messages = load_history(session_id)
    user_msg = UserMessage(content=user_input)
    messages.append(user_msg)

    while not token.is_cancelled():
        context = Context(system_prompt, messages, tool_defs)
        stream = provider.stream(context, options)
        for event in stream:
            if event.type == TextDelta:
                emit(MessageUpdate)
            elif event.type == ToolCallStart:
                emit(ToolStart)
            elif event.type == ToolCallEnd:
                result = execute_tool(tool_name, arguments)  # 危险工具等待用户批准
                emit(ToolEnd)
                messages.append(tool_result_msg)

        if stop_reason == "end_turn":
            break  # 正常结束
        # 有 tool call → 继续循环

    emit(AgentEnd)
```
<!-- /node -->

Agent 核心：流式 LLM 调用 → Tool 执行 → 循环直到模型结束输出。支持多轮 tool call 链。

<!-- node:agent.stream_provider status:unvisited -->
### LLM 提供者抽象层
```python
# trait Provider {  # async_trait
#     fn name(&self) -> &str       # "anthropic" | "openai"
#     fn api(&self) -> &str        # "anthropic-messages" | "openai-completions"
#     fn model_id(&self) -> &str   # 如 "claude-sonnet-4-5-20250929"
#     async fn stream(context, options) -> Stream<StreamEvent>
# }

# 实现:
# - AnthropicProvider: Anthropic Messages API（流式 SSE）
# - OpenAIProvider: 兼容 OpenAI Chat Completions API（流式 SSE）
```
<!-- /node -->

双协议支持。通过 trait 抽象，前端传 base_url + model + provider_name 来路由。

<!-- node:agent.tools status:unvisited -->
### Agent 工具集
```python
# trait Tool { fn invoke(params) -> ToolOutput; fn spec() -> ToolDef; }
# 
# Read-only 工具（自动执行）:
#   read(path)         → 读取文件内容
#   grep(pattern)      → 搜索文件内容
#   find(pattern)      → 按 glob 查找文件
#   ls(path)           → 列出目录内容
# 
# Dangerous 工具（需用户批准）:
#   write(path, content)  → 写入/覆盖文件
#   edit(path, old, new)  → 精确替换文件内容
#   shell(command)        → 执行 bash 命令
# 
# 危险工具批准机制: session 内 oneshot channel，前端弹出确认对话框
# 对当前打开的文件的 write/edit 跳过确认（smart auto-approve）
```
<!-- /node -->

8 个工具（4 只读 + 3 危险 + 1 其他），危险工具通过 human-in-the-loop 确认，当前文件操作自动批准。

<!-- node:agent.sessions status:unvisited -->
### 会话管理
```python
# SessionManager: Arc<Mutex<HashMap<String, Session>>>
# Session:
#   messages: Vec<Message>       # 对话历史
#   cancel_token: CancellationToken
#   approval_channels: HashMap<String, oneshot::Sender<bool>>
#   current_file: Option<String>  # 用户当前打开的文件（用于 auto-approve）
#   cwd: String                   # 工作目录
#
# 命令: start_agent_turn, approve_tool_call, close_session, clear_session, list_sessions
# 前端通过 session-scoped events (agent://{session_id}/...) 订阅流式事件
```
<!-- /node -->

支持多会话并行。每个会话有独立的取消令牌、批准通道、文件指针。

<!-- node:agent.compaction status:unvisited -->
### 上下文压缩
```python
def compact_if_needed(messages, context_window):
    tokens = estimate_tokens(messages)        # 字符数/3 ≈ token 数
    if tokens > context_window - RESERVE:     # 超过阈值
        cut_point = find_cut_point(messages, keep_recent=6000_tokens)
        old_msgs = messages[:cut_point]
        summary = summarize_with_llm(old_msgs)  # 调用 LLM 自己压缩自己
        messages = [summary] + messages[cut_point:]
```
<!-- /node -->

防止长会话 token 溢出。用 LLM 自己总结历史，替换早期消息。保守估计（每 token 3 字符）。

<!-- node:agent.skills_loader status:unvisited -->
### Skill 加载系统
```python
def load_skills(cwd, user_config_dir, extra_paths) -> (Vec<Skill>, Vec<String>):
    # 三层加载（优先级 project > user > builtin）
    # builtin: 编译到二进制中的 skill-creator（include_str!）
    # user: ~/.config/zipcode/skills/*/SKILL.md
    # project: .zcode/skills/*/SKILL.md 或 .zipcode/skills/*/SKILL.md
    # 同名 skill 高优先级覆盖低优先级
    # 格式化后注入 system prompt
```
<!-- /node -->

三层 Skill 系统：内置（编译时）、用户级（~/.config）、项目级（.zcode/ 或 .zipcode/）。同名上级覆盖下级。

<!-- node:agent.runtime_env status:unvisited -->
### 运行时环境
```python
# AgentRuntime:
#   venv_dir: PathBuf       # 捆绑的 Python 虚拟环境路径
#   bun_bin_dir: PathBuf    # 捆绑的 Bun 运行时路径
#
# 通过 tauri Resource 协议解析路径
# Windows: resources/runtime/python/python.exe + resources/runtime/bin/uv.exe
# macOS/Linux: resources/runtime/python/bin/python3
# 前端 Agent 调用 shell 工具时自动设置 PATH 指向这些运行时
```
<!-- /node -->

应用捆绑了 Python (uv 管理) 和 Bun 运行时。shell 工具在这些路径下执行。

<!-- node:frontend.main_ui status:unvisited -->
### Svelte 前端主界面
```python
# +page.svelte: 主页面布局
# 组件:
#   TitleBar:      顶栏（标题 + 窗口控制）
#   Sidebar:      左侧文件树 + 工作区文件夹
#   Editor:       编辑区域（编辑/预览切换）
#   MarkdownRenderer: 渲染后的 Markdown 视图
#   AgentPanel:   AI 聊天面板
#   AgentFab:     浮动按钮（打开 Agent 面板）
#   SettingsDialog: 设置对话框（API key、提供者、主题）
#   ToolConfirmDialog: 危险工具确认对话框
#
# 状态管理: Svelte 5 $state / stores
# 拖放: 原生拖放导入文件 → 复制到 sources 文件夹
# 文件监听: tauri watcher → 文件变更自动刷新
```
<!-- /node -->

<!-- node:frontend.stores status:unvisited -->
### 前端状态管理
```python
# stores/:
#   document.ts:          当前文档内容/路径/脏状态
#   folderTree.ts:        文件树数据
#   settings.ts:          设置（theme, model, base_url, etc.）
#   agentSession.ts:      Agent 会话状态
#   pinnedFolder.ts:      固定的工作区文件夹
#   workspaceFiles.ts:    工作区文件列表
#   recents.ts:           最近文件
#   externalFile.ts:      外部导入的文件
#   skills.svelte.ts:     Skill 管理状态
#   sharedStore.ts:       共享存储（tauri-plugin-store 包装）
```
<!-- /node -->

Svelte 5 响应式状态。settings 持久化到 tauri-plugin-store（本地 JSON）。

<!-- node:app.file_watcher status:unvisited -->
### 文件监听器
```python
# 使用 notify crate + notify-debouncer-mini
# start_watching(path): 开始监听目录变更（文件新增/修改/删除）
# stop_watching():      停止监听
# 事件 → emit 到前端 → 自动刷新文件树和编辑器内容
# SkillWatcherState: 独立监听技能目录变更 → 热重载 skill
```
<!-- /node -->

<!-- node:app.i18n status:unvisited -->
### 国际化
```python
# src/lib/i18n/ 目录
# 提供 t() / tt() 函数
# 支持中文和英文界面
# 在 UI 组件中通过 t('key') 或 tt('key', {count}) 获取翻译文本
```
<!-- /node -->

<!-- node:agent.sse status:unvisited -->
### SSE 流式事件
```python
# SSE 端点模块
# 当以浏览器模式运行时（无 Tauri），通过 HTTP SSE 提供 Agent 流式输出
# 使用 postMessage + EventSource 在 webview 中传输流式 token
```
<!-- /node -->
