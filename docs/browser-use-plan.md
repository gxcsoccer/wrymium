# Wrymium Browser Use 实现计划

## Context

AI Browser Use（AI 驱动的浏览器自动化）正成为 AI Agent 的核心能力。当前市面上的方案都需要**外部浏览器进程**（Playwright WebSocket / CDP over network），引入了额外的延迟和部署复杂性。wrymium 将 CEF 嵌入 Tauri 应用，天然拥有**进程内 CDP 通道**和**原生输入事件注入**能力，可以实现体验远超现有方案的 Browser Use。

---

## 一、竞品分析与方案对比

### 现有方案的核心架构与痛点

| 方案 | 架构 | 截图方式 | 输入方式 | 延迟/动作 | 核心痛点 |
|------|------|---------|---------|----------|---------|
| **browser-use** | Playwright WS → Chrome | DOM 解析 200-400 tokens | Playwright API | <100ms | DOM 脆弱性，2 层网络跳转 |
| **Anthropic Computer Use** | 桌面截图 → Vision API | 全屏截图 5K-10K tokens | OS 级坐标点击 | ~800ms | 极慢，token 昂贵 |
| **OpenAI Operator** | 云端虚拟浏览器 | Vision 截图 | 虚拟鼠标键盘 | 高（网络） | 云端依赖，同步问题 |
| **Playwright MCP** | CDP over WS | A11y 快照 ~114K tokens | Playwright API | <100ms | Context window 膨胀 |
| **Stagehand** | CDP over network | CDP Page.captureScreenshot | Playwright → CDP | <100ms | 依赖 Browserbase 基础设施 |

### 所有现有方案的共同弱点

1. **外部浏览器进程** — 至少 1 次网络跳转（WebSocket/TCP），增加延迟和故障点
2. **部署复杂** — 需要安装/管理独立 Chrome/Chromium 实例
3. **用户不可见** — 浏览器在后台运行，用户无法直观看到 AI 操作
4. **无法人机协作** — AI 操作时用户不能介入，反之亦然

### wrymium 的结构性优势

| 维度 | 现有方案 | wrymium |
|------|---------|---------|
| CDP 通道 | WebSocket/TCP（~1-5ms RTT） | **进程内函数调用（~0.01ms）** |
| 输入注入 | CDP Input.dispatch（需 JSON 序列化） | **原生 CEF API（零序列化）** |
| 截图 | CDP → base64 → 解码 | **OSR on_paint 直出像素 buffer** |
| 部署 | 应用 + 外部浏览器 | **单一 Tauri bundle** |
| 用户体验 | 不可见/外部窗口 | **嵌入应用 UI，实时可见** |
| 人机协作 | 不支持 | **用户随时接管/观察** |

---

## 二、场景验证

用 10 个典型 Browser Use 场景验证方案可行性：

### 场景 1: Web 搜索 + 信息提取
> "搜索 Google 找到 X 的最新信息并总结"

- **需要**: navigate, 等待加载, 获取页面内容, 点击链接
- **wrymium 方案**: `Page.navigate` → `Page.loadEventFired` 等待 → `Accessibility.getFullAXTree` 提取内容 → 原生 click 点击链接
- **优势**: 无障碍树比 DOM HTML 对 LLM 更友好（结构化、token 少）；进程内 CDP 获取树延迟 <1ms vs 网络 CDP ~5-10ms
- **✅ 可行且更优**

### 场景 2: 表单填写
> "用我的信息填写这个申请表"

- **需要**: DOM 查询定位输入框, 点击聚焦, 键入文本, 选择下拉, 提交
- **wrymium 方案**: `DOM.querySelector` 定位 → `DOM.getBoxModel` 获取坐标 → 原生 `send_mouse_click_event` 点击 → 原生 `send_key_event` 逐字键入
- **优势**: 原生键盘事件触发所有 JS 事件（keydown/keypress/input/change），比 CDP `Input.dispatchKeyEvent` 更接近真实用户行为，不会被反自动化检测
- **✅ 可行且更优**

### 场景 3: 电商比价
> "在多个网站比较商品价格"

- **需要**: 多标签页, 页面导航, 数据提取, 截图对比
- **wrymium 方案**: 多个 WebView 实例（每个对应一个标签页）→ 并行 CDP 获取内容 → `Page.captureScreenshot` 截图
- **优势**: Tauri 原生多 WebView 支持，每个 WebView 独立 CDP 通道；用户在应用 UI 中直接看到多个标签页对比结果
- **✅ 可行且更优**

