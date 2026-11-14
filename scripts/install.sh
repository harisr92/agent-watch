#!/usr/bin/env bash
set -e

# Resolve symlinks (./install.sh at the repo root links here) before finding the repo root.
SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}")"
DIR="$( cd "$( dirname "$SCRIPT_PATH" )/.." && pwd )"
cd "$DIR"

MODE="app"
SKIP_BUILD=false
NO_START=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --daemon-only)
            MODE="daemon"
            shift
            ;;
        --build)
            # Default behaviour now; kept for backwards compatibility.
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --no-start)
            NO_START=true
            shift
            ;;
        -h|--help)
            echo "Usage: ./scripts/install.sh [OPTIONS]"
            echo ""
            echo "Installs Agent Watch to /Applications, symlinks CLI to ~/.local/bin/aw,"
            echo "and configures a LaunchAgent so it starts automatically at login."
            echo ""
            echo "Options:"
            echo "  --skip-build   Reuse the existing dist/AgentWatch.app instead of rebuilding"
            echo "  --daemon-only  Configure launchd for headless daemon instead of Menu Bar app"
            echo "  --no-start     Install and configure without launching immediately"
            echo "  -h, --help     Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Run with --help for usage information."
            exit 1
            ;;
    esac
done

echo "🚀 Installing Agent Watch..."

# 1. Build and package. Always rebuild so the installed app matches the source
#    tree; cargo and swift build incrementally, so this is quick when nothing changed.
if [ "$SKIP_BUILD" = true ] && [ -d "$DIR/dist/AgentWatch.app" ]; then
    echo "⏭️  Skipping build, reusing existing dist/AgentWatch.app"
else
    echo "📦 Building and packaging AgentWatch.app..."
    "$DIR/scripts/package_app.sh"
fi

# 2. Stop any existing running instances before copying (avoids macOS code-sign cache issue)
echo "🛑 Stopping any running Agent Watch instances..."
pkill -x AgentWatch 2>/dev/null || true
pkill -f "aw daemon" 2>/dev/null || true
sleep 0.5

# 3. Copy AgentWatch.app to /Applications
echo "📂 Installing to /Applications/AgentWatch.app..."
rm -rf /Applications/AgentWatch.app
cp -R "$DIR/dist/AgentWatch.app" /Applications/

# 4. Create ~/.local/bin/aw symlink
echo "🔗 Creating symlink ~/.local/bin/aw -> /Applications/AgentWatch.app/Contents/MacOS/aw..."
mkdir -p "$HOME/.local/bin"
ln -sf /Applications/AgentWatch.app/Contents/MacOS/aw "$HOME/.local/bin/aw"

# 5. Setup LaunchAgent plist (starts at login)
echo "⚙️  Setting up LaunchAgent service (start at login)..."
LAUNCH_AGENTS_DIR="$HOME/Library/LaunchAgents"
mkdir -p "$LAUNCH_AGENTS_DIR"

if [ "$MODE" = "daemon" ]; then
    PLIST_NAME="com.agentwatch.daemon.plist"
    LABEL="com.agentwatch.daemon"
else
    PLIST_NAME="com.agentwatch.app.plist"
    LABEL="com.agentwatch.app"
fi

TARGET_PLIST="$LAUNCH_AGENTS_DIR/$PLIST_NAME"

# Unload previous service instance if loaded
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || launchctl unload "$TARGET_PLIST" 2>/dev/null || true

cp "$DIR/scripts/$PLIST_NAME" "$TARGET_PLIST"
chmod 644 "$TARGET_PLIST"

# Bootstrap / load into launchd
if launchctl bootstrap "gui/$(id -u)" "$TARGET_PLIST" 2>/dev/null; then
    echo "   ✅ Bootstrapped service '$LABEL' into launchd"
else
    launchctl load -w "$TARGET_PLIST" 2>/dev/null || true
    echo "   ✅ Loaded service '$LABEL' into launchd"
fi

# 6. Launch immediately unless --no-start was given
if [ "$NO_START" = false ]; then
    if [ "$MODE" = "app" ]; then
        echo "✨ Launching Agent Watch in Menu Bar..."
        open -a /Applications/AgentWatch.app
    else
        echo "✨ Starting daemon service..."
        launchctl kickstart -k "gui/$(id -u)/$LABEL" 2>/dev/null || true
    fi
fi

# 7. Check PATH
case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *)
        echo ""
        echo "⚠️  PATH notice: ~/.local/bin is not in your current PATH."
        echo "   To run 'aw' directly from any terminal, add this to your ~/.zshrc or ~/.bashrc:"
        echo "     export PATH=\"\$HOME/.local/bin:\$PATH\""
        ;;
esac

echo ""
echo "🎉 Agent Watch installation complete!"
echo "   • Application: /Applications/AgentWatch.app"
echo "   • CLI binary:  ~/.local/bin/aw"
echo "   • LaunchAgent: $TARGET_PLIST"
echo "   • Service:     Starts automatically at login"
echo ""
echo "Try running an agent test:"
echo "   aw run python3 scripts/mock_menu_agent.py"
