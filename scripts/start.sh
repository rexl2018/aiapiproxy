#!/bin/bash

# AI API Proxy startup script

set -e

# Color definitions
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

# Load environment variables from .env file
load_env() {
    # Load .env file (if exists)
    if [ -f ".env" ]; then
        print_info "Loading .env configuration file"
        export $(cat .env | grep -v '^#' | xargs)
    else
        print_warning ".env file does not exist, using environment variables"
    fi
}

# Check environment variables
check_env() {
    print_info "Checking environment variables..."
    
    if [ -z "$OPENAI_API_KEY" ]; then
        print_error "OPENAI_API_KEY environment variable not set"
        print_info "Please set your OpenAI API key:"
        print_info "export OPENAI_API_KEY=sk-your-api-key-here"
        exit 1
    fi
    
    print_success "Environment variable check passed"
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
    
    if [ "$1" = "--release" ]; then
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
    
    # Set default values
    export SERVER_HOST=${SERVER_HOST:-"0.0.0.0"}
    export SERVER_PORT=${SERVER_PORT:-"8082"}
    export RUST_LOG=${RUST_LOG:-"info"}
    
    print_info "Server configuration:"
    print_info "  Host: $SERVER_HOST"
    print_info "  Port: $SERVER_PORT"
    print_info "  Log level: $RUST_LOG"
    
    if [ "$1" = "--release" ]; then
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
    echo "  --check, -c       Check environment only, do not build or run"
    echo ""
    echo "Environment variables:"
    echo "  OPENAI_API_KEY    OpenAI API key (required)"
    echo "  SERVER_HOST       Server host (default: 0.0.0.0)"
    echo "  SERVER_PORT       Server port (default: 8082)"
    echo "  RUST_LOG          Log level (default: info)"
    echo ""
    echo "Examples:"
    echo "  $0                # Start in development mode"
    echo "  $0 --release      # Start in production mode"
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
            load_env
            check_env
            check_dependencies
            print_success "Environment check completed"
            exit 0
            ;;
        --build-only|-b)
            load_env
            check_env
            check_dependencies
            build_project "$1"
            print_success "Build completed"
            exit 0
            ;;
        --release|-r)
            load_env
            check_env
            check_dependencies
            build_project "$1"
            run_project "$1"
            ;;
        "")
            load_env
            check_env
            check_dependencies
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