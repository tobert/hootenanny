# Hootenanny development commands
# Install just: cargo install just

default:
    @just --list

# Build all crates in release mode
build:
    cargo build --release

# Run all tests
test:
    cargo test --all

# Run clippy
lint:
    cargo clippy --all -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Build and restart hootenanny service
restart-hootenanny: build
    systemctl --user restart hootenanny
    @sleep 1
    systemctl --user status hootenanny --no-pager | head -8

# Generate Python client from running hootenanny
# Requires: hootenanny running, uv installed
gen-python broker="tcp://localhost:5580":
    cd python && uv sync --reinstall && uv run hooteproto-gen --broker {{broker}}

# Check if Python client needs regeneration
check-python broker="tcp://localhost:5580":
    cd python && uv run hooteproto-gen --broker {{broker}} --check

# Install Python package in development mode
setup-python:
    cd python && uv sync

# Run Python tests
test-python:
    cd python && uv run pytest

# Full rebuild: cargo build, restart services, regenerate Python
rebuild: build restart-hootenanny
    @sleep 2
    just gen-python
