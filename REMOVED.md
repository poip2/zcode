# zcode 删除清单 — mdhero → zcode 精简对照

> 从 mdhero（完整版 Markdown 编辑器）精简为 zcode（最小可读写 MD 编辑器）的过程中，所有被删除的代码、文件、功能和配置。

---

## 一、前端组件（删除 21 个）

| 组件文件 | 功能 | 删除原因 |
|---|---|---|
| `src/lib/components/Toolbar.svelte` | 顶部工具栏（打开/粘贴/URL/编辑切换/保存/设置按钮） | 不需要 UI 工具栏，改用键盘快捷键 |
| `src/lib/components/TabBar.svelte` | 多标签页切换栏 | 去掉多 Tab 系统，单文件即可 |
| `src/lib/components/StatusBar.svelte` | 底部状态栏（文件名/进度/编辑状态） | 合并到 `+page.svelte` 的简化底部栏 |
| `src/lib/components/FrontmatterBar.svelte` | YAML frontmatter 元数据显示栏 | 不需要 |
| `src/lib/components/TableOfContents.svelte` | 右侧目录导航（TOC） | 不需要 |
| `src/lib/components/SearchOverlay.svelte` | 全文搜索覆盖层（/ 键触发） | 不需要 |
| `src/lib/components/DropZone.svelte` | 拖放文件区域覆盖层 | 合并到 `+page.svelte` 的 drop handler |
| `src/lib/components/EmptyState.svelte` | 空状态引导页 | 合并到 `+page.svelte` |
| `src/lib/components/OpenDialog.svelte` | 最近文件/文件夹浏览对话框 | 不需要，直接用系统文件对话框 |
| `src/lib/components/PasteModal.svelte` | 粘贴 Markdown / URL 导入弹窗 | 不需要 |
| `src/lib/components/SettingsDialog.svelte` | 设置面板（字体/字号/行高/主题/AI配置等） | 不需要，使用硬编码默认值 |
| `src/lib/components/AboutDialog.svelte` | 关于对话框 | 不需要 |
| `src/lib/components/CustomPromptModal.svelte` | AI 自定义提示词弹窗 | 不需要 AI 功能 |
| `src/lib/components/AILookupSettings.svelte` | AI 服务配置界面 | 不需要 AI 功能 |
| `src/lib/components/ImageLightbox.svelte` | 图片点击放大灯箱 | 不需要 |
| `src/lib/components/ProgressBar.svelte` | 页面顶部阅读进度条 | 不需要 |
| `src/lib/components/ScrollToTop.svelte` | 回到顶部浮动按钮 | 不需要 |
| `src/lib/components/Toast.svelte` | 全局 Toast 通知 | 不需要，状态内联显示 |
| `src/lib/components/UpdateToast.svelte` | 应用更新通知 | 不需要自动更新 |
| `src/lib/components/UpdateBanner.svelte` | 更新横幅提示 | 不需要自动更新 |
| `src/lib/components/ReaderControls.svelte` | 阅读模式控制（字号/主题切换） | 不需要 |

---

## 二、前端 Stores（删除 11 个）

| Store 文件 | 用途 | 删除原因 |
|---|---|---|
| `src/lib/stores/tabs.ts` | 多标签页管理（打开/关闭/切换/编辑状态/dirty 跟踪） | 单文件不需要 Tab 系统 |
| `src/lib/stores/settings.ts` | 用户设置（字体/字号/行高/主题/内容宽度等） | 硬编码默认值 |
| `src/lib/stores/theme.ts` | 主题切换（亮色/暗色/跟随系统） | 不需要主题切换 |
| `src/lib/stores/toc.ts` | 目录条目和激活标题追踪 | 不需要 TOC |
| `src/lib/stores/recents.ts` | 最近打开文件列表 | 不需要 |
| `src/lib/stores/pinned.ts` | 固定/收藏文件 | 不需要 |
| `src/lib/stores/aiLookup.ts` | AI 查词配置（Claude/ChatGPT/Perplexity/Wikipedia） | 不需要 AI 功能 |
| `src/lib/stores/readingProgress.ts` | 阅读进度保存和恢复 | 不需要 |
| `src/lib/stores/updater.ts` | 应用更新检查状态 | 不需要自动更新 |
| `src/lib/stores/autoUpdate.ts` | 自动更新开关 | 不需要自动更新 |
| `src/lib/stores/toast.ts` | Toast 通知队列 | 不需要 |

