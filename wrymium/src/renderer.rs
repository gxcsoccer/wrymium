//! Renderer process handler — runs in the CEF renderer subprocess.
//!
//! Responsibilities:
//! - Inject `window.ipc.postMessage` bridge via V8 extension
//! - Forward postMessage calls from renderer → browser via CefProcessMessage
//! - Inject initialization scripts via OnContextCreated (received via extra_info)

use std::collections::HashMap;
use std::sync::Mutex;

use cef::*;

use once_cell::sync::Lazy;

/// The IPC message name used for postMessage communication (renderer → browser).
pub const IPC_MSG_NAME: &str = "wrymium_ipc";

// Global shared state for the renderer process — survives across
// multiple RenderProcessHandler instances created by CEF.
static BROWSER_SCRIPTS: Lazy<Mutex<HashMap<i32, BrowserState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub(crate) struct BrowserState {
    scripts: Vec<String>,
    pending_injection: bool,
}

// --- RenderProcessHandler ---

wrap_render_process_handler! {
    pub struct WrymiumRenderProcessHandler;

    impl RenderProcessHandler {
        fn on_browser_created(
            &self,
            browser: Option<&mut Browser>,
            extra_info: Option<&mut DictionaryValue>,
        ) {
            let Some(browser) = browser else { return };
            let browser_id = ImplBrowser::identifier(browser);

            let mut scripts = Vec::new();
            if let Some(extra) = extra_info {
                let key = CefString::from("init_scripts");
                if ImplDictionaryValue::has_key(extra, Some(&key)) == 1 {
                    if let Some(list) = ImplDictionaryValue::list(extra, Some(&key)) {
                        let count = ImplListValue::size(&list);
                        for i in 0..count {
                            let s = ImplListValue::string(&list, i);
                            let script = CefString::from(&s).to_string();
                            if !script.is_empty() {
                                scripts.push(script);
                            }
                        }
                    }
                }
            }

            let script_count = scripts.len();
            let mut state = BROWSER_SCRIPTS.lock().unwrap();
            let needs_inject = if let Some(bs) = state.get_mut(&browser_id) {
                let pending = bs.pending_injection;
                bs.scripts = scripts;
                bs.pending_injection = false;
                pending
            } else {
                state.insert(browser_id, BrowserState {
                    scripts,
                    pending_injection: false,
                });
                false
            };
            drop(state);

            if script_count > 0 {
                eprintln!("[wrymium:renderer] Cached {script_count} init scripts for browser {browser_id}");
            }

            if needs_inject && script_count > 0 {
                eprintln!("[wrymium:renderer] Deferred injection triggered for browser {browser_id}");
                if let Some(mut frame) = ImplBrowser::main_frame(browser) {
                    let state = BROWSER_SCRIPTS.lock().unwrap();
                    if let Some(bs) = state.get(&browser_id) {
                        inject_scripts(&mut frame, &bs.scripts, browser_id);
                    }
                }
            }
        }

        fn on_browser_destroyed(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                let browser_id = ImplBrowser::identifier(browser);
                BROWSER_SCRIPTS.lock().unwrap().remove(&browser_id);
            }
        }

        fn on_context_created(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let Some(browser) = browser else { return };
            let Some(frame) = frame else { return };
            let Some(context) = context else { return };

            if ImplFrame::is_main(frame) != 1 {
                return;
            }

            inject_ipc_bridge(context);

            let browser_id = ImplBrowser::identifier(browser);
            let mut state = BROWSER_SCRIPTS.lock().unwrap();

            if let Some(bs) = state.get(&browser_id) {
                if !bs.scripts.is_empty() {
                    let scripts = bs.scripts.clone();
                    drop(state);
                    inject_scripts(frame, &scripts, browser_id);
                }
            } else {
                state.insert(browser_id, BrowserState {
                    scripts: Vec::new(),
                    pending_injection: true,
                });
                eprintln!(
                    "[wrymium:renderer] on_context_created before on_browser_created for browser {browser_id}, deferring"
                );
            }
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _source_process: ProcessId,
            _message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            0
        }
    }
}

fn inject_scripts(frame: &mut Frame, scripts: &[String], browser_id: i32) {
    let empty_url = CefString::from("");
    for script in scripts {
        let js = CefString::from(script.as_str());
        ImplFrame::execute_java_script(frame, Some(&js), Some(&empty_url), 0);
    }
    eprintln!(
        "[wrymium:renderer] Injected {} init scripts for browser {browser_id}",
        scripts.len()
    );
}

/// Inject the window.ipc.postMessage V8 bridge into the given context.
fn inject_ipc_bridge(context: &mut V8Context) {
    if ImplV8Context::enter(context) != 1 {
        return;
    }

    if let Some(mut global) = ImplV8Context::global(context) {
        let mut handler = WrymiumIpcV8Handler::new();

        let func_name = CefString::from("postMessage");
        if let Some(func) = v8_value_create_function(Some(&func_name), Some(&mut handler)) {
            let ipc_name = CefString::from("ipc");
            let mut ipc_obj =
                v8_value_create_object(None, None).expect("Failed to create ipc object");
            let mut func = func;
            let readonly = V8Propertyattribute::from(
                cef::sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_READONLY,
            );
            ImplV8Value::set_value_bykey(&mut ipc_obj, Some(&func_name), Some(&mut func), readonly);

            let readonly = V8Propertyattribute::from(
                cef::sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_READONLY,
            );
            ImplV8Value::set_value_bykey(&mut global, Some(&ipc_name), Some(&mut ipc_obj), readonly);
        }
    }

    ImplV8Context::exit(context);
    eprintln!("[wrymium:renderer] Injected window.ipc.postMessage bridge");
}

// --- V8 Handler for window.ipc.postMessage ---

wrap_v8_handler! {
    struct WrymiumIpcV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let Some(name) = name else { return 0 };

            if name.to_string() != "postMessage" {
                return 0;
            }

            let Some(args) = arguments else { return 0 };
            let Some(Some(arg0)) = args.first() else { return 0 };

            if ImplV8Value::is_string(arg0) != 1 {
                return 0;
            }

            let message_str = ImplV8Value::string_value(arg0);
            let message_string = CefString::from(&message_str).to_string();

            let Some(context) = v8_context_get_current_context() else { return 0 };
            let Some(mut _browser) = ImplV8Context::browser(&context) else { return 0 };
            let Some(mut frame) = ImplV8Context::frame(&context) else { return 0 };

            let msg_name = CefString::from(IPC_MSG_NAME);
            let Some(mut msg) = process_message_create(Some(&msg_name)) else { return 0 };

            if let Some(mut args_list) = ImplProcessMessage::argument_list(&msg) {
                let body_str = CefString::from(message_string.as_str());
                ImplListValue::set_string(&mut args_list, 0, Some(&body_str));
            }

            ImplFrame::send_process_message(&mut frame, ProcessId::BROWSER, Some(&mut msg));

            eprintln!(
                "[wrymium:renderer] postMessage sent: {}...",
                &message_string[..message_string.len().min(80)]
            );

            1
        }
    }
}
