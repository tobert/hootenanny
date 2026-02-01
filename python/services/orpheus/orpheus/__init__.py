"""
Orpheus MIDI generation service.

A unified ZMQ service handling all Orpheus tools:
- orpheus_generate: Generate MIDI from scratch
- orpheus_generate_seeded: Generate using MIDI as style seed
- orpheus_continue: Continue an existing MIDI sequence
- orpheus_bridge: Generate musical bridges between sections
- orpheus_loops: Generate drum/percussion loops
- orpheus_classify: Classify MIDI as human vs AI composed
"""

from .service import OrpheusService

__all__ = ["OrpheusService"]