### 场景 4: 登录 + 认证操作
> "登录我的邮箱，检查来自 X 的邮件"

- **需要**: 输入凭证, 处理 2FA, Cookie 持久化
- **wrymium 方案**: 原生输入事件填写凭证 → 截图供用户确认 2FA → Cookie 通过 `CefCookieManager` 持久化
- **优势**: **人机协作模式** — 敏感操作（密码、2FA）交给用户在嵌入式浏览器中手动完成，AI 只负责导航和信息提取。这是其他方案无法做到的
- **✅ 可行，人机协作是独特优势**

### 场景 5: 多步骤导航
> "去 GitHub，找到 repo X，查看最新 issues"

- **需要**: 连续导航, 等待 SPA 路由变化, 滚动加载
- **wrymium 方案**: navigate → `Page.loadEventFired` → A11y 树定位链接 → click → 循环
- **优势**: SPA 路由变化通过 CDP `Page.frameNavigated` 事件精确检测；`send_mouse_wheel_event` 触发 IntersectionObserver 懒加载
- **✅ 可行且更优**

### 场景 6: 数据抓取
> "提取这个页面上所有产品价格"

- **需要**: DOM 遍历, 选择器查询, 结构化数据提取
- **wrymium 方案**: `Runtime.evaluate` 执行 JS 提取 + `Accessibility.getFullAXTree` 获取语义结构
- **优势**: `Runtime.evaluate` 直接返回 JSON 结果（现有 evaluate_script 无返回值，CDP 解决了这个 TODO）；进程内调用无网络序列化开销
- **✅ 可行且更优**

### 场景 7: 文件下载
> "从这个仪表板下载最新报告"

- **需要**: 点击下载按钮, 监控下载进度, 获取文件
- **wrymium 方案**: wrymium 已有 `with_download_started_handler` / `with_download_completed_handler` → click 触发下载 → handler 回调获取文件路径
- **优势**: 下载处理已内置，其他方案需要额外配置 Playwright 下载行为
- **✅ 可行，已有基础设施**

### 场景 8: CAPTCHA / 视觉挑战
> "网站弹出了验证码"

- **需要**: 识别验证码, 交给用户或 Vision 模型处理
- **wrymium 方案**: 截图 → Vision 模型识别 / **直接交给用户在嵌入式浏览器中手动完成**
- **优势**: **杀手级特性** — 浏览器嵌入在应用 UI 中，AI 检测到 CAPTCHA 后暂停，用户直接在同一窗口中手动完成验证，然后 AI 继续。零上下文切换。其他方案要么完全无法处理，要么需要切换到外部浏览器窗口
- **✅ 可行，人机协作是独特优势**

### 场景 9: 动态内容 (SPA / 无限滚动)
> "滚动到底部加载所有评论"

- **需要**: 滚动, 等待 DOM 变化, 检测加载完成
- **wrymium 方案**: `send_mouse_wheel_event` 滚动 → CDP `DOM.documentUpdated` 事件监听 DOM 变化 → `Runtime.evaluate` 检查 scrollHeight 判断是否到底
- **优势**: 原生滚动事件 + CDP DOM 事件监听的组合比纯 CDP `Input.dispatchMouseEvent` 更可靠
- **✅ 可行且更优**

### 场景 10: 多标签页工作流
> "在多个标签页中对比不同方案的报价"

- **需要**: 创建/切换标签页, 并行操作, 数据汇总
- **wrymium 方案**: Tauri 原生支持多 WebView → 每个 WebView 独立 Browser/BrowserHost → 并行 CDP 操作
- **优势**: 不需要 CDP 的 Target.createTarget，直接用 Tauri 的 WebView 管理。用户在应用 UI 中直接看到所有标签页
- **✅ 可行且更优**

### 验证总结

