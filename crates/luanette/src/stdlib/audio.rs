//! Audio processing for Lua scripts.
//!
//! Provides `audio.*` namespace with local audio manipulation (no ZMQ overhead).
//! Uses hound for WAV I/O and rubato for high-quality resampling.
//!
//! # Sample Format
//!
//! Audio samples are represented as Lua tables:
//! ```lua
//! {
//!     data = { 0.0, 0.1, -0.2, ... },  -- interleaved f32 samples
//!     channels = 2,                     -- 1 = mono, 2 = stereo
//!     sample_rate = 44100,
//! }
//! ```
//!
//! # Usage
//!
//! ```lua
//! -- Read a WAV file
//! local samples, info = audio.read_wav("/path/to/file.wav")
//! -- or from CAS hash
//! local samples, info = audio.read_wav("abc123...")
//!
//! -- Get info without reading samples
//! local info = audio.info("abc123...")
//! -- info = { sample_rate=44100, channels=2, duration=3.5, bits=16, samples=154350 }
//!
//! -- Resample for BeatThis (needs 22050 Hz mono)
//! local resampled = audio.resample(samples, 44100, 22050, "high")
//! local mono = audio.to_mono(resampled)
//!
//! -- Normalize and trim
//! local normalized = audio.normalize(mono, -1.0)  -- to -1 dB
//! local trimmed = audio.trim(normalized, 0.5, 10.0)  -- 0.5s to 10s
//!
//! -- Write output
//! audio.write_wav(trimmed, "/tmp/output.wav", { bits = 16 })
//!
//! -- Generate silence for padding
//! local silence = audio.silence(1.0, 44100, 1)  -- 1 second mono
//!
//! -- Mix multiple sources
//! local mixed = audio.mix({track1, track2}, {0.7, 0.3})
//! ```

use anyhow::{Context, Result};
use cas::ContentStore;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use mlua::{Lua, Table};
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};
use std::io::Cursor;

use super::cas::get_store;

/// Load audio bytes from hash or path.
fn load_audio_bytes(hash_or_path: &str) -> Result<Vec<u8>> {
    // Try as CAS hash first (64 char hex)
    if hash_or_path.len() == 64 && hash_or_path.chars().all(|c| c.is_ascii_hexdigit()) {
        let store = get_store();
        let content_hash = hash_or_path.parse()
            .context("Invalid CAS hash")?;
        if let Some(path) = store.path(&content_hash) {
            return std::fs::read(&path)
                .with_context(|| format!("Failed to read audio from CAS: {}", hash_or_path));
        }
    }

    // Try as file path
    std::fs::read(hash_or_path)
        .with_context(|| format!("Failed to read audio: {}", hash_or_path))
}

