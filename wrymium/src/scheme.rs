//! Custom URI scheme handler for ipc://, tauri://, and asset:// protocols.
//!
//! CEF scheme handling has two phases:
//! 1. Scheme registration in CefApp::OnRegisterCustomSchemes (all processes)
//! 2. Handler factory registration via register_scheme_handler_factory (browser process)

use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cef::*;

use crate::webview::RequestAsyncResponder;

// Scheme option flags (from cef_scheme_options_t)
const SCHEME_STANDARD: i32 = 1;
const SCHEME_CORS_ENABLED: i32 = 16;
const SCHEME_CSP_BYPASSING: i32 = 32;
const SCHEME_FETCH_ENABLED: i32 = 64;

/// Register custom schemes. Called from CefApp::OnRegisterCustomSchemes.
/// Must be called in ALL process types (browser, renderer, GPU, utility).
pub fn register_custom_schemes(registrar: &mut SchemeRegistrar) {
    let ipc_options = SCHEME_STANDARD | SCHEME_CORS_ENABLED | SCHEME_FETCH_ENABLED;
    let tauri_options =
        SCHEME_STANDARD | SCHEME_CORS_ENABLED | SCHEME_FETCH_ENABLED | SCHEME_CSP_BYPASSING;
    let asset_options = SCHEME_STANDARD | SCHEME_CORS_ENABLED | SCHEME_FETCH_ENABLED;

    let ipc_name = CefString::from("ipc");
    let tauri_name = CefString::from("tauri");
    let asset_name = CefString::from("asset");

    registrar.add_custom_scheme(Some(&ipc_name), ipc_options);
    registrar.add_custom_scheme(Some(&tauri_name), tauri_options);
    registrar.add_custom_scheme(Some(&asset_name), asset_options);

    eprintln!("[wrymium] Registered custom schemes: ipc, tauri, asset");
}

/// Protocol handler type — wraps the user-provided async handler function.
pub type ProtocolHandler = Arc<
    dyn Fn(&str, http::Request<Vec<u8>>, RequestAsyncResponder) + Send + Sync + 'static,
>;

/// Register a scheme handler factory for the given scheme + domain.
/// If domain is "localhost" or empty, registers for all domains of that scheme.
pub fn register_protocol(scheme: &str, domain: &str, handler: ProtocolHandler) {
    let scheme_str = CefString::from(scheme);
    let mut factory = WrymiumSchemeHandlerFactory::new(handler);

    // Register with empty domain to match all requests for this scheme.
    // CEF's domain matching for custom schemes can be inconsistent,
    // so matching all domains is more reliable.
    let ret = register_scheme_handler_factory(
        Some(&scheme_str),
        None, // empty domain = match all
        Some(&mut factory),
    );

    eprintln!(
        "[wrymium] register_scheme_handler_factory({scheme}://*) = {ret}"
    );
}

// --- SchemeHandlerFactory ---

wrap_scheme_handler_factory! {
    struct WrymiumSchemeHandlerFactory {
        handler: ProtocolHandler,
    }

    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let handler = self.handler.clone();

            // Extract request info from CefRequest
            let (method, url, headers, body) = extract_request_info(request);
            eprintln!("[wrymium:scheme] create handler: method={method} url={url} body_len={}", body.len());

            // Build http::Request
            let mut builder = http::Request::builder().method(method.as_str()).uri(&url);
            for (name, value) in &headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            let http_request = builder.body(body).unwrap_or_else(|_| {
                http::Request::builder()
                    .body(Vec::new())
                    .unwrap()
            });

            // Create response state shared between the handler callback and resource handler
            let response_state = Arc::new(Mutex::new(ResponseState::Pending { callback: None }));
            let state_for_callback = response_state.clone();

            // Dispatch to the user handler
            let webview_id = "";
            let responder = RequestAsyncResponder::new(Box::new(move |http_response| {
                let mut state = state_for_callback.lock().unwrap();
                let stored_callback = if let ResponseState::Pending { callback } = &mut *state {
                    callback.take()
                } else {
                    None
                };
                *state = ResponseState::Ready(http_response);
                drop(state);
                if let Some(SendableCallback(cb)) = stored_callback {
                    cb.cont();
                }
            }));

            handler(webview_id, http_request, responder);

            Some(WrymiumResourceHandler::new(response_state, Arc::new(AtomicUsize::new(0))))
        }
    }
}

// --- ResourceHandler ---

/// Wrapper to make Callback Send-safe.
/// SAFETY: CEF callbacks are ref-counted and designed to be called from the IO thread.
/// We only store and call cont() once.
struct SendableCallback(Callback);
unsafe impl Send for SendableCallback {}

