use std::fmt;

/// Error type compatible with wry::Error.
///
/// Marked `#[non_exhaustive]` to match wry's convention.
/// tauri-runtime-wry only constructs `MessageSender` and never pattern-matches.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    MessageSender,
    CefError(String),
    CefNotInitialized,
    Io(std::io::Error),
    HttpError(http::Error),
    WindowHandleError(raw_window_handle::HandleError),
    UnsupportedWindowHandle,
    NulError(std::ffi::NulError),
    DuplicateCustomProtocol(String),
    UrlSchemeRegisterError(String),
    NotMainThread,
    InitScriptError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MessageSender => write!(f, "failed to send message"),
            Error::CefError(msg) => write!(f, "CEF error: {msg}"),
            Error::CefNotInitialized => write!(f, "CEF not initialized"),
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::HttpError(e) => write!(f, "HTTP error: {e}"),
            Error::WindowHandleError(e) => write!(f, "window handle error: {e}"),
            Error::UnsupportedWindowHandle => write!(f, "unsupported window handle"),
            Error::NulError(e) => write!(f, "nul error: {e}"),
            Error::DuplicateCustomProtocol(name) => {
                write!(f, "duplicate custom protocol: {name}")
            }
            Error::UrlSchemeRegisterError(msg) => {
                write!(f, "URL scheme register error: {msg}")
            }
            Error::NotMainThread => write!(f, "must be called from the main thread"),
            Error::InitScriptError => write!(f, "failed to add initialization script"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::HttpError(e)
    }
}

impl From<std::ffi::NulError> for Error {
    fn from(e: std::ffi::NulError) -> Self {
        Error::NulError(e)
    }
}

impl From<raw_window_handle::HandleError> for Error {
    fn from(e: raw_window_handle::HandleError) -> Self {
        Error::WindowHandleError(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
