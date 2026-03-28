use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[1..]),
        Some("bundle") => { cmd_bundle(&args[1..]); },
        Some("help") | Some("-h") | Some("--help") | None => print_help(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        r#"wrymium xtask — development helper

USAGE:
    cargo xtask <command> [options]

COMMANDS:
    run [name] [-- display-name]    Build, bundle, and launch a wrymium example
    bundle [name] [-- display-name] Build and bundle without launching
    help                            Show this help

EXAMPLES:
    cargo xtask run                              # run wrymium-basic-example
    cargo xtask run wrymium-feishu-example       # run feishu example
    cargo xtask run -- "My App"                  # custom display name
    cargo xtask bundle wrymium-basic-example     # bundle only, don't launch
"#
    );
}

fn project_root() -> PathBuf {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    Path::new(&manifest)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

fn find_cef_path() -> String {
    if let Ok(p) = env::var("CEF_PATH") {
        return p;
    }
    // Try to find in default location
    let home = env::var("HOME").unwrap_or_default();
    let default = format!("{home}/.local/share/cef");
    if let Ok(entries) = std::fs::read_dir(&default) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains("minimal") && entry.path().is_dir() {
                return entry.path().to_string_lossy().to_string();
            }
        }
    }
    eprintln!("ERROR: CEF_PATH not set and no CEF found in {default}");
    eprintln!("Run: cargo install export-cef-dir && export-cef-dir --force ~/.local/share/cef");
    std::process::exit(1);
}

fn cmd_bundle(args: &[String]) -> PathBuf {
    let root = project_root();
    let cef_path = find_cef_path();

    // Parse args
    let mut binary_name = "wrymium-basic-example".to_string();
    let mut display_name = None;
    let mut after_sep = false;
    for arg in args {
        if *arg == "--" {
            after_sep = true;
            continue;
        }
        if after_sep {
            display_name = Some(arg.to_string());
        } else {
            binary_name = arg.to_string();
        }
    }
    let display = display_name.unwrap_or_else(|| binary_name.clone());

    println!("Building {binary_name}...");
    let status = Command::new("cargo")
        .args(["build", "--bin", &binary_name])
        .current_dir(&root)
        .env("CEF_PATH", &cef_path)
        .status()
        .expect("Failed to run cargo build");
    if !status.success() {
        std::process::exit(1);
    }

    let target_dir = root.join("target/debug");
    let bundle_dir = root.join("target/bundle");
    let app_dir = bundle_dir.join(format!("{binary_name}.app"));
    let contents = app_dir.join("Contents");
    let macos = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");

    // Clean & create
    let _ = std::fs::remove_dir_all(&app_dir);
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(&frameworks).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();

    // Copy + strip binary
    let binary_src = target_dir.join(&binary_name);
    let binary_dst = macos.join(&binary_name);
    std::fs::copy(&binary_src, &binary_dst).expect("Failed to copy binary");
    let _ = Command::new("strip").arg(&binary_dst).status();

    // Copy CEF framework
    println!("Copying CEF framework...");
    let framework_src = Path::new(&cef_path).join("Release/Chromium Embedded Framework.framework");
    let framework_dst = frameworks.join("Chromium Embedded Framework.framework");
    copy_dir_all(&framework_src, &framework_dst).expect("Failed to copy CEF framework");

    // Strip locales (keep en, en_US, zh_CN, zh_Hans)
    let resources = framework_dst.join("Resources");
    if resources.exists() {
        if let Ok(entries) = std::fs::read_dir(&resources) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".lproj") {
                    let lang = name.trim_end_matches(".lproj");
                    match lang {
                        "en" | "en_US" | "zh_CN" | "zh_Hans" => {}
                        _ => { let _ = std::fs::remove_dir_all(entry.path()); }
                    }
                }
            }
        }
    }

    // Info.plist
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>{display}</string>
<key>CFBundleIdentifier</key><string>com.wrymium.{binary_name}</string>
<key>CFBundleExecutable</key><string>{binary_name}</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSEnvironment</key><dict><key>MallocNanoZone</key><string>0</string></dict>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>"#
    );
    std::fs::write(contents.join("Info.plist"), plist).unwrap();

    // Helper apps (hardlinks)
    let helpers = [
        "Helper", "Helper (GPU)", "Helper (Renderer)",
        "Helper (Plugin)", "Helper (Alerts)",
    ];
    for helper in &helpers {
        let helper_full = format!("{binary_name} {helper}");
        let helper_app = frameworks.join(format!("{helper_full}.app"));
        let helper_macos = helper_app.join("Contents/MacOS");
        std::fs::create_dir_all(&helper_macos).unwrap();
        std::fs::create_dir_all(helper_app.join("Contents/Resources")).unwrap();

        let helper_bin = helper_macos.join(&helper_full);
        // Try hardlink, fall back to copy
        if std::fs::hard_link(&binary_dst, &helper_bin).is_err() {
            std::fs::copy(&binary_dst, &helper_bin).unwrap();
        }

        let helper_plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>{helper_full}</string>
<key>CFBundleIdentifier</key><string>com.wrymium.{binary_name}.helper</string>
<key>CFBundleExecutable</key><string>{helper_full}</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><string>1</string>
</dict></plist>"#
        );
        std::fs::write(helper_app.join("Contents/Info.plist"), helper_plist).unwrap();
    }

    let size = dir_size(&app_dir).unwrap_or(0) / (1024 * 1024);
    println!("\nBundle created: {} ({size} MB)", app_dir.display());

    app_dir
}

fn cmd_run(args: &[String]) {
    let app_dir = cmd_bundle(args);
    println!("Launching...");
    let _ = Command::new("open").arg(&app_dir).status();
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                total += dir_size(&entry.path())?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}
