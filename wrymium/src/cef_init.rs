//! CEF initialization and message pump integration.
//!
//! CEF is initialized lazily on the first `WebViewBuilder::build()` call.
//! On macOS, a CFRunLoopTimer at 30fps drives `CefDoMessageLoopWork()`.
//! On Linux, glib's `g_timeout_add` serves the same purpose.
//! On Windows, `multi_threaded_message_loop = true` is used instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use cef::*;

use crate::error::{Error, Result};

static CEF_INIT: Once = Once::new();
static CEF_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
static CEF_LIBRARY: std::sync::OnceLock<cef::library_loader::LibraryLoader> =
    std::sync::OnceLock::new();

/// Check if the current process is a CEF subprocess (renderer, GPU, utility, etc.).
/// Must be called at the very beginning of `main()`.
///
/// Returns `true` if `--type=` argument is present (indicating a subprocess).
pub fn is_cef_subprocess() -> bool {
    std::env::args().any(|arg| arg.starts_with("--type="))
}

/// Run the CEF subprocess entry point. Returns the exit code.
/// Call `std::process::exit()` with the return value if `is_cef_subprocess()` is true.
pub fn run_cef_subprocess() -> i32 {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().unwrap();
        // helper = true: path resolves as exe/../../../CEF.framework
        let loader = cef::library_loader::LibraryLoader::new(&exe, true);
        if !loader.load() {
            wrymium_log!("[wrymium] Failed to load CEF library in subprocess");
            return 1;
        }
        let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

        // Pass WrymiumApp so renderer subprocesses get the RenderProcessHandler
        let mut app = WrymiumApp::new();
        let args = cef::args::Args::new();
        let ret = cef::execute_process(
            Some(args.as_main_args()),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        return ret;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut app = WrymiumApp::new();
        let args = cef::args::Args::new();
        let ret = cef::execute_process(
            Some(args.as_main_args()),
            Some(&mut app),
            std::ptr::null_mut(),
        );
        return ret;
    }
}

/// Ensure CEF is initialized. Called internally by `WebViewBuilder::build()`.
/// Safe to call multiple times — only the first call does anything.
pub(crate) fn ensure_initialized() -> Result<()> {
    let mut init_error: Option<String> = None;

    CEF_INIT.call_once(|| match initialize_cef() {
        Ok(()) => {
            CEF_INITIALIZED.store(true, Ordering::Release);
        }
        Err(e) => {
            init_error = Some(e.to_string());
        }
    });

    if let Some(err) = init_error {
        return Err(Error::CefError(err));
    }

    if !CEF_INITIALIZED.load(Ordering::Acquire) {
        return Err(Error::CefNotInitialized);
    }

    Ok(())
}

/// Returns true if CEF has been initialized.
pub(crate) fn is_initialized() -> bool {
    CEF_INITIALIZED.load(Ordering::Acquire)
}

