//! Windows platform extension traits (stubs for v0.1).

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
        // TODO: Use SetParent Win32 API on CEF's HWND
        Ok(())
    }
}
