//! Tool timing classification for dispatch layer.
//!
//! This module defines how tools should be dispatched based on their
//! execution characteristics. The dispatch layer uses this to decide:
//! - What timeout to use for gateway polling
//! - Whether to return job_id to client for long operations
//!
//! # Categories
//!
//! - **AsyncShort**: Fast operations or IO-bound. Gateway polls with ~30s timeout.
//! - **AsyncMedium**: GPU inference, ~2 minute timeout. Gateway polls internally.
//! - **AsyncLong**: Long-running (10+ minutes). Gateway returns job_id.
//! - **FireAndForget**: Control commands. Returns ack, errors go to logs.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timing classification for tool dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTiming {
    /// Fast operations or IO-bound (file read/write, ZMQ calls).
    /// Job created, gateway polls with ~30s timeout.
    AsyncShort,

    /// GPU inference or moderate compute.
    /// Job created, gateway polls with ~120s timeout.
    AsyncMedium,

    /// Long-running operation (musicgen, yue).
    /// Job created, gateway returns job_id immediately.
    /// Client manages polling.
    AsyncLong,

    /// Control command (play/stop/seek).
    /// Returns ack or dispatch error.
    /// Execution errors go to logs/iopub, not caller.
    FireAndForget,
}

impl ToolTiming {
    /// Get the timeout duration for gateway polling.
    /// Returns None for FireAndForget and AsyncLong.
    pub fn gateway_timeout(&self) -> Option<Duration> {
        match self {
            ToolTiming::AsyncShort => Some(Duration::from_secs(30)),
            ToolTiming::AsyncMedium => Some(Duration::from_secs(120)),
            ToolTiming::AsyncLong => None, // Client manages
            ToolTiming::FireAndForget => None,
        }
    }

    /// Should gateway return job_id instead of polling?
    pub fn returns_job_id_immediately(&self) -> bool {
        matches!(self, ToolTiming::AsyncLong)
    }

    /// Should a job be created for this tool?
    pub fn creates_job(&self) -> bool {
        matches!(
            self,
            ToolTiming::AsyncShort | ToolTiming::AsyncMedium | ToolTiming::AsyncLong
        )
    }
}

/// Get timing classification for a tool by name.
///
/// This is the source of truth for tool timing behavior.
pub fn tool_timing(name: &str) -> ToolTiming {
    match name {
        // === AsyncShort: Fast operations, I/O bound, or ZMQ calls (~30s timeout) ===
        "abc_parse" | "abc_validate" | "abc_transpose" => ToolTiming::AsyncShort,
        "soundfont_inspect" | "soundfont_preset_inspect" => ToolTiming::AsyncShort,
        "orpheus_classify" => ToolTiming::AsyncShort,
        "garden_status" | "garden_get_regions" | "garden_query" => ToolTiming::AsyncShort,
        "job_status" | "job_list" => ToolTiming::AsyncShort,
        "config_get" => ToolTiming::AsyncShort,
        "graph_find" | "graph_context" | "graph_query" => ToolTiming::AsyncShort,
        "artifact_get" | "artifact_list" => ToolTiming::AsyncShort,
        "cas_inspect" => ToolTiming::AsyncShort,
        "cas_store" | "cas_upload_file" | "cas_get" => ToolTiming::AsyncShort,
        "artifact_upload" => ToolTiming::AsyncShort,
        "abc_to_midi" => ToolTiming::AsyncShort, // Creates artifact (IO)
        "graph_bind" | "graph_tag" | "graph_connect" => ToolTiming::AsyncShort,
        "add_annotation" => ToolTiming::AsyncShort,

        // === AsyncMedium: GPU inference, ~120s ===
        "convert_midi_to_wav" => ToolTiming::AsyncMedium,
        "orpheus_generate" | "orpheus_generate_seeded" => ToolTiming::AsyncMedium,
        "orpheus_continue" | "orpheus_bridge" | "orpheus_loops" => ToolTiming::AsyncMedium,

        // === AsyncLong: Long-running, client manages ===
        "musicgen_generate" => ToolTiming::AsyncLong,
        "yue_generate" => ToolTiming::AsyncLong,
        "clap_analyze" => ToolTiming::AsyncLong,
        "beatthis_analyze" => ToolTiming::AsyncLong,

        // === FireAndForget: Control commands ===
        "garden_play" | "garden_pause" | "garden_stop" => ToolTiming::FireAndForget,
        "garden_seek" | "garden_set_tempo" => ToolTiming::FireAndForget,
        "garden_emergency_pause" => ToolTiming::FireAndForget,
        "garden_create_region" | "garden_delete_region" | "garden_move_region" => {
            ToolTiming::FireAndForget
        }

        // === Utility tools ===
        "job_poll" => ToolTiming::AsyncShort, // Already handles its own timeout
        "job_cancel" => ToolTiming::AsyncShort,
        "event_poll" => ToolTiming::AsyncShort, // Handles own timeout (max 30s)

        // Default: treat unknown as async medium (safer)
        _ => ToolTiming::AsyncMedium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_short_has_30s_timeout() {
        assert_eq!(
            tool_timing("abc_parse").gateway_timeout(),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            tool_timing("garden_status").gateway_timeout(),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            tool_timing("cas_store").gateway_timeout(),
            Some(Duration::from_secs(30))
        );
        assert!(tool_timing("abc_parse").creates_job());
        assert!(tool_timing("cas_store").creates_job());
    }

    #[test]
    fn async_medium_has_120s_timeout() {
        assert_eq!(
            tool_timing("orpheus_generate").gateway_timeout(),
            Some(Duration::from_secs(120))
        );
        assert!(tool_timing("orpheus_generate").creates_job());
    }

    #[test]
    fn async_long_returns_job_id() {
        assert!(tool_timing("musicgen_generate").returns_job_id_immediately());
        assert!(tool_timing("yue_generate").returns_job_id_immediately());
        assert!(tool_timing("musicgen_generate").creates_job());
    }

    #[test]
    fn fire_and_forget_no_job() {
        assert!(!tool_timing("garden_play").creates_job());
        assert_eq!(tool_timing("garden_play").gateway_timeout(), None);
    }

    #[test]
    fn unknown_tools_default_to_async_medium() {
        assert_eq!(tool_timing("unknown_tool"), ToolTiming::AsyncMedium);
    }
}
