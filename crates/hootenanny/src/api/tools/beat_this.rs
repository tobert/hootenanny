//! BeatThis audio analysis utilities
//!
//! Provides audio preparation for the beat-this service.

use hooteproto::ToolError;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::io::Cursor;

const REQUIRED_SAMPLE_RATE: u32 = 22050;

/// Prepare audio for BeatThis: convert to mono 22050 Hz WAV
///
/// Handles:
/// - Stereo → mono conversion (averages channels)
/// - Sample rate conversion via rubato (high-quality sinc resampling)
/// - Returns WAV bytes ready for the service
pub fn prepare_audio_for_beatthis(data: &[u8]) -> Result<Vec<u8>, ToolError> {
    let cursor = Cursor::new(data);
    let reader = hound::WavReader::new(cursor)
        .map_err(|e| ToolError::validation("invalid_params", format!("Invalid WAV file: {}", e)))?;

    let spec = reader.spec();
    let source_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    tracing::debug!(
        source_rate,
        channels,
        bits = spec.bits_per_sample,
        "Reading WAV for beat analysis"
    );

    // Read samples as f32
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ToolError::internal(format!("Failed to read samples: {}", e)))?,
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i32 << (bits - 1)) as f32;
            reader
                .into_samples::<i32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ToolError::internal(format!("Failed to read samples: {}", e)))?
                .into_iter()
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    // Convert to mono if stereo (average channels)
    let mono_samples: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    // Resample if needed
    let final_samples = if source_rate != REQUIRED_SAMPLE_RATE {
        tracing::info!(
            from = source_rate,
            to = REQUIRED_SAMPLE_RATE,
            "Resampling audio for BeatThis"
        );
        resample_audio(&mono_samples, source_rate, REQUIRED_SAMPLE_RATE)?
    } else {
        mono_samples
    };

    // Write output WAV
    let mut output = Cursor::new(Vec::new());
    let out_spec = hound::WavSpec {
        channels: 1,
        sample_rate: REQUIRED_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::new(&mut output, out_spec)
        .map_err(|e| ToolError::internal(format!("Failed to create WAV writer: {}", e)))?;

    for sample in final_samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = (clamped * 32767.0) as i16;
        writer
            .write_sample(int_sample)
            .map_err(|e| ToolError::internal(format!("Failed to write sample: {}", e)))?;
    }

    writer
        .finalize()
        .map_err(|e| ToolError::internal(format!("Failed to finalize WAV: {}", e)))?;

    Ok(output.into_inner())
}

/// Resample audio using rubato's high-quality sinc interpolation
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, ToolError> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let ratio = to_rate as f64 / from_rate as f64;

    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0, // max relative ratio (allows some flexibility)
        params,
        samples.len(),
        1, // mono
    )
    .map_err(|e| ToolError::internal(format!("Failed to create resampler: {}", e)))?;

    let input = vec![samples.to_vec()];
    let output = resampler
        .process(&input, None)
        .map_err(|e| ToolError::internal(format!("Resampling failed: {}", e)))?;

    Ok(output.into_iter().next().unwrap_or_default())
}
