//! External I/O for chaosgarden
//!
//! PipeWire integration for hardware audio and MIDI I/O. Feature-gated behind
//! `pipewire` feature for CI and offline rendering use cases.
//!
//! Architecture:
//! - ExternalIOManager owns PipeWire context and manages streams
//! - ExternalInputNode/ExternalOutputNode bridge PipeWire callbacks to graph
//! - Ring buffers enable lock-free communication between RT and non-RT threads

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::primitives::{
    MidiEvent, Node, NodeCapabilities, NodeDescriptor, Port, ProcessContext, ProcessError,
    SignalBuffer, SignalType,
};

/// Direction for MIDI devices
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiDirection {
    Input,
    Output,
}

/// PipeWire audio output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWireOutput {
    pub id: Uuid,
    pub pw_node_id: Option<u32>,
    pub name: String,
    pub channels: u8,
}

/// PipeWire audio input configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWireInput {
    pub id: Uuid,
    pub pw_node_id: Option<u32>,
    pub name: String,
    pub port_pattern: Option<String>,
    pub channels: u8,
}

/// MIDI device configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiDevice {
    pub id: Uuid,
    pub name: String,
    pub direction: MidiDirection,
    pub pw_node_id: Option<u32>,
}

/// Error type for external I/O operations
#[derive(Debug, thiserror::Error)]
pub enum ExternalIOError {
    #[error("PipeWire not available (compile with --features pipewire)")]
    NotAvailable,

    #[error("Failed to initialize PipeWire: {0}")]
    InitFailed(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(Uuid),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Stream error: {0}")]
    StreamError(String),
}

/// Manager for external I/O devices
///
/// When compiled with `pipewire` feature, this manages actual PipeWire streams.
/// Without the feature, methods return `ExternalIOError::NotAvailable`.
pub struct ExternalIOManager {
    outputs: HashMap<Uuid, PipeWireOutput>,
    inputs: HashMap<Uuid, PipeWireInput>,
    midi_devices: HashMap<Uuid, MidiDevice>,
    sample_rate: u32,
    buffer_size: usize,
    active_output_streams: HashMap<Uuid, crate::pipewire_output::PipeWireOutputStream>,
}

impl ExternalIOManager {
    /// Create a new ExternalIOManager
    ///
    /// Without `pipewire` feature, this creates a stub manager that tracks
    /// device configuration but cannot produce actual audio.
    pub fn new(sample_rate: u32, buffer_size: usize) -> Result<Self, ExternalIOError> {
        Ok(Self {
            outputs: HashMap::new(),
            inputs: HashMap::new(),
            midi_devices: HashMap::new(),
            sample_rate,
            buffer_size,
            active_output_streams: HashMap::new(),
        })
    }

    /// Create an audio output stream
    ///
    /// With `pipewire` feature enabled, this creates an actual PipeWire stream
    /// that will output audio to the system's default sink.
    pub fn create_output(&mut self, name: &str, channels: u8) -> Result<Uuid, ExternalIOError> {
        let id = Uuid::new_v4();
        let output = PipeWireOutput {
            id,
            pw_node_id: None,
            name: name.to_string(),
            channels,
        };

        use crate::pipewire_output::{PipeWireOutputConfig, PipeWireOutputStream};

        let config = PipeWireOutputConfig {
            name: name.to_string(),
            sample_rate: self.sample_rate,
            channels: channels as u32,
            latency_frames: self.buffer_size as u32,
        };

        let stream = PipeWireOutputStream::new(config)
            .map_err(|e| ExternalIOError::StreamError(e.to_string()))?;

        self.active_output_streams.insert(id, stream);

        self.outputs.insert(id, output);
        Ok(id)
    }

    /// Create an audio input stream
    pub fn create_input(&mut self, name: &str, channels: u8) -> Result<Uuid, ExternalIOError> {
        let id = Uuid::new_v4();
        let input = PipeWireInput {
            id,
            pw_node_id: None,
            name: name.to_string(),
            port_pattern: None,
            channels,
        };

        // TODO: PipeWire input stream creation

        self.inputs.insert(id, input);
        Ok(id)
    }

