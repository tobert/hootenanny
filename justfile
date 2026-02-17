# Hootenanny development commands
# Install just: cargo install just

default:
    @just --list

# All Python model service unit names (startup order)
python_services := "hoot-midi-role-classifier hoot-beatthis hoot-clap hoot-orpheus hoot-anticipatory hoot-rave hoot-demucs hoot-audioldm2 hoot-musicgen hoot-yue"
rust_services := "hootenanny holler vibeweaver chaosgarden"
all_services := rust_services + " " + python_services

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

# Build and restart vibeweaver service
restart-vibeweaver: build
    systemctl --user restart vibeweaver
    @sleep 1
    systemctl --user status vibeweaver --no-pager | head -8

# Build and restart holler service
restart-holler: build
    systemctl --user restart holler
    @sleep 1
    systemctl --user status holler --no-pager | head -8

# Build and restart chaosgarden service
restart-chaosgarden: build
    systemctl --user restart chaosgarden
    @sleep 1
    systemctl --user status chaosgarden --no-pager | head -8

# Restart all services (Rust + Python)
restart-all: build
    systemctl --user restart {{all_services}}
    @sleep 2
    @just status

# Restart just the Python model services
restart-python:
    systemctl --user restart {{python_services}}
    @sleep 2
    @just status

# Show status of all services
status:
    @echo "=== Hootenanny Services ==="
    @systemctl --user list-units {{rust_services}} {{python_services}} --no-pager --no-legend --all

# Generate systemd units for Python model services
gen-systemd:
    python3 bin/gen-systemd.py --all -o systemd/generated/

# Install systemd unit files (symlinks to repo)
install-services: gen-systemd
    mkdir -p ~/.config/systemd/user
    ln -sf $(pwd)/systemd/hootenanny.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/holler.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/vibeweaver.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/chaosgarden.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/generated/hootenanny-models.slice ~/.config/systemd/user/
    for f in $(pwd)/systemd/generated/hoot-*.service; do ln -sf "$f" ~/.config/systemd/user/; done
    systemctl --user daemon-reload
    @echo "Unit files installed. Enable with: just enable-services"

# Enable all services to start on login
enable-services:
    systemctl --user enable {{all_services}}

# Disable all services
disable-services:
    systemctl --user disable {{all_services}}

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

# Full rebuild: cargo build, restart all services, regenerate Python
rebuild: build restart-all
    @sleep 2
    just gen-python

# === RAVE Model Management ===

# Download RAVE models from IRCAM (vintage, percussion)
download-rave-models:
    #!/usr/bin/env bash
    set -euo pipefail

    MODELS_DIR="${HOME}/.hootenanny/models/rave"
    mkdir -p "$MODELS_DIR"

    # Available models from IRCAM (some models have been removed)
    MODELS=("vintage" "percussion")
    BASE_URL="https://play.forum.ircam.fr/rave-vst-api/get_model"

    echo "Downloading RAVE models to $MODELS_DIR..."

    for model in "${MODELS[@]}"; do
        echo "  ${model}..."
        if [[ ! -f "$MODELS_DIR/${model}.ts" ]]; then
            if curl -fL --progress-bar "${BASE_URL}/${model}" -o "$MODELS_DIR/${model}.ts.tmp"; then
                mv "$MODELS_DIR/${model}.ts.tmp" "$MODELS_DIR/${model}.ts"
                echo "    ✓ ${model}.ts ($(du -h "$MODELS_DIR/${model}.ts" | cut -f1))"
            else
                echo "    ✗ ${model}.ts (download failed)"
                rm -f "$MODELS_DIR/${model}.ts.tmp"
            fi
        else
            echo "    ⏭ ${model}.ts (exists)"
        fi
    done

    echo ""
    echo "Done! Models saved to $MODELS_DIR"
    ls -lh "$MODELS_DIR"

# List installed RAVE models
list-rave-models:
    @ls -lh ~/.hootenanny/models/rave/*.ts 2>/dev/null || echo "No RAVE models installed. Run: just download-rave-models"

# === Python Services ===

# Sync all Python packages
sync-python:
    cd python && uv sync --all-packages
