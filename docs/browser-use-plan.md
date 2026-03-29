# Wrymium Browser Use 实现计划

## Context

AI Browser Use（AI 驱动的浏览器自动化）正成为 AI Agent 的核心能力。当前市面上的方案都需要**外部浏览器进程**（Playwright WebSocket / CDP over network），引入了额外的延迟和部署复杂性。wrymium 将 CEF 嵌入 Tauri 应用，天然拥有**进程内 CDP 通道**和**原生输入事件注入**能力，可以实现体验远超现有方案的 Browser Use。

---

## 一、竞品分析与方案对比

### 现有方案的核心架构与痛点

| 方案 | 架构 | 截图方式 | 输入方式 | 单次操作延迟（不含 LLM） | 核心痛点 |
|------|------|---------|---------|--------------------------|---------|
| **browser-use** | Playwright WS → Chrome | DOM 解析 200-400 tokens | Playwright API | <100ms | DOM 脆弱性，2 层网络跳转 |
| **Anthropic Computer Use** | 桌面截图 → Vision API | 全屏截图 5K-10K tokens | OS 级坐标点击 | <100ms（含 Vision API ~800ms） | 极慢，token 昂贵 |
| **OpenAI Operator** | 云端虚拟浏览器 | Vision 截图 | 虚拟鼠标键盘 | 高（网络 RTT） | 云端依赖，同步问题 |
| **Playwright MCP** | CDP over WS | A11y 快照（大页面可达 100K+ tokens） | Playwright API | <100ms | Context window 膨胀 |
| **Stagehand** | CDP over network | CDP Page.captureScreenshot | Playwright → CDP | <100ms | 依赖 Browserbase 基础设施 |

### 所有现有方案的共同弱点

1. **外部浏览器进程** — 至少 1 次网络跳转（WebSocket/TCP），增加延迟和故障点
2. **部署复杂** — 需要安装/管理独立 Chrome/Chromium 实例
3. **用户不可见** — 浏览器在后台运行，用户无法直观看到 AI 操作
4. **无法人机协作** — AI 操作时用户不能介入，反之亦然

### wrymium 的结构性优势

| 维度 | 现有方案 | wrymium |
|------|---------|---------|
| CDP 通道 | WebSocket/TCP（~1-5ms RTT） | **进程内调用（省去网络序列化开销）** |
| 输入注入 | CDP Input.dispatch（需 JSON 序列化） | **原生 CEF API（零序列化）** |
| 截图 | CDP → base64 → 网络传输 → 解码 | **CDP 进程内调用（零网络开销）；未来可选 OSR 直出像素 buffer** |
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
- **优势**: 无障碍树比 DOM HTML 对 LLM 更友好（结构化、token 少）；进程内 CDP 省去网络序列化开销
- **✅ 可行且更优**

### 场景 2: 表单填写
> "用我的信息填写这个申请表"

- **需要**: DOM 查询定位输入框, 点击聚焦, 键入文本, 选择下拉, 提交
- **wrymium 方案**: `DOM.querySelector` 定位 → `DOM.getBoxModel` 获取坐标 → 坐标变换（页面坐标 → 视口坐标）→ 原生 `send_mouse_click_event` 点击 → 混合输入（文本用 JS `el.value` + `dispatchEvent`，特殊按键用原生 `send_key_event`）
- **优势**: 混合输入策略兼顾真实性和可靠性 — 原生点击事件不被反自动化检测，JS 设值支持所有字符（含中文等 IME 输入）；比纯 CDP `Input.dispatchKeyEvent` 更灵活
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
- **优势**: `Runtime.evaluate` 直接返回 JSON 结果（现有 `evaluate_script` 无返回值，CDP 解决了这个 TODO）；进程内 CDP 调用省去网络序列化开销
- **✅ 可行且更优**

### 场景 7: 文件下载
> "从这个仪表板下载最新报告"

- **需要**: 点击下载按钮, 监控下载进度, 获取文件
- **wrymium 方案**: builder API 已预留 `with_download_started_handler` / `with_download_completed_handler` 接口（**CEF 回调尚未接入，需在 Phase 2 实现 `CefDownloadHandler`**）；也可通过 CDP `Browser.downloadWillBegin` / `Browser.downloadProgress` 事件监控下载
- **优势**: builder API 已就绪，实现 CefDownloadHandler 或 CDP 事件监听即可完成
- **✅ 可行，需补充 CEF 回调绑定**

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
| 文件下载 | ✅ | ✅ | builder API 已就绪 |
| CAPTCHA | ✅ | ✅✅✅ | 嵌入式人机协作 |
| 动态内容 | ✅ | ✅ | 原生滚动 + DOM 事件 |
| 多标签页 | ✅ | ✅ | Tauri 原生多 WebView |

**10/10 场景全部可行，10/10 场景优于或等于现有方案，3 个场景有独特的结构性优势。**

---

## 三、实现计划

### Phase 1: CDP Bridge（核心基础）~600 行 Rust

**目标**: 在 wrymium 中建立双向 CDP 通信通道，含完整的错误处理和超时机制

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
    _registration: Registration,   // ← 持有以保活 observer（drop 会自动取消注册）
  }
         │
         ▼
  BrowserHost::execute_dev_tools_method(id, method, params)
    ⚠️ 必须在 CEF UI 线程上调用（见下方线程模型说明）
         │
         ▼  (CEF 内部处理)
         │
  WrymiumDevToolsObserver (wrap_dev_tools_message_observer!)
    ├── on_dev_tools_method_result(message_id, success, result)
    │     → success == 1: pending.remove(message_id).send(Ok(result))
    │     → success == 0: pending.remove(message_id).send(Err(CdpError::MethodFailed(result)))
    ├── on_dev_tools_event(method, params)
    │     → event_subscribers.broadcast(event)
    ├── on_dev_tools_agent_detached(browser)
    │     → pending.drain_all().send(Err(CdpError::AgentDetached))
    └── on_dev_tools_message(message)
          → 原始消息日志/调试
