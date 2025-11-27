#!/bin/bash
# Install automatic snd-virmidi loading on Arch Linux
# Run as root: sudo ./scripts/install_virmidi_autoload.sh
#
# This creates:
#   /etc/modules-load.d/virmidi.conf - loads module at boot
#   /etc/modprobe.d/virmidi.conf     - sets module options (4 devices)

set -e

if [[ $EUID -ne 0 ]]; then
    echo "❌ This script must be run as root"
    echo "   sudo $0"
    exit 1
fi

echo "🎹 Installing snd-virmidi autoload configuration..."

# Create modules-load.d config (loads module at boot)
cat > /etc/modules-load.d/virmidi.conf << 'EOF'
# Virtual MIDI devices for audio-graph-mcp testing
snd-virmidi
EOF
echo "✓ Created /etc/modules-load.d/virmidi.conf"

# Create modprobe.d config (module options)
cat > /etc/modprobe.d/virmidi.conf << 'EOF'
# snd-virmidi options for audio-graph-mcp
# Creates 4 virtual MIDI devices
options snd-virmidi midi_devs=4
EOF
echo "✓ Created /etc/modprobe.d/virmidi.conf"

# Load the module now if not already loaded
if lsmod | grep -q snd_virmidi; then
    echo "✓ snd-virmidi already loaded"
else
    echo "Loading snd-virmidi module..."
    modprobe snd-virmidi midi_devs=4
    echo "✓ snd-virmidi loaded"
fi

# Verify
echo ""
echo "📋 Current virtual MIDI ports:"
if command -v aplaymidi &> /dev/null; then
    aplaymidi -l 2>/dev/null | grep -i virtual || echo "  (none visible)"
else
    cat /proc/asound/seq/clients 2>/dev/null | grep -i virtual || echo "  (check /proc/asound/seq/clients)"
fi

echo ""
echo "✅ Installation complete!"
echo ""
echo "The snd-virmidi module will now load automatically on boot."
echo ""
echo "To uninstall:"
echo "  sudo rm /etc/modules-load.d/virmidi.conf"
echo "  sudo rm /etc/modprobe.d/virmidi.conf"
echo "  sudo modprobe -r snd-virmidi"