    /// Connect an input to specific PipeWire ports matching a pattern
    pub fn connect_input(&mut self, id: Uuid, port_pattern: &str) -> Result<(), ExternalIOError> {
        let input = self
            .inputs
            .get_mut(&id)
            .ok_or(ExternalIOError::DeviceNotFound(id))?;

        input.port_pattern = Some(port_pattern.to_string());

        // TODO: PipeWire port connection

        Ok(())
    }

    /// Register a MIDI device
    pub fn register_midi(
        &mut self,
        name: &str,
        direction: MidiDirection,
    ) -> Result<Uuid, ExternalIOError> {
        let id = Uuid::new_v4();
        let device = MidiDevice {
            id,
            name: name.to_string(),
            direction,
            pw_node_id: None,
        };

        // TODO: PipeWire MIDI device registration

        self.midi_devices.insert(id, device);
        Ok(id)
    }

    /// Get the configured sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the configured buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Iterate over registered outputs
    pub fn outputs(&self) -> impl Iterator<Item = &PipeWireOutput> {
        self.outputs.values()
    }

    /// Iterate over registered inputs
    pub fn inputs(&self) -> impl Iterator<Item = &PipeWireInput> {
        self.inputs.values()
    }

    /// Iterate over registered MIDI devices
    pub fn midi_devices(&self) -> impl Iterator<Item = &MidiDevice> {
        self.midi_devices.values()
    }

    /// Get an output by ID
    pub fn get_output(&self, id: Uuid) -> Option<&PipeWireOutput> {
        self.outputs.get(&id)
    }

    /// Get an input by ID
    pub fn get_input(&self, id: Uuid) -> Option<&PipeWireInput> {
        self.inputs.get(&id)
    }

    /// Check if PipeWire is available (always true now)
    pub fn is_pipewire_available(&self) -> bool {
        true
    }

    /// Create an ExternalOutputNode for use in the graph
    ///
    /// With `pipewire` feature enabled, the returned node shares its ring buffer
    /// with the active PipeWire stream, so audio written to the node will be
    /// output through PipeWire.
    pub fn create_output_node(
        &self,
        output_id: Uuid,
    ) -> Result<ExternalOutputNode, ExternalIOError> {
        let output = self
            .outputs
            .get(&output_id)
            .ok_or(ExternalIOError::DeviceNotFound(output_id))?;

        // If we have an active stream, create a node that shares its ring buffer
        if let Some(stream) = self.active_output_streams.get(&output_id) {
            let mut node = ExternalOutputNode::new(
                output.name.clone(),
                output.channels,
                self.buffer_size,
            );
            // Replace the node's ring buffer with the stream's shared one
            node.set_ring_buffer(stream.ring_buffer());
            node.set_active(true);
            return Ok(node);
        }

        // Fallback: create a standalone node (won't actually output audio)
        Ok(ExternalOutputNode::new(
            output.name.clone(),
            output.channels,
            self.buffer_size,
        ))
    }

    /// Create an ExternalInputNode for use in the graph
    pub fn create_input_node(&self, input_id: Uuid) -> Result<ExternalInputNode, ExternalIOError> {
        let input = self
            .inputs
            .get(&input_id)
            .ok_or(ExternalIOError::DeviceNotFound(input_id))?;

        Ok(ExternalInputNode::new(
            input.name.clone(),
            input.channels,
            self.buffer_size,
        ))
    }