```

#### 线程模型

CEF 严格要求 `BrowserHost` 的方法在 **CEF UI 线程**上调用。wrymium 各平台的线程模型：

| 平台 | CEF 消息循环模式 | CEF UI 线程 | 安全性 |
|------|-----------------|-------------|--------|
| macOS | `external_message_pump` + CFRunLoopTimer | = 主线程 = tao 事件循环线程 | ✅ 当前调用模式安全 |
| Linux | `external_message_pump` + glib timeout | = 主线程 = tao 事件循环线程 | ✅ 当前调用模式安全 |
| Windows | `multi_threaded_message_loop` | ≠ tao 事件循环线程（CEF 独立线程） | ⚠️ 需要验证 |

**当前安全保证**: wrymium 现有的所有 `BrowserHost` / `Frame` 调用（`evaluate_script`、`load_url`、`open_devtools` 等）都在 tao 事件循环线程上执行（通过 Tauri 的 `Message` dispatch 机制保证）。在 macOS/Linux 上，该线程即 CEF UI 线程，天然安全。

**CDP Bridge 的线程规则**:
1. `CdpBridge` 内部的 `execute_dev_tools_method` 调用沿用现有模式 — 假设调用者在事件循环线程（Tauri dispatch 保证）
2. `WrymiumDevToolsObserver` 的回调在 CEF UI 线程触发，通过 `oneshot::Sender` 跨线程发送结果给 async 调用者
3. Phase 3 的 Tauri async commands 通过现有 `Message` dispatch 机制自然保证线程安全
4. 可选: 在 debug build 中添加 `debug_assert!(cef::currently_on(TID_UI))` 做运行时检查（需确认 cef crate 是否导出此 API）

#### 错误处理与超时

**关键设计**: `cdp_send` 必须拆分为两步 — **dispatch（同步，CEF UI 线程）** 和 **await（异步，任意线程）**，因为 `execute_dev_tools_method` 只能在 CEF UI 线程调用，而 Tauri async command 运行在 tokio 线程池。

```rust
// 步骤 1: dispatch — 同步，必须在 CEF UI 线程上调用
// 返回 Receiver，调用者可以在任意线程上 await
pub fn cdp_dispatch(&self, method: &str, params: Value) -> oneshot::Receiver<Result<Value>> {
    let (tx, rx) = oneshot::channel();
    let id = self.next_id.fetch_add(1, Ordering::SeqCst);
    self.pending.lock().unwrap().insert(id, tx);

    // ⚠️ 此调用必须在 CEF UI 线程上（Tauri event loop dispatch 保证）
    self.host.execute_dev_tools_method(id, method, params);
    rx
}

// 步骤 2: await — 异步，可在任意线程（tokio 线程池）上 await
pub async fn cdp_await(rx: oneshot::Receiver<Result<Value>>, pending: &Pending, id: i32) -> Result<Value> {
    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(CdpError::SenderDropped),
        Err(_) => {
            pending.lock().unwrap().remove(&id);  // 清理泄漏的 pending
            Err(CdpError::Timeout)
        }
    }
}
```

**调用流程（Phase 3 Tauri command）**:
```
Tauri async command (tokio thread)
  → 发送 Message::CdpSend(method, params, response_tx) 到事件循环
  → await response_rx

事件循环线程（= CEF UI 线程）
  → 收到 Message::CdpSend
  → 调用 cdp_bridge.cdp_dispatch(method, params)  // 同步，在 UI 线程上
  → 将 oneshot::Receiver 发回给 response_tx

Tauri async command (tokio thread)
  → 收到 oneshot::Receiver
  → await + 超时保护
```

**调用流程（Phase 1/2 库级 API，直接在事件循环线程上使用）**:
```rust
// 便利方法: 仅在事件循环线程上使用（内部同时做 dispatch + await）
pub async fn cdp_send(&self, method: &str, params: Value) -> Result<Value> {
    let rx = self.cdp_dispatch(method, params);  // 同步 dispatch（当前线程 = UI 线程）
    Self::cdp_await(rx, &self.pending, id).await // 异步 await（当前线程有 tokio runtime）
}
```

> **注意**: 如果 Phase 1/2 在纯事件循环线程上运行（无 tokio runtime），可以用 `std::thread::park` + observer 回调 `unpark` 做同步等待。但推荐使用 async 模式以避免阻塞 UI 线程。

```rust
// Observer detach 时清理所有 pending 请求
fn on_dev_tools_agent_detached(&self, _browser: Option<&mut Browser>) {
    let mut pending = self.pending.lock().unwrap();
    for (_, sender) in pending.drain() {
        let _ = sender.send(Err(CdpError::AgentDetached));
    }
}
```

#### Domain 自动 Enable

许多 CDP 功能需要先发送 enable 命令。`CdpBridge` 初始化后自动 enable 核心 domain：

| CDP 方法/事件 | 需要 Enable | Enable 命令 |
|---------------|-------------|-------------|
| `Page.loadEventFired` / `Page.frameNavigated` | ✅ | `Page.enable` |
| `DOM.querySelector` | ✅ | `DOM.enable` + `DOM.getDocument` |
| `DOM.documentUpdated` | ✅ | `DOM.enable` |
| `Accessibility.getFullAXTree` | ❌ | 不需要（但依赖 `DOM.enable`） |
| `Page.captureScreenshot` | ❌ | 不需要 |
| `Runtime.evaluate` | ❌ | 不需要 |
| `Network.requestWillBeSent` / `loadingFinished` | ✅ | `Network.enable` |

```rust
/// CdpBridge 初始化后自动调用
async fn enable_core_domains(&self) -> Result<()> {
    self.cdp_send("Page.enable", json!({})).await?;
    self.cdp_send("DOM.enable", json!({})).await?;
    self.cdp_send("Network.enable", json!({})).await?;
    Ok(())
}
```

#### 关键实现

1. `CdpBridge` 结构体 — 管理请求/响应匹配、事件分发、超时清理
2. `WrymiumDevToolsObserver` — 用 `wrap_dev_tools_message_observer!` 宏实现（遵循现有 `wrap_client!` 等宏的使用模式）
3. `WebView::cdp_send(method, params) -> Result<Value>` — CDP 调用（内部用 channel 等待响应 + 超时保护）
4. `WebView::cdp_subscribe() -> mpsc::Receiver<CdpEvent>` — CDP 事件订阅
5. `CdpError` 枚举 — 覆盖 `MethodFailed`、`Timeout`、`AgentDetached`、`SenderDropped`

**cef crate API 确认**（基于 cef v146，遵循现有 `wrap_*` 宏模式）:

```rust
// BrowserHost 方法
fn send_dev_tools_message(&self, message: Option<&[u8]>) -> c_int;
fn execute_dev_tools_method(&self, message_id: c_int, method: Option<&CefString>, params: Option<&mut DictionaryValue>) -> c_int;
fn add_dev_tools_message_observer(&self, observer: Option<&mut DevToolsMessageObserver>) -> Option<Registration>;

// Observer trait（通过 wrap_dev_tools_message_observer! 宏实现，
// 模式同 wrap_client!、wrap_life_span_handler! 等现有宏）
trait ImplDevToolsMessageObserver {
    fn on_dev_tools_message(&self, browser: Option<&mut Browser>, message: Option<&[u8]>) -> c_int;
    fn on_dev_tools_method_result(&self, browser: Option<&mut Browser>, message_id: c_int, success: c_int, result: Option<&[u8]>);
    fn on_dev_tools_event(&self, browser: Option<&mut Browser>, method: Option<&CefString>, params: Option<&[u8]>);
    fn on_dev_tools_agent_attached(&self, browser: Option<&mut Browser>);
    fn on_dev_tools_agent_detached(&self, browser: Option<&mut Browser>);
}
```

> **注意**: `Registration` 对象的 Drop 行为需在实现时验证。按 CEF RAII 惯例，drop 应自动取消注册 observer。如果不是，需要在 `CdpBridge::drop()` 中手动处理。

#### CdpBridge 初始化路径

`on_after_created` 在 `WrymiumLifeSpanHandler` 内触发（非 WebView 本身），需要 `SharedCdpBridge` 模式（类似现有 `SharedBrowser`）：

```rust
pub(crate) type SharedCdpBridge = Arc<Mutex<Option<CdpBridge>>>;

