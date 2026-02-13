pub mod analyzer;
pub mod cache;
pub mod chord_templates;
pub mod chords;
pub mod key;
pub mod meter;
pub mod types;

pub use analyzer::{HeuristicAnalyzer, MusicAnalyzer};
pub use cache::AnalysisCache;
pub use key::key_to_abc;
pub use types::{
    ChordEvent, ChordQuality, ClassifiedVoice, KeyDetection, KeyMode, MeterDetection,
    MusicUnderstanding,
};

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;

/// Current algorithm version — bump to invalidate cache.
pub const CURRENT_VERSION: u32 = 1;

/// Unified music understanding engine.
///
/// Composes MIDI analysis, voice separation, key/meter/chord detection
/// into a single cached `MusicUnderstanding` result. Results are cached
/// in SQLite by `(content_hash, version)`.
pub struct MusicUnderstandingEngine {
    analyzer: Arc<dyn MusicAnalyzer>,
    cache: AnalysisCache,
    cas_dir: PathBuf,
}

impl MusicUnderstandingEngine {
    /// Create with the default heuristic analyzer.
    pub fn new(cas_dir: PathBuf, cache_db_path: PathBuf) -> Result<Self> {
        let cache = AnalysisCache::open(&cache_db_path)
            .context("opening music understanding cache")?;

        Ok(Self {
            analyzer: Arc::new(HeuristicAnalyzer),
            cache,
            cas_dir,
        })
    }

    /// Create with a custom analyzer (for testing or future ML backend).
    pub fn with_analyzer(
        analyzer: Arc<dyn MusicAnalyzer>,
        cas_dir: PathBuf,
        cache_db_path: PathBuf,
    ) -> Result<Self> {
        let cache = AnalysisCache::open(&cache_db_path)
            .context("opening music understanding cache")?;

        Ok(Self {
            analyzer,
            cache,
            cas_dir,
        })
    }

    /// Analyze a MIDI file by CAS hash, returning cached results when available.
    pub fn understand(&self, content_hash: &str) -> Result<MusicUnderstanding> {
        // 1. Check cache
        if let Some(cached) = self.cache.get(content_hash, CURRENT_VERSION)? {
            info!(hash = content_hash, "music understanding cache hit");
            return Ok(cached);
        }

        info!(hash = content_hash, "music understanding cache miss, computing");

        // 2. Read MIDI bytes from CAS
        let midi_bytes = self.read_cas(content_hash)?;

        // 3. Compute understanding
        let understanding = self.compute(content_hash, &midi_bytes)?;

        // 4. Cache result
        self.cache.put(&understanding)?;

        Ok(understanding)
    }

    /// Compute understanding from raw MIDI bytes (no cache interaction).
    pub fn compute(&self, content_hash: &str, midi_bytes: &[u8]) -> Result<MusicUnderstanding> {
        let smf = midly::Smf::parse(midi_bytes)
            .map_err(|e| anyhow::anyhow!("MIDI parse error: {}", e))?;

        // Extract notes and context
        let (all_notes, context) = midi_analysis::analyze::extract_notes(&smf);

        // Profile tracks
        let track_profiles =
            midi_analysis::analyze::profile_tracks(&smf, &all_notes, &context, 0.3);

        // Separate voices from tracks that need it
        let mut all_voices = Vec::new();
        let params = midi_analysis::SeparationParams::default();

        for profile in &track_profiles {
            let track_notes: Vec<_> = all_notes
                .iter()
                .filter(|n| n.track_index == profile.track_index)
                .cloned()
                .collect();

            if track_notes.is_empty() {
                continue;
            }

            if profile.merged_voices_likely {
                let voices =
                    midi_analysis::separate_voices(&track_notes, context.ppq, &params);
                all_voices.extend(voices);
            } else {
                // Single voice track
                all_voices.push(midi_analysis::SeparatedVoice {
                    notes: track_notes.clone(),
                    method: midi_analysis::SeparationMethod::AlreadyMonophonic,
                    voice_index: 0,
                    stats: midi_analysis::note::VoiceStats::from_notes(&track_notes),
                    source_channel: track_notes.first().map(|n| n.channel),
                    source_track: Some(profile.track_index),
                });
            }
        }

        // Classify voices
        let classified =
            self.analyzer
                .classify_voices(&all_voices, &context, &track_profiles);

        // Analyze key (using all non-percussion notes)
        let analysis_notes: Vec<_> = classified
            .iter()
            .filter(|v| {
                !matches!(
                    v.role,
                    midi_analysis::VoiceRole::Percussion | midi_analysis::VoiceRole::Rhythm
                )
            })
            .flat_map(|v| v.notes.iter().cloned())
            .collect();

        let key_detection = self.analyzer.analyze_key(&analysis_notes, &context);

        // Analyze meter (using all notes for onset density)
        let meter_detection = self.analyzer.analyze_meter(&all_notes, &context);

        // Extract chords (using harmony + bass partitioning)
        let (harmony_refs, bass_refs) = analyzer::partition_voices(&classified);
        let harmony_notes: Vec<_> = harmony_refs.into_iter().cloned().collect();
        let bass_notes: Vec<_> = bass_refs.into_iter().cloned().collect();
        let chord_events =
            self.analyzer
                .extract_chords(&harmony_notes, &bass_notes, &context, &key_detection);

        Ok(MusicUnderstanding {
            content_hash: content_hash.to_string(),
            version: CURRENT_VERSION,
            context,
            key: key_detection,
            meter: meter_detection,
            voices: classified,
            chords: chord_events,
        })
    }

    fn read_cas(&self, content_hash: &str) -> Result<Vec<u8>> {
        // CAS stores files at {cas_dir}/{prefix}/{hash}
        // The prefix is the first 2 chars of the hash
        let prefix = &content_hash[..2.min(content_hash.len())];
        let path = self.cas_dir.join(prefix).join(content_hash);

        std::fs::read(&path).with_context(|| format!("reading CAS content: {}", path.display()))
    }
}
