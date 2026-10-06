//! Lock-free lossy audio-to-editor telemetry. This effect has no developer MIDI channel.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct Telemetry {
    input: AtomicU32,
    wet: AtomicU32,
    output: AtomicU32,
    clipped: AtomicBool,
    tempo: mxm_tempo::TempoCell,
    left_time: AtomicU32,
    right_time: AtomicU32,
    activity: AtomicU32,
    held: AtomicBool,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            input: AtomicU32::new(0),
            wet: AtomicU32::new(0),
            output: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
            tempo: mxm_tempo::TempoCell::new(),
            left_time: AtomicU32::new(0),
            right_time: AtomicU32::new(0),
            activity: AtomicU32::new(0),
            held: AtomicBool::new(false),
        }
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    fn publish_max(slot: &AtomicU32, value: f32) {
        let value = if value.is_finite() { value.abs() } else { 0.0 };
        let mut current = slot.load(Ordering::Relaxed);
        while f32::from_bits(current) < value {
            match slot.compare_exchange_weak(
                current,
                value.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish(
        &self,
        input: f32,
        wet: f32,
        output: f32,
        tempo: Option<f64>,
        times: [f32; 2],
        activity: f32,
        held: bool,
    ) {
        Self::publish_max(&self.input, input);
        Self::publish_max(&self.wet, wet);
        Self::publish_max(&self.output, output);
        if output >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
        self.tempo.publish(tempo);
        self.left_time.store(times[0].to_bits(), Ordering::Relaxed);
        self.right_time.store(times[1].to_bits(), Ordering::Relaxed);
        self.activity.store(activity.to_bits(), Ordering::Relaxed);
        self.held.store(held, Ordering::Relaxed);
    }

    pub fn take_levels(&self) -> [f32; 3] {
        [
            f32::from_bits(self.input.swap(0, Ordering::Relaxed)),
            f32::from_bits(self.wet.swap(0, Ordering::Relaxed)),
            f32::from_bits(self.output.swap(0, Ordering::Relaxed)),
        ]
    }

    /// No tempo until the next callback reports one: what activation leaves.
    pub fn forget_tempo(&self) {
        self.tempo.publish(None);
    }

    pub fn tempo(&self) -> Option<f64> {
        self.tempo.get()
    }

    pub fn times(&self) -> [f32; 2] {
        [
            f32::from_bits(self.left_time.load(Ordering::Relaxed)),
            f32::from_bits(self.right_time.load(Ordering::Relaxed)),
        ]
    }

    pub fn activity(&self) -> f32 {
        f32::from_bits(self.activity.load(Ordering::Relaxed))
    }

    pub fn held(&self) -> bool {
        self.held.load(Ordering::Relaxed)
    }

    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_are_max_combined_and_clip_latches() {
        let telemetry = Telemetry::new();
        telemetry.publish(0.2, 0.3, 1.1, Some(120.0), [0.2, 0.3], 0.4, true);
        telemetry.publish(0.7, 0.1, 0.4, Some(90.0), [0.4, 0.5], 0.2, false);
        assert_eq!(telemetry.take_levels(), [0.7, 0.3, 1.1]);
        assert_eq!(telemetry.take_levels(), [0.0; 3]);
        assert!(telemetry.clipped());
        telemetry.clear_clip();
        assert!(!telemetry.clipped());
        assert_eq!(telemetry.tempo(), Some(90.0));
        assert_eq!(telemetry.times(), [0.4, 0.5]);
        assert!(!telemetry.held());
    }
}