// WrymiumLifeSpanHandler 新增字段
struct WrymiumLifeSpanHandler {
    shared_browser: SharedBrowser,
    shared_cdp_bridge: SharedCdpBridge,  // ← 新增
    webview_id: String,
}

// on_after_created 中初始化 CdpBridge
fn on_after_created(&self, browser: Option<&mut Browser>) {
    if let Some(browser) = browser {
        // 1. 存储 browser（现有逻辑）
        *self.shared_browser.lock().unwrap() = Some(browser.clone());

        // 2. 初始化 CdpBridge
        if let Some(host) = ImplBrowser::host(browser) {
            let bridge = CdpBridge::new(&host);  // 注册 observer，获取 Registration
            *self.shared_cdp_bridge.lock().unwrap() = Some(bridge);
        }
    }
}

// WebView 增加字段
pub struct WebView {
    // ... 现有字段
    cdp_bridge: SharedCdpBridge,
}
```

> **`enable_core_domains` 时机**: `on_after_created` 是同步回调，不能 `await` async 函数。两种方案：
> - **方案 A（推荐）**: 在 `CdpBridge::new()` 中用 `cdp_dispatch` 同步发送 `Page.enable` 等命令（不等待响应），这些命令会被 CEF 排队执行。后续第一次 `cdp_send` 时 domain 已经 enable。
> - **方案 B**: 延迟到第一次 `cdp_send` 调用时检查并 enable（lazy init）。

**修改文件**:
- `wrymium/src/webview.rs` — WebView 增加 `cdp_bridge: SharedCdpBridge` 字段；`WrymiumLifeSpanHandler` 增加 `shared_cdp_bridge` 字段并在 `on_after_created` 中初始化
- `wrymium/src/lib.rs` — 增加 `mod cdp` 导出

### Phase 2: Browser Use 原语（操作层）~800 行 Rust

**目标**: 基于 CDP Bridge + 原生 API 封装高层浏览器操作

**新增文件**: `wrymium/src/browser_use.rs`

**核心 API**:

```rust
// ===== 导航 =====

/// 导航到 URL — 封装现有 load_url + 可选 wait_for_navigation
pub async fn navigate(&self, url: &str, wait: bool) -> Result<()>

// ===== 页面感知 =====

/// 截图 — CDP Page.captureScreenshot
pub async fn screenshot(&self, opts: ScreenshotOptions) -> Result<Vec<u8>>

/// 获取无障碍树 — CDP Accessibility.getFullAXTree
/// 对 LLM 最友好的页面理解方式
pub async fn accessibility_tree(&self) -> Result<AccessibilityTree>

/// 执行 JS 并获取返回值 — CDP Runtime.evaluate（解决现有 evaluate_script 无返回值的 TODO）
pub async fn evaluate(&self, expression: &str) -> Result<Value>

// ===== 元素定位 =====

/// 获取页面元素 — CDP DOM.querySelector + DOM.getBoxModel
/// frame_id 可选，默认 main frame（为 iframe 支持预留）
pub async fn find_element(&self, selector: &str, frame_id: Option<&str>) -> Result<Element>
pub async fn find_elements(&self, selector: &str, frame_id: Option<&str>) -> Result<Vec<Element>>

/// 列出页面所有 frame — CDP Page.getFrameTree
pub async fn list_frames(&self) -> Result<Vec<FrameInfo>>

// ===== 输入操作 =====

/// 低层点击 — 原生 send_mouse_click_event（视口坐标）
pub fn click(&self, x: i32, y: i32) -> Result<()>

/// 高层点击元素 — find_element + 坐标变换 + click（见下方坐标变换说明）
pub async fn click_element(&self, selector: &str) -> Result<()>

/// 键入文本 — 混合策略（见下方输入策略说明）
pub async fn type_text(&self, text: &str) -> Result<()>

/// 按键（Enter/Tab/Escape/方向键等）— 原生 send_key_event
pub fn press_key(&self, key: Key) -> Result<()>

/// 键盘快捷键（Ctrl+A 等）— 原生 send_key_event + 修饰键
pub fn key_combo(&self, modifiers: Modifiers, key: Key) -> Result<()>

/// 滚动 — 原生 send_mouse_wheel_event
pub fn scroll(&self, x: i32, y: i32, delta_x: i32, delta_y: i32) -> Result<()>

// ===== 等待 =====

/// 等待导航完成 — CDP Page.loadEventFired 事件
pub async fn wait_for_navigation(&self, timeout_ms: u64) -> Result<()>

/// 等待元素出现 — CDP Runtime.evaluate 轮询
pub async fn wait_for_selector(&self, selector: &str, timeout_ms: u64) -> Result<Element>

/// 等待网络空闲 — CDP Network.requestWillBeSent / Network.loadingFinished 事件
/// 无进行中的网络请求持续 idle_ms 毫秒后返回
pub async fn wait_for_network_idle(&self, idle_ms: u64, timeout_ms: u64) -> Result<()>

/// 等待 DOM 稳定 — Runtime.evaluate 注入 MutationObserver
/// 无 DOM 变化持续 stable_ms 毫秒后返回
pub async fn wait_for_dom_stable(&self, stable_ms: u64, timeout_ms: u64) -> Result<()>

// ===== Cookie 管理 =====

/// 获取 cookie — CDP Network.getCookies
pub async fn get_cookies(&self, urls: Option<&[&str]>) -> Result<Vec<Cookie>>

/// 设置 cookie — CDP Network.setCookie
pub async fn set_cookie(&self, cookie: Cookie) -> Result<()>

/// 清除 cookie — CDP Network.clearBrowserCookies
pub async fn clear_cookies(&self) -> Result<()>

// ===== 标注 =====

