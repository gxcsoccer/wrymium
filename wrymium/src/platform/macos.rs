//! macOS platform extension traits.
//!
//! These mirror wry's WebViewExtMacOS, WebViewExtDarwin, and builder extension traits.
//! Methods that return native WebKit types (WKWebView, etc.) are not applicable to CEF
//! and will panic if called. The patched tauri-runtime-wry avoids calling these.

use std::ffi::c_void;

use crate::error::Result;
use crate::webview::{WebView, WebViewBuilder};

// --- WebViewBuilderExtMacos ---

pub trait WebViewBuilderExtMacos {
    /// Accept a webview configuration. In CEF this is a no-op since there is no
    /// WKWebViewConfiguration, but the method must exist for API compatibility.
    fn with_webview_configuration(self, config: *mut c_void) -> Self;

    /// Set the traffic light inset position.
    /// No-op in CEF — traffic light positioning is managed by Tauri at the
    /// NSWindow level, not by the embedded browser view.
    fn with_traffic_light_inset(self, position: dpi::Position) -> Self;
}

impl<'a> WebViewBuilderExtMacos for WebViewBuilder<'a> {
    fn with_webview_configuration(self, _config: *mut c_void) -> Self {
        // No-op — CEF does not use WKWebViewConfiguration
        self
    }

    fn with_traffic_light_inset(self, _position: dpi::Position) -> Self {
        // No-op — traffic light (close/minimize/zoom) buttons are positioned
        // by Tauri's NSWindow configuration, not by the embedded browser view.
        self
    }
}

// --- WebViewBuilderExtDarwin ---

pub trait WebViewBuilderExtDarwin {
    fn with_data_store_identifier(self, id: [u8; 16]) -> Self;
    fn with_allow_link_preview(self, enabled: bool) -> Self;
}

impl<'a> WebViewBuilderExtDarwin for WebViewBuilder<'a> {
    fn with_data_store_identifier(self, id: [u8; 16]) -> Self {
        // Delegate to the builder's built-in method
        WebViewBuilder::with_data_store_identifier(self, id)
    }

    fn with_allow_link_preview(self, enabled: bool) -> Self {
        WebViewBuilder::with_allow_link_preview(self, enabled)
    }
}

// --- WebViewExtMacOS ---

pub trait WebViewExtMacOS {
    /// Returns a pointer to the underlying view.
    /// In CEF, this returns the CEF browser's NSView (not WKWebView).
    /// Code expecting a WKWebView will not work correctly.
    fn webview(&self) -> *mut c_void;

    /// Returns a pointer to the content controller.
    /// Not applicable to CEF — returns null.
    fn manager(&self) -> *mut c_void;

    /// Returns a pointer to the parent NSWindow.
    fn ns_window(&self) -> *mut c_void;

    /// Reparent the webview to a new window.
    fn reparent(&self, window: *mut c_void) -> Result<()>;
}

impl WebViewExtMacOS for WebView {
    fn webview(&self) -> *mut c_void {
        // On macOS, browser_host.window_handle() returns the CEF browser's NSView.
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(host) = cef::ImplBrowser::host(browser) {
                let handle = cef::ImplBrowserHost::window_handle(&host);
                if !handle.is_null() {
                    return handle;
                }
            }
        }
        std::ptr::null_mut()
    }

    fn manager(&self) -> *mut c_void {
        // CEF has no WKUserContentController equivalent
        std::ptr::null_mut()
    }

    fn ns_window(&self) -> *mut c_void {
        // Get the NSView via window_handle(), then walk up to the parent NSWindow.
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(host) = cef::ImplBrowser::host(browser) {
                let handle = cef::ImplBrowserHost::window_handle(&host);
                if !handle.is_null() {
                    use objc2_app_kit::NSView;
                    let view = unsafe { &*(handle as *const NSView) };
                    if let Some(window) = view.window() {
                        return objc2::rc::Retained::as_ptr(&window) as *mut c_void;
                    }
                }
            }
        }
        std::ptr::null_mut()
    }

    fn reparent(&self, window: *mut c_void) -> Result<()> {
        if window.is_null() {
            return Ok(());
        }
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(host) = cef::ImplBrowser::host(browser) {
                let handle = cef::ImplBrowserHost::window_handle(&host);
                if !handle.is_null() {
                    use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindow};
                    let view = unsafe { &*(handle as *const NSView) };
                    let new_window = unsafe { &*(window as *const NSWindow) };
                    // Remove from current parent
                    view.removeFromSuperview();
                    // Add to new window's content view and fill it
                    if let Some(content_view) = new_window.contentView() {
                        view.setFrame(content_view.bounds());
                        view.setAutoresizingMask(
                            NSAutoresizingMaskOptions::ViewWidthSizable
                                | NSAutoresizingMaskOptions::ViewHeightSizable,
                        );
                        content_view.addSubview(view);
                    }
                }
            }
        }
        Ok(())
    }
}

// --- WebViewExtDarwin ---

pub trait WebViewExtDarwin {}

impl WebViewExtDarwin for WebView {}