    /// Create a MidiInputNode for use in the graph
    pub fn create_midi_input_node(
        &self,
        device_id: Uuid,
    ) -> Result<MidiInputNode, ExternalIOError> {
        let device = self
            .midi_devices
            .get(&device_id)
            .ok_or(ExternalIOError::DeviceNotFound(device_id))?;

        if device.direction != MidiDirection::Input {
            return Err(ExternalIOError::ConnectionFailed(
                "Device is not an input".to_string(),
            ));
        }

        Ok(MidiInputNode::new(device.name.clone()))
    }
}

/// Simple lock-free ring buffer for RT audio data
///
/// Used to transfer samples between PipeWire callback and graph processing.
/// Single producer, single consumer. Fixed size power-of-2 for efficient modulo.
pub struct RingBuffer {
    data: Vec<f32>,
    capacity: usize,
    write_pos: std::sync::atomic::AtomicUsize,
    read_pos: std::sync::atomic::AtomicUsize,
}

impl RingBuffer {
    /// Create a new ring buffer with the given capacity (rounded up to power of 2)
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
            data: vec![0.0; capacity],
            capacity,
            write_pos: std::sync::atomic::AtomicUsize::new(0),
            read_pos: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Write samples to the buffer. Returns number of samples written.
    pub fn write(&mut self, samples: &[f32]) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);

        let available = self.capacity - (write.wrapping_sub(read));
        let to_write = samples.len().min(available);

        for (i, &sample) in samples.iter().take(to_write).enumerate() {
            let pos = (write + i) & (self.capacity - 1);
            self.data[pos] = sample;
        }

        self.write_pos
            .store(write.wrapping_add(to_write), Ordering::Release);
        to_write
    }

    /// Read samples from the buffer. Returns number of samples read.
    pub fn read(&mut self, output: &mut [f32]) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);

        let available = write.wrapping_sub(read);
        let to_read = output.len().min(available);

        for (i, sample) in output.iter_mut().take(to_read).enumerate() {
            let pos = (read + i) & (self.capacity - 1);
            *sample = self.data[pos];
        }

        self.read_pos
            .store(read.wrapping_add(to_read), Ordering::Release);
        to_read
    }

    /// Number of samples available to read
    pub fn available(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        write.wrapping_sub(read)
    }

    /// Space available for writing
    pub fn space(&self) -> usize {
        self.capacity - self.available()
    }
}

/// Node that outputs audio to external hardware via PipeWire
///
/// Consumes audio from the graph and writes it to a ring buffer that
/// the PipeWire callback reads from.
pub struct ExternalOutputNode {
    descriptor: NodeDescriptor,
    ring_buffer: Arc<Mutex<RingBuffer>>,
    active: AtomicBool,
}

impl ExternalOutputNode {
    pub fn new(name: String, channels: u8, buffer_frames: usize) -> Self {
        let descriptor = NodeDescriptor {
            id: Uuid::new_v4(),
            name: name.clone(),
            type_id: "external.output".to_string(),
            inputs: vec![Port {
                name: "in".to_string(),
                signal_type: SignalType::Audio,
            }],
            outputs: vec![],
            latency_samples: buffer_frames as u64,
            capabilities: NodeCapabilities {
                realtime: true,
                offline: false,
            },
        };

        let ring_capacity = buffer_frames * channels as usize * 4;

        Self {
            descriptor,
            ring_buffer: Arc::new(Mutex::new(RingBuffer::new(ring_capacity))),
            active: AtomicBool::new(false),
        }
    }

    /// Get access to the ring buffer for PipeWire callback
    pub fn ring_buffer(&self) -> Arc<Mutex<RingBuffer>> {
        Arc::clone(&self.ring_buffer)
    }

    /// Replace the ring buffer with a shared one (used by PipeWire integration)
    pub fn set_ring_buffer(&mut self, ring_buffer: Arc<Mutex<RingBuffer>>) {
        self.ring_buffer = ring_buffer;
    }

    /// Mark the node as active (connected to PipeWire)
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    /// Check if the node is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

impl Node for ExternalOutputNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext,
        inputs: &[SignalBuffer],
        _outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        if !self.is_active() {
            return Err(ProcessError::Skipped {
                reason: "output not active",
            });
        }

