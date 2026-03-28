# wrymium — Open Issues & TODO

> Unresolved design problems and action items. Each item must be resolved before or during implementation.
> Updated with Spike 1/2/3 findings (2026-03-28).

---

## Critical Design Gaps

### 1. Message Loop Integration — wrymium 不拥有事件循环

**问题**: wrymium 作为 wry 的替代品，只提供 `WebView`/`WebViewBuilder`，不拥有主线程事件循环（tao 拥有）。但 CEF 的 `external_message_pump` 模式要求在主线程定期调用 `CefDoMessageLoopWork()`。wrymium 没有注入点。

**Spike 1 结论: 可行。** tao 和 CEF 可以在 macOS 主线程共存。

**确定方案**:
- macOS: 添加 `CFRunLoopTimer`（30fps）到 `CFRunLoopGetMain()` + `kCFRunLoopCommonModes`，回调中调用 `CefDoMessageLoopWork()`。tao 内部也用同样的机制（`EventLoopWaker`），两者共存是 CFRunLoop 的设计意图
- `OnScheduleMessagePumpWork(delay_ms)` 通过 `CFRunLoopTimerSetNextFireDate()` 动态调整定时器（Apple 文档确认线程安全）
- `CefInitialize()` 在 `WebViewBuilder::build()` 首次调用时执行（tao 的 `EventLoop::new()` 已创建 NSApplication，但 `EventLoop::run()` 尚未启动）
- 必须设置 `external_message_pump = true`，否则 `CefDoMessageLoopWork()` 会通过 `MessagePumpNSApplication` 窃取 tao 的事件（导致键盘事件丢失、重入死锁）
- Linux: `g_idle_add` / `g_timeout_add` 注入 GLib main loop（与 macOS 方案类似）
- Windows: `multi_threaded_message_loop = true`（仅 Windows 支持此模式）

**剩余风险**:
- `CefAppProtocol`：CEF 可能在运行时检查 NSApplication 是否实现了 `CefAppProtocol`。`external_message_pump = true` 应该不需要，但需要实际验证
- CEF 子进程 helper bundle 打包（cef-rs 的 `build_util::mac` 应能处理，但需实际跑通）

**详细方案**: → [spike-1-message-loop.md](./spike-1-message-loop.md)

**状态**: RESOLVED — 方案确定，待实现验证

---

### 2. 平台扩展 Trait 返回原生类型 — 必须同时 patch tauri-runtime-wry

**问题**: `tauri-runtime-wry` 在关键路径上调用返回 WebKit/WebView2/WebKitGTK 原生类型的方法，CEF 无法提供这些类型。仅 patch wry 不够。

**Spike 2 结论: 必须同时 patch tauri-runtime-wry。** Spec 中 "No Tauri source changes required" 的假设不成立。

**5 个 RED blocker**（编译级别，无法绕过）:

| # | 方法 | 位置 | 原因 |
|---|------|------|------|
| 1 | `WebViewExtMacOS::webview()` | lib.rs:5265 `inner_size()` | 关键路径：每次 macOS resize 都调用，cast 为 NSView 读取 frame |
| 2 | `WebViewExtWindows::controller()` | lib.rs:5134 | 关键路径：每次 Windows WebView 创建后设置 ~90 行 COM 事件处理（焦点+全屏） |
| 3 | `WebViewExtUnix::webview()` | undecorated_resizing.rs:532 | 关键路径：Linux 无边框窗口创建时附加 GTK 事件处理器 |
| 4 | `WebViewBuilderExtMacos::with_webview_configuration()` | lib.rs:4708 | 接受 `Retained<WKWebViewConfiguration>`，CEF 无法接受此类型 |
| 5 | `NewWindowResponse::Create` | lib.rs:4821-4834 | 携带平台原生 webview 句柄（WKWebView/ICoreWebView2/webkit2gtk::WebView） |

**YELLOW（可 stub/workaround）**: `manager()`, `ns_window()`, `environment()` — 仅在 `with_webview` API 中使用（opt-in，罕见）

