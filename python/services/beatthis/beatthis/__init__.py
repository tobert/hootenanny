"""
Beat-this beat/downbeat detection service.

Wraps the CPJKU/beat_this model for beat and downbeat detection.

Requirements:
- Audio must be 22050 Hz sample rate (Rust side handles resampling)
- Audio must be mono
- Max duration: 30 seconds
"""

from .service import BeatthisService

__all__ = ["BeatthisService"]
