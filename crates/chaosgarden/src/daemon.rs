//! GardenDaemon - Real state management for chaosgarden
//!
//! Replaces StubHandler with actual state:
//! - Transport state (playing, position, tempo)
//! - Regions on the timeline
//! - Trustfall queries over graph state
//! - Latent lifecycle management

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tracing::{debug, info, warn};
use trustfall::execute_query;

use crate::ipc::server::Handler;
use crate::ipc::{
    Beat as IpcBeat, ControlReply, ControlRequest, QueryReply,
    QueryRequest, RegionSummary, ShellReply, ShellRequest,
};
use crate::primitives::Behavior;
use crate::{
    Beat, ChaosgardenAdapter, Graph, LatentConfig, LatentManager, Region, TempoMap, Tick,
};

/// Transport state
#[derive(Debug, Clone, Default)]
pub struct TransportState {
    pub playing: bool,
    pub position: Beat,
}

/// Configuration for the daemon
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub auto_approve_tools: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            buffer_size: 256,
            auto_approve_tools: vec![],
        }
    }
}

/// The main daemon state
pub struct GardenDaemon {
    config: DaemonConfig,

    // Transport
    transport: RwLock<TransportState>,
    tempo_map: Arc<TempoMap>,

    // Timeline (Arc<RwLock> for sharing with query adapter)
    regions: Arc<RwLock<Vec<Region>>>,

    // Graph (Arc<RwLock> for sharing with query adapter)
    graph: Arc<RwLock<Graph>>,

    // Latent management
    latent_manager: Arc<LatentManager>,

    // Query adapter (pre-built, shares state refs)
    query_adapter: Option<Arc<ChaosgardenAdapter>>,
}

impl GardenDaemon {
    /// Create a new daemon with default configuration
    pub fn new() -> Self {
        Self::with_config(DaemonConfig::default())
    }

    /// Create a new daemon with custom configuration
    pub fn with_config(config: DaemonConfig) -> Self {
        let tempo_map = Arc::new(TempoMap::new(120.0, Default::default()));
        let regions = Arc::new(RwLock::new(Vec::new()));
        let graph = Arc::new(RwLock::new(Graph::new()));

        let latent_config = LatentConfig {
            auto_approve_tools: config.auto_approve_tools.iter().cloned().collect(),
            default_mix_in: Default::default(),
            max_concurrent_jobs: 4,
        };

        // Create a no-op IOPub publisher for now
        let publisher = Arc::new(NoOpPublisher);
        let latent_manager = Arc::new(LatentManager::new(latent_config, publisher));

        // Build query adapter
        let query_adapter = ChaosgardenAdapter::new(
            Arc::clone(&regions),
            Arc::clone(&graph),
            Arc::clone(&tempo_map),
        ).ok().map(Arc::new);

        Self {
            config,
            transport: RwLock::new(TransportState::default()),
            tempo_map,
            regions,
            graph,
            latent_manager,
            query_adapter,
        }
    }

    // === Transport operations ===

    fn play(&self) {
        let mut transport = self.transport.write().unwrap();
        transport.playing = true;
        info!("Playback started at beat {}", transport.position.0);
    }

    fn pause(&self) {
        let mut transport = self.transport.write().unwrap();
        transport.playing = false;
        info!("Playback paused at beat {}", transport.position.0);
    }

    fn stop(&self) {
        let mut transport = self.transport.write().unwrap();
        transport.playing = false;
        transport.position = Beat(0.0);
        info!("Playback stopped");
    }

    fn seek(&self, beat: Beat) {
        let mut transport = self.transport.write().unwrap();
        transport.position = beat;
        info!("Seeked to beat {}", beat.0);
    }

    fn set_tempo(&self, bpm: f64) {
        // Note: Currently we can't change tempo dynamically since tempo_map is Arc<TempoMap>
        // To support dynamic tempo changes, we'd need Arc<RwLock<TempoMap>> and rebuild the adapter
        // For now, just log the request
        warn!("set_tempo({}) - dynamic tempo changes not yet supported", bpm);
    }

    fn get_transport_state(&self) -> (bool, Beat, f64) {
        let transport = self.transport.read().unwrap();
        let tempo = self.tempo_map.tempo_at(Tick(0));
        (transport.playing, transport.position, tempo)
    }

    // === Region operations ===

    fn get_regions(&self, range: Option<(Beat, Beat)>) -> Vec<RegionSummary> {
        let regions = self.regions.read().unwrap();
        regions
            .iter()
            .filter(|r| {
                if let Some((start, end)) = range {
                    r.position >= start && r.position < end
                } else {
                    true
                }
            })
            .map(|r| {
                // Extract artifact_id from behavior
                let artifact_id = match &r.behavior {
                    Behavior::PlayContent { content_hash, .. } => Some(content_hash.clone()),
                    Behavior::Latent { state, .. } => {
                        state.resolved.as_ref().map(|rc| rc.artifact_id.clone())
                    }
                    _ => None,
                };
                RegionSummary {
                    region_id: r.id,
                    position: IpcBeat(r.position.0),
                    duration: IpcBeat(r.duration.0),
                    is_latent: r.is_latent(),
                    artifact_id,
                }
            })
            .collect()
    }

