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
| `src/lib/components/OpenDialog.svelte` | 最近文件/文件夹浏览对话框 | 不需要，其数据结构和交互思路被 Sidebar 吸收（常驻面板替代弹窗） |
| `src/lib/components/PasteModal.svelte` | 粘贴 Markdown / URL 导入弹窗 | 不需要 |
| `src/lib/components/SettingsDialog.svelte` | 设置面板（字体/字号/行高/主题/AI配置等） | ~~不需要~~ → v0.2 重新实现（精简版：仅 pin folder 设置） |
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
| `src/lib/stores/recents.ts` | 最近打开文件列表 | ~~不需要~~ → v0.2 重新实现（侧边栏 Recent 分组需要） |
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
| `src-tauri/src/watcher.rs` | 文件系统监听（文件外部变更时自动重载） | 不需要文件监听 → ★ v0.5 重新实现（含 500ms debounce、父目录监听存活原子写入、前后端抑制自身保存事件） |
| `src-tauri/tests/menu_window.rs` | 菜单单元测试 | 随 menu.rs 删除 |
| `src-tauri/tauri.release.conf.json` | 发布配置（含 updater 公钥和端点） | 不需要自动更新 |
| `src-tauri/capabilities/desktop.json` | 桌面端额外权限配置 | 不需要 process/window-state 权限 |
| `src-tauri/Info.plist` | macOS 特定配置 | 不需要 |

### 删除的 Rust 命令（8 个）

| 命令 | 文件 | 用途 |
|---|---|---|
| `start_watching` | `watcher.rs` | 启动文件监听 → ★ v0.5 重新实现 |
| `stop_watching` | `watcher.rs` | 停止文件监听 → ★ v0.5 重新实现 |
| `get_opened_files` | `lib.rs` | 获取 OS "Open With" 事件缓冲的文件列表 |
| `quit_app` | `commands.rs` | 显式退出应用（配合 Escape 关闭最后标签页） |
| `show_ai_context_menu` | `commands.rs` | 显示 AI 查词右键菜单（~120 行） |
| `list_claude_plans` | `commands.rs` | 列出 ~/.claude/plans 下的 Markdown 文件 |
| `list_folder_md_files` | `commands.rs` | 递归扫描目录中的 Markdown 文件（含深度限制） | ~~删除~~ → v0.2 重新实现为 `read_dir_tree`（树形结构替代扁平列表） |
| `path_exists` | `commands.rs` | 检查文件路径是否存在 | ~~删除~~ → v0.2 重新实现（侧边栏新建文件/文件夹前判断重名） |

### 删除的 Rust 结构体/类型

```rust
// commands.rs 中删除
struct PlanFile { name, path, modified }
struct MdFile { name, path, rel_path, modified }
struct AIPrompt { id, name }
struct AIProvider { id, name, prompts }

// lib.rs 中删除
struct OpenedFiles { paths: Mutex<Vec<String>> }  // "Open With" 缓冲
// WatcherState 管理（watcher.rs） → ★ v0.5 重新实现（新设计基于 notify + debouncer）
```

### 删除的 Tauri 插件（4 个）

| 插件 | Crate | 用途 |
|---|---|---|
| `tauri-plugin-updater` | `tauri-plugin-updater = "2"` | 应用自动更新 |
| `tauri-plugin-process` | `tauri-plugin-process = "2"` | 进程管理 |
| `tauri-plugin-window-state` | `tauri-plugin-window-state = "2"` | 窗口位置/大小记忆 |
| `tauri-plugin-cli` | `tauri-plugin-cli = "2"` | CLI 参数解析（`mdhero file.md`） |

### 删除的 Rust 依赖

```toml
# Cargo.toml 中删除
notify = { version = "7", features = ["macos_fsevent"] } → ★ v0.5 重新添加
notify-debouncer-mini = "0.5" → ★ v0.5 重新添加
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
lastWatchedPath → ★ v0.5 重新引入, searchVisible, pasteVisible, pasteDefaultMode,
openVisible, settingsVisible, aboutVisible, customPromptVisible,
customPromptSelection, zenMode, rawMode, contentMaxWidth,
lightboxVisible, lightboxImages, lightboxIndex
```

