//! Audio processing nodes
//!
//! This module contains concrete node implementations for the chaosgarden graph.

mod audio_file;

pub use audio_file::{
    decode_audio, decode_wav, AudioFileNode, ContentResolver, DecodedAudio, FileCasClient,
    MemoryResolver,
};

#[cfg(feature = "symphonia-decode")]
pub use audio_file::decode_audio_symphonia;