**GREEN（可用 CEF 实现）**: 所有 `reparent()` 方法、`wry::Error`（仅构造 `MessageSender` variant，不做模式匹配）

**确定方案**: Fork `tauri-runtime-wry`，维护一个薄 patch 集：
1. `webview.rs` — 重定义 `Webview` 平台结构体为 CEF 兼容类型
2. `lib.rs:5256-5269` — macOS `inner_size()` 改为读取 CEF browser view 的 NSView frame
3. `lib.rs:5132-5220` — Windows 焦点/全屏 改为使用 CEF 的 `CefFocusHandler`/`CefDisplayHandler`
4. `undecorated_resizing.rs:524-600` — Linux 改为使用 CEF widget 的 GTK 事件
5. `lib.rs:4821-4834` — `NewWindowResponse::Create` 使用 CEF 句柄
6. `lib.rs:3916-3963` — `WithWebview` handler 打包 CEF 句柄
- 使用 `#[cfg(feature = "cef")]` 条件编译，保持与上游的可合并性
- 补丁规模约 ~6 文件、~200 行改动
- `tauri-runtime-wry` 变化不频繁，可用 `git format-patch` 在每个 tauri 版本上 rebase

**对 Tauri Integration 章节的影响**: 用户需要 patch 两个 crate：
```toml
[patch.crates-io]
wry = { git = "https://github.com/wrymium/wrymium", tag = "v0.1.0" }
tauri-runtime-wry = { git = "https://github.com/wrymium/tauri-runtime-wry", tag = "v0.1.0" }
```

**详细分析**: → [platform-trait-callsite-analysis.md](./platform-trait-callsite-analysis.md)

**状态**: RESOLVED — 方案确定，spec 需更新

---

### 3. 跨进程脚本注入

**问题**: 初始化脚本在浏览器进程配置，但 `OnContextCreated` 在渲染进程执行。

**确定方案**: 方案 A — `CefProcessMessage` browser → renderer：
1. `WebViewBuilder::build()` 时将脚本存储在 browser 进程的 per-browser 映射表中
2. 渲染进程启动后，在 `CefRenderProcessHandler::OnBrowserCreated` 中发送 `CefProcessMessage(PID_BROWSER, "request_scripts")`
3. Browser 进程在 `CefClient::OnProcessMessageReceived` 中回复 `CefProcessMessage(PID_RENDERER, "init_scripts", [script1, script2, ...])`
4. Renderer 缓存脚本，在每次 `OnContextCreated` 中注入

**竞态问题**: `OnContextCreated` 可能在收到脚本响应之前触发（页面加载极快时）。缓解方案：
- 在 `OnContextCreated` 中检查脚本是否已缓存，如果没有则延迟注入（注册一个 pending flag）
- 收到脚本后检查是否有 pending 的 context，如果有则立即补注入
- 或者：改用 `CefBrowserProcessHandler::OnRenderProcessThreadCreated(extra_info)` 传递全局脚本（但此方法在沙箱模式下可能被跳过）

**多 WebView**: 按 `browser_id` 区分脚本集。

**状态**: RESOLVED — 方案确定，竞态缓解策略明确

---

## Secondary Issues

### 4. 窗口 Resize 同步

**Spike 4 结论: macOS 自动 resize，Windows/Linux 需要手动处理。**

- macOS: CEF 内部在 `CreateHostWindow` 中对 browser view 设置 `NSViewWidthSizable | NSViewHeightSizable` autoresizing mask，browser view 随父视图自动 resize。`set_bounds()` 可为 no-op（与 cefclient 一致）
- Windows: 手动调用 `SetWindowPos(browser_hwnd, ...)` 响应 `set_bounds()`。通过 `browser_host.window_handle()` 获取 HWND。建议在 resize 前调用 `notify_move_or_resize_started()`
- Linux: 手动调用 `XConfigureWindow` 响应 `set_bounds()`。CEF 内部的 `ConfigureNotify` 处理器会自动传播 resize 到内容窗口并调用 `NotifyMoveOrResizeStarted()`
- `WasResized()` 仅用于 OSR 模式，windowed 模式不需要
- Resize 闪烁是 Chromium 级别的已知问题，CEF v133+ 包含 Electron 团队的 DirectComposition 修复