### 删除的 $effect 副作用（3 个）
```
- Tab 切换同步（prevTabId 追踪 + 滚动位置保存恢复）
- 文件路径变化监听（startFileWatcher） → ★ v0.5 重新引入（含前端 debounce 100ms + 自身保存抑制 1500ms）
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

| 类别 | mdhero | zcode v0.1 | zcode v0.4 | zcode v0.5 | 备注 |
|---|---|---|---|---|
| 前端组件文件 | 22 个 | 2 个 | 5 个 | Sidebar, TitleBar, SettingsDialog, Editor, MarkdownRenderer |
| Stores | 12 个 | 1 个 | 6 个 | +recents, folderTree, pinnedFolder, settings, sharedStore |
| Frontend utils | 4 个 | 0 个 | 2 个 | 3 个 | +files.ts, ai.ts, watcher.ts |
| Rust 源文件 | 4 个 | 2 个 | 21 个 | 23 个 | +agent, model, provider, skills, sse, error, settings, providers/, tools/, watcher, agent_command（agent pipeline + watcher） |
| Rust 命令 | 12 个 | 4 个 | 10 个 | 12 个 | +read_dir_tree, path_exists, create_markdown_file, create_folder, save_api_key, call_ai_provider, start_watching, stop_watching |
| Tauri 插件 | 6 个 | 2 个 | 3 个 | +tauri-plugin-store |
| Rust 测试文件 | 1 个 | 0 个 | 6 个 | +agent_e2e, agent_mock, provider_smoke, settings_keychain, skill_e2e, tool_smoke |
| NPM 核心依赖 | 12 个 | 9 个 | 11 个 | +@tauri-apps/plugin-store |
| `+page.svelte` | ~700 行 | ~220 行 | ~330 行 | ~370 行 | 标题栏+侧边栏+主内容布局 + file watcher 生命周期 |
| **源文件总计** | **~60 个** | **~12 个** | **~47 个** | **~50 个** | 前端 22 + Rust 23 + 测试 6 |

---

---

# 保留清单 — zcode 当前状态

> 从 mdhero 保留/精简/复用的代码，以及后续迭代新增的内容。

---

## 十、前端源文件（20 个，不含配置和图标）

### 10.1 `src/lib/renderer/pipeline.ts` — 渲染管线（原样复用，未改动）

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

### 10.2 `src/lib/components/Editor.svelte` — 编辑器（v0.2 调整）

**来源**：`mdhero/src/lib/components/Editor.svelte`

**保留的功能**：
- 全屏等宽字体 textarea
- Tab 键插入 2 空格缩进（保留光标位置）
- `localValue` + `$effect` 模式
- 自动聚焦

**v0.2 改动**：
- ~~`position: fixed` 全屏覆盖~~ → `flex: 1` 填充主内容区（适配侧边栏布局）
- 背景色改用 CSS 变量 `--zc-bg-chrome`

**Props 接口**：
```typescript
{ value: string; onChange: (newValue: string) => void }
```

---

### 10.3 `src/lib/components/MarkdownRenderer.svelte` — 渲染器（v0.2 调整）

**来源**：`mdhero/src/lib/components/MarkdownRenderer.svelte`

**保留的功能**：
- `{@html html}` 渲染 sanitized HTML
- Tailwind Typography 排版
- 代码块复制按钮
- KaTeX 公式、表格、引用块、任务列表

**v0.2 改动**：
- 所有硬编码颜色 → CSS 变量（`--zc-text-primary`, `--zc-border` 等）
- 去掉 `#0891B2` 青色 → 统一暖白单色调

**Props 接口**：
```typescript
{ html: string }
```

---

### 10.4 `src/lib/components/Sidebar.svelte` — 侧边栏 ★ v0.2 新增

**功能**：
- **头部**：\"FILES\" 标题 + 图钉/新建文件/新建文件夹图标按钮
- **文件树**：递归渲染目录（深度 3 层），只显示 `.md` 文件，点击打开
- **图钉**：钉选当前文件夹，下次启动自动加载（持久化到 disk）
- **新建交互**：点击 +file/+folder → 顶部出现 inline 输入行 → 回车确认/Esc 取消
- **Recent 分组**：可折叠的最近打开文件列表（20 条上限，持久化）
- **底部**：\"Open Folder…\" 按钮

---

### 10.5 `src/lib/components/TitleBar.svelte` — 自绘标题栏 ★ v0.2 新增

**功能**：
- `data-tauri-drag-region` 实现窗口拖动
- 左：侧边栏开关 + 设置齿轮按钮
- 中：当前文件名显示
- 右：最小化 / 最大化 / 关闭按钮（`@tauri-apps/api/window`）
- 背景色 `--zc-bg-chrome`，与内容区一致

**配合**：`tauri.conf.json` 中 `decorations: false`

---

### 10.6 `src/lib/components/SettingsDialog.svelte` — 设置对话框 ★ v0.2 新增 / v0.3 扩展

**功能**：
- `<dialog>` 模态弹窗，带 3 个 Tab：**Default Folder** / **AI Provider** / **Skills**
- **Default Folder**：显示当前钉选路径 + Browse… / Change… 按钮
- **AI Provider**：Base URL / Model 输入 + API Key（遮罩显示，点击可编辑；编辑模式下眼睛图标切换密码/明文）。支持 OpenAI 兼容端点和 Anthropic 兼容端点，后端自动检测 base_url 内容并路由到对应协议。标准路径后缀（`/v1/chat/completions` 或 `/v1/messages`）自动补全。API Key 真实值存入 OS keychain，本地 store 仅存脱敏版（如 `sk-5d70d***5c60`）
- **Skills**：4 个 AI 技能开关（Summarize / Fix Grammar / TOC / Explain Code）+ 预留 "Add custom skill" 按钮
- 保存/取消按钮，保存失败有错误提示；keychain 不可用时显示警告横幅但不阻塞保存
- 非敏感数据（baseUrl / model / maskedApiKey）持久化到 `zcode-settings.json`（通过 `settings.ts` store）；真实 API Key 通过 Rust `keyring` crate 存入系统 keychain
- 点击标题栏齿轮图标打开

---

### 10.7 `src/lib/stores/document.ts` — 文档状态（精简，未改动）

**来源**：`mdhero/src/lib/stores/document.ts`

**保留的功能**：
- 单文档 Svelte writable store
- `DocumentState` 接口：filePath, fileName, content, renderedHtml, frontmatter, wordCount, loading, error

---

### 10.8 `src/lib/stores/recents.ts` — 最近文件 ★ v0.2 新增

**功能**：
- `writable<RecentEntry[]>` store
- `addRecent(path)` — 去重上浮、上限 20 条
- 通过 `@tauri-apps/plugin-store` 持久化到 `zcode-recents.json`
- `load()` — 启动时从磁盘恢复
- 每次 `loadFile()` 成功后自动调用

