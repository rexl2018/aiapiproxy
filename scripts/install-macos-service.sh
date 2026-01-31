#!/bin/bash
# ============================================================================
# macOS Service Installation Script for aiapiproxy
# Installs as a user-level LaunchAgent (no sudo required)
# ============================================================================

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SERVICE_NAME="com.aiapiproxy"
PLIST_FILE="$HOME/Library/LaunchAgents/${SERVICE_NAME}.plist"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/aiapiproxy"
LOG_DIR="$HOME/.local/var/log/aiapiproxy"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Functions
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running on macOS
check_macos() {
    if [[ "$(uname)" != "Darwin" ]]; then
        print_error "This script is for macOS only"
        exit 1
    fi
}

# Build the project
build_project() {
    print_info "Building aiapiproxy (release mode)..."
    cd "$PROJECT_DIR"
    cargo build --release
    print_success "Build completed"
}

# Create directories
create_directories() {
    print_info "Creating directories..."
    mkdir -p "$INSTALL_DIR"
    mkdir -p "$CONFIG_DIR"
    mkdir -p "$LOG_DIR"
    mkdir -p "$HOME/Library/LaunchAgents"
    print_success "Directories created"
}

# Install binary (symlink to build output)
install_binary() {
    local SOURCE_BINARY="$PROJECT_DIR/target/release/aiapiproxy"
    local TARGET_BINARY="$INSTALL_DIR/aiapiproxy"
    
    # Check if source binary exists
    if [[ ! -f "$SOURCE_BINARY" ]]; then
        print_error "Binary not found at $SOURCE_BINARY"
        print_error "Please run 'cargo build --release' first"
        exit 1
    fi
    
    print_info "Creating symlink to $INSTALL_DIR..."
    
    # Remove existing file/symlink if exists
    if [[ -e "$TARGET_BINARY" ]] || [[ -L "$TARGET_BINARY" ]]; then
        rm -f "$TARGET_BINARY"
    fi
    
    # Create symlink
    ln -s "$SOURCE_BINARY" "$TARGET_BINARY"
    print_success "Symlink created: $TARGET_BINARY -> $SOURCE_BINARY"
}

# Check/copy config
setup_config() {
    if [[ ! -f "$CONFIG_DIR/aiapiproxy.json" ]]; then
        if [[ -f "$PROJECT_DIR/aiapiproxy.example.json" ]]; then
            print_warn "No config found, copying example config..."
            cp "$PROJECT_DIR/aiapiproxy.example.json" "$CONFIG_DIR/aiapiproxy.json"
            print_warn "Please edit $CONFIG_DIR/aiapiproxy.json with your settings"
        else
            print_warn "No config file found. Please create $CONFIG_DIR/aiapiproxy.json"
        fi
    else
        print_success "Config file already exists"
    fi
}

# Create plist file
create_plist() {
    print_info "Creating LaunchAgent plist..."
    cat > "$PLIST_FILE" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${SERVICE_NAME}</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALL_DIR}/aiapiproxy</string>
    </array>
    
    <key>WorkingDirectory</key>
    <string>${CONFIG_DIR}</string>
    
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
        <key>HOME</key>
        <string>${HOME}</string>
    </dict>
    
    <key>RunAtLoad</key>
    <true/>
    
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>
    
    <key>StandardOutPath</key>
    <string>${LOG_DIR}/stdout.log</string>
    
    <key>StandardErrorPath</key>
    <string>${LOG_DIR}/stderr.log</string>
    
    <key>ThrottleInterval</key>
    <integer>10</integer>
</dict>
</plist>
EOF
    print_success "LaunchAgent plist created"
}

# Load/reload service
load_service() {
    print_info "Loading service..."
    
    # Unload if already loaded
    if launchctl list | grep -q "$SERVICE_NAME"; then
        print_info "Unloading existing service..."
        launchctl unload "$PLIST_FILE" 2>/dev/null || true
    fi
    
    # Load the service
    launchctl load "$PLIST_FILE"
    print_success "Service loaded"
}

# Start service
start_service() {
    print_info "Starting service..."
    launchctl start "$SERVICE_NAME"
    sleep 2
    
    # Check if running
    if launchctl list | grep -q "$SERVICE_NAME"; then
        print_success "Service started successfully!"
    else
        print_error "Service may have failed to start. Check logs:"
        print_info "  tail -f $LOG_DIR/stderr.log"
    fi
}

# Show status
show_status() {
    echo ""
    echo "=========================================="
    echo "  aiapiproxy Service Installation Complete"
    echo "=========================================="
    echo ""
    echo "Service Status:"
    launchctl list "$SERVICE_NAME" 2>/dev/null || echo "  (not found)"
    echo ""
    echo "Useful Commands:"
    echo "  Start:   launchctl start $SERVICE_NAME"
    echo "  Stop:    launchctl stop $SERVICE_NAME"
    echo "  Restart: launchctl stop $SERVICE_NAME && launchctl start $SERVICE_NAME"
    echo "  Status:  launchctl list $SERVICE_NAME"
    echo "  Logs:    tail -f $LOG_DIR/stderr.log"
    echo ""
    echo "Configuration:"
    echo "  Config:  $CONFIG_DIR/aiapiproxy.json"
    echo "  Plist:   $PLIST_FILE"
    echo "  Binary:  $INSTALL_DIR/aiapiproxy -> $(readlink "$INSTALL_DIR/aiapiproxy" 2>/dev/null || echo 'not a symlink')"
    echo ""
    echo "Test the service:"
    echo "  curl http://127.0.0.1:8082/health"
    echo ""
}

# Uninstall function
uninstall() {
    print_info "Uninstalling aiapiproxy service..."
    
    # Stop and unload
    if launchctl list | grep -q "$SERVICE_NAME"; then
        launchctl stop "$SERVICE_NAME" 2>/dev/null || true
        launchctl unload "$PLIST_FILE" 2>/dev/null || true
    fi
    
    # Remove plist
    rm -f "$PLIST_FILE"
    
    # Remove binary/symlink
    rm -f "$INSTALL_DIR/aiapiproxy"
    
    print_success "Service uninstalled (config and logs preserved)"
    print_info "To remove config: rm -rf $CONFIG_DIR"
    print_info "To remove logs: rm -rf $LOG_DIR"
}

# Main
main() {
    echo ""
    echo "=========================================="
    echo "  aiapiproxy macOS Service Installer"
    echo "=========================================="
    echo ""
    
    case "${1:-install}" in
        install)
            check_macos
            build_project
            create_directories
            install_binary
            setup_config
            create_plist
            load_service
            start_service
            show_status
            ;;
        uninstall)
            check_macos
            uninstall
            ;;
        reload)
            check_macos
            build_project
            # No need to reinstall - symlink points to the rebuilt binary
            print_info "Restarting service..."
            launchctl stop "$SERVICE_NAME" 2>/dev/null || true
            sleep 1
            launchctl start "$SERVICE_NAME"
            sleep 2
            show_status
            ;;
        status)
            launchctl list "$SERVICE_NAME" 2>/dev/null || echo "Service not found"
            ;;
        logs)
            tail -f "$LOG_DIR/stderr.log"
            ;;
        *)
            echo "Usage: $0 {install|uninstall|reload|status|logs}"
            echo ""
            echo "Commands:"
            echo "  install   - Build, install, and start the service"
            echo "  uninstall - Stop and remove the service"
            echo "  reload    - Rebuild and restart the service"
            echo "  status    - Show service status"
            echo "  logs      - Tail the log file"
            exit 1
            ;;
    esac
}

main "$@"
