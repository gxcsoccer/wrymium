//! WebView and WebViewBuilder — the primary public API.
//!
//! Mirrors wry 0.55's API surface as consumed by tauri-runtime-wry v2.10.1.

use std::borrow::Cow;
use std::path::PathBuf;

use cef::*;
use http::{Request, Response};
use raw_window_handle::HasWindowHandle;

use crate::context::WebContext;
use crate::error::{Error, Result};
use crate::types::{
    BackgroundThrottlingPolicy, Cookie, DragDropEvent, NewWindowFeatures, NewWindowResponse,
    PageLoadEvent, ProxyConfig, Rect, ScrollBarStyle, Theme,
};

/// Async responder for custom protocol handlers.
/// Wraps a FnOnce but exposes a `respond` method for wry API compatibility.
pub struct RequestAsyncResponder(Box<dyn FnOnce(Response<Cow<'static, [u8]>>) + Send>);

impl RequestAsyncResponder {
    pub fn new(f: Box<dyn FnOnce(Response<Cow<'static, [u8]>>) + Send>) -> Self {
        Self(f)
    }

    pub fn respond(self, response: Response<Cow<'static, [u8]>>) {
        (self.0)(response);
    }
}

/// Builder for constructing a `WebView`.
pub struct WebViewBuilder<'a> {
    #[allow(dead_code)]
    pub(crate) web_context: Option<&'a mut WebContext>,
    pub(crate) id: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) html: Option<String>,
    pub(crate) initialization_scripts: Vec<(String, bool)>, // (script, main_only)
    pub(crate) ipc_handler: Option<Box<dyn Fn(Request<String>) + 'static>>,
    pub(crate) custom_protocols:
        Vec<(String, Box<dyn Fn(&str, Request<Vec<u8>>, RequestAsyncResponder) + Send + Sync + 'static>)>,
    pub(crate) devtools: bool,
    pub(crate) transparent: bool,
    pub(crate) focused: bool,
    pub(crate) visible: bool,
    pub(crate) bounds: Option<Rect>,
    pub(crate) background_color: Option<(u8, u8, u8, u8)>,
    pub(crate) user_agent: Option<String>,
    pub(crate) javascript_disabled: bool,
    pub(crate) clipboard: bool,
    pub(crate) hotkeys_zoom: bool,
    pub(crate) incognito: bool,
    pub(crate) accept_first_mouse: bool,
    pub(crate) background_throttling: BackgroundThrottlingPolicy,
    pub(crate) proxy_config: Option<ProxyConfig>,
    pub(crate) navigation_handler: Option<Box<dyn Fn(String) -> bool + 'static>>,
    pub(crate) new_window_req_handler:
        Option<Box<dyn Fn(String, NewWindowFeatures) -> NewWindowResponse + 'static>>,
    pub(crate) document_title_changed_handler: Option<Box<dyn Fn(String) + 'static>>,
    pub(crate) download_started_handler:
        Option<Box<dyn Fn(String, &mut PathBuf) -> bool + 'static>>,
    pub(crate) download_completed_handler:
        Option<Box<dyn Fn(String, Option<PathBuf>, bool) + 'static>>,
    pub(crate) on_page_load_handler: Option<Box<dyn Fn(PageLoadEvent, String) + 'static>>,
    pub(crate) drag_drop_handler: Option<Box<dyn Fn(DragDropEvent) -> bool + 'static>>,
    pub(crate) data_store_identifier: Option<[u8; 16]>,
}

impl<'a> WebViewBuilder<'a> {
    fn default_fields() -> Self {
        Self {
            web_context: None,
            id: None,
            url: None,
            html: None,
            initialization_scripts: Vec::new(),
            ipc_handler: None,
            custom_protocols: Vec::new(),
            devtools: false,
            transparent: false,
            focused: true,
            visible: true,
            bounds: None,
            background_color: None,
            user_agent: None,
            javascript_disabled: false,
            clipboard: false,
            hotkeys_zoom: false,
            incognito: false,
            accept_first_mouse: false,
            background_throttling: BackgroundThrottlingPolicy::default(),
            proxy_config: None,
            navigation_handler: None,
            new_window_req_handler: None,
            document_title_changed_handler: None,
            download_started_handler: None,
            download_completed_handler: None,
            on_page_load_handler: None,
            drag_drop_handler: None,
            data_store_identifier: None,
        }
    }

    /// Create a new builder with the given web context.
    /// This is the constructor used by tauri-runtime-wry.
    pub fn new_with_web_context(web_context: &'a mut WebContext) -> Self {
        Self {
            web_context: Some(web_context),
            ..Self::default_fields()
        }
    }

    /// Create a new builder without a web context.
    pub fn new() -> Self {
        Self::default_fields()
    }

    // --- Content ---

    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.html = Some(html.into());
        self
    }

    // --- Scripts ---

    pub fn with_initialization_script(mut self, js: impl Into<String>) -> Self {
        self.initialization_scripts.push((js.into(), false));
        self
    }

    pub fn with_initialization_script_for_main_only(
        mut self,
        js: impl Into<String>,
        for_main_only: bool,
    ) -> Self {
        self.initialization_scripts.push((js.into(), for_main_only));
        self
    }

    pub fn with_javascript_disabled(mut self) -> Self {
        self.javascript_disabled = true;
        self
    }

    // --- IPC & Protocols ---

    pub fn with_ipc_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(Request<String>) + 'static,
    {
        self.ipc_handler = Some(Box::new(handler));
        self
    }

    pub fn with_custom_protocol<F>(mut self, name: String, handler: F) -> Self
    where
        F: Fn(&str, Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> + Send + Sync + 'static,
    {
        self.custom_protocols.push((
            name,
            Box::new(move |id, request, responder: RequestAsyncResponder| {
                let response = handler(id, request);
                responder.respond(response);
            }),
        ));
        self
    }

    pub fn with_asynchronous_custom_protocol<F>(mut self, name: String, handler: F) -> Self
    where
        F: Fn(&str, Request<Vec<u8>>, RequestAsyncResponder) + Send + Sync + 'static,
    {
        self.custom_protocols.push((name, Box::new(handler)));
        self
    }

    // --- UI ---

    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn with_background_color(mut self, color: (u8, u8, u8, u8)) -> Self {
        self.background_color = Some(color);
        self
    }

    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn with_accept_first_mouse(mut self, accept: bool) -> Self {
        self.accept_first_mouse = accept;
        self
    }

    // --- Behavior ---

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn with_hotkeys_zoom(mut self, enabled: bool) -> Self {
        self.hotkeys_zoom = enabled;
        self
    }

    pub fn with_clipboard(mut self, enabled: bool) -> Self {
        self.clipboard = enabled;
        self
    }

    pub fn with_incognito(mut self, incognito: bool) -> Self {
        self.incognito = incognito;
        self
    }

    pub fn with_background_throttling(mut self, policy: BackgroundThrottlingPolicy) -> Self {
        self.background_throttling = policy;
        self
    }

    pub fn with_proxy_config(mut self, config: ProxyConfig) -> Self {
        self.proxy_config = Some(config);
        self
    }

    // --- Events ---

    pub fn with_navigation_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String) -> bool + 'static,
    {
        self.navigation_handler = Some(Box::new(handler));
        self
    }

    pub fn with_new_window_req_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String, NewWindowFeatures) -> NewWindowResponse + 'static,
    {
        self.new_window_req_handler = Some(Box::new(handler));
        self
    }

    pub fn with_document_title_changed_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String) + 'static,
    {
        self.document_title_changed_handler = Some(Box::new(handler));
        self
    }

    pub fn with_download_started_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String, &mut PathBuf) -> bool + 'static,
    {
        self.download_started_handler = Some(Box::new(handler));
        self
    }

    pub fn with_download_completed_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String, Option<PathBuf>, bool) + 'static,
    {
        self.download_completed_handler = Some(Box::new(handler));
        self
    }

    pub fn with_on_page_load_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(PageLoadEvent, String) + 'static,
    {
        self.on_page_load_handler = Some(Box::new(handler));
        self
    }

    pub fn with_drag_drop_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(DragDropEvent) -> bool + 'static,
    {
        self.drag_drop_handler = Some(Box::new(handler));
        self
    }

    // --- DevTools ---

    pub fn with_devtools(mut self, enabled: bool) -> Self {
        self.devtools = enabled;
        self
    }

    // --- Platform-specific (stubs that compile) ---

    pub fn with_https_scheme(self, _enabled: bool) -> Self {
        self // No-op for CEF
    }

    pub fn with_additional_browser_args(self, _args: &str) -> Self {
        self // No-op — CEF has its own command line handling
    }

    pub fn with_theme(self, _theme: Theme) -> Self {
        self // TODO: Windows theme support
    }

    pub fn with_scroll_bar_style(self, _style: ScrollBarStyle) -> Self {
        self // TODO: Windows scrollbar style
    }

    pub fn with_browser_extensions_enabled(self, _enabled: bool) -> Self {
        self // No-op for CEF
    }

    pub fn with_extensions_path(self, _path: &std::path::Path) -> Self {
        self // No-op for CEF
    }

    pub fn with_data_store_identifier(mut self, id: [u8; 16]) -> Self {
        self.data_store_identifier = Some(id);
        self
    }

    pub fn with_allow_link_preview(self, _enabled: bool) -> Self {
        self // No-op — macOS/iOS only, no CEF equivalent
    }

    // --- Build ---

    /// Build the WebView, embedding it in the provided window.
    pub fn build<W: HasWindowHandle>(self, window: &W) -> Result<WebView> {
        crate::cef_init::ensure_initialized()?;
        WebView::create(self, window, false)
    }

    /// Build the WebView as a child view within the provided window.
    pub fn build_as_child<W: HasWindowHandle>(self, window: &W) -> Result<WebView> {
        crate::cef_init::ensure_initialized()?;
        WebView::create(self, window, true)
    }
}

