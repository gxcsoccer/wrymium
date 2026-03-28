//! cargo-wrymium — Cargo subcommand for wrymium apps.
//!
//! Install: cargo install --path cargo-wrymium
//! Usage:   cargo wrymium run [--release] [--name app-name]
//!          cargo wrymium bundle [--release] [--name app-name]

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    // cargo-wrymium is invoked as `cargo wrymium <cmd>`, so args[1] == "wrymium"
    let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let cmd_args = if args.len() > 1 && args[1] == "wrymium" {
        &args[2..]
    } else {
        &args[1..]
    };

    match cmd_args.first() {
        Some(&"run") => cmd_run(&cmd_args[1..]),
        Some(&"bundle") => { cmd_bundle(&cmd_args[1..]); }
        Some(&"help") | Some(&"-h") | Some(&"--help") | None => print_help(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        r#"cargo-wrymium — build, bundle, and run CEF-powered apps

USAGE:
    cargo wrymium <command> [options]

COMMANDS:
    run       Build, bundle as macOS .app, and launch
    bundle    Build and bundle without launching
    help      Show this help

OPTIONS:
    --release           Build in release mode (LTO + strip)
    --name <name>       Binary name (auto-detected from Cargo.toml if omitted)
    --display <name>    Display name for the .app bundle

EXAMPLES:
    cargo wrymium run                          # build + bundle + launch
    cargo wrymium run --release                # optimized release build
    cargo wrymium run --name my-app            # specify binary name
    cargo wrymium bundle --release             # bundle only

INSTALL:
    cargo install --path cargo-wrymium
"#
    );
}

struct Config {
    binary_name: String,
    display_name: String,
    release: bool,
    project_dir: PathBuf,
}

fn parse_config(args: &[&str]) -> Config {
    let mut binary_name: Option<String> = None;
    let mut display_name: Option<String> = None;
    let mut release = false;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--release" => release = true,
            "--name" => {
                i += 1;
                binary_name = args.get(i).map(|s| s.to_string());
            }
            "--display" => {
                i += 1;
                display_name = args.get(i).map(|s| s.to_string());
            }
            _ => {
                if binary_name.is_none() {
                    binary_name = Some(args[i].to_string());
                }
            }
        }
        i += 1;
    }

    let project_dir = env::current_dir().unwrap();

    // Auto-detect binary name from Cargo.toml if not specified
    let binary_name = binary_name.unwrap_or_else(|| {
        detect_binary_name(&project_dir)
    });

    let display_name = display_name.unwrap_or_else(|| binary_name.clone());

    Config { binary_name, display_name, release, project_dir }
}

fn detect_binary_name(dir: &Path) -> String {
    let cargo_toml = dir.join("Cargo.toml");
    if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("name") {
                if let Some(name) = line.split('"').nth(1) {
                    return name.to_string();
                }
            }
        }
    }
    eprintln!("Cannot detect binary name. Use --name <name>");
    std::process::exit(1);
}

fn find_cef_path() -> String {
    if let Ok(p) = env::var("CEF_PATH") {
        return p;
    }
    let home = env::var("HOME").unwrap_or_default();
    let search_dir = format!("{home}/.local/share/cef");
    if let Ok(entries) = std::fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains("minimal") && entry.path().is_dir() {
                return entry.path().to_string_lossy().to_string();
            }
        }
    }
    eprintln!("ERROR: CEF not found. Set CEF_PATH or run:");
    eprintln!("  cargo install export-cef-dir && export-cef-dir --force ~/.local/share/cef");
    std::process::exit(1);
}

