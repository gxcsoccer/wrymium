//! Browser Use primitives for wrymium.
//!
//! High-level browser automation API built on top of the CDP Bridge (Phase 1)
//! and CEF's native input APIs. All methods are synchronous and must be called
//! on the CEF UI thread. Phase 3 Tauri commands wrap these with async dispatch.

use std::os::raw::c_int;
use std::time::Duration;

use cef::*;

use crate::cdp::{CdpError, CdpResult};
use crate::webview::WebView;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default timeout for CDP operations.
const CDP_TIMEOUT: Duration = Duration::from_secs(30);

/// Short timeout for quick operations (evaluate, DOM query).
const CDP_TIMEOUT_SHORT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Options for taking a screenshot.
#[derive(Debug, Clone, Default)]
pub struct ScreenshotOptions {
    /// Image format: "png" (default) or "jpeg".
    pub format: Option<String>,
    /// JPEG quality (0-100). Only used when format is "jpeg".
    pub quality: Option<u32>,
    /// Clip region. If None, captures the full visible viewport.
    pub clip: Option<ClipRect>,
}

/// A rectangular clip region for screenshots.
#[derive(Debug, Clone)]
pub struct ClipRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A page element found by CSS selector.
#[derive(Debug, Clone)]
pub struct Element {
    /// CDP node ID (valid for the current DOM session).
    pub node_id: i64,
    /// Bounding box in viewport coordinates (CSS pixels).
    pub bounds: Option<ElementBounds>,
    /// The CSS selector used to find this element.
    pub selector: String,
}

/// Bounding box of an element in viewport coordinates (CSS pixels).
#[derive(Debug, Clone)]
pub struct ElementBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Information about a frame in the page.
#[derive(Debug, Clone)]
pub struct FrameInfo {
    pub id: String,
    pub url: String,
    pub name: String,
    pub is_main: bool,
}

/// An annotated screenshot with element labels.
#[derive(Debug)]
pub struct AnnotatedScreenshot {
    /// PNG image bytes (with overlay annotations baked in).
    pub image: Vec<u8>,
    /// List of annotated interactive elements.
    pub elements: Vec<AnnotatedElement>,
}

/// An interactive element found during annotation.
#[derive(Debug, Clone)]
pub struct AnnotatedElement {
    /// Numeric label shown on the screenshot (1, 2, 3...).
    pub label: u32,
    /// Accessibility role (e.g. "button", "link", "textbox").
    pub role: String,
    /// Accessible name / visible text.
    pub name: String,
    /// CSS selector to target this element.
    pub selector: String,
    /// Bounding box in viewport coordinates.
    pub bounds: ElementBounds,
}

/// Keyboard keys for press_key / key_combo.
#[derive(Debug, Clone, Copy)]
pub enum Key {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    /// A specific character key (ASCII only for native key events).
    Char(char),
}

impl Key {
    /// Windows virtual key code for this key.
    pub(crate) fn windows_key_code(self) -> c_int {
        match self {
            Key::Enter => 0x0D,
            Key::Tab => 0x09,
            Key::Escape => 0x1B,
            Key::Backspace => 0x08,
            Key::Delete => 0x2E,
            Key::ArrowUp => 0x26,
            Key::ArrowDown => 0x28,
            Key::ArrowLeft => 0x25,
            Key::ArrowRight => 0x27,
            Key::Home => 0x24,
            Key::End => 0x23,
            Key::PageUp => 0x21,
            Key::PageDown => 0x22,
            Key::Space => 0x20,
            Key::Char(c) => {
                // For ASCII letters, use uppercase VK code
                if c.is_ascii_alphabetic() {
                    c.to_ascii_uppercase() as c_int
                } else {
                    c as c_int
                }
            }
        }
    }

