# zcode Agent 系统文档

> AI 编程代理（Agent Pipeline），v0.3 从 pi-agent-rust（commit e7792d64）移植，v0.6 增加上下文压缩和卡死循环检测。

---

## 架构总览

```
用户输入
  │
  ▼
agent_command.rs  ←── 会话管理、多轮编排、危险工具守卫、auto-approve
  │
  ▼
agent.rs          ←── 主循环：输入 → Provider 调用 → 工具执行 → 循环
  │
  ├── provider.rs       ← LLM 抽象层（trait Provider）
  ├── providers/        ← Anthropic / OpenAI 实现
  ├── tools/            ← 7 个工具（read/bash/edit/write/grep/find/ls）
  ├── skills.rs         ← 技能加载注入
  ├── compaction.rs     ← 上下文压缩（v0.6）
  ├── runtime_env.rs    ← 内置运行时管理（Python + uv + Bun）
  ├── model.rs          ← 共享类型
  ├── sse.rs            ← SSE 流解析
  └── error.rs          ← 统一错误类型
```

---

## 模块详情

### `agent_command.rs` — 会话管理

**命令**：
- `start_agent_turn` — 启动 agent 会话，编排多轮工具调用
- `approve_tool_call` — 批准危险工具执行（oneshot 通道）

**关键设计**：
- **oneshot 审批通道**：危险工具（bash、文件外写操作）等待前端批准，不阻塞 agent 循环
- **Session scoped events**：每个会话独立事件流，前端实时接收工具调用/结果/流 token
- **自动批准**（v0.5）：write/edit 操作目标为当前编辑器打开文件时跳过确认
- **CWD 智能推导**（v0.5）：优先当前文件目录 → 回退钉选文件夹
- **内置运行时初始化**：`start_agent_turn` 中调用 `ensure_agent_venv` 初始化捆绑的 Python + uv + Bun 运行时，结果缓存于 `RuntimeState`。若运行时未下载或初始化失败，返回友好错误并提示手动运行 fetch 脚本
- **工具注册表增强**：`build_guarded_registry` 接受 `augmented_path` 和 `venv_dir`，将注入了内置运行时的 PATH 传递给 `BashTool`
- **事件类型**：Token、ToolCall、ToolResult、ApprovalRequired、Error、Done、CompactionStarted、CompactionFinished

---

### `agent.rs` — Agent 主循环

```
loop:
  1. 构建消息（system prompt + 技能 + 历史）
  2. 调用 Provider 流式请求
  3. 解析响应 → 文本内容 / 工具调用
  4. 工具调用 → 检查批准 → 执行 → 收集结果
  5. 结果注入到消息历史 → 继续循环
  6. 终止条件：
     - 模型返回纯文本（无工具调用）
     - 达到最大轮数
     - 卡死循环检测触发
     - 用户中断
```

**保护机制**（v0.6）：
- **卡死循环检测**：同一工具+相同参数连续 3 次 → 自动停止
- **40 轮软提示**：工具迭代达 40 轮时注入 system_note 提示收尾
- **图片年龄化（image age-out）**：工具返回的图片 base64 数据保存在真实消息历史中供下一轮发送给 Provider，发送后立即替换为 `[已读取图片]` 文本占位避免后续轮次重复传输 base64。通过 `pending_image_indices`（`Vec<usize>`）追踪哪些消息索引仍包含真实 Image 块。该追踪器在 compaction 时自动批量年龄化所有待处理图片后清空（compaction 会重排序消息，索引失效）
- **图片年龄化重试**：若 `provider.stream()` 失败且存在 `pending_image_indices`，自动年龄化这些图片后重试一次（某些 Provider 不接受 tool_result 中的 image 块）

---

### `compaction.rs` — 上下文压缩（v0.6）

**触发条件**：token 预估超有效窗口 85% 时触发