**详细方案**: → [spike-4-resize-sync.md](./spike-4-resize-sync.md)

**状态**: RESOLVED — 方案确定，三个平台策略明确

---

### 5. wry::Error 类型兼容

**Spike 2 结论**: wry::Error 是 `#[non_exhaustive]` enum，有 ~25 个 variant。`tauri-runtime-wry` 只构造 `MessageSender` variant，不做模式匹配。wrymium 可定义简化的 Error enum：

```rust
#[non_exhaustive]
pub enum Error {
    MessageSender,
    Io(std::io::Error),
    CefError(String),          // CEF-specific errors
    // ... 按需添加其他 variant
}
```

**状态**: RESOLVED

---

### 6. build.rs 传递依赖验证

**Spike 3 结论: 传递依赖链可以正常工作。** `wrymium -> cef -> cef-dll-sys` 的构建流程：

- `cef-dll-sys` 的 build.rs 自动下载 CEF（无需手动设置 `CEF_PATH`）
- CMake + Ninja 编译 `libcef_dll_wrapper`（C++ 静态库）
- link search path 通过 cargo 正确传递到最终二进制

**系统依赖**:

| 依赖 | 必需？ | 说明 |
|------|--------|------|
| CMake | 是 | 编译 libcef_dll_wrapper |
| **Ninja** | **是（硬依赖）** | build.rs 硬编码 `.generator("Ninja")`，无 fallback |
| C++ 编译器 | 是 | macOS: Xcode CLT, Windows: MSVC, Linux: g++/clang++ |
| 网络 | 首次构建 | 下载 CEF ~80-100MB |

**DX 摩擦点**:

| 问题 | 影响 | 缓解 |
|------|------|------|
| Ninja 未安装 | 构建失败，错误信息不清晰 | wrymium 的 build.rs 加检测 + 友好错误提示 |
| 首次构建 3-10 分钟 | 开发体验差 | 文档说明；推荐用 `CEF_PATH` + `export-cef-dir` 缓存 |
| `cargo clean` 触发重新下载 | 浪费时间/带宽 | 推荐用 `CEF_PATH` 指向共享缓存 |
| debug/release 各下载一次 | 两倍下载 | 用 `CEF_PATH` 共享 |
| macOS 不能直接 `cargo run` | 需要 bundle 工具 | 提供 `wrymium-bundler` 或复用 `bundle-cef-app` |
| 运行时需设置库路径 | 启动崩溃 | `DYLD_FALLBACK_LIBRARY_PATH` / `LD_LIBRARY_PATH` / `PATH` |

**wrymium 的 build.rs 设计**: 最小化 — 不重复 cef-dll-sys 的工作，只做：
1. 检测 CMake + Ninja 是否安装，缺失时输出友好错误
2. macOS: 编译 .xib 文件（如需要）
3. Windows: 嵌入 manifest/资源

**状态**: RESOLVED — 方案确定，已知限制已记录

---

### 7. CEF 启动性能

**研究结论**: 已完成全面的性能数据调研。关键发现：
- `CefInitialize()`: 冷启动 3-4s（Windows reboot 后），热启动 ~30ms，典型硬件 300-600ms
- 首次浏览器创建: 多进程模式下 2-3s 到可见内容，后续 <1s
- 与原生 WebView 对比: CEF 热启动多 400-800ms，冷启动多 2-4s
- 最大优化点: 禁用 Windows 代理自动检测（`--no-proxy-server`）可省 1-2s
- 缓解策略: 立即显示 tao 窗口 + splash/skeleton UI + 预热隐藏浏览器
- 结论: 热启动延迟与 Electron 相当（~1s），对桌面应用可接受

**详细分析**: → [cef-startup-performance.md](./cef-startup-performance.md)

**状态**: RESOLVED — 调研完成，仍需在 v0.1 原型中实测验证

---

### 8. 测试策略

**研究结论**: 已完成全面的测试策略调研。关键发现：

