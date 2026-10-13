//! Lock-free lossy audio-to-editor telemetry. This effect has no developer MIDI channel.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct Telemetry {
    /// The input level: one `mxm_telemetry::PeakMeter`, the collection's one meter policy.
    pub input: mxm_telemetry::PeakMeter,
    /// The wet level: one `mxm_telemetry::PeakMeter`, the collection's one meter policy.
    pub wet: mxm_telemetry::PeakMeter,
    /// The output level and the clip light, which judges the output: one
    /// `mxm_telemetry::PeakMeter`, the collection's one meter policy.
    pub output: mxm_telemetry::PeakMeter,
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
            input: mxm_telemetry::PeakMeter::new(),
            wet: mxm_telemetry::PeakMeter::new(),
            output: mxm_telemetry::PeakMeter::new(),
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
        self.input.publish(input);
        self.wet.publish(wet);
        self.output.publish(output);
        self.tempo.publish(tempo);
        self.left_time.store(times[0].to_bits(), Ordering::Relaxed);
        self.right_time.store(times[1].to_bits(), Ordering::Relaxed);
        self.activity.store(activity.to_bits(), Ordering::Relaxed);
        self.held.store(held, Ordering::Relaxed);
    }

    pub fn take_levels(&self) -> [f32; 3] {
        [self.input.take(), self.wet.take(), self.output.take()]
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
        assert!(telemetry.output.clipped());
        telemetry.output.clear_clip();
        assert!(!telemetry.output.clipped());
        assert_eq!(telemetry.tempo(), Some(90.0));
        assert_eq!(telemetry.times(), [0.4, 0.5]);
        assert!(!telemetry.held());
    }
}
