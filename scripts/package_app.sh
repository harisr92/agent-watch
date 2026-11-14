#!/usr/bin/env bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )/.." && pwd )"
cd "$DIR"

echo "🦀 1. Building Rust Core & CLI (aw)..."
cargo build --release

echo "🍎 2. Building Swift macOS App (AgentWatch)..."
cd macOS/AgentWatch
swift build -c release
cd "$DIR"

echo "📦 3. Assembling AgentWatch.app bundle..."
APP_DIR="$DIR/dist/AgentWatch.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

cp "macOS/AgentWatch/.build/release/AgentWatch" "$APP_DIR/Contents/MacOS/AgentWatch"
cp "$DIR/target/release/aw" "$APP_DIR/Contents/MacOS/aw"
cp "macOS/AgentWatch/Resources/AppIcon.icns" "$APP_DIR/Contents/Resources/AppIcon.icns"

cat << 'EOF' > "$APP_DIR/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>AgentWatch</string>
    <key>CFBundleDisplayName</key>
    <string>Agent Watch</string>
    <key>CFBundleIdentifier</key>
    <string>com.agentwatch.mac</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>AgentWatch</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

chmod +x "$APP_DIR/Contents/MacOS/AgentWatch"
chmod +x "$APP_DIR/Contents/MacOS/aw"

echo "✅ Successfully created $APP_DIR"
echo "👉 You can run it with: open $APP_DIR"
echo "👉 Or install everything (app, CLI, login service):"
echo "   ./scripts/install.sh"