- **CEF 自身测试架构**: 使用 GTest + 单进程二进制（同时充当浏览器和子进程），`TestHandler` 基类管理浏览器生命周期，`CompletionState` + `WaitForTests()` 处理异步等待
- **核心约束**: CEF 只能初始化一次 → 所有集成测试共享一个 `CefInitialize()`，用 `std::sync::Once` 保护
- **OSR 模式**: `windowless_rendering_enabled = true` + `SetAsWindowless(nullptr)` 可在无窗口环境测试 IPC、协议处理、脚本注入等核心功能
- **CI 环境**: Linux 需要 Xvfb（CEF 即使在无窗口模式也依赖 X11 库）；macOS/Windows CI runner 原生支持
- **四层测试策略**:
  - Layer A: 纯单元测试（无 CEF，<1s）— 类型、序列化、URL 解析
  - Layer B: 集成测试（CEF 浏览器进程，OSR）— scheme handler、浏览器生命周期
  - Layer C: E2E 测试（浏览器+渲染进程）— IPC 双路径、脚本注入、JS 求值
  - Layer D: Tauri 集成测试 — 编译检查 + 运行时验证
- **关键工具**: `pump_until()` 异步等待、title-change trick 获取 JS 返回值、MockSchemeHandler
- **CI 预估**: 冷启动 ~18min，缓存后 ~8min（三平台并行）

**详细方案**: → [testing-strategy.md](./testing-strategy.md)

**状态**: RESOLVED — 方案确定，待 v0.1 实现时落地

---

### 9. cef-rs wrap_* 宏 DX

**调研结论: 使用宏，采用 hybrid 架构缓解 DX 问题。**

**宏的工作原理**: 每个 `wrap_*!` 宏展开为：struct 定义 + `new()` 构造函数 + `WrapXxx` impl（存储 `RcImpl` 指针）+ `Clone` impl（引用计数）+ `Rc` impl + `ImplXxx` impl（你的方法覆盖）。`RcImpl<T, I>` 是核心：`#[repr(C)]` 结构体，包含 CEF vtable struct + 你的 Rust struct + AtomicUsize 引用计数。

**DX 问题实际评估**（来自 issue #297，amrbashir 报告）:
1. **不能 `cargo fmt`** — 宏内代码对格式化器不透明。影响中等，handler 逻辑通常不长
2. **无 IDE 补全** — `impl App {}` 块内 Ctrl+Space 无效。影响中等，需要对照 `ImplXxx` trait 文档写
3. **必须知道类层级和顺序** — 仅影响 `wrap_window_delegate!`/`wrap_browser_view_delegate!`（需要 ViewDelegate → PanelDelegate → WindowDelegate 顺序）。wrymium 需要的 11 个 handler **全部是扁平结构**（单 impl 块），不受此问题影响

**Raw FFI 成本评估**: 11 个 handler 用 raw `cef-dll-sys` 实现需要 ~1500-2000 行纯机械 boilerplate（vtable 设置、`extern "C"` thunk、引用计数）。**不现实。**

**Handler 覆盖率**: wrymium 需要的 **全部 11 个 handler 都有对应的 `wrap_*!` 宏和 `Impl*` trait**（均带默认实现）：

| Handler | 宏 | vtable 方法数 |
|---------|-----|-------------|
| CefApp | `wrap_app!` | 5 |
| CefClient | `wrap_client!` | 19 |
| CefBrowserProcessHandler | `wrap_browser_process_handler!` | 7 |
| CefRenderProcessHandler | `wrap_render_process_handler!` | 9 |
| CefSchemeHandlerFactory | `wrap_scheme_handler_factory!` | 1 |
| CefResourceHandler | `wrap_resource_handler!` | 7 |
| CefV8Handler | `wrap_v8_handler!` | 1 |
| CefFocusHandler | `wrap_focus_handler!` | 3 |
| CefDisplayHandler | `wrap_display_handler!` | 13 |
| CefLifeSpanHandler | `wrap_life_span_handler!` | 6 |
| CefLoadHandler | `wrap_load_handler!` | 4 |

