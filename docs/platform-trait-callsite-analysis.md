# Platform Extension Trait Call-Site Analysis

**Source**: `tauri-apps/tauri` `dev` branch, `crates/tauri-runtime-wry/`
**Wry version**: `0.55.0`
**Date**: 2026-03-28

---

## 1. Complete Call-Site Inventory

### 1.1 `WebViewExtMacOS::webview()` -> `Retained<WryWebView>`

**Call site A** -- `lib.rs:3931` -- `WebviewMessage::WithWebview` handler
```rust
use wry::WebViewExtMacOS;
f(Webview {
    webview: Retained::into_raw(webview.webview()) as *mut c_void,
    manager: Retained::into_raw(webview.manager()) as *mut c_void,
    ns_window: Retained::into_raw(webview.ns_window()) as *mut c_void,
});
```
- **Purpose**: Exposes raw native pointers to downstream Tauri users via `Webview::with_webview()`. The returned WKWebView pointer is packed into the `webview::Webview` struct (defined in `webview.rs`) and passed to a user-provided closure.
- **Critical path**: Only executed when a Tauri app calls `webview.with_webview(|wv| { ... })`. This is an opt-in API for advanced users who need native access.
- **Frequency**: Rare -- only used by apps that need direct WKWebView manipulation.

**Call site B** -- `lib.rs:4823` -- `NewWindowResponse::Create` construction
```rust
webview: wry::WebViewExtMacOS::webview(&*webview).as_super().into(),
```
- **Purpose**: When a new-window request is handled by creating a new window (multi-window popup), the native WKWebView handle must be passed back to wry's `NewWindowResponse::Create` variant so wry can associate the new webview with the opener.
- **Critical path**: Only executed when `NewWindowResponse::Create` is returned from the new-window handler. This is a desktop-only, opt-in feature for popup windows.
- **Frequency**: Low -- only for apps that opt into popup window creation.

**Call site C** -- `lib.rs:5265` -- `inner_size()` helper function (macOS-only)
```rust
use wry::WebViewExtMacOS;
let view = unsafe { Retained::cast_unchecked::<NSView>(webview.webview()) };
let view_frame = view.frame();
```
- **Purpose**: On macOS, when computing window inner size for a window with a single webview (no children), the function gets the WKWebView's frame directly rather than relying on the tao window's reported size. This is a workaround for macOS-specific size reporting discrepancies.
- **Critical path**: YES -- called during window resize calculations whenever a macOS window has exactly one webview and no child windows.
- **Frequency**: High on macOS -- called on every resize event.

### 1.2 `WebViewExtMacOS::manager()` -> `Retained<WKUserContentController>`

**Call site** -- `lib.rs:3933` -- `WebviewMessage::WithWebview` handler
```rust
manager: Retained::into_raw(webview.manager()) as *mut c_void,
```
- **Purpose**: Same as `webview()` call site A -- exposes the WKUserContentController to downstream users via `with_webview()`.
- **Critical path**: Only when `with_webview()` is called.
- **Frequency**: Rare.

### 1.3 `WebViewExtMacOS::ns_window()` -> `Retained<NSWindow>`

**Call site** -- `lib.rs:3935` -- `WebviewMessage::WithWebview` handler
```rust
ns_window: Retained::into_raw(webview.ns_window()) as *mut c_void,
```
- **Purpose**: Same as above -- exposes the NSWindow pointer to downstream users via `with_webview()`.
- **Critical path**: Only when `with_webview()` is called.
- **Frequency**: Rare.

**NOTE**: The `ns_window()` calls in `window/macos.rs` lines 13, 35, 40 are `tao::platform::macos::WindowExtMacOS::ns_window()` (from tao, NOT wry). Those are unrelated to wry's extension trait.

### 1.4 `WebViewExtMacOS::reparent()`