    // === Query operations ===

    fn execute_query(&self, query: &str, variables: &HashMap<String, serde_json::Value>) -> QueryReply {
        let adapter = match &self.query_adapter {
            Some(a) => Arc::clone(a),
            None => {
                return QueryReply::Error {
                    error: "Query adapter not initialized".to_string(),
                };
            }
        };

        // Convert variables to FieldValue
        let vars: std::collections::BTreeMap<Arc<str>, trustfall::FieldValue> = variables
            .iter()
            .map(|(k, v)| {
                let field_value = json_to_field_value(v);
                (Arc::from(k.as_str()), field_value)
            })
            .collect();

        let schema = adapter.schema_arc();
        match execute_query(&schema, adapter, query, vars) {
            Ok(results) => {
                let rows: Vec<serde_json::Value> = results
                    .take(100)
                    .map(|row| {
                        let obj: serde_json::Map<String, serde_json::Value> = row
                            .into_iter()
                            .map(|(k, v)| (k.to_string(), field_value_to_json(&v)))
                            .collect();
                        serde_json::Value::Object(obj)
                    })
                    .collect();
                QueryReply::Results { rows }
            }
            Err(e) => QueryReply::Error {
                error: e.to_string(),
            },
        }
    }
}

impl Default for GardenDaemon {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for GardenDaemon {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply {
        debug!("shell request: {:?}", req);

        match req {
            // Transport
            ShellRequest::GetTransportState => {
                let (playing, position, tempo) = self.get_transport_state();
                ShellReply::TransportState {
                    playing,
                    position: IpcBeat(position.0),
                    tempo,
                }
            }
            ShellRequest::Play => {
                self.play();
                ShellReply::Ok {
                    result: serde_json::json!({"status": "playing"}),
                }
            }
            ShellRequest::Pause => {
                self.pause();
                ShellReply::Ok {
                    result: serde_json::json!({"status": "paused"}),
                }
            }
            ShellRequest::Stop => {
                self.stop();
                ShellReply::Ok {
                    result: serde_json::json!({"status": "stopped"}),
                }
            }
            ShellRequest::Seek { beat } => {
                self.seek(Beat(beat.0));
                ShellReply::Ok {
                    result: serde_json::json!({"position": beat.0}),
                }
            }
            ShellRequest::SetTempo { bpm } => {
                self.set_tempo(bpm);
                ShellReply::Ok {
                    result: serde_json::json!({"tempo": bpm}),
                }
            }

            // Regions
            ShellRequest::GetRegions { range } => {
                let range = range.map(|(s, e)| (Beat(s.0), Beat(e.0)));
                let regions = self.get_regions(range);
                ShellReply::Regions { regions }
            }
            ShellRequest::GetPendingApprovals => {
                // TODO: Wire to latent_manager
                ShellReply::PendingApprovals { approvals: vec![] }
            }

            // Not yet implemented
            ShellRequest::CreateRegion { .. }
            | ShellRequest::DeleteRegion { .. }
            | ShellRequest::MoveRegion { .. }
            | ShellRequest::UpdateLatentStarted { .. }
            | ShellRequest::UpdateLatentProgress { .. }
            | ShellRequest::UpdateLatentResolved { .. }
            | ShellRequest::UpdateLatentFailed { .. }
            | ShellRequest::ApproveLatent { .. }
            | ShellRequest::RejectLatent { .. }
            | ShellRequest::AddNode { .. }
            | ShellRequest::RemoveNode { .. }
            | ShellRequest::Connect { .. }
            | ShellRequest::Disconnect { .. }
            | ShellRequest::RegisterParticipant { .. }
            | ShellRequest::UpdateParticipant { .. } => {
                warn!("shell request not yet implemented: {:?}", req);
                ShellReply::Error {
                    error: "not yet implemented".to_string(),
                    traceback: None,
                }
            }
        }
    }

