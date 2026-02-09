"""
ML model for MIDI voice role classification.

Uses a GradientBoostingClassifier trained on feature vectors extracted
by the Rust heuristic classifier. The model file is loaded from disk
if available; otherwise falls back to a simple rule-based approach
that mirrors the Rust heuristic.
"""

from __future__ import annotations

import logging
import os
from pathlib import Path
from typing import Any

import numpy as np

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

        Returns a list of dicts with 'role', 'confidence', and 'alternatives'.
        """
        if not features:
            return []

        X = features_to_array(features)

        if self.model is not None and self.is_loaded:
            return self._predict_ml(X)
        else:
            return self._predict_fallback(X, features)

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

    def _predict_fallback(
        self, X: np.ndarray, features: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        """Simple fallback when no trained model is available.

        This mirrors the Rust heuristic but runs in Python.
        It's primarily useful for testing the ML pipeline end-to-end
        before training data is available.
        """
        predictions = []
        for i, feat in enumerate(features):
            role, confidence, alternatives = _heuristic_classify(feat)
            predictions.append({
                "role": role,
                "confidence": confidence,
                "alternatives": alternatives,
            })
        return predictions


def _heuristic_classify(
    feat: dict[str, Any],
) -> tuple[str, float, list[dict[str, Any]]]:
    """Python mirror of the Rust heuristic classifier."""
    candidates: list[tuple[str, float]] = []

    is_drum = feat.get("is_drum_channel", False)
    gm_cat = feat.get("gm_program_category", 0)

    if is_drum or gm_cat == 14:
        candidates.append(("percussion", 0.95))
    if gm_cat == 4:
        candidates.append(("bass", 0.85))
    if feat.get("is_lowest_voice") and feat.get("mean_pitch_normalized", 1.0) < 0.378 and feat.get("coverage", 0) > 0.15:
        candidates.append(("bass", 0.75))
    if feat.get("is_highest_voice") and feat.get("coverage", 0) > 0.3 and feat.get("notes_per_beat", 0) > 0.5:
        candidates.append(("melody", 0.70))
    if feat.get("coverage", 0) > 0.4 and feat.get("ioi_std_dev_beats", 1.0) < 0.2 and feat.get("pitch_range_semitones", 128) <= 7:
        candidates.append(("rhythm", 0.65))
    if feat.get("pitch_rank_normalized", 0) > 0.5 and feat.get("coverage", 0) > 0.2 and not feat.get("is_highest_voice") and feat.get("notes_per_beat", 0) > 0.3:
        candidates.append(("countermelody", 0.55))
    if feat.get("polyphonic_fraction", 0) > 0.3 and feat.get("max_simultaneous", 0) >= 3:
        candidates.append(("primary_harmony", 0.60))
    if feat.get("polyphonic_fraction", 0) > 0.1 and feat.get("max_simultaneous", 0) >= 2 and feat.get("coverage", 1.0) < 0.5:
        candidates.append(("secondary_harmony", 0.50))
    if feat.get("coverage", 1.0) < 0.15 and feat.get("notes_per_beat", 1.0) < 0.3:
        candidates.append(("padding", 0.45))

    if not candidates:
        candidates.append(("harmonic_fill", 0.35))

    candidates.sort(key=lambda x: -x[1])
    role, confidence = candidates[0]
    alternatives = [{"role": r, "confidence": c} for r, c in candidates[1:]]

    return role, confidence, alternatives
