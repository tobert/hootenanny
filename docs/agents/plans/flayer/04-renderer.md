# Task 03: Renderer

**Priority:** High
**Estimated Sessions:** 3-4
**Depends On:** 01-core-structs, 02-midi-module

---

## Objective

Implement the rendering engine that flattens a Timeline to audio output. This handles:
- Loading clips from CAS
- Rendering MIDI through SoundFont
- Mixing tracks with volume/pan
- Resampling for playback rate changes
- Applying fades and envelopes

## Files to Create/Modify

### Add to `crates/flayer/Cargo.toml`

```toml
[dependencies]
hound = "3.5"
rustysynth = "1.3"
rubato = "0.15"  # or "1.0.0-preview.1" for latest
```

### Create `crates/flayer/src/audio_buffer.rs`

```rust
use anyhow::Result;

/// Stereo interleaved audio buffer
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,      // Interleaved L/R
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(num_samples: usize, sample_rate: u32) -> Self {
        Self {
            samples: vec![0.0; num_samples * 2],  // Stereo
            sample_rate,
        }
    }

    pub fn from_mono(mono: &[f32], sample_rate: u32) -> Self {
        let samples: Vec<f32> = mono.iter()
            .flat_map(|&s| [s, s])
            .collect();
        Self { samples, sample_rate }
    }

    pub fn num_frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn duration_seconds(&self) -> f64 {
        self.num_frames() as f64 / self.sample_rate as f64
    }

    /// Mix another buffer into this one at the given sample offset
    pub fn mix_at(&mut self, other: &AudioBuffer, offset_samples: usize, volume: f64, pan: f64) {
        let left_gain = (volume * (1.0 - pan.max(0.0))) as f32;
        let right_gain = (volume * (1.0 + pan.min(0.0))) as f32;

        for i in 0..other.num_frames() {
            let dst_idx = (offset_samples + i) * 2;
            let src_idx = i * 2;

            if dst_idx + 1 < self.samples.len() && src_idx + 1 < other.samples.len() {
                self.samples[dst_idx] += other.samples[src_idx] * left_gain;
                self.samples[dst_idx + 1] += other.samples[src_idx + 1] * right_gain;
            }
        }
    }

    /// Apply fade in/out
    pub fn apply_fades(&mut self, fade_in_samples: usize, fade_out_samples: usize) {
        let num_frames = self.num_frames();

        // Fade in
        for i in 0..fade_in_samples.min(num_frames) {
            let gain = i as f32 / fade_in_samples as f32;
            self.samples[i * 2] *= gain;
            self.samples[i * 2 + 1] *= gain;
        }

        // Fade out
        for i in 0..fade_out_samples.min(num_frames) {
            let frame = num_frames - 1 - i;
            let gain = i as f32 / fade_out_samples as f32;
            self.samples[frame * 2] *= gain;
            self.samples[frame * 2 + 1] *= gain;
        }
    }

    /// Apply gain
    pub fn apply_gain(&mut self, gain: f32) {
        for sample in &mut self.samples {
            *sample *= gain;
        }
    }

    /// Reverse the buffer
    pub fn reverse(&mut self) {
        let num_frames = self.num_frames();
        for i in 0..num_frames / 2 {
            let j = num_frames - 1 - i;
            self.samples.swap(i * 2, j * 2);
            self.samples.swap(i * 2 + 1, j * 2 + 1);
        }
    }

    /// Write to WAV file
    pub fn write_wav(&self, path: &str) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec)?;
        for &sample in &self.samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        Ok(())
    }

    /// Read from WAV file path
    pub fn read_wav(path: &str) -> Result<Self> {
        let reader = hound::WavReader::open(path)?;
        Self::read_wav_reader(reader)
    }

    /// Read from WAV bytes (for CAS loading)
    pub fn read_wav_bytes(bytes: &[u8]) -> Result<Self> {
        let cursor = std::io::Cursor::new(bytes);
        let reader = hound::WavReader::new(cursor)?;
        Self::read_wav_reader(reader)
    }

    fn read_wav_reader<R: std::io::Read>(reader: hound::WavReader<R>) -> Result<Self> {
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;

        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => {
                reader.into_samples::<f32>().filter_map(|s| s.ok()).collect()
            }
            hound::SampleFormat::Int => {
                let max = (1 << (spec.bits_per_sample - 1)) as f32;
                reader.into_samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / max)
                    .collect()
            }
        };

        // Convert mono to stereo if needed
        let samples = if spec.channels == 1 {
            samples.iter().flat_map(|&s| [s, s]).collect()
        } else {
            samples
        };

        Ok(Self { samples, sample_rate })
    }
}
```