| 场景 | 可行性 | 比现有方案更优 | 关键优势 |
|------|--------|--------------|---------|
| Web 搜索 | ✅ | ✅ | 进程内 CDP，延迟更低 |
| 表单填写 | ✅ | ✅ | 原生输入事件，更真实 |
| 电商比价 | ✅ | ✅ | 多 WebView 并行 |
| 登录认证 | ✅ | ✅✅ | 人机协作模式 |
| 多步导航 | ✅ | ✅ | SPA 事件精确检测 |
| 数据抓取 | ✅ | ✅ | Runtime.evaluate 直返 |
| 文件下载 | ✅ | ✅ | 下载 handler 已内置 |
| CAPTCHA | ✅ | ✅✅✅ | 嵌入式人机协作 |
| 动态内容 | ✅ | ✅ | 原生滚动 + DOM 事件 |
| 多标签页 | ✅ | ✅ | Tauri 原生多 WebView |

**10/10 场景全部可行，10/10 场景优于或等于现有方案，3 个场景有独特的结构性优势。**

---

## 三、实现计划

### Phase 1: CDP Bridge（核心基础）~500 行 Rust

**目标**: 在 wrymium 中建立双向 CDP 通信通道

**新增文件**: `wrymium/src/cdp.rs`

```
CDP Bridge 架构:

  WebView.cdp_send(method, params)
         │
         ▼
  CdpBridge {
    next_id: AtomicI32,
    pending: Arc<Mutex<HashMap<i32, oneshot::Sender<CdpResponse>>>>,
    event_subscribers: Arc<Mutex<Vec<mpsc::Sender<CdpEvent>>>>,
  }
         │
         ▼
  BrowserHost::execute_dev_tools_method(id, method, params)
         │
         ▼  (CEF 内部处理)
         │
  WrymiumDevToolsObserver (wrap_dev_tools_message_observer!)
    ├── on_dev_tools_method_result(message_id, success, result)
    │     → pending.remove(message_id).send(result)
    ├── on_dev_tools_event(method, params)
    │     → event_subscribers.broadcast(event)
    └── on_dev_tools_message(message)
          → 原始消息日志/调试
```

**关键实现**:

1. `CdpBridge` 结构体 — 管理请求/响应匹配和事件分发
2. `WrymiumDevToolsObserver` — 用 `wrap_dev_tools_message_observer!` 宏实现
3. `WebView::cdp_send(method, params) -> Result<Value>` — CDP 调用（内部用 channel 等待响应）
4. `WebView::cdp_subscribe() -> mpsc::Receiver<CdpEvent>` — CDP 事件订阅

**cef-rs API 确认**（全部可用）:

```rust
// BrowserHost 方法
fn send_dev_tools_message(&self, message: Option<&[u8]>) -> c_int;
fn execute_dev_tools_method(&self, message_id: c_int, method: Option<&CefString>, params: Option<&mut DictionaryValue>) -> c_int;
fn add_dev_tools_message_observer(&self, observer: Option<&mut DevToolsMessageObserver>) -> Option<Registration>;

// Observer trait
trait ImplDevToolsMessageObserver {
    fn on_dev_tools_message(&self, browser: Option<&mut Browser>, message: Option<&[u8]>) -> c_int;
    fn on_dev_tools_method_result(&self, browser: Option<&mut Browser>, message_id: c_int, success: c_int, result: Option<&[u8]>);
    fn on_dev_tools_event(&self, browser: Option<&mut Browser>, method: Option<&CefString>, params: Option<&[u8]>);
    fn on_dev_tools_agent_attached(&self, browser: Option<&mut Browser>);
    fn on_dev_tools_agent_detached(&self, browser: Option<&mut Browser>);
}

// 宏: wrap_dev_tools_message_observer! 可用
```

**修改文件**:
- `wrymium/src/webview.rs` — WebView 增加 `cdp_bridge: Option<CdpBridge>` 字段，在 `on_after_created` 中初始化
- `wrymium/src/lib.rs` — 增加 `mod cdp` 导出

### Phase 2: Browser Use 原语（操作层）~600 行 Rust

**目标**: 基于 CDP Bridge + 原生 API 封装高层浏览器操作

**新增文件**: `wrymium/src/browser_use.rs`

**核心 API**:

```rust
/// 截图 — CDP Page.captureScreenshot
pub fn screenshot(&self, opts: ScreenshotOptions) -> Result<Vec<u8>>

/// 获取无障碍树 — CDP Accessibility.getFullAXTree
/// 对 LLM 最友好的页面理解方式
pub fn accessibility_tree(&self) -> Result<AccessibilityTree>

/// 获取页面元素 — CDP DOM.querySelector + DOM.getBoxModel
pub fn find_element(&self, selector: &str) -> Result<Element>
pub fn find_elements(&self, selector: &str) -> Result<Vec<Element>>

/// 点击 — 原生 send_mouse_click_event（非 CDP，延迟更低）
pub fn click(&self, x: i32, y: i32) -> Result<()>

/// 键入文本 — 原生 send_key_event 序列
pub fn type_text(&self, text: &str) -> Result<()>

/// 按键 — 原生 send_key_event
pub fn press_key(&self, key: Key) -> Result<()>

/// 滚动 — 原生 send_mouse_wheel_event
pub fn scroll(&self, x: i32, y: i32, delta_x: i32, delta_y: i32) -> Result<()>

/// 等待导航完成 — CDP Page.loadEventFired 事件
pub fn wait_for_navigation(&self, timeout_ms: u64) -> Result<()>

/// 等待元素出现 — CDP Runtime.evaluate 轮询
pub fn wait_for_selector(&self, selector: &str, timeout_ms: u64) -> Result<Element>

/// 执行 JS 并获取返回值 — CDP Runtime.evaluate（解决现有 TODO）
pub fn evaluate(&self, expression: &str) -> Result<Value>

/// 元素标注 — 在截图上叠加 bounding box 编号
/// 帮助 LLM 准确引用页面元素
pub fn annotate_screenshot(&self) -> Result<AnnotatedScreenshot>
```

**原生输入 vs CDP 输入的选择策略**:
- **点击/滚动/键盘**: 使用原生 `send_*_event` API — 延迟更低，更接近真实用户行为，能触发所有 JS 事件
- **截图/DOM 查询/JS 执行/等待**: 使用 CDP — 这些操作本身就是 CDP 的强项

**cef-rs 原生输入 API 确认**（全部可用）:

```rust
// BrowserHost 输入方法
fn send_key_event(&self, event: Option<&KeyEvent>);
fn send_mouse_click_event(&self, event: Option<&MouseEvent>, type_: MouseButtonType, mouse_up: c_int, click_count: c_int);
fn send_mouse_move_event(&self, event: Option<&MouseEvent>, mouse_leave: c_int);
fn send_mouse_wheel_event(&self, event: Option<&MouseEvent>, delta_x: c_int, delta_y: c_int);

// 输入类型
struct MouseEvent { x: c_int, y: c_int, modifiers: u32 }
struct KeyEvent { type_: KeyEventType, modifiers: u32, windows_key_code: c_int, native_key_code: c_int, character: char16_t, ... }
MouseButtonType::LEFT | MIDDLE | RIGHT
KeyEventType::RAWKEYDOWN | KEYDOWN | KEYUP | CHAR
```

**修改文件**:
- `wrymium/src/webview.rs` — WebView 增加 browser use 方法代理
- `wrymium/src/lib.rs` — 增加 `mod browser_use` 导出

### Phase 3: Tauri Command 层（集成层）~300 行 Rust

**目标**: 将 Browser Use 原语暴露为 Tauri commands，供前端 Agent 循环调用

**修改位置**: Tauri 应用的 commands 层（在使用 wrymium 的 Tauri 应用中）

```rust
// 示例 Tauri commands
#[tauri::command]
async fn browser_screenshot(webview: WebView) -> Result<String, String>  // base64 PNG

#[tauri::command]
async fn browser_click(webview: WebView, x: i32, y: i32) -> Result<(), String>

#[tauri::command]
async fn browser_type(webview: WebView, text: String) -> Result<(), String>

#[tauri::command]
async fn browser_navigate(webview: WebView, url: String) -> Result<(), String>

#[tauri::command]
async fn browser_accessibility_tree(webview: WebView) -> Result<String, String>

#[tauri::command]
async fn browser_evaluate(webview: WebView, expr: String) -> Result<String, String>

#[tauri::command]
async fn browser_annotated_screenshot(webview: WebView) -> Result<AnnotatedResult, String>
```

这一层是薄代理，主要做 Tauri 类型转换和错误处理。

### Phase 4: Agent Loop 参考实现 ~400 行 TS

**目标**: 提供一个前端 Agent 循环的参考实现，展示如何用 LLM 驱动浏览器

**新增文件**: `examples/browser-use-agent/` (示例应用)