/// Shared browser handle — populated asynchronously by on_after_created.
pub(crate) type SharedBrowser = std::sync::Arc<std::sync::Mutex<Option<cef::Browser>>>;

/// A WebView backed by CEF.
pub struct WebView {
    id: String,
    url: String,
    #[allow(dead_code)]
    visible: bool,
    pub(crate) browser: SharedBrowser,
    pub(crate) cdp_bridge: crate::cdp::SharedCdpBridge,
}

impl WebView {
    pub(crate) fn create<W: HasWindowHandle>(
        builder: WebViewBuilder,
        window: &W,
        _as_child: bool,
    ) -> Result<Self> {
        let id = builder.id.unwrap_or_else(|| next_webview_id());
        let visible = builder.visible;

        // Determine the URL to load — HTML content uses a data: URI
        let url = if let Some(html) = builder.html {
            // Encode HTML as a data: URI
            let encoded = url::form_urlencoded::byte_serialize(html.as_bytes()).collect::<String>();
            format!("data:text/html,{encoded}")
        } else {
            builder.url.unwrap_or_else(|| "about:blank".to_string())
        };

        // Get the native window handle
        let window_handle = get_native_handle(window)?;

        // Create WindowInfo with set_as_child.
        // On macOS, CEF sets NSViewWidthSizable | NSViewHeightSizable autoresizing masks,
        // so the browser view auto-fills the parent after the first layout.
        // We set the initial bounds to match the parent to avoid a visible resize flash.
        #[cfg(target_os = "macos")]
        let (init_w, init_h) = {
            use objc2_app_kit::NSView;
            use objc2::rc::Retained;
            let ns_view = unsafe { Retained::retain(window_handle as *mut NSView) };
            match ns_view {
                Some(view) => {
                    let frame = view.frame();
                    (frame.size.width as i32, frame.size.height as i32)
                }
                None => (1280, 860),
            }
        };
        #[cfg(not(target_os = "macos"))]
        let (init_w, init_h) = (1280, 860);

        let bounds = cef::Rect {
            x: 0,
            y: 0,
            width: init_w,
            height: init_h,
        };
        let window_info = cef::WindowInfo {
            hidden: if visible { 0 } else { 1 },
            ..Default::default()
        }
        .set_as_child(window_handle, &bounds);

        // Create shared browser handle
        let shared_browser: SharedBrowser =
            std::sync::Arc::new(std::sync::Mutex::new(None));

        // Create shared CDP bridge handle (populated in on_after_created)
        let shared_cdp_bridge: crate::cdp::SharedCdpBridge =
            std::sync::Arc::new(std::sync::Mutex::new(None));

        // Create CefClient with handlers
        let webview_id_for_client = id.clone();
        let mut client = WrymiumClient::new(
            None,
            shared_browser.clone(),
            shared_cdp_bridge.clone(),
            webview_id_for_client,
        );

        // Browser settings
        let browser_settings = BrowserSettings::default();

        // Register custom protocol handlers
        for (name, handler) in builder.custom_protocols {
            let handler: crate::scheme::ProtocolHandler = std::sync::Arc::new(
                move |id: &str, req: http::Request<Vec<u8>>, resp: RequestAsyncResponder| {
                    handler(id, req, resp);
                },
            );
            crate::scheme::register_protocol(&name, "localhost", handler);
        }

        // Pack initialization scripts into extra_info for the renderer process
        let mut extra_info = dictionary_value_create();
        if let Some(ref mut extra) = extra_info {
            if !builder.initialization_scripts.is_empty() {
                let scripts_key = CefString::from("init_scripts");
                if let Some(mut scripts_list) = list_value_create() {
                    ImplListValue::set_size(&mut scripts_list, builder.initialization_scripts.len());
                    for (i, (script, _main_only)) in
                        builder.initialization_scripts.iter().enumerate()
                    {
                        let script_str = CefString::from(script.as_str());
                        ImplListValue::set_string(&mut scripts_list, i, Some(&script_str));
                    }
                    ImplDictionaryValue::set_list(extra, Some(&scripts_key), Some(&mut scripts_list));
                }
            }
        }

        // Create the browser asynchronously
        let cef_url = CefString::from(url.as_str());
        let ret = browser_host_create_browser(
            Some(&window_info),
            Some(&mut client),
            Some(&cef_url),
            Some(&browser_settings),
            extra_info.as_mut(),
            None,
        );

        if ret != 1 {
            return Err(Error::CefError(format!(
                "browser_host_create_browser failed (returned {ret})"
            )));
        }

        wrymium_log!("[wrymium] Browser creation initiated: id={id}, url={url}");

        Ok(WebView {
            id,
            url,
            visible,
            browser: shared_browser,
            cdp_bridge: shared_cdp_bridge,
        })
    }