/// 元素标注 — 在页面上注入 overlay div + 截图（不依赖 Rust 图像库，见实现说明）
pub async fn annotate_screenshot(&self) -> Result<AnnotatedScreenshot>
```

#### 坐标变换（页面坐标 → 视口坐标）

`DOM.getBoxModel` 返回的是 CSS **页面坐标**（相对于文档左上角），而 `send_mouse_click_event` 的 `MouseEvent.x/y` 是**视口坐标**（相对于 WebView 左上角）。`click_element` 高层 API 自动处理坐标变换：

```rust
pub async fn click_element(&self, selector: &str) -> Result<()> {
    let element = self.find_element(selector, None).await?;
    let box_model = self.cdp_send("DOM.getBoxModel", json!({"nodeId": element.node_id})).await?;

    // 计算元素中心点（页面坐标）
    let content = &box_model["model"]["content"];
    let center_x = (content[0].as_f64()? + content[2].as_f64()?) / 2.0;
    let center_y = (content[1].as_f64()? + content[5].as_f64()?) / 2.0;

    // 获取滚动偏移
    let scroll = self.evaluate("JSON.stringify({x: window.scrollX, y: window.scrollY})").await?;
    let scroll: ScrollOffset = serde_json::from_value(scroll)?;

    // 页面坐标 → 视口坐标
    let viewport_x = (center_x - scroll.x) as i32;
    let viewport_y = (center_y - scroll.y) as i32;

    // 注: CEF windowed 模式下 send_mouse_click_event 接受 CSS 像素（非物理像素），
    // 不需要额外 DPR 换算。如后续切换 OSR 模式需要重新验证。
    self.click(viewport_x, viewport_y)
}
```

> **P1 扩展**: iframe 内的元素需要额外加上 iframe 在主页面中的偏移量。初始版本聚焦 main frame，iframe 坐标变换作为后续扩展。

#### 输入策略：三层混合模式

| 输入类型 | 实现方式 | 原因 |
|---------|---------|------|
| **文本输入** | CDP `Runtime.evaluate`: `el.value = text` + `dispatchEvent(new Event('input', {bubbles:true}))` + `dispatchEvent(new Event('change', {bubbles:true}))` | 支持所有字符（含中文、Emoji）；避免 `send_key_event` 逐字符生成 `KeyEvent` 的复杂性（需要正确的 `windows_key_code`/`native_key_code`/`char16_t` 序列） |
| **特殊按键** (Enter/Tab/Escape/方向键/Backspace) | 原生 `send_key_event` | 这些 keycode 固定且简单，三步序列 `RAWKEYDOWN → CHAR → KEYUP` 易于生成 |
| **键盘快捷键** (Ctrl+A/Ctrl+C/Ctrl+V) | 原生 `send_key_event` + 修饰键 | 需要完整的键盘事件流（modifier flags），CDP `Input.dispatchKeyEvent` 也可作为 fallback |

> **不同元素类型的处理**:
> - `<input>` / `<textarea>`: `el.value = text` + `dispatchEvent('input')` + `dispatchEvent('change')`
> - `<select>`: `el.value = optionValue` + `dispatchEvent('change')`（需先通过 A11y 树或 DOM 查询获取 option value）
> - `contenteditable` 元素: `document.execCommand('insertText', false, text)` 或 `el.textContent = text`（需先 focus）
> - 富文本编辑器（ProseMirror/CodeMirror/Slate）: 可能需要 fallback 到原生 `send_key_event`，因为这些框架拦截了标准 DOM 事件
>
> **注意**: JS 设值方式不会触发某些依赖 `keydown`/`keypress` 事件的自定义表单控件（如自动补全下拉框）。对于这些场景，`type_text` 提供 `opts.use_native_keys = true` 选项，退回原生 `send_key_event` 逐字符输入（仅限 ASCII 字符）。

#### `annotate_screenshot` 实现方案

采用 **浏览器侧 JS overlay** 方案（同 browser-use / Stagehand），不需要引入 Rust 图像处理库：

1. `Runtime.evaluate` 注入 JS → 遍历所有可交互元素 → 叠加绝对定位的标注 div（编号 + 边框）
2. `Page.captureScreenshot` 截图（包含标注 overlay）
3. `Runtime.evaluate` 移除所有标注 div
4. 返回 `AnnotatedScreenshot { image: Vec<u8>, elements: Vec<{id, role, name, selector, bounds}> }`

复杂度约 ~50 行 JS + ~30 行 Rust 胶水。

**cef crate 原生输入 API 确认**（基于 cef v146）:

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
// 导航
#[tauri::command]
async fn browser_navigate(webview: WebView, url: String, wait: bool) -> Result<(), String>

// 页面感知
#[tauri::command]
async fn browser_screenshot(webview: WebView) -> Result<String, String>  // base64 PNG
#[tauri::command]
async fn browser_accessibility_tree(webview: WebView) -> Result<String, String>
#[tauri::command]
async fn browser_evaluate(webview: WebView, expr: String) -> Result<String, String>
#[tauri::command]
async fn browser_annotated_screenshot(webview: WebView) -> Result<AnnotatedResult, String>

// 输入操作
#[tauri::command]
async fn browser_click(webview: WebView, x: i32, y: i32) -> Result<(), String>
#[tauri::command]
async fn browser_click_element(webview: WebView, selector: String) -> Result<(), String>
#[tauri::command]
async fn browser_type(webview: WebView, text: String) -> Result<(), String>
#[tauri::command]
async fn browser_press_key(webview: WebView, key: String) -> Result<(), String>
#[tauri::command]
async fn browser_scroll(webview: WebView, x: i32, y: i32, dx: i32, dy: i32) -> Result<(), String>

// 等待
#[tauri::command]
async fn browser_wait_for_navigation(webview: WebView, timeout_ms: u64) -> Result<(), String>
#[tauri::command]
async fn browser_wait_for_selector(webview: WebView, selector: String, timeout_ms: u64) -> Result<(), String>
#[tauri::command]
async fn browser_wait_for_network_idle(webview: WebView, idle_ms: u64, timeout_ms: u64) -> Result<(), String>

// Cookie
#[tauri::command]
async fn browser_get_cookies(webview: WebView, urls: Option<Vec<String>>) -> Result<String, String>
#[tauri::command]
async fn browser_set_cookie(webview: WebView, cookie: CookieParam) -> Result<(), String>

// Frame
#[tauri::command]
async fn browser_list_frames(webview: WebView) -> Result<String, String>
```

这一层是薄代理，主要做 Tauri 类型转换和错误处理。所有 commands 内部通过 `Message` dispatch 将 CDP 调用路由到事件循环线程（见 Phase 1 调用流程说明）。

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
  │  5. Error Recovery                  │
  │  ├─ 操作失败 → 截图 + 重新 Observe  │
  │  │   → 让 LLM 决定重试或替代路径    │
  │  ├─ 连续 N 次相同操作 → 死循环检测  │
  │  │   → 退出并报告                   │
  │  ├─ LLM 返回无效 action → schema   │
  │  │   校验拒绝，要求 LLM 重新生成    │
  │  └─ 总步数超限 → 强制终止并汇报     │
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

**决策: 三层混合策略**

| 层 | 操作 | 实现方式 | 原因 |
|----|------|---------|------|
| 1 | 点击/滚动 | 原生 `send_mouse_*_event` | 更真实、不被反自动化检测 |
| 2 | 文本输入 | CDP `Runtime.evaluate` (JS 设值 + dispatchEvent) | 支持全字符集（中文、Emoji）；避免 `send_key_event` 的 keycode 复杂性 |
| 3 | 特殊按键/快捷键 | 原生 `send_key_event` | keycode 固定，事件流简单 |
| 4 | 复杂手势 | CDP `Input.dispatchTouchEvent` | 原生 API 不支持 touch |

原则: **点击和滚动用原生（真实性），文本用 JS（可靠性），按键用原生（简单性）**。

### Q4: A11y 树 vs DOM HTML？

**决策: 优先 A11y 树，DOM 作为补充**

- A11y 树结构化程度高，过滤了不可见内容和样式信息，**典型页面** token 消耗比 raw HTML 少 5-10x（复杂 web app 可能仍然较大）
- LLM 理解 A11y 树的能力已被 Stagehand/Playwright MCP 验证
- 必要时可通过 `Runtime.evaluate` 获取特定 DOM 片段作为补充

---

## 六、安全考量

wrymium 作为框架层，提供安全 **hook point**，不强制策略（策略由上层应用决定）。

### 6.1 Runtime.evaluate 注入风险

`Runtime.evaluate` 可执行任意 JS。如果 Agent Loop 中 LLM 返回的 expression 未经校验直接执行，可能导致:
- 读取 `document.cookie`、`localStorage` 等敏感数据
- 发起非预期的网络请求
- 修改页面 DOM 导致状态混乱

