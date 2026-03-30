//! wrymium WebView control example.
//!
//! Demonstrates the runtime WebView control APIs:
//! - set_visible / was_hidden
//! - bounds() — read current view frame
//! - zoom() — change zoom level
//! - focus() / print()
//! - macOS: webview(), ns_window(), reparent()
//! - theme / scrollbar style via builder

use std::sync::atomic::{AtomicU8, Ordering};

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

/// Demo stage counter — each tick advances through the demo sequence.
static STAGE: AtomicU8 = AtomicU8::new(0);

fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    // Print the actual CEF version (resolved from cef-dll-sys constant)
    let version = wry::webview_version().unwrap();
    eprintln!("[example] WebView version: {version}");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("wrymium WebView Control")
        .with_inner_size(tao::dpi::LogicalSize::new(1024.0, 768.0))
        .build(&event_loop)
        .unwrap();

    let mut web_context = WebContext::new(None);
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url("https://example.com")
        .with_devtools(true)
        // Theme: force dark color-scheme via init script
        .with_theme(wry::Theme::Dark)
        // Scrollbar: thin overlay scrollbars via ::-webkit-scrollbar CSS
        .with_scroll_bar_style(wry::ScrollBarStyle::FluentOverlay)
        .build(&window)
        .expect("Failed to create WebView");

    let start = std::time::Instant::now();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(500),
        );

        // Keep webview alive
        let _ = &webview;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                wry::shutdown();
                *control_flow = ControlFlow::Exit;
            }
            Event::NewEvents(tao::event::StartCause::ResumeTimeReached { .. }) => {
                // Run staged demos based on elapsed time
                let elapsed = start.elapsed().as_secs();
                let stage = STAGE.load(Ordering::Relaxed);

                if elapsed >= 3 && stage == 0 {
                    STAGE.store(1, Ordering::Relaxed);

                    // --- bounds() ---
                    match webview.bounds() {
                        Ok(bounds) => eprintln!("[example] Current bounds: {bounds:?}"),
                        Err(e) => eprintln!("[example] bounds() error: {e}"),
                    }

                    // --- set_visible(false) ---
                    eprintln!("[example] Hiding WebView...");
                    let _ = webview.set_visible(false);
                }

                if elapsed >= 4 && stage == 1 {
                    STAGE.store(2, Ordering::Relaxed);

                    // --- set_visible(true) ---
                    eprintln!("[example] Showing WebView");
                    let _ = webview.set_visible(true);

                    // --- zoom(1.5) ---
                    eprintln!("[example] Zooming to 1.5x");
                    let _ = webview.zoom(1.5);
                }

                if elapsed >= 6 && stage == 2 {
                    STAGE.store(3, Ordering::Relaxed);

                    // --- zoom back ---
                    eprintln!("[example] Zooming back to 1.0x");
                    let _ = webview.zoom(1.0);

                    // --- focus ---
                    eprintln!("[example] Focusing WebView");
                    let _ = webview.focus();

                    // --- macOS-specific APIs ---
                    #[cfg(target_os = "macos")]
                    {
                        use wry::WebViewExtMacOS;

                        let nsview = webview.webview();
                        eprintln!(
                            "[example/macOS] NSView pointer: {nsview:?} (null={})",
                            nsview.is_null()
                        );

                        let nswindow = webview.ns_window();
                        eprintln!(
                            "[example/macOS] NSWindow pointer: {nswindow:?} (null={})",
                            nswindow.is_null()
                        );

                        eprintln!(
                            "[example/macOS] reparent() available (not called in this demo)"
                        );
                    }

                    eprintln!("[example] All control API demos complete.");
                }
            }
            _ => {}
        }
    });
}
