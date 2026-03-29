//! Unit tests for wrymium — Layer A tests that don't require CEF runtime.

#[cfg(test)]
mod types_tests {
    use crate::types::*;

    #[test]
    fn rect_default() {
        let r = Rect::default();
        assert_eq!(
            r.position,
            dpi::Position::Logical(dpi::LogicalPosition::new(0.0, 0.0))
        );
        assert_eq!(
            r.size,
            dpi::Size::Logical(dpi::LogicalSize::new(800.0, 600.0))
        );
    }

    #[test]
    fn background_throttling_default() {
        assert_eq!(
            BackgroundThrottlingPolicy::default(),
            BackgroundThrottlingPolicy::Suspend
        );
    }

    #[test]
    fn drag_drop_event_variants() {
        let enter = DragDropEvent::Enter {
            paths: vec!["/tmp/file.txt".into()],
            position: (10.0, 20.0),
        };
        assert!(matches!(enter, DragDropEvent::Enter { .. }));

        let over = DragDropEvent::Over {
            position: (30.0, 40.0),
        };
        assert!(matches!(over, DragDropEvent::Over { .. }));

        let drop = DragDropEvent::Drop {
            paths: vec![],
            position: (0.0, 0.0),
        };
        assert!(matches!(drop, DragDropEvent::Drop { .. }));

        let leave = DragDropEvent::Leave;
        assert!(matches!(leave, DragDropEvent::Leave));
    }

    #[test]
    fn proxy_config_construction() {
        let http = ProxyConfig::Http(ProxyEndpoint {
            host: "127.0.0.1".into(),
            port: "8080".into(),
        });
        assert!(matches!(http, ProxyConfig::Http(_)));

        let socks = ProxyConfig::Socks5(ProxyEndpoint {
            host: "proxy.example.com".into(),
            port: "1080".into(),
        });
        assert!(matches!(socks, ProxyConfig::Socks5(_)));
    }

    #[test]
    fn page_load_event_values() {
        assert_ne!(PageLoadEvent::Started, PageLoadEvent::Finished);
    }

    #[test]
    fn new_window_response_debug() {
        let allow = NewWindowResponse::Allow;
        let deny = NewWindowResponse::Deny;
        assert!(format!("{:?}", allow).contains("Allow"));
        assert!(format!("{:?}", deny).contains("Deny"));
    }

    #[test]
    fn theme_values() {
        assert_ne!(Theme::Dark, Theme::Light);
    }

    #[test]
    fn scrollbar_style_values() {
        assert_ne!(ScrollBarStyle::Default, ScrollBarStyle::FluentOverlay);
    }

    #[test]
    fn cookie_construction() {
        let mut cookie = Cookie::new("session", "abc123");
        cookie.set_domain(".example.com");
        cookie.set_path("/");
        assert_eq!(cookie.name(), "session");
        assert_eq!(cookie.domain(), Some("example.com"));
    }

    #[test]
    fn new_window_features_default() {
        let f = NewWindowFeatures::default();
        assert!(f.size.is_none());
        assert!(f.position.is_none());
    }
}

#[cfg(test)]
mod error_tests {
    use crate::error::Error;

    #[test]
    fn error_display_message_sender() {
        let e = Error::MessageSender;
        assert_eq!(format!("{e}"), "failed to send message");
    }

    #[test]
    fn error_display_cef_error() {
        let e = Error::CefError("test failure".into());
        assert_eq!(format!("{e}"), "CEF error: test failure");
    }

    #[test]
    fn error_display_cef_not_initialized() {
        let e = Error::CefNotInitialized;
        assert_eq!(format!("{e}"), "CEF not initialized");
    }

    #[test]
    fn error_display_unsupported_window_handle() {
        let e = Error::UnsupportedWindowHandle;
        assert_eq!(format!("{e}"), "unsupported window handle");
    }

    #[test]
    fn error_display_duplicate_protocol() {
        let e = Error::DuplicateCustomProtocol("ipc".into());
        assert_eq!(format!("{e}"), "duplicate custom protocol: ipc");
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let e: Error = io_err.into();
        assert!(matches!(e, Error::Io(_)));
        assert!(format!("{e}").contains("not found"));
    }

    #[test]
    fn error_from_nul() {
        let nul_err = std::ffi::CString::new("hello\0world").unwrap_err();
        let e: Error = nul_err.into();
        assert!(matches!(e, Error::NulError(_)));
    }