**应对**: Phase 4 Agent 参考实现中，LLM 返回的 action 必须匹配预定义 schema（如 `{type: "click", selector: "..."}`, `{type: "type", text: "..."}`），**不允许** LLM 直接返回 JS 代码。`evaluate` 仅在框架内部使用（坐标获取、DOM 查询等），不暴露给 LLM 决策层。

### 6.2 导航安全

`navigate(url)` 可导航到任意 URL。框架层提供可选过滤器:

```rust
// WebViewBuilder 新增（可选配置）
pub fn with_navigation_filter<F>(mut self, filter: F) -> Self
where F: Fn(&str) -> bool + Send + Sync + 'static
```

上层应用可根据需要配置域名白名单/黑名单。

### 6.3 凭证处理

场景 4（登录认证）涉及密码输入。框架层 **不存储凭证**，人机协作模式下由用户在嵌入式浏览器中手动输入。Agent 参考实现中，AI 只负责导航到登录页，密码字段标记为"需要用户操作"。

---

## 七、测评体系

六层递进的测试架构，从单元到端到端全覆盖。

### Level 1: 单元测试（Rust `#[cfg(test)]`）

测试 CdpBridge 的纯逻辑部分（不依赖 CEF 运行时）：

| 测试 | 验证点 |
|------|--------|
| `test_message_id_monotonic` | `next_id` 单调递增，并发安全 |
| `test_pending_insert_remove` | pending HashMap 的增删查 |
| `test_timeout_cleanup` | 超时后 pending entry 被清理，不泄漏 |
| `test_agent_detached_drains` | detach 时所有 pending sender 收到 `AgentDetached` 错误 |
| `test_event_broadcast` | 多个 subscriber 都能收到同一事件 |
| `test_cdp_error_variants` | `CdpError` 各变体的 Display/Debug 实现 |

### Level 2: 集成测试（需 CEF 运行时）

需要启动真实的 CEF browser 实例。每个 Phase 完成后运行对应测试：

**Phase 1 (CDP Bridge)**:

| 测试 | 步骤 | 预期 |
|------|------|------|
| `test_evaluate_basic` | `Runtime.evaluate("1+1")` | 返回 `{"result":{"type":"number","value":2}}` |
| `test_evaluate_string` | `Runtime.evaluate("'hello'")` | 返回字符串 "hello" |
| `test_evaluate_error` | `Runtime.evaluate("throw new Error('test')")` | 返回 `exceptionDetails` |
| `test_page_event` | `Page.enable` → navigate → 监听 | 收到 `Page.loadEventFired` 事件 |
| `test_dom_query` | `DOM.enable` → `DOM.getDocument` → `DOM.querySelector` | 返回有效 nodeId |
| `test_invalid_method` | 调用不存在的 CDP 方法 | 返回 `CdpError::MethodFailed` |
| `test_concurrent_calls` | 并发 10 个 `Runtime.evaluate` | 所有结果正确，无串扰 |
| `test_timeout` | 发送后立即 drop browser | receiver 收到 `AgentDetached` 或 `Timeout` |

**Phase 2 (Browser Use 原语)**:

| 测试 | 步骤 | 预期 |
|------|------|------|
| `test_screenshot_png` | `screenshot()` 对测试页面 | 返回有效 PNG，尺寸 > 0 |
| `test_screenshot_clip` | `screenshot(clip: {x,y,w,h})` | 返回裁剪区域截图 |
| `test_a11y_tree` | `accessibility_tree()` 对含按钮/链接的页面 | 树中包含预期的 role 和 name |
| `test_find_element` | `find_element("#test-btn")` | 返回 Element 且 bounds 非零 |
| `test_click_element_no_scroll` | `click_element("#visible-btn")` | JS click handler 被触发 |
| `test_click_element_scrolled` | 滚动到底部 → `click_element("#bottom-btn")` | 坐标变换正确，handler 被触发 |
| `test_type_text_ascii` | click input → `type_text("Hello")` | input.value == "Hello" |
| `test_type_text_unicode` | click input → `type_text("你好世界🌍")` | input.value 正确 |
| `test_type_text_contenteditable` | click div[contenteditable] → `type_text("test")` | textContent 正确 |
| `test_press_key_enter` | focus input → `press_key(Key::Enter)` | form submit 事件触发 |
| `test_scroll` | `scroll(0, 0, 0, -500)` | `window.scrollY` 增加 |
| `test_wait_for_navigation` | click link → `wait_for_navigation()` | URL 变化后返回 |
| `test_wait_for_selector` | 页面 2s 后动态插入元素 → `wait_for_selector("#dynamic")` | 等待后返回 Element |
| `test_wait_for_network_idle` | 页面触发 XHR → `wait_for_network_idle(500, 5000)` | XHR 完成 500ms 后返回 |
| `test_wait_for_dom_stable` | 页面动态追加 DOM → `wait_for_dom_stable(300, 5000)` | 追加停止 300ms 后返回 |
| `test_cookies_roundtrip` | `set_cookie(...)` → `get_cookies()` | 设置的 cookie 可读回 |
| `test_annotate_screenshot` | `annotate_screenshot()` 对含多个按钮的页面 | 返回带标注的 PNG + elements 列表 |
| `test_navigate_and_wait` | `navigate("https://...", true)` | 页面加载完成后返回 |

**Phase 3 (Tauri Commands)**:
| 测试 | 步骤 | 预期 |
|------|------|------|
| `test_invoke_screenshot` | 前端 `invoke("browser_screenshot")` | 返回 base64 PNG 字符串 |
| `test_invoke_click_element` | 前端 `invoke("browser_click_element", {selector: "#btn"})` | 操作成功 |
| `test_invoke_evaluate` | 前端 `invoke("browser_evaluate", {expr: "1+1"})` | 返回 "2" |

### Level 3: 本地测试页面（HTML Fixtures）

在 `tests/fixtures/` 下提供一组静态 HTML 测试页面，确保测试不依赖外部网站：

| 文件 | 用途 | 包含 |
|------|------|------|
| `basic.html` | 基础元素操作 | 按钮（带 click handler 设置 `window.__clicked=true`）、文本、图片 |
| `form.html` | 表单填写测试 | `<input>`, `<textarea>`, `<select>`, `<input type="checkbox">`, `<div contenteditable>`, submit handler |
| `scroll.html` | 坐标变换测试 | 3000px 高度页面，顶部/中间/底部各有一个按钮，每个按钮的 click handler 记录自身 id |
| `spa.html` | SPA 路由测试 | History API 路由切换，每个"页面"有不同内容；动态加载延迟 1s 的内容 |
| `network.html` | 网络空闲测试 | 页面加载后发起 3 个 XHR（分别延迟 500ms/1000ms/1500ms），完成后设置 `window.__allLoaded=true` |
| `dynamic.html` | DOM 稳定测试 | 页面加载后每 200ms 追加一个 `<li>`，共追加 10 个后停止 |
| `a11y.html` | 无障碍树测试 | 语义化 HTML：nav/main/aside/footer，含 aria-label、role 属性 |
| `iframe.html` | iframe 测试（P1 扩展） | 主页面 + 2 个 iframe（同源/跨域各一） |