        let audio = match inputs.first() {
            Some(SignalBuffer::Audio(buf)) => buf,
            _ => {
                return Err(ProcessError::Skipped {
                    reason: "no audio input",
                })
            }
        };

        if let Ok(mut ring) = self.ring_buffer.try_lock() {
            ring.write(&audio.samples);
        }

        Ok(())
    }

    fn reset(&mut self) {
        if let Ok(mut ring) = self.ring_buffer.lock() {
            *ring = RingBuffer::new(ring.capacity);
        }
    }
}

/// Node that captures audio from external hardware via PipeWire
///
/// Reads from a ring buffer that the PipeWire callback writes to,
/// and outputs audio to the graph.
pub struct ExternalInputNode {
    descriptor: NodeDescriptor,
    ring_buffer: Arc<Mutex<RingBuffer>>,
    channels: u8,
    active: AtomicBool,
}

impl ExternalInputNode {
    pub fn new(name: String, channels: u8, buffer_frames: usize) -> Self {
        let descriptor = NodeDescriptor {
            id: Uuid::new_v4(),
            name: name.clone(),
            type_id: "external.input".to_string(),
            inputs: vec![],
            outputs: vec![Port {
                name: "out".to_string(),
                signal_type: SignalType::Audio,
            }],
            latency_samples: buffer_frames as u64,
            capabilities: NodeCapabilities {
                realtime: true,
                offline: false,
            },
        };

        let ring_capacity = buffer_frames * channels as usize * 4;

        Self {
            descriptor,
            ring_buffer: Arc::new(Mutex::new(RingBuffer::new(ring_capacity))),
            channels,
            active: AtomicBool::new(false),
        }
    }

    /// Get access to the ring buffer for PipeWire callback
    pub fn ring_buffer(&self) -> Arc<Mutex<RingBuffer>> {
        Arc::clone(&self.ring_buffer)
    }

    /// Mark the node as active (connected to PipeWire)
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    /// Check if the node is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

impl Node for ExternalInputNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        ctx: &ProcessContext,
        _inputs: &[SignalBuffer],
        outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        if !self.is_active() {
            return Err(ProcessError::Skipped {
                reason: "input not active",
            });
        }

        let audio = match outputs.first_mut() {
            Some(SignalBuffer::Audio(buf)) => buf,
            _ => {
                return Err(ProcessError::Skipped {
                    reason: "no audio output buffer",
                })
            }
        };

        let expected_samples = ctx.buffer_size * self.channels as usize;

        if let Ok(mut ring) = self.ring_buffer.try_lock() {
            if audio.samples.len() < expected_samples {
                audio.samples.resize(expected_samples, 0.0);
            }
            let read = ring.read(&mut audio.samples[..expected_samples]);
            if read < expected_samples {
                audio.samples[read..expected_samples].fill(0.0);
            }
        } else {
            audio.samples.fill(0.0);
        }

        Ok(())
    }

    fn reset(&mut self) {
        if let Ok(mut ring) = self.ring_buffer.lock() {
            *ring = RingBuffer::new(ring.capacity);
        }
    }
}

/// Node that captures MIDI events from external hardware
///
/// Uses a mutex-protected queue that the PipeWire MIDI callback writes to.
/// In a production implementation, this would use a lock-free SPSC queue.
pub struct MidiInputNode {
    descriptor: NodeDescriptor,
    event_queue: Arc<Mutex<Vec<MidiEvent>>>,
    active: AtomicBool,
}

impl MidiInputNode {
    pub fn new(name: String) -> Self {
        let descriptor = NodeDescriptor {
            id: Uuid::new_v4(),
            name: name.clone(),
            type_id: "external.midi_input".to_string(),
            inputs: vec![],
            outputs: vec![Port {
                name: "out".to_string(),
                signal_type: SignalType::Midi,
            }],
            latency_samples: 0,
            capabilities: NodeCapabilities {
                realtime: true,
                offline: false,
            },
        };

        Self {
            descriptor,
            event_queue: Arc::new(Mutex::new(Vec::with_capacity(256))),
            active: AtomicBool::new(false),
        }
    }