    /// Character value for CHAR key events (0 for non-printable keys).
    pub(crate) fn char_value(self) -> u16 {
        match self {
            Key::Enter => '\r' as u16,
            Key::Tab => '\t' as u16,
            Key::Backspace => 0x08,
            Key::Space => ' ' as u16,
            Key::Char(c) => c as u16,
            _ => 0,
        }
    }
}

/// Keyboard modifier flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool, // Cmd on macOS, Win on Windows
}

impl Modifiers {
    /// Convert to CEF modifier bit flags (from cef_event_flags_t).
    pub(crate) fn to_cef_flags(self) -> u32 {
        let mut flags = 0u32;
        if self.shift {
            flags |= 1 << 1; // EVENTFLAG_SHIFT_DOWN = 2
        }
        if self.ctrl {
            flags |= 1 << 2; // EVENTFLAG_CONTROL_DOWN = 4
        }
        if self.alt {
            flags |= 1 << 3; // EVENTFLAG_ALT_DOWN = 8
        }
        if self.meta {
            flags |= 1 << 7; // EVENTFLAG_COMMAND_DOWN = 128
        }
        flags
    }
}

/// A cookie for get/set operations.
#[derive(Debug, Clone)]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
}

// ---------------------------------------------------------------------------
// Helper: CDP call with default timeout
// ---------------------------------------------------------------------------

impl WebView {
    /// Internal helper: CDP send with default timeout.
    fn cdp(&self, method: &str, params: serde_json::Value) -> CdpResult<serde_json::Value> {
        self.cdp_send_blocking(method, params, CDP_TIMEOUT)
    }

