#!/usr/bin/env python3
"""Generate systemd user units for hootenanny Python model services.

Usage:
    ./bin/gen-systemd.py clap              # Print unit to stdout
    ./bin/gen-systemd.py --all             # Print all units
    ./bin/gen-systemd.py --all -o systemd/generated/  # Write all to directory
    ./bin/gen-systemd.py --list            # List available services
    ./bin/gen-systemd.py --verify          # Verify all units with systemd-analyze
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path
from textwrap import dedent

# Service definitions in startup order (small → large).
# (service_dir, module, env_var, description)
SERVICES: list[tuple[str, str, str, str]] = [
    ("midi-role-classifier", "midi_role_classifier", "MIDI_ROLE_ENDPOINT", "MIDI Voice Role Classifier"),
    ("beatthis", "beatthis", "BEATTHIS_ENDPOINT", "Beat and Downbeat Detection"),
    ("clap", "clap", "CLAP_ENDPOINT", "CLAP Audio Analysis and Embeddings"),
    ("orpheus", "orpheus", "ORPHEUS_ENDPOINT", "Orpheus MIDI Generation"),
    ("anticipatory", "anticipatory", "ANTICIPATORY_ENDPOINT", "Anticipatory Music Transformer"),
    ("rave", "rave", "RAVE_ENDPOINT", "RAVE Audio Codec"),
    ("demucs", "demucs_service", "DEMUCS_ENDPOINT", "Demucs Audio Source Separation"),
    ("audioldm2", "audioldm2", "AUDIOLDM2_ENDPOINT", "AudioLDM2 Text-to-Audio"),
    ("musicgen", "musicgen", "MUSICGEN_ENDPOINT", "MusicGen Text-to-Music"),
    ("yue", "yue_service", "YUE_ENDPOINT", "YuE Lyrics-to-Song"),
]

UNIT_TEMPLATE = """\
[Unit]
Description=Hootenanny: {description}
After=network.target{after_clause}
Wants=hootenanny.service
StartLimitBurst=3
StartLimitIntervalSec=300

[Service]
Type=simple
WorkingDirectory=%h/src/hootenanny/python/services/{service_dir}
ExecStartPre=/usr/bin/mkdir -p %t/hootenanny
ExecStart={uv_path} run python -m {module}.service
Slice=hootenanny-models.slice

Restart=on-failure
RestartSec=10
RestartMaxDelaySec=300
RestartSteps=5

Environment=PYTHONUNBUFFERED=1
Environment=CUDA_VISIBLE_DEVICES=0

StandardOutput=journal
StandardError=journal
SyslogIdentifier=hoot-{service_dir}

NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=default.target
"""

SLICE_TEMPLATE = """\
[Unit]
Description=Hootenanny Python Model Services