    /// Get access to the event queue for PipeWire callback
    pub fn event_queue(&self) -> Arc<Mutex<Vec<MidiEvent>>> {
        Arc::clone(&self.event_queue)
    }

    /// Push a MIDI event from the PipeWire callback
    pub fn push_event(&self, event: MidiEvent) {
        if let Ok(mut queue) = self.event_queue.try_lock() {
            queue.push(event);
        }
    }

    /// Mark the node as active
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    /// Check if the node is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

impl Node for MidiInputNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext,
        _inputs: &[SignalBuffer],
        outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        if !self.is_active() {
            return Err(ProcessError::Skipped {
                reason: "midi input not active",
            });
        }

        let midi = match outputs.first_mut() {
            Some(SignalBuffer::Midi(buf)) => buf,
            _ => {
                return Err(ProcessError::Skipped {
                    reason: "no midi output buffer",
                })
            }
        };

        if let Ok(mut queue) = self.event_queue.try_lock() {
            midi.events.clear();
            midi.events.append(&mut *queue);
            midi.events.sort_by_key(|e| e.frame);
        }

        Ok(())
    }

    fn reset(&mut self) {
        if let Ok(mut queue) = self.event_queue.lock() {
            queue.clear();
        }
    }
}

/// Node that sends MIDI events to external hardware
pub struct MidiOutputNode {
    descriptor: NodeDescriptor,
    event_queue: Arc<Mutex<Vec<MidiEvent>>>,
    active: AtomicBool,
}

impl MidiOutputNode {
    pub fn new(name: String) -> Self {
        let descriptor = NodeDescriptor {
            id: Uuid::new_v4(),
            name: name.clone(),
            type_id: "external.midi_output".to_string(),
            inputs: vec![Port {
                name: "in".to_string(),
                signal_type: SignalType::Midi,
            }],
            outputs: vec![],
            latency_samples: 0,
            capabilities: NodeCapabilities {
                realtime: true,
                offline: false,
            },
        };

        Self {
            descriptor,
            event_queue: Arc::new(Mutex::new(Vec::with_capacity(256))),
            active: AtomicBool::new(false),
        }
    }

    /// Get access to the event queue for PipeWire callback to read
    pub fn event_queue(&self) -> Arc<Mutex<Vec<MidiEvent>>> {
        Arc::clone(&self.event_queue)
    }

    /// Mark the node as active
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    /// Check if the node is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

impl Node for MidiOutputNode {
    fn descriptor(&self) -> &NodeDescriptor {
        &self.descriptor
    }

