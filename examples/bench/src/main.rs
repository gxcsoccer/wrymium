//! wrymium Browser Use micro-benchmarks.
//!
//! Measures CDP roundtrip, screenshot, A11y tree, DOM query, click, type_text,
//! navigate, concurrent CDP, and annotate_screenshot latencies.
//! Reports p50 / p95 / p99 / min / max for each.

use std::time::{Duration, Instant};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use wry::{WebContext, WebViewBuilder};

fn fixture_url(name: &str) -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_dir).join(format!("../../tests/fixtures/{name}"));
    let abs = std::fs::canonicalize(&path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", abs.display())
}

#[derive(Debug)]
enum BenchEvent {
    Run,
}

fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    let event_loop = EventLoopBuilder::<BenchEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = tao::window::WindowBuilder::new()
        .with_title("wrymium benchmarks")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 720.0))
        .build(&event_loop)
        .unwrap();

    let url = fixture_url("basic.html");
    let mut web_context = WebContext::new(None);
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(&url)
        .with_devtools(true)
        .build(&window)
        .expect("Failed to create WebView");

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        let _ = proxy.send_event(BenchEvent::Run);
    });

    let mut done = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                wry::shutdown();
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(BenchEvent::Run) if !done => {
                done = true;
                run_benchmarks(&webview);
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_secs(1));
                    std::process::exit(0);
                });
            }
            _ => {}
        }
    });
}

// ---------------------------------------------------------------------------
// Stats helper
// ---------------------------------------------------------------------------

struct BenchResult {
    name: &'static str,
    samples: Vec<Duration>,
}

impl BenchResult {
    fn new(name: &'static str) -> Self {
        Self { name, samples: Vec::new() }
    }

    fn record(&mut self, d: Duration) {
        self.samples.push(d);
    }

    fn print(&mut self) {
        if self.samples.is_empty() {
            println!("  {:<42} (no samples)", self.name);
            return;
        }
        self.samples.sort();
        let n = self.samples.len();
        let p = |pct: usize| -> Duration {
            let idx = (n * pct / 100).min(n - 1);
            self.samples[idx]
        };
        let min = self.samples[0];
        let max = self.samples[n - 1];
        let sum: Duration = self.samples.iter().sum();
        let avg = sum / n as u32;

        println!(
            "  {:<42} n={:<5} avg={:<9} p50={:<9} p95={:<9} p99={:<9} min={:<9} max={}",
            self.name,
            n,
            fmt_dur(avg),
            fmt_dur(p(50)),
            fmt_dur(p(95)),
            fmt_dur(p(99)),
            fmt_dur(min),
            fmt_dur(max),
        );
    }
}

fn fmt_dur(d: Duration) -> String {
    let us = d.as_micros();
    if us < 1000 {
        format!("{us}µs")
    } else if us < 1_000_000 {
        format!("{:.2}ms", us as f64 / 1000.0)
    } else {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    }
}

// ---------------------------------------------------------------------------
// Wait for page ready
// ---------------------------------------------------------------------------