    // --- Core ---

    pub fn evaluate_script(&self, js: &str) -> Result<()> {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(mut frame) = ImplBrowser::main_frame(browser) {
                let js_str = CefString::from(js);
                let url_str = CefString::from("");
                ImplFrame::execute_java_script(&mut frame, Some(&js_str), Some(&url_str), 0);
            }
        }
        Ok(())
    }

    pub fn evaluate_script_with_callback(
        &self,
        js: &str,
        callback: impl FnOnce(String) + Send + 'static,
    ) -> Result<()> {
        self.evaluate_script(js)?;
        // CEF's ExecuteJavaScript doesn't return a result directly.
        // Call back with empty string to unblock callers.
        // TODO: Implement proper result capture via V8 message passing.
        callback(String::new());
        Ok(())
    }

    pub fn load_url(&self, url: &str) -> Result<()> {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(mut frame) = ImplBrowser::main_frame(browser) {
                let url_str = CefString::from(url);
                ImplFrame::load_url(&mut frame, Some(&url_str));
            }
        }
        Ok(())
    }

    pub fn load_url_with_headers(&self, url: &str, _headers: http::HeaderMap) -> Result<()> {
        self.load_url(url)
    }

    pub fn load_html(&self, html: &str) -> Result<()> {
        let encoded = url::form_urlencoded::byte_serialize(html.as_bytes()).collect::<String>();
        let data_url = format!("data:text/html,{encoded}");
        self.load_url(&data_url)
    }

    pub fn reload(&self) -> Result<()> {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            ImplBrowser::reload(browser);
        }
        Ok(())
    }

    pub fn url(&self) -> Result<String> {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(frame) = ImplBrowser::main_frame(browser) {
                let url = ImplFrame::url(&frame);
                let url_str = CefString::from(&url);
                return Ok(url_str.to_string());
            }
        }
        Ok(self.url.clone())
    }

    // --- Display ---

    pub fn set_visible(&self, visible: bool) -> Result<()> {
        // TODO: Show/hide the CEF browser view
        let _ = visible;
        Ok(())
    }

    pub fn set_bounds(&self, bounds: Rect) -> Result<()> {
        // macOS: no-op (autoresizing mask handles it)
        // Windows: SetWindowPos(browser_hwnd, ...)
        // Linux: XConfigureWindow(...)
        let _ = bounds;
        Ok(())
    }

    pub fn bounds(&self) -> Result<Rect> {
        // TODO: Query the browser view's frame
        Ok(Rect::default())
    }

    pub fn zoom(&self, _scale_factor: f64) -> Result<()> {
        // TODO: browser_host.set_zoom_level(log(scale_factor) / log(1.2))
        Ok(())
    }

    pub fn focus(&self) -> Result<()> {
        // TODO: browser_host.set_focus(true)
        Ok(())
    }

    pub fn focus_parent(&self) -> Result<()> {
        Ok(())
    }

    pub fn set_background_color(&self, _color: (u8, u8, u8, u8)) -> Result<()> {
        Ok(())
    }

    pub fn print(&self) -> Result<()> {
        // TODO: browser_host.print()
        Ok(())
    }

    pub fn set_theme(&self, _theme: Theme) -> Result<()> {
        Ok(())
    }

    // --- Cookies ---

    pub fn cookies(&self) -> Result<Vec<Cookie<'static>>> {
        Ok(Vec::new())
    }

    pub fn cookies_for_url(&self, _url: &str) -> Result<Vec<Cookie<'static>>> {
        Ok(Vec::new())
    }

    pub fn set_cookie(&self, _cookie: &Cookie<'_>) -> Result<()> {
        Ok(())
    }

    pub fn delete_cookie(&self, _cookie: &Cookie<'_>) -> Result<()> {
        Ok(())
    }

    // --- Data ---

    pub fn clear_all_browsing_data(&self) -> Result<()> {
        Ok(())
    }

    // --- DevTools ---

    pub fn open_devtools(&self) {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(host) = ImplBrowser::host(browser) {
                let window_info = cef::WindowInfo::default();
                let settings = BrowserSettings::default();
                ImplBrowserHost::show_dev_tools(&host, Some(&window_info), None, Some(&settings), None);
            }
        }
    }

    pub fn close_devtools(&self) {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(host) = ImplBrowser::host(browser) {
                ImplBrowserHost::close_dev_tools(&host);
            }
        }
    }

    pub fn is_devtools_open(&self) -> bool {
        let guard = self.browser.lock().unwrap();
        if let Some(ref browser) = *guard {
            if let Some(host) = ImplBrowser::host(browser) {
                return ImplBrowserHost::has_dev_tools(&host) != 0;
            }
        }
        false
    }

    // --- CDP (Chrome DevTools Protocol) ---

    /// Dispatch a CDP method call. **Must be called on the CEF UI thread.**
    ///
    /// Returns `(message_id, Receiver)` for the response. The Receiver can be
    /// awaited/blocked from any thread.
    ///
    /// # Example
    /// ```ignore
    /// let (id, rx) = webview.cdp_dispatch("Runtime.evaluate",
    ///     serde_json::json!({"expression": "1+1"}))?;
    /// let result = rx.recv_timeout(Duration::from_secs(5))??;
    /// ```
    pub fn cdp_dispatch(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> crate::cdp::CdpResult<(
        i32,
        std::sync::mpsc::Receiver<crate::cdp::CdpResult<serde_json::Value>>,
    )> {
        let guard_browser = self.browser.lock().unwrap();
        let browser = guard_browser.as_ref().ok_or(crate::cdp::CdpError::NotReady)?;
        let host = ImplBrowser::host(browser).ok_or(crate::cdp::CdpError::NotReady)?;

        let guard_bridge = self.cdp_bridge.lock().unwrap();
        let bridge = guard_bridge.as_ref().ok_or(crate::cdp::CdpError::NotReady)?;

        bridge.dispatch(&host, method, params)
    }

    /// Send a CDP method call and spin-wait until the response arrives.
    /// **Must be called on the CEF UI thread.**
    ///
    /// Pumps the CEF message loop between checks so observer callbacks can fire.
    /// Releases browser/bridge locks before spinning to prevent deadlock with
    /// CEF callbacks (e.g. `on_before_close`) that also lock these mutexes.
    pub fn cdp_send_blocking(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: std::time::Duration,
    ) -> crate::cdp::CdpResult<serde_json::Value> {
        // Phase 1: dispatch (locks held briefly, then released)
        let (_id, rx) = self.cdp_dispatch(method, params)?;

        // Phase 2: spin-wait with message pump (no locks held)
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match rx.try_recv() {
                Ok(result) => return result,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(crate::cdp::CdpError::ChannelClosed);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if std::time::Instant::now() > deadline {
                        // Note: the pending entry in CdpBridgeInner will remain
                        // until the observer eventually tries to send (and fails
                        // because rx is dropped), or until agent detach drains it.
                        return Err(crate::cdp::CdpError::Timeout);
                    }
                    // Pump CEF message loop so observer callbacks can fire.
                    cef::do_message_loop_work();
                    std::thread::yield_now();
                }
            }
        }
    }

    /// Subscribe to CDP events. Returns a Receiver that yields CdpEvent values.
    /// The subscription is active until the Receiver is dropped.
    pub fn cdp_subscribe(&self) -> Option<std::sync::mpsc::Receiver<crate::cdp::CdpEvent>> {
        let guard = self.cdp_bridge.lock().unwrap();
        guard.as_ref().map(|bridge| bridge.subscribe())
    }

    // --- Static methods ---

    pub fn fetch_data_store_identifiers(
        cb: Box<dyn FnOnce(Vec<[u8; 16]>) + Send>,
    ) -> Result<()> {
        cb(Vec::new());
        Ok(())
    }

    pub fn remove_data_store(_uuid: &[u8; 16], cb: impl FnOnce(Result<()>) + Send) {
        cb(Ok(()));
    }

    // --- Internal ---

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Generate a unique ID for webview labels.
fn next_webview_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("wv-{id}")
}

