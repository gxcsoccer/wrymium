# wrymium Testing Strategy

> Comprehensive testing plan for a CEF-backed WebView integration layer.
> CEF's multi-process architecture and single-initialization constraint require
> a fundamentally different approach from standard Rust unit testing.

---

## Table of Contents

1. [How CEF Tests Itself](#1-how-cef-tests-itself)
2. [How Other CEF Embedding Projects Test](#2-how-other-cef-embedding-projects-test)
3. [Off-Screen Rendering for Headless Testing](#3-off-screen-rendering-for-headless-testing)
4. [CI Environment Setup](#4-ci-environment-setup)
5. [Testing Layers for wrymium](#5-testing-layers-for-wrymium)
6. [Test Utilities and Harness Design](#6-test-utilities-and-harness-design)
7. [Known Testing Challenges](#7-known-testing-challenges)
8. [Recommended CI Configuration](#8-recommended-ci-configuration)
9. [Implementation Roadmap](#9-implementation-roadmap)

---

## 1. How CEF Tests Itself

### Test Framework

CEF uses **Google Test (GTest)** and **Google Mock (GMock)** as its testing foundation.
The test suite lives in `tests/ceftests/` and contains **~110 files** covering every
major subsystem.

### Test Organization

CEF's tests are organized into these categories:

| Category | Example Files | What They Test |
|----------|--------------|----------------|
| Scheme handlers | `scheme_handler_unittest.cc` | Custom protocol registration, request/response handling, CORS |
| Process messages | `process_message_unittest.cc`, `shared_process_message_unittest.cc` | Browser-renderer IPC via `CefProcessMessage` |
| V8 integration | `v8_unittest.cc` | V8 value creation, context evaluation, function binding |
| Off-screen rendering | `os_rendering_unittest.cc`, `osr_display_unittest.cc` | Windowless rendering, paint callbacks, event dispatch |
| Message routing | `message_router_*_unittest.cc` (5 files) | `CefMessageRouterBrowserSide`/`RendererSide` |
| Navigation | `navigation_unittest.cc`, `frame_unittest.cc` | Page loading, frame management, redirects |
| Cookies | `cookie_unittest.cc` | Cookie CRUD operations |
| Downloads | `download_unittest.cc` | Download lifecycle events |
| DOM | `dom_unittest.cc` | DOM access from renderer process |
| Pure utilities | `string_unittest.cc`, `parser_unittest.cc`, `values_unittest.cc` | No CEF initialization needed |

### Multi-Process Test Architecture

CEF's test runner (`run_all_unittests.cc`) handles multi-process testing via a
**delegate-based pattern**:

1. **Single test binary** acts as both browser process and subprocess. The `main()`
   function checks `--type=` to decide which role to play.

2. **`TestHandler`** base class implements `CefClient` and manages browser lifecycle.
   It provides:
   - `CreateBrowser()` / `CloseBrowser()` for browser management
   - `CompletionState` / `WaitForTests()` for blocking until async operations finish
   - `SetTestTimeout()` to prevent hung tests
   - Resource mapping for controlled test environments

3. **Renderer-side delegates** implement `ClientAppRenderer::Delegate`. The browser
   process sends a `CefProcessMessage` with a test mode enum; the renderer receives
   it, runs the appropriate test function, and reports results back via another
   process message.

4. **Scheme handler tests** register handlers via `CefRegisterSchemeHandlerFactory()`,
   load a URL, and verify callback flags (`got_request`, `got_read`, `got_output`).

5. **CEF is initialized once** at test suite startup. Individual tests create/destroy
   browsers but never reinitialize CEF.

### Test Execution Modes

CEF supports two test execution modes:

- **Single-threaded**: A dedicated test thread runs tests via `CefPostTask()`, while
  the main thread runs a CEF message loop (standard or external pump)
- **Multi-threaded**: Tests run on the main thread while CEF's message loop runs on
  a separate thread (Windows only)

The `--external-message-pump` flag can be passed to `ceftests` to test external
pump integration -- directly relevant to wrymium's architecture.

---

## 2. How Other CEF Embedding Projects Test

### cef-rs (tauri-apps)

**Testing approach**: Minimal -- primarily build verification + example validation.

- CI workflow (`rust.yml`) runs on Ubuntu, macOS, and Windows
- Tests: `cargo fmt --check`, `cargo build --verbose`, `cargo test --verbose`
- CEF binaries cached via GitHub Actions cache keyed on `cef-${{ matrix.os }}-${{ hashFiles('Cargo.toml') }}`
- CEF downloaded via `cargo run -p export-cef-dir` to `~/.local/share/cef`
- Linux requires `libglib2.0-dev`; macOS and Windows need no extra dependencies
- The `cefsimple` example serves as the primary integration validation
- **No dedicated test suite** for CEF functionality -- relies on successful compilation
  and example execution

**Key takeaway for wrymium**: cef-rs validates that bindings compile and link
correctly but does not run behavioral CEF tests in CI. wrymium must build its own
test harness on top of cef-rs.

### cef-ui (HYTOPIA)

**Testing approach**: No visible test infrastructure.

- Repository is explicitly "work in progress"
- Validates via manual testing through the `cef-ui-simple` example
- No CI workflows, no test directories
- Uses its own bindgen-generated CEF C API bindings (not cef-rs)

**Key takeaway**: Even active CEF Rust projects lack automated testing.
wrymium has an opportunity to set a higher standard.

### Electron

**Testing approach**: Comprehensive multi-layer testing.

- **Unit tests**: Run as an Electron app in the `spec/` directory
- **electron-mocha**: Runs Mocha tests in both the main process and renderer process
- **IPC testing**: `electron-mock-ipc` provides mock `ipcMain`/`ipcRenderer` for
  unit testing without spawning real processes
- **Custom test drivers**: Spawn Electron via `child_process` with a messaging
  protocol for IPC
- **Headless CI**: Uses `xvfb-maybe` on Linux (auto-detects platform); macOS and
  Windows work natively
- **Spectron** (deprecated): Used Selenium WebDriver to drive Electron apps

**Key takeaway**: Electron separates pure-logic unit tests (mockable IPC) from
integration tests (real processes). wrymium should follow this layered approach.

### Chromium Content Embedding (browser_tests)

Chromium's `content/` layer uses `content_browsertests` and `content_unittests`:

- `content_unittests`: Tests that do not require a running browser (data structures,
  serialization, URL parsing)
- `content_browsertests`: Full browser process tests. Each test creates a browser,
  navigates, and asserts. Uses `content::WebContentsObserver` for event detection.
- Tests run with `--disable-gpu` in CI to avoid GPU process issues

**Key takeaway**: The unit/browser-test split is the industry standard for
Chromium-based projects.

---

## 3. Off-Screen Rendering for Headless Testing

### Can CEF Run in OSR Mode Without a Display Server?

**Partially.** OSR (off-screen rendering) eliminates the need for a visible window,
but platform constraints remain:

| Platform | Display Server Required? | Notes |
|----------|------------------------|-------|
| Linux | Yes (X11 libs needed) | Even in windowless mode, CEF links against X11 libraries. Use Xvfb as a virtual display. |
| macOS | No (mostly) | macOS GitHub Actions runners have a window server available. OSR works without extra setup. |
| Windows | No | Windows CI runners support headless operation natively. |

**Critical**: Do NOT use the `--headless` Chrome switch with CEF OSR -- it breaks
off-screen rendering. Use `--disable-gpu` instead.

### Setting Up OSR for Testing

```rust
// CefSettings
let mut settings = CefSettings::default();
settings.windowless_rendering_enabled = true;
// settings.no_sandbox = true;  // may be needed in CI

// CefWindowInfo -- platform-specific
let mut window_info = CefWindowInfo::new();

// Linux: pass kNullWindowHandle (0)
window_info.set_as_windowless(0 as _);

// macOS: pass kNullWindowHandle or a fake NSView
window_info.set_as_windowless(std::ptr::null_mut());

// Windows: pass GetDesktopWindow() or NULL
window_info.set_as_windowless(std::ptr::null_mut());
```

### What OSR Mode CAN Test

- Custom protocol handlers (`ipc://`, `tauri://`, `asset://`)
- IPC message round-trips (browser <-> renderer)
- Script injection (`CefRenderProcessHandler::OnContextCreated`)
- JavaScript evaluation (`CefFrame::ExecuteJavaScript`)
- Navigation events
- Cookie management
- Process message passing
- Scheme handler registration and request/response cycles

### What OSR Mode CANNOT Test

- Actual pixel rendering correctness (would need screenshot comparison)
- GPU-accelerated compositing
- Native window integration (resize, focus, reparent)
- Drag-and-drop with real OS events
- Platform-specific window decorations

### Recommendation

**Use OSR mode for all CI integration/E2E tests.** It tests the CEF integration
layer (IPC, protocols, script injection) without requiring real windows. Reserve
windowed testing for manual QA and optional display-attached CI runs.

---

## 4. CI Environment Setup

### Linux

```yaml
# GitHub Actions
runs-on: ubuntu-latest
steps:
  - name: Install system dependencies
    run: |
      sudo apt-get update
      sudo apt-get install -yq \
        cmake ninja-build \
        libglib2.0-dev \
        xvfb \
        libx11-dev libxcomposite-dev libxdamage-dev \
        libxrandr-dev libxss-dev libxtst-dev \
        libxkbcommon-dev \
        libnss3 libnspr4 \
        libatk1.0-0 libatk-bridge2.0-0 \
        libcups2 libdrm2 libdbus-1-3 \
        libgbm1 libpango-1.0-0 libcairo2 \
        libasound2

  - name: Run tests under Xvfb
    run: xvfb-run --auto-servernum cargo test --verbose
    env:
      DISPLAY: ":99"
```

**Why Xvfb?** CEF requires X11 libraries even in windowless mode. Xvfb provides
a virtual X server that satisfies this requirement without a physical display.

**Alternative**: `coactions/setup-xvfb` GitHub Action handles setup/teardown
automatically.

### macOS

```yaml
runs-on: macos-latest
steps:
  - name: Install system dependencies
    run: brew install cmake ninja

  - name: Run tests
    run: cargo test --verbose
```

**macOS GitHub Actions runners have a window server** (Quartz). No Xvfb equivalent
is needed. CEF can create browsers (windowed or OSR) without extra setup.

**Caveat**: Screen recording permissions may block some UI automation. For OSR-only
tests this is not an issue.

### Windows

```yaml
runs-on: windows-latest
steps:
  - name: Install system dependencies
    run: choco install cmake ninja -y

  - name: Run tests
    run: cargo test --verbose
```

**Windows CI runners support GUI operations** natively. No virtual display needed.

### Common CI Flags

```bash
# Recommended CEF command-line switches for CI:
--disable-gpu                    # Prevents GPU process crashes in virtualized environments
--disable-gpu-compositing        # Forces software compositing
--no-sandbox                     # May be needed in containerized CI (use sparingly)
--disable-software-rasterizer    # Reduces memory usage
--disable-dev-shm-usage          # Prevents /dev/shm exhaustion in Docker
```

These should be passed via `CefSettings::command_line_args_disabled = false` and
a custom `CefApp::OnBeforeCommandLineProcessing` implementation, or via
`CefSettings::browser_subprocess_path` arguments.

### Resource Requirements

| Resource | Estimate | Notes |
|----------|----------|-------|
| CEF download | ~80-100 MB compressed | Cached between CI runs |
| CEF unpacked | ~250-350 MB | Platform-dependent |
| CMake build (libcef_dll_wrapper) | ~200 MB | Cached in `target/` |
| Cargo build (wrymium + deps) | ~500 MB-1 GB | Standard Rust build cache |
| **Total disk** | **~1.5-2 GB** | Within GitHub Actions 14 GB limit |
| **RAM** | **~2-4 GB** | CEF spawns multiple processes |
| **Build time (cold)** | **8-15 min** | CEF download + C++ compilation + Rust compilation |
| **Build time (cached)** | **3-6 min** | Rust incremental + cached CEF/CMake |

### Caching Strategy

```yaml
- name: Cache CEF binaries
  uses: actions/cache@v4
  with:
    path: ~/.local/share/cef
    key: cef-${{ runner.os }}-${{ hashFiles('**/Cargo.toml') }}

- name: Cache Cargo build
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry
      ~/.cargo/git
      target/
    key: cargo-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}
```

---

## 5. Testing Layers for wrymium

### Layer A: Unit Tests (No CEF Required)

These tests run with `cargo test` without CEF initialization. They are fast,
deterministic, and should cover the majority of wrymium's non-CEF code.

**What to test:**

```
wrymium/src/
  types.rs      ← Rect, DragDropEvent, ProxyConfig, enums
  context.rs    ← WebContext (data directory path logic)
  lib.rs        ← Re-exports, feature flags
```

| Test Target | Example Tests |
|-------------|---------------|
| `Rect` construction | `Rect { position: Logical(0,0), size: Logical(800,600) }` round-trips correctly |
| `ProxyConfig` serialization | `ProxyConfig::Http(endpoint)` converts to CEF proxy string format |
| `DragDropEvent` enum | Pattern matching on all variants works |
| `WebContext::new()` | `None` path uses default; `Some(path)` stores correctly |
| `Error` type | `Error::MessageSender` exists; `Error::CefError(msg)` formats correctly |
| Feature flags | `protocol`, `os-webview`, `linux-body` features compile as no-ops |
| `webview_version()` | Returns a string (mock CEF version if CEF not available) |
| IPC message serialization | Request headers, callback IDs serialize/deserialize correctly |
| URL parsing | `ipc://localhost/cmd` parsed correctly |
| Initialization script storage | Scripts stored, retrieved by browser_id |

**Test structure:**

```rust
// wrymium/src/types.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_default() {
        let r = Rect {
            position: dpi::Position::Logical(dpi::LogicalPosition::new(0.0, 0.0)),
            size: dpi::Size::Logical(dpi::LogicalSize::new(800.0, 600.0)),
        };
        // assert fields
    }

    #[test]
    fn proxy_config_to_cef_string() {
        let config = ProxyConfig::Http(ProxyEndpoint {
            host: "127.0.0.1".into(),
            port: "8080".into(),
        });
        assert_eq!(to_cef_proxy_string(&config), "http=127.0.0.1:8080");
    }
}
```

**Estimated count**: 30-50 unit tests
**Run time**: < 1 second

### Layer B: Integration Tests (CEF Browser Process Only)

These tests initialize CEF once and test browser-process functionality in OSR mode.
They do NOT test renderer-process behavior (V8, script injection).

**What to test:**

| Test Target | What It Validates |
|-------------|-------------------|
| CEF initialization | `CefInitialize()` succeeds with `external_message_pump = true` |
| Scheme registration | `ipc://`, `tauri://`, `asset://` schemes registered without error |
| Scheme handler factory | Factory creates handlers; handlers receive `CefRequest` |
| WebViewBuilder configuration | All 35+ builder methods produce valid CEF settings |
| Browser creation (OSR) | `browser_host_create_browser()` succeeds in windowless mode |
| Browser lifecycle | `OnAfterCreated`, `OnBeforeClose` fire in correct order |
| URL loading | `load_url("about:blank")` triggers `OnLoadEnd` |
| Navigation handler | Navigation callback receives URL; returning `false` blocks navigation |
| Cookie management | `set_cookie` / `cookies_for_url` / `delete_cookie` round-trip |
| Message loop pump | `CefDoMessageLoopWork()` does not crash when called repeatedly |

**Test structure:**

```rust
// tests/integration_browser.rs
// This is a single test binary that acts as CEF browser + subprocess

use std::sync::Once;

static CEF_INIT: Once = Once::new();

fn ensure_cef_initialized() {
    CEF_INIT.call_once(|| {
        // Check if this is a subprocess
        if is_cef_subprocess() {
            // Run subprocess logic and exit
            std::process::exit(run_cef_subprocess());
        }

        let mut settings = CefSettings::default();
        settings.windowless_rendering_enabled = true;
        settings.external_message_pump = true;
        settings.no_sandbox = true; // CI-friendly
        // ... additional CI flags

        cef_initialize(&settings, app);
    });
}

#[test]
fn test_scheme_handler_registration() {
    ensure_cef_initialized();
    // Register ipc:// scheme handler
    // Create OSR browser loading ipc://localhost/test
    // Assert handler's got_request flag is set
    // Close browser
}

#[test]
fn test_browser_creation_osr() {
    ensure_cef_initialized();
    // Create windowless browser
    // Wait for OnAfterCreated
    // Close browser
    // Wait for OnBeforeClose
}
```

**Critical design decision**: All integration tests share a single `CefInitialize()`
call. Tests must be careful about shared state. Use `#[serial_test::serial]` or
a custom test runner to prevent browser-count races.

**Estimated count**: 20-30 integration tests
**Run time**: 10-30 seconds

### Layer C: End-to-End Tests (CEF Browser + Renderer Process)

These tests exercise the full multi-process pipeline: browser creates a page,
renderer executes JavaScript, results flow back to browser.

**What to test:**

| Test Target | Scenario |
|-------------|----------|
| IPC primary path | JS `fetch("ipc://localhost/test")` -> scheme handler -> response -> JS callback |
| IPC fallback path | `window.ipc.postMessage("hello")` -> V8 handler -> CefProcessMessage -> browser handler |
| Script injection | `with_initialization_script("window.injected = true")` -> verify via `evaluate_script` |
| evaluate_script | `evaluate_script("1 + 1")` -> result delivered (note: CEF ExecuteJavaScript is fire-and-forget; use message router for return values) |
| Navigation events | Load URL -> `PageLoadEvent::Started` -> `PageLoadEvent::Finished` with correct URL |
| Custom protocol serving | `tauri://localhost/index.html` -> scheme handler serves HTML -> page loads |
| Cross-process script injection | Scripts injected via CefProcessMessage arrive before first `OnContextCreated` |
| Document title change | Page sets `document.title = "Test"` -> `document_title_changed_handler` fires |
| Download events | Navigate to downloadable resource -> `download_started_handler` fires |

**Test structure:**

```rust
#[test]
fn test_ipc_custom_protocol_roundtrip() {
    ensure_cef_initialized();

    let (tx, rx) = std::sync::mpsc::channel();

    // Register scheme handler that echoes request body
    let handler = IpcSchemeHandlerFactory::new(move |request| {
        let body = read_request_body(&request);
        tx.send(body.clone()).unwrap();
        Ok(Response::new(body))
    });
    register_scheme_handler("ipc", "localhost", handler);

    // Create OSR browser
    let browser = create_test_browser("about:blank");
    wait_for_load(&browser);

    // Execute fetch from JS
    browser.get_main_frame().execute_javascript(
        r#"fetch("ipc://localhost/test", {
            method: "POST",
            body: "hello from js"
        })"#,
        "about:blank",
        0,
    );

    // Wait for scheme handler to receive request
    let received = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(received, "hello from js");

    close_test_browser(&browser);
}

#[test]
fn test_initialization_script_injection() {
    ensure_cef_initialized();

    let (tx, rx) = std::sync::mpsc::channel();

    // Build webview with init script
    let webview = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_initialization_script_for_main_only(
            "window.ipc.postMessage('script_loaded')".to_string(),
            true,
        )
        .with_ipc_handler(Box::new(move |req| {
            tx.send(req.body().clone()).unwrap();
        }))
        .with_url("about:blank")
        .build_osr()?;

    let msg = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(msg, "script_loaded");
}
```

**Estimated count**: 15-25 E2E tests
**Run time**: 30-90 seconds

### Layer D: Tauri Integration Tests

These tests verify that a real Tauri app compiles and runs with wrymium patches.

**What to test:**

| Test | How |
|------|-----|
| Compilation | `cargo check` on a minimal Tauri app with `[patch.crates-io]` pointing to wrymium |
| Feature resolution | All feature flags (`protocol`, `os-webview`, `devtools`, etc.) resolve correctly |
| App startup | App launches, CEF initializes, window appears (or OSR browser created) |
| IPC round-trip | Tauri `invoke("greet", { name: "test" })` -> Rust handler -> response |
| Version compatibility | wrymium's API surface matches the specific tauri-runtime-wry version |

**Implementation approach:**

```
wrymium/
  tests/
    tauri-app/               # Minimal Tauri project
      src-tauri/
        Cargo.toml           # [patch.crates-io] wry = { path = "../../.." }
        src/main.rs          # Basic app with one invoke handler
      src/
        index.html           # fetch("ipc://localhost/greet")
```

```rust
// tests/tauri_integration.rs
#[test]
#[ignore] // Run only with --ignored flag or in CI
fn test_tauri_app_compiles() {
    let status = Command::new("cargo")
        .args(["check", "--manifest-path", "tests/tauri-app/src-tauri/Cargo.toml"])
        .status()
        .unwrap();
    assert!(status.success());
}
```

**Automation**: Run as a separate CI job with longer timeout. The Tauri app test
can be marked `#[ignore]` for local development and explicitly included in CI.

**Estimated count**: 3-5 tests
**Run time**: 2-5 minutes (mostly compilation)

---

## 6. Test Utilities and Harness Design

### CEF Test Harness

The most critical piece of infrastructure. Handles CEF's single-initialization
constraint and subprocess routing.

```rust
// wrymium/tests/common/harness.rs

use std::sync::Once;
use std::time::Duration;

static CEF_INIT: Once = Once::new();
static mut CEF_APP: Option<TestCefApp> = None;

/// Must be called at the start of every integration/E2E test.
/// Initializes CEF exactly once. If the current process is a CEF
/// subprocess (renderer, GPU, utility), it runs the subprocess
/// logic and exits -- the test never continues.
pub fn ensure_cef() {
    CEF_INIT.call_once(|| {
        // Subprocess check MUST be first
        let args = std::env::args().collect::<Vec<_>>();
        if args.iter().any(|a| a.starts_with("--type=")) {
            std::process::exit(cef::execute_process(&args));
        }

        let settings = test_cef_settings();
        let app = TestCefApp::new();
        cef::initialize(&settings, &app);

        unsafe { CEF_APP = Some(app); }
    });
}

fn test_cef_settings() -> CefSettings {
    let mut s = CefSettings::default();
    s.windowless_rendering_enabled = true;
    s.external_message_pump = true;
    s.no_sandbox = true;
    s.log_severity = CefLogSeverity::Warning; // reduce noise

    // CI-specific flags
    if std::env::var("CI").is_ok() {
        // Add --disable-gpu etc. via command line processing
    }

    s
}

/// Run the CEF message loop for up to `timeout`, or until `predicate`
/// returns true. Returns true if predicate was satisfied.
pub fn pump_until(timeout: Duration, predicate: impl Fn() -> bool) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        cef::do_message_loop_work();
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}
```

### Test Browser Factory

```rust
// wrymium/tests/common/browser.rs

pub struct TestBrowser {
    browser: CefBrowser,
    load_complete: Arc<AtomicBool>,
    title: Arc<Mutex<String>>,
    close_complete: Arc<AtomicBool>,
}

impl TestBrowser {
    /// Create an OSR browser loading the given URL.
    /// Blocks (pumping CEF) until OnLoadEnd fires.
    pub fn new(url: &str) -> Self { /* ... */ }

    /// Execute JavaScript and wait for a result via message router.
    pub fn eval_js(&self, script: &str) -> Result<String, Timeout> { /* ... */ }

    /// Wait for the document title to change.
    pub fn wait_for_title(&self, expected: &str, timeout: Duration) -> bool { /* ... */ }

    /// Close the browser and wait for OnBeforeClose.
    pub fn close(self) { /* ... */ }
}
```

### Mock Scheme Handler

```rust
// wrymium/tests/common/mock_scheme.rs

pub struct MockSchemeHandler {
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    response_fn: Box<dyn Fn(&CefRequest) -> MockResponse + Send + Sync>,
}

pub struct RecordedRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub struct MockResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl MockSchemeHandler {
    pub fn new(response_fn: impl Fn(&CefRequest) -> MockResponse + Send + Sync + 'static) -> Self {
        /* ... */
    }

    /// Returns all requests recorded by this handler.
    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}
```

### JavaScript Evaluation Helper

CEF's `ExecuteJavaScript` is fire-and-forget (no return value). To get results
back, use one of these patterns:

**Pattern 1: Message Router**
```javascript
// In the JS being evaluated:
window.cefQuery({ request: JSON.stringify(result), persistent: false });
```

**Pattern 2: Custom Protocol Callback**
```javascript
// In the JS being evaluated:
fetch("ipc://localhost/__test_result__", {
    method: "POST",
    body: JSON.stringify(result)
});
```

**Pattern 3: Title Change (simplest)**
```javascript
document.title = JSON.stringify(result);
```
Then observe via `CefDisplayHandler::OnTitleChange`. This is the simplest approach
and is used by several CEF test suites.

```rust
// wrymium/tests/common/eval.rs

/// Evaluate JavaScript and return the result as a string.
/// Uses the document.title trick: sets title to the result,
/// then reads it back via OnTitleChange.
pub fn eval_js_sync(browser: &TestBrowser, script: &str, timeout: Duration) -> Result<String, EvalError> {
    let wrapped = format!(
        r#"try {{
            let __result = (function() {{ {} }})();
            document.title = "OK:" + JSON.stringify(__result);
        }} catch(e) {{
            document.title = "ERR:" + e.toString();
        }}"#,
        script
    );

    browser.get_main_frame().execute_javascript(&wrapped, "eval", 0);

    // Pump CEF and wait for title change
    let title = browser.wait_for_title_prefix("OK:", timeout)
        .or_else(|| browser.wait_for_title_prefix("ERR:", timeout));

    match title {
        Some(t) if t.starts_with("OK:") => Ok(t[3..].to_string()),
        Some(t) if t.starts_with("ERR:") => Err(EvalError::JsError(t[4..].to_string())),
        _ => Err(EvalError::Timeout),
    }
}
```

### Timeout / Async Helpers

```rust
/// Wait for a channel message with timeout, pumping CEF message loop
/// while waiting. This is essential because CEF operations (scheme
/// handlers, process messages) only progress when the message loop runs.
pub fn recv_with_pump<T>(rx: &Receiver<T>, timeout: Duration) -> Result<T, RecvTimeoutError> {
    let start = Instant::now();
    loop {
        match rx.try_recv() {
            Ok(val) => return Ok(val),
            Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
            Err(TryRecvError::Empty) => {
                if start.elapsed() > timeout {
                    return Err(RecvTimeoutError::Timeout);
                }
                cef::do_message_loop_work();
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}
```

---

## 7. Known Testing Challenges

### 7.1 CEF Can Only Be Initialized Once Per Process

**Problem**: `CefInitialize()` can only be called once. There is no `CefReset()`
or `CefReinitialize()`.

**Mitigation**:
- Use `std::sync::Once` to guard initialization
- All tests in a single integration test binary share one CEF instance
- Use separate `CefRequestContext` instances per test for isolation where needed
- Use `CefRequestContext::SetPreference()` for runtime configuration changes
- Accept that integration tests cannot have fully isolated CEF state

### 7.2 Subprocess Handling in Test Binaries

**Problem**: When `cargo test` runs an integration test binary, CEF may re-exec
the same binary with `--type=renderer`. The test binary must handle this.

**Mitigation**:
- Check for `--type=` in `main()` or test harness init
- If present, run `cef::execute_process()` and `std::process::exit()`
- This must happen before any test framework initialization
- For `cargo test`, this means using a custom test harness or checking in each
  test's setup

**Alternative**: Set `CefSettings::browser_subprocess_path` to a dedicated helper
binary. This separates concerns but adds build complexity.

### 7.3 GPU Process Crashes in CI

**Problem**: Virtualized CI environments (Docker, GitHub Actions) often lack
GPU drivers, causing the CEF GPU process to crash.

**Mitigation**:
- Always pass `--disable-gpu` and `--disable-gpu-compositing` in CI
- Use `--in-process-gpu` to keep GPU work in the browser process (less isolation
  but avoids the separate process crash)
- Set `GALLIUM_DRIVER=llvmpipe` on Linux for software rendering
- OSR mode sidesteps most GPU issues since it does not composite to a real surface

### 7.4 Flaky Tests Due to Multi-Process Timing

**Problem**: CEF operations are inherently async and cross-process. A test may
check a condition before the renderer process has finished its work.

**Mitigation**:
- Never assert immediately after an async operation
- Always use `pump_until()` with a predicate and timeout
- Set generous timeouts (5-10 seconds) to absorb CI variance
- Use callback flags (`AtomicBool`) instead of timing-based assertions
- Log CEF process messages to diagnose flaky test failures

### 7.5 Test Binary Acts as Subprocess

**Problem**: `cargo test` may run multiple test binaries. Each binary might be
re-executed by CEF as a subprocess. If a test binary does not handle the `--type=`
argument, it will fail or hang.

**Mitigation**:
- Every integration test binary must include subprocess handling at the top of
  its entry point
- Use a `#[ctor]` or custom test harness that checks arguments before the test
  framework starts
- Keep all CEF integration tests in a single binary to minimize subprocess confusion

### 7.6 Resource Cleanup Between Tests

**Problem**: Since CEF is initialized once, browsers and scheme handlers from one
test may affect the next.

**Mitigation**:
- Each test must close all browsers it creates
- Use unique scheme handler domains per test (e.g., `test1.localhost`, `test2.localhost`)
- Clear scheme handler factories between tests via `CefClearSchemeHandlerFactories()`
- Use `#[serial_test::serial]` for tests that share global scheme registrations

---

## 8. Recommended CI Configuration

### Full GitHub Actions Workflow

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  # ─────────────────────────────────────────────
  # Layer A: Unit tests (fast, no CEF)
  # ─────────────────────────────────────────────
  unit-tests:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Run unit tests
        run: cargo test --lib --verbose

  # ─────────────────────────────────────────────
  # Layer B+C: Integration & E2E tests (require CEF)
  # ─────────────────────────────────────────────
  integration-tests:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      # ── System dependencies ──
      - name: Install dependencies (Linux)
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -yq \
            cmake ninja-build \
            libglib2.0-dev xvfb \
            libx11-dev libxcomposite-dev libxdamage-dev \
            libxrandr-dev libxss-dev libxtst-dev \
            libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 \
            libcups2 libdrm2 libdbus-1-3 libgbm1 \
            libpango-1.0-0 libcairo2 libasound2

      - name: Install dependencies (macOS)
        if: runner.os == 'macOS'
        run: brew install cmake ninja

      - name: Install dependencies (Windows)
        if: runner.os == 'Windows'
        run: choco install cmake ninja -y

      # ── Caching ──
      - name: Cache CEF binaries
        uses: actions/cache@v4
        with:
          path: |
            ~/.local/share/cef
            ~/Library/Application Support/cef
            ~\AppData\Local\cef
          key: cef-${{ runner.os }}-v146

      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target/
          key: cargo-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}

      # ── Download CEF if not cached ──
      - name: Ensure CEF available
        run: cargo run -p export-cef-dir
        env:
          CEF_VERSION: "146"

      # ── Run integration tests ──
      - name: Run integration tests (Linux)
        if: runner.os == 'Linux'
        run: xvfb-run --auto-servernum cargo test --test '*' --verbose
        env:
          WRYMIUM_CI: "1"
          DISPLAY: ":99"

      - name: Run integration tests (macOS/Windows)
        if: runner.os != 'Linux'
        run: cargo test --test '*' --verbose
        env:
          WRYMIUM_CI: "1"

  # ─────────────────────────────────────────────
  # Layer D: Tauri integration (compile check)
  # ─────────────────────────────────────────────
  tauri-integration:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    needs: [integration-tests]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      # (same dependency and cache steps as above)

      - name: Check Tauri app compiles
        run: cargo check --manifest-path tests/tauri-app/src-tauri/Cargo.toml

  # ─────────────────────────────────────────────
  # Formatting & linting (no CEF)
  # ─────────────────────────────────────────────
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
```

### Estimated CI Times

| Job | Cold | Cached |
|-----|------|--------|
| Unit tests (per platform) | 2-3 min | 1-2 min |
| Integration tests (per platform) | 12-18 min | 4-8 min |
| Tauri integration (per platform) | 10-15 min | 3-5 min |
| Lint | 1-2 min | < 1 min |
| **Total (parallel)** | **~18 min** | **~8 min** |

---

## 9. Implementation Roadmap

### Phase 1: Unit Test Foundation (v0.1)

**Goal**: Test all non-CEF code. Zero external dependencies.

- [ ] Implement `types.rs` unit tests (Rect, ProxyConfig, DragDropEvent, etc.)
- [ ] Implement `context.rs` unit tests (WebContext path logic)
- [ ] Implement IPC message parsing tests (URL parsing, header extraction)
- [ ] Implement Error type tests
- [ ] Set up `cargo test --lib` in CI (no CEF needed)

### Phase 2: CEF Test Harness (v0.1)

**Goal**: Build the test infrastructure that all integration tests depend on.

- [ ] Implement `tests/common/harness.rs` (CEF init, subprocess handling)
- [ ] Implement `tests/common/browser.rs` (TestBrowser with OSR)
- [ ] Implement `tests/common/eval.rs` (JS evaluation helper via title trick)
- [ ] Implement `tests/common/mock_scheme.rs` (MockSchemeHandler)
- [ ] Implement `pump_until()` and `recv_with_pump()` helpers
- [ ] Verify harness works on macOS (primary v0.1 target)
- [ ] Add CI workflow for macOS integration tests

### Phase 3: Integration Tests (v0.1 - v0.2)

**Goal**: Test browser-process CEF operations.

- [ ] Test CEF initialization with `external_message_pump = true`
- [ ] Test scheme handler registration (`ipc://`, `tauri://`, `asset://`)
- [ ] Test browser creation and lifecycle in OSR mode
- [ ] Test URL loading and `OnLoadEnd` events
- [ ] Test navigation handler (block/allow)
- [ ] Test cookie CRUD
- [ ] Test `WebViewBuilder` configuration methods

### Phase 4: E2E Tests (v0.2)

**Goal**: Test full multi-process pipeline.

- [ ] Test IPC primary path (`fetch("ipc://localhost/...")`)
- [ ] Test IPC fallback path (`window.ipc.postMessage()`)
- [ ] Test initialization script injection
- [ ] Test `evaluate_script` with result retrieval
- [ ] Test page load events (Started/Finished)
- [ ] Test custom protocol asset serving (`tauri://localhost/index.html`)
- [ ] Test document title change handler

### Phase 5: Cross-Platform CI (v0.3 - v0.4)

**Goal**: Run all tests on all platforms.

- [ ] Add Linux CI with Xvfb
- [ ] Add Windows CI
- [ ] Verify OSR tests pass on all platforms
- [ ] Add platform-specific tests (extension traits)

### Phase 6: Tauri Integration Tests (v0.2+)

**Goal**: Verify real Tauri app compatibility.

- [ ] Create minimal Tauri test app in `tests/tauri-app/`
- [ ] Add `cargo check` CI job for Tauri app
- [ ] Add runtime test (app launches, IPC works)

---

## Summary

The testing strategy is built around four layers, ordered by CEF dependency:

```
Layer A: Unit Tests          ← No CEF, fast, deterministic
    types, serialization, URL parsing, config logic

Layer B: Integration Tests   ← CEF browser process, OSR mode
    scheme handlers, browser lifecycle, navigation, cookies

Layer C: E2E Tests           ← CEF browser + renderer, full IPC
    custom protocols, postMessage, script injection, JS eval

Layer D: Tauri Integration   ← Full Tauri app, compile + runtime check
    API compatibility, invoke round-trip
```

Key infrastructure pieces:
1. **Test harness** with `Once`-guarded CEF initialization and subprocess routing
2. **OSR mode** for all CI tests (no display server needed on macOS/Windows; Xvfb on Linux)
3. **`pump_until()`** helper for async CEF operations
4. **Title-change trick** for synchronous JS evaluation results
5. **Mock scheme handlers** for protocol testing
6. **CEF binary caching** in CI to avoid 80-100 MB re-downloads

The approach mirrors CEF's own `ceftests` architecture: one process binary, one
CEF initialization, delegate-based browser/renderer coordination, GTest-style
assertions with async completion tracking.
