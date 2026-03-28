# Wry API Surface Required by `tauri-runtime-wry` v2.10.1

**Source**: `tauri-apps/tauri` `dev` branch, `crates/tauri-runtime-wry/`
**Wry version**: `0.55.0`

---

## 1. Wry Features Enabled by `tauri-runtime-wry`

From `Cargo.toml`:

```toml
wry = { version = "0.55.0", default-features = false, features = [
  "protocol",
  "os-webview",
  "linux-body",
] }
```

Conditional features activated via `tauri-runtime-wry` feature flags:
- `devtools` -> `wry/devtools`
- `x11` -> `wry/x11`
- `macos-private-api` -> `wry/fullscreen`, `wry/transparent`
- `tracing` -> `wry/tracing`
- `macos-proxy` -> `wry/mac-proxy`

---

## 2. Re-exports (expose full wry namespace downstream)

```rust
pub use wry;                    // Full crate re-export
pub use wry::webview_version;   // Free function re-export
```

This means **any** downstream user of `tauri-runtime-wry` can access `wry::*` directly. A drop-in replacement must therefore re-export its own crate under the name `wry` or provide a compatibility shim.

---

## 3. Core Types Imported from `wry`

| Symbol | Used As | Platform |
|---|---|---|
| `WebView` | Core struct, stored in `Rc<WebView>` inside `WebviewWrapper` | all |
| `WebViewBuilder` | Builder pattern to construct `WebView` | all |
| `WebContext` (aliased `WryWebContext`) | Manages shared web context / data directories | all |
| `Rect` | Struct `{ position, size }` for webview bounds | all |
| `DragDropEvent` (aliased `WryDragDropEvent`) | Enum for drag-drop events | all |
| `ProxyConfig` | Enum for proxy configuration (`Http`, `Socks5`) | all |
| `ProxyEndpoint` | Struct `{ host: String, port: String }` | all |
| `Error` | Error type (used as `wry::Error::MessageSender`) | Linux |
| `BackgroundThrottlingPolicy` | Enum: `Disabled`, `Suspend`, `Throttle` | all |
| `PageLoadEvent` | Enum: `Started`, `Finished` | all |
| `NewWindowResponse` | Enum: `Allow`, `Deny`, `Create { webview }` | all |
| `Theme` | Enum: `Dark`, `Light` | Windows |
| `ScrollBarStyle` (aliased `WryScrollBarStyle`) | Enum: `Default`, `FluentOverlay` | Windows |
| `JniHandle` | Android JNI handle type | Android |

---

## 4. Free Functions

| Function | Signature | Usage |
|---|---|---|
| `wry::webview_version()` | `fn webview_version() -> Result<String>` | Check if webview runtime is installed |

---

## 5. WebContext (`WryWebContext`) Methods

| Method | Signature |
|---|---|
| `new` | `fn new(data_directory: Option<PathBuf>) -> Self` |
| `set_allows_automation` | `fn set_allows_automation(&mut self, flag: bool)` |

---

## 6. WebViewBuilder Methods Called

All these are called on `WebViewBuilder` during webview creation:

### Constructor
| Method | Signature |
|---|---|
| `new_with_web_context` | `fn new_with_web_context(web_context: &mut WebContext) -> Self` |

### `.with_*()` Configuration Methods

