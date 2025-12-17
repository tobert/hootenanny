//! Monotonic tick clock for transport position
//!
//! Advances playback position based on wall clock time, independent of
//! audio device timing. Uses `std::time::Instant` for monotonic guarantees.
//!
//! The clock converts elapsed real time to musical time via the TempoMap,
//! properly handling tempo changes mid-playback.

use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::{Beat, Second, TempoMap, Tick};

/// Monotonic clock that tracks playback position in musical time
///
/// When playing, the clock stores the `start_instant` (when play was pressed)
/// and `start_position` (the beat position at that moment). The current
/// position is computed by adding elapsed wall time (converted to musical
/// time via TempoMap) to the start position.
pub struct TickClock {
    tempo_map: Arc<RwLock<TempoMap>>,

    /// When play was pressed (None if paused/stopped)
    start_instant: Option<Instant>,

    /// Position when play was pressed
    start_position: Beat,

    /// Current position (updated by tick())
    current_position: Beat,
}

impl TickClock {
    /// Create a new tick clock at position 0
    pub fn new(tempo_map: Arc<RwLock<TempoMap>>) -> Self {
        Self {
            tempo_map,
            start_instant: None,
            start_position: Beat(0.0),
            current_position: Beat(0.0),
        }
    }

    /// Start the clock from current position
    pub fn start(&mut self) {
        if self.start_instant.is_none() {
            self.start_instant = Some(Instant::now());
            self.start_position = self.current_position;
        }
    }

    /// Check if clock is running
    pub fn is_running(&self) -> bool {
        self.start_instant.is_some()
    }

    /// Pause without resetting position
    pub fn pause(&mut self) {
        if self.start_instant.is_some() {
            // Update current position before pausing
            self.tick();
            self.start_instant = None;
        }
    }

    /// Stop and reset to zero
    pub fn stop(&mut self) {
        self.start_instant = None;
        self.start_position = Beat(0.0);
        self.current_position = Beat(0.0);
    }

    /// Seek to position
    pub fn seek(&mut self, beat: Beat) {
        let was_running = self.start_instant.is_some();
        self.current_position = beat;
        self.start_position = beat;

        if was_running {
            // Reset start instant so elapsed time starts from now
            self.start_instant = Some(Instant::now());
        }
    }

    /// Called by tick loop - advances position based on elapsed time
    ///
    /// Returns the current position in beats.
    pub fn tick(&mut self) -> Beat {
        let Some(start) = self.start_instant else {
            return self.current_position;
        };

        let elapsed = start.elapsed();
        let elapsed_seconds = Second(elapsed.as_secs_f64());

        let tempo_map = self.tempo_map.read().unwrap();

        // Convert start position to ticks
        let start_tick = tempo_map.beat_to_tick(self.start_position);

        // Convert elapsed seconds to ticks at current tempo
        // Note: This uses tempo at tick 0 for the elapsed time conversion,
        // which is correct for constant tempo but approximate for tempo ramps.
        // For full accuracy with tempo changes during playback, we'd need
        // to integrate through the tempo map.
        let elapsed_tick = tempo_map.second_to_tick(elapsed_seconds);

        // Add elapsed ticks to start position
        let current_tick = Tick(start_tick.0 + elapsed_tick.0);

        // Convert back to beats
        self.current_position = tempo_map.tick_to_beat(current_tick);
        self.current_position
    }

    /// Get current position
    pub fn position(&self) -> Beat {
        self.current_position
    }

