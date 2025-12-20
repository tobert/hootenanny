-- Vibeweaver SQLite Schema

-- Session metadata
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    vibe TEXT,
    tempo_bpm REAL NOT NULL DEFAULT 120.0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Scheduled rules (callbacks + latent jobs)
CREATE TABLE IF NOT EXISTS rules (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    trigger_type TEXT NOT NULL,
    trigger_params TEXT NOT NULL,
    action_type TEXT NOT NULL,
    action_params TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'normal',
    enabled INTEGER NOT NULL DEFAULT 1,
    one_shot INTEGER NOT NULL DEFAULT 0,
    fired_count INTEGER NOT NULL DEFAULT 0,
    last_fired_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rules_session ON rules(session_id);
CREATE INDEX IF NOT EXISTS idx_rules_trigger ON rules(trigger_type, enabled);

-- Timeline markers
CREATE TABLE IF NOT EXISTS markers (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    beat REAL NOT NULL,
    name TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_markers_session_beat ON markers(session_id, beat);

-- History (for context restoration)
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    params TEXT,
    result TEXT,
    success INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_history_session ON history(session_id, created_at DESC);

-- KernelState snapshots (Cap'n Proto, for fast restart)
CREATE TABLE IF NOT EXISTS kernel_snapshots (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    state_capnp BLOB NOT NULL,
    captured_at TEXT NOT NULL
);

-- Generation timing stats (for deadline estimation)
CREATE TABLE IF NOT EXISTS generation_stats (
    space TEXT PRIMARY KEY,
    avg_duration_ms INTEGER NOT NULL,
    sample_count INTEGER NOT NULL DEFAULT 1,
    last_updated TEXT NOT NULL
);