    fn process(
        &mut self,
        _ctx: &ProcessContext,
        inputs: &[SignalBuffer],
        _outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError> {
        if !self.is_active() {
            return Err(ProcessError::Skipped {
                reason: "midi output not active",
            });
        }

        let midi = match inputs.first() {
            Some(SignalBuffer::Midi(buf)) => buf,
            _ => {
                return Err(ProcessError::Skipped {
                    reason: "no midi input",
                })
            }
        };

        if let Ok(mut queue) = self.event_queue.try_lock() {
            queue.clear();
            queue.extend(midi.events.iter().cloned());
        }

        Ok(())
    }

    fn reset(&mut self) {
        if let Ok(mut queue) = self.event_queue.lock() {
            queue.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{
        AudioBuffer, Beat, MidiBuffer, MidiMessage, ProcessingMode, Sample, TempoMap,
        TransportState,
    };
    use std::sync::Arc;

    fn test_context(buffer_size: usize) -> ProcessContext {
        ProcessContext {
            sample_rate: 48000,
            buffer_size,
            position_samples: Sample::zero(),
            position_beats: Beat::zero(),
            tempo_map: Arc::new(TempoMap::default()),
            mode: ProcessingMode::Offline,
            transport: TransportState::Playing,
        }
    }

    #[test]
    fn test_external_io_manager_creation() {
        let manager = ExternalIOManager::new(48000, 256).unwrap();
        assert_eq!(manager.sample_rate(), 48000);
        assert_eq!(manager.buffer_size(), 256);
    }

    #[test]
    fn test_create_output() {
        let mut manager = ExternalIOManager::new(48000, 256).unwrap();
        let id = manager.create_output("main-out", 2).unwrap();

        let output = manager.get_output(id).unwrap();
        assert_eq!(output.name, "main-out");
        assert_eq!(output.channels, 2);
    }

    #[test]
    fn test_create_input() {
        let mut manager = ExternalIOManager::new(48000, 256).unwrap();
        let id = manager.create_input("mic-in", 1).unwrap();

        let input = manager.get_input(id).unwrap();
        assert_eq!(input.name, "mic-in");
        assert_eq!(input.channels, 1);
    }

    #[test]
    fn test_connect_input() {
        let mut manager = ExternalIOManager::new(48000, 256).unwrap();
        let id = manager.create_input("mic-in", 1).unwrap();

        manager
            .connect_input(id, "alsa_input.*:capture_FL")
            .unwrap();

        let input = manager.get_input(id).unwrap();
        assert_eq!(
            input.port_pattern,
            Some("alsa_input.*:capture_FL".to_string())
        );
    }

    #[test]
    fn test_register_midi_device() {
        let mut manager = ExternalIOManager::new(48000, 256).unwrap();
        let id = manager
            .register_midi("MIDI Controller", MidiDirection::Input)
            .unwrap();

        let device = manager.midi_devices().find(|d| d.id == id).unwrap();
        assert_eq!(device.name, "MIDI Controller");
        assert_eq!(device.direction, MidiDirection::Input);
    }

    #[test]
    fn test_ring_buffer_write_read() {
        let mut ring = RingBuffer::new(16);

        let samples = [1.0, 2.0, 3.0, 4.0];
        let written = ring.write(&samples);
        assert_eq!(written, 4);
        assert_eq!(ring.available(), 4);

        let mut output = [0.0; 4];
        let read = ring.read(&mut output);
        assert_eq!(read, 4);
        assert_eq!(output, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(ring.available(), 0);
    }

    #[test]
    fn test_ring_buffer_wraparound() {
        let mut ring = RingBuffer::new(8);

        let samples = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        ring.write(&samples);

        let mut output = [0.0; 4];
        ring.read(&mut output);

        let more_samples = [7.0, 8.0, 9.0, 10.0];
        let written = ring.write(&more_samples);
        assert_eq!(written, 4);

        let mut all = [0.0; 6];
        let read = ring.read(&mut all);
        assert_eq!(read, 6);
        assert_eq!(all, [5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
    }

    #[test]
    fn test_external_output_node_descriptor() {
        let node = ExternalOutputNode::new("main-out".to_string(), 2, 256);
        let desc = node.descriptor();

        assert_eq!(desc.type_id, "external.output");
        assert_eq!(desc.inputs.len(), 1);
        assert_eq!(desc.outputs.len(), 0);
        assert!(desc.capabilities.realtime);
    }

    #[test]
    fn test_external_output_node_inactive() {
        let mut node = ExternalOutputNode::new("main-out".to_string(), 2, 256);
        let ctx = test_context(256);

        let audio = AudioBuffer::new(256, 2);
        let inputs = vec![SignalBuffer::Audio(audio)];
        let mut outputs = vec![];

        let result = node.process(&ctx, &inputs, &mut outputs);
        assert!(matches!(result, Err(ProcessError::Skipped { .. })));
    }

    #[test]
    fn test_external_output_node_active() {
        let mut node = ExternalOutputNode::new("main-out".to_string(), 2, 256);
        node.set_active(true);

        let ctx = test_context(256);

        let mut audio = AudioBuffer::new(256, 2);
        audio.samples.fill(0.5);
        let inputs = vec![SignalBuffer::Audio(audio)];
        let mut outputs = vec![];

        let result = node.process(&ctx, &inputs, &mut outputs);
        assert!(result.is_ok());

        let ring = node.ring_buffer();
        let ring = ring.lock().unwrap();
        assert!(ring.available() > 0);
    }

    #[test]
    fn test_external_input_node_descriptor() {
        let node = ExternalInputNode::new("mic-in".to_string(), 1, 256);
        let desc = node.descriptor();

        assert_eq!(desc.type_id, "external.input");
        assert_eq!(desc.inputs.len(), 0);
        assert_eq!(desc.outputs.len(), 1);
        assert!(desc.capabilities.realtime);
    }

    #[test]
    fn test_external_input_node_active() {
        let mut node = ExternalInputNode::new("mic-in".to_string(), 2, 256);

        {
            let ring = node.ring_buffer();
            let mut ring = ring.lock().unwrap();
            let samples: Vec<f32> = (0..512).map(|i| i as f32 * 0.001).collect();
            ring.write(&samples);
        }

        node.set_active(true);

        let ctx = test_context(256);
        let inputs = vec![];
        let mut outputs = vec![SignalBuffer::Audio(AudioBuffer::new(256, 2))];

        let result = node.process(&ctx, &inputs, &mut outputs);
        assert!(result.is_ok());

        if let SignalBuffer::Audio(audio) = &outputs[0] {
            assert!((audio.samples[0] - 0.0).abs() < 0.0001);
            assert!((audio.samples[1] - 0.001).abs() < 0.0001);
        }
    }

    #[test]
    fn test_midi_input_node() {
        let mut node = MidiInputNode::new("controller".to_string());
        node.set_active(true);

        node.push_event(MidiEvent {
            frame: 100,
            message: MidiMessage::NoteOn {
                channel: 0,
                pitch: 60,
                velocity: 100,
            },
        });
        node.push_event(MidiEvent {
            frame: 50,
            message: MidiMessage::NoteOn {
                channel: 0,
                pitch: 64,
                velocity: 80,
            },
        });

        let ctx = test_context(256);
        let inputs = vec![];
        let mut outputs = vec![SignalBuffer::Midi(MidiBuffer::new())];

        let result = node.process(&ctx, &inputs, &mut outputs);
        assert!(result.is_ok());

        if let SignalBuffer::Midi(midi) = &outputs[0] {
            assert_eq!(midi.events.len(), 2);
            assert_eq!(midi.events[0].frame, 50);
            assert_eq!(midi.events[1].frame, 100);
        }
    }

    #[test]
    fn test_midi_output_node() {
        let mut node = MidiOutputNode::new("synth-out".to_string());
        node.set_active(true);

        let mut midi_buf = MidiBuffer::new();
        midi_buf.events.push(MidiEvent {
            frame: 0,
            message: MidiMessage::NoteOn {
                channel: 0,
                pitch: 72,
                velocity: 127,
            },
        });

        let ctx = test_context(256);
        let inputs = vec![SignalBuffer::Midi(midi_buf)];
        let mut outputs = vec![];

        let result = node.process(&ctx, &inputs, &mut outputs);
        assert!(result.is_ok());

        let queue = node.event_queue();
        let queue = queue.lock().unwrap();
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_create_output_node_from_manager() {
        let mut manager = ExternalIOManager::new(48000, 256).unwrap();
        let id = manager.create_output("speakers", 2).unwrap();

        let node = manager.create_output_node(id).unwrap();
        assert_eq!(node.descriptor().name, "speakers");
    }

    #[test]
    fn test_create_input_node_from_manager() {
        let mut manager = ExternalIOManager::new(48000, 256).unwrap();
        let id = manager.create_input("microphone", 1).unwrap();

        let node = manager.create_input_node(id).unwrap();
        assert_eq!(node.descriptor().name, "microphone");
    }
}