**流程**：
1. Token 预估（字符数/4 估算）
2. 切点查找（找到可摘要的消息范围）
3. LLM 摘要生成（迭代更新已有摘要）
4. 替换旧消息为结构化摘要块
5. 清理消息历史

**摘要格式**：
```
## Summary of earlier conversation
- Goal: ...
- Progress: ...
- Decisions: ...
- Next Steps: ...
- Critical Context: ...
```

**五重保护**：
1. 触发阈值：85% 有效窗口
2. 冷却轮次：5 轮间隔
3. 最大连续失败：3 次 → kill-switch
4. 摘要大小上限：15% 有效窗口
5. 摘要压缩降级：摘要本身过大时二次压缩

---

### `provider.rs` — LLM 抽象层

```rust
pub trait Provider {
    async fn stream(&self, ctx: Context, opts: StreamOptions) -> Result<Stream>;
}

pub struct Context {
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub tools: Vec<ToolDef>,
}

pub struct StreamOptions {
    pub model: String,
    pub max_tokens: Option<u32>,
}
```

---

### `providers/anthropic.rs` — Anthropic Messages API

- 原生 Anthropic Messages API
- 支持 extended thinking
- 工具调用通过 tool_use content block
- **ToolResult 合并**：同一 `tool_use_id` 的所有 content block 合并到单个 `ToolResult`（不再为每个 block 生成独立 ToolResult），`AnthropicToolResultContent` 支持 `Text` 和 `Image` 变体，通过 `block_to_tool_result_content` 辅助函数转换

### `providers/openai.rs` — OpenAI Chat Completions API

- 兼容 20+ 提供商：Groq、DeepSeek、OpenRouter、Together、Fireworks 等
- 工具调用通过 function_call

### `providers/mod.rs` — 自动分发

- `build_provider(base_url)` — 检测 base_url 中 "anthropic" 关键字（大小写不敏感）
- URL 规范化：自动补全 `/v1/messages`（Anthropic）或 `/v1/chat/completions`（OpenAI）
- 默认：不含 "anthropic" → OpenAI 协议

---

### `skills.rs` — 技能系统

**发现路径**：
- 项目级：`.zcode/skills/*/SKILL.md`
- 用户级：`~/.config/zcode/skills/*/SKILL.md`

**格式**：YAML frontmatter + Markdown 正文

```yaml
---
name: skill-name
description: skill description
---
skill instructions here...
```

**注入方式**：XML 格式化后注入 system prompt：

```xml
<skill>
  <name>skill-name</name>
  <description>skill description</description>
  <instructions>skill instructions here...</instructions>
</skill>
```

**UI 技能开关**（SettingsDialog → Skills tab）：
- Summarize
- Fix Grammar
- TOC
- Explain Code

---

## 内置运行时系统（runtime_env.rs）

`runtime_env` 模块管理捆绑在应用中的可移植 Python + uv + Bun 运行时。

**核心类型**：
- `AgentRuntime` — 持有的运行时路径：`venv_dir`（Python venv 目录）和 `bun_bin_dir`（Bun 可执行文件目录）
- `RuntimeState` — Tauri managed state，缓存 `Option<AgentRuntime>`，首次使用时惰性初始化

**关键函数**：
- `ensure_agent_venv(app)` — 幂等创建 agent venv：使用嵌入的 `uv` 和 `python-build-standalone` 解释器通过 `uv venv --python <embedded>` 创建隔离虚拟环境（不用 `python -m venv`），120 秒超时保护
- `augmented_path(runtime)` — 生成注入了 venv `bin/`（或 `Scripts/` on Windows）和 bundled `bin/` 目录的 PATH 字符串
- `embedded_python(app)` / `embedded_uv(app)` / `embedded_bun_dir(app)` — 通过 `AppHandle::path().resolve()` 定位 `resources/runtime/` 下的嵌入二进制文件