---

## 三、前端 Utils（删除 4 个）

| 工具文件 | 用途 | 删除原因 |
|---|---|---|
| `src/lib/utils/clipboard.ts` | 剪贴板操作辅助 | 不需要 |
| `src/lib/utils/scroll-sync.ts` | 编辑/预览/源码三模式滚动同步 | 不需要滚动同步 |
| `src/lib/utils/llm.ts` | LLM URL 模板拼接 | 不需要 AI 功能 |
| `src/lib/utils/url.ts` | URL 解析辅助 | 不需要 |

---

## 四、前端静态资源（删除 6 个）

| 文件 | 用途 |
|---|---|
| `src/lib/assets/mdhero-icon.png` | 应用图标 |
| `src/lib/assets/favicons/chatgpt.webp` | ChatGPT 图标 |
| `src/lib/assets/favicons/claude.png` | Claude 图标 |
| `src/lib/assets/favicons/perplexity.png` | Perplexity 图标 |
| `src/lib/assets/favicons/wikipedia.ico` | Wikipedia 图标 |
| `src/lib/assets/favicons/google.ico` | Google 图标 |

---

## 五、Tauri Rust 后端

### 删除的文件（2 个）

| 文件 | 用途 | 删除原因 |
|---|---|---|
| `src-tauri/src/menu.rs` | 原生菜单构建（App/File/Edit/View/Window 菜单 + AI 右键菜单） | 不需要原生菜单 |
| `src-tauri/src/watcher.rs` | 文件系统监听（文件外部变更时自动重载） | 不需要文件监听 |
| `src-tauri/tests/menu_window.rs` | 菜单单元测试 | 随 menu.rs 删除 |
| `src-tauri/tauri.release.conf.json` | 发布配置（含 updater 公钥和端点） | 不需要自动更新 |
| `src-tauri/capabilities/desktop.json` | 桌面端额外权限配置 | 不需要 process/window-state 权限 |
| `src-tauri/Info.plist` | macOS 特定配置 | 不需要 |

### 删除的 Rust 命令（8 个）

| 命令 | 文件 | 用途 |
|---|---|---|
| `start_watching` | `watcher.rs` | 启动文件监听 |
| `stop_watching` | `watcher.rs` | 停止文件监听 |
| `get_opened_files` | `lib.rs` | 获取 OS "Open With" 事件缓冲的文件列表 |
| `quit_app` | `commands.rs` | 显式退出应用（配合 Escape 关闭最后标签页） |
| `show_ai_context_menu` | `commands.rs` | 显示 AI 查词右键菜单（~120 行） |
| `list_claude_plans` | `commands.rs` | 列出 ~/.claude/plans 下的 Markdown 文件 |
| `list_folder_md_files` | `commands.rs` | 递归扫描目录中的 Markdown 文件（含深度限制） |
| `path_exists` | `commands.rs` | 检查文件路径是否存在 |

### 删除的 Rust 结构体/类型

```rust
// commands.rs 中删除
struct PlanFile { name, path, modified }
struct MdFile { name, path, rel_path, modified }
struct AIPrompt { id, name }
struct AIProvider { id, name, prompts }

// lib.rs 中删除
struct OpenedFiles { paths: Mutex<Vec<String>> }  // "Open With" 缓冲
// WatcherState 管理（watcher.rs）
```

