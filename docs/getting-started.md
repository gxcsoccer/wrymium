# Getting Started with wrymium

本教程将带你从零开始，用 wrymium 替换 Tauri 应用的原生 WebView，使用 Chromium (CEF) 渲染前端。

## 前置条件

### 系统依赖

```bash
# macOS
brew install cmake ninja
xcode-select --install  # Xcode Command Line Tools
```

### 安装 cargo-wrymium

```bash
cargo install cargo-wrymium --git https://github.com/gxcsoccer/wrymium
```

安装后即可使用 `cargo wrymium run` 一键编译、打包、启动。

### 下载 CEF（可选，加速首次构建）

首次 `cargo build` 会自动从 Spotify CDN 下载 CEF（约 80-100MB），需要 3-10 分钟。如果你想提前下载并缓存：

```bash
cargo install export-cef-dir
export-cef-dir --force ~/.local/share/cef
```

设置环境变量避免重复下载：

```bash
# 加到 ~/.zshrc 或 ~/.bashrc
export CEF_PATH="$(ls -d ~/.local/share/cef/cef_binary_*_minimal 2>/dev/null | head -1)"
```

---

## 方式一：独立使用（不依赖 Tauri）

最简单的方式——用 tao 创建窗口，wrymium 提供 CEF WebView。

### 1. 创建项目

```bash
cargo new my-cef-app
cd my-cef-app
```

### 2. 配置 Cargo.toml

```toml
[package]
name = "my-cef-app"
version = "0.1.0"
edition = "2021"

[dependencies]
wry = { git = "https://github.com/gxcsoccer/wrymium", package = "wry" }
tao = "0.35"
```

### 3. 编写代码

```rust
// src/main.rs
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

fn main() {
    // 必须放在 main() 最开头！
    // CEF 会用同一个 binary 启动 renderer/GPU 等子进程
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("My CEF App")
        .with_inner_size(tao::dpi::LogicalSize::new(1200.0, 800.0))
        .build(&event_loop)
        .unwrap();

    let mut ctx = WebContext::new(None);
    let _webview = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_url("https://tauri.app")
        .with_devtools(true)
        .build(&window)
        .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested, ..
        } = event
        {
            wry::shutdown();
            *control_flow = ControlFlow::Exit;
        }
    });
}
```

### 4. 编译并运行

```bash
cargo wrymium run
```

一个命令完成 编译 → CEF 打包 → 启动。首次编译较慢（下载 CEF + CMake 编译 libcef_dll_wrapper），后续秒级。

发布模式（更小的 binary、更好的性能）：

```bash
cargo wrymium run --release
```

### 5. 加载本地 HTML

```rust
let _webview = WebViewBuilder::new_with_web_context(&mut ctx)
    .with_html(r#"
        <html>
        <body style="background:#1a1a2e;color:#eee;font-family:sans-serif;padding:40px">
            <h1>Hello from CEF!</h1>
            <p>Chromium is rendering this page.</p>
        </body>
        </html>
    "#)
    .build(&window)
    .unwrap();
```

---

## 方式二：替换 Tauri 应用的 WebView

让现有的 Tauri 2.x 应用使用 CEF 渲染，只需 3 步。

### 1. 修改 Cargo.toml

在 Tauri app 的 `src-tauri/Cargo.toml` 中添加 patch：

```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
wry = "0.54"  # 需要显式依赖 wry 以调用 is_cef_subprocess()

[patch.crates-io]
wry = { git = "https://github.com/gxcsoccer/wrymium", package = "wry" }
tauri-runtime-wry = { git = "https://github.com/gxcsoccer/wrymium", path = "tauri-runtime-wry" }
```

> **注意**：需要同时 patch `wry` 和 `tauri-runtime-wry`，因为 tauri-runtime-wry 内部使用了 WebKit/WebView2 的平台原生类型，wrymium 的 fork 版本替换了这些类型。

### 2. 修改 main.rs

在 `main()` 最开头添加 CEF 子进程检查：

```rust
fn main() {
    // 必须是 main() 的第一行代码！
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![/* your commands */])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**为什么需要这行？** CEF 是多进程架构。主进程会用同一个 binary 启动 renderer、GPU、utility 等子进程，通过 `--type=` 命令行参数区分。这行代码检测到子进程参数时，立即进入 CEF 子进程模式而不初始化 Tauri。

### 3. 编译运行

```bash
cargo wrymium run              # debug 模式
cargo wrymium run --release    # 发布模式
```

### 验证

如果一切正常，你的 Tauri 应用会：
- 窗口正常显示，前端正常渲染
- User Agent 包含 `Chrome/146.0.0.0`（而非 Safari/WebKit）
- `window.__TAURI_INTERNALS__` 可用
- `invoke()` 调用 Rust 命令正常返回
- DevTools 可用（如果启用了 devtools feature）

---

## 常见问题

### Q: 首次编译很慢？

首次编译需要下载 CEF（~100MB）并用 CMake 编译 libcef_dll_wrapper。后续编译正常。

提前下载 CEF 可以跳过下载步骤：
```bash
cargo install export-cef-dir
export-cef-dir --force ~/.local/share/cef
export CEF_PATH="$(ls -d ~/.local/share/cef/cef_binary_*_minimal | head -1)"
```

### Q: 报错找不到 CMake 或 Ninja？

```bash
brew install cmake ninja  # macOS
```

### Q: `cargo run` 后白屏或崩溃？

macOS 上 CEF 需要 `.app` bundle 结构。请使用 `cargo wrymium run` 代替 `cargo run`——它会自动完成打包。

### Q: CefInitialize failed (returned 0)？

通常是因为已有一个 CEF 进程在运行（singleton 冲突）。杀掉残留进程后重试：
```bash
pkill -9 -f "your-app-name"
```

### Q: 弹出 Keychain 密码框？

这是 Chromium 请求访问 macOS 钥匙串存储 cookie。wrymium 默认添加了 `--use-mock-keychain` 参数来避免弹窗。如果仍然弹出，可能是旧缓存导致。

### Q: `invoke()` 返回 500 "failed to acquire webview reference"？

确保 Tauri app 的 `src-tauri/Cargo.toml` 中同时 patch 了 `wry` 和 `tauri-runtime-wry`。

### Q: 应用包体积很大？

CEF framework 约 100MB（解压后），这是使用 Chromium 的固有代价。wrymium 的定位是渲染一致性，不是小包体。

---

## 已知限制

| 限制 | 说明 |
|------|------|
| 仅 macOS | Windows 和 Linux 支持在开发中 |
| 包体积 ~100MB | CEF framework 的固有大小 |
| 首次编译慢 | CEF 下载 + CMake 编译 |
| 不能 `cargo run` | 用 `cargo wrymium run` 代替 |
| 部分 WebView API 是 stub | `focus`、`zoom`、`print`、`cookies` 等 |

---

## 下一步

- 查看 [examples/](../examples/) 了解更多用法
- 查看 [项目规范](wrymium-spec.md) 了解架构设计
- 查看 [TODO](TODO.md) 了解开发路线图
