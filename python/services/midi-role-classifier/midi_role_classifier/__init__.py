"""
MIDI voice role classifier service.

Classifies separated MIDI voices into musical roles (melody, bass,
countermelody, etc.) using a trained scikit-learn model. Feature
extraction happens in Rust; this service receives pre-computed
feature vectors.

Falls back gracefully if no trained model is available.
"""

from .service import MidiRoleClassifierService

__all__ = ["MidiRoleClassifierService"]