    /// Internal helper: CDP send with short timeout.
    fn cdp_quick(&self, method: &str, params: serde_json::Value) -> CdpResult<serde_json::Value> {
        self.cdp_send_blocking(method, params, CDP_TIMEOUT_SHORT)
    }
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

impl WebView {
    /// Navigate to a URL. If `wait` is true, waits for load event.
    pub fn navigate(&self, url: &str, wait: bool) -> CdpResult<()> {
        self.cdp("Page.navigate", serde_json::json!({ "url": url }))?;
        if wait {
            self.wait_for_navigation(CDP_TIMEOUT)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Page perception: screenshot, A11y tree, evaluate
// ---------------------------------------------------------------------------

impl WebView {
    /// Take a screenshot. Returns PNG (or JPEG) image bytes.
    pub fn screenshot(&self, opts: &ScreenshotOptions) -> CdpResult<Vec<u8>> {
        let mut params = serde_json::json!({});

        if let Some(ref fmt) = opts.format {
            params["format"] = serde_json::json!(fmt);
        }
        if let Some(q) = opts.quality {
            params["quality"] = serde_json::json!(q);
        }
        if let Some(ref clip) = opts.clip {
            params["clip"] = serde_json::json!({
                "x": clip.x,
                "y": clip.y,
                "width": clip.width,
                "height": clip.height,
                "scale": 1.0,
            });
        }

        let result = self.cdp("Page.captureScreenshot", params)?;

        // Result contains base64-encoded image data
        let data_b64 = result["data"]
            .as_str()
            .ok_or_else(|| CdpError::Json("missing 'data' in screenshot response".into()))?;

        // Decode base64
        base64_decode(data_b64)
    }

    /// Get the accessibility tree of the current page.
    /// Returns the raw CDP response as JSON (caller can parse as needed).
    pub fn accessibility_tree(&self) -> CdpResult<serde_json::Value> {
        self.cdp("Accessibility.getFullAXTree", serde_json::json!({}))
    }

    /// Execute JavaScript and return the result.
    /// Resolves the current `evaluate_script` TODO (no return value).
    pub fn evaluate(&self, expression: &str) -> CdpResult<serde_json::Value> {
        let result = self.cdp_quick(
            "Runtime.evaluate",
            serde_json::json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            }),
        )?;

        // Check for exceptions
        if result.get("exceptionDetails").is_some() {
            return Err(CdpError::MethodFailed(result));
        }

        // Return the value from result.result.value
        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

// ---------------------------------------------------------------------------
// Element location
// ---------------------------------------------------------------------------

impl WebView {
    /// Find a single element by CSS selector.
    pub fn find_element(&self, selector: &str) -> CdpResult<Element> {
        // Get document root
        let doc = self.cdp_quick("DOM.getDocument", serde_json::json!({}))?;
        let root_id = doc["root"]["nodeId"]
            .as_i64()
            .ok_or_else(|| CdpError::Json("missing root nodeId".into()))?;

        // Query selector
        let result = self.cdp_quick(
            "DOM.querySelector",
            serde_json::json!({
                "nodeId": root_id,
                "selector": selector,
            }),
        )?;

        let node_id = result["nodeId"]
            .as_i64()
            .ok_or_else(|| CdpError::Json("element not found".into()))?;

        if node_id == 0 {
            return Err(CdpError::Json(format!("no element matches '{selector}'")));
        }

        // Get bounding box
        let bounds = self.get_element_bounds(node_id).ok();

        Ok(Element {
            node_id,
            bounds,
            selector: selector.to_string(),
        })
    }

    /// Find all elements matching a CSS selector.
    pub fn find_elements(&self, selector: &str) -> CdpResult<Vec<Element>> {
        let doc = self.cdp_quick("DOM.getDocument", serde_json::json!({}))?;
        let root_id = doc["root"]["nodeId"]
            .as_i64()
            .ok_or_else(|| CdpError::Json("missing root nodeId".into()))?;

        let result = self.cdp_quick(
            "DOM.querySelectorAll",
            serde_json::json!({
                "nodeId": root_id,
                "selector": selector,
            }),
        )?;

        let node_ids = result["nodeIds"]
            .as_array()
            .ok_or_else(|| CdpError::Json("missing nodeIds".into()))?;

        let mut elements = Vec::new();
        for nid in node_ids {
            if let Some(id) = nid.as_i64() {
                let bounds = self.get_element_bounds(id).ok();
                elements.push(Element {
                    node_id: id,
                    bounds,
                    selector: selector.to_string(),
                });
            }
        }
        Ok(elements)
    }

    /// List all frames in the page.
    pub fn list_frames(&self) -> CdpResult<Vec<FrameInfo>> {
        let result = self.cdp_quick("Page.getFrameTree", serde_json::json!({}))?;
        let mut frames = Vec::new();
        collect_frames(&result["frameTree"], &mut frames);
        Ok(frames)
    }

    /// Get bounding box for a DOM node (in viewport coordinates, per CEF's DOM.getBoxModel).
    fn get_element_bounds(&self, node_id: i64) -> CdpResult<ElementBounds> {
        let result = self.cdp_quick(
            "DOM.getBoxModel",
            serde_json::json!({ "nodeId": node_id }),
        )?;

        // content quad: [x1,y1, x2,y2, x3,y3, x4,y4]
        // For non-rotated elements: top-left, top-right, bottom-right, bottom-left
        let content = result["model"]["content"]
            .as_array()
            .ok_or_else(|| CdpError::Json("missing box model content".into()))?;

        if content.len() < 8 {
            return Err(CdpError::Json("incomplete box model quad".into()));
        }

        let x1 = content[0].as_f64().unwrap_or(0.0);
        let y1 = content[1].as_f64().unwrap_or(0.0);
        let x3 = content[4].as_f64().unwrap_or(0.0); // bottom-right x
        let y3 = content[5].as_f64().unwrap_or(0.0); // bottom-right y

        Ok(ElementBounds {
            x: x1,
            y: y1,
            width: x3 - x1,
            height: y3 - y1,
        })
    }
}

/// Recursively collect FrameInfo from a Page.getFrameTree response.
fn collect_frames(tree: &serde_json::Value, out: &mut Vec<FrameInfo>) {
    if let Some(frame) = tree.get("frame") {
        out.push(FrameInfo {
            id: frame["id"].as_str().unwrap_or("").to_string(),
            url: frame["url"].as_str().unwrap_or("").to_string(),
            name: frame["name"].as_str().unwrap_or("").to_string(),
            is_main: frame["parentId"].is_null(),
        });
    }
    if let Some(children) = tree.get("childFrames").and_then(|c| c.as_array()) {
        for child in children {
            collect_frames(child, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Input: click, type, key, scroll
// ---------------------------------------------------------------------------

impl WebView {
    /// Low-level click at viewport coordinates (CSS pixels).
    pub fn click(&self, x: i32, y: i32) -> crate::Result<()> {
        let guard = self.browser.lock().unwrap();
        let browser = guard.as_ref().ok_or(crate::Error::CefError("browser not ready".into()))?;
        let host = ImplBrowser::host(browser)
            .ok_or(crate::Error::CefError("host not available".into()))?;

        let mouse_event = MouseEvent { x, y, modifiers: 0 };

        // mousedown
        ImplBrowserHost::send_mouse_click_event(
            &host,
            Some(&mouse_event),
            MouseButtonType::LEFT,
            0, // mouse_up = false (press)
            1, // click_count
        );
        // mouseup
        ImplBrowserHost::send_mouse_click_event(
            &host,
            Some(&mouse_event),
            MouseButtonType::LEFT,
            1, // mouse_up = true (release)
            1,
        );

        Ok(())
    }

    /// High-level click on an element by CSS selector.
    ///
    /// Uses `DOM.getBoxModel` to get element bounds. In CEF's implementation,
    /// the content quad is returned in **viewport coordinates** (already adjusted
    /// for scroll offset), so no additional scroll compensation is needed.
    pub fn click_element(&self, selector: &str) -> CdpResult<()> {
        let element = self.find_element(selector)?;
        let bounds = element
            .bounds
            .ok_or_else(|| CdpError::Json("element has no bounds".into()))?;

        // Element center — already in viewport coordinates from DOM.getBoxModel
        let center_x = (bounds.x + bounds.width / 2.0) as i32;
        let center_y = (bounds.y + bounds.height / 2.0) as i32;

        self.click(center_x, center_y)
            .map_err(|e| CdpError::Json(format!("click failed: {e}")))
    }

    /// Type text into the currently focused element.
    ///
    /// Uses JS `el.value` + `dispatchEvent` for text input (supports all characters
    /// including CJK and emoji). Falls back to native key events if `native_keys` is true.
    pub fn type_text(&self, text: &str) -> CdpResult<()> {
        // Detect element type and set value via JS
        let js = format!(
            r#"(() => {{
                const el = document.activeElement;
                if (!el) return 'no_focus';
                if (el.isContentEditable) {{
                    document.execCommand('insertText', false, {text});
                    return 'contenteditable';
                }}
                if ('value' in el) {{
                    el.value = {text};
                    el.dispatchEvent(new Event('input', {{bubbles: true}}));
                    el.dispatchEvent(new Event('change', {{bubbles: true}}));
                    return 'value';
                }}
                return 'unknown';
            }})()"#,
            text = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string()),
        );

        let result = self.evaluate(&js)?;
        let result_type = result.as_str().unwrap_or("unknown");

        if result_type == "no_focus" {
            return Err(CdpError::Json(
                "type_text: no element is focused (click an input first)".into(),
            ));
        }

        Ok(())
    }

    /// Press a special key (Enter, Tab, Escape, arrows, etc.).
    pub fn press_key(&self, key: Key) -> crate::Result<()> {
        let guard = self.browser.lock().unwrap();
        let browser = guard.as_ref().ok_or(crate::Error::CefError("browser not ready".into()))?;
        let host = ImplBrowser::host(browser)
            .ok_or(crate::Error::CefError("host not available".into()))?;

        send_key_to_host(&host, key, Modifiers::default());
        Ok(())
    }

    /// Press a key combination (e.g., Ctrl+A, Ctrl+C).
    pub fn key_combo(&self, modifiers: Modifiers, key: Key) -> crate::Result<()> {
        let guard = self.browser.lock().unwrap();
        let browser = guard.as_ref().ok_or(crate::Error::CefError("browser not ready".into()))?;
        let host = ImplBrowser::host(browser)
            .ok_or(crate::Error::CefError("host not available".into()))?;

        send_key_to_host(&host, key, modifiers);
        Ok(())
    }

    /// Scroll at the given viewport position.
    pub fn scroll(&self, x: i32, y: i32, delta_x: i32, delta_y: i32) -> crate::Result<()> {
        let guard = self.browser.lock().unwrap();
        let browser = guard.as_ref().ok_or(crate::Error::CefError("browser not ready".into()))?;
        let host = ImplBrowser::host(browser)
            .ok_or(crate::Error::CefError("host not available".into()))?;

        let mouse_event = MouseEvent {
            x,
            y,
            modifiers: 0,
        };
        ImplBrowserHost::send_mouse_wheel_event(&host, Some(&mouse_event), delta_x, delta_y);

        Ok(())
    }
}

/// Send a key event sequence (RAWKEYDOWN → CHAR → KEYUP) to a BrowserHost.
fn send_key_to_host(host: &BrowserHost, key: Key, modifiers: Modifiers) {
    let flags = modifiers.to_cef_flags();
    let vk = key.windows_key_code();
    let char_val = key.char_value();

    // RAWKEYDOWN
    let event_down = KeyEvent {
        type_: KeyEventType::RAWKEYDOWN,
        modifiers: flags,
        windows_key_code: vk,
        character: char_val,
        unmodified_character: char_val,
        ..Default::default()
    };
    ImplBrowserHost::send_key_event(host, Some(&event_down));

    // CHAR (only for printable characters)
    if char_val != 0 {
        let event_char = KeyEvent {
            type_: KeyEventType::CHAR,
            modifiers: flags,
            windows_key_code: char_val as c_int,
            character: char_val,
            unmodified_character: char_val,
            ..Default::default()
        };
        ImplBrowserHost::send_key_event(host, Some(&event_char));
    }

    // KEYUP
    let event_up = KeyEvent {
        type_: KeyEventType::KEYUP,
        modifiers: flags,
        windows_key_code: vk,
        character: char_val,
        unmodified_character: char_val,
        ..Default::default()
    };
    ImplBrowserHost::send_key_event(host, Some(&event_up));
}

// ---------------------------------------------------------------------------
// Wait primitives
// ---------------------------------------------------------------------------

impl WebView {
    /// Wait for a navigation to complete (Page.loadEventFired).
    pub fn wait_for_navigation(&self, timeout: Duration) -> CdpResult<()> {
        let rx = self
            .cdp_subscribe()
            .ok_or(CdpError::NotReady)?;

        let deadline = std::time::Instant::now() + timeout;
        loop {
            match rx.try_recv() {
                Ok(event) if event.method == "Page.loadEventFired" => return Ok(()),
                Ok(_) => {} // other event, keep waiting
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(CdpError::ChannelClosed);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if std::time::Instant::now() > deadline {
                        return Err(CdpError::Timeout);
                    }
                    cef::do_message_loop_work();
                    std::thread::yield_now();
                }
            }
        }
    }

    /// Wait for an element matching `selector` to appear in the DOM.
    pub fn wait_for_selector(&self, selector: &str, timeout: Duration) -> CdpResult<Element> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.find_element(selector) {
                Ok(el) => return Ok(el),
                Err(_) => {
                    if std::time::Instant::now() > deadline {
                        return Err(CdpError::Timeout);
                    }
                    cef::do_message_loop_work();
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }

    /// Wait for network to become idle (no in-flight requests for `idle_ms`).
    pub fn wait_for_network_idle(&self, idle_ms: u64, timeout: Duration) -> CdpResult<()> {
        // Use a JS-based approach: inject a PerformanceObserver that tracks
        // in-flight fetches. Simpler and more reliable than tracking CDP
        // Network events for this use case.
        let js = format!(
            r#"new Promise((resolve) => {{
                let timer = null;
                const reset = () => {{
                    clearTimeout(timer);
                    timer = setTimeout(resolve, {idle_ms});
                }};
                const observer = new PerformanceObserver((list) => reset());
                observer.observe({{ type: 'resource', buffered: false }});
                reset();
            }})"#,
        );

        // evaluate with awaitPromise = true
        let result = self.cdp_send_blocking(
            "Runtime.evaluate",
            serde_json::json!({
                "expression": js,
                "awaitPromise": true,
                "returnByValue": true,
            }),
            timeout,
        )?;

        if result.get("exceptionDetails").is_some() {
            return Err(CdpError::MethodFailed(result));
        }
        Ok(())
    }

