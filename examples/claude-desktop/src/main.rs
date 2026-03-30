//! Claude Desktop — A wrymium example replicating the Claude Desktop chat interface.
//!
//! Features:
//! - Two-column layout with sidebar conversation management
//! - Conversation persistence to ~/.claude-desktop/conversations/
//! - Claude Code CLI subprocess with NDJSON streaming
//! - Mode tabs (Chat / Canvas / Browse)
//! - Canvas: artifact preview (HTML/SVG/code)
//! - File browser with directory listing
//! - Browse: secondary WebView for web browsing

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

#[derive(Debug)]
enum AppEvent {
    ClaudeOutput(String),
    ClaudeExited(()),
    Error(String),
    IpcCommand(String),
}

// ---------- Frontend assets ----------

const INDEX_HTML: &str = include_str!("../frontend/index.html");
const STYLE_CSS: &str = include_str!("../frontend/style.css");
const APP_JS: &str = include_str!("../frontend/app.js");
const MARKED_JS: &str = include_str!("../frontend/vendor/marked.min.js");
const HIGHLIGHT_JS: &str = include_str!("../frontend/vendor/highlight.min.js");
const GITHUB_DARK_CSS: &str = include_str!("../frontend/vendor/github-dark.min.css");

fn build_html() -> String {
    INDEX_HTML
        .replacen("/*INJECT:github-dark.min.css*/", GITHUB_DARK_CSS, 1)
        .replacen("/*INJECT:style.css*/", STYLE_CSS, 1)
        .replacen("/*INJECT:marked.min.js*/", MARKED_JS, 1)
        .replacen("/*INJECT:highlight.min.js*/", HIGHLIGHT_JS, 1)
        .replacen("/*INJECT:app.js*/", APP_JS, 1)
}

// ---------- Conversation persistence ----------

fn conversations_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claude-desktop").join("conversations")
}

fn ensure_conversations_dir() {
    let dir = conversations_dir();
    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("[claude-desktop] Failed to create conversations dir: {e}");
        }
    }
}

/// Validate conversation ID: alphanumeric + hyphens only, no path traversal
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() < 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn list_conversations() -> String {
    let dir = conversations_dir();
    let mut convs: Vec<serde_json::Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                        convs.push(serde_json::json!({
                            "id": val.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                            "title": val.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled"),
                            "updated_at": val.get("updated_at").and_then(|v| v.as_str()).unwrap_or(""),
                        }));
                    }
                }
            }
        }
    }
    convs.sort_by(|a, b| {
        let ua = a.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
        let ub = b.get("updated_at").and_then(|v| v.as_str()).unwrap_or("");
        ub.cmp(ua)
    });
    serde_json::to_string(&convs).unwrap_or_else(|_| "[]".to_string())
}

fn load_conversation(id: &str) -> Option<String> {
    if !is_valid_id(id) { return None; }
    let path = conversations_dir().join(format!("{id}.json"));
    std::fs::read_to_string(path).ok()
}

fn save_conversation(id: &str, title: &str, messages: &serde_json::Value) {
    if !is_valid_id(id) { return; }
    let dir = conversations_dir();
    let path = dir.join(format!("{id}.json"));

    let created_at = std::fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str::<serde_json::Value>(&data).ok())
        .and_then(|v| v.get("created_at").and_then(|c| c.as_str()).map(String::from));

    let now = timestamp_now();
    let conv = serde_json::json!({
        "id": id,
        "title": title,
        "messages": messages,
        "created_at": created_at.unwrap_or_else(|| now.clone()),
        "updated_at": now,
    });

    let tmp_path = dir.join(format!("{id}.json.tmp"));
    if let Ok(data) = serde_json::to_string_pretty(&conv) {
        if std::fs::write(&tmp_path, &data).is_ok() {
            let _ = std::fs::rename(&tmp_path, &path);
        }
    }
}

fn delete_conversation(id: &str) {
    if !is_valid_id(id) { return; }
    let path = conversations_dir().join(format!("{id}.json"));
    let _ = std::fs::remove_file(path);
}

fn timestamp_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = secs / 86400;
    let t = secs % 86400;
    let (y, mo, d) = epoch_days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, (t % 3600) / 60, t % 60)
}