---

### 10.9 `src/lib/stores/folderTree.ts` — 文件树状态 ★ v0.2 新增

**功能**：
- `rootPath` / `tree` / `loading` / `error` 状态
- `expandedPaths: Set<string>` — 文件夹展开/收起（纯内存状态）
- `toggleExpanded(path)` / `isExpanded(path)`

---

### 10.10 `src/lib/stores/pinnedFolder.ts` — 钉选文件夹 ★ v0.2 新增

**功能**：
- 持久化钉选的文件夹路径到 `zcode-recents.json`（key `"pinnedFolder"`）
- `pin(path)` / `unpin()` / `load()`
- 侧边栏 `onMount` 时自动加载

---

### 10.11 `src/lib/tauri/files.ts` — 文件操作（v0.2 扩展）

**来源**：`mdhero/src/lib/tauri/files.ts`

**新增函数**：
| 函数 | 功能 |
|---|---|
| `listDirTree(rootPath)` | 调用 `read_dir_tree` 获取嵌套目录树 |
| `createMarkdownFile(dir, name)` | 调用 `create_markdown_file`，成功后自动 loadFile |
| `createFolder(dir, name)` | 调用 `create_folder` |
| `pathExists(path)` | 调用 `path_exists`，主要用于判断 pinned folder 是否存在 |
| `openFolderDialog()` | 系统文件夹选择器（`directory: true`） |
| `reloadCurrentFile(path, isOwnSave?)` | ★ v0.5: 从磁盘重载当前文件并重新渲染（跳过内容未变时的无谓 DOM 重建），`isOwnSave` 标记自身保存以抑制 watcher 回环 |

**改动**：
- `loadFile()` 成功后自动调用 `recents.addRecent()` 和 `getCurrentWindow().setTitle()`
- `openFileDialog()` 过滤器中新增 `markdown` / `mdown` / `mkd` / `txt` 扩展名
- 移除了 `refreshFolderTree()`（刷新逻辑直接在 Sidebar 中调用 `listDirTree`）

---

### 10.12 `src/routes/+page.svelte` — 主页面（v0.2 重构）

**布局**：
```
┌────────────────────────────┐
│  TitleBar                  │  ← 自绘标题栏
├──────┬─────────────────────┤
│      │                     │
│ Side │  Main Content       │  ← 侧边栏 + 主内容（编辑/预览/空状态）
│ bar  │                     │
│      │                     │
├──────┴─────────────────────┤
│  StatusBar                 │  ← 底部状态栏
└────────────────────────────┘
```

**新增状态/逻辑**：
- `sidebarVisible` — 侧边栏可见性（默认 `true`）
- `userCollapsed` — 区分「手动收起」和「窗口太小自动收起」
- `settingsOpen` — 设置对话框
- `agentPanelOpen` — ★ v0.5: AI Agent 面板开关
- `lastWatchedPath` — ★ v0.5: 追踪当前监听的文件路径，生命周期管理文件 watcher
- ★ v0.5: `$effect` 监听 `docStore.filePath` 变化 → 自动 `startFileWatcher`（含前端 debounce 100ms + 自身保存抑制）
- ★ v0.5: `$effect` 监听 `docStore.content` 外部变更 → 非编辑态时同步 `editContent`
- 窗口 resize 监听（debounce 100ms）：宽度 < 640px → 自动收起侧边栏
- 宽度恢复时不自动展开（除非用户之前是手动展开的）
- `⌘B` — 切换侧边栏快捷键
- 状态栏底部 hint 文本支持 container query 响应式（窄屏时显示简洁版快捷键）

**新增状态变量**：
```
sidebarVisible, userCollapsed, settingsOpen, agentPanelOpen, lastWatchedPath
```

---

### 10.13 `src/app.css` — 全局样式（v0.2 重写配色）

**v0.2 改动**：
- 新增暖白单色调 CSS 变量：
```css
--zc-bg-page: #FAF9F6;       /* 页面背景 */
--zc-bg-chrome: #F4F2ED;     /* 标题栏/编辑器/预览区背景 */
--zc-bg-card: #FDFDFB;       /* 侧边栏浮动卡片背景 */
--zc-border: #E7E4DD;
--zc-border-soft: #ECE9E2;
--zc-text-primary: #1F1E1C;
--zc-text-secondary: #8A8782;
--zc-text-tertiary: #A8A49D;
--zc-active-row: #EAE6DD;    /* 选中行背景 */
```
- 滚动条收窄：webkit `width: 6px`、thumb `border-radius: 999px`；Firefox `scrollbar-width: thin`

---

### 10.14 `src/routes/+layout.svelte` — 布局（极简，未改动）

仅 3 行：导入全局 CSS + `<slot />`。

---

### 10.15 `src/app.d.ts` — 类型声明（未改动）

为 `markdown-it-task-lists` 和 `markdown-it-texmath` 提供 TypeScript 类型声明。

---

### 10.16 `src/lib/stores/settings.ts` — 应用设置持久化 ★ v0.3 新增

