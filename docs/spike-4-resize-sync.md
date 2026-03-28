# Spike 4: CEF Browser View Resize Synchronization

> How to keep a CEF browser view in sync with a parent tao/winit window on macOS, Windows, and Linux.

---

## Executive Summary

**Conclusion: macOS auto-resizes for free; Windows and Linux require manual resize handling.**

CEF's internal implementation on macOS sets `NSViewWidthSizable | NSViewHeightSizable` autoresizing masks on the browser view when it is created as a child. This means the browser view automatically tracks the parent view's size with zero application code. On Windows and Linux, the CEF child window does NOT auto-resize; the host application must detect parent size changes and call `SetWindowPos` (Windows) or `XConfigureWindow` (Linux) on the CEF browser's window handle.

| Platform | Auto-resize? | Manual work needed | API for manual resize |
|----------|-------------|--------------------|-----------------------|
| macOS | **Yes** | None (Cocoa handles it) | `setFrame:` on NSView (fallback only) |
| Windows | **No** | Handle `WM_SIZE`, call `SetWindowPos` on browser HWND | `CefBrowserHost::window_handle()` to get HWND |
| Linux | **No** | Handle `ConfigureNotify`, call `XConfigureWindow` on browser X11 window | `CefBrowserHost::window_handle()` to get X11 Window |

---

## Platform Analysis

### macOS

#### What CEF creates internally

When `WindowInfo::set_as_child(ns_view)` is used, CEF's `BrowserPlatformDelegateNativeMac::CreateHostWindow()` creates a `CefBrowserHostView` (a custom NSView subclass) and adds it as a subview of the provided parent NSView. The critical code from CEF source (`browser_platform_delegate_native_mac.mm`):

```objc
// CEF internal code (not wrymium code)
CefBrowserHostView* browser_view =
    [[CefBrowserHostView alloc] initWithFrame:browser_view_rect];

[parent_view addSubview:browser_view];
[browser_view setAutoresizingMask:(NSViewWidthSizable | NSViewHeightSizable)];

NSView* native_view = web_contents_->GetNativeView().GetNativeNSView();
[browser_view addSubview:native_view];
[native_view setFrame:bounds];
[native_view setAutoresizingMask:(NSViewWidthSizable | NSViewHeightSizable)];
```

Key observations:

1. **CEF sets autoresizing masks internally.** Both the `CefBrowserHostView` and the inner Chromium content view (`native_view`) have `NSViewWidthSizable | NSViewHeightSizable` set. This means they automatically resize when the parent view resizes.

2. **cef-rs's `WindowInfo::set_as_child()` on macOS** sets `parent_view` to the provided NSView handle and sets `hidden: 0`. It does NOT set any autoresizing flags itself -- it does not need to, because CEF's internal `CreateHostWindow` sets them after creating the browser view.

3. **The cefclient example's `BrowserWindowStdMac::SetBounds()` is literally a no-op:**

```cpp
void BrowserWindowStdMac::SetBounds(int x, int y, size_t width, size_t height) {
    REQUIRE_MAIN_THREAD();
    // Nothing to do here. Cocoa will size the browser for us.
}
```

This confirms that Cocoa's autoresizing system handles everything.

4. **`NotifyMoveOrResizeStarted()` is documented as "only used on Windows and Linux"** and does not appear in the macOS platform delegate at all. It is not needed on macOS.

#### Known issues on macOS

- **Layout constraints conflict**: A known CEF forum issue reports that when the parent uses Auto Layout (not autoresizing masks), the CEF view can shift or clip. The workaround is to use a wrapper view that converts Auto Layout constraints to explicit frame values. Since tao uses standard NSView frame-based layout (not Auto Layout), this should not affect wrymium.

- **Non-integral frame values**: If the parent view ends up with fractional frame dimensions (common with HiDPI scaling), CEF's internal scroll view can shift. Workaround: ensure the CEF view's frame is set to integral values. With autoresizing masks this is usually handled correctly by Cocoa.

#### macOS conclusion

**No resize code needed.** The browser view auto-resizes with the parent. wrymium's `set_bounds()` on macOS can either be a no-op (like cefclient) or set the frame explicitly as a safety measure.

---

### Windows

#### What CEF creates internally

When `WindowInfo::set_as_child(hwnd)` is used, cef-rs sets:
- `style = WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP | WS_VISIBLE`
- `parent_window = hwnd`
- `bounds = rect`

