# wrymium — Project Spec

> A CEF-powered WebView backend for wry, bringing consistent Chromium rendering to Tauri apps.

---

## Background

Tauri uses system-native WebView to keep app bundles small. This works well on Windows (WebView2) and reasonably well on macOS (WebKit), but introduces rendering inconsistencies across platforms. Developers who need guaranteed Chromium rendering today must fall back to Electron, sacrificing the lightweight architecture Tauri provides.

wrymium bridges this gap: a drop-in wry-compatible backend that uses the Chromium Embedded Framework (CEF), giving Tauri apps a consistent Chromium rendering engine without pulling in all of Electron.

### Relationship to Official Tauri CEF Efforts

The Tauri team (via CrabNebula) is internally exploring CEF integration using their own `cef-rs` bindings. However:

- Their work is internal and not yet publicly available
- It may become a commercial/closed-source offering (no decision made yet, per Discussion #8524)
- There is no public timeline for release
- The Tauri team has stated (wry Discussion #1014, July 2025) that alternative renderers should be **separate crates**, not integrated into wry

wrymium fills the gap as an **open-source, community-driven** CEF integration that is available now. It builds on top of `tauri-apps/cef-rs` (the open-source CEF bindings) and contributes missing wrappers back upstream.

---

## Goals

- Provide a wry-compatible public API so Tauri can swap in wrymium with minimal changes
- Embed CEF as the WebView backend on all three major platforms (macOS, Windows, Linux)
- Support Tauri 2.x's dual-path IPC system (`ipc://localhost` custom protocol + `window.ipc.postMessage` fallback)
- Support custom URI schemes (`tauri://`, `asset://`)
- Build on `tauri-apps/cef-rs` rather than reinventing CEF bindings
- Keep the codebase focused on the integration layer — no legacy wry baggage

## Non-Goals

- Replacing Tauri's window management (windowing stays with `tao` or the OS)
- Supporting every wry feature on day one
- Being a general-purpose CEF wrapper (use `cef` crate directly for that)
- Mobile platform support (iOS, Android) in v1
- Reimplementing CEF FFI bindings (use `cef-dll-sys` from crates.io)

---

## Architecture

### Design Principle

wrymium is an **integration layer**, not a CEF binding. The CEF binding work is handled by `tauri-apps/cef-rs`, which provides production-quality FFI bindings (`cef-dll-sys`) and safe Rust wrappers (`cef`). wrymium focuses exclusively on the wry-compatible API surface and Tauri runtime integration.

### Why not fork wry?

wry's architecture makes several assumptions that conflict with CEF's model:

- wry 0.55 accepts `HasWindowHandle` and is windowing-library-agnostic; CEF requires early process initialization before any windowing code runs
- CEF is inherently multi-process (browser process + renderer process + GPU process + utility process); wry assumes single-process
- CEF requires custom message loop integration; wry delegates event loops to the consumer
- The Tauri team has explicitly stated alternative renderers should be separate crates, not wry forks

wrymium is written from scratch, designed around CEF's constraints, and exposes an interface that mirrors wry's public API.

### Why build on cef-rs?

`tauri-apps/cef-rs` is maintained by the Tauri organization. It:

- Tracks CEF releases within days (254+ releases, currently at v146)
- Handles the hardest infrastructure problems: FFI bindings via bindgen, CEF binary download, macOS helper bundle generation, CMake compilation
- Already wraps CefApp, CefBrowser, CefClient, WindowInfo (including `set_as_child`)
- Provides message router utilities (`CefMessageRouterBrowserSide`/`RendererSide`)

Building a separate `cef-sys` would be pure duplication of solved work.

### What cef-rs does NOT provide (wrymium's value)

| Gap | wrymium fills it |
|-----|-----------------|
| CefSchemeHandlerFactory / CefResourceHandler | Custom protocol IPC (`ipc://`, `tauri://`, `asset://`) |
| CefV8Handler / V8 extensions | `window.ipc.postMessage` fallback bridge |
| CefRenderProcessHandler | Initialization script injection |
| wry-compatible API surface | Drop-in replacement for `tauri-runtime-wry` |
| Message loop bridging | CEF external_message_pump ↔ tao event loop |

Missing safe wrappers will be **contributed upstream to cef-rs** where they are generally useful.

### Dependency Chain

```
Tauri App
  └── tauri 2.x
        └── tauri-runtime-wry (patched via [patch.crates-io])  ← fork with ~200 lines CEF patch
              └── wrymium (patched as "wry" via [patch.crates-io])  ← wry-compatible API
                    └── cef            ← tauri-apps/cef-rs safe wrapper (crates.io)
                          └── cef-dll-sys  ← raw CEF C API FFI (crates.io)
```

**Note**: Both `wry` AND `tauri-runtime-wry` must be patched. See [TODO.md](./TODO.md) issue #2 for details on why patching wry alone is insufficient.

### Repository Structure

```
wrymium/
  ├── Cargo.toml                    ← workspace root
  ├── wrymium/                      ← main library crate (package name: "wry")
  │   ├── src/
  │   │   ├── lib.rs                ← pub use re-exports, webview_version()
  │   │   ├── webview.rs            ← WebView / WebViewBuilder (mirrors wry 0.55)
  │   │   ├── types.rs              ← Rect, DragDropEvent, ProxyConfig, enums
  │   │   ├── context.rs            ← WebContext
  │   │   ├── ipc.rs                ← Dual-path IPC bridge (custom protocol + postMessage)
  │   │   ├── scheme.rs             ← CefSchemeHandlerFactory / CefResourceHandler
  │   │   ├── event_loop.rs         ← CEF message pump integration (CFRunLoopTimer on macOS)
  │   │   ├── renderer.rs           ← CefRenderProcessHandler, V8 extensions
  │   │   └── platform/
  │   │       ├── macos.rs          ← WebViewExtDarwin, WebViewExtMacOS, WebViewBuilderExtMacos
  │   │       ├── windows.rs        ← WebViewExtWindows, WebViewBuilderExtWindows
  │   │       └── linux.rs          ← WebViewExtUnix, WebViewBuilderExtUnix
  │   └── Cargo.toml                ← depends on `cef` from crates.io
  ├── tauri-runtime-wry/            ← fork of tauri-runtime-wry with CEF patches
  │   ├── src/
  │   │   ├── lib.rs                ← ~200 lines patched (inner_size, focus, resize handlers)
  │   │   ├── webview.rs            ← Webview struct redefined with CEF handle types
  │   │   └── undecorated_resizing.rs ← Linux resize handler patched for CEF widget
  │   └── Cargo.toml                ← depends on wrymium instead of wry
  └── examples/
      └── basic/                    ← minimal example (standalone, no Tauri)
```

---

## Public API

The public surface mirrors wry 0.55's API as consumed by `tauri-runtime-wry` v2.10.1. The full API inventory is documented in [wry-api-surface.md](./wry-api-surface.md).

### Key API Differences from Original Spec

wry 0.55 has significant changes from older versions:

| Old (spec v1) | Current (wry 0.55) |
|---|---|
| `WebViewBuilder::new(window: &Window)` | `WebViewBuilder::new()` — window not passed at construction |
| `.build() -> Result<WebView>` | `.build(&window) -> Result<WebView>` where `W: HasWindowHandle` |
| Accepts concrete `Window` type | Accepts any `HasWindowHandle` (from `raw-window-handle 0.6`) |
| 6 builder methods | **35+ builder methods** |
| 4 WebView methods | **20+ WebView methods** + 2 static methods |
| No platform extension traits | **6 platform extension traits** |

### Constructor & Build

```rust
pub struct WebViewBuilder { /* ... */ }

impl WebViewBuilder {
    /// Constructor — takes a mutable WebContext reference, NOT a window
    pub fn new_with_web_context(web_context: &mut WebContext) -> Self { ... }

    /// Build methods — window passed here, accepts HasWindowHandle
    pub fn build<W: HasWindowHandle>(self, window: &W) -> Result<WebView> { ... }
    pub fn build_as_child<W: HasWindowHandle>(self, window: &W) -> Result<WebView> { ... }
}
```

### Configuration Methods (35+)

Grouped by category — see [wry-api-surface.md](./wry-api-surface.md) for full signatures:

**Content**: `with_url`, `with_html`
**UI**: `with_transparent`, `with_background_color`, `with_visible`, `with_bounds`, `with_focused`, `with_accept_first_mouse`
**Behavior**: `with_javascript_disabled`, `with_hotkeys_zoom`, `with_clipboard`, `with_incognito`, `with_user_agent`, `with_proxy_config`, `with_background_throttling`
**IPC & Protocols**: `with_ipc_handler`, `with_asynchronous_custom_protocol`, `with_initialization_script_for_main_only`
**Events**: `with_navigation_handler`, `with_drag_drop_handler`, `with_new_window_req_handler`, `with_document_title_changed_handler`, `with_download_started_handler`, `with_download_completed_handler`, `with_on_page_load_handler`
**DevTools**: `with_devtools`
**Platform-specific**: `with_https_scheme`, `with_additional_browser_args`, `with_theme`, `with_scroll_bar_style`, `with_data_store_identifier`, `with_webview_configuration`, etc.

### WebView Runtime Methods (20+)

```rust
pub struct WebView { /* ... */ }

impl WebView {
    // Core
    pub fn evaluate_script(&self, js: &str) -> Result<()> { ... }
    pub fn load_url(&self, url: &str) -> Result<()> { ... }
    pub fn reload(&self) -> Result<()> { ... }
    pub fn url(&self) -> Result<String> { ... }

    // Display
    pub fn set_visible(&self, visible: bool) -> Result<()> { ... }
    pub fn set_bounds(&self, bounds: Rect) -> Result<()> { ... }
    pub fn bounds(&self) -> Result<Rect> { ... }
    pub fn zoom(&self, scale_factor: f64) -> Result<()> { ... }
    pub fn focus(&self) -> Result<()> { ... }
    pub fn set_background_color(&self, color: (u8, u8, u8, u8)) -> Result<()> { ... }
    pub fn print(&self) -> Result<()> { ... }

    // Cookies
    pub fn cookies(&self) -> Result<Vec<Cookie>> { ... }
    pub fn cookies_for_url(&self, url: &str) -> Result<Vec<Cookie>> { ... }
    pub fn set_cookie(&self, cookie: &Cookie) -> Result<()> { ... }
    pub fn delete_cookie(&self, cookie: &Cookie) -> Result<()> { ... }

    // Data
    pub fn clear_all_browsing_data(&self) -> Result<()> { ... }

    // DevTools (behind feature flag)
    pub fn open_devtools(&self) { ... }
    pub fn close_devtools(&self) { ... }
    pub fn is_devtools_open(&self) -> bool { ... }
}
```

### Types & Enums

```rust
pub struct Rect { pub position: dpi::Position, pub size: dpi::Size }
pub struct WebContext { /* data directory management */ }
pub struct ProxyEndpoint { pub host: String, pub port: String }

pub enum ProxyConfig { Http(ProxyEndpoint), Socks5(ProxyEndpoint) }
pub enum DragDropEvent { Enter { paths, position }, Over { position }, Drop { paths, position }, Leave }
pub enum BackgroundThrottlingPolicy { Disabled, Suspend, Throttle }
pub enum PageLoadEvent { Started, Finished }
pub enum NewWindowResponse { Allow, Deny, Create { webview } }
pub enum Theme { Dark, Light }           // Windows
pub enum ScrollBarStyle { Default, FluentOverlay }  // Windows
```

### Feature Flags

```toml
[features]
default = ["protocol", "os-webview"]
protocol = []          # custom protocol support (required by tauri-runtime-wry)
os-webview = []        # no-op for CEF, but must exist for compatibility
linux-body = []        # no-op for CEF
devtools = []          # enable open_devtools/close_devtools/is_devtools_open
x11 = []               # X11 support on Linux
fullscreen = []        # macOS fullscreen support
transparent = []       # transparent window support
tracing = []           # tracing instrumentation
mac-proxy = []         # macOS proxy support
```

### Re-exports

```rust
// lib.rs — required for tauri-runtime-wry compatibility
pub use crate as wry;          // self re-export for `pub use wry;` downstream
pub fn webview_version() -> Result<String> { ... }  // returns CEF version

// Re-export http, dpi, raw-window-handle for downstream use
pub use http;
pub use dpi;
pub use raw_window_handle;
```

### Platform Extension Traits

Each platform has builder and runtime extension traits. For macOS (the v0.1 target):

```rust
// platform/macos.rs
pub trait WebViewBuilderExtMacos {
    fn with_webview_configuration(self, config: /* ... */) -> Self;
    fn with_traffic_light_inset(self, position: /* ... */) -> Self;
}

pub trait WebViewBuilderExtDarwin {
    fn with_data_store_identifier(self, id: [u8; 16]) -> Self;
    fn with_allow_link_preview(self, enabled: bool) -> Self;
}

pub trait WebViewExtMacOS {
    fn reparent(&self, window: *mut c_void) -> Result<()>;
    // Note: webview(), manager(), ns_window() return platform-native types
    // that don't exist in CEF — these will return CEF equivalents or panic
}

pub trait WebViewExtDarwin {}  // trait must exist, methods may be stubs
```

---

## IPC Bridge

Tauri 2.x uses a **dual-path IPC** system, NOT the `window.__TAURI_IPC__` from Tauri v1. The full technical analysis is in [ipc-deep-dive.md](./ipc-deep-dive.md).

### Primary Path: `ipc://localhost` Custom Protocol

The frontend's `invoke()` function sends `fetch("ipc://localhost/{cmd}")` POST requests with:
- `Content-Type`: `application/json` or `application/octet-stream`
- `Tauri-Callback` / `Tauri-Error`: u32 callback IDs
- `Tauri-Invoke-Key`: runtime-generated authentication key
- Body: JSON string or raw bytes

This is dramatically faster than postMessage — transferring a 150MB file dropped from ~50s to <60ms (per PR #7170).

**CEF implementation**: Register `ipc` scheme in `CefApp::OnRegisterCustomSchemes` with `CEF_SCHEME_OPTION_FETCH_ENABLED`, then implement `CefSchemeHandlerFactory` + async `CefResourceHandler`:

```rust
// CefApp — all processes
fn on_register_custom_schemes(&self, registrar: &mut CefSchemeRegistrar) {
    registrar.add_custom_scheme("ipc",
        CEF_SCHEME_OPTION_STANDARD |
        CEF_SCHEME_OPTION_CORS_ENABLED |
        CEF_SCHEME_OPTION_FETCH_ENABLED
    );
    registrar.add_custom_scheme("tauri",
        CEF_SCHEME_OPTION_STANDARD |
        CEF_SCHEME_OPTION_CORS_ENABLED |
        CEF_SCHEME_OPTION_FETCH_ENABLED |
        CEF_SCHEME_OPTION_CSP_BYPASSING
    );
    registrar.add_custom_scheme("asset",
        CEF_SCHEME_OPTION_STANDARD |
        CEF_SCHEME_OPTION_CORS_ENABLED |
        CEF_SCHEME_OPTION_FETCH_ENABLED
    );
}

// CefBrowserProcessHandler — browser process only
fn on_context_initialized(&self) {
    cef_register_scheme_handler_factory("ipc", "localhost",
        Box::new(IpcSchemeHandlerFactory::new(self.ipc_handler.clone()))
    );
    cef_register_scheme_handler_factory("tauri", "localhost",
        Box::new(TauriSchemeHandlerFactory::new(self.protocol_handlers.clone()))
    );
}
```

The `CefResourceHandler` receives the request on the IO thread, dispatches to the Rust IPC handler asynchronously, and signals completion via `CefCallback::cont()`.

### Fallback Path: `window.ipc.postMessage`

Used when `fetch()` to the custom protocol fails (CSP blocks, etc.). wrymium implements this via a V8 extension in the renderer process:

```rust
// CefRenderProcessHandler::OnWebKitInitialized
const IPC_BRIDGE: &str = r#"
(function() {
    native function __wrymium_ipc_send__(message);
    Object.defineProperty(window, 'ipc', {
        value: Object.freeze({
            postMessage: function(s) { __wrymium_ipc_send__(s); }
        })
    });
})();
"#;
```

The native `CefV8Handler` sends `CefProcessMessage` from renderer to browser process, where `CefClient::OnProcessMessageReceived` dispatches to the `ipc_handler` callback.

Responses are sent back via `browser.get_main_frame().execute_javascript(callback_js)`, matching wry's `webview.eval()` behavior.

### Platform URL Differences

| Platform | IPC URL format | Reason |
|----------|---------------|--------|
| macOS/Linux | `ipc://localhost/{cmd}` | Native custom scheme support |
| Windows | `https://ipc.localhost/{cmd}` | WebView2 maps custom schemes to HTTPS |

CEF handles all platforms uniformly as `ipc://localhost/{cmd}` since it doesn't use WebView2.

---

## CEF Integration

### Process Model

CEF uses a multi-process architecture with 4-5 process types:

| Process | Role |
|---------|------|
| Browser | Main application process, manages all others |
| Renderer | Web content rendering + JavaScript (V8), sandboxed |
| GPU | GPU-accelerated compositing, WebGL |
| Utility | Network, storage, audio services |
| Plugin | Legacy (deprecated) |

By default, CEF re-executes the same binary with `--type=renderer`, `--type=gpu-process`, etc. The host app must handle this:

```rust
fn main() {
    // MUST be first — CEF may exec this process as a subprocess
    if wrymium::is_cef_subprocess() {
        std::process::exit(wrymium::run_cef_subprocess());
    }

    tauri::Builder::default()
        .run(tauri::generate_context!())
        .unwrap();
}
```

Alternatively, `CefSettings.browser_subprocess_path` can point to a separate helper executable (required on macOS for notarization).

### Message Loop

CEF provides three message loop modes. wrymium uses **external message pump** (`external_message_pump = true`):

| Mode | Description | Use |
|------|-------------|-----|
| `CefRunMessageLoop()` | CEF owns the loop entirely | Not suitable — conflicts with tao |
| `multi_threaded_message_loop` | CEF loop on separate thread | Windows only |
| **`external_message_pump`** | **App owns the loop, calls `CefDoMessageLoopWork()`** | **Recommended — macOS/Linux** |

wrymium does NOT own the event loop (tao does). Instead, it hooks into the platform's native run loop during `CefInitialize()`:

**macOS** — `CFRunLoopTimer` at 30fps on `CFRunLoopGetMain()`:
```
CefInitialize() called during first WebViewBuilder::build()
  ↓
CFRunLoopTimerCreate(callback: CefDoMessageLoopWork, interval: 33ms)
CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes)
  ↓
tao's [NSApp run] drives CFRunLoop → timer fires → CefDoMessageLoopWork()
```

This works because tao uses `[NSApp run]` which is built on `CFRunLoop`. tao itself uses the same pattern internally (its `EventLoopWaker` is a `CFRunLoopTimer`). Both timers coexist by design.

`OnScheduleMessagePumpWork(delay_ms)` dynamically adjusts the timer via `CFRunLoopTimerSetNextFireDate()` (thread-safe per Apple docs), enabling responsive scheduling without CPU spin.

**Linux** — `g_timeout_add` / `g_idle_add` on GLib main loop (same principle as macOS).

**Windows** — `multi_threaded_message_loop = true` (CEF runs its loop on a separate thread, avoiding the integration problem entirely).

**Critical**: `external_message_pump = true` must be set on macOS/Linux. Without it, `CefDoMessageLoopWork()` calls through `MessagePumpNSApplication` which steals OS events from tao's event queue, causing keyboard event loss and potential reentrancy deadlocks.

**Verified via Spike 1**: See [spike-1-message-loop.md](./spike-1-message-loop.md) for detailed analysis.

### Custom URI Schemes

All three schemes (`ipc://`, `tauri://`, `asset://`) are registered in two phases:

1. **Scheme declaration** (`CefApp::OnRegisterCustomSchemes`) — must happen in ALL process types (browser, renderer, GPU, utility) before `CefInitialize`
2. **Handler registration** (`cef_register_scheme_handler_factory`) — browser process only, after context initialization

### Windowed Embedding

wrymium uses `WindowInfo::set_as_child()` to embed CEF as a child view of the tao/winit window, not off-screen rendering (OSR). This is simpler, faster, and supported by cef-rs's existing `WindowInfo` API.

```rust
let mut window_info = CefWindowInfo::new();
window_info.set_as_child(tao_window.raw_window_handle());
browser_host_create_browser(&window_info, &client, &url, &settings);
```

---

## Platform Notes

### macOS

- CEF ships as a `.framework` bundle, embedded in the app via `build.rs`
- **5 helper app bundles required** (handled by cef-rs's `build_util::mac` module):
  - `App Helper.app` — generic helper
  - `App Helper (GPU).app` — GPU process
  - `App Helper (Renderer).app` — renderer process
  - `App Helper (Plugin).app` — plugin process (legacy)
  - `App Helper (Alerts).app` — notification process
- Each helper needs different entitlements for Apple notarization
- Linking: `framework = "Chromium Embedded Framework"`
- Minimum OS: macOS 10.15+

### Windows

- CEF ships as a flat directory of DLLs + resources
- `libcef.dll` must be next to the executable
- WebView2 is NOT used — CEF replaces it entirely
- IPC URLs use `ipc://localhost` (no HTTPS mapping needed, unlike native WebView2)
- Minimum OS: Windows 10

### Linux

- CEF ships as shared libraries + resources
- `libcef.so` must be on `LD_LIBRARY_PATH` or bundled with the app
- No GTK container-based embedding needed (unlike wry's `build_gtk`)
- wrymium provides `build_gtk` for API compatibility, but internally uses `set_as_child`
- Minimum: GTK3, glibc 2.31+

---

## CEF Distribution

CEF binary management is handled by the `download-cef` crate from `tauri-apps/cef-rs`:

```toml
# Cargo.toml
[build-dependencies]
download-cef = "2.3"    # downloads CEF from Spotify CDN
export-cef-dir = "1.0"  # manages shared CEF installations
```

The build workflow:
1. `download-cef` fetches the CEF Minimal distribution for the target platform from `cef-builds.spotifycdn.com`
2. `export-cef-dir` manages a shared cache (e.g., `~/.local/share/cef`) so multiple projects share one copy
3. The `cef-dll-sys` crate compiles `libcef_dll_wrapper` via CMake
4. The `cef` crate's build utilities handle platform-specific bundling (macOS helper apps, Windows DLL layout, Linux lib paths)

**Bundle size**: ~100MB unpacked per platform (macOS ~100-120MB, Windows ~94MB, Linux ~90-100MB). This is an inherent tradeoff — wrymium provides rendering consistency, not small bundles. ZSTD compression reduces download to ~50-60MB.

---

## Tauri Integration

### Approach: Patch `wry` AND `tauri-runtime-wry` via `[patch.crates-io]`

Consumers patch their `Cargo.toml` to point both crates at wrymium:

```toml
[dependencies]
tauri = "2"

[patch.crates-io]
wry = { git = "https://github.com/wrymium/wrymium", tag = "v0.1.0" }
tauri-runtime-wry = { git = "https://github.com/wrymium/tauri-runtime-wry", tag = "v0.1.0" }
```

**Why both crates?** Spike 2 revealed that `tauri-runtime-wry` has 5 code paths that use platform-native WebView types (WKWebView, ICoreWebView2Controller, webkit2gtk::WebView) in critical paths — not just in the `with_webview` escape hatch. These are compile-time blockers that cannot be resolved by patching wry alone. See [platform-trait-callsite-analysis.md](./platform-trait-callsite-analysis.md) for the full call-site inventory.

The `tauri-runtime-wry` fork is a **thin patch** (~6 files, ~200 lines changed) that:
1. Replaces macOS `inner_size()` WKWebView frame query with CEF view query
2. Replaces Windows focus/fullscreen COM hookup with `CefFocusHandler`/`CefDisplayHandler`
3. Replaces Linux undecorated-resize webkit2gtk event handlers with CEF widget events
4. Redefines `Webview` platform structs with CEF handle types
5. Uses `#[cfg(feature = "cef")]` for conditional compilation

### System Requirements

| Dependency | Required? | Notes |
|-----------|-----------|-------|
| CMake | Yes | Compiles libcef_dll_wrapper (C++ static library) |
| **Ninja** | **Yes (hard dependency)** | cef-rs build.rs hardcodes `.generator("Ninja")` |
| C++ compiler | Yes | macOS: Xcode CLT, Windows: MSVC, Linux: g++/clang++ |
| Network | First build only | Downloads CEF ~80-100MB from Spotify CDN |

First build takes 3-10 minutes (CEF download + C++ compilation). Subsequent builds are fast. Use `CEF_PATH` with `export-cef-dir` to share one CEF copy across debug/release and avoid re-download on `cargo clean`.

### Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Version tracking** — Tauri upgrades wry, patch silently stops applying | Silent fallback to real wry | Pin and test against specific Tauri versions; CI checks version match |
| **Feature flags** — `tauri-runtime-wry` enables `protocol`, `os-webview`, `linux-body` | Build failure if features missing | Define all as no-ops in Cargo.toml |
| **`pub use wry;` re-export** — downstream plugins can access any wry API | Unbounded compatibility surface | Document supported surface; best-effort for common plugins |
| **API signature mismatch** — any `Result<T, E>` or trait bound difference | Compilation error | Mirror exact signatures from wry-api-surface.md |
| **tauri-runtime-wry drift** — upstream changes break our patch | Merge conflicts | Patch is ~200 lines across 6 files; tauri-runtime-wry changes infrequently; use `git format-patch` + periodic rebase |
| **macOS can't `cargo run`** — CEF needs .app bundle with helper processes | Dev experience friction | Provide bundler tool; document `DYLD_FALLBACK_LIBRARY_PATH` setup |

### Alternative Approaches (Evaluated)

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **Patch wry only** | Simplest for users (one line) | **Does NOT work** — 5 RED blockers in tauri-runtime-wry | Rejected by Spike 2 |
| **Patch wry + tauri-runtime-wry** (chosen) | Minimal changes; maintains upstream compatibility | Users patch two crates; need to maintain fork | Best balance of effort and compatibility |
| **Implement `tauri-runtime` traits** | Clean abstraction; no wry API needed | 71+ required methods on `WindowDispatch` alone; under-tested path | Future option for v1.0+ |
| **Fork `tauri-runtime-wry` only** | Control the glue layer | Still need wry-compatible API for the fork to import | Incomplete without patching wry too |

---

## Milestones

### v0.1 — Proof of Concept (macOS only)

- [ ] Workspace setup with `cef` + `cef-dll-sys` dependencies from crates.io
- [ ] build.rs: CMake/Ninja detection with friendly error messages
- [ ] CEF initializes via `CefInitialize()` with `external_message_pump = true`
- [ ] CFRunLoopTimer at 30fps integrated with tao's run loop (no tao changes needed)
- [ ] CEF browser window via `WindowInfo::set_as_child()` embedded in tao window
- [ ] Loads a URL via `WebViewBuilder::new_with_web_context().with_url().build()`
- [ ] Custom `ipc://` scheme handler works (fetch-based primary IPC path)
- [ ] Custom `tauri://` scheme handler works (asset serving)
- [ ] `window.ipc.postMessage` fallback via V8 extension + CefProcessMessage
- [ ] Cross-process script injection via CefProcessMessage (browser → renderer)
- [ ] Minimal `WebView` API: `evaluate_script`, `load_url`, `set_visible`
- [ ] macOS helper bundles generated (via cef-rs build_util::mac)
- [ ] Standalone example (no Tauri) demonstrating all core features

### v0.2 — Tauri Integration (macOS)

- [ ] Fork `tauri-runtime-wry` with CEF patch (~6 files, ~200 lines)
  - [ ] macOS `inner_size()` → read CEF browser view NSView frame
  - [ ] `Webview` struct → CEF handle types
  - [ ] `NewWindowResponse::Create` → CEF handle types
  - [ ] `WithWebview` handler → pack CEF handles
- [ ] Full WebViewBuilder API (35+ methods, stubs for unsupported ones)
- [ ] Full WebView API (20+ methods)
- [ ] macOS extension traits (`WebViewExtMacOS`, `WebViewExtDarwin`, `WebViewBuilderExtMacos`)
- [ ] Cookie management (via CEF cookie manager)
- [ ] DevTools open/close/is_open
- [ ] Navigation, drag-drop, page-load event handlers forwarded
- [ ] Feature flags defined (protocol, os-webview, linux-body, devtools, etc.)
- [ ] `tauri-demo` example: minimal Tauri app runs with wrymium via dual `[patch.crates-io]`

### v0.3 — Windows Support

- [ ] All v0.1 + v0.2 features on Windows
- [ ] `multi_threaded_message_loop = true` (Windows-specific message loop mode)
- [ ] Windows extension traits (`WebViewExtWindows`, `WebViewBuilderExtWindows`)
- [ ] tauri-runtime-wry patch: focus/fullscreen → `CefFocusHandler`/`CefDisplayHandler`
- [ ] CEF DLLs bundled correctly
- [ ] `set_theme`, `scroll_bar_style` support

### v0.4 — Linux Support

- [ ] All features on Linux (GTK3)
- [ ] `g_timeout_add` message pump integration with GLib main loop
- [ ] Linux extension traits (`WebViewExtUnix`, `WebViewBuilderExtUnix`)
- [ ] tauri-runtime-wry patch: undecorated_resizing → CEF widget events
- [ ] `build_gtk` compatibility method

### v1.0 — Stable Release

- [ ] API stable and documented
- [ ] Published to crates.io
- [ ] CI on all three platforms
- [ ] Performance benchmarks vs. system WebView
- [ ] Sandbox support (start with `--no-sandbox` in earlier milestones)
- [ ] Evaluate implementing `tauri-runtime` traits directly (alternative to patching wry)

---

## Upstream Contribution Strategy

wrymium will contribute generally-useful safe wrappers back to `tauri-apps/cef-rs`:

| Wrapper | cef-rs Status | Needed For |
|---------|--------------|------------|
| `CefSchemeHandlerFactory` / `CefResourceHandler` | **Not wrapped** | Custom protocol IPC |
| `CefV8Handler` / `CefV8Context` | **Not wrapped** | postMessage bridge |
| `CefRenderProcessHandler` | **Not wrapped** | Script injection |

These belong in the `cef` crate, not in wrymium. wrymium only contains wry/Tauri-specific integration logic. PRs will be opened against the `dev` branch.

---

## Open Questions

1. **CEF version cadence** — Pin to the `cef` crate version from crates.io. Currently at v146, tracks upstream within days. Update wrymium when cef-rs publishes new versions.

2. **Bundle size** — CEF Minimal is ~100MB unpacked. Communicate this tradeoff clearly in README and docs. Consider ZSTD-compressed distribution (~50-60MB download).

3. **wry API drift** — Pin the Tauri version we test against. CI should check that `tauri-runtime-wry`'s wry imports still resolve against wrymium. Version mismatch = CI failure, not silent fallback.

4. **Sandboxing** — CEF's renderer sandbox works with both IPC paths (fetch goes through CEF's network layer; CefProcessMessage works within sandbox). Start with `--no-sandbox` for PoC, enable in v1.0. macOS sandbox requires per-helper entitlements.

5. ~~**`tao` dependency**~~ — **Resolved**: wry 0.55 no longer depends on tao. It accepts any `HasWindowHandle`. wrymium similarly accepts `HasWindowHandle` and uses `set_as_child` to embed CEF into the provided window.

6. **`pub use wry;` boundary** — `tauri-runtime-wry` re-exports the entire wry crate. Third-party plugins may depend on arbitrary wry internals. wrymium can only guarantee the subset documented in [wry-api-surface.md](./wry-api-surface.md). Undocumented access will be handled on a best-effort basis.

---

## References

- [wry](https://github.com/tauri-apps/wry) — the library we're replacing (v0.55.0)
- [tauri-apps/cef-rs](https://github.com/tauri-apps/cef-rs) — CEF Rust bindings we build on
- [cef-builds.spotifycdn.com](https://cef-builds.spotifycdn.com) — prebuilt CEF binaries
- [tauri-runtime](https://docs.rs/tauri-runtime) — Tauri's runtime abstraction layer
- [raw-window-handle](https://github.com/rust-windowing/raw-window-handle) — platform-agnostic window handle (v0.6)
- [CEF C API](https://bitbucket.org/chromiumembedded/cef) — upstream CEF
- [wry Discussion #1014](https://github.com/tauri-apps/wry/discussions/1014) — "The future of wry" (CEF as separate crate)
- [wry Issue #703](https://github.com/tauri-apps/wry/issues/703) — CEF as universal fallback (closed)
- [cef-rs Issue #192](https://github.com/tauri-apps/cef-rs/issues/192) — Tracking: Integration with Tauri
- [cef-rs Issue #208](https://github.com/tauri-apps/cef-rs/issues/208) — Implement WRY interface
- [Tauri PR #7170](https://github.com/tauri-apps/tauri/pull/7170) — IPC refactor to URI schemes

### Supporting Documents

- [wry-api-surface.md](./wry-api-surface.md) — Exhaustive inventory of wry symbols used by tauri-runtime-wry
- [ipc-deep-dive.md](./ipc-deep-dive.md) — Technical analysis of Tauri 2.x IPC mechanism and CEF implementation design
- [spike-1-message-loop.md](./spike-1-message-loop.md) — tao + CEF message loop coexistence analysis
- [platform-trait-callsite-analysis.md](./platform-trait-callsite-analysis.md) — Platform extension trait call-site analysis (Spike 2)
- [TODO.md](./TODO.md) — Open issues, spike results, and remaining action items
