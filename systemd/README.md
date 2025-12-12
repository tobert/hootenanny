# Hootenanny Systemd Units

Systemd user unit files for the Hootenanny music ensemble.

## Services

- **`chaosgarden.service`**: Generative timeline system (IPC sockets)
- **`hootenanny.service`**: Backend - artifacts, CAS, Orpheus, audio graph (port 8082, ZMQ 5580/5581)
- **`holler.service`**: MCP gateway (port 8080)
- **`luanette.service`**: Lua scripting server (ZMQ 5570)

## Prerequisites

Build release binaries:
```bash
cargo build --release
```

## Installation

```bash
# Link unit files
ln -sf $(pwd)/*.service ~/.config/systemd/user/

# Reload systemd
systemctl --user daemon-reload
```

## Usage

```bash
# Start all
systemctl --user start chaosgarden hootenanny holler luanette

# Check status
systemctl --user status chaosgarden hootenanny holler luanette

# View logs
journalctl --user -f -u chaosgarden -u hootenanny -u holler -u luanette

# Stop all
systemctl --user stop chaosgarden hootenanny holler luanette
```

## Notes

- Services are independent - ZMQ handles reconnection
- If luanette starts before holler, it will retry on restart
- Chaosgarden cleans up stale IPC sockets on start