    fn handle_control(&self, req: ControlRequest) -> ControlReply {
        info!("control request: {:?}", req);
        match req {
            ControlRequest::Shutdown => ControlReply::ShuttingDown,
            ControlRequest::Interrupt => {
                let was_playing = self.transport.read().unwrap().playing;
                if was_playing {
                    self.pause();
                }
                ControlReply::Interrupted {
                    was_running: if was_playing {
                        "playback".to_string()
                    } else {
                        "nothing".to_string()
                    },
                }
            }
            ControlRequest::EmergencyPause => {
                self.pause();
                ControlReply::Ok
            }
            ControlRequest::DebugDump => {
                let (playing, position, tempo) = self.get_transport_state();
                let regions = self.regions.read().unwrap();
                ControlReply::DebugDump {
                    state: serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "transport": {
                            "playing": playing,
                            "position": position.0,
                            "tempo": tempo,
                        },
                        "regions": regions.len(),
                    }),
                }
            }
        }
    }

    fn handle_query(&self, req: QueryRequest) -> QueryReply {
        debug!("query request: {}", req.query);
        self.execute_query(&req.query, &req.variables)
    }
}

// === Helper types ===

/// No-op IOPub publisher for daemon initialization
struct NoOpPublisher;

impl crate::IOPubPublisher for NoOpPublisher {
    fn publish(&self, _event: crate::LatentEvent) {
        // No-op for now - will wire to actual IOPub socket later
    }
}

/// Convert JSON value to Trustfall FieldValue
fn json_to_field_value(v: &serde_json::Value) -> trustfall::FieldValue {
    match v {
        serde_json::Value::Null => trustfall::FieldValue::Null,
        serde_json::Value::Bool(b) => trustfall::FieldValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                trustfall::FieldValue::Int64(i)
            } else if let Some(u) = n.as_u64() {
                trustfall::FieldValue::Uint64(u)
            } else if let Some(f) = n.as_f64() {
                trustfall::FieldValue::Float64(f)
            } else {
                trustfall::FieldValue::Null
            }
        }
        serde_json::Value::String(s) => trustfall::FieldValue::String(s.clone().into()),
        serde_json::Value::Array(arr) => {
            let items: Vec<_> = arr.iter().map(json_to_field_value).collect();
            trustfall::FieldValue::List(items.into())
        }
        serde_json::Value::Object(_) => {
            // Trustfall doesn't support objects directly
            trustfall::FieldValue::Null
        }
    }
}

/// Convert Trustfall FieldValue to JSON
fn field_value_to_json(v: &trustfall::FieldValue) -> serde_json::Value {
    match v {
        trustfall::FieldValue::Null => serde_json::Value::Null,
        trustfall::FieldValue::Boolean(b) => serde_json::Value::Bool(*b),
        trustfall::FieldValue::Int64(i) => serde_json::Value::Number((*i).into()),
        trustfall::FieldValue::Uint64(u) => serde_json::Value::Number((*u).into()),
        trustfall::FieldValue::Float64(f) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        trustfall::FieldValue::String(s) => serde_json::Value::String(s.to_string()),
        trustfall::FieldValue::List(items) => {
            let arr: Vec<_> = items.iter().map(field_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_state() {
        let daemon = GardenDaemon::new();

        // Initially stopped at beat 0
        let (playing, position, tempo) = daemon.get_transport_state();
        assert!(!playing);
        assert_eq!(position.0, 0.0);
        assert_eq!(tempo, 120.0);

        // Play
        daemon.play();
        let (playing, _, _) = daemon.get_transport_state();
        assert!(playing);

        // Pause
        daemon.pause();
        let (playing, _, _) = daemon.get_transport_state();
        assert!(!playing);

        // Seek
        daemon.seek(Beat(16.0));
        let (_, position, _) = daemon.get_transport_state();
        assert_eq!(position.0, 16.0);

        // Stop resets position
        daemon.stop();
        let (playing, position, _) = daemon.get_transport_state();
        assert!(!playing);
        assert_eq!(position.0, 0.0);
    }

    #[test]
    fn test_set_tempo() {
        let daemon = GardenDaemon::new();

        // Note: Dynamic tempo changes not yet supported
        // This test documents current behavior (tempo stays at initial value)
        daemon.set_tempo(140.0);
        let (_, _, tempo) = daemon.get_transport_state();
        assert_eq!(tempo, 120.0); // Still initial tempo
    }

    #[test]
    fn test_handler_transport() {
        let daemon = GardenDaemon::new();

        // GetTransportState
        let reply = daemon.handle_shell(ShellRequest::GetTransportState);
        match reply {
            ShellReply::TransportState { playing, position, tempo } => {
                assert!(!playing);
                assert_eq!(position.0, 0.0);
                assert_eq!(tempo, 120.0);
            }
            _ => panic!("expected TransportState"),
        }

        // Play
        let reply = daemon.handle_shell(ShellRequest::Play);
        assert!(matches!(reply, ShellReply::Ok { .. }));

        // Verify playing
        let reply = daemon.handle_shell(ShellRequest::GetTransportState);
        match reply {
            ShellReply::TransportState { playing, .. } => assert!(playing),
            _ => panic!("expected TransportState"),
        }
    }
}
