"""
ML model for MIDI voice role classification.

Uses a GradientBoostingClassifier trained on feature vectors extracted
by the Rust heuristic classifier. A trained model must be present on disk;
the canonical heuristic fallback lives in Rust (midi-analysis crate).
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Any

import numpy as np

from hootpy import ServiceError

log = logging.getLogger(__name__)

# Feature names in the order expected by the model (must match Rust VoiceFeatures)
FEATURE_NAMES = [
    "mean_pitch_normalized", "pitch_min", "pitch_max",
    "pitch_range_semitones", "pitch_std_dev",
    "coverage", "notes_per_beat", "mean_ioi_beats",
    "ioi_std_dev_beats", "mean_duration_beats",
    "on_beat_fraction", "on_downbeat_fraction",
    "mean_velocity", "velocity_std_dev", "velocity_range",
    "max_simultaneous", "polyphonic_fraction",
    "gm_program_category", "is_drum_channel",
    "pitch_rank_normalized", "is_highest_voice", "is_lowest_voice",
    "coverage_rank_normalized",
]

VOICE_ROLES = [
    "melody", "bass", "countermelody", "harmonic_fill",
    "percussion", "rhythm", "primary_harmony", "secondary_harmony", "padding",
]

MODEL_DIR = os.environ.get(
    "MIDI_ROLE_MODEL_DIR",
    str(Path.home() / ".hootenanny" / "models" / "midi-role"),
)
MODEL_PATH = os.path.join(MODEL_DIR, "classifier.joblib")


def features_to_array(features: list[dict[str, Any]]) -> np.ndarray:
    """Convert a list of feature dicts to a numpy array."""
    rows = []
    for feat in features:
        row = []
        for name in FEATURE_NAMES:
            val = feat.get(name, 0)
            if isinstance(val, bool):
                val = float(val)
            row.append(float(val))
        rows.append(row)
    return np.array(rows, dtype=np.float64)


class RoleClassifierModel:
    """Wraps sklearn model loading and inference."""

    def __init__(self):
        self.model = None
        self.is_loaded = False

    def load(self):
        """Load a trained model from disk, or log that none is available."""
        if os.path.exists(MODEL_PATH):
            try:
                import joblib
                self.model = joblib.load(MODEL_PATH)
                self.is_loaded = True
                log.info(f"Loaded MIDI role classifier model from {MODEL_PATH}")
            except Exception as e:
                log.warning(f"Failed to load model from {MODEL_PATH}: {e}")
                self.model = None
                self.is_loaded = False
        else:
            log.info(
                f"No trained model found at {MODEL_PATH}. "
                "Use train.py to bootstrap from heuristic labels."
            )

    def predict(self, features: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """
        Predict voice roles from feature vectors.

        Requires a trained model. If no model is loaded, raises ServiceError
        so the Rust side can fall back to its canonical heuristic classifier.
        """
        if not features:
            return []

        if self.model is None or not self.is_loaded:
            raise ServiceError(
                message=(
                    "No trained ML model available. "
                    "Rust heuristic classifier should be used as fallback."
                )
            )

        X = features_to_array(features)
        return self._predict_ml(X)

    def _predict_ml(self, X: np.ndarray) -> list[dict[str, Any]]:
        """Predict using the trained sklearn model."""
        predictions = []

        # Get class probabilities
        probas = self.model.predict_proba(X)
        classes = self.model.classes_

        for i in range(len(X)):
            probs = probas[i]
            sorted_indices = np.argsort(-probs)

            best_idx = sorted_indices[0]
            role = classes[best_idx] if best_idx < len(classes) else "harmonic_fill"
            confidence = float(probs[best_idx])

            alternatives = []
            for j in sorted_indices[1:4]:
                if probs[j] > 0.05:
                    alternatives.append({
                        "role": classes[j] if j < len(classes) else "harmonic_fill",
                        "confidence": float(probs[j]),
                    })

            predictions.append({
                "role": role,
                "confidence": confidence,
                "alternatives": alternatives,
            })

        return predictions