| Method | Parameter Type(s) | Platform |
|---|---|---|
| `with_id` | `&str` | all |
| `with_url` | `&str` | all |
| `with_focused` | `bool` | all |
| `with_transparent` | `bool` | all |
| `with_accept_first_mouse` | `bool` | all |
| `with_incognito` | `bool` | all |
| `with_clipboard` | `bool` | all |
| `with_hotkeys_zoom` | `bool` | all |
| `with_bounds` | `wry::Rect` | all |
| `with_background_color` | `(u8, u8, u8, u8)` | all |
| `with_background_throttling` | `BackgroundThrottlingPolicy` | all |
| `with_javascript_disabled` | (no args) | all |
| `with_user_agent` | `&str` | all |
| `with_proxy_config` | `ProxyConfig` | all |
| `with_devtools` | `bool` | debug/devtools feature |
| `with_ipc_handler` | `Box<dyn Fn(Request<String>) + 'static>` | all |
| `with_initialization_script_for_main_only` | `String, bool` | all |
| `with_drag_drop_handler` | `impl Fn(DragDropEvent) -> bool + 'static` | all |
| `with_navigation_handler` | `impl Fn(String) -> bool + 'static` | all |
| `with_new_window_req_handler` | `impl Fn(String, NewWindowFeatures) -> NewWindowResponse + 'static` | all |
| `with_document_title_changed_handler` | `impl Fn(String) + 'static` | all |
| `with_download_started_handler` | `impl Fn(String, &mut PathBuf) -> bool + 'static` | all |
| `with_download_completed_handler` | `impl Fn(String, Option<PathBuf>, bool) + 'static` | all |
| `with_on_page_load_handler` | `impl Fn(PageLoadEvent, String) + 'static` | all |
| `with_asynchronous_custom_protocol` | `String, impl Fn(WebViewId, Request, Responder) + 'static` | all |
| `with_https_scheme` | `bool` | Windows, Android |
| `with_additional_browser_args` | `&str` | Windows |
| `with_environment` | (environment object) | Windows |
| `with_theme` | `wry::Theme` | Windows |
| `with_scroll_bar_style` | `ScrollBarStyle` | Windows |
| `with_browser_extensions_enabled` | `bool` | Windows |
| `with_extensions_path` | `&Path` | Windows, Linux |
| `with_related_view` | (webkit2gtk view) | Linux |
| `with_data_store_identifier` | `[u8; 16]` | macOS, iOS |
| `with_allow_link_preview` | `bool` | macOS, iOS |
| `with_webview_configuration` | (WKWebViewConfiguration) | macOS |
| `with_traffic_light_inset` | position | macOS |
| `with_input_accessory_view_builder` | `impl Fn(*mut c_void) -> *mut c_void + 'static` | iOS |
| `on_webview_created` | `impl Fn(CreationContext) -> Result<()> + 'static` | Android |

### Build Methods

| Method | Signature | Platform |
|---|---|---|
| `build` | `fn build(self, window: &Window) -> Result<WebView>` | Windows, macOS, iOS, Android |
| `build_as_child` | `fn build_as_child(self, window: &Window) -> Result<WebView>` | Windows, macOS, iOS, Android |
| `build_gtk` | `fn build_gtk(self, container: &impl IsA<gtk::Container>) -> Result<WebView>` | Linux |

---

## 7. WebView Instance Methods Called

These methods are called on `WebView` (or via `Deref` through `WebviewWrapper`):

| Method | Signature | Platform |
|---|---|---|
| `evaluate_script` | `fn evaluate_script(&self, script: &str) -> Result<()>` | all |
| `load_url` | `fn load_url(&self, url: &str) -> Result<()>` | all |
| `reload` | `fn reload(&self) -> Result<()>` | all |
| `set_visible` | `fn set_visible(&self, visible: bool) -> Result<()>` | all |
| `print` | `fn print(&self) -> Result<()>` | all |
| `set_bounds` | `fn set_bounds(&self, bounds: Rect) -> Result<()>` | all |
| `bounds` | `fn bounds(&self) -> Result<Rect>` | all |
| `url` | `fn url(&self) -> Result<String>` | all |
| `zoom` | `fn zoom(&self, scale_factor: f64) -> Result<()>` | all |
| `set_background_color` | `fn set_background_color(&self, color: (u8,u8,u8,u8)) -> Result<()>` | all |
| `clear_all_browsing_data` | `fn clear_all_browsing_data(&self) -> Result<()>` | all |
| `focus` | `fn focus(&self) -> Result<()>` | all |
| `cookies` | `fn cookies(&self) -> Result<Vec<Cookie>>` | all |
| `cookies_for_url` | `fn cookies_for_url(&self, url: &str) -> Result<Vec<Cookie>>` | all |
| `set_cookie` | `fn set_cookie(&self, cookie: &Cookie) -> Result<()>` | all |
| `delete_cookie` | `fn delete_cookie(&self, cookie: &Cookie) -> Result<()>` | all |
| `open_devtools` | `fn open_devtools(&self)` | debug/devtools |
| `close_devtools` | `fn close_devtools(&self)` | debug/devtools |
| `is_devtools_open` | `fn is_devtools_open(&self) -> bool` | debug/devtools |
| `set_theme` | `fn set_theme(&self, theme: Theme) -> Result<()>` | Windows |
| `reparent` | platform-specific, see below | all desktop |