### Create `crates/flayer/src/render.rs`

```rust
use crate::{AudioBuffer, Clip, ClipSource, Latent, Timeline, Track};
use crate::midi::Sequence;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// Context for rendering a timeline
pub struct RenderContext {
    pub sample_rate: u32,
    pub soundfont: Option<Arc<rustysynth::SoundFont>>,  // Loaded SF2 for MIDI rendering

    // Content storage access
    pub cas_loader: Box<dyn Fn(&str) -> Result<Vec<u8>> + Send + Sync>,

    // Cache for loaded assets
    audio_cache: HashMap<String, Arc<AudioBuffer>>,
    midi_cache: HashMap<String, Arc<Sequence>>,
}

impl RenderContext {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            soundfont: None,
            cas_loader: Box::new(|_| Err(anyhow!("No CAS loader configured"))),
            audio_cache: HashMap::new(),
            midi_cache: HashMap::new(),
        }
    }

    /// Load soundfont from bytes and store as Arc for reuse
    pub fn with_soundfont_bytes(mut self, sf_bytes: &[u8]) -> Result<Self> {
        let mut cursor = std::io::Cursor::new(sf_bytes);
        let sf = rustysynth::SoundFont::new(&mut cursor)?;
        self.soundfont = Some(Arc::new(sf));
        Ok(self)
    }

    /// Use pre-loaded soundfont
    pub fn with_soundfont(mut self, sf: Arc<rustysynth::SoundFont>) -> Self {
        self.soundfont = Some(sf);
        self
    }

    pub fn with_cas_loader<F>(mut self, loader: F) -> Self
    where
        F: Fn(&str) -> Result<Vec<u8>> + Send + Sync + 'static,
    {
        self.cas_loader = Box::new(loader);
        self
    }

    fn load_audio(&mut self, hash: &str) -> Result<Arc<AudioBuffer>> {
        if let Some(cached) = self.audio_cache.get(hash) {
            return Ok(Arc::clone(cached));
        }

        let bytes = (self.cas_loader)(hash)?;
        let buffer = AudioBuffer::read_wav_bytes(&bytes)?;
        let arc = Arc::new(buffer);
        self.audio_cache.insert(hash.to_string(), Arc::clone(&arc));
        Ok(arc)
    }

    fn load_midi(&mut self, hash: &str) -> Result<Arc<Sequence>> {
        if let Some(cached) = self.midi_cache.get(hash) {
            return Ok(Arc::clone(cached));
        }

        let bytes = (self.cas_loader)(hash)?;
        let seq = Sequence::from_smf(&bytes)?;
        let arc = Arc::new(seq);
        self.midi_cache.insert(hash.to_string(), Arc::clone(&arc));
        Ok(arc)
    }
}

impl Timeline {
    /// Render the entire timeline to audio
    pub fn render(&self, ctx: &mut RenderContext) -> Result<AudioBuffer> {
        let duration_beats = self.total_duration_beats();
        let duration_seconds = duration_beats * 60.0 / self.bpm;
        let num_samples = (duration_seconds * ctx.sample_rate as f64).ceil() as usize;

        let mut output = AudioBuffer::new(num_samples, ctx.sample_rate);

        // Render each track
        for track in &self.tracks {
            if track.muted {
                continue;
            }

            let track_buffer = self.render_track(track, ctx)?;
            output.mix_at(&track_buffer, 0, track.volume, track.pan);
        }

        // Render embeds
        for embed in &self.embeds {
            // TODO: Load embedded timeline and render
        }

        Ok(output)
    }

    fn render_track(&self, track: &Track, ctx: &mut RenderContext) -> Result<AudioBuffer> {
        let duration_beats = self.total_duration_beats();
        let duration_seconds = duration_beats * 60.0 / self.bpm;
        let num_samples = (duration_seconds * ctx.sample_rate as f64).ceil() as usize;

        let mut buffer = AudioBuffer::new(num_samples, ctx.sample_rate);

        // Render clips
        for clip in &track.clips {
            let clip_buffer = self.render_clip(clip, ctx)?;
            let offset_samples = self.beats_to_samples(clip.at, ctx.sample_rate);
            buffer.mix_at(&clip_buffer, offset_samples, clip.gain, 0.0);
        }

        // Render resolved latents
        for latent in &track.latents {
            if let Some(resolved_clip) = &latent.resolved {
                let clip_buffer = self.render_clip(resolved_clip, ctx)?;
                let offset_samples = self.beats_to_samples(latent.at, ctx.sample_rate);
                buffer.mix_at(&clip_buffer, offset_samples, resolved_clip.gain, 0.0);
            }
        }

        Ok(buffer)
    }

    fn render_clip(&self, clip: &Clip, ctx: &mut RenderContext) -> Result<AudioBuffer> {
        let mut buffer = match &clip.source {
            ClipSource::Audio(src) => {
                let full_audio = ctx.load_audio(&src.hash)?;
                self.slice_audio(&full_audio, clip, ctx)?
            }
            ClipSource::Midi(src) => {
                let seq = ctx.load_midi(&src.hash)?;
                self.render_midi_to_audio(&seq, clip, ctx)?
            }
        };

        // Apply clip properties
        if clip.reverse {
            buffer.reverse();
        }

        buffer.apply_gain(clip.gain as f32);

        let fade_in_samples = self.beats_to_samples(clip.fade_in, ctx.sample_rate);
        let fade_out_samples = self.beats_to_samples(clip.fade_out, ctx.sample_rate);
        buffer.apply_fades(fade_in_samples, fade_out_samples);

        Ok(buffer)
    }

    fn slice_audio(&self, source: &AudioBuffer, clip: &Clip, ctx: &RenderContext) -> Result<AudioBuffer> {
        // Calculate source window in samples
        let src_offset_samples = self.beats_to_samples(clip.source_offset, source.sample_rate as u32);
        let src_duration_samples = self.beats_to_samples(clip.source_duration, source.sample_rate as u32);

        // Extract the slice
        let start_idx = (src_offset_samples * 2).min(source.samples.len());
        let end_idx = ((src_offset_samples + src_duration_samples) * 2).min(source.samples.len());
        let slice_samples: Vec<f32> = source.samples[start_idx..end_idx].to_vec();

        let mut buffer = AudioBuffer {
            samples: slice_samples,
            sample_rate: source.sample_rate,
        };

        // Apply playback rate if != 1.0
        if (clip.playback_rate - 1.0).abs() > 0.001 {
            buffer = self.resample(&buffer, clip.playback_rate, ctx.sample_rate)?;
        }

        // Resample to target sample rate if needed
        if buffer.sample_rate != ctx.sample_rate {
            buffer = self.resample(&buffer, 1.0, ctx.sample_rate)?;
        }

        Ok(buffer)
    }

    /// Render MIDI to audio using rustysynth's MidiFileSequencer
    ///
    /// Two approaches are provided:
    /// 1. `render_midi_to_audio_sequencer` - Uses MidiFileSequencer for SMF bytes
    ///    (handles tempo maps automatically, preferred for standard MIDI files)
    /// 2. `render_midi_to_audio_manual` - Manual event processing for Sequence structs
    ///    (needed when working with already-parsed sequences)

    fn render_midi_to_audio(&self, seq: &Sequence, clip: &Clip, ctx: &RenderContext) -> Result<AudioBuffer> {
        // For parsed sequences, use manual rendering with tempo map support
        self.render_midi_to_audio_manual(seq, clip, ctx)
    }

    /// Render directly from SMF bytes using MidiFileSequencer
    /// This is more robust as rustysynth handles tempo changes natively.
    fn render_midi_to_audio_from_bytes(&self, midi_bytes: &[u8], ctx: &RenderContext) -> Result<AudioBuffer> {
        use rustysynth::MidiFileSequencer;

        let soundfont = ctx.soundfont.as_ref()
            .ok_or_else(|| anyhow!("No soundfont configured for MIDI rendering"))?;

        // Parse MIDI file with rustysynth
        let mut cursor = std::io::Cursor::new(midi_bytes);
        let midi_file = rustysynth::MidiFile::new(&mut cursor)
            .context("Failed to parse MIDI file with rustysynth")?;

        // Create synthesizer and sequencer
        let settings = rustysynth::SynthesizerSettings::new(ctx.sample_rate as i32);
        let synth = rustysynth::Synthesizer::new(soundfont, &settings)?;
        let mut sequencer = MidiFileSequencer::new(synth);

        // Start playback (false = don't loop)
        sequencer.play(&midi_file, false);

        // Calculate total duration
        let duration_seconds = midi_file.get_length();
        let num_samples = (duration_seconds * ctx.sample_rate as f64).ceil() as usize;

        // Render to buffers
        let mut left = vec![0.0f32; num_samples];
        let mut right = vec![0.0f32; num_samples];
        sequencer.render(&mut left, &mut right);

        // Interleave to stereo
        let samples: Vec<f32> = left.iter().zip(right.iter())
            .flat_map(|(&l, &r)| [l, r])
            .collect();

        Ok(AudioBuffer { samples, sample_rate: ctx.sample_rate })
    }

    /// Manual MIDI rendering for parsed Sequence structs
    /// Handles tempo map for accurate timing with tempo changes.
    fn render_midi_to_audio_manual(&self, seq: &Sequence, clip: &Clip, ctx: &RenderContext) -> Result<AudioBuffer> {
        let soundfont = ctx.soundfont.as_ref()
            .ok_or_else(|| anyhow!("No soundfont configured for MIDI rendering"))?;

        let settings = rustysynth::SynthesizerSettings::new(ctx.sample_rate as i32);
        let mut synth = rustysynth::Synthesizer::new(soundfont, &settings)?;

        // Use tempo map for accurate duration calculation
        let duration_seconds = seq.duration_seconds();
        let num_samples = (duration_seconds * ctx.sample_rate as f64).ceil() as usize;

        let mut left = vec![0.0f32; num_samples];
        let mut right = vec![0.0f32; num_samples];

        // Collect and sort all events by tick
        let mut all_events: Vec<_> = seq.tracks.iter()
            .flat_map(|t| t.events.iter())
            .collect();
        all_events.sort_by_key(|e| e.tick);

        let mut current_sample: usize = 0;

        for event in all_events {
            // Convert tick to sample using tempo map (handles tempo changes)
            let event_seconds = seq.tick_to_seconds(event.tick);
            let event_sample = (event_seconds * ctx.sample_rate as f64) as usize;

            // Render up to this event
            if event_sample > current_sample && current_sample < num_samples {
                let render_count = (event_sample - current_sample).min(num_samples - current_sample);
                synth.render(
                    &mut left[current_sample..current_sample + render_count],
                    &mut right[current_sample..current_sample + render_count],
                );
                current_sample = event_sample;
            }

            // Process MIDI event
            match &event.kind {
                crate::midi::EventKind::NoteOn { channel, pitch, velocity } => {
                    synth.note_on(*channel as i32, *pitch as i32, *velocity as i32);
                }
                crate::midi::EventKind::NoteOff { channel, pitch } => {
                    synth.note_off(*channel as i32, *pitch as i32);
                }
                crate::midi::EventKind::ControlChange { channel, controller, value } => {
                    synth.process_midi_message(*channel as i32, 0xB0, *controller as i32, *value as i32);
                }
                crate::midi::EventKind::ProgramChange { channel, program } => {
                    synth.process_midi_message(*channel as i32, 0xC0, *program as i32, 0);
                }
                crate::midi::EventKind::Tempo(_) => {
                    // Tempo changes are already accounted for in tick_to_seconds
                }
                _ => {}
            }
        }

        // Render any remaining samples
        if current_sample < num_samples {
            synth.render(&mut left[current_sample..], &mut right[current_sample..]);
        }

        // Interleave to stereo
        let samples: Vec<f32> = left.iter().zip(right.iter())
            .flat_map(|(&l, &r)| [l, r])
            .collect();

        Ok(AudioBuffer { samples, sample_rate: ctx.sample_rate })
    }

    fn resample(&self, source: &AudioBuffer, rate: f64, target_rate: u32) -> Result<AudioBuffer> {
        use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, Resampler, WindowFunction};

        let effective_rate = source.sample_rate as f64 * rate;
        let resample_ratio = target_rate as f64 / effective_rate;

        if (resample_ratio - 1.0).abs() < 0.001 {
            return Ok(source.clone());
        }

        // Configure sinc interpolation
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let chunk_size = 1024;
        let mut resampler = SincFixedIn::<f32>::new(
            resample_ratio,
            2.0,    // max_resample_ratio_relative
            params,
            chunk_size,
            2,      // channels
        )?;

        // Deinterleave to channel vectors
        let num_frames = source.num_frames();
        let mut left_src: Vec<f32> = Vec::with_capacity(num_frames);
        let mut right_src: Vec<f32> = Vec::with_capacity(num_frames);
        for i in 0..num_frames {
            left_src.push(source.samples[i * 2]);
            right_src.push(source.samples[i * 2 + 1]);
        }

        // Process in chunks
        let mut left_out = Vec::with_capacity((num_frames as f64 * resample_ratio) as usize + 1024);
        let mut right_out = Vec::with_capacity((num_frames as f64 * resample_ratio) as usize + 1024);

        let mut input_frames_next = resampler.input_frames_next();
        let mut current_frame = 0;

        while current_frame < num_frames {
            let end_frame = (current_frame + input_frames_next).min(num_frames);
            let chunk_len = end_frame - current_frame;

            let mut left_chunk = left_src[current_frame..end_frame].to_vec();
            let mut right_chunk = right_src[current_frame..end_frame].to_vec();

            // Pad last chunk with zeros if needed
            if chunk_len < input_frames_next {
                left_chunk.resize(input_frames_next, 0.0);
                right_chunk.resize(input_frames_next, 0.0);
            }

            let input = vec![left_chunk, right_chunk];
            let output = resampler.process(&input, None)?;

            // Output usually contains delay/padding, handled by caller or simple append here
            // Note: rubato might return different output sizes per chunk
            left_out.extend_from_slice(&output[0]);
            right_out.extend_from_slice(&output[1]);

            current_frame += input_frames_next;
            input_frames_next = resampler.input_frames_next();
        }

        // Reinterleave
        // Truncate if we padded too much (simple heuristic or exact calc)
        // For now, return what we got, it might have a few ms of silence at end
        let result_len = left_out.len();
        let mut samples: Vec<f32> = Vec::with_capacity(result_len * 2);
        for i in 0..result_len {
            samples.push(left_out[i]);
            samples.push(right_out[i]);
        }

        Ok(AudioBuffer { samples, sample_rate: target_rate })
    }

    fn beats_to_samples(&self, beats: f64, sample_rate: u32) -> usize {
        let seconds = beats * 60.0 / self.bpm;
        (seconds * sample_rate as f64) as usize
    }
}
```

### Update `crates/flayer/src/lib.rs`

```rust
pub mod audio_buffer;
pub mod render;

pub use audio_buffer::AudioBuffer;
pub use render::RenderContext;
```

## Acceptance Criteria

- [ ] `Timeline::render()` produces valid stereo WAV
- [ ] Audio clips load from CAS and mix correctly
- [ ] MIDI clips render through SoundFont
- [ ] Playback rate changes work (resampling)
- [ ] Fades apply correctly
- [ ] Track volume/pan affect output
- [ ] Multiple clips on same track mix additively

## Tests to Write

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_render_audio_clip() {
        // Create timeline with audio clip, render, verify duration
    }

    #[test]
    fn test_render_midi_clip() {
        // Create timeline with MIDI clip + soundfont, render, verify output
    }

    #[test]
    fn test_track_mixing() {
        // Create timeline with multiple tracks, different volumes, verify mix
    }

    #[test]
    fn test_playback_rate() {
        // Create clip with 0.5 rate, verify output is stretched
    }
}
```