**确定方案**: Hybrid 架构 — 用 `wrap_*!` 宏做 CEF 接口绑定（薄 delegation 层），将实际业务逻辑放在独立的 plain-Rust struct/方法中：

```rust
// 薄宏层（不可 fmt，无 IDE 支持，但只有几行）
wrap_resource_handler!(IpcResourceHandler {
    inner: Arc<IpcResourceHandlerInner>,
});
impl ImplResourceHandler for IpcResourceHandler {
    fn open(&self, request, handle_request, callback) -> bool {
        self.inner.handle_open(request, handle_request, callback)
    }
    fn get_response_headers(&self, response, length, redirect) {
        self.inner.handle_get_response_headers(response, length, redirect)
    }
    fn read(&self, data_out, bytes_read, callback) -> bool {
        self.inner.handle_read(data_out, bytes_read, callback)
    }
}

// 业务逻辑层（可 fmt、有 IDE 支持、可测试）
struct IpcResourceHandlerInner { ... }
impl IpcResourceHandlerInner {
    fn handle_open(&self, ...) -> bool { /* 实际逻辑 */ }
    fn handle_get_response_headers(&self, ...) { /* 实际逻辑 */ }
    fn handle_read(&self, ...) -> bool { /* 实际逻辑 */ }
}
```

这样宏层只是 3-5 行 delegation，所有可测试/可格式化/有 IDE 支持的代码都在 plain Rust 中。

**状态**: RESOLVED — 使用 `wrap_*!` 宏 + 业务逻辑分离架构

---

## Spike 结果汇总

| 调研 | 结论 | 关键发现 | 详细文档 |
|-------|------|---------|---------|
| **Spike 1** | **可行** | CFRunLoopTimer 30fps + external_message_pump 可与 tao 共存，无需修改 tao | [spike-1-message-loop.md](./spike-1-message-loop.md) |
| **Spike 2** | **必须同时 patch tauri-runtime-wry** | 5 个 RED blocker：macOS inner_size、Windows 焦点/全屏、Linux resize、WKWebViewConfiguration、NewWindowResponse | [platform-trait-callsite-analysis.md](./platform-trait-callsite-analysis.md) |
| **Spike 3** | **可行，有摩擦** | 传递依赖正常工作，但 Ninja 是硬依赖，首次构建慢，macOS 需要 bundle 工具 | (inline above) |
| **Spike 4** | **macOS 自动，Win/Linux 手动** | CEF 内部设 autoresizing mask（macOS）；Windows 需 `SetWindowPos`；Linux 需 `XConfigureWindow`；`WasResized()` 仅 OSR | [spike-4-resize-sync.md](./spike-4-resize-sync.md) |
| **调研 #7** | **可接受** | 热启动 +400-800ms（与 Electron 相当），冷启动 +2-4s；禁用代理检测省 1-2s；splash UI 缓解感知 | [cef-startup-performance.md](./cef-startup-performance.md) |
| **调研 #8** | **四层测试策略** | OSR 无窗口测试；Once 保护 CefInitialize；pump_until 异步等待；Linux Xvfb；CI ~8min 缓存后 | [testing-strategy.md](./testing-strategy.md) |
| **调研 #9** | **使用宏 + 逻辑分离** | 全部 11 个 handler 有 `wrap_*!` 宏；raw FFI 需 ~2000 行不现实；thin macro delegation + plain Rust 业务逻辑 | (inline above) |

---

## 对 Spec 的必要更新

基于 Spike 结果，以下 spec 章节需要更新：

1. **Tauri Integration**: 用户需要 patch 两个 crate（wry + tauri-runtime-wry），而非仅 wry
2. **Repository Structure**: 需要增加 `tauri-runtime-wry/` fork 目录或独立仓库
3. **Architecture Layers**: 依赖链增加 tauri-runtime-wry patch
4. **Message Loop**: 用 CFRunLoopTimer 方案替代模糊的 "CEF thread" 描述
5. **Milestones**: v0.1 增加 tauri-runtime-wry patch 工作项
6. **系统依赖**: 增加 CMake + Ninja 要求的说明