**功能**：
- 通过 `@tauri-apps/plugin-store` 持久化到 `zcode-settings.json`
- `AppSettings` 接口：AI 后端配置（baseUrl / model / maskedApiKey）+ 技能开关。`maskedApiKey` 为脱敏版本（如 `sk-5d70d***5c60`），可安全明文存储；真实 apiKey 由 Rust 命令存入 OS keychain
- `load()` 返回合并默认值的结果（向后兼容旧版本缺少的 key）
- `save()` 保存 baseUrl / model / maskedApiKey 到磁盘；真实 apiKey 由 `tauri/ai.ts` 的 `saveApiKey()` 通过 Rust `keyring` crate 写入系统 keychain
- 默认值：Summarize / FixGrammar 开启，TOC / ExplainCode 关闭

---

### 10.17 `src/lib/stores/sharedStore.ts` — 共享 Store 实例 ★ v0.3 新增

**功能**：
- 单例模式暴露 `zcode-recents.json` 的 `@tauri-apps/plugin-store` 实例
- 被 `recents.ts` 和 `pinnedFolder.ts` 共享，避免重复创建 Store 连接

---

### 10.18 `src/lib/tauri/ai.ts` — AI 前端接口 ★ v0.4 新增 / v0.6 扩展

**功能**：
- `saveApiKey(apiKey)` — 调用 Rust `save_api_key` 命令将 API Key 存入 OS keychain（空串=删除）
- `callAIProvider(baseUrl, model, prompt, providerName?)` — 调用 Rust `call_ai_provider`，后端从 keychain 读取 key 并发起流式 AI 调用
- `maskApiKey(key)` — 纯前端脱敏：前 3 字符 + `***` + 后 4 字符
- `startAgentTurn(args)` — ★ v0.6: `StartAgentTurnArgs` 新增 `contextWindowTokens` 字段，用于显式传递模型上下文窗口大小给后端压缩系统

---

### 10.19 `zcode-mock.html` — 开发 mock 页面 ★ 新增

项目根目录下的独立 HTML 文件，用于在浏览器中快速预览 Markdown 渲染效果（无需启动 Tauri）。

---

## 十一、Tauri Rust 后端（20 个源文件）

### 11.1 `src-tauri/src/commands.rs` — 命令（10 个命令）

| 命令 | 功能 | v0.1 | v0.2 | v0.3+ |
|---|---|---|---|---|
| `read_markdown_file(path)` | 读取文件内容为 UTF-8 字符串 | ✅ | ✅ | ✅ |
| `write_markdown_file(path, content)` | 写入字符串到文件 | ✅ | ✅ | ✅ |
| `resolve_path(path)` | 将相对路径解析为绝对路径 | ✅ | ✅ | ✅ |
| `allow_assets(paths)` | 图片路径加入 asset protocol 白名单 | ✅ | ✅ | ✅ |
| `read_dir_tree(root)` | 递归扫描目录，返回 `DirNode` 嵌套树 | ❌ | ✅ | ✅ |
| `path_exists(path)` | 检查路径是否存在 | ❌ | ✅ | ✅ |
| `create_markdown_file(dir, name)` | 在目录下创建 `.md` 文件 | ❌ | ✅ | ✅ |
| `create_folder(dir, name)` | 在目录下创建子文件夹 | ❌ | ✅ | ✅ |
| `save_api_key(api_key)` | 将 API Key 存入 OS keychain（空串=删除），keychain 不可用时返回警告 | ❌ | ❌ | ✅ |
| `call_ai_provider(base_url, model, prompt, provider_name?)` | 从 keychain 读取 API Key 并发起 AI 流式调用，返回完整响应文本 | ❌ | ❌ | ✅ |

**新增数据结构**：
```rust
struct DirNode {
    name: String,
    path: String,       // 绝对路径
    is_dir: bool,
    modified: Option<i64>,   // unix timestamp，文件才有
    children: Option<Vec<DirNode>>,  // 只有 is_dir=true 才填充
}
```

**`read_dir_tree` 策略**：
- 支持 `.md` / `.markdown` / `.mdown` / `.mkd` 文件扩展名
- 跳过隐藏文件/目录（`.` 开头）
- 跳过 `node_modules` / `target` / `dist` / `build` / `__pycache__` / `vendor` / `zig-cache` / `zig-out`
- 最大深度 3 层（`MAX_TREE_DEPTH = 3`，前端 Sidebar 模板也渲染 3 层）
- 空目录不返回（不含任何 md 文件的目录节点会被剪掉）
- 排序：目录在前、文件在后，各自按字母序

---

### 11.2 `src-tauri/src/lib.rs` — 应用入口