### 删除的 Tauri 插件（3 个）

| 插件 | Crate | 用途 |
|---|---|---|
| `tauri-plugin-updater` | `tauri-plugin-updater = "2"` | 应用自动更新 |
| `tauri-plugin-process` | `tauri-plugin-process = "2"` | 进程管理 |
| `tauri-plugin-window-state` | `tauri-plugin-window-state = "2"` | 窗口位置/大小记忆 |
| `tauri-plugin-cli` | `tauri-plugin-cli = "2"` | CLI 参数解析（`mdhero file.md`） |

### 删除的 Rust 依赖

```toml
# Cargo.toml 中删除
notify = { version = "7", features = ["macos_fsevent"] }
notify-debouncer-mini = "0.5"
tauri-plugin-process = "2"
tauri-plugin-window-state = "2"
tauri-plugin-cli = "2"
tauri-plugin-updater = "2"  # (条件编译 target)
```

---

## 六、前端 NPM 依赖（删除 3 个）

| 包名 | 用途 |
|---|---|
| `mermaid` | Mermaid 图表渲染 |
| `mark.js` | 文本高亮（搜索高亮） |
| `@lucide/svelte` | 图标库 |

---

## 七、主页面 `+page.svelte` 中删除的逻辑

### 删除的状态变量（8 个）
```
lastWatchedPath, searchVisible, pasteVisible, pasteDefaultMode,
openVisible, settingsVisible, aboutVisible, customPromptVisible,
customPromptSelection, zenMode, rawMode, contentMaxWidth,
lightboxVisible, lightboxImages, lightboxIndex
```

### 删除的 $effect 副作用（3 个）
```
- Tab 切换同步（prevTabId 追踪 + 滚动位置保存恢复）
- 文件路径变化监听（startFileWatcher）
- 主题/设置/字体联动（contentMaxWidth $derived）
```

### 删除的事件处理函数（20+ 个）
```
handleScrollForProgress()     - 阅读进度滚动保存
saveProgressNow()             - 强制保存阅读进度
restoreProgress(filePath)     - 恢复阅读进度
handleLocalLink(href)         - 本地文件链接处理器
handleEditToggle()            - 编辑切换（简化版用 toggleEdit）
handleRawToggle()             - Raw 源码模式切换
handleCloseTab(id)            - 关闭标签页
switchMode(target)            - 三模式切换（编辑/预览/Raw）
startScroll/stopScroll()      - Vim j/k 连续滚动
jumpToHeading(direction)      - 跳转标题
handleVisibilityChange()      - 页面可见性变化
handleKeyup()                 - Vim 键释放

// window.__mdhero_* 暴露的函数（7 个）
__mdhero_open_file, __mdhero_open_path, __mdhero_paste,
__mdhero_toggle_theme, __mdhero_find, __mdhero_zen,
__mdhero_about, __mdhero_check_updates, __mdhero_ai_lookup
```

### 删除的键盘快捷键（12 个）

| 快捷键 | 功能 |
|---|---|
| `⌘1-9` | 切换标签页 |
| `⌘+/=` | 放大字号 |
| `⌘-` | 缩小字号 |
| `⌘0` | 重置字号 |
| `⌘U` | Raw 源码模式 |
| `⌘T` | 新标签页 |
| `⌘,` | 打开设置 |
| `⌘W` | 关闭标签页 |
| `⌘⇧F` | Zen Mode |
| `j` / `k` | Vim 滚动 |
| `d` / `u` | 半页滚动 |
| `gg` / `G` / `]` / `[` / `/` / `n` | Vim 导航 |