fn wait_ready(webview: &wry::WebView) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(v) = webview.evaluate("document.readyState") {
            if v.as_str() == Some("complete") { return; }
        }
        if Instant::now() > deadline { return; }
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn run_benchmarks(webview: &wry::WebView) {
    println!("\n================================================================");
    println!("  wrymium Browser Use Micro-Benchmarks");
    println!("================================================================\n");

    wait_ready(webview);

    // Warmup
    for _ in 0..5 {
        let _ = webview.cdp_send_blocking(
            "Runtime.evaluate",
            serde_json::json!({"expression": "1", "returnByValue": true}),
            Duration::from_secs(5),
        );
    }

    // ------------------------------------------------------------------
    // 1. CDP roundtrip: Runtime.evaluate("1")
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("cdp_roundtrip (evaluate \"1\")");
        for _ in 0..1000 {
            let t = Instant::now();
            let _ = webview.cdp_send_blocking(
                "Runtime.evaluate",
                serde_json::json!({"expression": "1", "returnByValue": true}),
                Duration::from_secs(5),
            );
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 2. Screenshot (full viewport, PNG)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("screenshot (full viewport PNG)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.screenshot(&wry::browser_use::ScreenshotOptions::default());
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 3. Screenshot (200x200 clip)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("screenshot (200x200 clip)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.screenshot(&wry::browser_use::ScreenshotOptions {
                clip: Some(wry::browser_use::ClipRect {
                    x: 0.0, y: 0.0, width: 200.0, height: 200.0,
                }),
                ..Default::default()
            });
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 4. Accessibility tree (basic.html)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("a11y_tree (basic.html)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.accessibility_tree();
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 5. Accessibility tree (a11y.html — more complex)
    // ------------------------------------------------------------------
    {
        let a11y_url = fixture_url("a11y.html");
        webview.navigate(&a11y_url, false).ok();
        std::thread::sleep(Duration::from_secs(1));
        wait_ready(webview);

        let mut b = BenchResult::new("a11y_tree (a11y.html, complex)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.accessibility_tree();
            b.record(t.elapsed());
        }
        b.print();

        // Navigate back
        let basic_url = fixture_url("basic.html");
        webview.navigate(&basic_url, false).ok();
        std::thread::sleep(Duration::from_secs(1));
        wait_ready(webview);
    }

    // ------------------------------------------------------------------
    // 6. DOM query: querySelector + getBoxModel
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("dom_query (find_element \"#title\")");
        for _ in 0..1000 {
            let t = Instant::now();
            let _ = webview.find_element("#title");
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 7. Native click dispatch
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("click (native send_mouse_click)");
        for _ in 0..1000 {
            let t = Instant::now();
            let _ = webview.click(100, 100);
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 8. click_element (find + coordinate transform + click)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("click_element (\"#test-btn\")");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.click_element("#test-btn");
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 9. type_text (JS-based)
    // ------------------------------------------------------------------
    {
        // Focus the input first
        let _ = webview.click_element("#test-input");
        std::thread::sleep(Duration::from_millis(100));

        let mut b = BenchResult::new("type_text (\"Hello World\")");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.type_text("Hello World");
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 10. Navigate (local file + wait for load via polling)
    // ------------------------------------------------------------------
    {
        let basic_url = fixture_url("basic.html");
        let mut b = BenchResult::new("navigate (file:// + poll ready)");
        for _ in 0..20 {
            let t = Instant::now();
            let _ = webview.navigate(&basic_url, false);
            wait_ready(webview);
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 11. Concurrent CDP (10 parallel evaluates)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("concurrent_cdp (10 parallel evals)");
        for _ in 0..100 {
            let t = Instant::now();
            // Dispatch 10 at once
            let mut receivers = Vec::new();
            for i in 0..10 {
                if let Ok((_id, rx)) = webview.cdp_dispatch(
                    "Runtime.evaluate",
                    serde_json::json!({"expression": format!("{i}"), "returnByValue": true}),
                ) {
                    receivers.push(rx);
                }
            }
            // Collect all with message pump
            let mut done = vec![false; receivers.len()];
            let deadline = Instant::now() + Duration::from_secs(5);
            while done.iter().any(|d| !d) && Instant::now() < deadline {
                for (i, rx) in receivers.iter().enumerate() {
                    if !done[i] {
                        if rx.try_recv().is_ok() {
                            done[i] = true;
                        }
                    }
                }
                let _ = webview.cdp_send_blocking(
                    "Runtime.evaluate",
                    serde_json::json!({"expression": "0", "returnByValue": true}),
                    Duration::from_millis(10),
                );
            }
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 12. Annotate screenshot (full pipeline)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("annotate_screenshot (full pipeline)");
        for _ in 0..50 {
            let t = Instant::now();
            let _ = webview.annotate_screenshot();
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 13. screenshot_fast (JPEG q60 — optimized for LLM observe)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("screenshot_fast (JPEG q60)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.screenshot_fast();
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 14. accessibility_tree_compact (CDP-based)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("a11y_tree_compact (CDP-based)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.accessibility_tree_compact();
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 14b. accessibility_tree_fast (JS-based, single roundtrip)
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("a11y_tree_fast (JS, 1 roundtrip)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.accessibility_tree_fast();
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // 15. interactive_elements
    // ------------------------------------------------------------------
    {
        let mut b = BenchResult::new("interactive_elements (basic.html)");
        for _ in 0..100 {
            let t = Instant::now();
            let _ = webview.interactive_elements();
            b.record(t.elapsed());
        }
        b.print();
    }

    // ------------------------------------------------------------------
    // Summary
    // ------------------------------------------------------------------
    println!("\n================================================================");
    println!("  Benchmarks complete.");
    println!("================================================================");
}