**运行时不存在的处理**：
- 若 `resources/runtime/` 下找不到二进制文件，返回中文错误消息提示用户运行 `scripts/fetch-runtime/` 脚本
- `start_agent_turn` 捕获此错误并向前端返回友好提示

**fetch 脚本**：
- `scripts/ensure-runtime.cjs` — Node.js 入口，在 `tauri dev`/`build` 前自动执行（由 `tauri.conf.json` 的 `beforeDevCommand`/`beforeBuildCommand` 触发）
- `scripts/fetch-runtime/fetch-macos.sh` — macOS aarch64 下载脚本
- `scripts/fetch-runtime/fetch-linux.sh` — Linux x86_64 下载脚本
- `scripts/fetch-runtime/fetch-windows.ps1` — Windows x86_64 下载脚本
- 均幂等：已存在则跳过；支持 `GITHUB_TOKEN` 环境变量避免 GitHub API 限速

## 工具系统

### Tool trait

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;  // JSON Schema
    fn is_dangerous(&self) -> bool;
    async fn execute(&self, input: Value) -> Result<ToolResult>;
}
```

### 工具注册表（7 个）

| 工具 | 文件 | 危险 | 说明 |
|---|---|---|---|
| **read** | `read.rs` | ❌ | 文件读取：offset/limit、图片（png/jpg/gif/webp/bmp）、截断 2000 行/50KB |
| **bash** | `bash.rs` | ✅ | Shell 执行：120s 超时、输出截断 2000 行/50KB、保存到临时文件。当内置运行时可用时，注入增强 PATH（含 Python venv 和 Bun 目录）和 `VIRTUAL_ENV` 环境变量 |
| **edit** | `edit.rs` | ❌ | 精确文本替换：多编辑批量、oldText 唯一性校验、边界检查 |
| **write** | `write.rs` | ❌ | 文件创建/覆盖：自动建父目录、路径限制 100MB |
| **grep** | `grep.rs` | ❌ | ripgrep 文本搜索（需系统装 `rg`） |
| **find** | `find.rs` | ❌ | fd-find 文件搜索（需系统装 `fd`） |
| **ls** | `ls.rs` | ❌ | 目录列表：截断 500 条目、扫描上限 20000 |

### 路径安全

- `enforce_cwd_scope` — 所有文件操作强制限制在 CWD 内
- `resolve_path` — 相对路径规范化
- `canonicalize_safe` — 防止符号链接逃逸

### 工具输出策略

- 最大 500 行 / 100KB（v0.6 从 2000 行/1MB 收紧）
- Head+tail 截断：保留头部上下文 + 尾部错误信息
- 截断标记注明省略字节/行数
- UTF-8 安全截断：`truncate_at_char_boundary()`（v0.5）

---

## 事件流（前端接收）

| 事件 | 携带数据 | 说明 |
|---|---|---|
| `Token` | delta_text | LLM 流式输出 token |
| `ToolCall` | tool_name, input | 模型请求执行工具 |
| `ToolResult` | tool_name, output, is_error | 工具执行结果 |
| `ApprovalRequired` | tool_name, input, reason | 危险工具需用户批准 |
| `Error` | message | 错误信息 |
| `CompactionStarted` | — | 上下文压缩开始（v0.6） |
| `CompactionFinished` | summary_length | 上下文压缩完成（v0.6） |
| `Done` | usage | Agent 轮次结束（含 token 用量） |

---

## 前端 Agent UI

### `src/lib/tauri/ai.ts` — AI 前端接口

```typescript
// API Key 操作
saveApiKey(apiKey: string)        // 调用 Rust save_api_key 存入 keychain（空串=删除）
maskApiKey(key: string): string   // 纯前端脱敏：前3字符 + *** + 后4字符（≤7 字符返回 "***"）

// AI 调用
callAIProvider(baseUrl, model, prompt, providerName?): Promise<string>
  // 调用 Rust call_ai_provider，后端从 keychain 读取 key 并发起流式调用

