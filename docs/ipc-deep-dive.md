# Tauri 2.x IPC Mechanism Deep Dive & wrymium CEF Design

> Technical analysis for redesigning the IPC bridge in a CEF-backed wry replacement.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [The `ipc://localhost` Custom Protocol (Primary Path)](#2-the-ipclocalhost-custom-protocol-primary-path)
3. [The `window.ipc.postMessage()` Fallback](#3-the-windowipcpostmessage-fallback)
4. [How wry Registers Custom Protocols](#4-how-wry-registers-custom-protocols)
5. [Security Layer](#5-security-layer)
6. [wrymium CEF Design](#6-wrymium-cef-design)

---

## 1. Architecture Overview

Tauri 2.x uses a **dual-path IPC** system. The primary path is a **custom URI scheme protocol** (`ipc://localhost`) that behaves like an HTTP request/response cycle via `fetch()`. The fallback is the legacy **`window.ipc.postMessage()`** bridge. The custom protocol path was introduced in [PR #7170](https://github.com/tauri-apps/tauri/pull/7170) for a massive performance gain: transferring a 150MB file dropped from ~50 seconds to <60ms because binary data no longer needs JSON serialization.

### Data flow (custom protocol path)

```
Frontend JS                      wry (native)                    Tauri Core
─────────────────────────────────────────────────────────────────────────────
invoke("my_cmd", payload)
  → fetch("ipc://localhost/my_cmd",  ──→  custom protocol      ──→  parse_invoke_request()
      { method: POST, body, headers })     handler intercepts         extracts cmd, headers, body
                                                                  ──→  webview.on_message(InvokeRequest)
                                                                  ──→  command handler runs
  ← HTTP Response                    ←──  responder.respond()    ←──  InvokeResponse (JSON or Raw)
     Tauri-Response: ok|error
     Content-Type: application/json | application/octet-stream
```

### Data flow (postMessage fallback)

```
Frontend JS                      wry (native)                    Tauri Core
─────────────────────────────────────────────────────────────────────────────
invoke("my_cmd", payload)
  → window.ipc.postMessage(json)  ──→  ipc_handler callback    ──→  handle_ipc_message()
                                       (platform bridge)              deserialize Message struct
                                                                  ──→  webview.on_message(InvokeRequest)
  ← webview.eval(callback_js)    ←──  eval JS on webview       ←──  format_callback(response)
```

---

## 2. The `ipc://localhost` Custom Protocol (Primary Path)

### 2.1 Frontend: How `invoke()` Constructs Requests

**Source**: `crates/tauri/scripts/core.js`, `crates/tauri/scripts/ipc-protocol.js`, `crates/tauri/scripts/process-ipc-message-fn.js`

#### Step 1: `invoke()` registers callbacks and calls `ipc()`

```javascript
// core.js — defines window.__TAURI_INTERNALS__.invoke
Object.defineProperty(window.__TAURI_INTERNALS__, 'invoke', {
  value: function (cmd, payload = {}, options) {
    return new Promise(function (resolve, reject) {
      const callback = registerCallback((r) => {
        resolve(r); unregisterCallback(error);
      }, true);
      const error = registerCallback((e) => {
        reject(e); unregisterCallback(callback);
      }, true);
      // calls window.__TAURI_INTERNALS__.ipc() which is wired to sendIpcMessage
      window.__TAURI_INTERNALS__.ipc({ cmd, callback, error, payload, options });
    });
  }
});
```

Callbacks are stored in a `Map<u32, Function>` keyed by a random `crypto.getRandomValues(Uint32Array)` ID. These numeric IDs are passed as headers.

#### Step 2: `sendIpcMessage()` builds and sends the fetch request

```javascript
// ipc-protocol.js — the actual transport
function sendIpcMessage(message) {
  const { cmd, callback, error, payload, options } = message;

  if (!customProtocolIpcFailed && canUseCustomProtocol) {
    const { contentType, data } = processIpcMessage(payload);

    const headers = new Headers((options && options.headers) || {});
    headers.set('Content-Type', contentType);
    headers.set('Tauri-Callback', callback);
    headers.set('Tauri-Error', error);
    headers.set('Tauri-Invoke-Key', __TAURI_INVOKE_KEY__);

    fetch(window.__TAURI_INTERNALS__.convertFileSrc(cmd, 'ipc'), {
      method: 'POST',
      body: data,
      headers
    }).then(/* ... handle response ... */);
  }
}
```

#### Step 3: URL construction via `convertFileSrc()`

```javascript
// core.js
convertFileSrc: function (filePath, protocol = 'asset') {
  const path = encodeURIComponent(filePath);
  return osName === 'windows' || osName === 'android'
    ? `${protocolScheme}://${protocol}.localhost/${path}`  // https://ipc.localhost/cmd
    : `${protocol}://localhost/${path}`;                   // ipc://localhost/cmd
}
```

**Platform URL differences**:
| Platform | URL format | Reason |
|----------|-----------|--------|
| macOS/Linux | `ipc://localhost/{cmd}` | WKWebView/WebKitGTK support non-standard scheme natively |
| Windows | `https://ipc.localhost/{cmd}` | WebView2 maps custom schemes to `https://{scheme}.localhost` |
| Android | N/A (postMessage only) | Cannot read custom protocol request bodies |

#### Step 4: `processIpcMessage()` determines content type

```javascript
// process-ipc-message-fn.js
function processIpcMessage(message) {
  if (message instanceof ArrayBuffer || ArrayBuffer.isView(message) || Array.isArray(message)) {
    return { contentType: 'application/octet-stream', data: message };
  } else {
    const data = JSON.stringify(message, (_k, val) => {
      if (val instanceof Map) return Object.fromEntries(val.entries());
      if (val instanceof Uint8Array) return Array.from(val);
      if (val instanceof ArrayBuffer) return Array.from(new Uint8Array(val));
      if (typeof val === 'object' && val !== null && '__TAURI_TO_IPC_KEY__' in val)
        return val['__TAURI_TO_IPC_KEY__']();
      return val;
    });
    return { contentType: 'application/json', data };
  }
}
```

#### Step 5: Response handling

```javascript
.then((response) => {
  const callbackId = response.headers.get('Tauri-Response') === 'ok' ? callback : error;
  switch ((response.headers.get('content-type') || '').split(',')[0]) {
    case 'application/json':  return response.json().then((r) => [callbackId, r]);
    case 'text/plain':        return response.text().then((r) => [callbackId, r]);
    default:                  return response.arrayBuffer().then((r) => [callbackId, r]);
  }
})
.then(([callbackId, data]) => {
  window.__TAURI_INTERNALS__.runCallback(callbackId, data);
})
```

### 2.2 Complete Request/Response Format

#### Request

```
POST ipc://localhost/{percent_encoded_command}
Content-Type: application/json | application/octet-stream
Tauri-Callback: {u32}
Tauri-Error: {u32}
Tauri-Invoke-Key: {runtime_generated_string}
Origin: tauri://localhost

{body: JSON string or raw bytes}
```

#### Response

```
HTTP/1.1 200 OK
Tauri-Response: ok | error
Content-Type: application/json | application/octet-stream
Access-Control-Allow-Origin: *
Access-Control-Expose-Headers: Tauri-Response

{body: JSON string or raw bytes}
```

For CORS preflight:
```
OPTIONS ipc://localhost/{cmd}
→ 200 OK
  Access-Control-Allow-Headers: *
  Access-Control-Allow-Origin: *
```

### 2.3 Backend: Protocol Registration & Request Parsing

**Source**: `crates/tauri/src/manager/webview.rs`, `crates/tauri/src/ipc/protocol.rs`

#### Registration in `prepare_pending_webview()`

```rust
// crates/tauri/src/manager/webview.rs
if !registered_scheme_protocols.contains(&"ipc".into()) {
    let protocol = crate::ipc::protocol::get(manager.manager_owned());
    pending.register_uri_scheme_protocol("ipc", move |webview_id, request, responder| {
        protocol(webview_id, request, UriSchemeResponder(responder))
    });
}
```

This calls `PendingWebview::register_uri_scheme_protocol()`, which stores the handler in a map that wry later consumes via `with_asynchronous_custom_protocol()`.

#### The `get()` function — the core protocol handler

```rust
// crates/tauri/src/ipc/protocol.rs
pub fn get<R: Runtime>(manager: Arc<AppManager<R>>) -> UriSchemeProtocolHandler {
    Box::new(move |label, request, responder| {
        let respond = move |mut response: http::Response<Cow<'static, [u8]>>| {
            response.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
            response.headers_mut().insert(ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static("Tauri-Response"));
            responder.respond(response);
        };

        match *request.method() {
            Method::POST => {
                // parse_invoke_request extracts cmd, headers, body
                // webview.on_message() dispatches to command handler
                // response callback constructs HTTP response with Tauri-Response header
            }
            Method::OPTIONS => { /* CORS preflight */ }
            _ => { /* 405 Method Not Allowed */ }
        }
    })
}
```

#### `parse_invoke_request()` — the parser

```rust
fn parse_invoke_request<R: Runtime>(manager: &AppManager<R>, request: http::Request<Vec<u8>>)
    -> Result<InvokeRequest, String>
{
    let (parts, body) = request.into_parts();

    // Command from URI path (skip leading '/')
    let cmd = percent_encoding::percent_decode(&parts.uri.path().as_bytes()[1..])
        .decode_utf8_lossy().to_string();

    // Content-Type determines body parsing
    let content_type = parts.headers.get(CONTENT_TYPE)...;

    // Required headers
    let invoke_key = parts.headers.get("Tauri-Invoke-Key")...;
    let url = Url::parse(parts.headers.get("Origin")...);
    let callback = CallbackFn(parts.headers.get("Tauri-Callback")...parse::<u32>());
    let error = CallbackFn(parts.headers.get("Tauri-Error")...parse::<u32>());

    // Body: either raw bytes or parsed JSON
    let body = if content_type == APPLICATION_OCTET_STREAM {
        InvokeBody::Raw(body)
    } else if content_type == APPLICATION_JSON {
        InvokeBody::Json(serde_json::from_slice(&body)?)
    };

    Ok(InvokeRequest { cmd, callback, error, url, body, headers, invoke_key })
}
```

### 2.4 Key Types

```rust
pub struct InvokeRequest {
    pub cmd: String,
    pub callback: CallbackFn,       // u32 wrapper
    pub error: CallbackFn,          // u32 wrapper
    pub url: Url,                   // origin URL
    pub body: InvokeBody,           // Json(Value) | Raw(Vec<u8>)
    pub headers: HeaderMap,
    pub invoke_key: String,
}

pub enum InvokeBody { Json(serde_json::Value), Raw(Vec<u8>) }
pub enum InvokeResponseBody { Json(String), Raw(Vec<u8>) }
pub enum InvokeResponse { Ok(InvokeResponseBody), Err(InvokeError) }

// wry's handler signature
type UriSchemeProtocolHandler = dyn Fn(
    &str,                                              // webview label
    http::Request<Vec<u8>>,                           // incoming request
    Box<dyn FnOnce(http::Response<Cow<'static, [u8]>>) + Send>  // async responder
) + Send + Sync + 'static;
```

---

## 3. The `window.ipc.postMessage()` Fallback

### 3.1 When the Fallback is Used

The fallback triggers in two scenarios:
1. **Android**: Always (`canUseCustomProtocol = osName !== 'android'`). Android WebView cannot read custom protocol request bodies.
2. **CSP/protocol failure**: If the initial `fetch()` to `ipc://localhost` fails (blocked by Content Security Policy, webview restrictions, etc.), `customProtocolIpcFailed` is set to `true` and all subsequent calls use postMessage.

```javascript
// From ipc-protocol.js - the catch handler
.then(/*...*/, (e) => {
    console.warn('IPC custom protocol failed, Tauri will now use the postMessage interface instead', e);
    customProtocolIpcFailed = true;
    sendIpcMessage(message);  // retry via postMessage
})
```

### 3.2 Message Format (postMessage path)

When using postMessage, the **entire invoke envelope** is JSON-serialized as the message body:

```javascript
// postMessage path wraps everything into one JSON blob
const { data } = processIpcMessage({
    cmd,
    callback,
    error,
    options: { ...options, customProtocolIpcBlocked: customProtocolIpcFailed },
    payload,
    __TAURI_INVOKE_KEY__
});
window.ipc.postMessage(data);
```

This produces a JSON string like:
```json
{
    "cmd": "plugin:fs|read_file",
    "callback": 482731923,
    "error": 129837412,
    "payload": { "path": "/tmp/file.txt" },
    "options": { "headers": {}, "customProtocolIpcBlocked": true },
    "__TAURI_INVOKE_KEY__": "a8f2k3..."
}
```

### 3.3 Platform Bridge Injection

wry injects the `window.ipc.postMessage` bridge per-platform:

**macOS (WKWebView)**:
```javascript
Object.defineProperty(window, 'ipc', {
    value: Object.freeze({
        postMessage: function(s) {
            window.webkit.messageHandlers.ipc.postMessage(s);
        }
    })
});
```
Uses WKScriptMessageHandler under the hood.

**Windows (WebView2)**:
```javascript
Object.defineProperty(window, 'ipc', {
    value: Object.freeze({
        postMessage: s => window.chrome.webview.postMessage(s)
    })
});
```
Uses `ICoreWebView2::add_WebMessageReceived`.

**Linux (WebKitGTK)**: Similar to macOS, using WebKit's script message handler.

### 3.4 Backend Processing (postMessage path)

```rust
// crates/tauri/src/ipc/protocol.rs
fn handle_ipc_message<R: Runtime>(request: Request<String>, manager: &AppManager<R>, label: &str) {
    #[derive(Deserialize)]
    struct Message {
        cmd: String,
        callback: CallbackFn,
        error: CallbackFn,
        payload: serde_json::Value,
        options: Option<RequestOptions>,
        #[serde(rename = "__TAURI_INVOKE_KEY__")]
        invoke_key: String,
    }

    let message: Message = serde_json::from_str(request.body())?;

    let request = InvokeRequest {
        cmd: message.cmd,
        callback: message.callback,
        error: message.error,
        url: Url::parse(&request.uri().to_string())?,
        body: message.payload.into(),
        headers: options.headers.0,
        invoke_key: message.invoke_key,
    };

    webview.on_message(request, Box::new(move |webview, cmd, response, callback, error| {
        // Response is sent back via webview.eval(javascript) instead of HTTP response
        match response {
            InvokeResponse::Ok(InvokeResponseBody::Json(v)) => {
                // On non-macOS, uses Channel for large JSON objects
                // On macOS, uses eval with format_callback
                webview.eval(format_callback(callback, &v));
            }
            InvokeResponse::Err(e) => {
                webview.eval(format_callback(error, &e));
            }
        }
    }));
}
```

**Key difference from custom protocol path**: Responses go back via `webview.eval(javascript)` rather than as HTTP response bodies. This is why the custom protocol path is dramatically faster for large payloads.

---

## 4. How wry Registers Custom Protocols

### 4.1 wry API Surface

**Source**: `wry/src/lib.rs`

```rust
// Synchronous variant — handler returns immediately
pub fn with_custom_protocol<F>(mut self, name: String, handler: F) -> Self
where F: Fn(WebViewId, Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> + 'static
{
    // Internally wraps into async form
    self.attrs.custom_protocols.insert(name, Box::new(move |id, request, responder| {
        let http_response = handler(id, request);
        responder.respond(http_response);
    }));
    self
}

// Asynchronous variant — handler gets a responder to call later
pub fn with_asynchronous_custom_protocol<F>(mut self, name: String, handler: F) -> Self
where F: Fn(WebViewId, Request<Vec<u8>>, RequestAsyncResponder) + 'static
{
    self.attrs.custom_protocols.insert(name, Box::new(handler));
    self
}

// IPC handler (for postMessage)
pub fn with_ipc_handler<F>(mut self, handler: F) -> Self
where F: Fn(Request<String>) + 'static
{
    self.attrs.ipc_handler = Some(Box::new(handler));
    self
}
```

### 4.2 Which Does Tauri Use?

Tauri uses **`register_uri_scheme_protocol()`** on `PendingWebview`, which maps to `with_asynchronous_custom_protocol()` internally. The asynchronous variant is required because command handlers may be async — the responder callback is stored and invoked later when the command completes.

Tauri also sets `ipc_handler` via:
```rust
pending.ipc_handler = Some(crate::ipc::protocol::message_handler(manager.manager_owned()));
```

So **both** paths are always registered.

### 4.3 Platform Implementation Details

#### macOS (WKWebView)

Custom protocols are registered via `WKWebViewConfiguration.setURLSchemeHandler_forURLScheme()`:

```rust
// wry/src/wkwebview/mod.rs
for (name, function) in attributes.custom_protocols {
    let url_scheme_handler_cls = url_scheme_handler::create(&name);
    let handler: *mut AnyObject = msg_send![url_scheme_handler_cls, new];
    protocol_ptrs.push(Rc::from(function));
    config.setURLSchemeHandler_forURLScheme(
        Some(&*(handler.cast::<ProtocolObject<dyn WKURLSchemeHandler>>())),
        &NSString::from_str(&name),
    );
}
```

The `WKURLSchemeHandler` protocol methods (`webView:startURLSchemeTask:` and `webView:stopURLSchemeTask:`) receive requests and call back with responses.

#### Windows (WebView2)

Custom protocols use `AddWebResourceRequestedFilter` with a workaround URI:

```rust
// wry/src/webview2/mod.rs
// WebView2 converts custom schemes to: https://{scheme}.localhost/
let filter = format!("https://{name}.localhost/*");
webview.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
```

The `WebResourceRequested` event handler intercepts matching requests and provides responses via a deferral mechanism.

#### Linux (WebKitGTK)

Uses `webkit_web_context_register_uri_scheme()` which is similar to macOS but through the GTK WebKit API.

### 4.4 Type Signatures Summary

```
Request type:  http::Request<Vec<u8>>     — standard http crate Request with byte body
Response type: http::Response<Cow<'static, [u8]>>  — Response with owned or static bytes
Responder:     Box<dyn FnOnce(Response) + Send>    — one-shot async callback
WebViewId:     &str                       — webview label string
```

---

## 5. Security Layer

### 5.1 Invoke Key

A runtime-generated cryptographic key (`invoke_key`) is created at app startup and injected into the JS context via initialization scripts. Every IPC request must include this key in the `Tauri-Invoke-Key` header (custom protocol) or `__TAURI_INVOKE_KEY__` field (postMessage). The key is validated server-side:

```rust
// crates/tauri/src/webview/mod.rs
pub fn on_message(self, request: InvokeRequest, responder: Box<OwnedInvokeResponder<R>>) {
    let expected = manager.invoke_key();
    if request.invoke_key != expected {
        return;  // silently drop the request
    }
    // ...
}
```

The key is declared outside `window.__TAURI_INTERNALS__` to prevent leakage via `.toString()`.

### 5.2 Capability-Based ACL

Tauri 2.x uses a `RuntimeAuthority` for command authorization:

```rust
pub fn resolve_access(
    &self,
    command: &str,    // e.g. "plugin:fs|read_file"
    window: &str,     // window label
    webview: &str,    // webview label
    origin: &Origin,  // Local or Remote { url }
) -> Option<Vec<ResolvedCommand>>
```

**Resolution order**:
1. Check if command is in the **deny** list. If matched, return `None` immediately (deny takes precedence).
2. Check **allow** list, filtering by execution context (Local vs Remote URL pattern), window label, and webview label.
3. Commands must match against capability definitions in `tauri.conf.json` or plugin manifests.

**Scope enforcement** happens within command handlers via `CommandScope<T>`, which provides allow/deny scope lists that commands check at runtime.

### 5.3 Origin Validation

The `Origin` is extracted from the request's `Origin` header (custom protocol path) or from the request URI (postMessage path):

```rust
pub enum Origin {
    Local,                    // tauri://localhost
    Remote { url: Url },      // any external URL
}

fn matches(&self, context: &ExecutionContext) -> bool {
    match (self, context) {
        (Self::Local, ExecutionContext::Local) => true,
        (Self::Remote { url }, ExecutionContext::Remote { url_pattern }) => {
            url_pattern.test(url)  // glob matching
        }
        _ => false,
    }
}
```

### 5.4 CORS Headers

The custom protocol handler adds:
```
Access-Control-Allow-Origin: *
Access-Control-Expose-Headers: Tauri-Response
```
The OPTIONS handler adds `Access-Control-Allow-Headers: *`.

### 5.5 Isolation Pattern (Optional)

When the `isolation` feature is enabled, IPC payloads are encrypted with AES-GCM in an isolated iframe before reaching the main context:

```rust
struct RawIsolationPayload {
    nonce: [u8; 12],
    payload: Vec<u8>,        // encrypted
    contentType: String,     // original MIME type
}
```

---

## 6. wrymium CEF Design

### 6.1 Registering `ipc://localhost` as a CEF Custom Scheme

CEF requires custom scheme registration in two places:

#### A. Scheme Registration (all processes)

```rust
// Must be called in CefApp::OnRegisterCustomSchemes for ALL process types
// (browser, renderer, gpu, utility)
impl CefApp for WrymiumApp {
    fn on_register_custom_schemes(&self, registrar: &mut CefSchemeRegistrar) {
        // CEF_SCHEME_OPTION_STANDARD: enables standard URL parsing
        // CEF_SCHEME_OPTION_CORS_ENABLED: allows cross-origin requests to this scheme
        // CEF_SCHEME_OPTION_FETCH_ENABLED: allows fetch() to this scheme
        registrar.add_custom_scheme("ipc",
            CEF_SCHEME_OPTION_STANDARD |
            CEF_SCHEME_OPTION_CORS_ENABLED |
            CEF_SCHEME_OPTION_FETCH_ENABLED
        );

        registrar.add_custom_scheme("tauri",
            CEF_SCHEME_OPTION_STANDARD |
            CEF_SCHEME_OPTION_CORS_ENABLED |
            CEF_SCHEME_OPTION_FETCH_ENABLED |
            CEF_SCHEME_OPTION_CSP_BYPASSING  // tauri:// serves app assets
        );

        registrar.add_custom_scheme("asset",
            CEF_SCHEME_OPTION_STANDARD |
            CEF_SCHEME_OPTION_CORS_ENABLED |
            CEF_SCHEME_OPTION_FETCH_ENABLED
        );
    }
}
```

**Critical**: `CEF_SCHEME_OPTION_FETCH_ENABLED` is required for the `fetch()` IPC path to work. Without it, the browser will block fetch requests to `ipc://localhost`.

#### B. Scheme Handler Factory Registration (browser process only)

```rust
// Called in CefBrowserProcessHandler::OnContextInitialized
fn on_context_initialized(&self) {
    // Register IPC handler
    cef_register_scheme_handler_factory(
        "ipc",
        "localhost",
        Box::new(IpcSchemeHandlerFactory::new(self.ipc_handler.clone()))
    );

    // Register asset handler
    cef_register_scheme_handler_factory(
        "tauri",
        "localhost",
        Box::new(TauriSchemeHandlerFactory::new(self.protocol_handlers.clone()))
    );
}
```

### 6.2 Implementing the IPC CefResourceHandler

The `CefSchemeHandlerFactory` creates a `CefResourceHandler` for each request. The handler must implement the full request/response lifecycle:

```rust
struct IpcResourceHandler {
    // Shared reference to the Rust IPC dispatch function
    ipc_dispatcher: Arc<dyn Fn(&str, http::Request<Vec<u8>>,
        Box<dyn FnOnce(http::Response<Cow<'static, [u8]>>) + Send>) + Send + Sync>,

    // Response state (populated asynchronously)
    response: Arc<Mutex<Option<http::Response<Vec<u8>>>>>,
    response_ready: Arc<AtomicBool>,
    bytes_read: usize,
    callback: Option<CefCallback>,  // for async continuation
}

impl CefResourceHandler for IpcResourceHandler {
    /// Called on the IO thread. Return true if the request will be handled.
    fn open(&self, request: &CefRequest, handle_request: &mut bool,
            callback: &CefCallback) -> bool {
        // Extract method, URI, headers, body from CefRequest
        let method = request.get_method();  // "POST"
        let url = request.get_url();        // "ipc://localhost/my_cmd"
        let headers = request.get_header_map();
        let body = request.get_post_data();  // CefPostData → Vec<u8>

        // Build http::Request
        let mut builder = http::Request::builder()
            .method(method.as_str())
            .uri(url.as_str());
        for (k, v) in headers.iter() {
            builder = builder.header(k, v);
        }
        let http_request = builder.body(body).unwrap();

        // Dispatch asynchronously
        let response_slot = self.response.clone();
        let ready_flag = self.response_ready.clone();
        let callback_clone = callback.clone();

        (self.ipc_dispatcher)("webview-label", http_request,
            Box::new(move |http_response| {
                let (parts, body) = http_response.into_parts();
                let response = http::Response::from_parts(parts, body.to_vec());
                *response_slot.lock().unwrap() = Some(response);
                ready_flag.store(true, Ordering::SeqCst);
                callback_clone.cont();  // signal CEF that headers are ready
            })
        );

        *handle_request = false;  // false = async, will call callback.cont() later
        true
    }

    /// Called after open() completes (callback.cont() was called)
    fn get_response_headers(&self, response: &mut CefResponse,
                            response_length: &mut i64, redirect_url: &mut CefString) {
        let resp = self.response.lock().unwrap();
        let resp = resp.as_ref().unwrap();

        response.set_status(resp.status().as_u16() as i32);
        if let Some(ct) = resp.headers().get("content-type") {
            response.set_mime_type(ct.to_str().unwrap_or("application/octet-stream"));
        }

        // Copy custom headers (Tauri-Response, Access-Control-*)
        let mut header_map = CefStringMultimap::new();
        for (name, value) in resp.headers().iter() {
            header_map.append(name.as_str(), value.to_str().unwrap_or(""));
        }
        response.set_header_map(&header_map);

        *response_length = resp.body().len() as i64;
    }

    /// Called to read response body. May be called multiple times.
    fn read(&self, data_out: &mut [u8], bytes_read: &mut usize,
            callback: &CefCallback) -> bool {
        let resp = self.response.lock().unwrap();
        let body = resp.as_ref().unwrap().body();
        let remaining = body.len() - self.bytes_read;

        if remaining == 0 {
            *bytes_read = 0;
            return false;  // EOF
        }

        let to_copy = remaining.min(data_out.len());
        data_out[..to_copy].copy_from_slice(&body[self.bytes_read..self.bytes_read + to_copy]);
        self.bytes_read += to_copy;
        *bytes_read = to_copy;
        true
    }
}
```

### 6.3 Supporting the `postMessage` Fallback via V8 Extension

CEF's multi-process model means the renderer process (V8) and browser process are separate. The `window.ipc.postMessage` bridge needs a cross-process messaging channel.

#### Option A: CefV8 Extension + Process Messaging (Recommended)

```rust
// Registered in CefRenderProcessHandler::OnWebKitInitialized
const IPC_EXTENSION: &str = r#"
(function() {
    native function __wrymium_ipc_send__(message);

    Object.defineProperty(window, 'ipc', {
        value: Object.freeze({
            postMessage: function(s) {
                __wrymium_ipc_send__(s);
            }
        })
    });
})();
"#;

impl CefRenderProcessHandler for WrymiumRendererHandler {
    fn on_webkit_initialized(&self) {
        cef_register_extension("v8/wrymium_ipc", IPC_EXTENSION,
            Box::new(IpcV8Handler));
    }
}

impl CefV8Handler for IpcV8Handler {
    fn execute(&self, name: &str, _object: &CefV8Value,
               arguments: &[CefV8Value], _retval: &mut CefV8Value,
               _exception: &mut CefString) -> bool {
        if name == "__wrymium_ipc_send__" {
            let message_str = arguments[0].get_string_value();

            // Send from renderer → browser process via CefProcessMessage
            let browser = CefV8Context::get_current_context().get_browser();
            let msg = CefProcessMessage::create("ipc_message");
            msg.get_argument_list().set_string(0, &message_str);
            browser.get_main_frame().send_process_message(PID_BROWSER, msg);

            return true;
        }
        false
    }
}
```

On the browser process side:

```rust
impl CefClient for WrymiumClient {
    fn on_process_message_received(&self, browser: &CefBrowser,
                                    frame: &CefFrame,
                                    source_process: CefProcessId,
                                    message: &CefProcessMessage) -> bool {
        if message.get_name() == "ipc_message" {
            let body = message.get_argument_list().get_string(0);
            let url = frame.get_url();

            // Build Request<String> matching wry's ipc_handler signature
            let request = http::Request::builder()
                .uri(url.as_str())
                .body(body.to_string())
                .unwrap();

            // Dispatch to the registered ipc_handler
            (self.ipc_handler)(request);
            return true;
        }
        false
    }
}
```

#### Option B: CefMessageRouter (Simpler but less control)

CEF provides `CefMessageRouterBrowserSide` / `CefMessageRouterRendererSide` as a built-in query/response mechanism. However, it uses a different API (`window.cefQuery()`) that would require patching Tauri's JS, so Option A is preferred.

### 6.4 Sending Responses Back (postMessage path)

For the postMessage fallback, responses are sent back by evaluating JavaScript on the main frame:

```rust
// Browser process — after command handler completes
fn send_ipc_response(browser: &CefBrowser, js: &str) {
    browser.get_main_frame().execute_javascript(js, "", 0);
}
```

This maps to wry's `webview.eval()` which Tauri uses for postMessage responses.

### 6.5 CEF-Specific Challenges

#### Challenge 1: Cross-Process Scheme Handling

**Problem**: CEF's `CefSchemeHandlerFactory` and `CefResourceHandler` run in the **browser process** on the IO thread. The `fetch()` from JavaScript in the **renderer process** automatically routes through CEF's network layer to the browser process. This is transparent for the custom protocol path.

**Solution**: No special handling needed. CEF's architecture naturally handles this. When JS calls `fetch("ipc://localhost/cmd")`, CEF's network stack intercepts it in the browser process where our `CefSchemeHandlerFactory` is registered.

#### Challenge 2: POST Body Access

**Problem**: `CefRequest::GetPostData()` may return null for certain request types or if the body is streaming.

**Solution**: For the IPC use case, bodies are always fully buffered (they come from `fetch()` with an explicit body). Use `CefPostData::GetElements()` to read all `CefPostDataElement` entries and concatenate them.

```rust
fn extract_post_body(request: &CefRequest) -> Vec<u8> {
    let post_data = match request.get_post_data() {
        Some(pd) => pd,
        None => return Vec::new(),
    };
    let mut body = Vec::new();
    for element in post_data.get_elements() {
        match element.get_type() {
            PDE_TYPE_BYTES => {
                let mut buf = vec![0u8; element.get_bytes_count()];
                element.get_bytes(buf.len(), &mut buf);
                body.extend_from_slice(&buf);
            }
            PDE_TYPE_FILE => { /* unlikely for IPC, but handle gracefully */ }
            _ => {}
        }
    }
    body
}
```

#### Challenge 3: V8 Extension vs. Initialization Scripts

**Problem**: wry injects `window.ipc` via initialization scripts that run at document-start. In CEF, V8 extensions run even earlier (at context creation), but `CefRenderProcessHandler::OnContextCreated` is the proper place for per-context JS injection.

**Solution**: Use `OnContextCreated` for the `window.ipc` bridge and inject Tauri's initialization scripts there too:

```rust
impl CefRenderProcessHandler for WrymiumRendererHandler {
    fn on_context_created(&self, browser: &CefBrowser, frame: &CefFrame,
                          context: &CefV8Context) {
        // Inject window.ipc bridge
        frame.execute_javascript(IPC_BRIDGE_JS, "", 0);

        // Inject Tauri initialization scripts
        for script in &self.initialization_scripts {
            frame.execute_javascript(script, "", 0);
        }
    }
}
```

#### Challenge 4: Thread Safety

**Problem**: CEF's `CefResourceHandler` methods are called on the IO thread. The IPC dispatcher (command handlers) may run on arbitrary threads. The async responder must post the response back to the IO thread.

**Solution**: Use CEF's `CefCallback` mechanism. The `open()` method stores the callback, dispatches work off-thread, and calls `callback.cont()` when the response is ready. `get_response_headers()` and `read()` are then called sequentially on the IO thread.

#### Challenge 5: Renderer Process Sandbox

**Problem**: CEF's renderer sandbox restricts what the renderer process can do. V8 extensions and process messages work within the sandbox, but direct file access or network calls from the renderer are blocked.

**Solution**: This is actually a non-issue for IPC. All heavy lifting happens in the browser process. The renderer only needs to: (1) call `fetch()` to `ipc://localhost` (handled by CEF's network layer), and (2) send `CefProcessMessage` for the postMessage fallback. Both work within the sandbox.

#### Challenge 6: Multiple WebViews

**Problem**: Tauri supports multiple webviews, each with a label. The scheme handler must route to the correct webview.

**Solution**: CEF's `CefSchemeHandlerFactory::Create()` receives the `CefBrowser` and `CefFrame`, which can be mapped to a webview label via a lookup table maintained in the browser process. The `Origin` header also helps identify the source.

### 6.6 Architecture Summary

```
┌─────────────────────────────────────────────────────────────┐
│  Renderer Process                                            │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  V8 Context                                              │ │
│  │  ├── window.__TAURI_INTERNALS__.invoke()  (core.js)     │ │
│  │  ├── sendIpcMessage()                     (ipc-protocol)│ │
│  │  │   ├── fetch("ipc://localhost/cmd")     ← primary     │ │
│  │  │   └── window.ipc.postMessage(json)     ← fallback    │ │
│  │  └── window.ipc (V8 extension bridge)                   │ │
│  └──────────┬───────────────────────────┬──────────────────┘ │
│             │ fetch (network layer)     │ CefProcessMessage   │
└─────────────┼───────────────────────────┼────────────────────┘
              │                           │
    ┌─────────▼───────────┐    ┌──────────▼──────────┐
    │  IO Thread          │    │  UI/Main Thread      │
    │  CefResourceHandler │    │  OnProcessMessage    │
    │  ├── open()         │    │  ├── deserialize     │
    │  ├── getHeaders()   │    │  └── dispatch to     │
    │  └── read()         │    │      ipc_handler     │
    └─────────┬───────────┘    └──────────┬──────────┘
              │                           │
    ┌─────────▼───────────────────────────▼──────────┐
    │  Tauri Command Dispatch                         │
    │  ├── validate invoke_key                        │
    │  ├── ACL check (RuntimeAuthority)               │
    │  ├── parse cmd → plugin:name|command            │
    │  └── execute handler → InvokeResponse           │
    │       ├── Ok(Json)  → HTTP 200 / eval(callback) │
    │       ├── Ok(Raw)   → HTTP 200 / Channel        │
    │       └── Err       → HTTP 200 + Tauri-Response │
    └─────────────────────────────────────────────────┘
```

### 6.7 wrymium API Mapping to wry

| wry API | wrymium CEF Implementation |
|---------|---------------------------|
| `with_custom_protocol(name, handler)` | `CefRegisterSchemeHandlerFactory(name, "localhost", factory)` where factory wraps `handler` |
| `with_asynchronous_custom_protocol(name, handler)` | Same, but `CefResourceHandler::open()` uses callback for async |
| `with_ipc_handler(handler)` | V8 extension + `CefProcessMessage` → browser process → `handler` |
| `with_initialization_script(js)` | `CefRenderProcessHandler::OnContextCreated` → `frame.execute_javascript()` |
| `evaluate_script(js)` | `browser.get_main_frame().execute_javascript()` |

### 6.8 Implementation Priority

1. **Phase 1**: Register `ipc` and `tauri` custom schemes with `CEF_SCHEME_OPTION_FETCH_ENABLED`. Implement `CefSchemeHandlerFactory` + `CefResourceHandler` for the custom protocol IPC path. This covers the primary (fast) path.

2. **Phase 2**: Implement V8 extension for `window.ipc.postMessage` fallback via `CefProcessMessage`. Wire `OnProcessMessage` to dispatch to the ipc_handler.

3. **Phase 3**: Inject Tauri initialization scripts (`core.js`, `ipc-protocol.js`, etc.) via `OnContextCreated`. Ensure `__TAURI_INTERNALS__` is set up before any app JS runs.

4. **Phase 4**: Response channels — for the postMessage path, implement `webview.eval()` response delivery. For the custom protocol path, responses flow naturally through `CefResourceHandler`.

---

## References

- [Tauri IPC protocol.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/ipc/protocol.rs) — core IPC handler
- [Tauri ipc-protocol.js](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/scripts/ipc-protocol.js) — frontend IPC transport
- [Tauri core.js](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/scripts/core.js) — invoke() and convertFileSrc()
- [Tauri manager/webview.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/manager/webview.rs) — protocol registration
- [wry lib.rs](https://github.com/tauri-apps/wry/blob/dev/src/lib.rs) — with_custom_protocol, with_ipc_handler
- [wry wkwebview/mod.rs](https://github.com/tauri-apps/wry/blob/dev/src/wkwebview/mod.rs) — macOS implementation
- [wry webview2/mod.rs](https://github.com/tauri-apps/wry/blob/dev/src/webview2/mod.rs) — Windows implementation
- [PR #7170](https://github.com/tauri-apps/tauri/pull/7170) — IPC refactor to URI schemes
- [CEF General Usage](https://chromiumembedded.github.io/cef/general_usage.html) — scheme registration docs
- [CEF scheme_handler example](https://github.com/chromiumembedded/cef-project/tree/master/examples/scheme_handler) — reference implementation
- [PR #13227](https://github.com/tauri-apps/tauri/commit/f888502fd228ad96b105e1e66f01c20c9f109983) — Headers fix in sendIpcMessage
