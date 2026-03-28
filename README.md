# wrymium

[中文文档](README.zh-CN.md)

A CEF-powered WebView backend for [wry](https://github.com/tauri-apps/wry), bringing consistent Chromium rendering to [Tauri](https://tauri.app) apps.

## What is wrymium?

Tauri uses system-native WebView (WebKit on macOS, WebView2 on Windows, WebKitGTK on Linux), which introduces rendering inconsistencies across platforms. wrymium replaces the native WebView with the [Chromium Embedded Framework (CEF)](https://bitbucket.org/chromiumembedded/cef), giving Tauri apps a consistent Chromium rendering engine on all platforms.

wrymium exposes a **wry-compatible API**, so it can be used as a drop-in replacement via `[patch.crates-io]`.

## Status

**v0.2 Tauri Integration** — macOS, Tauri 2.x end-to-end verified.

Working features:
- **Tauri 2.x app running on CEF** via `[patch.crates-io]` (wry + tauri-runtime-wry)
- `tauri://localhost` custom protocol serving Tauri frontend assets
- `window.__TAURI_INTERNALS__` injected and functional
- CEF initialization with `external_message_pump` (CFRunLoopTimer at 30fps)
- Browser window embedded in tao window via `set_as_child`
- Custom scheme registration (`ipc://`, `tauri://`, `asset://`) with async `CefResourceHandler`
- `CefSchemeHandlerFactory` with proper async response handling (callback.cont())
- `window.ipc.postMessage` V8 bridge (renderer -> browser IPC)
- Cross-process initialization script injection via `extra_info` (with race condition handling)
- wry-compatible `WebViewBuilder` (35+ methods) and `WebView` (20+ methods)
- **Tauri `invoke()` IPC end-to-end** — frontend calls Rust commands, receives responses
- Shared browser handle via `Arc<Mutex<Option<Browser>>>` for post-creation API calls
- POST body extraction via `CefPostData` + response headers propagation
- DevTools support (`open_devtools` / `close_devtools` / `is_devtools_open`)
- macOS `.app` bundle with 5 helper apps (GPU, Renderer, Plugin, Alerts, Helper)
- Debug-only logging via `wrymium_log!` macro (silent in release builds)
- 41 unit tests, zero warnings

## Architecture

wrymium is an **integration layer** built on top of [`tauri-apps/cef-rs`](https://github.com/tauri-apps/cef-rs):

```
Tauri App
  └── tauri 2.x
        └── tauri-runtime-wry (patched)
              └── wrymium (as "wry")       <- this project
                    └── cef (crates.io)    <- tauri-apps/cef-rs
                          └── cef-dll-sys  <- raw CEF C API FFI
```

## Prerequisites

- **Rust** (stable)
- **CMake** (>= 3.x)
- **Ninja** (required by cef-rs build system)
- **C++ compiler** (Xcode CLT on macOS, MSVC on Windows, g++/clang++ on Linux)

```bash
# macOS
brew install cmake ninja

# Linux
sudo apt install cmake ninja-build build-essential

# Windows — install CMake + Ninja + Visual Studio Build Tools
```

## Quick Start

### 1. Download CEF (first time only)

```bash
cargo install export-cef-dir
export-cef-dir --force ~/.local/share/cef
export CEF_PATH="$(ls -d ~/.local/share/cef/cef_binary_*_minimal)"
```

Or let it auto-download on first `cargo build` (takes 3-10 minutes).

### 2. Build and run an example

```bash
# Set CEF_PATH if you downloaded manually
export CEF_PATH="$HOME/.local/share/cef/cef_binary_146.0.6+g68649e2+chromium-146.0.7680.154_macosarm64_minimal"

# Bundle and run
bash scripts/bundle-macos.sh wrymium-feishu-example "Feishu Doc"
open target/bundle/wrymium-feishu-example.app
```

### 3. Use in your own project

```rust
use tao::{event::{Event, WindowEvent}, event_loop::{ControlFlow, EventLoop}, window::WindowBuilder};
use wry::{WebContext, WebViewBuilder};

fn main() {
    // CEF subprocess check — MUST be first
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("My App")
        .build(&event_loop)
        .unwrap();

    let mut ctx = WebContext::new(None);
    let _webview = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_url("https://example.com")
        .build(&window)
        .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            wry::shutdown();
            *control_flow = ControlFlow::Exit;
        }
    });
}
```

## Examples

| Example | Description | Command |
|---------|-------------|---------|
| `basic` | IPC test with postMessage + custom protocol | `bash scripts/bundle-macos.sh wrymium-basic-example` |
| `feishu` | Opens a Feishu document | `bash scripts/bundle-macos.sh wrymium-feishu-example "Feishu Doc"` |
| `tauri-app` | Full Tauri 2.x app running on CEF | See [Tauri Integration](#tauri-integration) below |

## Tauri Integration

wrymium can replace wry in a real Tauri 2.x application via `[patch.crates-io]`:

```toml
# In your Tauri app's src-tauri/Cargo.toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
wry = "0.54"

[patch.crates-io]
wry = { path = "/path/to/wrymium/wrymium" }
tauri-runtime-wry = { path = "/path/to/wrymium/tauri-runtime-wry" }
```

Add CEF subprocess check at the very beginning of `main()`:

```rust
fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

The patched `tauri-runtime-wry` is included in this repository with ~30 lines of changes to handle CEF-specific type differences (NSView pointers, NewWindowResponse, etc.).

## Project Structure

```
wrymium/
  ├── Cargo.toml                 # workspace root
  ├── wrymium/                   # main library crate (package name: "wry")
  │   ├── src/
  │   │   ├── lib.rs             # public API re-exports
  │   │   ├── webview.rs         # WebView / WebViewBuilder + CefClient
  │   │   ├── cef_init.rs        # CEF lifecycle + message pump
  │   │   ├── scheme.rs          # custom URI scheme handlers (async CefResourceHandler)
  │   │   ├── renderer.rs        # renderer process: V8 bridge + script injection
  │   │   ├── types.rs           # Rect, DragDropEvent, enums
  │   │   ├── error.rs           # Error type (wry-compatible)
  │   │   ├── context.rs         # WebContext
  │   │   ├── tests.rs           # unit tests (34 tests)
  │   │   └── platform/          # platform extension traits
  │   └── Cargo.toml
  ├── tauri-runtime-wry/         # patched fork of tauri-runtime-wry 2.10.1
  ├── examples/
  │   ├── basic/                 # IPC test example
  │   ├── feishu/                # Feishu document viewer
  │   └── tauri-app/             # full Tauri 2.x app on CEF
  ├── scripts/
  │   └── bundle-macos.sh        # macOS .app bundler
  └── docs/                      # spec, research, TODO
```

## How It Works

### CEF Initialization

wrymium initializes CEF lazily on the first `WebViewBuilder::build()` call:

1. Load CEF framework via `LibraryLoader` (macOS dynamic loading)
2. `CefExecuteProcess` — routes subprocess execution
3. `CefInitialize` with `external_message_pump = true`
4. Install `CFRunLoopTimer` at 30fps to drive `CefDoMessageLoopWork()`

### IPC

Tauri 2.x uses a dual-path IPC system:

- **Primary**: `ipc://localhost` custom protocol via `fetch()` — handled by `CefSchemeHandlerFactory`
- **Fallback**: `window.ipc.postMessage` — handled by V8 extension + `CefProcessMessage`

### Script Injection

Initialization scripts are passed from the browser process to renderer subprocess via `extra_info` (`DictionaryValue`), then injected in `OnContextCreated`. A race condition where `OnContextCreated` fires before `OnBrowserCreated` is handled with deferred injection.

## Documentation

- [Getting Started / 使用教程](docs/getting-started.md) — step-by-step guide for standalone and Tauri integration
- [Project Spec](docs/wrymium-spec.md) — full design document
- [wry API Surface](docs/wry-api-surface.md) — inventory of wry symbols wrymium must implement
- [IPC Deep Dive](docs/ipc-deep-dive.md) — Tauri 2.x IPC protocol analysis
- [TODO](docs/TODO.md) — open issues and spike results

## License

MIT