/// Register the `audio` global table.
pub fn register_audio_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();
    let audio_table = lua.create_table()?;

    // =====================
    // I/O Functions
    // =====================

    // audio.read_wav(hash_or_path) -> samples, info
    let read_wav_fn = lua.create_function(|lua, hash_or_path: String| {
        let bytes = load_audio_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let cursor = Cursor::new(&bytes);
        let reader = WavReader::new(cursor)
            .map_err(|e| mlua::Error::external(format!("Invalid WAV: {}", e)))?;

        let spec = reader.spec();
        let num_samples = reader.len() as usize;
        let duration = num_samples as f64 / spec.sample_rate as f64 / spec.channels as f64;

        // Read samples as f32
        let samples_f32: Vec<f32> = match spec.sample_format {
            SampleFormat::Float => {
                reader.into_samples::<f32>()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| mlua::Error::external(format!("Failed to read samples: {}", e)))?
            }
            SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_val = (1i64 << (bits - 1)) as f32;
                reader.into_samples::<i32>()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| mlua::Error::external(format!("Failed to read samples: {}", e)))?
                    .into_iter()
                    .map(|s| s as f32 / max_val)
                    .collect()
            }
        };

        // Build samples table
        let samples = lua.create_table()?;
        let data = lua.create_table()?;
        for (i, &s) in samples_f32.iter().enumerate() {
            data.set(i + 1, s)?;
        }
        samples.set("data", data)?;
        samples.set("channels", spec.channels)?;
        samples.set("sample_rate", spec.sample_rate)?;

        // Build info table
        let info = lua.create_table()?;
        info.set("sample_rate", spec.sample_rate)?;
        info.set("channels", spec.channels)?;
        info.set("bits", spec.bits_per_sample)?;
        info.set("duration", duration)?;
        info.set("samples", num_samples)?;
        info.set("format", "wav")?;

        Ok((samples, info))
    })?;
    audio_table.set("read_wav", read_wav_fn)?;

    // audio.write_wav(samples, path, opts?) -> path
    let write_wav_fn = lua.create_function(|_, (samples, path, opts): (Table, String, Option<Table>)| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        let bits: u16 = opts.as_ref()
            .and_then(|o| o.get("bits").ok())
            .unwrap_or(16);

        // Collect samples
        let mut samples_f32: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            samples_f32.push(s);
        }

        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: bits,
            sample_format: if bits == 32 { SampleFormat::Float } else { SampleFormat::Int },
        };

        // Create parent dirs if needed
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| mlua::Error::external(format!("Failed to create directory: {}", e)))?;
        }

        let mut writer = WavWriter::create(&path, spec)
            .map_err(|e| mlua::Error::external(format!("Failed to create WAV: {}", e)))?;

        if bits == 32 {
            for s in samples_f32 {
                writer.write_sample(s)
                    .map_err(|e| mlua::Error::external(format!("Write error: {}", e)))?;
            }
        } else {
            let max_val = (1i32 << (bits - 1)) - 1;
            for s in samples_f32 {
                let clamped = s.clamp(-1.0, 1.0);
                let int_sample = (clamped * max_val as f32) as i32;
                writer.write_sample(int_sample)
                    .map_err(|e| mlua::Error::external(format!("Write error: {}", e)))?;
            }
        }

        writer.finalize()
            .map_err(|e| mlua::Error::external(format!("Failed to finalize: {}", e)))?;

        Ok(path)
    })?;
    audio_table.set("write_wav", write_wav_fn)?;

    // audio.info(hash_or_path) -> info table
    let info_fn = lua.create_function(|lua, hash_or_path: String| {
        let bytes = load_audio_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let cursor = Cursor::new(&bytes);
        let reader = WavReader::new(cursor)
            .map_err(|e| mlua::Error::external(format!("Invalid WAV: {}", e)))?;

        let spec = reader.spec();
        let num_samples = reader.len() as usize;
        let duration = num_samples as f64 / spec.sample_rate as f64 / spec.channels as f64;

        let info = lua.create_table()?;
        info.set("sample_rate", spec.sample_rate)?;
        info.set("channels", spec.channels)?;
        info.set("bits", spec.bits_per_sample)?;
        info.set("duration", duration)?;
        info.set("samples", num_samples)?;
        info.set("format", "wav")?;

        Ok(info)
    })?;
    audio_table.set("info", info_fn)?;

    // audio.duration(hash_or_path) -> seconds
    let duration_fn = lua.create_function(|_, hash_or_path: String| {
        let bytes = load_audio_bytes(&hash_or_path).map_err(mlua::Error::external)?;
        let cursor = Cursor::new(&bytes);
        let reader = WavReader::new(cursor)
            .map_err(|e| mlua::Error::external(format!("Invalid WAV: {}", e)))?;

        let spec = reader.spec();
        let num_samples = reader.len() as usize;
        let duration = num_samples as f64 / spec.sample_rate as f64 / spec.channels as f64;

        Ok(duration)
    })?;
    audio_table.set("duration", duration_fn)?;

    // =====================
    // Format Conversion
    // =====================

    // audio.resample(samples, from_rate, to_rate, quality?) -> samples
    // quality: "quick", "medium", "high" (default)
    let resample_fn = lua.create_function(|lua, (samples, from_rate, to_rate, quality): (Table, u32, u32, Option<String>)| {
        if from_rate == to_rate {
            return Ok(samples);
        }

        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);

        // Collect input samples
        let mut input_f32: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            input_f32.push(s);
        }

        // Configure resampler based on quality
        let quality_str = quality.as_deref().unwrap_or("high");
        let (sinc_len, oversampling) = match quality_str {
            "quick" => (64, 64),
            "medium" => (128, 128),
            "high" | _ => (256, 256),
        };

        let params = SincInterpolationParameters {
            sinc_len,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: oversampling,
            window: WindowFunction::BlackmanHarris2,
        };

        let ratio = to_rate as f64 / from_rate as f64;
        let frames = input_f32.len() / channels as usize;

        // Split into channels
        let mut channel_data: Vec<Vec<f32>> = (0..channels)
            .map(|_| Vec::with_capacity(frames))
            .collect();

        for (i, &s) in input_f32.iter().enumerate() {
            let ch = i % channels as usize;
            channel_data[ch].push(s);
        }

        let mut resampler = SincFixedIn::<f32>::new(
            ratio,
            2.0,
            params,
            frames,
            channels as usize,
        ).map_err(|e| mlua::Error::external(format!("Resampler error: {}", e)))?;

        let output_channels = resampler.process(&channel_data, None)
            .map_err(|e| mlua::Error::external(format!("Resample failed: {}", e)))?;

        // Interleave output
        let out_frames = output_channels.first().map(|c| c.len()).unwrap_or(0);
        let mut output_f32 = Vec::with_capacity(out_frames * channels as usize);
        for frame in 0..out_frames {
            for ch in 0..channels as usize {
                output_f32.push(output_channels[ch][frame]);
            }
        }

        // Build result
        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in output_f32.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", to_rate)?;

        Ok(result)
    })?;
    audio_table.set("resample", resample_fn)?;

    // audio.to_mono(samples) -> samples
    let to_mono_fn = lua.create_function(|lua, samples: Table| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        if channels == 1 {
            return Ok(samples);
        }

        let mut input_f32: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            input_f32.push(s);
        }

        // Average channels
        let mono: Vec<f32> = input_f32
            .chunks(channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect();

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in mono.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", 1)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("to_mono", to_mono_fn)?;

    // audio.to_stereo(samples) -> samples
    let to_stereo_fn = lua.create_function(|lua, samples: Table| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        if channels >= 2 {
            return Ok(samples);
        }

        let mut input_f32: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            input_f32.push(s);
        }

        // Duplicate mono to stereo
        let stereo: Vec<f32> = input_f32
            .iter()
            .flat_map(|&s| [s, s])
            .collect();

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in stereo.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", 2)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("to_stereo", to_stereo_fn)?;

    // =====================
    // Time Operations
    // =====================

    // audio.trim(samples, start_sec, end_sec?) -> samples
    let trim_fn = lua.create_function(|lua, (samples, start_sec, end_sec): (Table, f64, Option<f64>)| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        let mut input_f32: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            input_f32.push(s);
        }

        let total_frames = input_f32.len() / channels as usize;
        let start_frame = (start_sec * sample_rate as f64) as usize;
        let end_frame = end_sec
            .map(|e| (e * sample_rate as f64) as usize)
            .unwrap_or(total_frames)
            .min(total_frames);

        if start_frame >= end_frame {
            return Err(mlua::Error::external("Invalid trim range"));
        }

        let start_sample = start_frame * channels as usize;
        let end_sample = end_frame * channels as usize;
        let trimmed: Vec<f32> = input_f32[start_sample..end_sample].to_vec();

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in trimmed.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("trim", trim_fn)?;

    // audio.pad(samples, before_sec, after_sec) -> samples
    let pad_fn = lua.create_function(|lua, (samples, before_sec, after_sec): (Table, f64, f64)| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        let mut input_f32: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            input_f32.push(s);
        }

        let before_samples = (before_sec * sample_rate as f64) as usize * channels as usize;
        let after_samples = (after_sec * sample_rate as f64) as usize * channels as usize;

        let mut padded = vec![0.0f32; before_samples];
        padded.extend(&input_f32);
        padded.extend(vec![0.0f32; after_samples]);

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in padded.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("pad", pad_fn)?;

    // audio.fade(samples, fade_in_sec, fade_out_sec, type?) -> samples
    // type: "linear" (default), "log", "quarter"
    let fade_fn = lua.create_function(|lua, (samples, fade_in, fade_out, fade_type): (Table, f64, f64, Option<String>)| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        let mut audio: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            audio.push(s);
        }

        let total_frames = audio.len() / channels as usize;
        let fade_in_frames = (fade_in * sample_rate as f64) as usize;
        let fade_out_frames = (fade_out * sample_rate as f64) as usize;
        let fade_type_str = fade_type.as_deref().unwrap_or("linear");

        // Apply fade curve
        let curve = |t: f64, fade_type: &str| -> f64 {
            match fade_type {
                "log" => (t * 10.0 + 1.0).ln() / (11.0_f64).ln(),
                "quarter" => (t * std::f64::consts::FRAC_PI_2).sin(),
                "linear" | _ => t,
            }
        };

        // Fade in
        for frame in 0..fade_in_frames.min(total_frames) {
            let t = frame as f64 / fade_in_frames as f64;
            let gain = curve(t, fade_type_str) as f32;
            for ch in 0..channels as usize {
                let idx = frame * channels as usize + ch;
                audio[idx] *= gain;
            }
        }

        // Fade out
        let fade_out_start = total_frames.saturating_sub(fade_out_frames);
        for frame in fade_out_start..total_frames {
            let t = (total_frames - frame) as f64 / fade_out_frames as f64;
            let gain = curve(t, fade_type_str) as f32;
            for ch in 0..channels as usize {
                let idx = frame * channels as usize + ch;
                audio[idx] *= gain;
            }
        }

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in audio.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("fade", fade_fn)?;

    // =====================
    // Level Operations
    // =====================

    // audio.gain(samples, db) -> samples
    let gain_fn = lua.create_function(|lua, (samples, db): (Table, f64)| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        let gain = 10.0_f64.powf(db / 20.0) as f32;

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            out_data.set(i, s * gain)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("gain", gain_fn)?;

    // audio.peak(samples) -> db
    let peak_fn = lua.create_function(|_, samples: Table| {
        let data: Table = samples.get("data")?;

        let mut max_abs: f32 = 0.0;
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            max_abs = max_abs.max(s.abs());
        }

        let db = if max_abs > 0.0 {
            20.0 * (max_abs as f64).log10()
        } else {
            -f64::INFINITY
        };

        Ok(db)
    })?;
    audio_table.set("peak", peak_fn)?;

    // audio.rms(samples) -> db
    let rms_fn = lua.create_function(|_, samples: Table| {
        let data: Table = samples.get("data")?;

        let mut sum_sq: f64 = 0.0;
        let len = data.raw_len();
        for i in 1..=len {
            let s: f32 = data.get(i)?;
            sum_sq += (s as f64) * (s as f64);
        }

        let rms = (sum_sq / len as f64).sqrt();
        let db = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            -f64::INFINITY
        };

        Ok(db)
    })?;
    audio_table.set("rms", rms_fn)?;

    // audio.normalize(samples, target_db?) -> samples
    // Default target is 0 dB (peak normalization)
    let normalize_fn = lua.create_function(|lua, (samples, target_db): (Table, Option<f64>)| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);
        let target = target_db.unwrap_or(0.0);

        // Find peak
        let mut max_abs: f32 = 0.0;
        let mut audio: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            max_abs = max_abs.max(s.abs());
            audio.push(s);
        }

        if max_abs == 0.0 {
            return Ok(samples);
        }

        // Calculate gain needed
        let current_db = 20.0 * (max_abs as f64).log10();
        let gain_db = target - current_db;
        let gain = 10.0_f64.powf(gain_db / 20.0) as f32;

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in audio.iter().enumerate() {
            out_data.set(i + 1, s * gain)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("normalize", normalize_fn)?;

    // =====================
    // Utility Functions
    // =====================

    // audio.silence(duration_sec, sample_rate, channels?) -> samples
    let silence_fn = lua.create_function(|lua, (duration, sample_rate, channels): (f64, u32, Option<u16>)| {
        let ch = channels.unwrap_or(1);
        let num_samples = (duration * sample_rate as f64) as usize * ch as usize;

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for i in 1..=num_samples {
            out_data.set(i, 0.0f32)?;
        }
        result.set("data", out_data)?;
        result.set("channels", ch)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("silence", silence_fn)?;

    // audio.mix(samples_list, gains?) -> samples
    let mix_fn = lua.create_function(|lua, (samples_list, gains): (Table, Option<Table>)| {
        let num_inputs = samples_list.raw_len();
        if num_inputs == 0 {
            return Err(mlua::Error::external("No inputs to mix"));
        }

        // Get first input for reference
        let first: Table = samples_list.get(1)?;
        let channels: u16 = first.get("channels").unwrap_or(1);
        let sample_rate: u32 = first.get("sample_rate").unwrap_or(44100);

        // Collect all inputs with gains
        let mut inputs: Vec<(Vec<f32>, f32)> = Vec::new();
        let mut max_len = 0usize;

        for i in 1..=num_inputs {
            let samples: Table = samples_list.get(i)?;
            let data: Table = samples.get("data")?;

            let mut audio: Vec<f32> = Vec::new();
            for j in 1..=data.raw_len() {
                let s: f32 = data.get(j)?;
                audio.push(s);
            }
            max_len = max_len.max(audio.len());

            let gain: f32 = gains.as_ref()
                .and_then(|g| g.get::<f32>(i).ok())
                .unwrap_or(1.0 / num_inputs as f32);

            inputs.push((audio, gain));
        }

        // Mix
        let mut mixed = vec![0.0f32; max_len];
        for (audio, gain) in inputs {
            for (i, &s) in audio.iter().enumerate() {
                mixed[i] += s * gain;
            }
        }

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in mixed.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("mix", mix_fn)?;

    // audio.concat(samples_list) -> samples
    let concat_fn = lua.create_function(|lua, samples_list: Table| {
        let num_inputs = samples_list.raw_len();
        if num_inputs == 0 {
            return Err(mlua::Error::external("No inputs to concat"));
        }

        let first: Table = samples_list.get(1)?;
        let channels: u16 = first.get("channels").unwrap_or(1);
        let sample_rate: u32 = first.get("sample_rate").unwrap_or(44100);

        let mut concatenated: Vec<f32> = Vec::new();

        for i in 1..=num_inputs {
            let samples: Table = samples_list.get(i)?;
            let data: Table = samples.get("data")?;

            for j in 1..=data.raw_len() {
                let s: f32 = data.get(j)?;
                concatenated.push(s);
            }
        }

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in concatenated.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("concat", concat_fn)?;

    // audio.reverse(samples) -> samples
    let reverse_fn = lua.create_function(|lua, samples: Table| {
        let data: Table = samples.get("data")?;
        let channels: u16 = samples.get("channels").unwrap_or(1);
        let sample_rate: u32 = samples.get("sample_rate").unwrap_or(44100);

        let mut audio: Vec<f32> = Vec::new();
        for i in 1..=data.raw_len() {
            let s: f32 = data.get(i)?;
            audio.push(s);
        }

        // Reverse by frames (keep channels together)
        let frames: Vec<_> = audio.chunks(channels as usize).collect();
        let reversed: Vec<f32> = frames.iter().rev().flat_map(|f| f.iter().copied()).collect();

        let result = lua.create_table()?;
        let out_data = lua.create_table()?;
        for (i, &s) in reversed.iter().enumerate() {
            out_data.set(i + 1, s)?;
        }
        result.set("data", out_data)?;
        result.set("channels", channels)?;
        result.set("sample_rate", sample_rate)?;

        Ok(result)
    })?;
    audio_table.set("reverse", reverse_fn)?;

    globals.set("audio", audio_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    #[test]
    fn test_register_audio_globals() {
        let lua = Lua::new();
        register_audio_globals(&lua).unwrap();

        let globals = lua.globals();
        let audio: Table = globals.get("audio").unwrap();

        // I/O
        assert!(audio.contains_key("read_wav").unwrap());
        assert!(audio.contains_key("write_wav").unwrap());
        assert!(audio.contains_key("info").unwrap());
        assert!(audio.contains_key("duration").unwrap());

        // Format conversion
        assert!(audio.contains_key("resample").unwrap());
        assert!(audio.contains_key("to_mono").unwrap());
        assert!(audio.contains_key("to_stereo").unwrap());

        // Time operations
        assert!(audio.contains_key("trim").unwrap());
        assert!(audio.contains_key("pad").unwrap());
        assert!(audio.contains_key("fade").unwrap());

        // Level operations
        assert!(audio.contains_key("gain").unwrap());
        assert!(audio.contains_key("peak").unwrap());
        assert!(audio.contains_key("rms").unwrap());
        assert!(audio.contains_key("normalize").unwrap());

        // Utility
        assert!(audio.contains_key("silence").unwrap());
        assert!(audio.contains_key("mix").unwrap());
        assert!(audio.contains_key("concat").unwrap());
        assert!(audio.contains_key("reverse").unwrap());
    }

    #[test]
    fn test_audio_silence() {
        let lua = Lua::new();
        register_audio_globals(&lua).unwrap();

        let code = r#"
            local s = audio.silence(0.1, 44100, 1)
            return s.channels, s.sample_rate, #s.data
        "#;

        let (ch, sr, len): (u16, u32, usize) = lua.load(code).eval().unwrap();
        assert_eq!(ch, 1);
        assert_eq!(sr, 44100);
        assert_eq!(len, 4410); // 0.1 * 44100
    }

    #[test]
    fn test_audio_gain() {
        let lua = Lua::new();
        register_audio_globals(&lua).unwrap();

        let code = r#"
            local s = { data = {0.5, 0.5}, channels = 1, sample_rate = 44100 }
            local gained = audio.gain(s, 6.0)  -- +6 dB ≈ 2x
            return gained.data[1]
        "#;

        let result: f32 = lua.load(code).eval().unwrap();
        assert!((result - 1.0).abs() < 0.1); // ~2x gain
    }

    #[test]
    fn test_audio_to_mono() {
        let lua = Lua::new();
        register_audio_globals(&lua).unwrap();

        let code = r#"
            local s = { data = {1.0, 0.0, 0.5, 0.5}, channels = 2, sample_rate = 44100 }
            local mono = audio.to_mono(s)
            return mono.channels, #mono.data, mono.data[1], mono.data[2]
        "#;

        let (ch, len, s1, s2): (u16, usize, f32, f32) = lua.load(code).eval().unwrap();
        assert_eq!(ch, 1);
        assert_eq!(len, 2);
        assert_eq!(s1, 0.5); // avg(1.0, 0.0)
        assert_eq!(s2, 0.5); // avg(0.5, 0.5)
    }

    #[test]
    fn test_audio_peak() {
        let lua = Lua::new();
        register_audio_globals(&lua).unwrap();

        let code = r#"
            local s = { data = {0.5, -0.5, 0.25}, channels = 1, sample_rate = 44100 }
            return audio.peak(s)
        "#;

        let db: f64 = lua.load(code).eval().unwrap();
        // peak of 0.5 = 20 * log10(0.5) ≈ -6.02 dB
        assert!((db - (-6.02)).abs() < 0.1);
    }
}