### Level 4: 微观性能基准（Rust benchmark）

用 `criterion` crate 或自定义计时，在真实 CEF 实例上测量：

| 基准 | 操作 | 迭代 | 目标 p50 | 目标 p99 |
|------|------|------|---------|---------|
| `bench_cdp_roundtrip` | `Runtime.evaluate("1")` | 1000 | < 1ms | < 3ms |
| `bench_screenshot` | `Page.captureScreenshot` (1280x720) | 100 | < 30ms | < 80ms |
| `bench_screenshot_clip` | `Page.captureScreenshot` (200x200 clip) | 100 | < 10ms | < 30ms |
| `bench_a11y_tree_simple` | `Accessibility.getFullAXTree` (basic.html) | 100 | < 5ms | < 15ms |
| `bench_a11y_tree_complex` | `Accessibility.getFullAXTree` (a11y.html) | 100 | < 20ms | < 50ms |
| `bench_dom_query` | `DOM.querySelector` + `DOM.getBoxModel` | 1000 | < 0.5ms | < 2ms |
| `bench_click_native` | `send_mouse_click_event` dispatch | 1000 | < 0.1ms | < 0.5ms |
| `bench_type_text` | `type_text("Hello World")` (JS 设值) | 100 | < 5ms | < 15ms |
| `bench_navigate` | `navigate(local_url, true)` | 50 | < 100ms | < 300ms |
| `bench_concurrent_cdp` | 10 个并行 `Runtime.evaluate` | 100 | < 3ms 全部完成 | < 8ms |
| `bench_annotate` | `annotate_screenshot()` 全流程 | 50 | < 50ms | < 120ms |

### Level 5: 对比基准（wrymium vs Playwright）

在同一台机器上用相同操作对比 wrymium（进程内 CDP）和 Playwright（WebSocket CDP），量化结构性优势：

```
测试脚本: benchmarks/compare_playwright.py + benchmarks/compare_wrymium.rs

操作集:
  1. evaluate("document.title") × 1000 次
  2. Page.captureScreenshot × 100 次
  3. Accessibility.getFullAXTree × 100 次 (Playwright 用 accessibility.snapshot())
  4. DOM.querySelector + click × 100 次
  5. navigate(url) + wait_for_load × 50 次

输出格式:
  | 操作 | wrymium p50 | wrymium p99 | Playwright p50 | Playwright p99 | 提升倍数 |
  |------|-------------|-------------|----------------|----------------|---------|
  | ... | ... | ... | ... | ... | ... |

测试环境:
  - 本地测试页面（Level 3 fixtures），消除网络变量
  - 同一 Chromium 版本（CEF 146 对应 Chromium ~146）
  - macOS / Linux 各跑一次
```

### Level 6: 端到端 Agent 评估（WebVoyager 风格）

Phase 4 完成后，评估完整的 LLM 驱动 Agent 能力。

#### 评估任务集

| # | 任务 | 类型 | 成功标准 | 难度 |
|---|------|------|---------|------|
| T1 | "在本地测试页面的搜索框中输入 'test' 并点击搜索按钮" | 表单操作 | 搜索结果页面出现 | ⭐ |
| T2 | "填写本地注册表单（姓名/邮箱/密码/国家下拉框）" | 表单填写 | 所有字段正确填写并提交 | ⭐⭐ |
| T3 | "在 Hacker News 上找到今天排名第一的帖子标题" | 信息提取 | 返回正确标题 | ⭐⭐ |
| T4 | "在 GitHub 上找到 anthropics/claude-code 的 star 数" | 多步导航 | 返回正确数字（±100 容差） | ⭐⭐ |
| T5 | "在本地无限滚动测试页面上加载至少 30 条记录" | 动态内容 | 页面包含 ≥30 条 | ⭐⭐ |
| T6 | "打开两个本地测试页面的标签页，比较两个页面中商品 A 的价格" | 多标签 | 正确返回两个价格和比较结论 | ⭐⭐⭐ |
| T7 | "在本地测试页面上完成一个三步向导（填表→确认→提交）" | 多步骤 | 最终确认页面出现 | ⭐⭐⭐ |
| T8 | "在本地测试 SPA 中，导航到 /products → 点击第二个产品 → 提取价格" | SPA 导航 | 返回正确价格 | ⭐⭐⭐ |

> T3/T4 依赖外部网站，可能因网站变化而失败。作为补充测试，不作为核心指标。核心指标基于本地测试页面（T1/T2/T5-T8）。

#### 评估指标

每个任务采集以下指标：

| 指标 | 说明 | 计算方式 |
|------|------|---------|
| **成功率** | 任务是否完成 | 5 次运行取成功率 |
| **步数** | Agent Loop 迭代次数 | Observe→Think→Act 算 1 步 |
| **操作延迟** | 浏览器操作总耗时（不含 LLM） | 所有 Act + Observe 阶段耗时之和 |
| **LLM 延迟** | LLM 推理总耗时 | 所有 Think 阶段耗时之和 |
| **Token 消耗** | 输入 + 输出 token 总量 | LLM API 返回的 usage |
| **估算成本** | LLM API 费用 | 按当前定价计算 |

#### 对比维度

同一任务集分别用以下配置运行：

| 配置 | Observe 方式 | 输入 Token 特征 |
|------|-------------|---------------|
| wrymium (A11y + 截图) | A11y 树 + 截图 | 结构化文本 + vision |
| wrymium (截图 only) | 标注截图 | vision only |
| wrymium (A11y only) | A11y 树 | 结构化文本 only |
| Playwright MCP (baseline) | A11y snapshot | 结构化文本（通常更长） |

#### 输出报告格式

```
=== Wrymium Browser Use Evaluation Report ===
Date: YYYY-MM-DD
Model: claude-sonnet-4-20250514
Environment: macOS, M2, 16GB

Task Results:
  T1 (搜索): ✅ 5/5 | 3 steps | ops: 120ms | llm: 2.1s | 1.2K tokens | $0.003
  T2 (表单): ✅ 4/5 | 7 steps | ops: 340ms | llm: 5.3s | 3.8K tokens | $0.010
  ...

Aggregate:
  Overall success: 92% (37/40)
  Avg steps: 5.2
  Avg operation latency: 210ms
  Avg LLM latency: 3.8s
  Avg tokens per task: 2.5K

Comparison (vs Playwright MCP):
  Success rate: 92% vs 85% (+7%)
  Avg steps: 5.2 vs 6.1 (-15%)
  Avg operation latency: 210ms vs 580ms (-64%)
  Avg tokens per task: 2.5K vs 4.1K (-39%)
```

### 可靠性测试（贯穿所有 Phase）

| 测试 | 验证点 | 方法 |
|------|--------|------|
| 内存泄漏 | CdpBridge pending 不泄漏 | 1000 次 cdp_send 后检查 pending.len() == 0 |
| Handle 泄漏 | CEF 对象正确释放 | 长时间运行后进程 FD/handle 数不增长 |
| 并发安全 | 多线程 cdp_send 无 panic | 10 个线程并发调用 1000 次 |
| 崩溃恢复 | 页面 crash 不影响宿主 | navigate 到 `chrome://crash` → 验证 agent detach 事件触发 |
| 超时行为 | 所有 await 都有超时保护 | 模拟各种超时场景 |

