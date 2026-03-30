//! Windows platform extension traits.
//!
//! Reparenting requires Win32 `SetParent` API on the CEF HWND —
//! not implemented yet since wrymium is currently macOS-focused.

use crate::error::Result;
use crate::types::{ScrollBarStyle, Theme};
use crate::webview::{WebView, WebViewBuilder};

pub trait WebViewBuilderExtWindows {
    fn with_additional_browser_args(self, args: &str) -> Self;
    fn with_theme(self, theme: Theme) -> Self;
    fn with_scroll_bar_style(self, style: ScrollBarStyle) -> Self;
    fn with_browser_extensions_enabled(self, enabled: bool) -> Self;
}

impl<'a> WebViewBuilderExtWindows for WebViewBuilder<'a> {
    fn with_additional_browser_args(self, args: &str) -> Self {
        WebViewBuilder::with_additional_browser_args(self, args)
    }
    fn with_theme(self, theme: Theme) -> Self {
        WebViewBuilder::with_theme(self, theme)
    }
    fn with_scroll_bar_style(self, style: ScrollBarStyle) -> Self {
        WebViewBuilder::with_scroll_bar_style(self, style)
    }
    fn with_browser_extensions_enabled(self, enabled: bool) -> Self {
        WebViewBuilder::with_browser_extensions_enabled(self, enabled)
    }
}

pub trait WebViewExtWindows {
    fn reparent(&self, hwnd: isize) -> Result<()>;
}

impl WebViewExtWindows for WebView {
    fn reparent(&self, _hwnd: isize) -> Result<()> {
        // Not implemented — requires Win32 SetParent(cef_hwnd, new_parent_hwnd).
        // CEF's window_handle() returns the HWND which could be reparented,
        // but this path is untested.
        Ok(())
    }
}