/// Extract the platform-native window handle for CEF embedding.
fn get_native_handle<W: HasWindowHandle>(window: &W) -> Result<cef::sys::cef_window_handle_t> {
    use raw_window_handle::RawWindowHandle;

    let handle = window.window_handle().map_err(Error::from)?;

    match handle.as_raw() {
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(h) => {
            // On macOS, cef_window_handle_t is *mut c_void (NSView pointer)
            Ok(h.ns_view.as_ptr())
        }
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(h) => {
            // On Windows, cef_window_handle_t is HWND
            Ok(h.hwnd.get() as cef::sys::cef_window_handle_t)
        }
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(h) => {
            // On Linux, cef_window_handle_t is X11 Window (unsigned long)
            Ok(h.window as cef::sys::cef_window_handle_t)
        }
        _ => Err(Error::UnsupportedWindowHandle),
    }
}

// --- CefClient implementation ---

wrap_client! {
    pub struct WrymiumClient {
        ipc_handler: Option<std::sync::Arc<dyn Fn(http::Request<String>) + Send + Sync>>,
        shared_browser: SharedBrowser,
        shared_cdp_bridge: crate::cdp::SharedCdpBridge,
        webview_id: String,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(WrymiumLifeSpanHandler::new(
                self.shared_browser.clone(),
                self.shared_cdp_bridge.clone(),
                self.webview_id.clone(),
            ))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(WrymiumDisplayHandler::new())
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            let Some(message) = message else { return 0 };

            let msg_name = ImplProcessMessage::name(message);
            let name_str = CefString::from(&msg_name).to_string();

            if name_str != crate::renderer::IPC_MSG_NAME {
                return 0; // not our message
            }

            // Extract the message body from argument list
            let Some(args_list) = ImplProcessMessage::argument_list(message) else {
                return 0;
            };
            let body_cef = ImplListValue::string(
                &args_list,
                0,
            );
            let body = CefString::from(&body_cef).to_string();

            // Get the frame URL for the request URI
            let url = frame
                .map(|f| {
                    let u = ImplFrame::url(f);
                    CefString::from(&u).to_string()
                })
                .unwrap_or_default();

            // Call the ipc_handler if registered
            if let Some(ref handler) = self.ipc_handler {
                let request = http::Request::builder()
                    .uri(&url)
                    .body(body)
                    .unwrap_or_else(|_| http::Request::new(String::new()));
                handler(request);
            }

            wrymium_log!("[wrymium] IPC postMessage received from renderer");
            1 // handled
        }
    }
}

