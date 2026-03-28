#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // CEF subprocess check — MUST be first.
    // CEF re-executes this binary with --type=renderer, --type=gpu-process, etc.
    if wry::is_cef_subprocess() {
        std::process::exit(wry::run_cef_subprocess());
    }

    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
