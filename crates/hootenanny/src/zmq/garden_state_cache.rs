//! GardenStateCache - local cache of chaosgarden state for Trustfall queries
//!
//! Instead of sending Trustfall queries to chaosgarden (requiring JSON/GraphQL
//! parsing in the RT audio process), hootenanny fetches raw state snapshots
//! and evaluates queries locally.
//!
//! Cache invalidation strategy:
//! - IOPub events trigger cache invalidation based on event type
//! - Snapshot version numbers detect stale cache
//! - Lazy refresh on next query (no background polling)

use anyhow::{Context, Result};
use hooteproto::garden::{ShellReply, ShellRequest};
use hooteproto::garden_snapshot::{
    AudioInput, AudioOutput, GardenSnapshot, GraphEdge, GraphNode, IOPubEvent, MidiDeviceInfo,
};
use hooteproto::GardenPeer;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, trace, warn};

/// Cache staleness threshold - after this duration, always refresh
const CACHE_TTL: Duration = Duration::from_secs(30);

/// Cached garden state with metadata
pub struct CachedState {
    /// The actual snapshot data
    pub snapshot: GardenSnapshot,
    /// When this snapshot was fetched
    pub fetched_at: Instant,
    /// Whether the cache has been invalidated by IOPub events
    pub invalidated: bool,
}

impl CachedState {
    /// Check if cache is stale (exceeded TTL or invalidated)
    pub fn is_stale(&self) -> bool {
        self.invalidated || self.fetched_at.elapsed() > CACHE_TTL
    }
}

/// Local cache of chaosgarden state for Trustfall query evaluation
pub struct GardenStateCache {
    /// Cached full snapshot
    state: RwLock<Option<CachedState>>,
    /// Peer for fetching fresh state
    peer: Arc<RwLock<Option<GardenPeer>>>,
}

impl GardenStateCache {
    /// Create a new cache sharing a peer with GardenManager
    pub fn new(peer: Arc<RwLock<Option<GardenPeer>>>) -> Self {
        Self {
            state: RwLock::new(None),
            peer,
        }
    }

    /// Get snapshot, fetching from chaosgarden if stale or missing
    pub async fn get_snapshot(&self) -> Result<GardenSnapshot> {
        // Fast path: check if we have a fresh cache
        {
            let cache = self.state.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_stale() {
                    trace!("cache hit, version={}", cached.snapshot.version);
                    return Ok(cached.snapshot.clone());
                }
                debug!(
                    "cache stale: invalidated={}, age={:?}",
                    cached.invalidated,
                    cached.fetched_at.elapsed()
                );
            }
        }

        // Slow path: fetch fresh snapshot
        self.refresh().await
    }

    /// Force refresh of cached state
    pub async fn refresh(&self) -> Result<GardenSnapshot> {
        let peer = self.peer.read().await;
        let peer = peer
            .as_ref()
            .context("not connected to chaosgarden")?;

        debug!("fetching fresh snapshot from chaosgarden");
        let reply = peer
            .request(ShellRequest::GetSnapshot)
            .await
            .context("failed to send GetSnapshot request")?;

        match reply {
            ShellReply::Snapshot { snapshot } => {
                debug!("received snapshot version={}", snapshot.version);

                // Update cache
                let mut cache = self.state.write().await;
                *cache = Some(CachedState {
                    snapshot: snapshot.clone(),
                    fetched_at: Instant::now(),
                    invalidated: false,
                });

                Ok(snapshot)
            }
            ShellReply::Error { error, .. } => {
                anyhow::bail!("chaosgarden error: {}", error)
            }
            other => {
                anyhow::bail!("unexpected reply: {:?}", other)
            }
        }
    }

    /// Get just the graph (nodes + edges), fetching if stale
    pub async fn get_graph(&self) -> Result<(Vec<GraphNode>, Vec<GraphEdge>)> {
        // Try to use cached full snapshot first
        {
            let cache = self.state.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_stale() {
                    return Ok((
                        cached.snapshot.nodes.clone(),
                        cached.snapshot.edges.clone(),
                    ));
                }
            }
        }

        // Fetch lightweight graph-only response
        let peer = self.peer.read().await;
        let peer = peer
            .as_ref()
            .context("not connected to chaosgarden")?;

        let reply = peer
            .request(ShellRequest::GetGraph)
            .await
            .context("failed to send GetGraph request")?;

        match reply {
            ShellReply::GraphSnapshot { nodes, edges } => Ok((nodes, edges)),
            ShellReply::Error { error, .. } => {
                anyhow::bail!("chaosgarden error: {}", error)
            }
            other => {
                anyhow::bail!("unexpected reply: {:?}", other)
            }
        }
    }

    /// Get I/O device state, fetching if stale
    pub async fn get_io_state(
        &self,
    ) -> Result<(Vec<AudioOutput>, Vec<AudioInput>, Vec<MidiDeviceInfo>)> {
        // Try to use cached full snapshot first
        {
            let cache = self.state.read().await;
            if let Some(ref cached) = *cache {
                if !cached.is_stale() {
                    return Ok((
                        cached.snapshot.outputs.clone(),
                        cached.snapshot.inputs.clone(),
                        cached.snapshot.midi_devices.clone(),
                    ));
                }
            }
        }

        // Fetch lightweight I/O-only response
        let peer = self.peer.read().await;
        let peer = peer
            .as_ref()
            .context("not connected to chaosgarden")?;

        let reply = peer
            .request(ShellRequest::GetIOState)
            .await
            .context("failed to send GetIOState request")?;

        match reply {
            ShellReply::IOState {
                outputs,
                inputs,
                midi_devices,
            } => Ok((outputs, inputs, midi_devices)),
            ShellReply::Error { error, .. } => {
                anyhow::bail!("chaosgarden error: {}", error)
            }
            other => {
                anyhow::bail!("unexpected reply: {:?}", other)
            }
        }
    }

    /// Handle IOPub event and invalidate cache if needed
    pub async fn handle_iopub_event(&self, event: &IOPubEvent) {
        if event.invalidates_cache() {
            debug!("invalidating cache due to IOPub event");
            let mut cache = self.state.write().await;
            if let Some(ref mut cached) = *cache {
                cached.invalidated = true;
            }
        } else {
            trace!("IOPub event does not invalidate cache");
        }
    }

    /// Explicitly invalidate the cache
    pub async fn invalidate(&self) {
        let mut cache = self.state.write().await;
        if let Some(ref mut cached) = *cache {
            cached.invalidated = true;
        }
    }

    /// Check if cache is currently valid (for diagnostics)
    pub async fn is_cached(&self) -> bool {
        let cache = self.state.read().await;
        matches!(*cache, Some(ref c) if !c.is_stale())
    }

    /// Get cache statistics (for diagnostics)
    pub async fn stats(&self) -> CacheStats {
        let cache = self.state.read().await;
        match *cache {
            Some(ref cached) => CacheStats {
                has_snapshot: true,
                version: cached.snapshot.version,
                age_secs: cached.fetched_at.elapsed().as_secs_f64(),
                invalidated: cached.invalidated,
                region_count: cached.snapshot.regions.len(),
                node_count: cached.snapshot.nodes.len(),
                edge_count: cached.snapshot.edges.len(),
            },
            None => CacheStats::default(),
        }
    }
}

/// Cache statistics for diagnostics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub has_snapshot: bool,
    pub version: u64,
    pub age_secs: f64,
    pub invalidated: bool,
    pub region_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
}
