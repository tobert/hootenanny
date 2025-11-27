#!/bin/bash
# Setup virtual MIDI devices for testing
#
# Usage: ./scripts/setup_virmidi.sh
#
# This script loads the snd-virmidi kernel module which creates
# virtual MIDI ports that appear identical to hardware MIDI devices.

set -e

echo "🎹 Setting up virtual MIDI devices..."

# Check if already loaded
if lsmod | grep -q snd_virmidi; then
    echo "✓ snd-virmidi already loaded"

    # Show current ports
    echo ""
    echo "Current virtual MIDI ports:"
    aplaymidi -l 2>/dev/null | grep -i virtual || echo "  (none visible via aplaymidi)"
else
    echo "Loading snd-virmidi module with 4 virtual devices..."

    if ! sudo modprobe snd-virmidi midi_devs=4; then
        echo "❌ Failed to load snd-virmidi"
        echo ""
        echo "Possible causes:"
        echo "  - Module not installed (try: sudo apt install linux-modules-extra-\$(uname -r))"
        echo "  - Running in container without kernel module access"
        echo "  - Need to enable in kernel config"
        exit 1
    fi

    echo "✓ snd-virmidi loaded"
fi

# Verify
echo ""
echo "Available MIDI ports:"
if command -v aplaymidi &> /dev/null; then
    aplaymidi -l | grep -i virtual || echo "  ⚠️  No virtual ports found (this may be normal)"
else
    echo "  (aplaymidi not installed - skipping port listing)"
fi

echo ""
echo "ALSA sequencer clients:"
if [ -f /proc/asound/seq/clients ]; then
    cat /proc/asound/seq/clients | grep -i virtual || echo "  (no virtual clients in /proc/asound/seq/clients)"
else
    echo "  (ALSA sequencer not available)"
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "To run tests:"
echo "  cargo test --package audio-graph-mcp"
echo ""
echo "To unload virtual devices:"
echo "  sudo modprobe -r snd-virmidi"
