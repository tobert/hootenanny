"""
hoot-cli: Command-line interface for testing Hootenanny model services.

Allows testing Orpheus and Beat-this models without the full ZMQ/MCP stack.
Services are run headlessly, with file I/O bridged through CAS.

Usage:
    hoot-cli orpheus generate -o output.mid --temperature 0.8
    hoot-cli orpheus classify -i unknown.mid
    hoot-cli beatthis analyze -i song.wav -o beats.json
"""

import argparse
import asyncio
import json
import logging
import sys
from pathlib import Path
from typing import Any


def setup_logging(verbose: bool) -> None:
    """Configure logging based on verbosity."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s" if verbose else "%(message)s",
    )
    # Quiet down noisy libraries unless verbose
    if not verbose:
        logging.getLogger("hootpy").setLevel(logging.WARNING)


def output_result(result: dict[str, Any], output_file: Path | None, format_json: bool = True) -> None:
    """Output result to file or stdout."""
    if output_file and output_file.suffix == ".json":
        output_file.write_text(json.dumps(result, indent=2))
        print(f"✓ Wrote JSON to {output_file}")
    elif format_json:
        print(json.dumps(result, indent=2))
    else:
        for key, value in result.items():
            print(f"{key}: {value}")


def parse_json_params(json_str: str | None) -> dict[str, Any] | None:
    """
    Parse JSON params string, returning None on error.

    Returns dict on success, None if json_str is None/empty.
    Raises SystemExit with helpful message on parse error.
    """
    if not json_str:
        return None
    try:
        return json.loads(json_str)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in --json-params: {e}", file=sys.stderr)
        sys.exit(1)


def merge_params(json_params: dict[str, Any] | None, cli_params: dict[str, Any]) -> dict[str, Any]:
    """
    Merge JSON params with CLI params. CLI params take precedence.

    Args:
        json_params: Base params from --json-params (or None)
        cli_params: Explicit CLI arguments (override JSON)

    Returns:
        Merged params dict
    """
    result = json_params.copy() if json_params else {}
    # CLI args override JSON - only include non-None values
    for key, value in cli_params.items():
        if value is not None:
            result[key] = value
    return result


# ─────────────────────────────────────────────────────────────────────────────
# Orpheus commands
# ─────────────────────────────────────────────────────────────────────────────


async def orpheus_generate(args: argparse.Namespace) -> int:
    """Generate MIDI from scratch."""
    from orpheus.service import OrpheusService
    from .headless import HeadlessRunner

    runner = HeadlessRunner(OrpheusService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    params = merge_params(json_params, _orpheus_params(args))

    output = Path(args.output) if args.output else None
    result = await runner.run_tool("orpheus_generate", params, output_file=output)

    if output:
        print(f"✓ Generated {result.get('num_tokens', '?')} tokens → {output}")
    else:
        output_result(result, None)

    return 0


async def orpheus_generate_seeded(args: argparse.Namespace) -> int:
    """Generate MIDI using input as style inspiration."""
    from orpheus.service import OrpheusService
    from .headless import HeadlessRunner

    if not args.input:
        print("Error: --input/-i required for seeded generation", file=sys.stderr)
        return 1

    runner = HeadlessRunner(OrpheusService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    cli_params = _orpheus_params(args)
    cli_params["input_file"] = args.input
    params = merge_params(json_params, cli_params)

    output = Path(args.output) if args.output else None
    result = await runner.run_tool("orpheus_generate_seeded", params, output_file=output)

    if output:
        print(f"✓ Generated {result.get('num_tokens', '?')} tokens (seeded) → {output}")
    else:
        output_result(result, None)

    return 0


async def orpheus_continue(args: argparse.Namespace) -> int:
    """Continue an existing MIDI sequence."""
    from orpheus.service import OrpheusService
    from .headless import HeadlessRunner

    if not args.input:
        print("Error: --input/-i required for continue", file=sys.stderr)
        return 1

    runner = HeadlessRunner(OrpheusService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    cli_params = _orpheus_params(args)
    cli_params["input_file"] = args.input
    params = merge_params(json_params, cli_params)

    output = Path(args.output) if args.output else None
    result = await runner.run_tool("orpheus_continue", params, output_file=output)

    if output:
        print(f"✓ Continued with {result.get('num_tokens', '?')} tokens → {output}")
    else:
        output_result(result, None)

    return 0


async def orpheus_bridge(args: argparse.Namespace) -> int:
    """Generate a musical bridge from section_a."""
    from orpheus.service import OrpheusService
    from .headless import HeadlessRunner

    if not args.input:
        print("Error: --input/-i required for bridge (section_a)", file=sys.stderr)
        return 1

    runner = HeadlessRunner(OrpheusService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    cli_params = _orpheus_params(args)
    cli_params["section_a_file"] = args.input
    params = merge_params(json_params, cli_params)

    output = Path(args.output) if args.output else None
    result = await runner.run_tool("orpheus_bridge", params, output_file=output)

    if output:
        print(f"✓ Generated bridge with {result.get('num_tokens', '?')} tokens → {output}")
    else:
        output_result(result, None)

    return 0


async def orpheus_loops(args: argparse.Namespace) -> int:
    """Generate drum/percussion loops."""
    from orpheus.service import OrpheusService
    from .headless import HeadlessRunner

    runner = HeadlessRunner(OrpheusService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    cli_params = _orpheus_params(args)
    if args.input:
        cli_params["seed_file"] = args.input
    params = merge_params(json_params, cli_params)

    output = Path(args.output) if args.output else None
    result = await runner.run_tool("orpheus_loops", params, output_file=output)

    if output:
        print(f"✓ Generated loops with {result.get('num_tokens', '?')} tokens → {output}")
    else:
        output_result(result, None)

    return 0


async def orpheus_classify(args: argparse.Namespace) -> int:
    """Classify MIDI as human vs AI composed."""
    from orpheus.service import OrpheusService
    from .headless import HeadlessRunner

    if not args.input:
        print("Error: --input/-i required for classify", file=sys.stderr)
        return 1

    runner = HeadlessRunner(OrpheusService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    cli_params = {"input_file": args.input}
    params = merge_params(json_params, cli_params)

    result = await runner.run_tool("orpheus_classify", params)

    # Pretty output for classify
    is_human = result.get("is_human", False)
    confidence = result.get("confidence", 0) * 100
    probs = result.get("probabilities", {})

    print(f"Classification: {'🧑 Human' if is_human else '🤖 AI'}")
    print(f"Confidence: {confidence:.1f}%")
    print(f"Probabilities: human={probs.get('human', 0):.3f}, ai={probs.get('ai', 0):.3f}")
    print(f"Tokens analyzed: {result.get('num_tokens', '?')}")

    if args.output:
        output_result(result, Path(args.output))

    return 0


def _orpheus_params(args: argparse.Namespace) -> dict[str, Any]:
    """Extract common Orpheus parameters from args."""
    return {
        "temperature": args.temperature,
        "top_p": args.top_p,
        "max_tokens": args.max_tokens,
    }


# ─────────────────────────────────────────────────────────────────────────────
# Beat-this commands
# ─────────────────────────────────────────────────────────────────────────────


async def beatthis_analyze(args: argparse.Namespace) -> int:
    """Analyze beats and downbeats in audio."""
    from beatthis.service import BeatthisService
    from .headless import HeadlessRunner

    if not args.input:
        print("Error: --input/-i required for analyze", file=sys.stderr)
        return 1

    runner = HeadlessRunner(BeatthisService)
    await runner.initialize()

    # JSON first, then CLI args override
    json_params = parse_json_params(args.json_params)
    cli_params = {"audio_file": args.input}
    params = merge_params(json_params, cli_params)

    result = await runner.run_tool("beatthis_analyze", params)

    # Summary output
    num_beats = result.get("num_beats", 0)
    num_downbeats = result.get("num_downbeats", 0)
    bpm = result.get("bpm")
    duration = result.get("duration_seconds", 0)

    print(f"🥁 Beats: {num_beats}")
    print(f"⬇️  Downbeats: {num_downbeats}")
    print(f"🎵 BPM: {bpm if bpm else 'N/A'}")
    print(f"⏱️  Duration: {duration:.1f}s")

    if args.output:
        output_path = Path(args.output)
        # Remove probability arrays for cleaner output unless requested
        if not args.include_probs:
            result.pop("beat_probs", None)
            result.pop("downbeat_probs", None)
        output_result(result, output_path)

    return 0


# ─────────────────────────────────────────────────────────────────────────────
# CLI setup
# ─────────────────────────────────────────────────────────────────────────────


def add_common_args(parser: argparse.ArgumentParser) -> None:
    """Add common arguments to a subparser."""
    parser.add_argument("-i", "--input", type=str, help="Input file path")
    parser.add_argument("-o", "--output", type=str, help="Output file path")
    parser.add_argument("--json-params", type=str, help="JSON string of additional parameters")


def add_orpheus_args(parser: argparse.ArgumentParser) -> None:
    """Add Orpheus-specific arguments."""
    parser.add_argument("--temperature", type=float, default=1.0, help="Sampling temperature (default: 1.0)")
    parser.add_argument("--top-p", type=float, default=0.95, help="Top-p sampling threshold (default: 0.95)")
    parser.add_argument("--max-tokens", type=int, default=1024, help="Maximum tokens to generate (default: 1024)")


def create_parser() -> argparse.ArgumentParser:
    """Create the argument parser."""
    parser = argparse.ArgumentParser(
        prog="hoot-cli",
        description="Test Hootenanny model services without ZMQ/MCP infrastructure",
    )
    parser.add_argument("-v", "--verbose", action="store_true", help="Enable verbose logging")

    subparsers = parser.add_subparsers(dest="service", help="Service to use")

    # ── Orpheus subcommands ──────────────────────────────────────────────────
    orpheus_parser = subparsers.add_parser("orpheus", help="Orpheus MIDI generation")
    orpheus_subs = orpheus_parser.add_subparsers(dest="command", help="Orpheus command")

    # orpheus generate
    gen_parser = orpheus_subs.add_parser("generate", help="Generate MIDI from scratch")
    add_common_args(gen_parser)
    add_orpheus_args(gen_parser)
    gen_parser.set_defaults(func=orpheus_generate)

    # orpheus generate-seeded
    seeded_parser = orpheus_subs.add_parser("generate-seeded", help="Generate using input as style inspiration")
    add_common_args(seeded_parser)
    add_orpheus_args(seeded_parser)
    seeded_parser.set_defaults(func=orpheus_generate_seeded)

    # orpheus continue
    cont_parser = orpheus_subs.add_parser("continue", help="Continue an existing MIDI sequence")
    add_common_args(cont_parser)
    add_orpheus_args(cont_parser)
    cont_parser.set_defaults(func=orpheus_continue)

    # orpheus bridge
    bridge_parser = orpheus_subs.add_parser("bridge", help="Generate a musical bridge from section_a")
    add_common_args(bridge_parser)
    add_orpheus_args(bridge_parser)
    bridge_parser.set_defaults(func=orpheus_bridge)

    # orpheus loops
    loops_parser = orpheus_subs.add_parser("loops", help="Generate drum/percussion loops")
    add_common_args(loops_parser)
    add_orpheus_args(loops_parser)
    loops_parser.set_defaults(func=orpheus_loops)

    # orpheus classify
    classify_parser = orpheus_subs.add_parser("classify", help="Classify MIDI as human vs AI")
    add_common_args(classify_parser)
    classify_parser.set_defaults(func=orpheus_classify)

    # ── Beat-this subcommands ────────────────────────────────────────────────
    beatthis_parser = subparsers.add_parser("beatthis", help="Beat-this audio analysis")
    beatthis_subs = beatthis_parser.add_subparsers(dest="command", help="Beat-this command")

    # beatthis analyze
    analyze_parser = beatthis_subs.add_parser("analyze", help="Detect beats and downbeats in audio")
    add_common_args(analyze_parser)
    analyze_parser.add_argument(
        "--include-probs", action="store_true",
        help="Include probability arrays in JSON output"
    )
    analyze_parser.set_defaults(func=beatthis_analyze)

    return parser


async def async_main() -> int:
    """Async entry point."""
    parser = create_parser()
    args = parser.parse_args()

    setup_logging(args.verbose)

    if not args.service:
        parser.print_help()
        return 0

    if not hasattr(args, "command") or not args.command:
        # Print help for the service
        if args.service == "orpheus":
            parser.parse_args(["orpheus", "--help"])
        elif args.service == "beatthis":
            parser.parse_args(["beatthis", "--help"])
        return 0

    if not hasattr(args, "func"):
        parser.print_help()
        return 1

    try:
        return await args.func(args)
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    except Exception as e:
        if args.verbose:
            logging.exception("Command failed")
        else:
            print(f"Error: {e}", file=sys.stderr)
        return 1


def main() -> None:
    """CLI entry point."""
    sys.exit(asyncio.run(async_main()))


if __name__ == "__main__":
    main()