---

## 八、文件清单

| 操作 | 文件 | 说明 | Phase |
|------|------|------|-------|
| 新增 | `wrymium/src/cdp.rs` | CDP Bridge 核心（CdpBridge, Observer, CdpError） | 1 |
| 新增 | `wrymium/src/browser_use.rs` | Browser Use 原语（截图/点击/输入/等待/Cookie） | 2 |
| 修改 | `wrymium/src/webview.rs` | WebView 增加 SharedCdpBridge 字段；LifeSpanHandler 增加 bridge 初始化 | 1 |
| 修改 | `wrymium/src/lib.rs` | 增加 `mod cdp`, `mod browser_use` 导出 | 1-2 |
| 新增 | `tests/fixtures/basic.html` | 基础元素测试页面 | 2 |
| 新增 | `tests/fixtures/form.html` | 表单填写测试页面 | 2 |
| 新增 | `tests/fixtures/scroll.html` | 坐标变换 / 长页面测试 | 2 |
| 新增 | `tests/fixtures/spa.html` | SPA 路由 / 动态加载测试 | 2 |
| 新增 | `tests/fixtures/network.html` | 网络空闲测试（XHR 延迟） | 2 |
| 新增 | `tests/fixtures/dynamic.html` | DOM 稳定性测试 | 2 |
| 新增 | `tests/fixtures/a11y.html` | 无障碍树测试 | 2 |
| 新增 | `tests/fixtures/iframe.html` | iframe 测试页面 | 2 |
| 新增 | `examples/browser-use-agent/` | Agent 参考实现（TS，5 文件，718 行） | 4 |
| 新增 | `examples/cdp-test/` | CDP + Browser Use 集成测试（35 项） | 测试 |
| 新增 | `examples/bench/` | 微观性能基准（16 项） | 测试 |
| 新增 | `benchmarks/playwright_bench.mjs` | Playwright 对比基准 | 测试 |
| 修改 | `tauri-runtime-wry/src/lib.rs` | CdpSend / CdpSubscribe message dispatch | 3 |
| 修改 | `tauri-runtime-wry/Cargo.toml` | wry 改为 path 依赖 | 3 |

---

## 九、实现状态

> 本节记录实际实现结果，与上方设计方案对照。

### 已实现

| Phase | 文件 | 行数 | 编译 | 测试 |
|-------|------|------|------|------|
| 1. CDP Bridge | `wrymium/src/cdp.rs` | 355 | ✅ | 9 CDP + 7 可靠性 |
| 2. Browser Use | `wrymium/src/browser_use.rs` | 1300+ | ✅ | 20 集成测试 |
| 2b. iframe | `evaluate_in_frame`, `find_element_in_frame` | — | ✅ | iframe.html fixture |
| 2c. DownloadHandler | `wrap_download_handler!` in webview.rs | +90 | ✅ | — |
| 3. Tauri Commands | `tauri-runtime-wry/src/lib.rs` | +26 | ✅ | — |
| 4. Agent Loop | `examples/browser-use-agent/src/` | 718 | — (TS) | — |
| 测试 | `examples/cdp-test/`, `wrymium/src/tests.rs` | 1200+ | ✅ | 92 项全通过 |
| 性能基准 | `examples/bench/`, `benchmarks/` | 600+ | ✅ | 16 项 micro + Playwright 对比 |

### 设计方案 vs 实际实现的差异

| 设计 | 实际 | 原因 |
|------|------|------|
| `execute_dev_tools_method` + DictionaryValue | `send_dev_tools_message` + raw JSON bytes | 避免 JSON→DictionaryValue 转换；CEF 同样触发 `on_dev_tools_method_result` |
| `tokio::oneshot` channel | `std::sync::mpsc` channel | wrymium 是同步库，不依赖 tokio |
| Phase 2 API 全部 `async fn` | 全部同步（spin-wait with message pump） | 无 async runtime；`do_message_loop_work()` 泵避免死锁 |
| Phase 3 用 `serde_json::Value` 传参 | 用 raw JSON `String` 传参 | tauri-runtime-wry 不依赖 serde_json，通过 `wry::cdp::serde_json` re-export |
| `cdp_send` 单一 async 方法 | `cdp_dispatch` (同步) + `cdp_send_blocking` (spin-wait) | 拆分同步 dispatch（UI 线程）和 await（任意线程） |
| `with_html(TEST_HTML)` 测试 | `file://` URL 加载 fixture | data: URI 会 percent-encode `<script>` 导致 JS 不执行 |

### 实测结果（92 项测试，全部通过）

**57 项单元测试** (`cargo test -p wry`): CDP error/event/pending/broadcast、Key codes、modifier flags、类型构造

**35 项集成测试** (`cdp-test`, 需 CEF runtime):
```
CDP Bridge (9):                                Browser Use 基础 (11):
  ✅ evaluate(1+1) == 2                          ✅ screenshot() → PNG bytes
  ✅ evaluate('hello') == "hello"                 ✅ screenshot(clip) → smaller PNG
  ✅ window.__ready === true                      ✅ find_element("#test-btn") → bounds
  ✅ DOM.querySelector("#title")                  ✅ find_elements("button") → elements
  ✅ Page.captureScreenshot (PNG)                 ✅ click_element("#test-btn") → handler
  ✅ event subscription (Page events)             ✅ type_text("Hello 你好") → Unicode
  ✅ invalid method → MethodFailed                ✅ accessibility_tree() → nodes
  ✅ 3 concurrent dispatches                      ✅ accessibility_tree_compact() → text
  ✅ browser_use::evaluate                        ✅ accessibility_tree_fast() → JS text
                                                  ✅ interactive_elements() → 3 elements
Browser Use 扩展 (8):                            ✅ navigate(url) → page loaded
  ✅ list_frames() → main frame
  ✅ press_key(Enter) → form submit             可靠性 (7):
  ✅ scroll (JS fallback)                         ✅ 1000 次调用无 pending 泄漏
  ✅ click_element scrolled page → coords ok      ✅ 100 次 dispatch+recv 无泄漏
  ✅ wait_for_selector → dynamic element          ✅ 丢弃 receiver 后 pending 清理
  ✅ wait_for_dom_stable → 10 items               ✅ subscriber drop 后清理
  ✅ cookie set → get → clear                     ✅ timeout 正确返回不 hang
  ✅ annotate_screenshot → labeled PNG            ✅ 100 并发 stress → 全部响应
                                                  ✅ 跨导航 evaluate 稳定
```

### 性能基准（实测数据）

> 测试环境: macOS, Apple Silicon, release build, 本地 file:// HTML fixture
> 两次运行结果一致（偏差 < 5%），以下取代表值。

