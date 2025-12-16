//! Tool timing classification for dispatch layer.
//!
//! This module defines how tools should be dispatched based on their
//! execution characteristics. The dispatch layer uses this to decide:
//! - Whether to create a job or return immediately
//! - What timeout to use for MCP polling
//! - Whether to return job_id to client for long operations
//!
//! # Categories
//!
//! - **Sync**: Pure compute or in-memory queries. No job created.
//! - **AsyncShort**: IO-bound, moderate timeouts. MCP polls internally.
//! - **AsyncMedium**: GPU inference, ~2 minute timeout. MCP polls internally.
//! - **AsyncLong**: Long-running (10+ minutes). MCP returns job_id.
//! - **FireAndForget**: Control commands. Returns ack, errors go to logs.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timing classification for tool dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTiming {
    /// Pure compute or in-memory query.
    /// No job created, immediate return.
    Sync,

    /// IO-bound operation (file read/write).
    /// Job created, MCP polls with ~30s timeout.
    AsyncShort,

    /// GPU inference or moderate compute.
    /// Job created, MCP polls with ~120s timeout.
    AsyncMedium,

    /// Long-running operation (musicgen, yue).
    /// Job created, MCP returns job_id immediately.
    /// Client manages polling.
    AsyncLong,

    /// Control command (play/stop/seek).
    /// Returns ack or dispatch error.
    /// Execution errors go to logs/iopub, not caller.
    FireAndForget,
}

impl ToolTiming {
    /// Get the timeout duration for MCP polling.
    /// Returns None for Sync, FireAndForget, and AsyncLong.
    pub fn mcp_timeout(&self) -> Option<Duration> {
        match self {
            ToolTiming::Sync => None,
            ToolTiming::AsyncShort => Some(Duration::from_secs(30)),
            ToolTiming::AsyncMedium => Some(Duration::from_secs(120)),
            ToolTiming::AsyncLong => None, // Client manages
            ToolTiming::FireAndForget => None,
        }
    }

    /// Should MCP return job_id instead of polling?
    pub fn mcp_returns_job_id(&self) -> bool {
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
        // === Sync: Pure compute, in-memory queries ===
        "abc_parse" | "abc_validate" | "abc_transpose" => ToolTiming::Sync,
        "soundfont_inspect" | "soundfont_preset_inspect" => ToolTiming::Sync,
        "orpheus_classify" => ToolTiming::Sync,
        "garden_status" | "garden_get_regions" | "garden_query" => ToolTiming::Sync,
        "job_status" | "job_list" => ToolTiming::Sync,
        "config_get" => ToolTiming::Sync,
        "graph_find" | "graph_context" | "graph_query" => ToolTiming::Sync,
        "artifact_get" | "artifact_list" => ToolTiming::Sync,
        "cas_inspect" => ToolTiming::Sync,

        // === AsyncShort: IO-bound, ~30s ===
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
        "job_poll" => ToolTiming::Sync, // Already handles its own timeout
        "job_cancel" => ToolTiming::Sync,
        "job_sleep" => ToolTiming::AsyncShort, // Bounded by input

        // Default: treat unknown as async medium (safer)
        _ => ToolTiming::AsyncMedium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_tools_have_no_timeout() {
        assert_eq!(tool_timing("abc_parse").mcp_timeout(), None);
        assert_eq!(tool_timing("garden_status").mcp_timeout(), None);
        assert!(!tool_timing("abc_parse").creates_job());
    }

    #[test]
    fn async_short_has_30s_timeout() {
        assert_eq!(
            tool_timing("cas_store").mcp_timeout(),
            Some(Duration::from_secs(30))
        );
        assert!(tool_timing("cas_store").creates_job());
    }

    #[test]
    fn async_medium_has_120s_timeout() {
        assert_eq!(
            tool_timing("orpheus_generate").mcp_timeout(),
            Some(Duration::from_secs(120))
        );
        assert!(tool_timing("orpheus_generate").creates_job());
    }

    #[test]
    fn async_long_returns_job_id() {
        assert!(tool_timing("musicgen_generate").mcp_returns_job_id());
        assert!(tool_timing("yue_generate").mcp_returns_job_id());
        assert!(tool_timing("musicgen_generate").creates_job());
    }

    #[test]
    fn fire_and_forget_no_job() {
        assert!(!tool_timing("garden_play").creates_job());
        assert_eq!(tool_timing("garden_play").mcp_timeout(), None);
    }

    #[test]
    fn unknown_tools_default_to_async_medium() {
        assert_eq!(tool_timing("unknown_tool"), ToolTiming::AsyncMedium);
    }
}
