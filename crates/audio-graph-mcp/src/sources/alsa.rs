use alsa::seq::{ClientIter, PortCap, PortIter, Seq};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::HintKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlsaMidiDevice {
    pub client_id: i32,
    pub client_name: String,
    pub ports: Vec<AlsaMidiPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlsaMidiPort {
    pub port_id: i32,
    pub name: String,
    pub direction: PortDirection,
    pub addr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortDirection {
    Input,
    Output,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    pub kind: HintKind,
    pub value: String,
}

pub struct AlsaSource {
    seq: Option<Seq>,
}

impl AlsaSource {
    pub fn new() -> Self {
        let seq = Seq::open(None, None, false)
            .ok()
            .inspect(|s| {
                s.set_client_name(&std::ffi::CString::new("audio-graph-mcp").unwrap())
                    .ok();
            });

        Self { seq }
    }

    pub fn is_available(&self) -> bool {
        self.seq.is_some()
    }

    pub fn enumerate_devices(&self) -> Result<Vec<AlsaMidiDevice>> {
        let seq = self.seq.as_ref().context("ALSA sequencer not available")?;

        let mut devices = Vec::new();

        for client in ClientIter::new(seq) {
            let client_id = client.get_client();
            let client_name = client
                .get_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("Client {}", client_id));

            // Skip system clients (0 = System, 14 = Midi Through)
            if client_id == 0 || client_id == 14 {
                continue;
            }

            let mut ports = Vec::new();

            for port in PortIter::new(seq, client_id) {
                let port_id = port.get_port();
                let port_name = port
                    .get_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| format!("Port {}", port_id));

                let caps = port.get_capability();
                let direction = if caps.contains(PortCap::READ) && caps.contains(PortCap::WRITE) {
                    PortDirection::Bidirectional
                } else if caps.contains(PortCap::READ) {
                    PortDirection::Output // READ = we can read from it = output
                } else if caps.contains(PortCap::WRITE) {
                    PortDirection::Input // WRITE = we can write to it = input
                } else {
                    continue; // Skip non-accessible ports
                };

                ports.push(AlsaMidiPort {
                    port_id,
                    name: port_name,
                    direction,
                    addr: format!("{}:{}", client_id, port_id),
                });
            }

            if !ports.is_empty() {
                devices.push(AlsaMidiDevice {
                    client_id,
                    client_name,
                    ports,
                });
            }
        }

        Ok(devices)
    }

    pub fn extract_fingerprints(&self, device: &AlsaMidiDevice) -> Vec<DeviceFingerprint> {
        vec![
            DeviceFingerprint {
                kind: HintKind::AlsaCard,
                value: device.client_name.clone(),
            },
            DeviceFingerprint {
                kind: HintKind::MidiName,
                value: device.client_name.clone(),
            },
        ]
    }
}

impl Default for AlsaSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alsa_source_creation() {
        let source = AlsaSource::new();
        // May or may not be available depending on system
        println!("ALSA available: {}", source.is_available());
    }

    #[test]
    fn test_enumerate_devices() {
        let source = AlsaSource::new();
        if !source.is_available() {
            println!("Skipping test - ALSA not available");
            return;
        }

        let devices = source.enumerate_devices().unwrap();
        println!("Found {} ALSA MIDI clients:", devices.len());
        for device in &devices {
            println!("  {} (id {})", device.client_name, device.client_id);
            for port in &device.ports {
                println!("    - {} ({:?})", port.name, port.direction);
            }
        }
    }

    #[test]
    fn test_extract_fingerprints() {
        let source = AlsaSource::new();
        let device = AlsaMidiDevice {
            client_id: 20,
            client_name: "Roland JD-Xi".to_string(),
            ports: vec![],
        };

        let fingerprints = source.extract_fingerprints(&device);
        assert_eq!(fingerprints.len(), 2);
        assert_eq!(fingerprints[0].value, "Roland JD-Xi");
    }
}