```rust
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())     // 外部链接用系统浏览器打开
        .plugin(tauri_plugin_dialog::init())      // 系统文件/文件夹对话框
        .plugin(tauri_plugin_store::Builder::default().build())  // ★ v0.2: 键值持久化
        .setup(|app| {
            // ★ v0.4: 启动时自动迁移旧版明文 apiKey 到 keychain
            if let Ok(config_dir) = app.path().app_config_dir() {
                crate::settings::migrate_old_settings(&config_dir);
            }
            Ok(())
        })
        .manage(agent_command::SessionManager::new())
        .manage(watcher::WatcherState::default())                // ★ v0.5: 文件监听状态
        .invoke_handler(tauri::generate_handler![
            commands::read_markdown_file,
            commands::write_markdown_file,
            commands::resolve_path,
            commands::allow_assets,
            commands::read_dir_tree,              // ★ v0.2
            commands::path_exists,                // ★ v0.2
            commands::create_markdown_file,       // ★ v0.2
            commands::create_folder,              // ★ v0.2
            commands::save_api_key,               // ★ v0.4: keychain 存储
            commands::call_ai_provider,           // ★ v0.4: AI 流式调用（key 从 keychain 读取）
            agent_command::start_agent_turn,      // ★ v0.3: 启动 AI agent 会话
            agent_command::approve_tool_call,     // ★ v0.3: 批准危险工具调用
            watcher::start_watching,              // ★ v0.5: 启动文件系统监听
            watcher::stop_watching,               // ★ v0.5: 停止文件系统监听
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**v0.4 新增**：`pub mod settings;` 模块（keychain API Key 存储 + 旧版迁移），`use tauri::Manager;` 引入。
**v0.5 新增**：`pub mod watcher;` 模块、`watcher::WatcherState` managed state、`watcher::start_watching` / `watcher::stop_watching` 命令、`agent_command::start_agent_turn` / `agent_command::approve_tool_call` 命令注册。
**v0.6 新增**：`pub mod compaction;` 模块（上下文压缩）。

---

### 11.3 `src-tauri/src/settings.rs` — API Key 安全存储 ★ v0.4 新增

**功能**：
- `set_api_key(key)` — 将 API Key 写入 OS keychain（通过 `keyring` crate），空字符串表示删除
- `get_api_key()` — 从 OS keychain 读取 API Key
- `delete_api_key()` — 删除 keychain 中的 API Key
- `mask_api_key(key)` — 前端用脱敏函数：`"sk-abc123...xyz789"` → `"sk-***z789"`（前 3 + `***` + 后 4）
- `migrate_old_settings(config_dir, data_dir)` — 启动时自动迁移旧版 `zcode-settings.json` 中的明文 `apiKey` 到 keychain（v0.3 → v0.4 升级），同时扫描 app config 和 app data 两个目录
- 所有 keychain 操作采用 best-effort 策略：keychain 不可用时（如 WSL 无 secret-service）返回 `Ok(Some(warning))` 而非 `Err`，不阻塞应用正常运行

**依赖**：`keyring = { version = "3", features = ["apple-native"/"windows-native"/"linux-native"] }`（按目标平台条件编译）

**设计要点**：
- `zcode-settings.json` 仅存脱敏版 `maskedApiKey`（如 `"sk-5d70d***5c60"`），真实 key 只在 keychain
- 迁移时：读取旧版明文 key → 存入 keychain → 将 store 中的 `apiKey` 替换为 `maskedApiKey` → 若 keychain 不可用则保留原文件不变
- `mask_api_key` 逻辑：≤7 字符返回 `"***"`，否则 `prefix[..3] + "***" + suffix[-4..]`

---

### 11.4 `src-tauri/src/main.rs` — 二进制入口（未改动）

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() { zcode_lib::run() }
```

---

### 11.5 Agent Pipeline ★ v0.3 新增

Agent pipeline 从 pi-agent-rust 参考项目（commit e7792d64）移植而来，提供 AI 编程代理能力：

**模块结构**：

| 模块 | 文件 | 用途 |
|---|---|---|
| **Agent Loop** | `agent.rs` | 主循环编排：用户输入 → Provider 流式调用 → 工具执行 → 循环，支持事件回调 |
| **Model** | `model.rs` | 共享消息类型：UserMessage, AssistantMessage, ToolResultMessage, ContentBlock, StreamEvent, Usage |
| **Provider** | `provider.rs` | LLM 抽象层：定义 `Provider` trait、`Context`、`ToolDef`、`StreamOptions` |
| **Anthropic** | `providers/anthropic.rs` | Anthropic Messages API 实现（原生 API，含 extended thinking） |
| **OpenAI** | `providers/openai.rs` | OpenAI Chat Completions API 实现（兼容 20+ 提供商：Groq, DeepSeek, OpenRouter, Together 等） |
| **Provider dispatch** | `providers/mod.rs` | `build_provider()` 自动分发：检测 base_url 中的 "anthropic" 关键字（大小写不敏感），自动路由到 AnthropicProvider 或 OpenAIProvider；外加 URL 规范化（自动补全 `/v1/messages` 或 `/v1/chat/completions`） |
| **Skills** | `skills.rs` | 技能加载器：从 `.zcode/skills/*/SKILL.md`（项目级）和 `~/.config/zcode/skills/*/SKILL.md`（用户级）发现技能，YAML frontmatter 解析，XML 格式化注入 system prompt |
| **SSE** | `sse.rs` | Server-Sent Events 流解析器，基于 reqwest streaming response |
| **Error** | `error.rs` | 统一错误类型：Provider/Tool/Validation/Api/Sse/Io/Other |
| **Tools** | `tools/mod.rs` | 工具 trait + 注册表 + 路径安全工具（enforce_cwd_scope, resolve_path, canonicalize_safe） |
| **Read** | `tools/read.rs` | 文件读取（支持 offset/limit、图片、截断） |
| **Bash** | `tools/bash.rs` | Shell 命令执行（120s 超时、输出截断） |
| **Edit** | `tools/edit.rs` | 精确文本替换编辑（多编辑批量、边界检查） |
| **Write** | `tools/write.rs` | 文件创建/覆盖（路径限制 100MB） |
| **Grep** | `tools/grep.rs` | ripgrep 文本搜索（需安装 `rg`） |
| **Find** | `tools/find.rs` | fd-find 文件搜索（需安装 `fd`） |
| **Ls** | `tools/ls.rs` | 目录列表（截断 500 条目、扫描上限 20000） |
| **Agent Command** | `agent_command.rs` | ★ v0.3+: Agent 会话管理（`start_agent_turn` 多轮编排、危险工具守卫、oneshot 审批通道、session-scoped events、`context_window_tokens` 参数），★ v0.5: 针对当前文件自动批准 write/edit、系统提示同步当前文件路径、智能 CWD 推导，★ v0.6: CompactionStarted/CompactionFinished 流事件 |
| **File Watcher** | `watcher.rs` | ★ v0.5: 文件系统监听（`notify` + `notify-debouncer-mini`，500ms debounce，父目录监听存活原子写入，`file-changed` 事件发射到前端） |
| **Compaction** | `compaction.rs` | ★ v0.6: 上下文压缩（token 预估 → 触发检测 → 切点查找 → LLM 摘要替换 → 清理），迭代摘要更新、thrashing 防抖、摘要压缩降级、模型名窗口推断 |

