# wrymium

[中文文档](README.zh-CN.md)

A CEF-powered WebView backend for [wry](https://github.com/tauri-apps/wry), bringing consistent Chromium rendering to [Tauri](https://tauri.app) apps — with the performance of Rust instead of Node.js.

## Why wrymium?

| | Electron | Tauri (native WebView) | **wrymium (Tauri + CEF)** |
|---|---|---|---|
| Rendering | Chromium | WebKit/WebView2/WebKitGTK | **Chromium (consistent)** |
| Backend | Node.js | Rust | **Rust** |
| Cross-platform consistency | Excellent | Poor (3 different engines) | **Excellent** |
| Backend performance | JavaScript (V8) | Native (Rust) | **Native (Rust)** |
| Security model | Node.js full access | Tauri ACL | **Tauri ACL** |

**wrymium = Electron's rendering consistency + Tauri's Rust performance and security model.**

### Benchmark (Apple-to-Apple)

Same HTML page, same `greet` IPC command, macOS, Apple M2 Max:

| Metric | wrymium | Electron | |
|--------|---------|----------|-|
| Bundle size | **257 MB** | 247 MB | Comparable |
| App binary | **4.6 MB** | ~49 MB (Node.js) | **91% smaller** |
| Main process memory | **198 MB** | 285 MB (3 processes) | **30% less** |
| Total memory | 569 MB | 452 MB | CEF uses stricter process isolation |
| Process count | **5** | 7 | Fewer processes |
| IPC latency | **0.48 ms** | 0.3-0.5 ms | Same ballpark |

Full benchmark: [docs/benchmark.md](docs/benchmark.md)

## Quick Start

### Install tooling

```bash
# macOS
brew install cmake ninja
cargo install cargo-wrymium --git https://github.com/gxcsoccer/wrymium
```

### One-command run

```bash
# Clone and run an example
git clone https://github.com/gxcsoccer/wrymium
cd wrymium/examples/feishu
cargo wrymium run
```

That's it. `cargo wrymium` handles build → CEF bundle → launch automatically.

### Use in your own Tauri app

**1. Add patches** to `src-tauri/Cargo.toml`:

```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
wry = "0.54"

[patch.crates-io]
wry = { git = "https://github.com/gxcsoccer/wrymium", package = "wry" }
tauri-runtime-wry = { git = "https://github.com/gxcsoccer/wrymium", path = "tauri-runtime-wry" }
```

**2. Add CEF subprocess check** to `main.rs`:

```rust
fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![/* your commands */])
        .run(tauri::generate_context!())
        .unwrap();
}
```

**3. Build and run:**

```bash
cargo wrymium run              # debug
cargo wrymium run --release    # optimized (LTO + strip)
```

## Examples

| Example | Description | Command |
|---------|-------------|---------|
| `basic` | IPC test (postMessage + custom protocol + init scripts) | `cargo wrymium run` |
| `feishu` | Feishu document viewer | `cd examples/feishu && cargo wrymium run` |
| `tauri-app` | Full Tauri 2.x app with `invoke()` IPC | See [Getting Started](docs/getting-started.md) |

## Developer Tools

```bash
# cargo xtask — works within the wrymium repo
cargo xtask run wrymium-feishu-example

# cargo wrymium — works in any wrymium project
cargo wrymium run                  # auto-detect binary name
cargo wrymium run --release        # optimized build
cargo wrymium bundle --release     # bundle without launching
```

## Architecture

```
Tauri App
  └── tauri 2.x
        └── tauri-runtime-wry (patched, ~30 lines changed)
              └── wrymium (as "wry")       <- this project
                    └── cef (crates.io)    <- tauri-apps/cef-rs
                          └── cef-dll-sys  <- raw CEF C API FFI
```

### Key implementation details

- **CEF message pump**: `CFRunLoopTimer` at 30fps drives `CefDoMessageLoopWork()`, coexisting with tao's event loop
- **IPC**: Tauri's `invoke()` goes through `fetch('ipc://localhost/cmd')` → `CefSchemeHandlerFactory` → async `CefResourceHandler` → Tauri command dispatch (0.48ms round-trip)
- **Script injection**: Browser → renderer via `extra_info` (`DictionaryValue`), with deferred injection for race conditions
- **V8 bridge**: `window.ipc.postMessage` via `CefV8Handler` + `CefProcessMessage` for fallback path

## Project Structure

```
wrymium/
  ├── wrymium/                   # main library (package name: "wry", version 0.54.0)
  ├── tauri-runtime-wry/         # patched fork (based on 2.10.1)
  ├── cargo-wrymium/             # cargo subcommand for build + bundle + run
  ├── xtask/                     # workspace dev helper
  ├── examples/
  │   ├── basic/                 # IPC test
  │   ├── feishu/                # Feishu doc viewer
  │   └── tauri-app/             # full Tauri 2.x app
  ├── scripts/bundle-macos.sh    # shell-based bundler
  └── docs/                      # spec, benchmark, tutorial
```

## Documentation

- [Getting Started / 使用教程](docs/getting-started.md) — step-by-step guide
- [Benchmark](docs/benchmark.md) — wrymium vs Electron comparison
- [Project Spec](docs/wrymium-spec.md) — architecture and design
- [TODO](docs/TODO.md) — roadmap and open issues

## License

MIT
