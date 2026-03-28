use std::process::Command;

fn main() {
    // Check for CMake
    if Command::new("cmake").arg("--version").output().is_err() {
        panic!(
            "\n\n\
            ========================================\n\
            ERROR: CMake is required to build wrymium (cef-dll-sys compiles libcef_dll_wrapper via CMake).\n\
            \n\
            Install CMake:\n\
            - macOS:   brew install cmake\n\
            - Linux:   sudo apt install cmake\n\
            - Windows: https://cmake.org/download/\n\
            ========================================\n\n"
        );
    }

    // Check for Ninja
    if Command::new("ninja").arg("--version").output().is_err() {
        panic!(
            "\n\n\
            ========================================\n\
            ERROR: Ninja is required to build wrymium (cef-dll-sys uses Ninja as CMake generator).\n\
            \n\
            Install Ninja:\n\
            - macOS:   brew install ninja\n\
            - Linux:   sudo apt install ninja-build\n\
            - Windows: https://ninja-build.org/\n\
            ========================================\n\n"
        );
    }

    // Set up cfg aliases for platform-specific code
    cfg_aliases::cfg_aliases! {
        macos: { target_os = "macos" },
        windows: { target_os = "windows" },
        linux: { target_os = "linux" },
        desktop: { any(target_os = "macos", target_os = "windows", target_os = "linux") },
    }
}
