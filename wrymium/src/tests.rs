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
