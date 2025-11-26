use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct IdentityId(pub String);

impl fmt::Display for IdentityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for IdentityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for IdentityId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: IdentityId,
    pub name: String,
    pub created_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityHint {
    pub identity_id: IdentityId,
    pub kind: HintKind,
    pub value: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    UsbDeviceId,
    UsbSerial,
    UsbPath,
    MidiName,
    AlsaCard,
    AlsaHw,
    PipewireName,
    PipewireAlsaPath,
}

impl fmt::Display for HintKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl HintKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UsbDeviceId => "usb_device_id",
            Self::UsbSerial => "usb_serial",
            Self::UsbPath => "usb_path",
            Self::MidiName => "midi_name",
            Self::AlsaCard => "alsa_card",
            Self::AlsaHw => "alsa_hw",
            Self::PipewireName => "pipewire_name",
            Self::PipewireAlsaPath => "pipewire_alsa_path",
        }
    }
}

impl FromStr for HintKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "usb_device_id" => Ok(Self::UsbDeviceId),
            "usb_serial" => Ok(Self::UsbSerial),
            "usb_path" => Ok(Self::UsbPath),
            "midi_name" => Ok(Self::MidiName),
            "alsa_card" => Ok(Self::AlsaCard),
            "alsa_hw" => Ok(Self::AlsaHw),
            "pipewire_name" => Ok(Self::PipewireName),
            "pipewire_alsa_path" => Ok(Self::PipewireAlsaPath),
            _ => Err(format!("Unknown hint kind: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub identity_id: IdentityId,
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub target_kind: String,
    pub target_id: String,
    pub created_at: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualConnection {
    pub id: String,
    pub from_identity: IdentityId,
    pub from_port: String,
    pub to_identity: IdentityId,
    pub to_port: String,
    pub transport_kind: Option<String>,
    pub signal_direction: Option<String>,
    pub created_at: String,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub id: i64,
    pub timestamp: String,
    pub source: String,
    pub operation: String,
    pub target_kind: String,
    pub target_id: String,
    pub details: serde_json::Value,
}
