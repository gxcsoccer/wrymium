use std::path::PathBuf;

/// Rectangle with position and size, using `dpi` crate types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub position: dpi::Position,
    pub size: dpi::Size,
}

impl Default for Rect {
    fn default() -> Self {
        Self {
            position: dpi::Position::Logical(dpi::LogicalPosition::new(0.0, 0.0)),
            size: dpi::Size::Logical(dpi::LogicalSize::new(800.0, 600.0)),
        }
    }
}

/// Drag-and-drop events.
#[derive(Debug, Clone)]
pub enum DragDropEvent {
    Enter {
        paths: Vec<PathBuf>,
        position: (f64, f64),
    },
    Over {
        position: (f64, f64),
    },
    Drop {
        paths: Vec<PathBuf>,
        position: (f64, f64),
    },
    Leave,
}

/// Proxy configuration.
#[derive(Debug, Clone)]
pub enum ProxyConfig {
    Http(ProxyEndpoint),
    Socks5(ProxyEndpoint),
}

/// Proxy endpoint.
#[derive(Debug, Clone)]
pub struct ProxyEndpoint {
    pub host: String,
    pub port: String,
}

/// Background throttling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundThrottlingPolicy {
    Disabled,
    Suspend,
    Throttle,
}

impl Default for BackgroundThrottlingPolicy {
    fn default() -> Self {
        Self::Suspend
    }
}

/// Page load events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLoadEvent {
    Started,
    Finished,
}

/// Response to a new window request.
#[derive(Debug)]
pub enum NewWindowResponse {
    Allow,
    Deny,
    /// Create a new window with the given webview handle.
    Create {
        webview: *mut std::ffi::c_void,
    },
}

// SAFETY: webview pointer only used on main thread
unsafe impl Send for NewWindowResponse {}
unsafe impl Sync for NewWindowResponse {}

/// Theme preference (Windows only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

/// Scrollbar style (Windows only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBarStyle {
    Default,
    FluentOverlay,
}

/// Re-export the cookie crate's Cookie type for wry compatibility.
pub use cookie::Cookie;

/// Opener info for new window requests (stub for CEF).
#[derive(Debug, Clone, Default)]
pub struct NewWindowOpener {
    pub webview: *mut std::ffi::c_void,
    #[cfg(target_os = "macos")]
    pub target_configuration: *mut std::ffi::c_void,
    #[cfg(target_os = "windows")]
    pub environment: *mut std::ffi::c_void,
}

// SAFETY: Opener pointers are only used on the main thread.
unsafe impl Send for NewWindowOpener {}
unsafe impl Sync for NewWindowOpener {}

/// Features of a new window request.
#[derive(Debug, Clone, Default)]
pub struct NewWindowFeatures {
    pub size: Option<(f64, f64)>,
    pub position: Option<(f64, f64)>,
    pub opener: NewWindowOpener,
}

/// The ID type for webviews.
pub type WebViewId = String;
