use std::borrow::Cow;

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

const TEST_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <title>wrymium IPC Test</title>
    <style>
        body { font-family: -apple-system, sans-serif; padding: 40px; background: #1a1a2e; color: #eee; }
        h1 { color: #e94560; }
        button { padding: 12px 24px; font-size: 16px; cursor: pointer; background: #0f3460; color: #eee;
                 border: 1px solid #e94560; border-radius: 8px; margin: 8px; }
        button:hover { background: #e94560; }
        #log { margin-top: 20px; padding: 16px; background: #16213e; border-radius: 8px;
               font-family: monospace; white-space: pre-wrap; min-height: 100px; }
        .info { color: #a8dadc; }
    </style>
</head>
<body>
    <h1>wrymium IPC Test</h1>
    <p class="info">CEF-powered WebView running inside a tao window</p>

    <button onclick="testIpc()">Test window.ipc.postMessage</button>
    <button onclick="testFetch()">Test fetch ipc://localhost</button>
    <button onclick="checkIpc()">Check window.ipc exists</button>
    <button onclick="checkInitScript()">Check init script</button>

    <div id="log"></div>

    <script>
        function log(msg) {
            document.getElementById('log').textContent += new Date().toLocaleTimeString() + ' ' + msg + '\n';
        }

        function checkIpc() {
            if (window.ipc && typeof window.ipc.postMessage === 'function') {
                log('[OK] window.ipc.postMessage is available');
            } else {
                log('[MISSING] window.ipc is not defined');
                log('  window.ipc = ' + JSON.stringify(window.ipc));
            }
        }

        function testIpc() {
            if (!window.ipc) {
                log('[ERROR] window.ipc not available');
                return;
            }
            const msg = JSON.stringify({ cmd: 'test_command', payload: { hello: 'wrymium' } });
            window.ipc.postMessage(msg);
            log('[SENT] postMessage: ' + msg);
        }

        function testFetch() {
            log('[FETCH] Sending to ipc://localhost/test_cmd ...');
            fetch('ipc://localhost/test_cmd', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ hello: 'from fetch' })
            }).then(r => r.text()).then(text => {
                log('[RESPONSE] ' + text);
            }).catch(err => {
                log('[ERROR] ' + err.message);
            });
        }

        function checkInitScript() {
            if (window.__WRYMIUM_INIT__) {
                log('[OK] Initialization script ran: __WRYMIUM_INIT__ = ' + window.__WRYMIUM_INIT__);
            } else {
                log('[MISSING] __WRYMIUM_INIT__ not set — init script did not run');
            }
        }

        // Auto-check on load
        setTimeout(() => { checkIpc(); checkInitScript(); }, 500);
    </script>
</body>
</html>
"#;

fn main() {
    // CEF subprocess check — MUST be first.
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("wrymium IPC test")
        .with_inner_size(tao::dpi::LogicalSize::new(1024.0, 768.0))
        .build(&event_loop)
        .unwrap();

    let mut web_context = WebContext::new(None);
    let _webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_html(TEST_HTML)
        .with_devtools(true)
        .with_initialization_script(
            r#"console.log('[init-script] wrymium initialization script executed!');
               window.__WRYMIUM_INIT__ = true;"#,
        )
        .with_ipc_handler(|request| {
            eprintln!(
                "[example] IPC postMessage received: {}",
                &request.body()[..request.body().len().min(200)]
            );
        })
        .with_asynchronous_custom_protocol("ipc".to_string(), |_id, request, responder| {
            let uri = request.uri().to_string();
            let body = String::from_utf8_lossy(request.body()).to_string();
            eprintln!("[example] IPC fetch received: {uri} body={body}");

            let response = http::Response::builder()
                .status(200)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .header("Tauri-Response", "ok")
                .body(Cow::from(
                    br#"{"status":"ok","message":"Hello from wrymium!"}"#.to_vec(),
                ))
                .unwrap();
            responder.respond(response);
        })
        .build(&window)
        .expect("Failed to create WebView");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                wry::shutdown();
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
