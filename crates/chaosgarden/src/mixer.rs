//! Audio mixer for RT signal summing
//!
//! Provides a reusable mixer architecture for combining multiple audio sources.
//! Designed for use in:
//! - Hardware I/O mixing (monitor input + timeline → output)
//! - Synthesis voice mixing (N oscillator voices → output)
//! - Submix buses (group channels → bus → master)
//!
//! The mixer separates concerns:
//! - **MixerChannel**: Control state for one input (gain, pan, mute, solo)
//! - **MixerState**: Collection of channels + master controls
//! - **mix_buffers()**: The RT-safe mixing function (pure math, no I/O)
//!
//! Ring buffers are NOT part of the mixer - they're a transport concern.
//! The RT callback reads from rings into temp buffers, then calls mix_buffers().

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use portable_atomic::AtomicF32;
use uuid::Uuid;

/// A single input channel in the mixer
///
/// All fields are Arc to allow sharing with RT callback without copying.
/// All control values use relaxed atomic ordering for RT safety.
#[derive(Debug)]
pub struct MixerChannel {
    /// Unique channel identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Whether this channel is active (can be disabled to skip processing)
    pub enabled: Arc<AtomicBool>,
    /// Linear gain 0.0-2.0 (1.0 = unity, >1.0 = boost)
    pub gain: Arc<AtomicF32>,
    /// Pan position: -1.0 (full left) to 1.0 (full right), 0.0 = center
    pub pan: Arc<AtomicF32>,
    /// Mute flag (separate from enabled for UI distinction)
    pub mute: Arc<AtomicBool>,
    /// Solo flag (when any channel is solo'd, only solo'd channels play)
    pub solo: Arc<AtomicBool>,
}

impl MixerChannel {
    /// Create a new mixer channel with default settings
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: Arc::new(AtomicBool::new(true)),
            gain: Arc::new(AtomicF32::new(1.0)),
            pan: Arc::new(AtomicF32::new(0.0)),
            mute: Arc::new(AtomicBool::new(false)),
            solo: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a channel with specific ID (for deterministic testing)
    pub fn with_id(id: Uuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            enabled: Arc::new(AtomicBool::new(true)),
            gain: Arc::new(AtomicF32::new(1.0)),
            pan: Arc::new(AtomicF32::new(0.0)),
            mute: Arc::new(AtomicBool::new(false)),
            solo: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if this channel should contribute to output
    ///
    /// Takes into account: enabled, mute, and solo logic
    fn should_play(&self, any_solo_active: bool) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }
        if self.mute.load(Ordering::Relaxed) {
            return false;
        }
        if any_solo_active && !self.solo.load(Ordering::Relaxed) {
            return false;
        }
        true
    }

    /// Get current gain value
    pub fn get_gain(&self) -> f32 {
        self.gain.load(Ordering::Relaxed)
    }

    /// Get current pan value
    pub fn get_pan(&self) -> f32 {
        self.pan.load(Ordering::Relaxed)
    }

    /// Set gain (clamped to 0.0-2.0)
    pub fn set_gain(&self, value: f32) {
        self.gain.store(value.clamp(0.0, 2.0), Ordering::Relaxed);
    }

    /// Set pan (clamped to -1.0 to 1.0)
    pub fn set_pan(&self, value: f32) {
        self.pan.store(value.clamp(-1.0, 1.0), Ordering::Relaxed);
    }
}

impl Clone for MixerChannel {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            enabled: Arc::clone(&self.enabled),
            gain: Arc::clone(&self.gain),
            pan: Arc::clone(&self.pan),
            mute: Arc::clone(&self.mute),
            solo: Arc::clone(&self.solo),
        }
    }
}

