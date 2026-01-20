//! MIDI I/O via ALSA (through midir)
//!
//! Direct ALSA access for low-latency MIDI, bypassing PipeWire.
//! Provides bidirectional MIDI communication with hardware devices.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use tracing::{debug, info};

use crate::primitives::MidiMessage;

/// Information about a discovered MIDI port
#[derive(Debug, Clone)]
pub struct MidiPortInfo {
    /// Port index (for midir connection)
    pub index: usize,
    /// Port name from ALSA
    pub name: String,
}

/// A timestamped MIDI message received from hardware
#[derive(Debug, Clone)]
pub struct TimestampedMidiMessage {
    /// Timestamp in microseconds (from midir, relative to some epoch)
    pub timestamp_us: u64,
    /// The parsed MIDI message
    pub message: MidiMessage,
    /// Raw bytes (for forwarding or logging)
    pub raw: Vec<u8>,
}

/// Error type for MIDI operations
#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    #[error("Failed to initialize MIDI: {0}")]
    InitFailed(String),

    #[error("Port not found: {0}")]
    PortNotFound(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Port already connected: {0}")]
    AlreadyConnected(String),

    #[error("Partial send failure: {0} of {1} outputs failed")]
    PartialSendFailure(usize, usize),
}

/// Parse raw MIDI bytes into a MidiMessage
pub fn parse_midi_bytes(data: &[u8]) -> Option<MidiMessage> {
    if data.is_empty() {
        return None;
    }

    let status = data[0];
    let channel = status & 0x0F;
    let msg_type = status & 0xF0;

    match msg_type {
        0x90 if data.len() >= 3 => {
            let velocity = data[2];
            if velocity == 0 {
                // Note On with velocity 0 is Note Off
                Some(MidiMessage::NoteOff {
                    channel,
                    pitch: data[1],
                })
            } else {
                Some(MidiMessage::NoteOn {
                    channel,
                    pitch: data[1],
                    velocity,
                })
            }
        }
        0x80 if data.len() >= 3 => Some(MidiMessage::NoteOff {
            channel,
            pitch: data[1],
        }),
        0xB0 if data.len() >= 3 => Some(MidiMessage::ControlChange {
            channel,
            controller: data[1],
            value: data[2],
        }),
        0xC0 if data.len() >= 2 => Some(MidiMessage::ProgramChange {
            channel,
            program: data[1],
        }),
        0xE0 if data.len() >= 3 => {
            // Pitch bend: 14-bit value, center at 8192
            let lsb = data[1] as i16;
            let msb = data[2] as i16;
            let value = ((msb << 7) | lsb) - 8192;
            Some(MidiMessage::PitchBend { channel, value })
        }
        _ => {
            // System messages, sysex, etc. - not parsed yet
            debug!("Unhandled MIDI message type: {:02X}", status);
            None
        }
    }
}

/// Encode a MidiMessage to raw MIDI bytes
pub fn encode_midi_message(msg: &MidiMessage) -> Vec<u8> {
    match msg {
        MidiMessage::NoteOn {
            channel,
            pitch,
            velocity,
        } => vec![0x90 | (channel & 0x0F), *pitch, *velocity],
        MidiMessage::NoteOff { channel, pitch } => {
            vec![0x80 | (channel & 0x0F), *pitch, 0]
        }
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => vec![0xB0 | (channel & 0x0F), *controller, *value],
        MidiMessage::ProgramChange { channel, program } => {
            vec![0xC0 | (channel & 0x0F), *program]
        }
        MidiMessage::PitchBend { channel, value } => {
            let centered = (*value + 8192) as u16;
            let lsb = (centered & 0x7F) as u8;
            let msb = ((centered >> 7) & 0x7F) as u8;
            vec![0xE0 | (channel & 0x0F), lsb, msb]
        }
    }
}

/// List available MIDI input ports
pub fn list_input_ports() -> Result<Vec<MidiPortInfo>, MidiError> {
    let midi_in = MidiInput::new("hootenanny-scan")
        .map_err(|e| MidiError::InitFailed(e.to_string()))?;

    let ports: Vec<MidiPortInfo> = midi_in
        .ports()
        .iter()
        .enumerate()
        .filter_map(|(i, port)| {
            midi_in.port_name(port).ok().map(|name| MidiPortInfo {
                index: i,
                name,
            })
        })
        .collect();

    Ok(ports)
}

/// List available MIDI output ports
pub fn list_output_ports() -> Result<Vec<MidiPortInfo>, MidiError> {
    let midi_out = MidiOutput::new("hootenanny-scan")
        .map_err(|e| MidiError::InitFailed(e.to_string()))?;

    let ports: Vec<MidiPortInfo> = midi_out
        .ports()
        .iter()
        .enumerate()
        .filter_map(|(i, port)| {
            midi_out.port_name(port).ok().map(|name| MidiPortInfo {
                index: i,
                name,
            })
        })
        .collect();

    Ok(ports)
}

