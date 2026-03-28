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
pub fn register_protocol(scheme: &str, domain: &str, handler: ProtocolHandler) {
    let scheme_str = CefString::from(scheme);
    let domain_str = CefString::from(domain);
    let mut factory = WrymiumSchemeHandlerFactory::new(handler);

    let ret = register_scheme_handler_factory(
        Some(&scheme_str),
        Some(&domain_str),
        Some(&mut factory),
    );

    eprintln!(
        "[wrymium] register_scheme_handler_factory({scheme}://{domain}) = {ret}"
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
            let response_state = Arc::new(Mutex::new(ResponseState::Pending));
            let state_for_callback = response_state.clone();

            // Dispatch to the user handler
            // The webview_id is extracted from the URL or defaults to empty
            let webview_id = ""; // TODO: extract from browser label
            let responder: RequestAsyncResponder = Box::new(move |http_response| {
                let mut state = state_for_callback.lock().unwrap();
                *state = ResponseState::Ready(http_response);
            });

            handler(webview_id, http_request, responder);

            Some(WrymiumResourceHandler::new(response_state, Arc::new(AtomicUsize::new(0))))
        }
    }
}

// --- ResourceHandler ---

enum ResponseState {
    Pending,
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
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            // Check if response is already available (synchronous handler)
            let state = self.state.lock().unwrap();
            match &*state {
                ResponseState::Ready(_) => {
                    // Response ready immediately
                    if let Some(hr) = handle_request {
                        *hr = 1; // handled synchronously
                    }
                    1 // return true
                }
                ResponseState::Pending => {
                    // TODO: For async handlers, store callback and call callback.cont()
                    // when response arrives. For now, treat as synchronous.
                    if let Some(hr) = handle_request {
                        *hr = 1;
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
                if let Some(resp) = response {
                    ImplResponse::set_status(resp, http_resp.status().as_u16() as i32);

                    // Set Content-Type
                    if let Some(ct) = http_resp.headers().get("content-type") {
                        if let Ok(ct_str) = ct.to_str() {
                            let mime = CefString::from(ct_str);
                            ImplResponse::set_mime_type(resp, Some(&mime));
                        }
                    }

                    // Set custom headers (Access-Control-*, Tauri-Response, etc.)
                    // TODO: Set header map on response
                }

                if let Some(len) = response_length {
                    *len = http_resp.body().len() as i64;
                }
            }
        }

        fn read_response(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut Callback>,
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
            // Nothing to clean up
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

    let method_cef = ImplRequest::method(request);
    let method = CefString::from(&method_cef).to_string();
    let method = if method.is_empty() {
        "GET".to_string()
    } else {
        method
    };

    let url_cef = ImplRequest::url(request);
    let url = CefString::from(&url_cef).to_string();

    // Extract headers
    // TODO: Use get_header_map when available in safe wrapper

    // Extract POST body
    let body = Vec::new(); // TODO: Extract via get_post_data -> get_elements

    (method, url, vec![], body)
}