/// Master mixer state
///
/// Holds multiple input channels and master output controls.
/// The actual mixing happens via `mix_buffers()` or `mix_stereo_buffers()`.
#[derive(Debug)]
pub struct MixerState {
    /// Input channels (each has its own controls)
    channels: Vec<Arc<MixerChannel>>,
    /// Master output gain
    pub master_gain: Arc<AtomicF32>,
    /// Master mute
    pub master_mute: Arc<AtomicBool>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerState {
    /// Create a new empty mixer
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            master_gain: Arc::new(AtomicF32::new(1.0)),
            master_mute: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Add a channel and return its Arc for external control
    pub fn add_channel(&mut self, channel: MixerChannel) -> Arc<MixerChannel> {
        let arc = Arc::new(channel);
        self.channels.push(Arc::clone(&arc));
        arc
    }

    /// Remove a channel by ID
    pub fn remove_channel(&mut self, id: Uuid) -> Option<Arc<MixerChannel>> {
        if let Some(pos) = self.channels.iter().position(|c| c.id == id) {
            Some(self.channels.remove(pos))
        } else {
            None
        }
    }

    /// Get a channel by ID
    pub fn get_channel(&self, id: Uuid) -> Option<&Arc<MixerChannel>> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// Get channel by index
    pub fn channel(&self, index: usize) -> Option<&Arc<MixerChannel>> {
        self.channels.get(index)
    }

    /// Number of channels
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Iterate over channels
    pub fn channels(&self) -> impl Iterator<Item = &Arc<MixerChannel>> {
        self.channels.iter()
    }

    /// Check if any channel has solo enabled
    fn any_solo_active(&self) -> bool {
        self.channels
            .iter()
            .any(|c| c.solo.load(Ordering::Relaxed))
    }

    /// Mix mono buffers into output (simplest case)
    ///
    /// Each input buffer corresponds to a channel (by index).
    /// Output is summed with gain applied.
    /// Extra inputs beyond channel count are ignored.
    /// Missing inputs (fewer than channels) leave those channels silent.
    ///
    /// This is the core mixing function - pure math, RT-safe.
    pub fn mix_mono(&self, inputs: &[&[f32]], output: &mut [f32]) {
        // Clear output first
        output.fill(0.0);

        if self.master_mute.load(Ordering::Relaxed) {
            return;
        }

        let any_solo = self.any_solo_active();
        let master_gain = self.master_gain.load(Ordering::Relaxed);

        for (idx, channel) in self.channels.iter().enumerate() {
            if !channel.should_play(any_solo) {
                continue;
            }

            let Some(input) = inputs.get(idx) else {
                continue;
            };

            let gain = channel.get_gain() * master_gain;

            for (i, sample) in input.iter().enumerate() {
                if i < output.len() {
                    output[i] += sample * gain;
                }
            }
        }
    }

    /// Mix stereo interleaved buffers with pan support
    ///
    /// Inputs are mono, output is stereo interleaved (L,R,L,R,...).
    /// Pan is applied using constant-power panning.
    ///
    /// This is the typical case for synthesis voice mixing.
    pub fn mix_to_stereo(&self, inputs: &[&[f32]], output: &mut [f32]) {
        // Clear output first (stereo interleaved)
        output.fill(0.0);

        if self.master_mute.load(Ordering::Relaxed) {
            return;
        }

        let any_solo = self.any_solo_active();
        let master_gain = self.master_gain.load(Ordering::Relaxed);

        for (idx, channel) in self.channels.iter().enumerate() {
            if !channel.should_play(any_solo) {
                continue;
            }

            let Some(input) = inputs.get(idx) else {
                continue;
            };

            let gain = channel.get_gain() * master_gain;
            let pan = channel.get_pan();

            // Constant power panning: sqrt(2)/2 at center
            // pan = -1.0: left_gain = 1.0, right_gain = 0.0
            // pan = 0.0:  left_gain = 0.707, right_gain = 0.707
            // pan = 1.0:  left_gain = 0.0, right_gain = 1.0
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4; // 0 to PI/2
            let left_gain = angle.cos() * gain;
            let right_gain = angle.sin() * gain;

            let output_frames = output.len() / 2;
            for (i, &sample) in input.iter().enumerate() {
                if i >= output_frames {
                    break;
                }
                output[i * 2] += sample * left_gain;
                output[i * 2 + 1] += sample * right_gain;
            }
        }
    }

    /// Mix stereo interleaved inputs to stereo output
    ///
    /// Both inputs and output are stereo interleaved.
    /// Pan affects the L/R balance of each input.
    ///
    /// This is typical for hardware I/O mixing.
    pub fn mix_stereo_to_stereo(&self, inputs: &[&[f32]], output: &mut [f32]) {
        output.fill(0.0);

        if self.master_mute.load(Ordering::Relaxed) {
            return;
        }

        let any_solo = self.any_solo_active();
        let master_gain = self.master_gain.load(Ordering::Relaxed);

        for (idx, channel) in self.channels.iter().enumerate() {
            if !channel.should_play(any_solo) {
                continue;
            }

            let Some(input) = inputs.get(idx) else {
                continue;
            };

            let gain = channel.get_gain() * master_gain;
            let pan = channel.get_pan();

            // For stereo inputs, pan shifts the balance
            // pan = 0: L and R pass through equally
            // pan = -1: both L and R go to L output
            // pan = 1: both L and R go to R output
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            let left_mix = angle.cos();
            let right_mix = angle.sin();

            let frames = input.len().min(output.len()) / 2;
            for i in 0..frames {
                let in_l = input[i * 2];
                let in_r = input[i * 2 + 1];

                // Cross-fade based on pan
                output[i * 2] += (in_l * left_mix + in_r * (1.0 - right_mix)) * gain;
                output[i * 2 + 1] += (in_r * right_mix + in_l * (1.0 - left_mix)) * gain;
            }
        }
    }
}

/// Configuration for creating a mixer
#[derive(Debug, Clone)]
pub struct MixerConfig {
    /// Initial number of channels
    pub num_channels: usize,
    /// Channel name prefix (channels named "prefix_0", "prefix_1", etc.)
    pub channel_prefix: String,
    /// Initial master gain
    pub master_gain: f32,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            num_channels: 0,
            channel_prefix: "ch".to_string(),
            master_gain: 1.0,
        }
    }
}