// Agent 会话
startAgentTurn(args: StartAgentTurnArgs): Promise<void>
  // 启动 agent 多轮对话，v0.6 新增 contextWindowTokens 字段显式传递窗口大小
```

### +page.svelte — Agent 面板集成

- `agentPanelOpen` 状态 — 控制 agent 面板开关
- 标题栏按钮切换 agent 面板
- `$effect` 监听 docStore.filePath → 同步当前文件路径到 agent 上下文

### 事件监听（前端接收后端流事件）

前端通过 Tauri event system 监听 agent 会话事件：

| 事件 | 前端处理 |
|---|---|
| `Token` | 追加 delta 文本到聊天显示 |
| `ToolCall` | 显示工具调用卡片（工具名 + 参数） |
| `ToolResult` | 显示工具结果（折叠，可展开） |
| `ApprovalRequired` | 弹出确认对话框（危险工具拦截） |
| `CompactionStarted` | 显示压缩进度指示 |
| `CompactionFinished` | 显示压缩完成提示 |
| `Error` | 错误提示 |
| `Done` | 显示本轮 token 用量 |

---

## 设置界面（SettingsDialog）

### AI Provider Tab

- **Base URL**：OpenAI/Anthropic 兼容端点，自动检测协议 + 补全路径后缀
- **Model**：模型名（如 `deepseek-chat`、`claude-sonnet-4-20250514`）
- **API Key**：
  - 真实值存入 OS keychain（通过 `keyring` crate）
  - 本地 store 仅存脱敏版（如 `sk-5d70d***5c60`）
  - 眼睛图标切换密码/明文
  - keychain 不可用时显示警告横幅但不阻塞保存
- 保存/取消按钮，保存失败有错误提示

### Skills Tab

4 个预置技能开关（Summarize / FixGrammar 默认开启，TOC / ExplainCode 默认关闭）+ "Add custom skill" 预留按钮

### 持久化

- 非敏感数据（baseUrl / model / maskedApiKey）→ `zcode-settings.json`（通过 `settings.ts` store + `@tauri-apps/plugin-store`）
- 真实 API Key → Rust `keyring` crate → OS keychain
- `sharedStore.ts` 单例共享 store 实例

---

## 安全存储（settings.rs）

- OS keychain 后端：macOS Keychain / Windows Credential Manager / Linux secret-service
- 条件编译按平台选 native feature
- Best-effort 策略：keychain 不可用返回警告不阻塞
- 启动时自动迁移：扫描 config + data 目录迁移旧版明文 `apiKey`
- 空字符串 = 删除 keychain 条目

---

## 测试

| 测试 | 说明 |
|---|---|
| `agent_e2e.rs` | Agent 端到端（含工具调用循环，需 API key） |
| `agent_mock.rs` | Agent pipeline mock（无外部依赖，v0.4） |
| `provider_smoke.rs` | 两个 Provider 流式调用冒烟 |
| `settings_keychain.rs` | keychain 存储+迁移+mask 边界（v0.4） |
| `skill_e2e.rs` | 技能注入+模型识别端到端（需 API key） |
| `tool_smoke.rs` | 所有 7 个工具单元测试 |

---

## 依赖（Cargo.toml agent 相关）

```toml
async-trait = "0.1"
reqwest = { version = "0.12", default-features = false, features = ["stream", "rustls-tls", "json"] }
futures = "0.3"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time", "process", "fs"] }
base64 = "0.22"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"

[target.'cfg(target_os = "macos")'.dependencies]
keyring = { version = "3", features = ["apple-native"] }
[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3", features = ["windows-native"] }
[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "3", features = ["linux-native"] }
```

---

## 设计原则

- 纯 tokio 异步，无 async supersync
- 无 TUI 依赖，专为 Tauri 桌面应用
- 工作目录安全强制限制
- 工具输出截断防爆上下文
- 上下文自动压缩防 token 爆炸
- 卡死循环自动检测
- 图片 base64 不进消息历史
