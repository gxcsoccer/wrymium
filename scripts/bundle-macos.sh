#!/bin/bash
# Bundle a wrymium example as a macOS .app with CEF framework
# Usage: bundle-macos.sh [binary-name] [display-name]
set -euo pipefail

APP_NAME="${1:-wrymium-basic-example}"
DISPLAY_NAME="${2:-$APP_NAME}"
BUNDLE_ID="com.wrymium.${APP_NAME}"

# Find CEF directory
if [ -n "${CEF_PATH:-}" ]; then
    CEF_DIR="$CEF_PATH"
else
    CEF_DIR="$HOME/.local/share/cef/cef_binary_146.0.6+g68649e2+chromium-146.0.7680.154_macosarm64_minimal"
fi

if [ ! -d "$CEF_DIR" ]; then
    echo "ERROR: CEF directory not found at $CEF_DIR"
    echo "Set CEF_PATH or download CEF first."
    exit 1
fi

# Build the binary
echo "Building $APP_NAME..."
cargo build --bin "$APP_NAME"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="$PROJECT_DIR/target/debug"
BUNDLE_DIR="$PROJECT_DIR/target/bundle"
APP_DIR="$BUNDLE_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
FRAMEWORKS_DIR="$CONTENTS_DIR/Frameworks"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

# Clean previous bundle
rm -rf "$APP_DIR"

# Create directory structure
mkdir -p "$MACOS_DIR" "$FRAMEWORKS_DIR" "$RESOURCES_DIR"

# Copy main executable
cp "$TARGET_DIR/$APP_NAME" "$MACOS_DIR/$APP_NAME"

# Copy CEF framework
echo "Copying CEF framework..."
FRAMEWORK_SRC="$CEF_DIR/Release/Chromium Embedded Framework.framework"
if [ ! -d "$FRAMEWORK_SRC" ]; then
    echo "ERROR: CEF framework not found at $FRAMEWORK_SRC"
    exit 1
fi
cp -R "$FRAMEWORK_SRC" "$FRAMEWORKS_DIR/"

# Copy CEF resources
if [ -d "$CEF_DIR/Resources" ]; then
    cp -R "$CEF_DIR/Resources/"* "$RESOURCES_DIR/" 2>/dev/null || true
fi

# Create Info.plist for main app
cat > "$CONTENTS_DIR/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$DISPLAY_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleDisplayName</key>
    <string>$DISPLAY_NAME</string>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSEnvironment</key>
    <dict>
        <key>MallocNanoZone</key>
        <string>0</string>
    </dict>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

# Create helper app bundles
# CEF requires separate helper apps for GPU, Renderer, Plugin, Alerts processes.
# They all use the same binary — CEF distinguishes via --type= argument.
HELPERS=("Helper" "Helper (GPU)" "Helper (Renderer)" "Helper (Plugin)" "Helper (Alerts)")

for HELPER in "${HELPERS[@]}"; do
    HELPER_FULL="$APP_NAME $HELPER"
    HELPER_APP="$FRAMEWORKS_DIR/$HELPER_FULL.app"
    HELPER_MACOS="$HELPER_APP/Contents/MacOS"
    mkdir -p "$HELPER_MACOS" "$HELPER_APP/Contents/Frameworks" "$HELPER_APP/Contents/Resources"

    # Copy same binary as helper
    cp "$TARGET_DIR/$APP_NAME" "$HELPER_MACOS/$HELPER_FULL"

    # Create helper Info.plist
    cat > "$HELPER_APP/Contents/Info.plist" << HPLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$HELPER_FULL</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}.helper</string>
    <key>CFBundleDisplayName</key>
    <string>$HELPER_FULL</string>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>$HELPER_FULL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <string>1</string>
    <key>LSEnvironment</key>
    <dict>
        <key>MallocNanoZone</key>
        <string>0</string>
    </dict>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
HPLIST
done

echo ""
echo "Bundle created at: $APP_DIR"
echo ""
echo "To run:"
echo "  open $APP_DIR"
echo ""
echo "Or run directly:"
echo "  DYLD_FALLBACK_LIBRARY_PATH=\"$FRAMEWORKS_DIR/Chromium Embedded Framework.framework/Libraries\" $MACOS_DIR/$APP_NAME"