impl MixerConfig {
    /// Create a mixer from this configuration
    pub fn build(self) -> MixerState {
        let mut mixer = MixerState::new();
        mixer
            .master_gain
            .store(self.master_gain, Ordering::Relaxed);

        for i in 0..self.num_channels {
            let name = format!("{}_{}", self.channel_prefix, i);
            mixer.add_channel(MixerChannel::new(name));
        }

        mixer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_creation() {
        let ch = MixerChannel::new("test");
        assert_eq!(ch.name, "test");
        assert!(ch.enabled.load(Ordering::Relaxed));
        assert!((ch.get_gain() - 1.0).abs() < 0.001);
        assert!((ch.get_pan() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_channel_gain_clamping() {
        let ch = MixerChannel::new("test");
        ch.set_gain(3.0);
        assert!((ch.get_gain() - 2.0).abs() < 0.001);

        ch.set_gain(-1.0);
        assert!((ch.get_gain() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_channel_pan_clamping() {
        let ch = MixerChannel::new("test");
        ch.set_pan(2.0);
        assert!((ch.get_pan() - 1.0).abs() < 0.001);

        ch.set_pan(-2.0);
        assert!((ch.get_pan() - -1.0).abs() < 0.001);
    }

    #[test]
    fn test_mixer_mono_mix() {
        let mut mixer = MixerState::new();
        mixer.add_channel(MixerChannel::new("a"));
        mixer.add_channel(MixerChannel::new("b"));

        let input_a = [0.5, 0.5, 0.5, 0.5];
        let input_b = [0.25, 0.25, 0.25, 0.25];
        let inputs: Vec<&[f32]> = vec![&input_a, &input_b];

        let mut output = [0.0f32; 4];
        mixer.mix_mono(&inputs, &mut output);

        // 0.5 + 0.25 = 0.75
        for &s in &output {
            assert!((s - 0.75).abs() < 0.001);
        }
    }

    #[test]
    fn test_mixer_mono_with_gain() {
        let mut mixer = MixerState::new();
        let ch = mixer.add_channel(MixerChannel::new("a"));
        ch.set_gain(0.5);

        let input_a = [1.0, 1.0, 1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 4];
        mixer.mix_mono(&inputs, &mut output);

        for &s in &output {
            assert!((s - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn test_mixer_mute() {
        let mut mixer = MixerState::new();
        let ch = mixer.add_channel(MixerChannel::new("a"));
        ch.mute.store(true, Ordering::Relaxed);

        let input_a = [1.0, 1.0, 1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 4];
        mixer.mix_mono(&inputs, &mut output);

        for &s in &output {
            assert!((s - 0.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_mixer_solo() {
        let mut mixer = MixerState::new();
        let ch_a = mixer.add_channel(MixerChannel::new("a"));
        let _ch_b = mixer.add_channel(MixerChannel::new("b"));

        // Solo channel A only
        ch_a.solo.store(true, Ordering::Relaxed);

        let input_a = [1.0, 1.0, 1.0, 1.0];
        let input_b = [0.5, 0.5, 0.5, 0.5];
        let inputs: Vec<&[f32]> = vec![&input_a, &input_b];

        let mut output = [0.0f32; 4];
        mixer.mix_mono(&inputs, &mut output);

        // Only A should play
        for &s in &output {
            assert!((s - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_mixer_master_mute() {
        let mut mixer = MixerState::new();
        mixer.add_channel(MixerChannel::new("a"));
        mixer.master_mute.store(true, Ordering::Relaxed);

        let input_a = [1.0, 1.0, 1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 4];
        mixer.mix_mono(&inputs, &mut output);

        for &s in &output {
            assert!((s - 0.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_mixer_master_gain() {
        let mut mixer = MixerState::new();
        mixer.add_channel(MixerChannel::new("a"));
        mixer.master_gain.store(0.5, Ordering::Relaxed);

        let input_a = [1.0, 1.0, 1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 4];
        mixer.mix_mono(&inputs, &mut output);

        for &s in &output {
            assert!((s - 0.5).abs() < 0.001);
        }
    }

    #[test]
    fn test_mix_to_stereo_center_pan() {
        let mut mixer = MixerState::new();
        mixer.add_channel(MixerChannel::new("a")); // pan = 0 (center)

        let input_a = [1.0, 1.0, 1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 8]; // 4 stereo frames
        mixer.mix_to_stereo(&inputs, &mut output);

        // At center pan, both L and R should be ~0.707 (sqrt(2)/2)
        let expected = std::f32::consts::FRAC_1_SQRT_2;
        for i in 0..4 {
            assert!((output[i * 2] - expected).abs() < 0.01, "L[{}]={}", i, output[i * 2]);
            assert!(
                (output[i * 2 + 1] - expected).abs() < 0.01,
                "R[{}]={}",
                i,
                output[i * 2 + 1]
            );
        }
    }

    #[test]
    fn test_mix_to_stereo_hard_left() {
        let mut mixer = MixerState::new();
        let ch = mixer.add_channel(MixerChannel::new("a"));
        ch.set_pan(-1.0);

        let input_a = [1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 4];
        mixer.mix_to_stereo(&inputs, &mut output);

        // Hard left: L=1.0, R=0.0
        assert!((output[0] - 1.0).abs() < 0.01);
        assert!((output[1] - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_mix_to_stereo_hard_right() {
        let mut mixer = MixerState::new();
        let ch = mixer.add_channel(MixerChannel::new("a"));
        ch.set_pan(1.0);

        let input_a = [1.0, 1.0];
        let inputs: Vec<&[f32]> = vec![&input_a];

        let mut output = [0.0f32; 4];
        mixer.mix_to_stereo(&inputs, &mut output);

        // Hard right: L=0.0, R=1.0
        assert!((output[0] - 0.0).abs() < 0.01);
        assert!((output[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_config_build() {
        let config = MixerConfig {
            num_channels: 4,
            channel_prefix: "voice".to_string(),
            master_gain: 0.8,
        };

        let mixer = config.build();
        assert_eq!(mixer.channel_count(), 4);
        assert!((mixer.master_gain.load(Ordering::Relaxed) - 0.8).abs() < 0.001);
        assert_eq!(mixer.channel(0).unwrap().name, "voice_0");
        assert_eq!(mixer.channel(3).unwrap().name, "voice_3");
    }

    #[test]
    fn test_remove_channel() {
        let mut mixer = MixerState::new();
        let ch_a = mixer.add_channel(MixerChannel::new("a"));
        let ch_b = mixer.add_channel(MixerChannel::new("b"));

        assert_eq!(mixer.channel_count(), 2);

        let removed = mixer.remove_channel(ch_a.id);
        assert!(removed.is_some());
        assert_eq!(mixer.channel_count(), 1);
        assert_eq!(mixer.channel(0).unwrap().id, ch_b.id);
    }
}
