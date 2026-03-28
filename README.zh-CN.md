# wrymium

基于 CEF 的 WebView 后端，为 [Tauri](https://tauri.app) 应用提供跨平台一致的 Chromium 渲染。

## 这是什么？

Tauri 使用系统原生 WebView（macOS 的 WebKit、Windows 的 WebView2、Linux 的 WebKitGTK），这导致不同平台之间存在渲染差异。wrymium 用 [Chromium Embedded Framework (CEF)](https://bitbucket.org/chromiumembedded/cef) 替换原生 WebView，让 Tauri 应用在所有平台上获得一致的 Chromium 渲染引擎。

wrymium 暴露 **wry 兼容的 API**，可以通过 `[patch.crates-io]` 作为 wry 的直接替代品使用。

## 当前状态

**v0.2 Tauri 集成** — macOS，Tauri 2.x 端到端验证通过。

已实现的功能：
- **Tauri 2.x 应用在 CEF 上运行**——通过 `[patch.crates-io]` 替换 wry + tauri-runtime-wry
- `tauri://localhost` 自定义协议正常服务 Tauri 前端资源
- `window.__TAURI_INTERNALS__` 注入并可用
- CEF 初始化 + `external_message_pump`（CFRunLoopTimer 30fps 驱动）
- 浏览器窗口通过 `set_as_child` 嵌入 tao 窗口
- 自定义协议注册（`ipc://`、`tauri://`、`asset://`）+ 异步 `CefResourceHandler`
- `CefSchemeHandlerFactory` 支持异步响应（callback.cont()）
- `window.ipc.postMessage` V8 桥接（渲染进程 → 浏览器进程 IPC）
- 跨进程初始化脚本注入（通过 `extra_info`，含竞态处理）
- wry 兼容的 `WebViewBuilder`（35+ 方法）和 `WebView`（20+ 方法）
- 共享浏览器句柄 `Arc<Mutex<Option<Browser>>>`
- POST body 提取 + 响应 headers 完整传递
- macOS `.app` 打包 + 5 个 Helper 子进程应用
- 34 个单元测试

## 架构

wrymium 是一个**集成层**，构建在 [`tauri-apps/cef-rs`](https://github.com/tauri-apps/cef-rs) 之上：

```
Tauri 应用
  └── tauri 2.x
        └── tauri-runtime-wry（通过 patch 替换）
              └── wrymium（包名 "wry"）     ← 本项目
                    └── cef（crates.io）    ← tauri-apps/cef-rs
                          └── cef-dll-sys  ← CEF C API FFI 绑定
```

## 环境要求

- **Rust**（stable）
- **CMake**（>= 3.x）
- **Ninja**（cef-rs 构建系统硬依赖）
- **C++ 编译器**（macOS: Xcode CLT，Windows: MSVC，Linux: g++/clang++）

```bash
# macOS
brew install cmake ninja

# Linux
sudo apt install cmake ninja-build build-essential

# Windows — 安装 CMake + Ninja + Visual Studio 构建工具
```

## 快速开始

### 1. 下载 CEF（首次）

```bash
cargo install export-cef-dir
export-cef-dir --force ~/.local/share/cef
export CEF_PATH="$(ls -d ~/.local/share/cef/cef_binary_*_minimal)"
```

也可以跳过手动下载，首次 `cargo build` 时会自动下载（约 3-10 分钟）。

### 2. 编译运行示例

```bash
# 设置 CEF_PATH（如果手动下载了的话）
export CEF_PATH="$HOME/.local/share/cef/cef_binary_146.0.6+g68649e2+chromium-146.0.7680.154_macosarm64_minimal"

# 打包并运行飞书文档示例
bash scripts/bundle-macos.sh wrymium-feishu-example "飞书文档"
open target/bundle/wrymium-feishu-example.app
```

### 3. 在自己的项目中使用

```rust
use tao::{event::{Event, WindowEvent}, event_loop::{ControlFlow, EventLoop}, window::WindowBuilder};
use wry::{WebContext, WebViewBuilder};

fn main() {
    // CEF 子进程检查 — 必须放在 main 最开头
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("我的应用")
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

## 示例

| 示例 | 描述 | 运行命令 |
|------|------|---------|
| `basic` | IPC 测试：postMessage + 自定义协议 | `bash scripts/bundle-macos.sh wrymium-basic-example` |
| `feishu` | 打开飞书文档 | `bash scripts/bundle-macos.sh wrymium-feishu-example "飞书文档"` |
| `tauri-app` | 完整 Tauri 2.x 应用在 CEF 上运行 | 见下方 [Tauri 集成](#tauri-集成) |

## Tauri 集成

wrymium 可以通过 `[patch.crates-io]` 替换真实 Tauri 2.x 应用中的 wry：

```toml
# 在 Tauri 应用的 src-tauri/Cargo.toml 中
[dependencies]
tauri = { version = "2", features = ["devtools"] }
wry = "0.54"

[patch.crates-io]
wry = { path = "/path/to/wrymium/wrymium" }
tauri-runtime-wry = { path = "/path/to/wrymium/tauri-runtime-wry" }
```

在 `main()` 最开头添加 CEF 子进程检查：

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

仓库中包含了 patched 版本的 `tauri-runtime-wry`，仅修改了约 30 行代码来处理 CEF 特有的类型差异（NSView 指针、NewWindowResponse 等）。

## 项目结构

```
wrymium/
  ├── Cargo.toml                 # workspace 根
  ├── wrymium/                   # 主库 crate（包名："wry"）
  │   ├── src/
  │   │   ├── lib.rs             # 公开 API 导出
  │   │   ├── webview.rs         # WebView / WebViewBuilder + CefClient
  │   │   ├── cef_init.rs        # CEF 生命周期 + 消息泵
  │   │   ├── scheme.rs          # 自定义 URI 协议处理（异步 CefResourceHandler）
  │   │   ├── renderer.rs        # 渲染进程：V8 桥接 + 脚本注入
  │   │   ├── types.rs           # Rect、DragDropEvent、枚举类型
  │   │   ├── error.rs           # Error 类型（wry 兼容）
  │   │   ├── context.rs         # WebContext
  │   │   ├── tests.rs           # 单元测试（34 个）
  │   │   └── platform/          # 平台扩展 trait
  │   └── Cargo.toml
  ├── tauri-runtime-wry/         # patched fork（基于 2.10.1）
  ├── examples/
  │   ├── basic/                 # IPC 测试示例
  │   ├── feishu/                # 飞书文档查看器
  │   └── tauri-app/             # 完整 Tauri 2.x 应用
  ├── scripts/
  │   └── bundle-macos.sh        # macOS .app 打包脚本
  └── docs/                      # 设计文档、调研、TODO
```

## 工作原理

### CEF 初始化

wrymium 在首次调用 `WebViewBuilder::build()` 时懒加载初始化 CEF：

1. 通过 `LibraryLoader` 加载 CEF framework（macOS 动态加载）
2. `CefExecuteProcess` — 路由子进程执行
3. `CefInitialize`，设置 `external_message_pump = true`
4. 安装 `CFRunLoopTimer`（30fps）驱动 `CefDoMessageLoopWork()`

### IPC 通信

Tauri 2.x 使用双路径 IPC 系统：

- **主路径**：`ipc://localhost` 自定义协议，通过 `fetch()` 发送 — 由 `CefSchemeHandlerFactory` 处理
- **回退路径**：`window.ipc.postMessage` — 由 V8 扩展 + `CefProcessMessage` 处理

### 脚本注入

初始化脚本通过 `extra_info`（`DictionaryValue`）从浏览器进程传递到渲染子进程，在 `OnContextCreated` 中注入。当 `OnContextCreated` 先于 `OnBrowserCreated` 触发时（竞态），wrymium 使用延迟注入机制处理。

## 与 Tauri 官方 CEF 计划的关系

Tauri 团队（CrabNebula）正在内部探索 CEF 集成，但该工作尚未公开，可能成为商业/闭源产品。wrymium 是一个**开源、社区驱动**的替代方案，填补了官方 CEF 支持可能商业化的空白。wrymium 构建在 Tauri 团队维护的开源 `cef-rs` 绑定之上。

## 文档

- [项目规范](docs/wrymium-spec.md) — 完整设计文档
- [wry API 清单](docs/wry-api-surface.md) — tauri-runtime-wry 引用的全部 wry 符号
- [IPC 深入分析](docs/ipc-deep-dive.md) — Tauri 2.x IPC 协议技术分析
- [TODO](docs/TODO.md) — 遗留问题与 Spike 结果

## 许可证

MIT
