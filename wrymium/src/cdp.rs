//! CDP (Chrome DevTools Protocol) Bridge for wrymium.
//!
//! Provides in-process CDP communication via CEF's DevTools message observer API.
//! Key design: `cdp_dispatch` is synchronous (must run on CEF UI thread),
//! returns a `Receiver` that can be awaited/blocked from any thread.

use std::collections::HashMap;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use cef::*;

// Re-export serde_json for consumers (e.g., tauri-runtime-wry) that need
// to serialize/deserialize CDP parameters without adding their own dependency.
pub use serde_json;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Errors from CDP operations.
#[derive(Debug)]
pub enum CdpError {
    /// CDP method returned an error (success == 0).
    MethodFailed(serde_json::Value),
    /// The CDP call timed out (no response within deadline).
    Timeout,
    /// The DevTools agent detached (browser closing or navigating away).
    AgentDetached,
    /// The response channel was dropped before a result arrived.
    ChannelClosed,
    /// Failed to serialize/deserialize JSON.
    Json(String),
    /// The browser/host is not yet available.
    NotReady,
    /// `send_dev_tools_message` returned an error code.
    SendFailed(c_int),
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdpError::MethodFailed(v) => write!(f, "CDP method failed: {v}"),
            CdpError::Timeout => write!(f, "CDP call timed out"),
            CdpError::AgentDetached => write!(f, "DevTools agent detached"),
            CdpError::ChannelClosed => write!(f, "CDP response channel closed"),
            CdpError::Json(msg) => write!(f, "CDP JSON error: {msg}"),
            CdpError::NotReady => write!(f, "browser not ready"),
            CdpError::SendFailed(code) => write!(f, "send_dev_tools_message failed: {code}"),
        }
    }
}

impl std::error::Error for CdpError {}

/// Result type alias for CDP operations.
pub type CdpResult<T> = std::result::Result<T, CdpError>;

/// A CDP event received from the browser.
#[derive(Debug, Clone)]
pub struct CdpEvent {
    /// The CDP event method name (e.g. "Page.loadEventFired").
    pub method: String,
    /// Raw JSON bytes of the event params.
    pub params: Vec<u8>,
}