fn initialize_cef() -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| Error::CefError(e.to_string()))?;

    // 1. Load CEF library (macOS requires dynamic loading)
    #[cfg(target_os = "macos")]
    {
        let loader = cef::library_loader::LibraryLoader::new(&exe, false);
        if !loader.load() {
            return Err(Error::CefError(
                "Failed to load CEF framework".to_string(),
            ));
        }
        let _ = CEF_LIBRARY.set(loader); // Store to prevent Drop
    }

    // 2. Validate CEF API version
    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    // 3. Create args and run execute_process for the browser process (returns -1)
    let args = cef::args::Args::new();
    let ret = cef::execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if ret >= 0 {
        return Err(Error::CefError(format!(
            "Unexpected subprocess execution (exit code {ret})"
        )));
    }

    // 4. Create CefApp
    let mut app = WrymiumApp::new();

    // 5. Configure settings
    let mut settings = Settings {
        no_sandbox: 1,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        external_message_pump: 1,
        #[cfg(target_os = "windows")]
        multi_threaded_message_loop: 1,
        ..Default::default()
    };

    // On macOS, set paths relative to the .app bundle and configure DYLD
    #[cfg(target_os = "macos")]
    {
        let macos_dir = exe.parent().ok_or_else(|| {
            Error::CefError("Cannot determine MacOS dir from executable path".into())
        })?;
        let contents_dir = macos_dir.parent().ok_or_else(|| {
            Error::CefError("Cannot determine Contents dir — is this a .app bundle?".into())
        })?;
        let frameworks_dir = contents_dir.join("Frameworks");

        let framework_path = frameworks_dir.join("Chromium Embedded Framework.framework");
        if framework_path.exists() {
            settings.framework_dir_path =
                CefString::from(framework_path.to_str().unwrap_or_default());
        }

        let app_dir = contents_dir.parent().ok_or_else(|| {
            Error::CefError("Cannot determine .app dir from bundle structure".into())
        })?;
        settings.main_bundle_path =
            CefString::from(app_dir.to_str().unwrap_or_default());

        let exe_name = exe
            .file_name()
            .unwrap()
            .to_str()
            .unwrap_or("wrymium");
        let helper_path = frameworks_dir
            .join(format!("{exe_name} Helper.app"))
            .join("Contents/MacOS")
            .join(format!("{exe_name} Helper"));
        if helper_path.exists() {
            settings.browser_subprocess_path =
                CefString::from(helper_path.to_str().unwrap_or_default());
        }

        // Set DYLD_FALLBACK_LIBRARY_PATH so child processes can find CEF dylibs
        let lib_path = framework_path.join("Libraries");
        if lib_path.exists() {
            let existing = std::env::var("DYLD_FALLBACK_LIBRARY_PATH").unwrap_or_default();
            let new_path = if existing.is_empty() {
                lib_path.to_string_lossy().to_string()
            } else {
                format!("{}:{}", lib_path.to_string_lossy(), existing)
            };
            // SAFETY: set_var is safe here — called once during single-threaded init
            #[allow(deprecated)]
            std::env::set_var("DYLD_FALLBACK_LIBRARY_PATH", &new_path);
        }
    }

    // 6. Initialize CEF
    let result = initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );

    if result != 1 {
        return Err(Error::CefError(format!(
            "CefInitialize failed (returned {result})"
        )));
    }

    wrymium_log!("[wrymium] CEF initialized successfully");

    // 7. Install platform message pump (external_message_pump mode)
    install_message_pump();

    Ok(())
}

// --- CefApp implementation ---

wrap_app! {
    pub struct WrymiumApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if let Some(cmd) = command_line {
                // Avoid macOS Keychain password prompts
                let flag = CefString::from("use-mock-keychain");
                ImplCommandLine::append_switch(cmd, Some(&flag));

                // Limit renderer processes — Tauri apps use a single origin
                // (tauri://localhost) so multiple renderers are wasteful
                let key = CefString::from("renderer-process-limit");
                let val = CefString::from("1");
                ImplCommandLine::append_switch_with_value(cmd, Some(&key), Some(&val));
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(registrar) = registrar {
                crate::scheme::register_custom_schemes(registrar);
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(WrymiumBrowserProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(crate::renderer::WrymiumRenderProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct WrymiumBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            wrymium_log!("[wrymium] CEF context initialized");
        }
    }
}

// --- Platform message pump ---

#[cfg(target_os = "macos")]
fn install_message_pump() {
    use core_foundation_sys::date::CFAbsoluteTimeGetCurrent;
    use core_foundation_sys::runloop::{
        kCFRunLoopCommonModes, CFRunLoopAddTimer, CFRunLoopGetMain, CFRunLoopTimerCreate,
        CFRunLoopTimerRef,
    };
    use std::ffi::c_void;

    // Timer interval: ~30fps = 33ms
    const INTERVAL: f64 = 1.0 / 30.0;

    extern "C" fn timer_callback(
        _timer: CFRunLoopTimerRef,
        _info: *mut c_void,
    ) {
        do_message_loop_work();
    }

    unsafe {
        let now = CFAbsoluteTimeGetCurrent();
        let timer = CFRunLoopTimerCreate(
            std::ptr::null(),     // allocator
            now + INTERVAL,       // first fire date
            INTERVAL,             // interval
            0,                    // flags
            0,                    // order
            timer_callback,       // callback
            std::ptr::null_mut(), // context
        );
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }

    wrymium_log!("[wrymium] macOS CFRunLoopTimer installed at 30fps");
}

#[cfg(target_os = "linux")]
fn install_message_pump() {
    // glib timeout at ~30fps
    glib::timeout_add(std::time::Duration::from_millis(33), || {
        cef::do_message_loop_work();
        glib::ControlFlow::Continue
    });
    wrymium_log!("[wrymium] Linux glib timeout installed at 30fps");
}

#[cfg(target_os = "windows")]
fn install_message_pump() {
    // Windows uses multi_threaded_message_loop = true, no pump needed.
    wrymium_log!("[wrymium] Windows multi_threaded_message_loop enabled");
}

/// Shutdown CEF. Should be called at application exit.
pub fn shutdown() {
    if is_initialized() {
        cef::shutdown();
        wrymium_log!("[wrymium] CEF shutdown complete");
    }
}