fn cmd_bundle(args: &[&str]) -> PathBuf {
    let config = parse_config(args);
    let cef_path = find_cef_path();

    // Build
    let profile = if config.release { "release" } else { "debug" };
    println!("Building {} ({profile})...", config.binary_name);
    let mut build_cmd = Command::new("cargo");
    build_cmd.args(["build", "--bin", &config.binary_name]);
    if config.release {
        build_cmd.arg("--release");
    }
    build_cmd
        .current_dir(&config.project_dir)
        .env("CEF_PATH", &cef_path);
    let status = build_cmd.status().expect("Failed to run cargo build");
    if !status.success() {
        std::process::exit(1);
    }

    // Find target dir — could be in project dir or workspace root
    let target_dir = find_target_dir(&config.project_dir, profile);
    let bundle_dir = config.project_dir.join("target/bundle");
    let app_dir = bundle_dir.join(format!("{}.app", config.binary_name));
    let contents = app_dir.join("Contents");
    let macos = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");

    // Clean & create
    let _ = std::fs::remove_dir_all(&app_dir);
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(&frameworks).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();

    // Copy + strip binary
    let binary_src = target_dir.join(&config.binary_name);
    let binary_dst = macos.join(&config.binary_name);
    std::fs::copy(&binary_src, &binary_dst).expect("Failed to copy binary");
    if config.release {
        let _ = Command::new("strip").arg(&binary_dst).status();
    }

    // Copy CEF framework
    println!("Copying CEF framework...");
    let framework_src = Path::new(&cef_path).join("Release/Chromium Embedded Framework.framework");
    let framework_dst = frameworks.join("Chromium Embedded Framework.framework");
    copy_dir_all(&framework_src, &framework_dst).expect("Failed to copy framework");

    // Strip unused locales
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
<key>CFBundleName</key><string>{}</string>
<key>CFBundleIdentifier</key><string>com.wrymium.{}</string>
<key>CFBundleExecutable</key><string>{}</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSEnvironment</key><dict><key>MallocNanoZone</key><string>0</string></dict>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>"#,
        config.display_name, config.binary_name, config.binary_name
    );
    std::fs::write(contents.join("Info.plist"), plist).unwrap();

    // Helper apps (hardlinks)
    let helpers = [
        "Helper", "Helper (GPU)", "Helper (Renderer)",
        "Helper (Plugin)", "Helper (Alerts)",
    ];
    for helper in &helpers {
        let helper_full = format!("{} {helper}", config.binary_name);
        let helper_app = frameworks.join(format!("{helper_full}.app"));
        let helper_macos = helper_app.join("Contents/MacOS");
        std::fs::create_dir_all(&helper_macos).unwrap();
        std::fs::create_dir_all(helper_app.join("Contents/Resources")).unwrap();

        let helper_bin = helper_macos.join(&helper_full);
        if std::fs::hard_link(&binary_dst, &helper_bin).is_err() {
            std::fs::copy(&binary_dst, &helper_bin).unwrap();
        }

        let helper_plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>{helper_full}</string>
<key>CFBundleIdentifier</key><string>com.wrymium.{}.helper</string>
<key>CFBundleExecutable</key><string>{helper_full}</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><string>1</string>
</dict></plist>"#,
            config.binary_name
        );
        std::fs::write(helper_app.join("Contents/Info.plist"), helper_plist).unwrap();
    }

    let size = dir_size(&app_dir).unwrap_or(0) / (1024 * 1024);
    println!("\nBundle: {} ({size} MB)", app_dir.display());
    app_dir
}

fn cmd_run(args: &[&str]) {
    let app_dir = cmd_bundle(args);
    println!("Launching...");
    let _ = Command::new("open").arg(&app_dir).status();
}

fn find_target_dir(project_dir: &Path, profile: &str) -> PathBuf {
    // Check project dir first, then walk up to find workspace root
    let local = project_dir.join("target").join(profile);
    if local.exists() {
        return local;
    }
    let mut dir = project_dir.to_path_buf();
    loop {
        let candidate = dir.join("target").join(profile);
        if candidate.exists() {
            return candidate;
        }
        if !dir.pop() {
            break;
        }
    }
    // Fallback
    project_dir.join("target").join(profile)
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
