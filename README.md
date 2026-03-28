# wrymium

[中文文档](README.zh-CN.md)

A CEF-powered WebView backend for [wry](https://github.com/tauri-apps/wry), bringing consistent Chromium rendering to [Tauri](https://tauri.app) apps.

## What is wrymium?

Tauri uses system-native WebView (WebKit on macOS, WebView2 on Windows, WebKitGTK on Linux), which introduces rendering inconsistencies across platforms. wrymium replaces the native WebView with the [Chromium Embedded Framework (CEF)](https://bitbucket.org/chromiumembedded/cef), giving Tauri apps a consistent Chromium rendering engine on all platforms.

wrymium exposes a **wry-compatible API**, so it can be used as a drop-in replacement via `[patch.crates-io]`.

## Status

**v0.1 Proof of Concept** — macOS only.

Working features:
- CEF initialization with `external_message_pump` (CFRunLoopTimer at 30fps)
- Browser window embedded in tao window via `set_as_child`
- Custom scheme registration (`ipc://`, `tauri://`, `asset://`)
- `CefSchemeHandlerFactory` + `CefResourceHandler` for protocol handling
- `window.ipc.postMessage` V8 bridge (renderer -> browser IPC)
- Cross-process initialization script injection via `extra_info`
- wry-compatible `WebViewBuilder` (35+ methods) and `WebView` (20+ methods)
- macOS `.app` bundle with 5 helper apps (GPU, Renderer, Plugin, Alerts, Helper)

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

## Project Structure

```
wrymium/
  ├── Cargo.toml                 # workspace root
  ├── wrymium/                   # main library crate (package name: "wry")
  │   ├── src/
  │   │   ├── lib.rs             # public API re-exports
  │   │   ├── webview.rs         # WebView / WebViewBuilder + CefClient
  │   │   ├── cef_init.rs        # CEF lifecycle + message pump
  │   │   ├── scheme.rs          # custom URI scheme handlers
  │   │   ├── renderer.rs        # renderer process: V8 bridge + script injection
  │   │   ├── types.rs           # Rect, DragDropEvent, enums
  │   │   ├── error.rs           # Error type (wry-compatible)
  │   │   ├── context.rs         # WebContext
  │   │   ├── tests.rs           # unit tests
  │   │   └── platform/          # platform extension traits
  │   └── Cargo.toml
  ├── examples/
  │   ├── basic/                 # IPC test example
  │   └── feishu/                # Feishu document viewer
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

- [Project Spec](docs/wrymium-spec.md) — full design document
- [wry API Surface](docs/wry-api-surface.md) — inventory of wry symbols wrymium must implement
- [IPC Deep Dive](docs/ipc-deep-dive.md) — Tauri 2.x IPC protocol analysis
- [TODO](docs/TODO.md) — open issues and spike results

## License

MIT
