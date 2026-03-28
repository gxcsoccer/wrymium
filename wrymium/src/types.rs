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
}

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

/// Cookie type for cookie management.
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
}

/// Features of a new window request.
#[derive(Debug, Clone, Default)]
pub struct NewWindowFeatures {
    pub size: Option<(f64, f64)>,
    pub position: Option<(f64, f64)>,
}

/// The ID type for webviews.
pub type WebViewId = String;
