//! Linux platform extension traits.
//!
//! Reparenting requires X11 `XReparentWindow` or GTK `gtk_widget_reparent` —
//! not implemented yet since wrymium is currently macOS-focused.

use crate::error::Result;
use crate::webview::{WebView, WebViewBuilder};

pub trait WebViewBuilderExtUnix {
    fn with_extensions_path(self, path: &std::path::Path) -> Self;
}

impl<'a> WebViewBuilderExtUnix for WebViewBuilder<'a> {
    fn with_extensions_path(self, path: &std::path::Path) -> Self {
        WebViewBuilder::with_extensions_path(self, path)
    }
}

pub trait WebViewExtUnix {
    fn reparent(&self) -> Result<()>;
}

impl WebViewExtUnix for WebView {
    fn reparent(&self) -> Result<()> {
        // Not implemented — requires X11/Wayland backend.
        // CEF's window_handle() returns the X11 Window ID which could be
        // reparented via XReparentWindow, but this path is untested.
        Ok(())
    }
}