    #[test]
    fn error_is_std_error() {
        let e = Error::MessageSender;
        let _: &dyn std::error::Error = &e;
    }

    #[test]
    fn result_type_alias_works() {
        let ok: crate::error::Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: crate::error::Result<i32> = Err(Error::MessageSender);
        assert!(err.is_err());
    }
}

#[cfg(test)]
mod context_tests {
    use crate::context::WebContext;
    use std::path::PathBuf;

    #[test]
    fn new_with_no_directory() {
        let ctx = WebContext::new(None);
        assert!(ctx.data_directory().is_none());
        assert!(!ctx.allows_automation());
    }

    #[test]
    fn new_with_directory() {
        let ctx = WebContext::new(Some(PathBuf::from("/tmp/cef-data")));
        assert_eq!(
            ctx.data_directory().unwrap(),
            &PathBuf::from("/tmp/cef-data")
        );
    }

    #[test]
    fn set_allows_automation() {
        let mut ctx = WebContext::new(None);
        assert!(!ctx.allows_automation());
        ctx.set_allows_automation(true);
        assert!(ctx.allows_automation());
        ctx.set_allows_automation(false);
        assert!(!ctx.allows_automation());
    }
}

#[cfg(test)]
mod webview_builder_tests {
    use crate::{WebContext, WebViewBuilder};

    #[test]
    fn builder_new_defaults() {
        let builder = WebViewBuilder::new();
        assert!(builder.url.is_none());
        assert!(builder.html.is_none());
        assert!(builder.ipc_handler.is_none());
        assert!(builder.custom_protocols.is_empty());
        assert!(builder.initialization_scripts.is_empty());
        assert!(!builder.devtools);
        assert!(!builder.transparent);
        assert!(builder.focused);
        assert!(builder.visible);
        assert!(!builder.javascript_disabled);
    }

    #[test]
    fn builder_with_web_context() {
        let mut ctx = WebContext::new(None);
        let builder = WebViewBuilder::new_with_web_context(&mut ctx);
        assert!(builder.web_context.is_some());
    }

    #[test]
    fn builder_chaining() {
        let builder = WebViewBuilder::new()
            .with_url("https://example.com")
            .with_devtools(true)
            .with_transparent(true)
            .with_focused(false)
            .with_visible(false)
            .with_user_agent("wrymium/test")
            .with_accept_first_mouse(true)
            .with_clipboard(true)
            .with_hotkeys_zoom(true)
            .with_incognito(true);

        assert_eq!(builder.url.as_deref(), Some("https://example.com"));
        assert!(builder.devtools);
        assert!(builder.transparent);
        assert!(!builder.focused);
        assert!(!builder.visible);
        assert_eq!(builder.user_agent.as_deref(), Some("wrymium/test"));
        assert!(builder.accept_first_mouse);
        assert!(builder.clipboard);
        assert!(builder.hotkeys_zoom);
        assert!(builder.incognito);
    }

    #[test]
    fn builder_with_html() {
        let builder = WebViewBuilder::new().with_html("<h1>Hello</h1>");
        assert_eq!(builder.html.as_deref(), Some("<h1>Hello</h1>"));
        assert!(builder.url.is_none());
    }

    #[test]
    fn builder_with_id() {
        let builder = WebViewBuilder::new().with_id("my-webview");
        assert_eq!(builder.id.as_deref(), Some("my-webview"));
    }

    #[test]
    fn builder_with_initialization_scripts() {
        let builder = WebViewBuilder::new()
            .with_initialization_script("console.log('hello')")
            .with_initialization_script_for_main_only("window.x = 1", true);

        assert_eq!(builder.initialization_scripts.len(), 2);
        assert_eq!(builder.initialization_scripts[0].0, "console.log('hello')");
        assert!(!builder.initialization_scripts[0].1); // not main_only
        assert_eq!(builder.initialization_scripts[1].0, "window.x = 1");
        assert!(builder.initialization_scripts[1].1); // main_only
    }

    #[test]
    fn builder_with_javascript_disabled() {
        let builder = WebViewBuilder::new().with_javascript_disabled();
        assert!(builder.javascript_disabled);
    }

    #[test]
    fn builder_with_background_color() {
        let builder = WebViewBuilder::new().with_background_color((255, 0, 0, 128));
        assert_eq!(builder.background_color, Some((255, 0, 0, 128)));
    }