### 删除的 UI 区块（HTML 模板中）
```
- <ProgressBar />                  进度条
- <Toolbar />                      工具栏
- <TabBar />                       标签栏
- <DropZone />                     拖放区
- <TableOfContents />              TOC 侧栏
- <SearchOverlay />                搜索覆盖层
- <PasteModal />                   粘贴弹窗
- <OpenDialog />                   打开对话框
- <SettingsDialog />               设置对话框
- <AboutDialog />                  关于对话框
- <CustomPromptModal />            自定义提示词弹窗
- <FrontmatterBar />               Frontmatter 栏
- <StatusBar />                    状态栏
- <ScrollToTop />                  回到顶部
- <ImageLightbox />                图片灯箱
- <UpdateToast />                  更新通知
- <Toast />                        Toast 通知
```

---

## 八、MarkdownRenderer 组件中删除的逻辑

| 删除的功能 | 说明 |
|---|---|
| Mermaid 图表渲染 | `initMermaid()`, `renderMermaidBlocks()`, `mermaid` 依赖 |
| TOC 提取与观察器 | `extractToc()`, `setupTocObserver()`, IntersectionObserver |
| AI 右键菜单 | `handleContextMenu()`, `show_ai_context_menu` 调用 |
| 链接 tooltip | `addLinkTooltips()`, `tooltipEl` 元素 |
| 外部链接处理器 | `addLinkHandlers()`, `isUrlHref()` |
| 图片 Lightbox 点击 | `addImageClickHandlers()`, `onImageClick` prop |
| 本地文件链接 | `onLocalLink` prop |
| Settings store 依赖 | `fontSize` / `lineHeight` / `fontFamily` / `contentMaxWidth` 动态绑定 → 改为硬编码 |
| Dark mode 样式 | 所有 `:global(html.dark)` 覆盖样式 |

---

## 九、总量统计

| 类别 | mdhero | zcode | 精简率 |
|---|---|---|---|
| 前端组件文件 | 22 个 | 2 个 | 91% |
| Stores | 12 个 | 1 个 | 92% |
| Utils | 4 个 | 0 个 | 100% |
| Rust 源文件 | 4 个 | 2 个 | 50% |
| Rust 命令 | 12 个 | 4 个 | 67% |
| Tauri 插件 | 6 个 | 2 个 | 67% |
| NPM 核心依赖 | 12 个 | 9 个 | 25% |
| `+page.svelte` | ~700 行 | ~220 行 | 69% |
| **前端源文件总数** | **~55 个** | **~10 个** | **82%** |

---

---

# 保留清单 — zcode 保留了什么

> 从 mdhero 中保留、精简、复用的所有代码和功能。

---

## 十、前端源文件（保留 8 个）

### 10.1 `src/lib/renderer/pipeline.ts` — 渲染管线（原样复用）

**来源**：`mdhero/src/lib/renderer/pipeline.ts`，完全照搬，未做任何修改。

**功能**：
- **Markdown 解析**：markdown-it，支持 GFM、链接自动识别、排版优化
- **代码高亮**：highlight.js，注册了 24 种语言（js/ts/py/rs/go/sh/json/yaml/xml/css/sql/md/java/c/cpp/diff/dockerfile/toml/ini/swift/kotlin/ruby/php/jsx/tsx）
- **数学公式**：KaTeX，通过 markdown-it-texmath，支持 `$...$` 和 `$$...$$` 分隔符
- **安全过滤**：DOMPurify，防止 XSS，允许 KaTeX 的 SVG/MathML 标签和属性
- **Frontmatter 解析**：提取 YAML 头部元数据（`---...---` 块）
- **任务列表**：markdown-it-task-lists，渲染 `- [ ]` / `- [x]` 复选框
- **标题锚点**：markdown-it-anchor，自动生成标题 id
- **本地图片解析**：将相对路径/绝对路径图片转为 Tauri asset protocol URL
- **源码行号标注**：`data-source-line` 属性注入（用于未来的滚动同步）

**导出接口**：
```typescript
initRenderer()                       // 初始化渲染器
render(markdown, baseDir?) → string  // 渲染为 HTML
renderFull(markdown, baseDir?) → RenderResult  // 渲染+frontmatter+字数+图片路径
resolveLocalPath(src, baseDir) → string  // 解析本地路径
isInitialized() → boolean            // 是否已初始化
```