/// Callback type for receiving MIDI input
/// Must be Send + Sync because it's called from the MIDI thread
pub type MidiInputCallback = Box<dyn Fn(TimestampedMidiMessage) + Send + Sync + 'static>;

/// Active MIDI input connection
pub struct ActiveMidiInput {
    /// Connection (dropped to close)
    connection: Option<MidiInputConnection<()>>,
    /// Port name
    pub port_name: String,
    /// Message counter
    pub messages_received: Arc<AtomicU64>,
    /// Running flag
    running: AtomicBool,
}

impl ActiveMidiInput {
    /// Open a MIDI input port by name pattern
    pub fn open(
        port_pattern: &str,
        callback: MidiInputCallback,
    ) -> Result<Self, MidiError> {
        let midi_in = MidiInput::new("hootenanny-in")
            .map_err(|e| MidiError::InitFailed(e.to_string()))?;

        let ports = midi_in.ports();
        let port = ports
            .iter()
            .find(|p| {
                midi_in
                    .port_name(p)
                    .map(|n| n.contains(port_pattern))
                    .unwrap_or(false)
            })
            .ok_or_else(|| MidiError::PortNotFound(port_pattern.to_string()))?;

        let port_name = midi_in
            .port_name(port)
            .map_err(|e| MidiError::ConnectionFailed(e.to_string()))?;

        let messages_received = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&messages_received);
        let callback = Arc::new(callback);

        let connection = midi_in
            .connect(
                port,
                "hootenanny-input",
                move |timestamp_us, data, _| {
                    if let Some(message) = parse_midi_bytes(data) {
                        counter.fetch_add(1, Ordering::Relaxed);
                        let msg = TimestampedMidiMessage {
                            timestamp_us,
                            message,
                            raw: data.to_vec(),
                        };
                        callback(msg);
                    }
                },
                (),
            )
            .map_err(|e| MidiError::ConnectionFailed(e.to_string()))?;

        info!("Opened MIDI input: {}", port_name);

        Ok(Self {
            connection: Some(connection),
            port_name,
            messages_received,
            running: AtomicBool::new(true),
        })
    }

    /// Check if still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Close the connection
    pub fn close(&mut self) {
        if let Some(conn) = self.connection.take() {
            self.running.store(false, Ordering::Relaxed);
            conn.close();
            info!("Closed MIDI input: {}", self.port_name);
        }
    }
}

impl Drop for ActiveMidiInput {
    fn drop(&mut self) {
        self.close();
    }
}

/// Active MIDI output connection
pub struct ActiveMidiOutput {
    /// Connection (requires mutex for send)
    connection: Mutex<Option<MidiOutputConnection>>,
    /// Port name
    pub port_name: String,
    /// Message counter
    pub messages_sent: AtomicU64,
}

impl ActiveMidiOutput {
    /// Open a MIDI output port by name pattern
    pub fn open(port_pattern: &str) -> Result<Self, MidiError> {
        let midi_out = MidiOutput::new("hootenanny-out")
            .map_err(|e| MidiError::InitFailed(e.to_string()))?;

        let ports = midi_out.ports();
        let port = ports
            .iter()
            .find(|p| {
                midi_out
                    .port_name(p)
                    .map(|n| n.contains(port_pattern))
                    .unwrap_or(false)
            })
            .ok_or_else(|| MidiError::PortNotFound(port_pattern.to_string()))?;

        let port_name = midi_out
            .port_name(port)
            .map_err(|e| MidiError::ConnectionFailed(e.to_string()))?;

        let connection = midi_out
            .connect(port, "hootenanny-output")
            .map_err(|e| MidiError::ConnectionFailed(e.to_string()))?;

        info!("Opened MIDI output: {}", port_name);

        Ok(Self {
            connection: Mutex::new(Some(connection)),
            port_name,
            messages_sent: AtomicU64::new(0),
        })
    }

    /// Send a MIDI message
    pub fn send(&self, msg: &MidiMessage) -> Result<(), MidiError> {
        let bytes = encode_midi_message(msg);
        self.send_raw(&bytes)
    }

    /// Send raw MIDI bytes
    pub fn send_raw(&self, data: &[u8]) -> Result<(), MidiError> {
        let mut guard = self.connection.lock().expect("midi output mutex poisoned");
        if let Some(ref mut conn) = *guard {
            conn.send(data)
                .map_err(|e| MidiError::SendFailed(e.to_string()))?;
            self.messages_sent.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            Err(MidiError::SendFailed("Connection closed".to_string()))
        }
    }

