//! Small recursive filters used by the loop, model playback and detector.

use crate::finite;

#[derive(Clone, Copy, Default)]
pub struct OnePole {
    state: f64,
}

impl OnePole {
    #[inline]
    pub fn low_pass(&mut self, input: f32, cutoff_hz: f32, sample_rate: f32) -> f32 {
        let cutoff = finite(cutoff_hz).clamp(1.0, sample_rate * 0.45) as f64;
        let coefficient = 1.0 - (-2.0 * core::f64::consts::PI * cutoff / sample_rate as f64).exp();
        // Flush the state itself, not a copy of it on the way out. Returning `flush(self.state)`
        // while leaving the subnormal in `self.state` keeps the recursion doing subnormal
        // arithmetic for as long as the filter runs, which is exactly what this guard exists to
        // prevent — and a held Freeze runs it indefinitely.
        self.state = flush(self.state + coefficient * (finite(input) as f64 - self.state));
        self.state as f32
    }

    pub fn reset(&mut self) {
        self.state = 0.0;
    }
}

#[derive(Clone, Copy, Default)]
pub struct LoopFilter {
    low: OnePole,
    hp_low: OnePole,
}

impl LoopFilter {
    #[inline]
    pub fn process(
        &mut self,
        input: f32,
        low_cut_hz: f32,
        high_cut_hz: f32,
        sample_rate: f32,
    ) -> f32 {
        let high = self.low.low_pass(input, high_cut_hz, sample_rate);
        let removed = self.hp_low.low_pass(high, low_cut_hz, sample_rate);
        finite(high - removed)
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.hp_low.reset();
    }
}

#[inline]
fn flush(value: f64) -> f64 {
    if value.is_finite() && value.abs() >= f64::MIN_POSITIVE {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_input_does_not_poison_recursive_state() {
        let mut filter = LoopFilter::default();
        let _ = filter.process(f32::NAN, 20.0, 10_000.0, 48_000.0);
        for _ in 0..1000 {
            assert!(filter.process(0.2, 20.0, 10_000.0, 48_000.0).is_finite());
        }
    }
}