    #[test]
    fn builder_with_data_store_identifier() {
        let id = [1u8; 16];
        let builder = WebViewBuilder::new().with_data_store_identifier(id);
        assert_eq!(builder.data_store_identifier, Some([1u8; 16]));
    }

    #[test]
    fn builder_noop_methods_dont_panic() {
        // These are no-op stub methods that should not panic
        let _ = WebViewBuilder::new()
            .with_https_scheme(true)
            .with_additional_browser_args("--flag")
            .with_theme(crate::types::Theme::Dark)
            .with_scroll_bar_style(crate::types::ScrollBarStyle::Default)
            .with_browser_extensions_enabled(true)
            .with_extensions_path(std::path::Path::new("/tmp"))
            .with_allow_link_preview(true);
    }
}

#[cfg(test)]
mod cef_init_tests {
    use crate::cef_init;

    #[test]
    fn is_cef_subprocess_detects_type_flag() {
        // In a normal test run, there's no --type= argument
        assert!(!cef_init::is_cef_subprocess());
    }
}

#[cfg(test)]
mod lib_tests {
    #[test]
    fn webview_version_returns_ok() {
        let version = crate::webview_version();
        assert!(version.is_ok());
        let v = version.unwrap();
        assert!(v.contains("CEF"));
        assert!(v.contains("wrymium"));
    }

    #[test]
    fn is_cef_subprocess_false_in_tests() {
        assert!(!crate::is_cef_subprocess());
    }
}

#[cfg(test)]
mod responder_tests {
    use crate::webview::RequestAsyncResponder;
    use std::borrow::Cow;
    use std::sync::{Arc, Mutex};

    #[test]
    fn responder_calls_closure() {
        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        let responder = RequestAsyncResponder::new(Box::new(move |response| {
            assert_eq!(response.status(), 200);
            *received_clone.lock().unwrap() = true;
        }));

        let response = http::Response::builder()
            .status(200)
            .body(Cow::from(b"ok".to_vec()))
            .unwrap();

        responder.respond(response);
        assert!(*received.lock().unwrap());
    }
}

#[cfg(test)]
mod scheme_tests {
    use crate::scheme;

    #[test]
    fn register_browser_webview_mapping() {
        scheme::register_browser_webview(42, "test-webview");
        // Just verify it doesn't panic — actual lookup tested via integration
    }
}

#[cfg(test)]
mod builder_protocol_tests {
    use crate::WebViewBuilder;
    use std::borrow::Cow;

    #[test]
    fn builder_with_custom_protocol() {
        let builder = WebViewBuilder::new()
            .with_custom_protocol("test".to_string(), |_id, _req| {
                http::Response::builder()
                    .body(Cow::from(b"response".to_vec()))
                    .unwrap()
            });
        assert_eq!(builder.custom_protocols.len(), 1);
        assert_eq!(builder.custom_protocols[0].0, "test");
    }

    #[test]
    fn builder_with_async_protocol() {
        let builder = WebViewBuilder::new()
            .with_asynchronous_custom_protocol("async-test".to_string(), |_id, _req, responder| {
                responder.respond(
                    http::Response::builder()
                        .body(Cow::from(b"async response".to_vec()))
                        .unwrap(),
                );
            });
        assert_eq!(builder.custom_protocols.len(), 1);
        assert_eq!(builder.custom_protocols[0].0, "async-test");
    }

    #[test]
    fn builder_with_ipc_handler() {
        let builder = WebViewBuilder::new()
            .with_ipc_handler(|_req| {});
        assert!(builder.ipc_handler.is_some());
    }

    #[test]
    fn builder_with_event_handlers() {
        let builder = WebViewBuilder::new()
            .with_navigation_handler(|_url| true)
            .with_document_title_changed_handler(|_title| {})
            .with_on_page_load_handler(|_event, _url| {})
            .with_drag_drop_handler(|_event| true);

        assert!(builder.navigation_handler.is_some());
        assert!(builder.document_title_changed_handler.is_some());
        assert!(builder.on_page_load_handler.is_some());
        assert!(builder.drag_drop_handler.is_some());
    }
}

// ==========================================================================
// CDP Bridge unit tests (no CEF runtime needed)
// ==========================================================================

#[cfg(test)]
mod cdp_tests {
    use crate::cdp::*;
    use std::sync::Arc;