CEF then creates a child HWND with these styles. The child HWND is a standard Win32 child window.

#### Does the child HWND auto-resize?

**No.** Win32 child windows do NOT automatically resize when the parent resizes. This is fundamental to Win32 -- there is no equivalent of NSView autoresizing masks in the base Win32 API. The parent must explicitly resize child windows.

#### The standard resize pattern

From `cefclient/browser/root_window_win.cc`:

```cpp
// In the parent window's WndProc:
case WM_SIZE:
    self->OnSize(wParam == SIZE_MINIMIZED);
    break;

// OnSize implementation:
void RootWindowWin::OnSize(bool minimized) {
    if (minimized) {
        if (browser_window_) browser_window_->Hide();
        return;
    }
    if (browser_window_) browser_window_->Show();

    RECT rect;
    GetClientRect(hwnd_, &rect);

    // Size the browser window to the whole client area
    browser_window_->SetBounds(0, 0, rect.right, rect.bottom);
}
```

And `BrowserWindowStdWin::SetBounds()`:

```cpp
void BrowserWindowStdWin::SetBounds(int x, int y, size_t width, size_t height) {
    REQUIRE_MAIN_THREAD();
    HWND hwnd = GetWindowHandle();
    if (hwnd) {
        SetWindowPos(hwnd, nullptr, x, y,
                     static_cast<int>(width),
                     static_cast<int>(height),
                     SWP_NOZORDER);
    }
}
```

#### How to get the browser HWND

`CefBrowserHost::window_handle()` returns the `HWND` of the browser's top-level window. In cef-rs this is exposed as `browser_host.window_handle()` returning a `cef_window_handle_t` (which is `HWND` on Windows).

#### `NotifyMoveOrResizeStarted()` on Windows

The CEF API docs state: "Notify the browser that the window hosting it is about to be moved or resized. This method is only used on Windows and Linux."

In cefclient's GTK example, it is called from the `configure-event` signal handler. On Windows, the cefclient examples do NOT explicitly call it in the windowed (non-OSR) case -- the native Win32 message pump handles move/resize notifications internally.

However, calling it is harmless and can help CEF correctly position popups (dropdown menus, context menus) during resize. The recommendation is to call it before resizing in `set_bounds()`.

#### Windows conclusion

**Manual resize required.** wrymium must:
1. Get the browser HWND via `browser_host.window_handle()`
2. When `set_bounds()` is called, call `SetWindowPos(hwnd, ...)` with the new dimensions
3. Optionally call `browser_host.notify_move_or_resize_started()` before the resize

The resize is triggered by `tauri-runtime-wry`, which calls `WebView::set_bounds()` in response to tao's window resize events.

---

### Linux

#### CEF's embedding model on Linux

CEF on Linux uses **X11** for windowed rendering, not GTK widgets directly. When `WindowInfo::set_as_child(x11_window)` is used:

1. cef-rs sets `parent_window` to the provided X11 window ID
2. CEF creates a `CefWindowX11` object that calls `XCreateWindow` with the parent set to `parent_window`
3. The CEF browser content is rendered into this X11 child window

The parent window handle is obtained from the GTK widget via `GDK_WINDOW_XID(gtk_widget_get_window(widget))` -- tao provides this through `HasWindowHandle`.

#### Does the child X11 window auto-resize?

**No.** X11 does not have an auto-resize mechanism for child windows. The parent must explicitly resize the child.

#### How CEF handles resize internally

From `window_x11.cc`, when CEF's own X11 window receives a `ConfigureNotify` event (i.e., it was resized externally), it propagates the resize to the Chromium content child window:

```cpp
// CEF internal: ConfigureNotify handler in CefWindowX11
if (auto* configure = event.As<x11::ConfigureNotifyEvent>()) {
    bounds_ = gfx::Rect(configure->x, configure->y,
                        configure->width, configure->height);
    if (browser_.get()) {
        auto child = FindChild(connection_, xwindow_);
        if (child != x11::Window::None) {
            x11::ConfigureWindowRequest req{
                .window = child,
                .width = bounds_.width(),
                .height = bounds_.height(),
            };
            connection_->ConfigureWindow(req);
            browser_->NotifyMoveOrResizeStarted();
        }
    }
}
```

This means: if we resize CEF's X11 window, CEF will internally propagate the resize to the Chromium content window and call `NotifyMoveOrResizeStarted()` automatically.