fn epoch_days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    loop {
        let yd = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if days < yd { break; }
        days -= yd;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let md: [u64; 12] = if leap { [31,29,31,30,31,30,31,31,30,31,30,31] } else { [31,28,31,30,31,30,31,31,30,31,30,31] };
    let mut mo = 0;
    while mo < 12 && days >= md[mo] { days -= md[mo]; mo += 1; }
    (y, mo as u64 + 1, days + 1)
}

// ---------- Claude CLI subprocess ----------

struct ClaudeProcess {
    child: Child,
    stdin: BufWriter<std::process::ChildStdin>,
}

fn find_claude_binary() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    for path in [
        format!("{home}/.local/bin/claude"),
        format!("{home}/.claude/local/bin/claude"),
        "/usr/local/bin/claude".to_string(),
        "/opt/homebrew/bin/claude".to_string(),
    ] {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    Command::new("claude").arg("--version").output().ok().map(|_| "claude".to_string())
}

fn spawn_claude(
    proxy: tao::event_loop::EventLoopProxy<AppEvent>,
    alive: Arc<AtomicBool>,
) -> Result<ClaudeProcess, String> {
    let binary = find_claude_binary().ok_or("Claude Code CLI not found")?;

    let mut child = Command::new(&binary)
        .args(["-p", "--output-format", "stream-json", "--input-format", "stream-json",
               "--verbose", "--dangerously-skip-permissions"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {e}"))?;

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let proxy_out = proxy.clone();
    let alive_out = alive.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if !alive_out.load(Ordering::Relaxed) { break; }
            match line {
                Ok(l) if !l.is_empty() => { let _ = proxy_out.send_event(AppEvent::ClaudeOutput(l)); }
                Err(_) => break,
                _ => {}
            }
        }
        let _ = proxy_out.send_event(AppEvent::ClaudeExited(()));
    });

    let alive_err = alive;
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            if !alive_err.load(Ordering::Relaxed) { break; }
            if let Ok(l) = line { eprintln!("[claude stderr] {l}"); } else { break; }
        }
    });

    Ok(ClaudeProcess { child, stdin: BufWriter::new(stdin) })
}

impl ClaudeProcess {
    fn send_message(&mut self, text: &str) {
        let msg = serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": text }
        });
        let _ = writeln!(self.stdin, "{}", msg);
        let _ = self.stdin.flush();
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------- JS helpers ----------

