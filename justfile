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

# Restart all services (hootenanny, holler, vibeweaver, chaosgarden, rave)
restart-all: build
    systemctl --user restart hootenanny holler vibeweaver chaosgarden rave
    @sleep 2
    @just status

# Show status of all services
status:
    @echo "=== Hootenanny Services ==="
    @systemctl --user is-active hootenanny holler vibeweaver chaosgarden rave 2>/dev/null | paste - - - - - || true
    @systemctl --user list-units hootenanny.service holler.service vibeweaver.service chaosgarden.service rave.service --no-pager --no-legend

# Install systemd unit files (symlinks to repo)
install-services:
    mkdir -p ~/.config/systemd/user
    ln -sf $(pwd)/systemd/hootenanny.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/holler.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/vibeweaver.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/chaosgarden.service ~/.config/systemd/user/
    ln -sf $(pwd)/systemd/rave.service ~/.config/systemd/user/
    systemctl --user daemon-reload
    @echo "Unit files installed. Enable with: systemctl --user enable hootenanny holler vibeweaver chaosgarden rave"

# Enable all services to start on login
enable-services:
    systemctl --user enable hootenanny holler vibeweaver chaosgarden rave

# Disable all services
disable-services:
    systemctl --user disable hootenanny holler vibeweaver chaosgarden rave

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

# Download RAVE models from IRCAM (vintage, percussion, darbouka)
download-rave-models:
    #!/usr/bin/env bash
    set -euo pipefail

    MODELS_DIR="${HOME}/.hootenanny/models/rave"
    mkdir -p "$MODELS_DIR"

    # Models to download (both batch and streaming variants)
    MODELS=("vintage" "percussion" "darbouka")
    BASE_URL="https://play.forum.ircam.fr/rave-vst-api/get_model"

    echo "Downloading RAVE models to $MODELS_DIR..."

    for model in "${MODELS[@]}"; do
        echo "  Downloading ${model}..."
        if [[ ! -f "$MODELS_DIR/${model}.ts" ]]; then
            curl -fsSL "${BASE_URL}/${model}" -o "$MODELS_DIR/${model}.ts"
            echo "    ✓ ${model}.ts"
        else
            echo "    ⏭ ${model}.ts (exists)"
        fi

        # Streaming variant (if available)
        if [[ ! -f "$MODELS_DIR/${model}_streaming.ts" ]]; then
            if curl -fsSL "${BASE_URL}/${model}_streaming" -o "$MODELS_DIR/${model}_streaming.ts" 2>/dev/null; then
                echo "    ✓ ${model}_streaming.ts"
            else
                echo "    ⏭ ${model}_streaming.ts (not available)"
                rm -f "$MODELS_DIR/${model}_streaming.ts"
            fi
        else
            echo "    ⏭ ${model}_streaming.ts (exists)"
        fi
    done

    echo "Done! Models saved to $MODELS_DIR"
    ls -lh "$MODELS_DIR"

# List installed RAVE models
list-rave-models:
    @ls -lh ~/.hootenanny/models/rave/*.ts 2>/dev/null || echo "No RAVE models installed. Run: just download-rave-models"

# Run RAVE service (foreground)
run-rave:
    cd python/services/rave && uv run python -m rave.service

# === Python Services ===

# Sync all Python packages
sync-python:
    cd python && uv sync --all-packages