enum ResponseState {
    Pending {
        callback: Option<SendableCallback>,
    },
    Ready(http::Response<Cow<'static, [u8]>>),
}

wrap_resource_handler! {
    struct WrymiumResourceHandler {
        state: Arc<Mutex<ResponseState>>,
        bytes_read: Arc<AtomicUsize>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut ::std::os::raw::c_int>,
            callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            let state = self.state.lock().unwrap();
            match &*state {
                ResponseState::Ready(_) => {
                    eprintln!("[wrymium:scheme] open: response already ready");
                    if let Some(hr) = handle_request {
                        *hr = 1;
                    }
                    1
                }
                ResponseState::Pending { .. } => {
                    eprintln!("[wrymium:scheme] open: response pending, deferring");
                    if let Some(hr) = handle_request {
                        *hr = 0;
                    }
                    drop(state);
                    if let Some(cb) = callback {
                        let mut state = self.state.lock().unwrap();
                        // Check if response arrived while we were setting up
                        if matches!(*state, ResponseState::Ready(_)) {
                            drop(state);
                            cb.cont();
                        } else if let ResponseState::Pending { callback: ref mut stored_cb } = *state {
                            *stored_cb = Some(SendableCallback(cb.clone()));
                        }
                    }
                    1
                }
            }
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            let state = self.state.lock().unwrap();
            if let ResponseState::Ready(ref http_resp) = *state {
                eprintln!("[wrymium:scheme] response_headers: status={} body_len={}", http_resp.status(), http_resp.body().len());
                if let Some(resp) = response {
                    ImplResponse::set_status(resp, http_resp.status().as_u16() as i32);

                    // Set Content-Type
                    if let Some(ct) = http_resp.headers().get("content-type") {
                        if let Ok(ct_str) = ct.to_str() {
                            let mime = CefString::from(ct_str);
                            ImplResponse::set_mime_type(resp, Some(&mime));
                        }
                    }

                    // Set all response headers (CORS, Tauri-Response, etc.)
                    let mut header_map = CefStringMultimap::new();
                    for (name, value) in http_resp.headers().iter() {
                        if let Ok(v) = value.to_str() {
                            header_map.append(name.as_str(), v);
                        }
                    }
                    ImplResponse::set_header_map(resp, Some(&mut header_map));
                }

                if let Some(len) = response_length {
                    *len = http_resp.body().len() as i64;
                }
            }
        }

        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> ::std::os::raw::c_int {
            let state = self.state.lock().unwrap();
            if let ResponseState::Ready(ref http_resp) = *state {
                let body = http_resp.body();
                let offset = self.bytes_read.load(Ordering::Acquire);
                let remaining = body.len().saturating_sub(offset);

                if remaining == 0 {
                    if let Some(br) = bytes_read {
                        *br = 0;
                    }
                    return 0; // EOF
                }

                let to_copy = remaining.min(bytes_to_read as usize);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        body[offset..].as_ptr(),
                        data_out,
                        to_copy,
                    );
                }

                self.bytes_read.fetch_add(to_copy, Ordering::Release);

                if let Some(br) = bytes_read {
                    *br = to_copy as i32;
                }
                1
            } else {
                if let Some(br) = bytes_read {
                    *br = 0;
                }
                0
            }
        }

        fn cancel(&self) {
        }
    }
}

// --- Helper functions ---

fn extract_request_info(
    request: Option<&mut Request>,
) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let Some(request) = request else {
        return ("GET".into(), "".into(), vec![], vec![]);
    };

    // Method
    let method_cef = ImplRequest::method(request);
    let method = CefString::from(&method_cef).to_string();
    let method = if method.is_empty() {
        "GET".to_string()
    } else {
        method
    };

    // URL
    let url_cef = ImplRequest::url(request);
    let url = CefString::from(&url_cef).to_string();

    // Headers — extract individual known headers via header_by_name
    let mut headers = Vec::new();
    for name in &["Content-Type", "Tauri-Callback", "Tauri-Error", "Tauri-Invoke-Key", "Origin"] {
        let key = CefString::from(*name);
        let val = ImplRequest::header_by_name(request, Some(&key));
        let val_str = CefString::from(&val).to_string();
        if !val_str.is_empty() {
            headers.push((name.to_string(), val_str));
        }
    }

    // POST body
    let mut body = Vec::new();
    if let Some(post_data) = ImplRequest::post_data(request) {
        let count = ImplPostData::element_count(&post_data);
        let mut elements: Vec<Option<PostDataElement>> = Vec::with_capacity(count);
        ImplPostData::elements(&post_data, Some(&mut elements));
        for element in elements.into_iter().flatten() {
            let size = ImplPostDataElement::bytes_count(&element);
            if size > 0 {
                let mut buf = vec![0u8; size];
                ImplPostDataElement::bytes(&element, size, buf.as_mut_ptr());
                body.extend_from_slice(&buf);
            }
        }
    }

    (method, url, headers, body)
}