**依赖**：`markdown-it`, `highlight.js`, `katex`, `dompurify`, `markdown-it-task-lists`, `markdown-it-anchor`, `markdown-it-texmath`

---

### 10.2 `src/lib/components/Editor.svelte` — 编辑器（轻度精简）

**来源**：`mdhero/src/lib/components/Editor.svelte`

**保留的功能**：
- 全屏等宽字体 textarea
- Tab 键插入 2 空格缩进（保留光标位置）
- `localValue` + `$effect` 模式：防止父组件更新导致光标跳动（仅当 textarea 未聚焦时同步）
- 自动聚焦

**删除的 props**（相比 mdhero）：
- ~~`fontSize`~~ → 硬编码 `14px`
- ~~`lineHeight`~~ → 硬编码 `1.6`
- ~~`maxWidth`~~ → 改为 `max-width: 900px`（自适应窗口）

**Props 接口**：
```typescript
{ value: string; onChange: (newValue: string) => void }
```

---

### 10.3 `src/lib/components/MarkdownRenderer.svelte` — 渲染器（大幅精简）

**来源**：`mdhero/src/lib/components/MarkdownRenderer.svelte`

**保留的功能**：
- `{@html html}` 渲染 sanitized HTML
- Tailwind Typography (`prose prose-slate`) 排版
- **代码块复制按钮**：hover 显示 "Copy" 按钮，点击复制到剪贴板
- KaTeX 公式样式、表格、引用块、任务列表、链接样式
- 自定义滚动条

**删除的功能**（详见第八节）：
- Mermaid 图表渲染 → 只保留代码块展示
- TOC 提取与 IntersectionObserver → 去除
- AI 右键菜单 → 去除
- 链接 tooltip → 去除
- 外部链接处理器 → 去除
- 图片 Lightbox → 去除
- Dark mode 样式 → 去除
- Settings store 动态绑定 → 硬编码

**Props 接口**：
```typescript
{ html: string }
```

---

### 10.4 `src/lib/stores/document.ts` — 文档状态（精简）

**来源**：`mdhero/src/lib/stores/document.ts`

**保留的功能**：
- 单文档 Svelte writable store
- `DocumentState` 接口：filePath, fileName, content, renderedHtml, frontmatter, wordCount, loading, error

**删除的功能**：
- Tab 系统相关的所有逻辑（tabs 管理、activeTabId、addTab/closeTab/switchTab 等）

---

### 10.5 `src/lib/tauri/files.ts` — 文件操作（精简）

**来源**：`mdhero/src/lib/tauri/files.ts`

**保留的函数**：
| 函数 | 功能 |
|---|---|
| `readMarkdownFile(path)` | 调用 Rust 读取文件内容 |
| `saveFile(path, content)` | 调用 Rust 写入文件内容 |
| `resolvePath(path)` | 调用 Rust 解析为绝对路径 |
| `getBaseDir(path)` | 提取文件所在目录（纯 JS） |
| `loadFile(path)` | 一站式加载：resolve → read → render → allowAssets → 更新 store |
| `openFileDialog()` | 打开系统文件对话框，返回选中路径 |
| `allowAssets(paths)` | 白名单本地图片路径到 Tauri asset protocol |

**删除的函数**（相比 mdhero）：
- ~~`reloadCurrentFile()`~~ — 文件热重载（无 watcher 不需要）
- ~~`pathExists()`~~ — 文件存在检查（合并到 loadFile 错误处理）
- ~~`openWithSystem()`~~ — OS 默认应用打开（不需要）

---

### 10.6 `src/routes/+page.svelte` — 主页面（重写）

**来源**：完全重写，仅保留核心交互模式

