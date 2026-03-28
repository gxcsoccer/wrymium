//! Linux platform extension traits (stubs for v0.1).

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
        // TODO: X11 reparent or GTK container manipulation
        Ok(())
    }
}