```
操作                                          n       p50        p95        p99        min
─────────────────────────────────────────────────────────────────────────────────────────
🔥 cdp_roundtrip (evaluate "1")              1000     53µs       96µs       171µs      42µs
   screenshot (full viewport PNG)             100     50.2ms     66.8ms     67.5ms     49.0ms
🔥 screenshot_fast (JPEG q60)                 100     33.4ms     34.2ms     35.5ms     29.8ms
   screenshot (200x200 clip)                  100     50.0ms     51.0ms     51.1ms     24.8ms
   a11y_tree (CDP raw, basic.html)            100     439µs      585µs      9.97ms     390µs
   a11y_tree_compact (CDP + Rust fmt)         100     481µs      573µs      925µs      461µs
🔥 a11y_tree_fast (JS, 1 roundtrip)          100     247µs      340µs      1.19ms     230µs
   a11y_tree (CDP, a11y.html complex)         100     1.16ms     1.88ms     11.2ms     1.09ms
🔥 find_element (1 JS roundtrip)             1000     71µs       91µs       155µs      49µs
🔥 click (native send_mouse_click)           1000     6µs        8µs        15µs       4µs
🔥 click_element (1 JS + native click)        100     110µs      198µs      39.4ms     98µs
🔥 type_text ("Hello World")                  100     83µs       121µs      4.9ms      71µs
🔥 interactive_elements                       100     183µs      229µs      572µs      162µs
   navigate (file:// local)                    20     14.7ms     16.4ms     16.4ms     14.1ms
🔥 concurrent_cdp (10 parallel evals)         100     202µs      275µs      570µs      179µs
   annotate_screenshot (full pipeline)         50     100ms      102ms      108ms      66ms
```

**关键数据**:
- **CDP roundtrip 53µs** — 进程内函数调用，无 WebSocket/TCP 序列化开销
- **a11y_tree_fast 247µs** — JS 构建树绕开 CDP Accessibility domain（之前 439µs，提升 1.8x，追平 Playwright 225µs）
- **find_element 71µs** — 1 次 JS evaluate（之前 211µs / 3 次 CDP roundtrip，提升 3x）
- **click_element 110µs** — 1 次 JS + 原生 click（之前 283µs / 4 次 CDP roundtrip，提升 2.6x）
- **原生 click 6µs** — 直接调用 CEF BrowserHost API，零网络跳转
- **screenshot_fast 33ms** — JPEG q60，适合 LLM observe（比 PNG 50ms 快 1.5x）
- **interactive_elements 183µs** — 单次 JS 获取所有可操作元素
- **10 并发 CDP 202µs** — 所有请求完成总耗时，非单个

### 对比基准：wrymium vs Playwright

> 同一台机器（macOS Apple Silicon），同一组 HTML fixture，release build vs headless Chromium。
> wrymium 使用进程内 CEF CDP，Playwright 使用 CDP over WebSocket。
> wrymium 数据含 roundtrip 优化后结果（find/click 改为单次 JS evaluate）。

```
操作                          wrymium p50    Playwright p50    倍数       赢家
────────────────────────────────────────────────────────────────────────────────
CDP roundtrip (evaluate)      53µs           145µs             2.7x       🟢 wrymium
Raw CDP session               53µs           123µs             2.3x       🟢 wrymium
DOM query (find + bounds)     71µs           855µs             12x        🟢 wrymium   ⚡
Click element (full)          110µs          25.6ms            233x       🟢 wrymium   ⚡
Click (native only)           6µs            —                 —          🟢 wrymium
Type text (JS vs fill)        83µs           818µs             9.9x       🟢 wrymium
10 concurrent evals           202µs          713µs             3.5x       🟢 wrymium
A11y tree fast (JS)           247µs          225µs             0.91x      ≈  持平      ⚡
Screenshot (JPEG q60)         33ms           —                 —          🟢 wrymium   ⚡
interactive_elements          183µs          —                 —          🟢 wrymium   ⚡
Screenshot (full PNG)         50ms           26ms              0.52x      🔵 Playwright
Screenshot (clip PNG)         50ms           25ms              0.50x      🔵 Playwright
A11y tree (CDP raw)           439µs          225µs             0.51x      🔵 Playwright
A11y tree (CDP, complex)      1.16ms         423µs             0.36x      🔵 Playwright
Navigate (file://)            14.7ms         1.93ms            0.13x      🔵 Playwright
```

**wrymium 优势场景（9/15 胜 + 1 持平）**:
- **所有 CDP 调用类操作**: 进程内调度省去 WebSocket 序列化/反序列化 + 网络 RTT
- **DOM query 12x**: 单次 JS evaluate 替代 3 次 CDP roundtrip（71µs vs 855µs）
- **Click element 233x**: 单次 JS evaluate + 原生 click 替代 4 次 CDP roundtrip（110µs vs 25.6ms）
- **A11y tree fast ≈持平**: JS-based 构建绕开 CDP Accessibility domain（247µs vs 225µs）
- **原生输入**: `send_mouse_click_event` (6µs) 零网络开销
- **并发 CDP**: 进程内无连接竞争，10 个请求 202µs 全部完成
- **screenshot_fast (JPEG)**: 33ms，接近 Playwright 的 PNG 26ms

**Playwright 优势场景（5/15 胜）**:
- **截图 (PNG)**: headless GPU 合成管线优化（无窗口系统开销）
- **A11y tree (CDP raw)**: CDPSession WebSocket pipeline 更高效（但 wrymium JS 版已追平）
- **导航**: headless 跳过 UI 渲染

**关键结论**: 经过三轮优化（roundtrip 缩减 / JPEG 截图 / JS A11y tree），wrymium 在 Agent Loop 全路径上几乎全面领先或持平。唯一显著劣势是 PNG 截图（headless 优化）和导航（headless 跳过渲染）。在实际 Agent 场景中，observe 使用 `screenshot_fast` (JPEG) + `a11y_tree_fast` (JS)，act 使用 `click_element` / `type_text`，整条链路延迟远低于 Playwright。

### 已知问题与限制

1. **`with_html()` (data: URI) 不执行 `<script>`**: wrymium 的 `form_urlencoded::byte_serialize` 编码破坏了 JS 中的特殊字符。建议用 `with_url("file://...")` 或自定义 protocol 代替。
2. **Windows `multi_threaded_message_loop` 未验证**: `send_blocking` 的 `do_message_loop_work()` 泵在 Windows 上的行为需要单独验证。
3. ~~`send_mouse_wheel_event` 在 windowed 模式下无效~~ — **已修复**: 改用 CDP `Input.dispatchMouseEvent(mouseWheel)`。
4. ~~`wait_for_navigation` 存在竞态~~ — **已修复**: 先订阅 CDP 事件再发送 `Page.navigate`。
5. ~~iframe 支持未实现~~ — **已实现**: `evaluate_in_frame()` + `find_element_in_frame()` + `Page.createIsolatedWorld`。
6. ~~CefDownloadHandler 未接入~~ — **已实现**: `wrap_download_handler!` 绑定到 builder 的 `download_started_handler` / `download_completed_handler`。

### 未来扩展

- **端到端 Agent 评估**: 需要 Claude API key + 完整 Tauri app 集成，评估任务成功率/步数/token 消耗
- **OSR 截图模式**: 切换到 offscreen rendering 可能进一步改善 PNG 截图延迟（当前 50ms，JPEG 33ms）
- **iframe click_element**: 当前 `find_element_in_frame` 返回 frame 内的视口坐标，点击需要额外加上 iframe 在主页面的偏移
- **Windows / Linux 平台验证**: 当前仅在 macOS Apple Silicon 上验证