#### How to resize CEF's X11 window

From `BrowserWindowStdGtk::SetBounds()`:

```cpp
void BrowserWindowStdGtk::SetBounds(int x, int y, size_t width, size_t height) {
    REQUIRE_MAIN_THREAD();
    if (xdisplay_ && browser_) {
        ::Window xwindow = browser_->GetHost()->GetWindowHandle();
        SetXWindowBounds(xdisplay_, xwindow, x, y, width, height);
    }
}

void SetXWindowBounds(XDisplay* xdisplay, ::Window xwindow,
                      int x, int y, size_t width, size_t height) {
    XWindowChanges changes = {0};
    changes.x = x;
    changes.y = y;
    changes.width = static_cast<int>(width);
    changes.height = static_cast<int>(height);
    XConfigureWindow(xdisplay, xwindow, CWX | CWY | CWHeight | CWWidth, &changes);
}
```

#### GTK integration

CEF does NOT create a GTK widget for the browser. It creates a raw X11 window that is a child of the GTK window's X11 window. cef-rs does not provide any GTK widget integration.

The RootWindowGtk example in cefclient uses GTK for the shell (window chrome, toolbar, menu bar) and embeds the CEF X11 window inside the GTK layout by getting the GTK container's X11 window ID. Resize events flow: GTK `size-allocate` signal -> calculate browser area -> `XConfigureWindow` on CEF's X11 window.

#### Wayland note

CEF's windowed mode on Linux currently requires X11. Wayland support for embedded windows is tracked in CEF issue #2804 and is not yet available. tao with the `x11` feature provides X11 window handles, which is what wrymium should use.

#### Linux conclusion