**功能**：
- **空状态**：显示图标 + "Open a Markdown file" + ⌘O 提示 + 按钮
- **打开文件**：⌘O 快捷键 / 点击按钮 → 系统文件对话框 → 加载并渲染
- **拖放支持**：拖 .md 文件到窗口即可打开
- **编辑模式**：⌘E 切换 → 显示 Editor 组件（全屏 textarea）
- **预览模式**：显示 MarkdownRenderer 组件
- **保存**：⌘S → 写入磁盘 → 重新渲染 → 自动回到预览模式
- **底部状态栏**：显示文件名、dirty 状态、当前模式、快捷键提示
- **错误处理**：文件不存在/读取失败时显示错误信息 + 重试按钮
- **dirty 追踪**：编辑内容与磁盘内容不同时标记 "(unsaved)"
- **自适应宽度**：内容最大 900px，窗口缩小时自动收窄

**状态变量（仅 5 个）**：
```
rendererReady, isEditing, editContent, dirty, statusMessage
```

**键盘快捷键（仅 3 个）**：

| 快捷键 | 功能 |
|---|---|
| `⌘O` | 打开文件 |
| `⌘E` | 切换编辑/预览 |
| `⌘S` | 保存并回到预览 |

---

### 10.7 `src/app.css` — 全局样式（精简）

**保留**：
- Tailwind CSS v4 入口 (`@import "tailwindcss"`)
- KaTeX 样式 (`katex/dist/katex.min.css`)
- highlight.js GitHub 主题 (`highlight.js/styles/github.min.css`)
- Tailwind Typography 插件
- 基础排版变量和滚动条样式

**删除**：
- Dark mode 变量 (`@custom-variant dark`)
- 打印样式 (`@media print`)
- 深色滚动条样式

---

### 10.8 `src/routes/+layout.svelte` — 布局（极简）

仅 3 行：导入全局 CSS + `<slot />`，无任何额外逻辑。

---

### 10.9 `src/app.html` — HTML 模板（微调）

标题从 `MDHero` 改为 `zcode`，其余不变。

---

### 10.10 `src/app.d.ts` — 类型声明（新增）

为 `markdown-it-task-lists` 和 `markdown-it-texmath` 提供 TypeScript 类型声明（这两个包没有自带类型）。

---

## 十一、Tauri Rust 后端（保留 2 个源文件）

### 11.1 `src-tauri/src/commands.rs` — 命令（重写，仅 4 个命令）

| 命令 | 功能 | 代码行数 |
|---|---|---|
| `read_markdown_file(path)` | 读取文件内容为 UTF-8 字符串 | ~10 行 |
| `write_markdown_file(path, content)` | 写入字符串到文件 | ~8 行 |
| `resolve_path(path)` | 将相对路径解析为绝对路径 | ~15 行 |
| `allow_assets(paths)` | 将图片路径加入 asset protocol 白名单 | ~8 行 |

**设计原则**：只做纯文件 I/O，不涉及任何业务逻辑。

---

