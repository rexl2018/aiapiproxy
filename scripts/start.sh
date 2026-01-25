#!/bin/bash

# AI API Proxy startup script

set -e

# Color definitions
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Config file path
CONFIG_FILE="${HOME}/.config/aiapiproxy/aiapiproxy.json"

# Print colored messages
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check configuration file
check_config() {
    print_info "Checking configuration..."
    
    if [ ! -f "$CONFIG_FILE" ]; then
        print_error "Configuration file not found: $CONFIG_FILE"
        print_info "Please create the configuration file."
        print_info "You can copy the example file:"
        print_info "  mkdir -p ~/.config/aiapiproxy"
        print_info "  cp aiapiproxy.example.json ~/.config/aiapiproxy/aiapiproxy.json"
        exit 1
    fi
    
    # Check if config is valid JSON
    if ! python3 -c "import json; json.load(open('$CONFIG_FILE'))" 2>/dev/null; then
        if ! jq empty "$CONFIG_FILE" 2>/dev/null; then
            print_error "Configuration file is not valid JSON"
            exit 1
        fi
    fi
    
    print_success "Configuration file found: $CONFIG_FILE"
}

# Check dependencies
check_dependencies() {
    print_info "Checking dependencies..."
    
    if ! command -v cargo &> /dev/null; then
        print_error "Rust/Cargo not installed"
        print_info "Please visit https://rustup.rs/ to install Rust"
        exit 1
    fi
    
    print_success "Dependency check passed"
}

# Build project
build_project() {
    print_info "Building project..."
    
    if [ "$1" = "--release" ] || [ "$1" = "-r" ]; then
        cargo build --release
        print_success "Release build completed"
    else
        cargo build
        print_success "Debug build completed"
    fi
}

# Run project
run_project() {
    print_info "Starting AI API Proxy..."
    
    # Set log level (default: info, use debug for more details)
    export RUST_LOG=${RUST_LOG:-"info"}
    
    # Extract host and port from config for display
    if command -v jq &> /dev/null; then
        HOST=$(jq -r '.server.host // "127.0.0.1"' "$CONFIG_FILE")
        PORT=$(jq -r '.server.port // 8082' "$CONFIG_FILE")
        print_info "Server will listen on: $HOST:$PORT"
    fi
    
    print_info "Log level: $RUST_LOG"
    print_info "Config file: $CONFIG_FILE"
    
    if [ "$1" = "--release" ] || [ "$1" = "-r" ]; then
        ./target/release/aiapiproxy
    else
        cargo run
    fi
}

# Show help information
show_help() {
    echo "AI API Proxy startup script"
    echo ""
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --help, -h        Show this help information"
    echo "  --release, -r     Build and run in release mode"
    echo "  --build-only, -b  Build only, do not run"
    echo "  --check, -c       Check configuration and dependencies only"
    echo "  --debug, -d       Run with debug logging"
    echo ""
    echo "Configuration:"
    echo "  Config file: ~/.config/aiapiproxy/aiapiproxy.json"
    echo ""
    echo "Environment variables:"
    echo "  RUST_LOG          Log level (default: info, use 'debug' for verbose)"
    echo ""
    echo "Examples:"
    echo "  $0                # Start in development mode"
    echo "  $0 --release      # Start in production mode"
    echo "  $0 --debug        # Start with debug logging"
    echo "  $0 --build-only   # Build project only"
}

# Main function
main() {
    print_info "=== AI API Proxy startup script ==="
    
    case "$1" in
        --help|-h)
            show_help
            exit 0
            ;;
        --check|-c)
            check_dependencies
            check_config
            print_success "All checks passed"
            exit 0
            ;;
        --build-only|-b)
            check_dependencies
            build_project "$2"
            print_success "Build completed"
            exit 0
            ;;
        --debug|-d)
            export RUST_LOG="debug"
            check_dependencies
            check_config
            build_project
            run_project
            ;;
        --release|-r)
            check_dependencies
            check_config
            build_project "$1"
            run_project "$1"
            ;;
        "")
            check_dependencies
            check_config
            build_project
            run_project
            ;;
        *)
            print_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
}

# Capture interrupt signals
trap 'print_info "\nStopping server..."; exit 0' INT TERM

# Run main function
main "$@"
