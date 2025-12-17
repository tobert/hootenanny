use anyhow::{Context as _, Result};
use pipewire::{
    context::ContextRc,
    main_loop::MainLoopRc,
    registry::GlobalObject,
    types::ObjectType,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::HintKind;
use super::DeviceFingerprint;

static PIPEWIRE_INIT: Once = Once::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWireNode {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub media_class: Option<String>,
    pub device_bus_path: Option<String>,
    pub alsa_card: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWirePort {
    pub id: u32,
    pub node_id: u32,
    pub name: String,
    pub direction: PortDirection,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortDirection {
    In,
    Out,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWireLink {
    pub id: u32,
    pub output_node_id: u32,
    pub output_port_id: u32,
    pub input_node_id: u32,
    pub input_port_id: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PipeWireSnapshot {
    pub nodes: Vec<PipeWireNode>,
    pub ports: Vec<PipeWirePort>,
    pub links: Vec<PipeWireLink>,
}

/// Events emitted by PipeWire registry changes for hot-plug detection
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    NodeAdded(PipeWireNode),
    NodeRemoved { id: u32 },
    PortAdded(PipeWirePort),
    PortRemoved { id: u32 },
    LinkAdded(PipeWireLink),
    LinkRemoved { id: u32 },
}

/// Live PipeWire listener that keeps snapshot updated and emits events.
///
/// Runs in a blocking task since PipeWire requires single-threaded event loop.
/// Updates the shared snapshot on each registry change and sends events
/// for downstream processing (identity matching, IOPub broadcast).
pub struct PipeWireListener {
    snapshot: Arc<RwLock<PipeWireSnapshot>>,
    event_tx: mpsc::Sender<DeviceEvent>,
}

impl PipeWireListener {
    pub fn new(
        snapshot: Arc<RwLock<PipeWireSnapshot>>,
        event_tx: mpsc::Sender<DeviceEvent>,
    ) -> Self {
        Self { snapshot, event_tx }
    }

    /// Spawn the listener in a blocking task. Returns join handle.
    ///
    /// The listener runs forever, updating the snapshot and emitting events
    /// as devices are added/removed from PipeWire.
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            PIPEWIRE_INIT.call_once(|| {
                pipewire::init();
            });

            let mainloop = match MainLoopRc::new(None) {
                Ok(ml) => ml,
                Err(e) => {
                    warn!("Failed to create PipeWire main loop: {}", e);
                    return;
                }
            };

            let context = match ContextRc::new(&mainloop, None) {
                Ok(ctx) => ctx,
                Err(e) => {
                    warn!("Failed to create PipeWire context: {}", e);
                    return;
                }
            };

            let core = match context.connect_rc(None) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to connect to PipeWire: {}", e);
                    return;
                }
            };

            let registry = match core.get_registry_rc() {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to get PipeWire registry: {}", e);
                    return;
                }
            };

            // Shared state for closures
            let snapshot_add = self.snapshot.clone();
            let snapshot_remove = self.snapshot.clone();
            let event_tx_add = self.event_tx.clone();
            let event_tx_remove = self.event_tx.clone();

            let _listener = registry
                .add_listener_local()
                .global(move |global| {
                    if let Some(event) = process_global_to_event(global) {
                        // Update snapshot
                        let mut snap = snapshot_add.blocking_write();
                        apply_add_event(&mut snap, &event);
                        drop(snap);

                        // Send event (non-blocking, drop if channel full)
                        if event_tx_add.blocking_send(event.clone()).is_err() {
                            debug!("Event channel full, dropping event");
                        }
                    }
                })
                .global_remove(move |id| {
                    // Determine what type was removed and update snapshot
                    let mut snap = snapshot_remove.blocking_write();
                    let event = remove_by_id(&mut snap, id);
                    drop(snap);

                    if let Some(event) = event {
                        if event_tx_remove.blocking_send(event).is_err() {
                            debug!("Event channel full, dropping remove event");
                        }
                    }
                })
                .register();

            debug!("🔌 PipeWire listener started, watching for device changes...");

            // Run forever - mainloop exits when process exits
            mainloop.run();
        })
    }
}