### WebView Static Methods

| Method | Signature | Platform |
|---|---|---|
| `WebView::fetch_data_store_identifiers` | `fn fetch_data_store_identifiers(cb: Box<dyn FnOnce(Vec<[u8;16]>) + Send>) -> Result<()>` | macOS, iOS |
| `WebView::remove_data_store` | `fn remove_data_store(uuid: &[u8;16], cb: impl FnOnce(Result<()>) + Send)` | macOS, iOS |

---

## 8. Platform Extension Traits

### `WebViewExtWindows` (Windows)

```rust
use wry::WebViewExtWindows;
```

Methods used:
| Method | Signature |
|---|---|
| `controller()` | `fn controller(&self) -> ICoreWebView2Controller` |
| `environment()` | `fn environment(&self) -> ICoreWebView2Environment` |
| `reparent()` | `fn reparent(&self, hwnd: isize) -> Result<()>` |

### `WebViewBuilderExtWindows` (Windows)

```rust
use wry::WebViewBuilderExtWindows;
```

Methods used (called via `with_*` methods on the builder):
- `with_additional_browser_args(&str)`
- `with_environment(...)`
- `with_theme(Theme)`
- `with_scroll_bar_style(ScrollBarStyle)`
- `with_browser_extensions_enabled(bool)`

### `WebViewExtMacOS` (macOS, via `WebViewExtDarwin` + `WebViewExtMacOS`)

```rust
use wry::{WebViewBuilderExtDarwin, WebViewExtDarwin};
use wry::WebViewBuilderExtMacos;
```

Methods used on `WebViewExtDarwin`:
| Method | Signature |
|---|---|
| (none directly - traits imported for availability) | |

Methods used on `WebViewExtMacOS` (imported locally):
| Method | Signature |
|---|---|
| `webview()` | `fn webview(&self) -> Retained<WKWebView>` |
| `manager()` | `fn manager(&self) -> Retained<WKUserContentController>` |
| `ns_window()` | `fn ns_window(&self) -> Retained<NSWindow>` |
| `reparent()` | `fn reparent(&self, window: *mut c_void) -> Result<()>` |

`WebViewBuilderExtMacos` methods used:
- `with_webview_configuration(...)`
- `with_traffic_light_inset(...)`

`WebViewBuilderExtDarwin` methods used:
- `with_data_store_identifier([u8; 16])`
- `with_allow_link_preview(bool)`

### `WebViewExtIOS` (iOS)

```rust
use wry::WebViewBuilderExtIos;
```

Methods used on `WebViewExtIOS`:
| Method | Signature |
|---|---|
| `webview()` | `fn webview(&self) -> Retained<WKWebView>` |
| `manager()` | `fn manager(&self) -> Retained<WKUserContentController>` |

`WebViewBuilderExtIos` methods used:
- `with_input_accessory_view_builder(...)`

### `WebViewExtUnix` (Linux/BSD)

```rust
use wry::{WebViewBuilderExtUnix, WebViewExtUnix};
```

Methods used on `WebViewExtUnix`:
| Method | Signature |
|---|---|
| `webview()` | `fn webview(&self) -> webkit2gtk::WebView` |
| `reparent()` | `fn reparent(&self, container: &impl IsA<gtk::Container>) -> Result<()>` |

`WebViewBuilderExtUnix` methods used:
- `with_related_view(...)`

### `WebViewExtAndroid` / `WebViewBuilderExtAndroid` (Android)