wrap_life_span_handler! {
    struct WrymiumLifeSpanHandler {
        shared_browser: SharedBrowser,
        shared_cdp_bridge: crate::cdp::SharedCdpBridge,
        webview_id: String,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let browser_id = ImplBrowser::identifier(browser);
                wrymium_log!("[wrymium] Browser created (browser_id={browser_id}, webview_id={})", self.webview_id);
                // Register browser ID → webview ID mapping for scheme handlers
                crate::scheme::register_browser_webview(browser_id, &self.webview_id);
                // Store the browser reference so WebView methods can use it
                let mut guard = self.shared_browser.lock().unwrap();
                *guard = Some(browser.clone());

                // Initialize CDP Bridge: register DevTools observer and enable core domains
                if let Some(host) = ImplBrowser::host(browser) {
                    if let Some(bridge) = crate::cdp::CdpBridge::new(&host) {
                        bridge.enable_core_domains(&host);
                        *self.shared_cdp_bridge.lock().unwrap() = Some(bridge);
                        wrymium_log!("[wrymium] CDP Bridge initialized for browser_id={browser_id}");
                    }
                }
            }
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            wrymium_log!("[wrymium] Browser closing");
            if let Some(browser) = browser {
                let browser_id = ImplBrowser::identifier(browser);
                crate::scheme::unregister_browser_webview(browser_id);

                let mut guard = self.shared_browser.lock().unwrap();
                let is_main = guard
                    .as_ref()
                    .map(|stored| ImplBrowser::identifier(stored) == browser_id)
                    .unwrap_or(false);
                if is_main {
                    *guard = None;
                    drop(guard);
                    quit_message_loop();
                }
            }
        }
    }
}

wrap_display_handler! {
    struct WrymiumDisplayHandler;

    impl DisplayHandler {
    }
}