/// Process a PipeWire global object into an add event
fn process_global_to_event<P: AsRef<pipewire::spa::utils::dict::DictRef>>(
    global: &GlobalObject<P>,
) -> Option<DeviceEvent> {
    let props = global.props.as_ref()?.as_ref();

    match global.type_ {
        ObjectType::Node => {
            let node = PipeWireNode {
                id: global.id,
                name: props.get(*pipewire::keys::NODE_NAME)
                    .map(String::from)
                    .unwrap_or_else(|| format!("node-{}", global.id)),
                description: props.get(*pipewire::keys::NODE_DESCRIPTION).map(String::from),
                media_class: props.get(*pipewire::keys::MEDIA_CLASS).map(String::from),
                device_bus_path: props.get(*pipewire::keys::DEVICE_BUS_PATH).map(String::from),
                alsa_card: props.get("alsa.card").map(String::from),
            };
            Some(DeviceEvent::NodeAdded(node))
        }
        ObjectType::Port => {
            let node_id = props.get(*pipewire::keys::NODE_ID)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let direction = match props.get(*pipewire::keys::PORT_DIRECTION) {
                Some("in") => PortDirection::In,
                _ => PortDirection::Out,
            };
            let port = PipeWirePort {
                id: global.id,
                node_id,
                name: props.get(*pipewire::keys::PORT_NAME)
                    .map(String::from)
                    .unwrap_or_else(|| format!("port-{}", global.id)),
                direction,
                media_type: props.get(*pipewire::keys::FORMAT_DSP).map(String::from),
            };
            Some(DeviceEvent::PortAdded(port))
        }
        ObjectType::Link => {
            let link = PipeWireLink {
                id: global.id,
                output_node_id: props.get(*pipewire::keys::LINK_OUTPUT_NODE)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                output_port_id: props.get(*pipewire::keys::LINK_OUTPUT_PORT)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                input_node_id: props.get(*pipewire::keys::LINK_INPUT_NODE)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                input_port_id: props.get(*pipewire::keys::LINK_INPUT_PORT)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            };
            Some(DeviceEvent::LinkAdded(link))
        }
        _ => None,
    }
}

/// Apply an add event to the snapshot
fn apply_add_event(snap: &mut PipeWireSnapshot, event: &DeviceEvent) {
    match event {
        DeviceEvent::NodeAdded(node) => {
            // Remove any existing node with same ID (update case)
            snap.nodes.retain(|n| n.id != node.id);
            snap.nodes.push(node.clone());
        }
        DeviceEvent::PortAdded(port) => {
            snap.ports.retain(|p| p.id != port.id);
            snap.ports.push(port.clone());
        }
        DeviceEvent::LinkAdded(link) => {
            snap.links.retain(|l| l.id != link.id);
            snap.links.push(link.clone());
        }
        _ => {} // Remove events handled separately
    }
}

/// Remove an object by ID from the snapshot, returning the appropriate event
fn remove_by_id(snap: &mut PipeWireSnapshot, id: u32) -> Option<DeviceEvent> {
    // Check nodes first
    if snap.nodes.iter().any(|n| n.id == id) {
        snap.nodes.retain(|n| n.id != id);
        // Also remove associated ports
        snap.ports.retain(|p| p.node_id != id);
        return Some(DeviceEvent::NodeRemoved { id });
    }

    // Check ports
    if snap.ports.iter().any(|p| p.id == id) {
        snap.ports.retain(|p| p.id != id);
        return Some(DeviceEvent::PortRemoved { id });
    }

    // Check links
    if snap.links.iter().any(|l| l.id == id) {
        snap.links.retain(|l| l.id != id);
        return Some(DeviceEvent::LinkRemoved { id });
    }

    None
}

pub struct PipeWireSource {
    available: bool,
}

impl PipeWireSource {
    pub fn new() -> Self {
        PIPEWIRE_INIT.call_once(|| {
            pipewire::init();
        });
        Self { available: true }
    }

    pub fn is_available(&self) -> bool {
        self.available
    }

    pub fn snapshot(&self) -> Result<PipeWireSnapshot> {
        if !self.available {
            anyhow::bail!("PipeWire not available");
        }

        let snapshot = Rc::new(RefCell::new(PipeWireSnapshot::default()));

        let mainloop = MainLoopRc::new(None)
            .context("Failed to create PipeWire main loop")?;
        let context = ContextRc::new(&mainloop, None)
            .context("Failed to create PipeWire context")?;
        let core = context.connect_rc(None)
            .context("Failed to connect to PipeWire")?;
        let registry = core.get_registry_rc()
            .context("Failed to get PipeWire registry")?;

        let snapshot_clone = snapshot.clone();
        let _listener = registry
            .add_listener_local()
            .global(move |global| {
                process_global(&snapshot_clone, global);
            })
            .register();

        let mainloop_weak = mainloop.downgrade();
        let timer = mainloop.loop_().add_timer(move |_| {
            if let Some(mainloop) = mainloop_weak.upgrade() {
                mainloop.quit();
            }
        });
        timer
            .update_timer(Some(Duration::from_millis(100)), None)
            .into_result()
            .context("Failed to set timer")?;

        mainloop.run();

        let result = snapshot.borrow().clone();
        Ok(result)
    }

