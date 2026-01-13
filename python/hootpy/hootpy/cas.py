"""
Content-Addressed Storage utilities for hootpy

Provides read/write access to the hootenanny CAS directory.
Uses BLAKE3 for hashing, matching the Rust implementation.
"""

import os
from pathlib import Path
from typing import Optional

import blake3


def default_cas_dir() -> Path:
    """Get the default CAS directory path"""
    return Path(os.environ.get(
        "HOOTENANNY_CAS_DIR",
        os.path.expanduser("~/.hootenanny/cas")
    ))


def hash_bytes(data: bytes) -> str:
    """Compute BLAKE3 hash of bytes, returns hex string"""
    return blake3.blake3(data).hexdigest()


def cas_path(cas_dir: Path, content_hash: str) -> Path:
    """Get the filesystem path for a CAS hash"""
    # CAS uses first 2 chars as subdirectory for sharding
    return cas_dir / content_hash[:2] / content_hash


def store(data: bytes, cas_dir: Optional[Path] = None) -> str:
    """
    Store bytes in CAS, returns content hash.

    Creates the shard directory if needed.
    Writes atomically using temp file + rename.
    """
    if cas_dir is None:
        cas_dir = default_cas_dir()

    content_hash = hash_bytes(data)
    target_path = cas_path(cas_dir, content_hash)

    # Skip if already exists
    if target_path.exists():
        return content_hash

    # Ensure shard directory exists
    target_path.parent.mkdir(parents=True, exist_ok=True)

    # Write atomically
    temp_path = target_path.with_suffix(".tmp")
    try:
        temp_path.write_bytes(data)
        temp_path.rename(target_path)
    except Exception:
        temp_path.unlink(missing_ok=True)
        raise

    return content_hash


def fetch(content_hash: str, cas_dir: Optional[Path] = None) -> Optional[bytes]:
    """
    Fetch bytes from CAS by hash.

    Returns None if not found.
    """
    if cas_dir is None:
        cas_dir = default_cas_dir()

    target_path = cas_path(cas_dir, content_hash)

    if not target_path.exists():
        return None

    return target_path.read_bytes()


def exists(content_hash: str, cas_dir: Optional[Path] = None) -> bool:
    """Check if content exists in CAS"""
    if cas_dir is None:
        cas_dir = default_cas_dir()

    return cas_path(cas_dir, content_hash).exists()
