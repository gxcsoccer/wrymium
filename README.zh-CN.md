# wrymium

基于 CEF 的 WebView 后端，为 [Tauri](https://tauri.app) 应用提供跨平台一致的 Chromium 渲染——后端用 Rust 而非 Node.js。

## 为什么选择 wrymium？

| | Electron | Tauri（原生 WebView） | **wrymium（Tauri + CEF）** |
|---|---|---|---|
| 渲染引擎 | Chromium | WebKit/WebView2/WebKitGTK | **Chromium（一致）** |
| 后端 | Node.js | Rust | **Rust** |
| 跨平台一致性 | 优秀 | 差（3 套引擎） | **优秀** |
| 后端性能 | JavaScript (V8) | 原生 (Rust) | **原生 (Rust)** |
| 安全模型 | Node.js 全权限 | Tauri ACL | **Tauri ACL** |

**wrymium = Electron 的渲染一致性 + Tauri 的 Rust 性能和安全模型。**

### 实测对比（Apple-to-Apple）

相同 HTML 页面、相同 `greet` IPC 命令、macOS、Apple M2 Max：

| 指标 | wrymium | Electron | |
|------|---------|----------|-|
| 包体积 | **257 MB** | 247 MB | 基本持平 |
| 应用 binary | **4.6 MB** | ~49 MB (Node.js) | **小 91%** |
| Main 进程内存 | **198 MB** | 285 MB（3 进程） | **少 30%** |
| 总内存 | 569 MB | 452 MB | CEF 进程隔离更严格 |
| 进程数 | **5** | 7 | 更少 |
| IPC 延迟 | **0.48 ms** | 0.3-0.5 ms | 同一量级 |

完整评测：[docs/benchmark.md](docs/benchmark.md)

## 快速开始

### 安装工具

```bash
# macOS
brew install cmake ninja
cargo install cargo-wrymium --git https://github.com/gxcsoccer/wrymium
```

### 一键运行

```bash
git clone https://github.com/gxcsoccer/wrymium
cd wrymium/examples/feishu
cargo wrymium run
```

`cargo wrymium` 自动完成 编译 → CEF 打包 → 启动。

### 在你的 Tauri 应用中使用

**1. 添加 patch** 到 `src-tauri/Cargo.toml`：

```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
wry = "0.54"

[patch.crates-io]
wry = { git = "https://github.com/gxcsoccer/wrymium", package = "wry" }
tauri-runtime-wry = { git = "https://github.com/gxcsoccer/wrymium", path = "tauri-runtime-wry" }
```

**2. 添加 CEF 子进程检查** 到 `main.rs`：

```rust
fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![/* 你的 commands */])
        .run(tauri::generate_context!())
        .unwrap();
}
```

**3. 编译运行：**

```bash
cargo wrymium run              # debug 模式
cargo wrymium run --release    # 优化模式（LTO + strip）
```

## 示例

| 示例 | 描述 | 命令 |
|------|------|------|
| `basic` | IPC 测试（postMessage + 自定义协议 + 脚本注入） | `cargo wrymium run` |
| `feishu` | 飞书文档查看器 | `cd examples/feishu && cargo wrymium run` |
| `tauri-app` | 完整 Tauri 2.x 应用 + `invoke()` IPC | 见 [使用教程](docs/getting-started.md) |

## 开发工具

```bash
# cargo xtask — 仅在 wrymium 仓库内使用
cargo xtask run wrymium-feishu-example

# cargo wrymium — 在任意 wrymium 项目中使用
cargo wrymium run                  # 自动检测 binary 名称
cargo wrymium run --release        # 优化构建
cargo wrymium bundle --release     # 只打包不启动
```

## 架构

```
Tauri 应用
  └── tauri 2.x
        └── tauri-runtime-wry（patched，~30 行改动）
              └── wrymium（包名 "wry"）     ← 本项目
                    └── cef（crates.io）    ← tauri-apps/cef-rs
                          └── cef-dll-sys  ← CEF C API FFI
```

### 关键实现

- **CEF 消息泵**：`CFRunLoopTimer` 30fps 驱动 `CefDoMessageLoopWork()`，与 tao 事件循环共存
- **IPC**：Tauri 的 `invoke()` 经过 `fetch('ipc://localhost/cmd')` → `CefSchemeHandlerFactory` → 异步 `CefResourceHandler` → Tauri 命令分发（0.48ms 往返）
- **脚本注入**：browser → renderer 通过 `extra_info`（`DictionaryValue`），含竞态延迟注入
- **V8 桥接**：`window.ipc.postMessage` 通过 `CefV8Handler` + `CefProcessMessage` 实现回退路径

## 与 Tauri 官方 CEF 计划的关系

Tauri 团队（CrabNebula）正在内部探索 CEF 集成，但尚未公开，可能成为商业产品。wrymium 是**开源、社区驱动**的替代方案，构建在 Tauri 团队维护的开源 `cef-rs` 绑定之上。

## 文档

- [使用教程](docs/getting-started.md) — 从零开始的 step-by-step 教程
- [性能评测](docs/benchmark.md) — wrymium vs Electron 对比
- [项目规范](docs/wrymium-spec.md) — 架构设计
- [TODO](docs/TODO.md) — 路线图和遗留问题

## 许可证

MIT
