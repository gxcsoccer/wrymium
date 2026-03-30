//! wrymium — CEF-powered WebView backend, wry-compatible API for Tauri.
//!
//! This crate provides a drop-in replacement for `wry` that uses the Chromium
//! Embedded Framework (CEF) instead of platform-native WebView engines.

#[macro_use]
mod log;
pub mod browser_use;
pub mod cdp;
mod cef_init;
mod context;
mod error;
pub mod platform;
pub(crate) mod renderer;
mod scheme;
#[cfg(test)]
mod tests;
mod types;
mod webview;

// --- Public re-exports (mirrors wry 0.55) ---

pub use context::WebContext;
pub use error::{Error, Result};
pub use types::*;
pub use webview::{RequestAsyncResponder, WebView, WebViewBuilder};

// Re-export crates that wry re-exports (used by tauri-runtime-wry)
pub use dpi;
pub use http;
pub use raw_window_handle;

// Platform extension traits
#[cfg(target_os = "macos")]
pub use platform::macos::{
    WebViewBuilderExtDarwin, WebViewBuilderExtMacos, WebViewExtDarwin, WebViewExtMacOS,
};

#[cfg(target_os = "windows")]
pub use platform::windows::{WebViewBuilderExtWindows, WebViewExtWindows};

#[cfg(target_os = "linux")]
pub use platform::linux::{WebViewBuilderExtUnix, WebViewExtUnix};

// --- Public API ---

/// Returns the CEF version string.
/// Compatible with `wry::webview_version()`.
pub fn webview_version() -> Result<String> {
    // CEF_VERSION from cef-dll-sys: e.g. "146.0.6+g68649e2+chromium-146.0.7680.154\0"
    let bytes = cef::sys::CEF_VERSION;
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let version = std::str::from_utf8(&bytes[..len]).unwrap_or("unknown");
    Ok(format!("{version} (wrymium)"))
}

/// Check if the current process is a CEF subprocess.
/// Must be called at the very beginning of `main()`.
pub fn is_cef_subprocess() -> bool {
    cef_init::is_cef_subprocess()
}

/// Run the CEF subprocess entry point. Returns the exit code.
pub fn run_cef_subprocess() -> i32 {
    cef_init::run_cef_subprocess()
}

/// Shutdown CEF. Should be called at application exit.
pub fn shutdown() {
    cef_init::shutdown()
}