    /// Close the connection
    pub fn close(&self) {
        let mut guard = self.connection.lock().expect("midi output mutex poisoned");
        if let Some(conn) = guard.take() {
            conn.close();
            info!("Closed MIDI output: {}", self.port_name);
        }
    }
}

impl Drop for ActiveMidiOutput {
    fn drop(&mut self) {
        self.close();
    }
}

/// MIDI I/O Manager for the daemon
///
/// Manages multiple input and output connections, routes events.
pub struct MidiIOManager {
    inputs: Mutex<Vec<ActiveMidiInput>>,
    outputs: Mutex<Vec<ActiveMidiOutput>>,
}

impl MidiIOManager {
    pub fn new() -> Self {
        Self {
            inputs: Mutex::new(Vec::new()),
            outputs: Mutex::new(Vec::new()),
        }
    }

    /// Attach a MIDI input by port name pattern
    ///
    /// Returns an error if a port matching the pattern is already connected.
    pub fn attach_input(
        &self,
        port_pattern: &str,
        callback: MidiInputCallback,
    ) -> Result<String, MidiError> {
        // Check for duplicate connection before opening
        {
            let inputs = self.inputs.lock().expect("midi inputs mutex poisoned");
            if let Some(existing) = inputs.iter().find(|i| i.port_name.contains(port_pattern)) {
                return Err(MidiError::AlreadyConnected(existing.port_name.clone()));
            }
        }

        let input = ActiveMidiInput::open(port_pattern, callback)?;
        let port_name = input.port_name.clone();
        self.inputs.lock().expect("midi inputs mutex poisoned").push(input);
        Ok(port_name)
    }

    /// Attach a MIDI output by port name pattern
    ///
    /// Returns an error if a port matching the pattern is already connected.
    pub fn attach_output(&self, port_pattern: &str) -> Result<String, MidiError> {
        // Check for duplicate connection before opening
        {
            let outputs = self.outputs.lock().expect("midi outputs mutex poisoned");
            if let Some(existing) = outputs.iter().find(|o| o.port_name.contains(port_pattern)) {
                return Err(MidiError::AlreadyConnected(existing.port_name.clone()));
            }
        }

        let output = ActiveMidiOutput::open(port_pattern)?;
        let port_name = output.port_name.clone();
        self.outputs.lock().expect("midi outputs mutex poisoned").push(output);
        Ok(port_name)
    }

    /// Send to all connected outputs (useful for clock, transport)
    ///
    /// Continues sending to all outputs even if some fail.
    /// Returns `Ok(())` if all sends succeed, or `PartialSendFailure` if any failed.
    pub fn send_to_all(&self, msg: &MidiMessage) -> Result<(), MidiError> {
        let outputs = self.outputs.lock().expect("midi outputs mutex poisoned");
        let total = outputs.len();
        let mut failures = 0;

        for output in outputs.iter() {
            if output.send(msg).is_err() {
                failures += 1;
            }
        }

        if failures == 0 {
            Ok(())
        } else {
            Err(MidiError::PartialSendFailure(failures, total))
        }
    }

    /// Send to a specific output by port name pattern
    pub fn send_to(&self, port_pattern: &str, msg: &MidiMessage) -> Result<(), MidiError> {
        let outputs = self.outputs.lock().expect("midi outputs mutex poisoned");
        let output = outputs
            .iter()
            .find(|o| o.port_name.contains(port_pattern))
            .ok_or_else(|| MidiError::PortNotFound(port_pattern.to_string()))?;
        output.send(msg)
    }

    /// Send raw MIDI bytes to all connected outputs
    ///
    /// Continues sending to all outputs even if some fail.
    /// Returns `Ok(())` if all sends succeed, or `PartialSendFailure` if any failed.
    pub fn send_raw_to_all(&self, data: &[u8]) -> Result<(), MidiError> {
        let outputs = self.outputs.lock().expect("midi outputs mutex poisoned");
        let total = outputs.len();
        let mut failures = 0;

        for output in outputs.iter() {
            if output.send_raw(data).is_err() {
                failures += 1;
            }
        }

        if failures == 0 {
            Ok(())
        } else {
            Err(MidiError::PartialSendFailure(failures, total))
        }
    }

    /// Send raw MIDI bytes to a specific output by port name pattern
    pub fn send_raw_to(&self, port_pattern: &str, data: &[u8]) -> Result<(), MidiError> {
        let outputs = self.outputs.lock().expect("midi outputs mutex poisoned");
        let output = outputs
            .iter()
            .find(|o| o.port_name.contains(port_pattern))
            .ok_or_else(|| MidiError::PortNotFound(port_pattern.to_string()))?;
        output.send_raw(data)
    }