```rust
use wry::{WebViewBuilderExtAndroid, WebViewExtAndroid};
use wry::prelude::{dispatch, find_class};
```

Methods used on `WebViewExtAndroid`:
| Method | Signature |
|---|---|
| `handle()` | `fn handle(&self) -> JniHandle` |

`WebViewBuilderExtAndroid` methods used:
- `on_webview_created(impl Fn(CreationContext) -> Result<()>)`

### `wry::prelude` (Android)

| Symbol | Type |
|---|---|
| `dispatch` | function |
| `find_class` | function |

---

## 9. Wry Enums and Their Variants Used

### `DragDropEvent`
- `Enter { paths: Vec<PathBuf>, position: (f64, f64) }`
- `Over { position: (f64, f64) }`
- `Drop { paths: Vec<PathBuf>, position: (f64, f64) }`
- `Leave`

### `BackgroundThrottlingPolicy`
- `Disabled`
- `Suspend`
- `Throttle`

### `PageLoadEvent`
- `Started`
- `Finished`

### `NewWindowResponse`
- `Allow`
- `Deny`
- `Create { webview: ... }` (platform-specific webview handle)

### `ProxyConfig`
- `Http(ProxyEndpoint)`
- `Socks5(ProxyEndpoint)`

### `Theme` (Windows only)
- `Dark`
- `Light`

### `ScrollBarStyle` (Windows only)
- `Default`
- `FluentOverlay`

### `Error`
- `MessageSender` variant referenced on Linux

---

## 10. Wry Structs and Their Fields Used

### `Rect`
```rust
pub struct Rect {
  pub position: dpi::Position,  // from raw-window-handle/dpi crate
  pub size: dpi::Size,
}
```

### `ProxyEndpoint`
```rust
pub struct ProxyEndpoint {
  pub host: String,
  pub port: String,
}
```

### `WebContext`
- Constructor: `new(Option<PathBuf>)`
- Method: `set_allows_automation(bool)`

### `NewWindowFeatures` (passed to new_window_req_handler)
Fields accessed:
- `size`
- `position`
- `opener.webview` (desktop)
- `opener.environment` (Windows)
- `opener.target_configuration` (macOS)

---

## 11. Re-exported Crates (via `wry`)

Wry 0.55 re-exports these, and tauri-runtime-wry uses types from them:
- `http` (v1) - `Request<String>` used for IPC
- `dpi` types - `Position`, `Size`, `LogicalPosition`, `LogicalSize`, `PhysicalPosition`, `PhysicalSize` used in `Rect`
- `raw-window-handle` (v0.6) - used independently by tauri-runtime-wry

---

## 12. Cookie Type

The `Cookie` type is used extensively:
- `webview.cookies() -> Result<Vec<Cookie>>`
- `webview.cookies_for_url(url) -> Result<Vec<Cookie>>`
- `webview.set_cookie(&cookie) -> Result<()>`
- `webview.delete_cookie(&cookie) -> Result<()>`

This is likely `wry::Cookie` or re-exported from a cookie crate.

---

## Summary: Minimum API Surface for Drop-in Replacement

A drop-in replacement (wrymium) must provide:

1. **1 free function**: `webview_version()`
2. **3 core structs**: `WebView`, `WebViewBuilder`, `WebContext`
3. **1 geometry struct**: `Rect`
4. **2 proxy structs**: `ProxyConfig` (enum), `ProxyEndpoint`
5. **5 enums**: `DragDropEvent`, `BackgroundThrottlingPolicy`, `PageLoadEvent`, `NewWindowResponse`, `Theme`, `ScrollBarStyle`
6. **1 error type**: `Error` with at least `MessageSender` variant
7. **~35 builder methods** on `WebViewBuilder`
8. **~20 instance methods** on `WebView` + 2 static methods
9. **6 platform extension traits** with their methods
10. **3 build methods**: `build()`, `build_as_child()`, `build_gtk()`
11. **Full crate re-export** (`pub use wry;`)
12. **Android prelude**: `dispatch`, `find_class`, `JniHandle`
13. **Cookie type** with full CRUD operations