### 11.2 `src-tauri/src/lib.rs` — 应用入口（重写）

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())   // 外部链接用系统浏览器打开
        .plugin(tauri_plugin_dialog::init())    // 系统文件对话框
        .invoke_handler(tauri::generate_handler![
            commands::read_markdown_file,
            commands::write_markdown_file,
            commands::resolve_path,
            commands::allow_assets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**删除的 setup 逻辑**（相比 mdhero）：
- ~~原生菜单创建~~ (`menu::create_menu`)
- ~~菜单事件处理~~ (`on_menu_event`)
- ~~"Open With" 文件缓冲~~ (`OpenedFiles` state)
- ~~macOS URL open 事件~~ (`RunEvent::Opened`)

---

### 11.3 `src-tauri/src/main.rs` — 二进制入口（原样复用）

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() { zcode_lib::run() }
```

---

## 十二、配置和工程文件（保留）

| 文件 | 处理方式 | 说明 |
|---|---|---|
| `package.json` | 精简 | 删除 mermaid/mark.js/@lucide-svelte 依赖 |
| `vite.config.js` | 原样复用 | 添加 Tailwind 插件，其余同 mdhero |
| `svelte.config.js` | 原样复用 | adapter-static + vitePreprocess |
| `tsconfig.json` | 原样复用 | 不变 |
| `Cargo.toml` | 精简 | 删除 notify/process/window-state/cli/updater 依赖 |
| `tauri.conf.json` | 精简 | 删除 updater/cli 插件配置，保留 assetProtocol 和文件关联 |
| `capabilities/default.json` | 精简 | 删除 window-state/event/webview/cli 权限，只保留 opener+dialog |

---

## 十三、保留的技术栈一览

| 层 | 技术 | 版本 |
|---|---|---|
| 前端框架 | SvelteKit + Svelte 5 | ^2.9 / ^5.0 |
| UI 样式 | Tailwind CSS v4 + Typography 插件 | ^4.3 |
| Markdown 解析 | markdown-it | ^14.3 |
| 代码高亮 | highlight.js | ^11.11 |
| 数学公式 | KaTeX | ^0.17 |
| XSS 防护 | DOMPurify | ^3.4 |
| 桌面框架 | Tauri v2 | ^2 |
| 系统对话框 | tauri-plugin-dialog | ^2.7 |
| 外部链接 | tauri-plugin-opener | ^2.5 |
| 构建工具 | Vite | ^6.4 |

---

## 十四、保留的 npm 依赖（9 个核心 + 10 个 dev）

### 运行时依赖
```json
{
  "@tauri-apps/api": "^2",
  "@tauri-apps/plugin-dialog": "^2",
  "@tauri-apps/plugin-opener": "^2",
  "dompurify": "^3",
  "highlight.js": "^11",
  "katex": "^0.17",
  "markdown-it": "^14",
  "markdown-it-anchor": "^9",
  "markdown-it-task-lists": "^2",
  "markdown-it-texmath": "^1"
}
```

### 开发依赖
```json
{
  "@sveltejs/adapter-static": "^3",
  "@sveltejs/kit": "^2",
  "@sveltejs/vite-plugin-svelte": "^5",
  "@tailwindcss/typography": "^0.5",
  "@tailwindcss/vite": "^4",
  "@tauri-apps/cli": "^2",
  "@types/markdown-it": "^14",
  "svelte": "^5",
  "svelte-check": "^4",
  "tailwindcss": "^4",
  "typescript": "~5.6",
  "vite": "^6"
}
```

---

## 十五、保留的 Rust 依赖

```toml
[dependencies]
tauri = { version = "2", features = ["protocol-asset"] }
tauri-plugin-opener = "2"
tauri-plugin-dialog = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

---

## 十六、最终文件结构

```
zcode/
├── package.json
├── pnpm-lock.yaml
├── vite.config.js
├── svelte.config.js
├── tsconfig.json
├── src/
│   ├── app.css                          # 全局样式
│   ├── app.d.ts                         # 类型声明（新增）
│   ├── app.html                         # HTML 模板
│   ├── routes/
│   │   ├── +layout.svelte               # 布局（极简）
│   │   ├── +layout.ts                   # SSR=off
│   │   └── +page.svelte                 # 主页面 ★ 核心
│   └── lib/
│       ├── components/
│       │   ├── Editor.svelte            # 编辑器
│       │   └── MarkdownRenderer.svelte  # 渲染器
│       ├── stores/
│       │   └── document.ts              # 文档状态
│       ├── renderer/
│       │   └── pipeline.ts             # 渲染管线
│       └── tauri/
│           └── files.ts                 # 文件操作
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   ├── icons/...
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       └── commands.rs
└── REMOVED.md                           # 本文档
```

**源文件总计：15 个**（不含配置和图标）