    /// Detach an input by port name pattern
    pub fn detach_input(&self, port_pattern: &str) -> bool {
        let mut inputs = self.inputs.lock().expect("midi inputs mutex poisoned");
        if let Some(pos) = inputs.iter().position(|i| i.port_name.contains(port_pattern)) {
            inputs.remove(pos);
            true
        } else {
            false
        }
    }

    /// Detach an output by port name pattern
    pub fn detach_output(&self, port_pattern: &str) -> bool {
        let mut outputs = self.outputs.lock().expect("midi outputs mutex poisoned");
        if let Some(pos) = outputs.iter().position(|o| o.port_name.contains(port_pattern)) {
            outputs.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get status of all connections
    pub fn status(&self) -> MidiIOStatus {
        let inputs = self.inputs.lock().expect("midi inputs mutex poisoned");
        let outputs = self.outputs.lock().expect("midi outputs mutex poisoned");

        MidiIOStatus {
            inputs: inputs
                .iter()
                .map(|i| MidiConnectionStatus {
                    port_name: i.port_name.clone(),
                    messages: i.messages_received.load(Ordering::Relaxed),
                })
                .collect(),
            outputs: outputs
                .iter()
                .map(|o| MidiConnectionStatus {
                    port_name: o.port_name.clone(),
                    messages: o.messages_sent.load(Ordering::Relaxed),
                })
                .collect(),
        }
    }
}

impl Default for MidiIOManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of a single MIDI connection
#[derive(Debug, Clone)]
pub struct MidiConnectionStatus {
    pub port_name: String,
    pub messages: u64,
}

/// Status of all MIDI I/O
#[derive(Debug, Clone)]
pub struct MidiIOStatus {
    pub inputs: Vec<MidiConnectionStatus>,
    pub outputs: Vec<MidiConnectionStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_on() {
        let data = [0x90, 60, 100]; // Note On, channel 0, middle C, velocity 100
        let msg = parse_midi_bytes(&data).unwrap();
        match msg {
            MidiMessage::NoteOn {
                channel,
                pitch,
                velocity,
            } => {
                assert_eq!(channel, 0);
                assert_eq!(pitch, 60);
                assert_eq!(velocity, 100);
            }
            _ => panic!("Expected NoteOn"),
        }
    }

    #[test]
    fn test_parse_note_on_velocity_zero_is_note_off() {
        let data = [0x90, 60, 0]; // Note On with velocity 0 = Note Off
        let msg = parse_midi_bytes(&data).unwrap();
        match msg {
            MidiMessage::NoteOff { channel, pitch } => {
                assert_eq!(channel, 0);
                assert_eq!(pitch, 60);
            }
            _ => panic!("Expected NoteOff"),
        }
    }

    #[test]
    fn test_parse_control_change() {
        let data = [0xB0, 1, 64]; // CC1 (mod wheel), value 64
        let msg = parse_midi_bytes(&data).unwrap();
        match msg {
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => {
                assert_eq!(channel, 0);
                assert_eq!(controller, 1);
                assert_eq!(value, 64);
            }
            _ => panic!("Expected ControlChange"),
        }
    }

    #[test]
    fn test_encode_note_on() {
        let msg = MidiMessage::NoteOn {
            channel: 0,
            pitch: 60,
            velocity: 100,
        };
        let bytes = encode_midi_message(&msg);
        assert_eq!(bytes, vec![0x90, 60, 100]);
    }

    #[test]
    fn test_encode_pitch_bend() {
        let msg = MidiMessage::PitchBend {
            channel: 0,
            value: 0, // Center
        };
        let bytes = encode_midi_message(&msg);
        // Center (8192) = 0x2000 -> LSB=0x00, MSB=0x40
        assert_eq!(bytes, vec![0xE0, 0x00, 0x40]);
    }

    #[test]
    fn test_roundtrip_note_on() {
        let original = MidiMessage::NoteOn {
            channel: 5,
            pitch: 72,
            velocity: 127,
        };
        let bytes = encode_midi_message(&original);
        let parsed = parse_midi_bytes(&bytes).unwrap();
        match parsed {
            MidiMessage::NoteOn {
                channel,
                pitch,
                velocity,
            } => {
                assert_eq!(channel, 5);
                assert_eq!(pitch, 72);
                assert_eq!(velocity, 127);
            }
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_list_ports() {
        // This test just verifies the functions don't panic
        // Actual port availability depends on the system
        let _ = list_input_ports();
        let _ = list_output_ports();
    }
}