**依赖新增**（Cargo.toml）：
- `async-trait`, `reqwest` (stream + rustls-tls + json), `futures`, `tokio` (rt-multi-thread + sync + time + process + fs)
- `base64`, `chrono` (serde), `anyhow`
- `tempfile` (dev-dependency)

**测试**：
- `agent_e2e.rs` — 端到端代理测试（含工具调用循环，需 DeepSeek API key）
- `agent_mock.rs` — ★ v0.4 新增：Agent pipeline mock 测试（不依赖外部 API）
- `provider_smoke.rs` — 两个 Provider 的流式调用冒烟测试
- `settings_keychain.rs` — ★ v0.4 新增：keychain 存储 + 迁移逻辑 + mask 边界测试
- `skill_e2e.rs` — 技能注入 + 模型识别端到端测试
- `tool_smoke.rs` — 所有 7 个工具的单元测试

**关键设计**：
- 无 async supersync 依赖，纯 tokio 异步运行时
- 无 TUI 依赖，专为 Tauri 桌面应用上下文设计
- 工作目录安全：所有文件操作强制限制在 CWD 范围内（`enforce_cwd_scope`）
- 工具输出截断：最大 500 行 / 100KB（head+tail 策略：保留头部上下文和尾部错误信息，截断标记注明省略字节/行数）
- 上下文自动压缩（context compaction）：token 预估触发 LLM 摘要替换旧消息，迭代摘要更新、thrashing 防抖（冷却轮次+最大连续失败数）、摘要压缩降级
- 卡死循环检测（stuck-loop）：同一工具+相同参数连续 3 次调用自动停止，置错误状态
- 40 轮软提示：工具迭代达到 40 轮时注入 system_note 提示即将收尾
- 图片历史占位：工具执行后将图片内容替换为文本占位（`[已读取图片]`），避免 base64 撑爆上下文

---

## 十二、配置和工程文件（v0.2 改动）

| 文件 | v0.2 改动 |
|---|---|
| `package.json` | +`@tauri-apps/plugin-store` 依赖 |
| `Cargo.toml` | +`tauri-plugin-store = "2"`；+ Phase 1 agent pipeline deps（async-trait, reqwest, tokio, chrono, anyhow 等 7 个 deps + 1 个 dev-dep）；★ v0.5 +`notify` +`notify-debouncer-mini`（文件监听） |
| `tauri.conf.json` | `decorations: false`（自绘标题栏）；fileAssociations 支持 `.md`/`.markdown`/`.mdown`/`.mkd` |
| `capabilities/default.json` | +`core:window:allow-set-title/minimize/toggle-maximize/close/start-dragging`；+`store:default` |
| `.github/workflows/build.yml` | ★ GitHub Actions CI/CD（tag 推送 + 手动触发，macOS + Windows 构建并生成 bundle artifact；tag 推送时自动创建 GitHub Release） |

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
| 本地持久化 | tauri-plugin-store | ^2.4 |
| 构建工具 | Vite | ^6.4 |

---

## 十四、保留的 npm 依赖

### 运行时依赖
```json
{
  "@tauri-apps/api": "^2",
  "@tauri-apps/plugin-dialog": "^2",
  "@tauri-apps/plugin-opener": "^2",
  "@tauri-apps/plugin-store": "^2",
  "dompurify": "^3",
  "highlight.js": "^11",
  "katex": "^0.17",
  "markdown-it": "^14",
  "markdown-it-anchor": "^9",
  "markdown-it-task-lists": "^2",
  "markdown-it-texmath": "^1"
}
```

### 开发依赖（不变）
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
tauri-plugin-store = "2"
[target.'cfg(target_os = "macos")'.dependencies]
keyring = { version = "3", features = ["apple-native"] }

[target.'cfg(target_os = "windows")'.dependencies]
keyring = { version = "3", features = ["windows-native"] }

[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "3", features = ["linux-native"] }

serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Phase 1: Provider abstraction + LLM API clients
async-trait = "0.1"
reqwest = { version = "0.12", default-features = false, features = ["stream", "rustls-tls", "json"] }
futures = "0.3"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time", "process", "fs"] }
base64 = "0.22"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
notify = { version = "7", features = ["macos_fsevent"] }              # ★ v0.5: 文件系统监听
notify-debouncer-mini = "0.5"                                      # ★ v0.5: 事件防抖（500ms）