[Slice]
# Soft limit — kernel reclaims aggressively above this
MemoryHigh=12G
# Hard limit — protect system RAM
MemoryMax=28G
# Let models swap rather than OOM
MemorySwapMax=64G
"""


def find_uv() -> Path:
    """Find the uv binary path."""
    for candidate in [
        Path.home() / ".local" / "bin" / "uv",
        Path.home() / ".cargo" / "bin" / "uv",
        Path("/usr/local/bin/uv"),
        Path("/usr/bin/uv"),
    ]:
        if candidate.exists():
            return candidate
    return Path("uv")


def generate_unit(index: int, service_dir: str, module: str, env_var: str,
                  description: str, uv_path: Path) -> str:
    """Generate a systemd unit file for a service."""
    if index == 0:
        after_clause = ""
    else:
        prev_dir = SERVICES[index - 1][0]
        after_clause = f" hoot-{prev_dir}.service"

    return UNIT_TEMPLATE.format(
        description=description,
        service_dir=service_dir,
        module=module,
        uv_path=uv_path,
        after_clause=after_clause,
    )


def service_unit_name(service_dir: str) -> str:
    return f"hoot-{service_dir}.service"


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate systemd user units for hootenanny Python model services",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=dedent("""\
            Examples:
              %(prog)s clap                          # Print clap unit to stdout
              %(prog)s --all                         # Print all units to stdout
              %(prog)s --all -o systemd/generated/   # Write all units to directory
              %(prog)s --list                        # List services in startup order
              %(prog)s --verify                      # Verify units with systemd-analyze
        """),
    )
    parser.add_argument("service", nargs="?", help="Service name to generate unit for")
    parser.add_argument("--all", action="store_true", help="Generate units for all services")
    parser.add_argument("--list", action="store_true", help="List available services and exit")
    parser.add_argument("--verify", action="store_true", help="Verify units with systemd-analyze")
    parser.add_argument("-o", "--output-dir", type=Path, help="Write units to directory")

    args = parser.parse_args()

    service_names = [s[0] for s in SERVICES]

    if args.list:
        print("Available services (startup order, small → large):")
        for i, (sdir, mod, env, desc) in enumerate(SERVICES, 1):
            print(f"  {i:2}. {sdir:25} module={mod:25} env={env}")
        return 0

    if args.verify:
        uv_path = find_uv()
        failed = False
        with tempfile.TemporaryDirectory(prefix="systemd-verify-") as tmpdir:
            tmppath = Path(tmpdir)
            print("Verifying systemd units...")
            for i, (sdir, mod, env, desc) in enumerate(SERVICES):
                content = generate_unit(i, sdir, mod, env, desc, uv_path)
                unit_path = tmppath / service_unit_name(sdir)
                unit_path.write_text(content)
                result = subprocess.run(
                    ["systemd-analyze", "verify", "--user", str(unit_path)],
                    capture_output=True, text=True,
                )
                # Filter out "not-found" warnings for dependencies that aren't installed in verify mode
                stderr_lines = [
                    l for l in result.stderr.strip().splitlines()
                    if "not-found" not in l and l.strip()
                ]
                if result.returncode != 0 and stderr_lines:
                    print(f"  ✗ {sdir}")
                    for line in stderr_lines:
                        print(f"    {line}")
                    failed = True
                else:
                    print(f"  ✓ {sdir}")

            # Also verify slice
            slice_path = tmppath / "hootenanny-models.slice"
            slice_path.write_text(SLICE_TEMPLATE)
            result = subprocess.run(
                ["systemd-analyze", "verify", "--user", str(slice_path)],
                capture_output=True, text=True,
            )
            stderr_lines = [
                l for l in result.stderr.strip().splitlines()
                if "not-found" not in l and l.strip()
            ]
            if result.returncode != 0 and stderr_lines:
                print(f"  ✗ hootenanny-models.slice")
                failed = True
            else:
                print(f"  ✓ hootenanny-models.slice")

        return 1 if failed else 0

    if not args.all and not args.service:
        parser.error("Either specify a service name or use --all")

    if args.service and args.service not in service_names:
        print(f"Error: Unknown service '{args.service}'", file=sys.stderr)
        print("Use --list to see available services", file=sys.stderr)
        return 1

    uv_path = find_uv()

    if args.all:
        indices = range(len(SERVICES))
    else:
        indices = [service_names.index(args.service)]

    # Write slice if --all
    if args.all:
        if args.output_dir:
            args.output_dir.mkdir(parents=True, exist_ok=True)
            path = args.output_dir / "hootenanny-models.slice"
            path.write_text(SLICE_TEMPLATE)
            print(f"  {path}")
        else:
            print("# === hootenanny-models.slice ===")
            print(SLICE_TEMPLATE)

    for i in indices:
        sdir, mod, env, desc = SERVICES[i]
        content = generate_unit(i, sdir, mod, env, desc, uv_path)
        if args.output_dir:
            args.output_dir.mkdir(parents=True, exist_ok=True)
            path = args.output_dir / service_unit_name(sdir)
            path.write_text(content)
            print(f"  {path}")
        else:
            if args.all:
                print(f"# === {service_unit_name(sdir)} ===")
            print(content)

    return 0


if __name__ == "__main__":
    sys.exit(main())
