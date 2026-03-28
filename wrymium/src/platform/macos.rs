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
    fn with_traffic_light_inset(self, position: (f64, f64)) -> Self;
}

impl<'a> WebViewBuilderExtMacos for WebViewBuilder<'a> {
    fn with_webview_configuration(self, _config: *mut c_void) -> Self {
        // No-op — CEF does not use WKWebViewConfiguration
        self
    }

    fn with_traffic_light_inset(self, _position: (f64, f64)) -> Self {
        // TODO: Could be mapped to CefWindowInfo positioning
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
        // TODO: Return the CEF browser's NSView pointer
        // browser_host.get_window_handle() on macOS returns the NSView
        std::ptr::null_mut()
    }

    fn manager(&self) -> *mut c_void {
        // CEF has no WKUserContentController equivalent
        std::ptr::null_mut()
    }

    fn ns_window(&self) -> *mut c_void {
        // TODO: Get the parent NSWindow from the browser's view hierarchy
        std::ptr::null_mut()
    }

    fn reparent(&self, _window: *mut c_void) -> Result<()> {
        // TODO: Remove CEF browser view from current parent,
        // add to new NSWindow's contentView
        Ok(())
    }
}

// --- WebViewExtDarwin ---

pub trait WebViewExtDarwin {}

impl WebViewExtDarwin for WebView {}
