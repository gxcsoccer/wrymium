use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

const FEISHU_URL: &str =
    "https://waytoagi.feishu.cn/wiki/Cvu8wBkLXiATBckEwl7c6eSvncf";

fn main() {
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Way to AGI - Feishu Doc")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 860.0))
        .build(&event_loop)
        .unwrap();

    let mut web_context = WebContext::new(None);
    let _webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(FEISHU_URL)
        .with_devtools(true)
        .build(&window)
        .expect("Failed to create WebView");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            wry::shutdown();
            *control_flow = ControlFlow::Exit;
        }
    });
}