```
Agent Loop:

  ┌─────────────────────────────────────┐
  │  1. Observe                         │
  │  ├─ screenshot → base64 PNG         │
  │  ├─ accessibility_tree → AXTree     │
  │  └─ (可选) annotate → 标注截图      │
  │                                     │
  │  2. Think (LLM)                     │
  │  ├─ 发送截图 + AXTree + 任务描述    │
  │  └─ LLM 返回下一步动作              │
  │                                     │
  │  3. Act                             │
  │  ├─ click(x, y)                     │
  │  ├─ type_text("...")                │
  │  ├─ scroll(...)                     │
  │  ├─ navigate(url)                   │
  │  └─ 等待页面稳定                    │
  │                                     │
  │  4. Verify                          │
  │  ├─ 截图确认操作结果                │
  │  ├─ 判断任务是否完成                │
  │  └─ 如遇 CAPTCHA → 暂停等待用户    │
  │                                     │
  │  ↩ 循环直到任务完成                  │
  └─────────────────────────────────────┘
```

**人机协作模式**（独特特性）:
- AI 操作时，用户实时看到浏览器变化
- 遇到 CAPTCHA/敏感操作 → AI 暂停，UI 提示用户接管
- 用户完成后点击"继续" → AI 恢复
- 用户随时可以手动操作浏览器（AI 自动暂停）

---

## 四、实现优先级与里程碑

```
Phase 1 (CDP Bridge)         ← 核心，后续全部依赖
  │
Phase 2 (Browser Use 原语)   ← 完成后 wrymium 即具备 browser use 能力
  │
Phase 3 (Tauri Commands)     ← 完成后可集成到任意 Tauri 应用
  │
Phase 4 (Agent 参考实现)     ← 端到端演示
```

建议从 Phase 1 开始，逐步推进。每个 Phase 完成后都可以独立验证。

---

## 五、关键技术决策

### Q1: CDP 调用模型 — 同步 vs 异步？

**决策: 异步 (channel-based)**

CDP 本质是异步的（send → observer 回调），用 `oneshot::channel` 桥接为 `async fn`。
在 Tauri command 中用 `async` 自然集成。

### Q2: 截图方案 — CDP vs OSR？

**决策: Phase 1-3 用 CDP，Phase 4+ 可选 OSR**

- CDP `Page.captureScreenshot` 实现简单，~10 行代码，足够 Browser Use 场景
- OSR `on_paint` 性能更好（每帧回调，无 base64 编码），但需要改变 WebView 渲染模式（从 windowed 改为 offscreen），影响较大
- 优先用 CDP 验证方案，后续有性能需求再切 OSR

### Q3: 原生输入 vs CDP 输入？

**决策: 混合模式 — 原生输入为主，CDP 为辅**

- 点击/滚动/键盘 → 原生 `send_*_event`（更真实、更快、不被检测）
- 复杂手势 → CDP `Input.dispatchTouchEvent`（原生 API 不支持 touch）

### Q4: A11y 树 vs DOM HTML？

**决策: 优先 A11y 树，DOM 作为补充**

- A11y 树结构化程度高，token 消耗少（通常比 HTML 少 5-10x）
- LLM 理解 A11y 树的能力已被 Stagehand/Playwright MCP 验证
- 必要时可通过 `Runtime.evaluate` 获取特定 DOM 片段

---

## 六、验证方案

### 单元验证（每个 Phase）
- Phase 1: 发送 `Runtime.evaluate("1+1")` 并验证返回 `2`
- Phase 2: `screenshot()` 返回有效 PNG；`click()` + `type_text()` 能填写表单
- Phase 3: 前端 `invoke("browser_screenshot")` 返回 base64 图片

### 端到端验证
- 启动 Tauri 示例应用 → 导航到 Google → 搜索 "wrymium" → 点击第一个结果 → 截图
- 衡量指标: 单次操作延迟 < 5ms，截图延迟 < 50ms，完整任务 < 10s

---

## 七、文件清单

| 操作 | 文件 | 说明 |
|------|------|------|
| 新增 | `wrymium/src/cdp.rs` | CDP Bridge 核心 |
| 新增 | `wrymium/src/browser_use.rs` | Browser Use 原语 |
| 修改 | `wrymium/src/webview.rs` | WebView 增加 cdp_bridge 字段和方法 |
| 修改 | `wrymium/src/lib.rs` | 增加模块导出 |
| 新增 | `examples/browser-use-agent/` | Agent 参考实现 |