    #[test]
    fn cdp_error_display() {
        assert_eq!(CdpError::Timeout.to_string(), "CDP call timed out");
        assert_eq!(CdpError::AgentDetached.to_string(), "DevTools agent detached");
        assert_eq!(CdpError::ChannelClosed.to_string(), "CDP response channel closed");
        assert_eq!(CdpError::NotReady.to_string(), "browser not ready");
        assert_eq!(CdpError::SendFailed(0).to_string(), "send_dev_tools_message failed: 0");
        assert_eq!(
            CdpError::Json("bad json".into()).to_string(),
            "CDP JSON error: bad json"
        );
        let v = serde_json::json!({"code": -32601, "message": "method not found"});
        assert!(CdpError::MethodFailed(v).to_string().contains("method not found"));
    }

    #[test]
    fn cdp_event_params_json_empty() {
        let event = CdpEvent {
            method: "Page.loadEventFired".into(),
            params: vec![],
        };
        let v = event.params_json().unwrap();
        assert!(v.is_null());
    }

    #[test]
    fn cdp_event_params_json_valid() {
        let event = CdpEvent {
            method: "Page.frameNavigated".into(),
            params: br#"{"frameId":"abc"}"#.to_vec(),
        };
        let v = event.params_json().unwrap();
        assert_eq!(v["frameId"], "abc");
    }

    #[test]
    fn cdp_event_params_json_invalid() {
        let event = CdpEvent {
            method: "test".into(),
            params: b"not json".to_vec(),
        };
        assert!(event.params_json().is_err());
    }