    pub fn extract_fingerprints(&self, node: &PipeWireNode) -> Vec<DeviceFingerprint> {
        let mut fingerprints = vec![
            DeviceFingerprint {
                kind: HintKind::PipewireName,
                value: node.name.clone(),
            },
        ];

        if let Some(ref bus_path) = node.device_bus_path {
            fingerprints.push(DeviceFingerprint {
                kind: HintKind::PipewireAlsaPath,
                value: bus_path.clone(),
            });
        }

        fingerprints
    }
}

fn process_global<P: AsRef<pipewire::spa::utils::dict::DictRef>>(
    snapshot: &Rc<RefCell<PipeWireSnapshot>>,
    global: &GlobalObject<P>,
) {
    let props = match &global.props {
        Some(p) => p.as_ref(),
        None => return,
    };

    match global.type_ {
        ObjectType::Node => {
            let node = PipeWireNode {
                id: global.id,
                name: props.get(*pipewire::keys::NODE_NAME)
                    .map(String::from)
                    .unwrap_or_else(|| format!("node-{}", global.id)),
                description: props.get(*pipewire::keys::NODE_DESCRIPTION).map(String::from),
                media_class: props.get(*pipewire::keys::MEDIA_CLASS).map(String::from),
                device_bus_path: props.get(*pipewire::keys::DEVICE_BUS_PATH).map(String::from),
                alsa_card: props.get("alsa.card").map(String::from),
            };
            snapshot.borrow_mut().nodes.push(node);
        }
        ObjectType::Port => {
            let node_id = props.get(*pipewire::keys::NODE_ID)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let direction = match props.get(*pipewire::keys::PORT_DIRECTION) {
                Some("in") => PortDirection::In,
                _ => PortDirection::Out,
            };
            let port = PipeWirePort {
                id: global.id,
                node_id,
                name: props.get(*pipewire::keys::PORT_NAME)
                    .map(String::from)
                    .unwrap_or_else(|| format!("port-{}", global.id)),
                direction,
                media_type: props.get(*pipewire::keys::FORMAT_DSP).map(String::from),
            };
            snapshot.borrow_mut().ports.push(port);
        }
        ObjectType::Link => {
            let link = PipeWireLink {
                id: global.id,
                output_node_id: props.get(*pipewire::keys::LINK_OUTPUT_NODE)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                output_port_id: props.get(*pipewire::keys::LINK_OUTPUT_PORT)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                input_node_id: props.get(*pipewire::keys::LINK_INPUT_NODE)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                input_port_id: props.get(*pipewire::keys::LINK_INPUT_PORT)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
            };
            snapshot.borrow_mut().links.push(link);
        }
        _ => {}
    }
}

impl Default for PipeWireSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipewire_source_creation() {
        let source = PipeWireSource::new();
        println!("PipeWire available: {}", source.is_available());
    }

    #[test]
    fn test_snapshot() {
        let source = PipeWireSource::new();
        if !source.is_available() {
            println!("Skipping test - PipeWire not available");
            return;
        }

        match source.snapshot() {
            Ok(snapshot) => {
                println!("Found {} PipeWire nodes:", snapshot.nodes.len());
                for node in &snapshot.nodes {
                    println!("  {} ({:?})", node.name, node.media_class);
                }
                println!("Found {} ports, {} links", snapshot.ports.len(), snapshot.links.len());
            }
            Err(e) => {
                println!("Failed to get snapshot: {}", e);
            }
        }
    }

    #[test]
    fn test_extract_fingerprints() {
        let source = PipeWireSource::new();
        let node = PipeWireNode {
            id: 42,
            name: "JD-Xi".to_string(),
            description: Some("Roland JD-Xi".to_string()),
            media_class: Some("Midi/Bridge".to_string()),
            device_bus_path: Some("pci-0000:00:14.0-usb-0:3.2:1.0".to_string()),
            alsa_card: Some("2".to_string()),
        };

        let fingerprints = source.extract_fingerprints(&node);
        assert_eq!(fingerprints.len(), 2);
        assert_eq!(fingerprints[0].kind, HintKind::PipewireName);
        assert_eq!(fingerprints[0].value, "JD-Xi");
    }
}