    /// Wait for DOM to stabilize (no mutations for `stable_ms`).
    pub fn wait_for_dom_stable(&self, stable_ms: u64, timeout: Duration) -> CdpResult<()> {
        let js = format!(
            r#"new Promise((resolve) => {{
                let timer = null;
                const reset = () => {{
                    clearTimeout(timer);
                    timer = setTimeout(resolve, {stable_ms});
                }};
                const observer = new MutationObserver(() => reset());
                observer.observe(document.body, {{
                    childList: true, subtree: true,
                    attributes: true, characterData: true,
                }});
                reset();
            }})"#,
        );

        let result = self.cdp_send_blocking(
            "Runtime.evaluate",
            serde_json::json!({
                "expression": js,
                "awaitPromise": true,
                "returnByValue": true,
            }),
            timeout,
        )?;

        if result.get("exceptionDetails").is_some() {
            return Err(CdpError::MethodFailed(result));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Cookies
// ---------------------------------------------------------------------------

impl WebView {
    /// Get cookies, optionally filtered by URLs.
    pub fn get_cookies(&self, urls: Option<&[&str]>) -> CdpResult<Vec<BrowserCookie>> {
        let mut params = serde_json::json!({});
        if let Some(urls) = urls {
            params["urls"] = serde_json::json!(urls);
        }

        let result = self.cdp("Network.getCookies", params)?;

        let cookies = result["cookies"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|c| BrowserCookie {
                        name: c["name"].as_str().unwrap_or("").to_string(),
                        value: c["value"].as_str().unwrap_or("").to_string(),
                        domain: c["domain"].as_str().unwrap_or("").to_string(),
                        path: c["path"].as_str().unwrap_or("/").to_string(),
                        secure: c["secure"].as_bool().unwrap_or(false),
                        http_only: c["httpOnly"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(cookies)
    }

    /// Set a cookie via CDP.
    pub fn cdp_set_cookie(&self, cookie: &BrowserCookie) -> CdpResult<()> {
        self.cdp(
            "Network.setCookie",
            serde_json::json!({
                "name": cookie.name,
                "value": cookie.value,
                "domain": cookie.domain,
                "path": cookie.path,
                "secure": cookie.secure,
                "httpOnly": cookie.http_only,
            }),
        )?;
        Ok(())
    }

    /// Clear all browser cookies.
    pub fn clear_cookies(&self) -> CdpResult<()> {
        self.cdp("Network.clearBrowserCookies", serde_json::json!({}))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Annotated screenshot
// ---------------------------------------------------------------------------

/// JavaScript to inject annotation overlays on interactive elements.
const ANNOTATE_JS: &str = r#"(() => {
    const selectors = 'a, button, input, select, textarea, [role="button"], [role="link"], [onclick], [tabindex]';
    const els = document.querySelectorAll(selectors);
    const results = [];
    let label = 1;

    els.forEach(el => {
        const rect = el.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) return;
        if (window.getComputedStyle(el).visibility === 'hidden') return;

        // Create overlay div
        const overlay = document.createElement('div');
        overlay.className = '__wrymium_annotate__';
        overlay.style.cssText = `
            position: fixed; z-index: 2147483647;
            left: ${rect.left}px; top: ${rect.top}px;
            width: ${rect.width}px; height: ${rect.height}px;
            border: 2px solid red; background: rgba(255,0,0,0.1);
            pointer-events: none; box-sizing: border-box;
        `;
        // Label badge
        const badge = document.createElement('span');
        badge.style.cssText = `
            position: absolute; top: -10px; left: -10px;
            background: red; color: white; font-size: 11px;
            padding: 1px 4px; border-radius: 8px;
            font-family: monospace; font-weight: bold;
        `;
        badge.textContent = label;
        overlay.appendChild(badge);
        document.body.appendChild(overlay);

        results.push({
            label: label,
            role: el.getAttribute('role') || el.tagName.toLowerCase(),
            name: el.getAttribute('aria-label') || el.innerText?.slice(0, 50) || '',
            selector: buildSelector(el),
            bounds: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
        });
        label++;
    });

    function buildSelector(el) {
        if (el.id) return '#' + el.id;
        let sel = el.tagName.toLowerCase();
        if (el.className && typeof el.className === 'string') {
            sel += '.' + el.className.trim().split(/\s+/).join('.');
        }
        return sel;
    }

    return JSON.stringify(results);
})()"#;

/// JavaScript to remove all annotation overlays.
const ANNOTATE_CLEANUP_JS: &str =
    "document.querySelectorAll('.__wrymium_annotate__').forEach(el => el.remove())";

impl WebView {
    /// Take an annotated screenshot: overlay interactive elements with labels,
    /// capture, then clean up.
    pub fn annotate_screenshot(&self) -> CdpResult<AnnotatedScreenshot> {
        // 1. Inject overlays and collect element info
        let elements_json = self.evaluate(ANNOTATE_JS)?;
        let elements_str = elements_json
            .as_str()
            .ok_or_else(|| CdpError::Json("annotate JS returned non-string".into()))?;

        let raw_elements: Vec<serde_json::Value> =
            serde_json::from_str(elements_str).map_err(|e| CdpError::Json(e.to_string()))?;

        // 2. Take screenshot (with overlays visible)
        let image = self.screenshot(&ScreenshotOptions::default())?;

        // 3. Clean up overlays
        let _ = self.evaluate(ANNOTATE_CLEANUP_JS);

        // 4. Parse elements
        let elements = raw_elements
            .into_iter()
            .filter_map(|e| {
                Some(AnnotatedElement {
                    label: e["label"].as_u64()? as u32,
                    role: e["role"].as_str()?.to_string(),
                    name: e["name"].as_str().unwrap_or("").to_string(),
                    selector: e["selector"].as_str()?.to_string(),
                    bounds: ElementBounds {
                        x: e["bounds"]["x"].as_f64()?,
                        y: e["bounds"]["y"].as_f64()?,
                        width: e["bounds"]["width"].as_f64()?,
                        height: e["bounds"]["height"].as_f64()?,
                    },
                })
            })
            .collect();

        Ok(AnnotatedScreenshot { image, elements })
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Simple base64 decoder (avoids adding a base64 crate dependency).
/// Supports standard base64 alphabet (A-Z, a-z, 0-9, +, /) with = padding.
fn base64_decode(input: &str) -> CdpResult<Vec<u8>> {
    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let input = input.as_bytes();
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &byte in input {
        if byte == b'=' || byte == b'\n' || byte == b'\r' {
            continue;
        }
        let val = decode_char(byte)
            .ok_or_else(|| CdpError::Json(format!("invalid base64 char: {}", byte as char)))?;
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}