[dev-dependencies]
tempfile = "3"
dotenvy = "0.15"
```

---

## 十六、最终文件结构

```
zcode/
├── package.json
├── pnpm-lock.yaml
├── pnpm-workspace.yaml
├── vite.config.js
├── svelte.config.js
├── tsconfig.json
├── zcode-mock.html                      # 开发 mock 页面（浏览器内预览 Markdown）
├── src/
│   ├── app.css                          # 全局样式 + 暖白调 CSS 变量 + 细滚动条
│   ├── app.d.ts                         # 类型声明
│   ├── app.html                         # HTML 模板
│   ├── routes/
│   │   ├── +layout.svelte               # 布局（极简）
│   │   ├── +layout.ts                   # SSR=off
│   │   └── +page.svelte                 # 主页面：标题栏+侧边栏+主内容布局
│   └── lib/
│       ├── components/
│       │   ├── Editor.svelte            # 编辑器
│       │   ├── MarkdownRenderer.svelte  # 渲染器
│       │   ├── Sidebar.svelte           # 侧边栏（文件树+Recent+pin，渲染 3 层嵌套）
│       │   ├── TitleBar.svelte          # 自绘标题栏
│       │   └── SettingsDialog.svelte    # 设置对话框（3 个 Tab：Folder/AI/Skills）
│       ├── stores/
│       │   ├── document.ts              # 文档状态
│       │   ├── recents.ts               # 最近文件（持久化到 zcode-recents.json）
│       │   ├── folderTree.ts            # 文件树状态 + 展开/收起
│       │   ├── pinnedFolder.ts          # 钉选文件夹（持久化到 zcode-recents.json）
│       │   ├── settings.ts              # 应用设置（持久化到 zcode-settings.json）
│       │   └── sharedStore.ts           # 共享 Store 单例
│       ├── renderer/
│       │   └── pipeline.ts              # 渲染管线
│       └── tauri/
│           ├── files.ts                 # 文件操作（9 个函数：+reloadCurrentFile）
│           ├── ai.ts                    # AI keychain 操作 + callAIProvider + maskApiKey
│           └── watcher.ts               # ★ v0.5: 前端文件监听（listen file-changed 事件 + save 抑制）
├── .github/
│   └── workflows/
│       └── build.yml                    # CI/CD（tag 推送 + 手动触发，macOS/Windows 构建+Release）
├── src-tauri/
│   ├── Cargo.toml                       # +tauri-plugin-store + agent pipeline deps
│   ├── tauri.conf.json                  # decorations:false + fileAssociations
│   ├── capabilities/
│   │   └── default.json                 # +window +store 权限
│   ├── icons/...
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs                       # 14 命令 + 3 插件 + agent pipeline + compaction + watcher 模块声明 + migration setup
│   │   ├── commands.rs                  # 10 个命令，MAX_TREE_DEPTH=3
│   │   ├── settings.rs                  # ★ v0.4: OS keychain API Key 安全存储（读写/删/脱敏/旧版迁移）
│   │   ├── agent_command.rs             # ★ v0.3: Agent 会话管理（start_agent_turn / approve_tool_call + 危险工具守卫）
│   │   ├── watcher.rs                   # ★ v0.5: 文件系统监听（notify + debouncer，父目录监听存活原子写入）
│   │   ├── agent.rs                     # ★ v0.3: Agent 主循环编排，★ v0.6: 上下文压缩集成+卡死循环检测
│   │   ├── compaction.rs                # ★ v0.6: 上下文压缩（token 预估/触发/摘要/替换，迭代更新+thrashing 防抖）
│   │   ├── model.rs                     # ★ v0.3: 共享消息/内容块/流事件类型
│   │   ├── provider.rs                  # ★ v0.3: Provider trait 抽象层
│   │   ├── providers/
│   │   │   ├── mod.rs
│   │   │   ├── anthropic.rs              # ★ v0.3: Anthropic Messages API 实现
│   │   │   └── openai.rs                 # ★ v0.3: OpenAI Chat Completions 实现
│   │   ├── skills.rs                    # ★ v0.3: 技能加载器（YAML frontmatter + XML 注入）
│   │   ├── sse.rs                       # ★ v0.3: SSE 流解析器
│   │   ├── error.rs                     # ★ v0.3: 统一错误类型
│   │   └── tools/
│   │       ├── mod.rs                   # ★ v0.3: Tool trait + 注册表 + 路径安全
│   │       ├── bash.rs                  # ★ v0.3: Shell 命令执行
│   │       ├── edit.rs                  # ★ v0.3: 精确文本替换编辑
│   │       ├── find.rs                  # ★ v0.3: fd-find 文件搜索
│   │       ├── grep.rs                  # ★ v0.3: ripgrep 文本搜索
│   │       ├── ls.rs                    # ★ v0.3: 目录列表
│   │       ├── read.rs                  # ★ v0.3: 文件读取
│   │       └── write.rs                 # ★ v0.3: 文件创建/覆盖
│   └── tests/
│       ├── agent_e2e.rs                 # ★ v0.3: Agent 端到端测试（含工具调用，需 API key）
│       ├── agent_mock.rs                # ★ v0.4: Agent pipeline mock 测试（无外部依赖）
│       ├── provider_smoke.rs            # ★ v0.3: Provider 流式调用测试
│       ├── settings_keychain.rs         # ★ v0.4: keychain 存储 + 迁移 + mask 边界测试
│       ├── skill_e2e.rs                 # ★ v0.3: 技能注入端到端测试（需 API key）
│       └── tool_smoke.rs                # ★ v0.3: 工具注册表 + 7 个工具单元测试
└── REMOVED.md                           # 本文档
```

**源文件总计：45 个**（前端 22 个 + Rust 23 个，不含配置/图标/zcode-mock.html/测试文件）

---

## 十七、变更摘要

### 新增功能
- **侧边栏**：文件树浏览（3 层嵌套）、新建文件/文件夹、最近文件列表
- **文件系统监听** ★ v0.5：检测外部文件变更并自动重载到编辑器，500ms debounce + 父目录监听（存活原子写入），前后端协同抑制自身保存事件防止回环
- **智能自动批准** ★ v0.5：write/edit 操作目标为当前编辑器打开的文件时自动跳过确认对话框，无需全局 auto-approve
- **UTF-8 截断安全** ★ v0.5：`truncate_at_char_boundary()` 确保所有字符串截断（grep/edit/agent prompt）不会切割多字节字符
- **CWD 智能推导** ★ v0.5：agent 工作目录优先使用当前文件所在目录，回退到钉选文件夹
- **编辑器/预览显示逻辑修复** ★ v0.5：从检查 `renderedHtml` 改为检查 `filePath`，修复新文件保存后仍显示空状态的 bug
- **自绘标题栏**：borderless 窗口、窗口控制按钮（最小化/最大化/关闭）、文件名显示
- **钉选文件夹**：持久化记住文件夹路径，启动时自动加载
- **设置对话框**：3 个 Tab — Default Folder / AI Provider（含 API Key + Model 配置）/ Skills（4 个 AI 技能开关）
- **小窗口适配**：宽度 < 640px 自动收起侧边栏，状态栏 hints 通过 container query 响应式适配
- **CI/CD 构建流水线**：GitHub Actions（tag 推送 + 手动触发），macOS + Windows 构建，tag 自动 Release
- **AI 后端配置 + 安全存储**（v0.4 重构）：API Key 通过 `keyring` crate 存入 OS keychain（按目标平台使用原生后端：macOS Keychain / Windows Credential Manager / Linux secret-service），`zcode-settings.json` 仅存脱敏版（如 `sk-5d70d***5c60`）；新模块 `settings.rs` 统一管理 keychain 读写/删除/脱敏/旧版迁移；`ai.ts` 前端封装接口；keychain 不可用时 best-effort 降级不阻塞；启动时自动扫描 config + data 双目录迁移旧版明文 `apiKey`
- **共享 Store 实例**：`sharedStore.ts` 单例模式，供 recents 和 pinnedFolder 共用 `zcode-recents.json`
- **AI Agent Pipeline** ★ v0.3：完整的 AI 编程代理（Agent Loop + Provider + Tools + Skills），从 pi-agent-rust 移植，支持 Anthropic 原生 API 和 OpenAI Chat Completions（兼容 20+ 提供商）。`build_provider()` 自动分发——根据 base_url 检测协议类型并自动补全路径后缀。7 个工具（read/bash/edit/write/grep/find/ls + 路径安全），技能系统（YAML frontmatter + XML system prompt 注入），纯 tokio 异步运行时，6 个测试覆盖端到端/mock/工具单元/keychain 安全
- **上下文压缩（Context Compaction）** ★ v0.6：防止长会话 token 无限增长。token 预估触发送代 LLM 摘要（支持增量更新已有摘要），替换旧消息为结构化摘要（Goal/Progress/Decisions/Next Steps/Critical Context）。五重保护机制：触发阈值（85% 有效窗口）+ 冷却轮次（5 轮间）+ 最大连续失败 kill-switch（3 次）+ 摘要大小上限（15% 有效窗口）+ 摘要压缩降级
- **卡死循环检测（Stuck-Loop Detection）** ★ v0.6：同一工具调用相同参数连续 3 次→自动停止、置错误状态并通知前端
- **输出截断改进** ★ v0.6：从简单头部截断改为 head+tail 策略，保留尾部错误信息（构建日志、测试失败等），截断标记注明省略量。默认限制从 2000 行/1MB 收紧为 500 行/100KB
- **图片历史占位** ★ v0.6：工具执行后替换图片内容为文本占位符避免 base64 膨胀消息历史
- **40 轮软提示** ★ v0.6：工具迭代达 40 轮时自动注入 system_note 提示准备收尾

### 色彩体系
- 从硬编码 `#fafafa` / `#1c1c1e` / `#0891B2` 切换为暖白单调 CSS 变量
- 无任何品牌色（蓝/青），纯 `#1F1E1C` 灰度体系

### 快捷键
- `⌘O` — 打开文件
- `⌘E` — 编辑/预览切换
- `⌘S` — 保存
- `⌘B` — 切换侧边栏

### 技术细节
- `MAX_TREE_DEPTH = 3`（前后端一致）
- 支持 `.md` / `.markdown` / `.mdown` / `.mkd` 文件扩展名
- Rust 跳过目录：`node_modules` / `target` / `dist` / `build` / `__pycache__` / `vendor` / `zig-cache` / `zig-out`
- Store 文件：`zcode-recents.json`（最近文件 + 钉选文件夹）、`zcode-settings.json`（AI 配置 + 技能）

### 已知取舍
- **Windows Snap Layouts**：`decorations: false` 会丢失此系统功能
- **文件树深度**：后端扫描 3 层，前端 Sidebar 模板渲染 3 层
- **macOS 交通灯**：使用 `decorations: false` 而非 `titleBarStyle: "Overlay"`，macOS 上会丢失原生红黄绿按钮
- **工具依赖**：grep 工具需系统安装 `rg`（ripgrep），find 工具需系统安装 `fd`（fd-find）