    #[test]
    fn cdp_bridge_inner_complete_request() {
        let inner = CdpBridgeInner::new();
        let (tx, rx) = std::sync::mpsc::channel();
        inner.pending.lock().unwrap().insert(42, tx);

        inner.complete_request(42, Ok(serde_json::json!({"result": "ok"})));

        let result = rx.recv().unwrap().unwrap();
        assert_eq!(result["result"], "ok");
        assert!(inner.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn cdp_bridge_inner_complete_missing_id() {
        let inner = CdpBridgeInner::new();
        // Should not panic when completing a non-existent request
        inner.complete_request(999, Ok(serde_json::json!(null)));
    }

    #[test]
    fn cdp_bridge_inner_drain_all_pending() {
        let inner = CdpBridgeInner::new();
        let (tx1, rx1) = std::sync::mpsc::channel();
        let (tx2, rx2) = std::sync::mpsc::channel();
        inner.pending.lock().unwrap().insert(1, tx1);
        inner.pending.lock().unwrap().insert(2, tx2);

        inner.drain_all_pending(|| CdpError::AgentDetached);

        assert!(inner.pending.lock().unwrap().is_empty());
        assert!(matches!(rx1.recv().unwrap(), Err(CdpError::AgentDetached)));
        assert!(matches!(rx2.recv().unwrap(), Err(CdpError::AgentDetached)));
    }

    #[test]
    fn cdp_bridge_inner_broadcast_event() {
        let inner = CdpBridgeInner::new();
        let (tx1, rx1) = std::sync::mpsc::channel();
        let (tx2, rx2) = std::sync::mpsc::channel();
        inner.event_subscribers.lock().unwrap().push(tx1);
        inner.event_subscribers.lock().unwrap().push(tx2);

        let event = CdpEvent {
            method: "Page.loadEventFired".into(),
            params: vec![],
        };
        inner.broadcast_event(event);

        assert_eq!(rx1.recv().unwrap().method, "Page.loadEventFired");
        assert_eq!(rx2.recv().unwrap().method, "Page.loadEventFired");
    }

    #[test]
    fn cdp_bridge_inner_broadcast_removes_closed_channels() {
        let inner = CdpBridgeInner::new();
        let (tx1, _rx1_dropped) = std::sync::mpsc::channel::<CdpEvent>();
        let (tx2, rx2) = std::sync::mpsc::channel();
        inner.event_subscribers.lock().unwrap().push(tx1);
        inner.event_subscribers.lock().unwrap().push(tx2);

        // Drop rx1 — tx1 send will fail
        drop(_rx1_dropped);

        let event = CdpEvent {
            method: "test".into(),
            params: vec![],
        };
        inner.broadcast_event(event);

        // tx1 should be removed, only tx2 remains
        assert_eq!(inner.event_subscribers.lock().unwrap().len(), 1);
        assert_eq!(rx2.recv().unwrap().method, "test");
    }
}

// ==========================================================================
// Browser Use unit tests (no CEF runtime needed)
// ==========================================================================

#[cfg(test)]
mod browser_use_tests {
    use crate::browser_use::*;

    #[test]
    fn key_windows_key_codes() {
        assert_eq!(Key::Enter.windows_key_code(), 0x0D);
        assert_eq!(Key::Tab.windows_key_code(), 0x09);
        assert_eq!(Key::Escape.windows_key_code(), 0x1B);
        assert_eq!(Key::Backspace.windows_key_code(), 0x08);
        assert_eq!(Key::Space.windows_key_code(), 0x20);
        assert_eq!(Key::ArrowUp.windows_key_code(), 0x26);
        assert_eq!(Key::ArrowDown.windows_key_code(), 0x28);
        assert_eq!(Key::ArrowLeft.windows_key_code(), 0x25);
        assert_eq!(Key::ArrowRight.windows_key_code(), 0x27);
        assert_eq!(Key::Delete.windows_key_code(), 0x2E);
        assert_eq!(Key::Home.windows_key_code(), 0x24);
        assert_eq!(Key::End.windows_key_code(), 0x23);
        assert_eq!(Key::PageUp.windows_key_code(), 0x21);
        assert_eq!(Key::PageDown.windows_key_code(), 0x22);
        // ASCII letters → uppercase VK
        assert_eq!(Key::Char('a').windows_key_code(), 'A' as i32);
        assert_eq!(Key::Char('Z').windows_key_code(), 'Z' as i32);
        // Digits
        assert_eq!(Key::Char('0').windows_key_code(), '0' as i32);
    }

    #[test]
    fn key_char_values() {
        assert_eq!(Key::Enter.char_value(), '\r' as u16);
        assert_eq!(Key::Tab.char_value(), '\t' as u16);
        assert_eq!(Key::Space.char_value(), ' ' as u16);
        assert_eq!(Key::Backspace.char_value(), 0x08);
        assert_eq!(Key::Char('x').char_value(), 'x' as u16);
        // Non-printable keys have char_value 0
        assert_eq!(Key::ArrowUp.char_value(), 0);
        assert_eq!(Key::Delete.char_value(), 0);
        assert_eq!(Key::Escape.char_value(), 0);
    }

    #[test]
    fn modifiers_to_cef_flags() {
        let none = Modifiers::default();
        assert_eq!(none.to_cef_flags(), 0);

        let shift = Modifiers { shift: true, ..Default::default() };
        assert_eq!(shift.to_cef_flags(), 2); // EVENTFLAG_SHIFT_DOWN

        let ctrl = Modifiers { ctrl: true, ..Default::default() };
        assert_eq!(ctrl.to_cef_flags(), 4); // EVENTFLAG_CONTROL_DOWN

        let alt = Modifiers { alt: true, ..Default::default() };
        assert_eq!(alt.to_cef_flags(), 8); // EVENTFLAG_ALT_DOWN

        let meta = Modifiers { meta: true, ..Default::default() };
        assert_eq!(meta.to_cef_flags(), 128); // EVENTFLAG_COMMAND_DOWN

        // Combined
        let ctrl_shift = Modifiers { ctrl: true, shift: true, ..Default::default() };
        assert_eq!(ctrl_shift.to_cef_flags(), 6); // 4 + 2
    }

    #[test]
    fn screenshot_options_default() {
        let opts = ScreenshotOptions::default();
        assert!(opts.format.is_none());
        assert!(opts.quality.is_none());
        assert!(opts.clip.is_none());
    }

    #[test]
    fn element_bounds_fields() {
        let bounds = ElementBounds {
            x: 10.0, y: 20.0, width: 100.0, height: 50.0,
        };
        assert_eq!(bounds.x, 10.0);
        assert_eq!(bounds.y, 20.0);
        assert_eq!(bounds.width, 100.0);
        assert_eq!(bounds.height, 50.0);
    }

    #[test]
    fn browser_cookie_clone() {
        let cookie = BrowserCookie {
            name: "session".into(),
            value: "abc123".into(),
            domain: ".example.com".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
        };
        let cloned = cookie.clone();
        assert_eq!(cloned.name, "session");
        assert_eq!(cloned.domain, ".example.com");
        assert!(cloned.secure);
    }

    #[test]
    fn frame_info_fields() {
        let frame = FrameInfo {
            id: "main".into(),
            url: "https://example.com".into(),
            name: "".into(),
            is_main: true,
        };
        assert!(frame.is_main);
        assert_eq!(frame.id, "main");
    }
}