**Manual resize required.** wrymium must:
1. Get the browser X11 window via `browser_host.window_handle()`
2. Get the X11 display connection (from tao's display handle or via `XOpenDisplay`)
3. When `set_bounds()` is called, call `XConfigureWindow` with the new dimensions
4. CEF's internal `ConfigureNotify` handler will propagate the resize to the content window and call `NotifyMoveOrResizeStarted()` automatically -- no need to call it explicitly

---

## Cross-Platform Questions

### `CefBrowserHost::WasResized()` -- windowed vs OSR

**`WasResized()` is for OSR (off-screen rendering) ONLY.** From the CEF documentation:

> "Notify the browser that the widget has been resized. The browser will first call CefRenderHandler::GetViewRect to get the new size and then call CefRenderHandler::OnPaint asynchronously with the updated regions. This method is only used when window rendering is disabled."

wrymium uses windowed rendering (`set_as_child`), not OSR. Therefore `WasResized()` is **not needed and should not be called**.

### `NotifyMoveOrResizeStarted()` -- when is it needed?

From CEF docs: "Notify the browser that the window hosting it is about to be moved or resized. This method is only used on Windows and Linux."

In practice:
- **macOS**: Not needed, not implemented in CEF's macOS delegate
- **Windows**: Optional but recommended. Helps CEF correctly position popups during resize
- **Linux**: Called automatically by CEF's `CefWindowX11::ProcessXEvent` when it receives `ConfigureNotify`. wrymium does not need to call it explicitly if it resizes via `XConfigureWindow` (which triggers `ConfigureNotify`)

### `SetAutoResizeEnabled()` -- is it relevant?

**No.** This API allows the *browser content* to notify the host when the *content* wants a different size (e.g., a popup that sizes to fit its content). It is the inverse of what we need. We need the host to tell the browser its new size, not the other way around.

### Windowed mode vs OSR resize handling

| Aspect | Windowed mode | OSR mode |
|--------|--------------|----------|
| Who owns the window? | CEF creates a native window | App creates a texture/surface |
| How to resize | Resize the native window (SetWindowPos / XConfigureWindow) | Call `WasResized()`, implement `GetViewRect` |
| Who repaints? | CEF/Chromium compositor (automatic) | App calls `OnPaint` and blits to texture |
| Flickering | Managed by Chromium's compositor | Managed by app |
| `NotifyMoveOrResizeStarted` | Helps popup positioning (Win/Linux) | Helps popup positioning (Win/Linux) |

### Resize performance and known issues

**Flickering/black frames during resize is a known long-standing issue in Chromium/CEF.** The root cause is architectural: during resize, the compositor needs to produce a new frame for the new size, which takes time. During that gap, stale pixels from the previous frame or an unpainted background can become visible.

Key findings from Electron's technical analysis (December 2025):

1. **Windows-specific DirectComposition bug**: When using ANGLE D3D11 backend, `IDXGISwapChain1::Present1` and `IDCompositionDevice::Commit` execute asynchronously on the GPU, potentially out of order. This caused stale pixels to appear outside the viewport during resize. Fixed in Chromium by painting areas outside the viewport transparent.

2. **Frame size mismatch during resize**: The window dimensions and compositor frame dimensions become desynchronized during active resize. Chromium now adjusts the viewport/clip rect to match the actual frame size rather than stretching.

3. **These fixes are in Chromium 133+ / Electron 39.2.6+.** CEF versions based on Chromium 133+ (CEF v133+) should include them. The cef-rs crate currently tracks CEF v146, which includes these fixes.

4. **macOS is generally better** -- the macOS compositor (Core Animation / Metal) handles resize more smoothly than Windows DirectComposition.

5. **Linux**: Some reports of flickering with X11 compositing window managers. Using `--disable-gpu-compositing` can help but degrades rendering performance.

Practical impact: resize flickering in wrymium will be similar to what Electron and Chrome experience. There is no silver bullet; this is a Chromium-level issue.

---

## wrymium Design Recommendations

### 1. Concrete resize handling strategy per platform

#### macOS (v0.1)

```rust
// WebView::set_bounds() on macOS
pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
    // Option A: No-op (recommended, matching cefclient)
    // Cocoa autoresizing handles everything.
    // The parent tao window resizes -> the content view resizes
    // -> CEF's browser view auto-resizes via autoresizing masks.

    // Option B: Explicit frame set (safety net)
    // Only needed if we encounter edge cases where autoresizing fails
    // (e.g., Auto Layout interference, non-integral frames)
    //
    // unsafe {
    //     let ns_view: *mut NSView = self.browser_host.window_handle() as _;
    //     let frame = NSRect::new(
    //         NSPoint::new(bounds.position.x, bounds.position.y),
    //         NSSize::new(bounds.size.width, bounds.size.height),
    //     );
    //     msg_send![ns_view, setFrame: frame];
    // }

    Ok(())
}
```

#### Windows (v0.3)

```rust
// WebView::set_bounds() on Windows
pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
    let hwnd = self.browser_host.window_handle();
    if hwnd.is_null() {
        return Err(Error::CefError("Browser HWND not available".into()));
    }

    // Notify CEF that resize is starting (helps popup positioning)
    self.browser_host.notify_move_or_resize_started();

    // Convert logical to physical pixels if needed
    let (x, y, w, h) = bounds.to_physical(self.scale_factor);

    unsafe {
        SetWindowPos(
            hwnd as HWND,
            std::ptr::null_mut(),  // no z-order change
            x, y, w, h,
            SWP_NOZORDER,
        );
    }
    Ok(())
}
```

#### Linux (v0.4)

```rust
// WebView::set_bounds() on Linux
pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
    let xwindow = self.browser_host.window_handle();
    if xwindow == 0 {
        return Err(Error::CefError("Browser X11 window not available".into()));
    }

    // Convert logical to physical pixels if needed
    let (x, y, w, h) = bounds.to_physical(self.scale_factor);

    unsafe {
        let mut changes = XWindowChanges {
            x, y,
            width: w,
            height: h,
            ..std::mem::zeroed()
        };
        XConfigureWindow(
            self.xdisplay,
            xwindow,
            (CWX | CWY | CWWidth | CWHeight) as u32,
            &mut changes,
        );
    }
    // NotifyMoveOrResizeStarted() is called automatically by CEF's
    // CefWindowX11 when it processes the resulting ConfigureNotify event.
    Ok(())
}
```

### 2. Where the resize hook lives

The resize hook lives in **`WebView::set_bounds()`**. This is the correct place because:

- `tauri-runtime-wry` calls `WebView::set_bounds()` when the tao window resizes
- wry's existing API contract is that `set_bounds()` resizes the webview
- No separate event listener or auto mechanism is needed on Windows/Linux
- On macOS, `set_bounds()` can be a no-op because autoresizing handles it

The call chain is:
```
tao window resize event
  -> tauri-runtime-wry handles resize
    -> calls webview.set_bounds(new_rect)
      -> wrymium::WebView::set_bounds()
        -> platform-specific resize code
```

For the **initial size** at creation time:
- `WebViewBuilder::with_bounds(rect)` stores the initial bounds
- `WebViewBuilder::build()` passes these bounds to `WindowInfo::set_as_child()` via the `rect` parameter
- CEF creates the browser view at this initial size

### 3. `NotifyMoveOrResizeStarted()` and `WasResized()` in windowed mode

| Method | Needed? | When to call |
|--------|---------|-------------|
| `NotifyMoveOrResizeStarted()` | **macOS: No** | Not applicable |
| `NotifyMoveOrResizeStarted()` | **Windows: Yes (recommended)** | Before `SetWindowPos` in `set_bounds()` |
| `NotifyMoveOrResizeStarted()` | **Linux: No (automatic)** | CEF calls it internally on `ConfigureNotify` |
| `WasResized()` | **No (all platforms)** | Only for OSR mode, not windowed mode |

### 4. Gotchas and known issues

#### macOS gotchas

1. **Auto Layout interference**: If the parent tao window uses Auto Layout internally, the autoresizing masks on CEF's browser view may not work correctly. tao uses frame-based layout, so this should not be an issue. If it ever becomes one, use a wrapper NSView.

2. **Non-integral frame values**: HiDPI displays can produce fractional frame coordinates. CEF's internal scroll view may shift if the frame is non-integral. Monitor for this and round to integral values if needed.

3. **Initial bounds must be correct**: The `rect` parameter in `set_as_child()` sets the initial frame. If it is (0,0,0,0), the browser view starts with zero size and relies entirely on autoresizing to fill the parent. Passing the parent's bounds is safer.

#### Windows gotchas

4. **Thread safety**: `SetWindowPos` must be called from the thread that created the window (the UI thread). wrymium's `set_bounds()` is called from `tauri-runtime-wry` on the main thread, which is correct.

5. **DPI scaling**: The bounds from `tauri-runtime-wry` may be in logical pixels. Convert to physical pixels using the window's scale factor before calling `SetWindowPos`. CEF's cefclient uses `GetWindowBoundsAndContinue` for DIP-to-pixel conversion.

6. **`multi_threaded_message_loop` mode**: On Windows, wrymium uses `multi_threaded_message_loop = true`, which means CEF runs its message loop on a separate thread. `SetWindowPos` is still safe because it operates on the HWND directly via Win32 API. However, `notify_move_or_resize_started()` should be called from the UI thread.

7. **Resize flickering**: The Chromium compositor may show stale frames during resize. This is a known issue. CEF v146 (based on Chromium 146) includes the fixes from the Electron team for the DirectComposition desynchronization bug.

#### Linux gotchas

8. **X11 display connection**: wrymium needs access to the X11 `Display*` pointer to call `XConfigureWindow`. This can be obtained from tao's `DisplayHandle` or by caching it during initialization.

9. **Wayland**: CEF's windowed mode does not support Wayland embedding yet (CEF issue #2804). wrymium must use X11 on Linux. Ensure the `x11` feature flag is enabled.

10. **GTK realization**: The GTK widget must be "realized" (mapped to an X11 window) before CEF can embed into it. tao handles this, but if `build()` is called before the window is shown, the X11 window ID may not be available yet. Verify that `HasWindowHandle` returns a valid handle.

11. **Compositor interactions**: Some Linux compositing window managers (KDE's KWin, GNOME's Mutter) may introduce their own resize artifacts. `--disable-gpu-compositing` is a fallback but degrades performance.

#### Cross-platform gotchas

12. **Scale factor changes**: When a window moves between monitors with different DPI, the scale factor changes. wrymium should handle `ScaleFactorChanged` events from tao and update the browser view bounds accordingly. On macOS this is automatic (Cocoa handles it). On Windows/Linux, a `set_bounds()` call with the new physical size is needed.

13. **`CefBrowserHost::window_handle()` timing**: This returns the browser's window handle, but it may not be available immediately after `CefBrowserHost::CreateBrowser()`. The handle becomes available after `CefLifeSpanHandler::OnAfterCreated()` fires. wrymium should cache the handle at that point.

14. **Zero-size windows**: Creating a browser with zero width or height can cause rendering issues. CEF's Linux code defaults to 800x600 if bounds are zero. Ensure non-zero bounds are always passed.

---

## How Electron handles this

Electron uses CEF (via libchromiumcontent) but with significant modifications:

1. **Electron does NOT use `set_as_child` windowed mode** for its main BrowserWindow. Electron creates its own native windows and uses Chromium's content API directly (not the CEF embedding API).

2. **For `<webview>` tags and BrowserView** (now deprecated), Electron uses an architecture more similar to CEF's OSR mode, where the compositor output is composited into the parent window's render tree.

3. **Electron's resize handling** is therefore fundamentally different from wrymium's approach. The Electron team's recent work (December 2025) on DirectComposition resize fixes was in Chromium's `viz` compositor, which benefits all Chromium embedders including CEF.

4. The key takeaway from Electron is not the resize mechanism itself, but the **knowledge that resize flickering is a Chromium-level problem** that is being actively worked on at the compositor layer.

---

## Summary of findings

| Question | Answer |
|----------|--------|
| Does CEF browser view auto-resize on macOS? | **Yes** -- autoresizing masks are set internally by CEF |
| Does CEF browser HWND auto-resize on Windows? | **No** -- manual `SetWindowPos` required |
| Does CEF browser X11 window auto-resize on Linux? | **No** -- manual `XConfigureWindow` required |
| Is `WasResized()` needed in windowed mode? | **No** -- only for OSR |
| Is `NotifyMoveOrResizeStarted()` needed? | **macOS: No. Windows: Recommended. Linux: Automatic** |
| Where should resize code live? | `WebView::set_bounds()` |
| Will there be resize flickering? | Yes, same as Chrome/Electron. Chromium-level issue, improved in v133+ |
| Does cef-rs expose the needed APIs? | Yes: `BrowserHost::window_handle()`, `notify_move_or_resize_started()`, `was_resized()` all available |

**Status**: RESOLVED -- strategy confirmed for all three platforms.

---

## Sources

- [CEF browser_platform_delegate_native_mac.mm](https://github.com/chromiumembedded/cef/blob/master/libcef/browser/native/browser_platform_delegate_native_mac.mm) -- macOS autoresizing mask code
- [CEF browser_window_std_mac.mm](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/browser_window_std_mac.mm) -- `SetBounds()` no-op on macOS
- [CEF browser_window_std_win.cc](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/browser_window_std_win.cc) -- `SetWindowPos` on Windows
- [CEF browser_window_std_gtk.cc](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/browser_window_std_gtk.cc) -- `XConfigureWindow` on Linux
- [CEF root_window_win.cc](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/root_window_win.cc) -- `WM_SIZE` handling
- [CEF root_window_gtk.cc](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/root_window_gtk.cc) -- GTK `configure-event` / `size-allocate` handling
- [CEF window_x11.cc](https://github.com/chromiumembedded/cef/blob/master/libcef/browser/native/window_x11.cc) -- `ConfigureNotify` propagation
- [CEF cef_browser.h](https://github.com/chromiumembedded/cef/blob/master/include/cef_browser.h) -- `NotifyMoveOrResizeStarted()` and `WasResized()` docs
- [CEF cef_types_mac.h](https://github.com/chromiumembedded/cef/blob/master/include/internal/cef_types_mac.h) -- macOS `cef_window_info_t` struct
- [cef-rs WindowInfo source](https://docs.rs/cef/latest/src/cef/window_info.rs.html) -- `set_as_child()` implementation
- [cef-rs BrowserHost docs](https://docs.rs/cef/latest/cef/struct.BrowserHost.html) -- `window_handle()`, `was_resized()`, `notify_move_or_resize_started()`
- [Electron: Improving Window Resize Behavior](https://www.electronjs.org/blog/tech-talk-window-resize-behavior) -- DirectComposition resize fix analysis
- [CEF Forum: Browser don't resize properly if container NSView resize](https://magpcss.org/ceforum/viewtopic.php?f=6&t=16341)
- [CEF Forum: CEF view lags on parent window resize](https://www.magpcss.org/ceforum/viewtopic.php?f=10&t=19718)
- [CEF Forum: Resizing Window isn't resizing the browser](https://magpcss.org/ceforum/viewtopic.php?f=6&t=10852)
- [Chromium bug: Resizing browser flickers black](https://bugs.chromium.org/p/chromium/issues/detail?id=326995)
- [CEF issue #2804: Linux Wayland support](https://github.com/chromiumembedded/cef/issues/2804)