/// Escape a JSON string for safe embedding in evaluate_script.
/// Handles </script> injection and JS line separators.
fn js_safe(json: &str) -> String {
    json.replace("</", "<\\/")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Build a JS error event call string.
fn js_error_event(msg: &str) -> String {
    let escaped = serde_json::to_string(msg).unwrap_or_else(|_| "\"error\"".to_string());
    format!(r#"window.__onClaudeEvent({{"type":"error","message":{escaped}}})"#)
}

/// Safely truncate a string at a valid UTF-8 char boundary.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes { return s; }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    &s[..end]
}

// ---------- Main ----------

fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    ensure_conversations_dir();

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Claude")
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 720.0))
        .build(&event_loop)
        .expect("Failed to create window");

    let html = build_html();
    let html_path = std::env::temp_dir().join("claude-desktop-wrymium.html");
    std::fs::write(&html_path, &html).expect("Failed to write temp HTML");
    let html_url = format!("file://{}", html_path.display());

    let mut web_context = WebContext::new(None);
    let ipc_proxy = proxy.clone();

    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(&html_url)
        .with_devtools(true)
        .with_ipc_handler(move |request| {
            let _ = ipc_proxy.send_event(AppEvent::IpcCommand(request.body().to_string()));
        })
        .build(&window)
        .expect("Failed to create WebView");

    let mut browse_webview: Option<wry::WebView> = None;
    let mut browse_web_context = WebContext::new(None);

    let mut alive = Arc::new(AtomicBool::new(true));
    let mut claude = match spawn_claude(proxy.clone(), alive.clone()) {
        Ok(c) => Some(c),
        Err(e) => {
            let p = proxy.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = p.send_event(AppEvent::Error(e));
            });
            None
        }
    };

    let respawn = |alive: &mut Arc<AtomicBool>, claude: &mut Option<ClaudeProcess>, proxy: &tao::event_loop::EventLoopProxy<AppEvent>| {
        alive.store(false, Ordering::Relaxed);
        if let Some(ref mut c) = claude { c.kill(); }
        *alive = Arc::new(AtomicBool::new(true));
        match spawn_claude(proxy.clone(), alive.clone()) {
            Ok(c) => *claude = Some(c),
            Err(e) => { let _ = proxy.send_event(AppEvent::Error(e)); *claude = None; }
        }
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                alive.store(false, Ordering::Relaxed);
                if let Some(ref mut c) = claude { c.kill(); }
                wry::shutdown();
                *control_flow = ControlFlow::Exit;
            }

            Event::UserEvent(ref app_event) => match app_event {
                AppEvent::IpcCommand(ref body) => {
                    let Ok(cmd) = serde_json::from_str::<serde_json::Value>(body) else { return };
                    let cmd_name = cmd.get("cmd").and_then(|c| c.as_str()).unwrap_or("");

                    match cmd_name {
                        "send" => {
                            if let Some(text) = cmd.get("text").and_then(|t| t.as_str()) {
                                if let Some(ref mut c) = claude { c.send_message(text); }
                            }
                        }
                        "new_chat" => respawn(&mut alive, &mut claude, &proxy),
                        "stop" => {
                            respawn(&mut alive, &mut claude, &proxy);
                            let _ = webview.evaluate_script(
                                r#"window.__onClaudeEvent({"type":"result","result":""})"#,
                            );
                        }
                        "list_conversations" => {
                            let json = list_conversations();
                            let _ = webview.evaluate_script(&format!("window.__onConversations({})", json));
                        }
                        "load_conversation" => {
                            if let Some(id) = cmd.get("id").and_then(|i| i.as_str()) {
                                if let Some(data) = load_conversation(id) {
                                    let _ = webview.evaluate_script(
                                        &format!("window.__onConversationLoaded({})", js_safe(&data)),
                                    );
                                }
                            }
                        }
                        "save_conversation" => {
                            if let (Some(id), Some(title)) = (
                                cmd.get("id").and_then(|i| i.as_str()),
                                cmd.get("title").and_then(|t| t.as_str()),
                            ) {
                                let msgs = cmd.get("messages").cloned().unwrap_or(serde_json::json!([]));
                                save_conversation(id, title, &msgs);
                                let id_json = serde_json::to_string(id).unwrap_or_default();
                                let _ = webview.evaluate_script(
                                    &format!(r#"window.__onSaved({{"id":{id_json}}})"#),
                                );
                            }
                        }
                        "delete_conversation" => {
                            if let Some(id) = cmd.get("id").and_then(|i| i.as_str()) {
                                delete_conversation(id);
                                let id_json = serde_json::to_string(id).unwrap_or_default();
                                let _ = webview.evaluate_script(
                                    &format!(r#"window.__onDeleted({{"id":{id_json}}})"#),
                                );
                            }
                        }
                        "get_cwd" => {
                            let cwd = std::env::current_dir().unwrap_or_default().display().to_string();
                            let cwd_json = serde_json::to_string(&cwd).unwrap_or_default();
                            let _ = webview.evaluate_script(&format!("window.__onCwd({cwd_json})"));
                        }
                        "list_files" => {
                            if let Some(path) = cmd.get("path").and_then(|p| p.as_str()) {
                                // Canonicalize to prevent traversal via symlinks
                                let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
                                let mut entries = Vec::new();
                                if let Ok(dir) = std::fs::read_dir(&canon) {
                                    for entry in dir.flatten() {
                                        let name = entry.file_name().to_string_lossy().to_string();
                                        if name.starts_with('.') { continue; }
                                        let meta = entry.metadata().ok();
                                        entries.push(serde_json::json!({
                                            "name": name,
                                            "is_dir": meta.as_ref().map_or(false, |m| m.is_dir()),
                                            "size": meta.as_ref().map_or(0, |m| m.len()),
                                        }));
                                    }
                                }
                                let result = serde_json::json!({ "path": canon.display().to_string(), "entries": entries });
                                let _ = webview.evaluate_script(
                                    &format!("window.__onFiles({})", js_safe(&result.to_string())),
                                );
                            }
                        }
                        "read_file" => {
                            if let Some(path) = cmd.get("path").and_then(|p| p.as_str()) {
                                let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
                                let name = canon.file_name().unwrap_or_default().to_string_lossy().to_string();
                                match std::fs::read_to_string(&canon) {
                                    Ok(content) => {
                                        let truncated = truncate_utf8(&content, 500_000);
                                        let result = serde_json::json!({
                                            "path": canon.display().to_string(), "name": name, "content": truncated,
                                        });
                                        let _ = webview.evaluate_script(
                                            &format!("window.__onFileContent({})", js_safe(&result.to_string())),
                                        );
                                    }
                                    Err(e) => {
                                        let _ = webview.evaluate_script(&js_error_event(&e.to_string()));
                                    }
                                }
                            }
                        }
                        "activate_browse" => {
                            if browse_webview.is_none() {
                                let inner = window.inner_size();
                                let scale = window.scale_factor();
                                let sidebar_w = 260.0;
                                let top_offset = 88.0; // top-bar(48) + browse-toolbar(40)
                                let w = (inner.width as f64 / scale) - sidebar_w;
                                let h = (inner.height as f64 / scale) - top_offset;
                                let bounds = wry::Rect {
                                    position: tao::dpi::Position::Logical(tao::dpi::LogicalPosition::new(sidebar_w, top_offset)),
                                    size: tao::dpi::Size::Logical(tao::dpi::LogicalSize::new(w, h)),
                                };
                                match WebViewBuilder::new_with_web_context(&mut browse_web_context)
                                    .with_url("about:blank")
                                    .with_bounds(bounds)
                                    .with_devtools(true)
                                    .build_as_child(&window)
                                {
                                    Ok(bv) => {
                                        let _ = webview.evaluate_script("window.__onBrowserReady()");
                                        browse_webview = Some(bv);
                                    }
                                    Err(e) => { let _ = webview.evaluate_script(&js_error_event(&e.to_string())); }
                                }
                            } else if let Some(ref bv) = browse_webview {
                                let _ = bv.set_visible(true);
                            }
                        }
                        "deactivate_browse" => {
                            if let Some(ref bv) = browse_webview { let _ = bv.set_visible(false); }
                        }
                        "browser_navigate" => {
                            if let Some(ref bv) = browse_webview {
                                if let Some(url) = cmd.get("url").and_then(|u| u.as_str()) {
                                    let _ = bv.load_url(url);
                                    let url_json = serde_json::to_string(url).unwrap_or_default();
                                    let _ = webview.evaluate_script(
                                        &format!("window.__onBrowserUrlChanged({{\"url\":{url_json}}})"),
                                    );
                                }
                            }
                        }
                        "browser_back" => { if let Some(ref bv) = browse_webview { let _ = bv.evaluate_script("history.back()"); } }
                        "browser_forward" => { if let Some(ref bv) = browse_webview { let _ = bv.evaluate_script("history.forward()"); } }
                        "browser_refresh" => { if let Some(ref bv) = browse_webview { let _ = bv.reload(); } }
                        "browser_screenshot" => {
                            if let Some(ref bv) = browse_webview {
                                match bv.screenshot_fast() {
                                    Ok(bytes) => {
                                        use base64::Engine;
                                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                        let _ = webview.evaluate_script(
                                            &format!("window.__onBrowserScreenshot({{\"image_base64\":\"{b64}\"}})"),
                                        );
                                    }
                                    Err(e) => { let _ = webview.evaluate_script(&js_error_event(&format!("{e:?}"))); }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                AppEvent::ClaudeOutput(ref json) => {
                    let _ = webview.evaluate_script(&format!("window.__onClaudeEvent({})", js_safe(json)));
                }
                AppEvent::ClaudeExited(()) => {}
                AppEvent::Error(ref msg) => { let _ = webview.evaluate_script(&js_error_event(msg)); }
            },

            _ => {}
        }
    });
}
