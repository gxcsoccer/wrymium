//! Simple logging macro for wrymium debug output.
//!
//! Only prints in debug builds to avoid log noise in production.

#[cfg(debug_assertions)]
macro_rules! wrymium_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}

#[cfg(not(debug_assertions))]
macro_rules! wrymium_log {
    ($($arg:tt)*) => {};
}