impl CdpEvent {
    /// Parse the params as a JSON value.
    pub fn params_json(&self) -> CdpResult<serde_json::Value> {
        if self.params.is_empty() {
            Ok(serde_json::Value::Null)
        } else {
            serde_json::from_slice(&self.params).map_err(|e| CdpError::Json(e.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Shared inner state (between CdpBridge and Observer)
// ---------------------------------------------------------------------------

type PendingSender = mpsc::Sender<CdpResult<serde_json::Value>>;

pub(crate) struct CdpBridgeInner {
    pub(crate) pending: Mutex<HashMap<i32, PendingSender>>,
    pub(crate) event_subscribers: Mutex<Vec<mpsc::Sender<CdpEvent>>>,
}

impl CdpBridgeInner {
    pub(crate) fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            event_subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Complete a pending request with a result.
    pub(crate) fn complete_request(&self, message_id: i32, result: CdpResult<serde_json::Value>) {
        if let Some(sender) = self.pending.lock().unwrap().remove(&message_id) {
            let _ = sender.send(result);
        }
    }

    /// Broadcast a CDP event to all subscribers.
    pub(crate) fn broadcast_event(&self, event: CdpEvent) {
        let mut subs = self.event_subscribers.lock().unwrap();
        // Remove closed channels
        subs.retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Drain all pending requests with an error (e.g. on agent detach).
    pub(crate) fn drain_all_pending(&self, error_fn: impl Fn() -> CdpError) {
        let mut pending = self.pending.lock().unwrap();
        for (_, sender) in pending.drain() {
            let _ = sender.send(Err(error_fn()));
        }
    }
}

// ---------------------------------------------------------------------------
// DevTools Message Observer (CEF callback handler)
// ---------------------------------------------------------------------------

wrap_dev_tools_message_observer! {
    pub(crate) struct WrymiumDevToolsObserver {
        inner: Arc<CdpBridgeInner>,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(
            &self,
            _browser: Option<&mut Browser>,
            _message: Option<&[u8]>,
        ) -> c_int {
            // Return 0 to let CEF also dispatch to on_dev_tools_method_result
            // and on_dev_tools_event. We don't intercept raw messages.
            0
        }

        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: c_int,
            success: c_int,
            result: Option<&[u8]>,
        ) {
            let parsed = match result {
                Some(bytes) if !bytes.is_empty() => {
                    serde_json::from_slice(bytes)
                        .unwrap_or_else(|_| serde_json::Value::Null)
                }
                _ => serde_json::Value::Null,
            };

            if success != 0 {
                self.inner.complete_request(message_id, Ok(parsed));
            } else {
                self.inner
                    .complete_request(message_id, Err(CdpError::MethodFailed(parsed)));
            }
        }

        fn on_dev_tools_event(
            &self,
            _browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            let method_str = match method {
                Some(s) => s.to_string(),
                None => return,
            };
            let params_bytes = params.map(|b| b.to_vec()).unwrap_or_default();

            self.inner.broadcast_event(CdpEvent {
                method: method_str,
                params: params_bytes,
            });
        }

        fn on_dev_tools_agent_attached(&self, _browser: Option<&mut Browser>) {
            wrymium_log!("[wrymium/cdp] DevTools agent attached");
        }

        fn on_dev_tools_agent_detached(&self, _browser: Option<&mut Browser>) {
            wrymium_log!("[wrymium/cdp] DevTools agent detached, draining pending requests");
            self.inner.drain_all_pending(|| CdpError::AgentDetached);
        }
    }
}

// ---------------------------------------------------------------------------
// CdpBridge — public API
// ---------------------------------------------------------------------------

/// CDP Bridge for a single browser instance.
///
/// Created automatically when a browser is created (in `on_after_created`).
/// All `send` / `dispatch` calls must happen on the CEF UI thread.
pub struct CdpBridge {
    next_id: AtomicI32,
    inner: Arc<CdpBridgeInner>,
    // Holding Registration keeps the observer alive.
    // Dropping CdpBridge will unregister the observer.
    _registration: Registration,
}

impl CdpBridge {
    /// Create a new CdpBridge and register a DevTools observer on the given host.
    ///
    /// Must be called on the CEF UI thread (typically in `on_after_created`).
    pub(crate) fn new(host: &BrowserHost) -> Option<Self> {
        let inner = Arc::new(CdpBridgeInner::new());

        let mut observer = WrymiumDevToolsObserver::new(inner.clone());

        let registration =
            ImplBrowserHost::add_dev_tools_message_observer(host, Some(&mut observer))?;

        wrymium_log!("[wrymium/cdp] CdpBridge created, observer registered");

        Some(CdpBridge {
            next_id: AtomicI32::new(1),
            inner,
            _registration: registration,
        })
    }

    /// Dispatch a CDP method call. **Must be called on the CEF UI thread.**
    ///
    /// Returns `(message_id, Receiver)`. The Receiver will yield the result
    /// when the DevTools agent responds. It can be awaited/blocked from any thread.
    pub fn dispatch(
        &self,
        host: &BrowserHost,
        method: &str,
        params: serde_json::Value,
    ) -> CdpResult<(i32, mpsc::Receiver<CdpResult<serde_json::Value>>)> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();

        self.inner.pending.lock().unwrap().insert(id, tx);

        // Build the CDP message as raw JSON bytes.
        // Using send_dev_tools_message (raw JSON) instead of execute_dev_tools_method
        // (DictionaryValue) because it's simpler — no JSON→DictionaryValue conversion.
        // CEF will still trigger on_dev_tools_method_result with our message id.
        let msg = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        let bytes = serde_json::to_vec(&msg).map_err(|e| {
            // Clean up pending entry on serialization failure
            self.inner.pending.lock().unwrap().remove(&id);
            CdpError::Json(e.to_string())
        })?;

        let ret = ImplBrowserHost::send_dev_tools_message(host, Some(&bytes));
        if ret == 0 {
            // send failed — clean up and return error
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(CdpError::SendFailed(ret));
        }

        Ok((id, rx))
    }

    /// Convenience: dispatch + spin-wait with CEF message pump.
    ///
    /// **Must be called on the CEF UI thread** (for the dispatch part).
    /// Between response checks, pumps the CEF message loop via `do_message_loop_work()`
    /// so that DevTools observer callbacks can fire. This avoids the deadlock that
    /// would occur with a naive `recv_timeout()` (dispatch and callback are on the
    /// same thread in `external_message_pump` mode on macOS/Linux).
    pub fn send_blocking(
        &self,
        host: &BrowserHost,
        method: &str,
        params: serde_json::Value,
        timeout: std::time::Duration,
    ) -> CdpResult<serde_json::Value> {
        let (id, rx) = self.dispatch(host, method, params)?;
        let deadline = std::time::Instant::now() + timeout;

        loop {
            match rx.try_recv() {
                Ok(result) => return result,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.inner.pending.lock().unwrap().remove(&id);
                    return Err(CdpError::ChannelClosed);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if std::time::Instant::now() > deadline {
                        self.inner.pending.lock().unwrap().remove(&id);
                        return Err(CdpError::Timeout);
                    }
                    // Pump CEF message loop so observer callbacks can fire.
                    // Safe in external_message_pump mode — only processes CEF's
                    // internal work queue, not OS events.
                    cef::do_message_loop_work();
                    std::thread::yield_now();
                }
            }
        }
    }

    /// Subscribe to CDP events. Returns a Receiver that yields CdpEvent values.
    ///
    /// The subscription is active until the returned Receiver is dropped.
    pub fn subscribe(&self) -> mpsc::Receiver<CdpEvent> {
        let (tx, rx) = mpsc::channel();
        self.inner.event_subscribers.lock().unwrap().push(tx);
        rx
    }

    /// Number of pending (in-flight) CDP requests.
    pub fn pending_count(&self) -> usize {
        self.inner.pending.lock().unwrap().len()
    }

    /// Number of active event subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.inner.event_subscribers.lock().unwrap().len()
    }

    /// Fire-and-forget: enable core CDP domains needed by Browser Use.
    ///
    /// Called during initialization (from on_after_created, which is synchronous).
    /// We dispatch without waiting for responses — the domains will be enabled
    /// by the time the first real CDP call arrives.
    pub(crate) fn enable_core_domains(&self, host: &BrowserHost) {
        for domain in &["Page.enable", "DOM.enable", "Network.enable"] {
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            // Don't insert into pending — we don't care about the response
            let msg = serde_json::json!({
                "id": id,
                "method": domain,
                "params": {},
            });
            if let Ok(bytes) = serde_json::to_vec(&msg) {
                ImplBrowserHost::send_dev_tools_message(host, Some(&bytes));
            }
        }
        wrymium_log!("[wrymium/cdp] Core domains enable dispatched (Page, DOM, Network)");
    }
}

/// Shared handle to a CdpBridge, populated asynchronously in on_after_created.
pub(crate) type SharedCdpBridge = Arc<Mutex<Option<CdpBridge>>>;