**Call site** -- `lib.rs:3644`
```rust
use wry::WebViewExtMacOS;
webview.inner.reparent(new_parent_window.ns_window() as _)
```
- **Purpose**: Moves a webview from one window to another. The `ns_window()` here is from tao (getting the new parent's NSWindow handle), and `reparent()` is from wry (moving the webview into it).
- **Critical path**: Only when Tauri's `webview.reparent()` API is called explicitly by an application.
- **Frequency**: Rare -- advanced multi-window API.

### 1.5 `WebViewBuilderExtMacos::with_webview_configuration()`

**Call site** -- `lib.rs:4708`
```rust
if let Some(webview_configuration) = webview_attributes.webview_configuration {
    webview_builder = webview_builder.with_webview_configuration(webview_configuration);
}
```
- **Purpose**: Allows passing a pre-configured `WKWebViewConfiguration` object to the webview builder. Used for sharing configuration between opener and popup windows (required for `window.open()` to work correctly on macOS).
- **Critical path**: Conditional -- only when `webview_configuration` is explicitly provided. Required for popup/child window creation on macOS.
- **Frequency**: Low -- only for multi-window popup scenarios.

### 1.6 `WebViewBuilderExtDarwin::with_data_store_identifier()`

**Call site** -- `lib.rs:4989`
```rust
if let Some(data_store_identifier) = &webview_attributes.data_store_identifier {
    webview_builder = webview_builder.with_data_store_identifier(*data_store_identifier);
}
```
- **Purpose**: Sets a UUID-based identifier for the WKWebsiteDataStore, allowing data isolation between webviews.
- **Critical path**: Conditional -- only when explicitly configured.
- **Frequency**: Low.

### 1.7 `WebViewExtWindows::controller()` -> `ICoreWebView2Controller`

**Call site A** -- `lib.rs:3956` -- `WebviewMessage::WithWebview` handler
```rust
f(Webview {
    controller: webview.controller(),
    environment: webview.environment(),
});
```
- **Purpose**: Exposes the WebView2 controller to downstream users via `with_webview()`.
- **Critical path**: Only when `with_webview()` is called.
- **Frequency**: Rare.

**Call site B** -- `lib.rs:5134` -- Post-creation event handler setup
```rust
let controller = webview.controller();
// ... sets up GotFocus, LostFocus, and ContainsFullScreenElementChanged handlers
controller.add_GotFocus(...);
controller.add_LostFocus(...);
controller.CoreWebView2().add_ContainsFullScreenElementChanged(...);
```
- **Purpose**: After creating a webview, tauri-runtime-wry hooks into the WebView2 controller to:
  1. Track focus changes between webviews (for multi-webview focus events)
  2. Detect fullscreen element changes (for HTML5 fullscreen API)
- **Critical path**: YES -- always executed during webview creation on Windows. This is essential for correct focus management and fullscreen behavior.
- **Frequency**: Once per webview creation (high importance).

### 1.8 `WebViewExtWindows::environment()` -> `ICoreWebView2Environment`

**Call site** -- `lib.rs:3957` -- `WebviewMessage::WithWebview` handler
```rust
environment: webview.environment(),
```
- **Purpose**: Exposes the WebView2 environment to downstream users via `with_webview()`.
- **Critical path**: Only when `with_webview()` is called.
- **Frequency**: Rare.

### 1.9 `WebViewExtWindows::reparent()`

**Call site** -- `lib.rs:3647`
```rust
let reparent_result = { webview.inner.reparent(new_parent_window.hwnd()) };
```
- **Purpose**: Moves a webview between windows on Windows.
- **Critical path**: Only when `webview.reparent()` is explicitly called.
- **Frequency**: Rare.

### 1.10 `WebViewExtWindows::webview()` -> `ICoreWebView2`

**Call site** -- `lib.rs:4833` -- `NewWindowResponse::Create` construction
```rust
#[cfg(windows)]
webview: webview.webview(),
```
- **Purpose**: Gets the ICoreWebView2 interface to pass back to wry for popup window association.
- **Critical path**: Only for `NewWindowResponse::Create` (popup windows).
- **Frequency**: Low.

### 1.11 `WebViewExtUnix::webview()` -> `webkit2gtk::WebView`

**Call site A** -- `lib.rs:3925` -- `WebviewMessage::WithWebview` handler
```rust
f(webview.webview());
```
- **Purpose**: Exposes the WebKitGTK WebView to downstream users via `with_webview()`.
- **Critical path**: Only when `with_webview()` is called.
- **Frequency**: Rare.

**Call site B** -- `undecorated_resizing.rs:532` -- `attach_resize_handler()`
```rust
use wry::WebViewExtUnix;
let webview = webview.webview();
webview.add_events(...);
webview.connect_button_press_event(...);
webview.connect_touch_event(...);
```
- **Purpose**: Attaches mouse/touch event handlers directly to the GTK webview widget to implement borderless window resizing. The native WebKitGTK widget is needed to intercept low-level GTK events.
- **Critical path**: YES -- always executed during webview creation on Linux when the window is undecorated.
- **Frequency**: Once per webview creation on Linux with undecorated windows.

**Call site C** -- `lib.rs:4831` -- `NewWindowResponse::Create` construction
```rust
webview: webview.webview(),
```
- **Purpose**: Gets the webkit2gtk::WebView for popup window association.
- **Critical path**: Only for popup windows.
- **Frequency**: Low.

### 1.12 `WebViewExtUnix::reparent()`

**Call site** -- `lib.rs:3658`
```rust
webview.inner.reparent(container)
```
- **Purpose**: Moves a webview between GTK containers (windows) on Linux.
- **Critical path**: Only when `webview.reparent()` is explicitly called.
- **Frequency**: Rare.

---

## 2. Purpose Classification

### 2a. Reparenting (moving webview between windows)
- `WebViewExtMacOS::reparent()` -- lib.rs:3644
- `WebViewExtWindows::reparent()` -- lib.rs:3647
- `WebViewExtUnix::reparent()` -- lib.rs:3658

All three are triggered by the same code path: when a Tauri application calls `webview.reparent(new_window)`. This is a multi-window management API.

### 2b. Exposing native handles to downstream consumers (`with_webview` API)
- `WebViewExtMacOS::webview()` -- lib.rs:3931
- `WebViewExtMacOS::manager()` -- lib.rs:3933
- `WebViewExtMacOS::ns_window()` -- lib.rs:3935
- `WebViewExtWindows::controller()` -- lib.rs:3956
- `WebViewExtWindows::environment()` -- lib.rs:3957
- `WebViewExtUnix::webview()` -- lib.rs:3925

All packed into the `webview::Webview` struct (defined in `webview.rs`) and passed to user closures. The struct definition per platform:

**macOS**: `{ webview: *mut c_void, manager: *mut c_void, ns_window: *mut c_void }`
**Windows**: `{ controller: ICoreWebView2Controller, environment: ICoreWebView2Environment }`
**Linux**: `webkit2gtk::WebView` (type alias, not struct)

### 2c. Internal platform-specific functionality (always executed)
- `WebViewExtMacOS::webview()` at lib.rs:5265 -- macOS inner_size calculation (CRITICAL)
- `WebViewExtWindows::controller()` at lib.rs:5134 -- Windows focus/fullscreen event setup (CRITICAL)
- `WebViewExtUnix::webview()` at undecorated_resizing.rs:532 -- Linux borderless resize (CRITICAL for undecorated windows)

### 2d. Popup window creation (NewWindowResponse::Create)
- `WebViewExtMacOS::webview()` at lib.rs:4823
- `WebViewExtWindows::webview()` at lib.rs:4833
- `WebViewExtUnix::webview()` at lib.rs:4831

### 2e. Builder configuration (passthrough)
- `WebViewBuilderExtMacos::with_webview_configuration()` at lib.rs:4708
- `WebViewBuilderExtDarwin::with_data_store_identifier()` at lib.rs:4989

---

## 3. Reparenting Deep Dive

### What triggers reparenting in Tauri?

Reparenting is triggered when a Tauri application calls `webview.reparent(new_window_label)`. This sends a `WebviewMessage::Reparent` through the event loop, which is handled at lib.rs:3630-3679. The flow:

1. Find the webview in its current parent window's webview list
2. Remove it from the old parent
3. Find the new parent window
4. Call `wry::WebView::reparent()` with the new parent's native handle
5. Move the webview struct into the new parent's webview list

### Can wrymium implement reparent using CEF?

CEF's `CefBrowserHost` has `SetWindowParent()` (on Windows) and the browser can be associated with different windows. However:

- **Windows**: `CefBrowserHost::GetWindowHandle()` returns the browser's HWND. Reparenting could use `SetParent()` Win32 API. FEASIBLE.
- **macOS**: CEF embeds itself as an NSView in the window's view hierarchy. Reparenting would require removing the CefBrowserView from one NSWindow's contentView and adding it to another. FEASIBLE with care.
- **Linux**: CEF on Linux uses X11 window embedding. Reparenting is possible via `XReparentWindow()` or GTK container manipulation if using CefBrowserView. FEASIBLE.

**Verdict**: Reparenting is implementable with CEF, though the API surface differs from wry's. wrymium can provide `reparent()` methods that accept the same parameter types (HWND on Windows, `*mut NSWindow` on macOS, `&gtk::Container` on Linux) and internally manipulate CEF's window embedding.

---

## 4. `wry::Error` Type Analysis

### Complete definition (from wry 0.55 `src/error.rs`):

```rust
#[non_exhaustive]
pub enum Error {
    // Linux-only (cfg(gtk))
    GlibError(gtk::glib::Error),
    GlibBoolError(gtk::glib::BoolError),
    MissingManager,
    X11DisplayNotFound,
    XlibError(x11_dl::error::OpenError),          // cfg(gtk, feature = "x11")

    // Cross-platform
    InitScriptError,
    RpcScriptError(String, String),
    NulError(std::ffi::NulError),
    ReceiverError(std::sync::mpsc::RecvError),
    ReceiverTimeoutError(crossbeam_channel::RecvTimeoutError),  // Android
    SenderError(std::sync::mpsc::SendError<String>),
    MessageSender,
    Io(std::io::Error),
    HttpError(http::Error),
    Infallible(std::convert::Infallible),
    ProxyEndpointCreationFailed,
    WindowHandleError(raw_window_handle::HandleError),
    UnsupportedWindowHandle,
    Utf8Error(std::str::Utf8Error),
    NotMainThread,
    CustomProtocolTaskInvalid,
    UrlSchemeRegisterError(String),
    DuplicateCustomProtocol(String),
    ContextDuplicateCustomProtocol(String),

    // Windows-only
    WebView2Error(webview2_com::Error),

    // Android-only
    JniError(jni::errors::Error),
    CrossBeamRecvError(crossbeam_channel::RecvError),
    ActivityNotFound,

    // Apple-only (macOS + iOS)
    UrlParse(url::ParseError),
    DataStoreInUse,
}
```

### Which variants does tauri-runtime-wry match against?

**Only one**: `wry::Error::MessageSender` at lib.rs:3660 -- used as a fallback error when the Linux vbox container is not available during reparenting. tauri-runtime-wry never pattern-matches on the Error enum; it only constructs this one variant and otherwise treats errors opaquely (logging them or converting to its own error type).

### Implication for wrymium

wrymium's `Error` type needs:
1. The `MessageSender` variant (used directly in construction)
2. `Display` and `Debug` implementations (for logging)
3. The type must be constructible for all variants that wry's methods might return (since wrymium replaces wry)
4. Since the enum is `#[non_exhaustive]`, wrymium has freedom to define a subset of variants

In practice, wrymium can define a simplified Error enum with the most relevant variants and map CEF errors into them.

---

## 5. Compatibility Verdict Per Method

### macOS

| Method | Verdict | Rationale |
|--------|---------|-----------|
| `WebViewExtMacOS::webview()` | **RED** | Called in critical path (inner_size at lib.rs:5265). Returns WKWebView which CEF does not have. Also needed for with_webview and NewWindowResponse. |
| `WebViewExtMacOS::manager()` | **YELLOW** | Only used in with_webview handler. Can return a dummy/null pointer since CEF has no WKUserContentController. |
| `WebViewExtMacOS::ns_window()` | **YELLOW** | Only used in with_webview handler. CEF can provide the parent NSWindow. |
| `WebViewExtMacOS::reparent()` | **GREEN** | Can be implemented with CEF's view hierarchy manipulation. |
| `WebViewBuilderExtMacos::with_webview_configuration()` | **RED** | Accepts WKWebViewConfiguration. CEF has no equivalent. Cannot accept this type at all. |
| `WebViewBuilderExtDarwin::with_data_store_identifier()` | **YELLOW** | Can accept the UUID and use it to configure CEF's cache path. Semantic equivalent exists. |

### Windows

| Method | Verdict | Rationale |
|--------|---------|-----------|
| `WebViewExtWindows::controller()` | **RED** | Called in critical path (focus/fullscreen setup at lib.rs:5134). Returns ICoreWebView2Controller which CEF does not have. |
| `WebViewExtWindows::environment()` | **YELLOW** | Only used in with_webview handler. |
| `WebViewExtWindows::webview()` | **YELLOW** | Only used in NewWindowResponse::Create (popup feature). |
| `WebViewExtWindows::reparent()` | **GREEN** | Can be implemented with SetParent Win32 API on CEF's HWND. |

### Linux

| Method | Verdict | Rationale |
|--------|---------|-----------|
| `WebViewExtUnix::webview()` | **RED** | Called in critical path (undecorated_resizing.rs:532). Returns webkit2gtk::WebView which CEF does not have. Also needed for with_webview and NewWindowResponse. |
| `WebViewExtUnix::reparent()` | **GREEN** | Can be implemented with GTK container manipulation on CEF's widget. |

---

## 6. RED Blockers -- Detailed Analysis

### RED #1: `WebViewExtMacOS::webview()` in `inner_size()` (lib.rs:5256-5269)

The function computes the actual content size by reading the WKWebView's NSView frame. This is called because on macOS, `tao::Window::inner_size()` may not account for the webview's actual layout.

**wrymium mitigation options**:
- Return CEF's browser view frame instead (CefBrowserView extends NSView)
- Requires either: (a) returning a CEF view that can be cast to NSView, or (b) patching tauri-runtime-wry to use a different size-query mechanism

### RED #2: `WebViewExtWindows::controller()` in focus/fullscreen setup (lib.rs:5132-5220)

This is ~90 lines of critical Windows code that hooks into WebView2's COM interfaces for focus tracking and fullscreen detection. The `ICoreWebView2Controller` interface is deeply WebView2-specific.

**wrymium mitigation options**:
- CEF has `CefFocusHandler` for focus events and can detect fullscreen via `CefDisplayHandler::OnFullscreenModeChange`
- Requires patching tauri-runtime-wry to use wrymium's own event callbacks instead of WebView2 COM interfaces

### RED #3: `WebViewExtUnix::webview()` in `undecorated_resizing.rs` (line 532)

Gets the webkit2gtk::WebView to attach GTK event handlers for borderless window resize. The entire function operates on `webkit2gtk::WebView` methods (`add_events`, `connect_button_press_event`, `connect_touch_event`).

**wrymium mitigation options**:
- CEF on Linux creates its own X11/GTK widget. Could expose a GtkWidget that supports similar event connections
- Requires either: (a) returning a compatible GtkWidget, or (b) patching undecorated_resizing.rs

### RED #4: `with_webview_configuration()` (lib.rs:4708)

Accepts `Retained<WKWebViewConfiguration>` from objc2_web_kit. CEF fundamentally cannot accept this type.

**wrymium mitigation options**:
- Accept the parameter and silently ignore it (lossy but compiles)
- This is conditional (`if let Some(...)`) so it only fires when explicitly set
- However, it is REQUIRED for macOS popup windows to share configuration with their opener
- Patching tauri-runtime-wry to not pass this would break macOS popup functionality

### RED #5: `NewWindowResponse::Create` (lib.rs:4821-4834)

The `Create` variant carries a native webview handle that differs per platform:
- macOS: `Retained<WKWebView>`
- Windows: `ICoreWebView2`
- Linux: `webkit2gtk::WebView`

CEF has none of these types. wrymium would need to redefine `NewWindowResponse::Create` with CEF-compatible types.

**Impact**: This only matters for the popup-window-creation feature. If wrymium does not support `NewWindowResponse::Create`, it can return `Allow` or `Deny` instead, and popup windows would open in the default system browser or be blocked.

---

## 7. `with_webview` API Impact Assessment

The `Webview::with_webview()` API (defined in `tauri/src/webview/mod.rs:1645`) is Tauri's escape hatch for native access. The struct passed to user closures is defined in `tauri-runtime-wry/src/webview.rs`:

```rust
// macOS
pub struct Webview {
    pub webview: *mut c_void,    // WKWebView pointer
    pub manager: *mut c_void,    // WKUserContentController pointer
    pub ns_window: *mut c_void,  // NSWindow pointer
}

// Windows
pub struct Webview {
    pub controller: ICoreWebView2Controller,
    pub environment: ICoreWebView2Environment,
}

// Linux
pub type Webview = webkit2gtk::WebView;
```

**Key insight**: These are raw native types that user code consumes. If wrymium replaces wry, `with_webview` callers would receive CEF handles instead of WebKit/WebView2 handles. This is a **semantic break** -- the types compile but the values are meaningless for users expecting WebKit/WebView2 APIs.

However, this is documented as an unstable API ("pin Tauri to at least a minor version when using with_webview"), and most Tauri apps do not use it. For wrymium's target use case, this is acceptable breakage.

---

## 8. Recommendation: Patch Strategy

### Verdict: wrymium MUST also patch tauri-runtime-wry

Patching only wry is insufficient because:

1. **Type-level incompatibilities in critical paths**: The `inner_size()` (macOS), focus/fullscreen setup (Windows), and `undecorated_resizing` (Linux) code paths use platform-native types from WebKit/WebView2/WebKitGTK that CEF simply cannot provide. These are not behind `with_webview` -- they are internal to tauri-runtime-wry.

2. **`NewWindowResponse::Create` carries native types**: The enum variant's fields are platform-specific native webview handles. wrymium would need to change these field types.

3. **`webview.rs` struct definitions**: The `Webview` struct exposed to `with_webview` users hard-codes WebKit/WebView2 types.

### Recommended Patch Scope

**Tier 1 -- Must patch in tauri-runtime-wry** (otherwise it will not compile):
- `webview.rs` -- Redefine the `Webview` platform structs with CEF-equivalent types
- `lib.rs:5132-5220` -- Replace Windows focus/fullscreen COM hookup with CEF event handlers
- `lib.rs:5256-5269` -- Replace macOS inner_size WKWebView frame query with CEF view query
- `undecorated_resizing.rs:524-600` -- Replace Linux webkit2gtk event hookup with CEF widget events
- `lib.rs:4821-4834` -- Redefine or remove `NewWindowResponse::Create` native handle fields
- `lib.rs:3916-3963` -- Replace `WithWebview` handler to pack CEF handles instead of native ones

**Tier 2 -- Can be handled in wrymium (wry replacement) with stubs**:
- `reparent()` on all platforms -- wrymium implements using CEF's window manipulation
- `with_data_store_identifier()` -- accept and map to CEF cache configuration
- `with_webview_configuration()` -- accept and silently ignore (or map what is mappable)
- `wry::Error` -- define compatible enum with `MessageSender` variant

**Tier 3 -- No change needed**:
- All `WebViewBuilder::with_*()` configuration methods -- wrymium maps these to CEF settings
- All `WebView` instance methods (evaluate_script, load_url, etc.) -- wrymium implements these
- `webview_version()` -- wrymium returns CEF version

### Concrete Approach

1. **Fork tauri-runtime-wry** alongside the wry replacement. Maintain a thin patch set (~6 files, ~200 lines of changes) that replaces native-type-dependent code with CEF equivalents.

2. **Keep the patch minimal and mechanical**: Replace type references and ~3 critical code blocks. The rest of tauri-runtime-wry (message loop, window management, builder wiring) works unchanged.

3. **Use `cfg` feature gates**: Add a `cef` feature to the forked tauri-runtime-wry that conditionally compiles the CEF paths. This keeps the patch maintainable across tauri version updates.

4. **Automated patch tracking**: Since tauri-runtime-wry changes infrequently (it is a thin glue layer), a git-based patch series (`git format-patch`) rebased on each tauri release is practical.

---

## 9. Summary Table

| Method | Call Sites | Critical Path? | Verdict |
|--------|-----------|---------------|---------|
| `WebViewExtMacOS::webview()` | 3 (with_webview, NewWindowResponse, inner_size) | YES (inner_size) | RED |
| `WebViewExtMacOS::manager()` | 1 (with_webview) | No | YELLOW |
| `WebViewExtMacOS::ns_window()` | 1 (with_webview) | No | YELLOW |
| `WebViewExtMacOS::reparent()` | 1 (reparent handler) | No | GREEN |
| `WebViewBuilderExtMacos::with_webview_configuration()` | 1 (builder) | Conditional | RED |
| `WebViewBuilderExtDarwin::with_data_store_identifier()` | 1 (builder) | Conditional | YELLOW |
| `WebViewExtWindows::controller()` | 2 (with_webview, focus/fullscreen) | YES (focus setup) | RED |
| `WebViewExtWindows::environment()` | 1 (with_webview) | No | YELLOW |
| `WebViewExtWindows::webview()` | 1 (NewWindowResponse) | No | YELLOW |
| `WebViewExtWindows::reparent()` | 1 (reparent handler) | No | GREEN |
| `WebViewExtUnix::webview()` | 3 (with_webview, NewWindowResponse, resize) | YES (resize) | RED |
| `WebViewExtUnix::reparent()` | 1 (reparent handler) | No | GREEN |
| `wry::Error` | 1 (construct MessageSender) | No | GREEN |
