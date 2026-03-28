#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! This response came from Rust via CEF IPC.", name)
}

fn main() {
    // CEF subprocess check — MUST be first.
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