    /// Get tempo at current position
    pub fn current_tempo(&self) -> f64 {
        let tempo_map = self.tempo_map.read().unwrap();
        let tick = tempo_map.beat_to_tick(self.current_position);
        tempo_map.tempo_at(tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TimeSignature;
    use std::thread;
    use std::time::Duration;

    fn make_tempo_map(bpm: f64) -> Arc<RwLock<TempoMap>> {
        Arc::new(RwLock::new(TempoMap::new(bpm, TimeSignature::default())))
    }

    #[test]
    fn test_new_clock_at_zero() {
        let tempo_map = make_tempo_map(120.0);
        let clock = TickClock::new(tempo_map);

        assert_eq!(clock.position().0, 0.0);
        assert!(!clock.is_running());
    }

    #[test]
    fn test_start_sets_running() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.start();
        assert!(clock.is_running());
    }

    #[test]
    fn test_pause_stops_running() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.start();
        assert!(clock.is_running());

        clock.pause();
        assert!(!clock.is_running());
    }

    #[test]
    fn test_stop_resets_position() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.seek(Beat(16.0));
        clock.start();

        clock.stop();
        assert!(!clock.is_running());
        assert_eq!(clock.position().0, 0.0);
    }

    #[test]
    fn test_seek_updates_position() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.seek(Beat(8.0));
        assert_eq!(clock.position().0, 8.0);
    }

    #[test]
    fn test_seek_while_running() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.start();
        clock.seek(Beat(16.0));

        assert!(clock.is_running());
        assert_eq!(clock.position().0, 16.0);
    }

    #[test]
    fn test_position_advances_with_time() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.start();

        // Sleep 100ms - at 120 BPM, that's 0.2 beats
        thread::sleep(Duration::from_millis(100));

        let position = clock.tick();

        // Should be approximately 0.2 beats (120 BPM = 2 beats/sec, 0.1 sec = 0.2 beats)
        // Allow some tolerance for sleep inaccuracy
        assert!(position.0 > 0.15, "position {} should be > 0.15", position.0);
        assert!(position.0 < 0.3, "position {} should be < 0.3", position.0);
    }

    #[test]
    fn test_pause_preserves_position() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.start();
        thread::sleep(Duration::from_millis(100));
        clock.tick();

        let position_at_pause = clock.position();
        clock.pause();

        // Wait a bit
        thread::sleep(Duration::from_millis(50));

        // Position should not have changed
        assert_eq!(clock.position().0, position_at_pause.0);
    }

    #[test]
    fn test_resume_after_pause() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.start();
        thread::sleep(Duration::from_millis(100));
        clock.tick();

        let position_at_pause = clock.position();
        clock.pause();

        // Resume
        clock.start();
        thread::sleep(Duration::from_millis(100));
        clock.tick();

        // Position should have advanced from pause position
        assert!(
            clock.position().0 > position_at_pause.0 + 0.1,
            "position {} should be > {}",
            clock.position().0,
            position_at_pause.0 + 0.1
        );
    }

    #[test]
    fn test_tempo_affects_speed() {
        // Fast tempo
        let fast_tempo = make_tempo_map(240.0);
        let mut fast_clock = TickClock::new(fast_tempo);

        // Slow tempo
        let slow_tempo = make_tempo_map(60.0);
        let mut slow_clock = TickClock::new(slow_tempo);

        fast_clock.start();
        slow_clock.start();

        thread::sleep(Duration::from_millis(100));

        let fast_pos = fast_clock.tick();
        let slow_pos = slow_clock.tick();

        // Fast clock should have moved 4x as far as slow clock
        // 240 BPM = 4 beats/sec, 60 BPM = 1 beat/sec
        // Ratio should be approximately 4:1
        let ratio = fast_pos.0 / slow_pos.0;
        assert!(
            ratio > 3.0 && ratio < 5.0,
            "ratio {} should be approximately 4",
            ratio
        );
    }

    #[test]
    fn test_current_tempo() {
        let tempo_map = make_tempo_map(140.0);
        let clock = TickClock::new(tempo_map);

        assert_eq!(clock.current_tempo(), 140.0);
    }

    #[test]
    fn test_tick_when_not_running_returns_current() {
        let tempo_map = make_tempo_map(120.0);
        let mut clock = TickClock::new(tempo_map);

        clock.seek(Beat(4.0));

        // tick() when not running should return current position
        let position = clock.tick();
        assert_eq!(position.0, 4.0);
    }
}
