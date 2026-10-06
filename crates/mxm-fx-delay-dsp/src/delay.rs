//! Delay storage and the three finite time-change laws.

use crate::finite;

/// The longest public delay. Chosen for ordinary slow-tempo echoes while keeping the two 192 kHz
/// lines below 25 MiB including generation tags: eight bytes per slot over `ceil(rate * 8 s) + 4`
/// slots is 23.44 MiB for the pair.
pub const MAX_DELAY_S: f32 = 8.0;
/// The shortest public delay. Below this, interpolation and feedback filtering become a resonator
/// rather than the general-purpose echo this product promises.
pub const MIN_DELAY_S: f32 = 0.005;

/// One ring whose logical clear is constant-time. A generation per cell prevents stale samples from
/// becoming readable after reset or a topology change.
pub struct DelayLine {
    samples: Vec<f32>,
    generations: Vec<u32>,
    generation: u32,
    write: usize,
    valid: usize,
}

impl DelayLine {
    pub fn new(sample_rate: f32) -> Option<Self> {
        let slots = (sample_rate as f64 * MAX_DELAY_S as f64).ceil() as usize;
        let slots = slots.checked_add(4)?;
        let mut samples = Vec::new();
        samples.try_reserve_exact(slots).ok()?;
        samples.resize(slots, 0.0);
        let mut generations = Vec::new();
        generations.try_reserve_exact(slots).ok()?;
        generations.resize(slots, 0);
        Some(Self {
            samples,
            generations,
            generation: 1,
            write: 0,
            valid: 0,
        })
    }

    #[inline]
    pub fn write(&mut self, sample: f32) {
        let sample = finite(sample);
        self.samples[self.write] = sample;
        self.generations[self.write] = self.generation;
        self.write += 1;
        if self.write == self.samples.len() {
            self.write = 0;
        }
        self.valid = (self.valid + 1).min(self.samples.len());
    }

    /// Four-point Catmull-Rom interpolation over sample age. Integer ages are exact. The result is
    /// clamped to the four contributing samples, so feedback cannot amplify interpolation overshoot
    /// that was not in the stored signal.
    ///
    /// The cubic is chosen for **magnitude**, not phase: measured against linear interpolation of
    /// the same line, it retains more of the band at every frequency and fraction swept, and the
    /// margin grows with frequency (at 12 kHz/48 kHz it keeps 0.458 against linear's 0.395 of a
    /// 0.5 reference). Its fractional-delay accuracy is within 0.02 samples but is *not* better
    /// than linear's — an earlier version of this comment claimed a phase advantage that the
    /// measurement does not support. See
    /// `the_fractional_read_is_measured_against_linear_interpolation`.
    #[inline]
    pub fn read(&self, delay_samples: f64) -> f32 {
        if self.valid == 0 || !delay_samples.is_finite() {
            return 0.0;
        }
        let maximum = (self.samples.len() - 3) as f64;
        let delay = delay_samples.clamp(1.0, maximum);
        let whole = delay.floor() as usize;
        let fraction = (delay - whole as f64) as f32;
        let ym1 = self.at_age(whole.saturating_sub(1));
        let y0 = self.at_age(whole);
        let y1 = self.at_age(whole + 1);
        let y2 = self.at_age(whole + 2);
        if fraction == 0.0 {
            return y0;
        }
        let a = 0.5 * (2.0 * y0);
        let b = 0.5 * (-ym1 + y1);
        let c = 0.5 * (2.0 * ym1 - 5.0 * y0 + 4.0 * y1 - y2);
        let d = 0.5 * (-ym1 + 3.0 * y0 - 3.0 * y1 + y2);
        let cubic = ((d * fraction + c) * fraction + b) * fraction + a;
        let lo = ym1.min(y0).min(y1).min(y2);
        let hi = ym1.max(y0).max(y1).max(y2);
        finite(cubic.clamp(lo, hi))
    }

    #[inline]
    fn at_age(&self, age: usize) -> f32 {
        if age == 0 || age > self.valid || age >= self.samples.len() {
            return 0.0;
        }
        let index = (self.write + self.samples.len() - age) % self.samples.len();
        if self.generations[index] == self.generation {
            self.samples[index]
        } else {
            0.0
        }
    }

    pub fn clear(&mut self) {
        // `valid` is what actually makes a clear correct: it returns to zero here and only grows
        // one step per subsequent write, so any age `at_age` will read has been written since this
        // clear by construction. The generation tag is defence in depth on top of that, which is
        // why a wrapping `u32` is enough — a wrap can only ever re-admit a cell that `valid`
        // already covers. Keeping the clear O(1) matters more than the tag's width.
        self.generation = self.generation.wrapping_add(1).max(1);
        self.write = 0;
        self.valid = 0;
    }

    #[cfg(test)]
    pub fn backing_prefix(&self, count: usize) -> Vec<f32> {
        self.samples[..count.min(self.samples.len())].to_vec()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Repitch,
    Fade,
    Jump,
}

#[derive(Debug, Clone, Copy)]
pub struct ReadPlan {
    pub first: f64,
    pub second: f64,
    pub second_weight: f32,
    pub return_gain: f32,
}

impl ReadPlan {
    /// Mixes the two projections. This deliberately does **not** apply [`Self::return_gain`]: the
    /// Jump envelope has to sit after the loop's stateful colour, or a one-pole with populated
    /// state keeps ringing through the address change that the envelope exists to hide.
    pub fn blend(self, first: f32, second: f32) -> f32 {
        let b = self.second_weight.clamp(0.0, 1.0);
        (1.0 - b) * first + b * second
    }

    pub fn has_second(self) -> bool {
        self.second_weight > 0.0 && self.second_weight < 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Stable,
    Repitch,
    Fade,
    JumpDown,
    JumpUp,
}

/// A line's command-time state. Motion is deliberately not stored here: it is a continuous offset
/// around these command endpoints and therefore cannot recursively retarget a transition.
pub struct TimeState {
    current: f64,
    start: f64,
    target: f64,
    pending: Option<f64>,
    stage: Stage,
    position: u32,
    transition_samples: u32,
    jump_gain: f32,
}

impl TimeState {
    pub fn new(delay_samples: f64, sample_rate: f32) -> Self {
        Self {
            current: delay_samples,
            start: delay_samples,
            target: delay_samples,
            pending: None,
            stage: Stage::Stable,
            position: 0,
            transition_samples: (0.025 * sample_rate).round().max(2.0) as u32,
            jump_gain: 1.0,
        }
    }

    pub fn settle(&mut self, delay_samples: f64) {
        let delay = delay_samples.max(1.0);
        self.current = delay;
        self.start = delay;
        self.target = delay;
        self.pending = None;
        self.stage = Stage::Stable;
        self.position = 0;
        self.jump_gain = 1.0;
    }

    pub fn retarget(&mut self, delay_samples: f64, change: Change) {
        if !delay_samples.is_finite() {
            return;
        }
        let target = delay_samples.max(1.0);
        // Compare against the endpoint this line is actually promised to reach, which during a Fade
        // is the queued one. Comparing against `self.target` alone would treat "go back to the
        // endpoint already being faded to" as a no-op and leave an older queued endpoint to fire
        // after it, settling the line somewhere nobody asked for.
        let desired = self.pending.unwrap_or(self.target);
        if (target - desired).abs() < 1e-7 {
            return;
        }

        match self.stage {
            Stage::Fade => {
                // Complete the two-endpoint transition already heard; keep only the latest desired
                // endpoint. This bounds readers at two without collapsing a live blend into a fake
                // single history or adding an automation-dependent click.
                self.pending = Some(target);
            }
            Stage::JumpDown | Stage::JumpUp => {
                self.target = target;
                if self.stage == Stage::JumpUp {
                    self.start = self.current;
                    self.stage = Stage::JumpDown;
                    // Restart the down-ramp from the gain actually being emitted. `position` means
                    // "how far up" in JumpUp and "how far down" in JumpDown, so carrying it over
                    // unchanged turns a recovery near unity into a mute in one sample.
                    let half = (self.transition_samples / 2).max(1);
                    let emitted = (self.jump_gain.clamp(0.0, 1.0) * half as f32).round() as u32;
                    self.position = half.saturating_sub(emitted);
                }
            }
            Stage::Stable | Stage::Repitch => self.begin(target, change),
        }
    }

    fn begin(&mut self, target: f64, change: Change) {
        self.start = self.current;
        self.target = target;
        self.position = 0;
        self.stage = match change {
            Change::Repitch => Stage::Repitch,
            Change::Fade => Stage::Fade,
            Change::Jump => Stage::JumpDown,
        };
    }

    pub fn plan(&self) -> ReadPlan {
        let x = self.position as f32 / self.transition_samples as f32;
        match self.stage {
            Stage::Stable | Stage::Repitch => ReadPlan {
                first: self.current,
                second: self.current,
                second_weight: 0.0,
                return_gain: 1.0,
            },
            Stage::Fade => ReadPlan {
                first: self.start,
                second: self.target,
                second_weight: x.clamp(0.0, 1.0),
                return_gain: 1.0,
            },
            Stage::JumpDown => ReadPlan {
                first: self.current,
                second: self.current,
                second_weight: 0.0,
                return_gain: self.jump_gain,
            },
            Stage::JumpUp => ReadPlan {
                first: self.current,
                second: self.current,
                second_weight: 0.0,
                return_gain: self.jump_gain,
            },
        }
    }

    /// Advances one sample and reports whether a Fade promoted its second projection into the
    /// primary projection, so the caller can promote that projection's playback-filter state too.
    pub fn advance(&mut self, change: Change) -> bool {
        match self.stage {
            Stage::Stable => false,
            Stage::Repitch => {
                self.position += 1;
                let x = (self.position as f64 / self.transition_samples as f64).clamp(0.0, 1.0);
                // Raised cosine: finite endpoint and zero velocity at both ends.
                let shaped = 0.5 - 0.5 * (core::f64::consts::PI * x).cos();
                self.current = self.start + (self.target - self.start) * shaped;
                if self.position >= self.transition_samples {
                    self.current = self.target;
                    self.stage = Stage::Stable;
                    self.position = 0;
                }
                false
            }
            Stage::Fade => {
                self.position += 1;
                if self.position >= self.transition_samples {
                    self.current = self.target;
                    self.stage = Stage::Stable;
                    self.position = 0;
                    let pending = self.pending.take();
                    if let Some(next) = pending {
                        if (next - self.current).abs() > 1e-7 {
                            self.begin(next, change);
                        }
                    }
                    true
                } else {
                    false
                }
            }
            Stage::JumpDown => {
                self.position += 1;
                let half = (self.transition_samples / 2).max(1);
                self.jump_gain = 1.0 - (self.position as f32 / half as f32).clamp(0.0, 1.0);
                if self.position >= half {
                    self.current = self.target;
                    self.stage = Stage::JumpUp;
                    self.position = 0;
                    self.jump_gain = 0.0;
                }
                false
            }
            Stage::JumpUp => {
                self.position += 1;
                let half = (self.transition_samples / 2).max(1);
                self.jump_gain = (self.position as f32 / half as f32).clamp(0.0, 1.0);
                if self.position >= half {
                    self.jump_gain = 1.0;
                    self.stage = Stage::Stable;
                    self.position = 0;
                }
                false
            }
        }
    }

    #[cfg(test)]
    pub fn current(&self) -> f64 {
        self.current
    }

    #[cfg(test)]
    pub fn endpoint_count(&self) -> usize {
        usize::from(self.stage == Stage::Fade) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_reads_are_exact_and_fractional_reads_are_bounded() {
        let mut line = DelayLine::new(48_000.0).unwrap();
        for n in 1..=16 {
            line.write(n as f32);
        }
        assert_eq!(line.read(1.0), 16.0);
        assert_eq!(line.read(4.0), 13.0);
        let value = line.read(4.5);
        assert!((12.0..=13.0).contains(&value), "{value}");
    }

    #[test]
    fn clear_rewrites_no_backing_samples_and_stale_audio_never_returns() {
        let mut line = DelayLine::new(8_000.0).unwrap();
        for n in 0..512 {
            line.write(n as f32 + 1.0);
        }
        let before = line.backing_prefix(512);
        line.clear();
        assert_eq!(line.backing_prefix(512), before);
        for delay in [1.0, 10.5, 500.0] {
            assert_eq!(line.read(delay), 0.0);
        }
    }

    #[test]
    fn fade_never_grows_a_third_endpoint_under_retargeting() {
        let mut state = TimeState::new(100.0, 8_000.0);
        state.retarget(200.0, Change::Fade);
        for n in 0..10_000 {
            state.retarget(300.0 + (n % 17) as f64, Change::Fade);
            assert!(state.endpoint_count() <= 2);
            state.advance(Change::Fade);
        }
    }

    #[test]
    fn the_last_fade_command_wins_over_an_older_queued_one() {
        // A→B, queue C, then command B again. B equals the live endpoint, so a target-only
        // comparison treats it as a no-op and lets the stale C fire after the fade completes.
        let mut state = TimeState::new(100.0, 8_000.0);
        state.retarget(200.0, Change::Fade);
        state.retarget(300.0, Change::Fade);
        state.retarget(200.0, Change::Fade);
        for _ in 0..4000 {
            state.advance(Change::Fade);
        }
        assert_eq!(state.current(), 200.0);
        assert!(!state.plan().has_second());
    }

    #[test]
    fn retargeting_late_in_jump_recovery_does_not_step_the_gain() {
        let mut state = TimeState::new(100.0, 8_000.0);
        state.retarget(400.0, Change::Jump);
        // Run into JumpUp and most of the way back to unity.
        let half = (0.025 * 8_000.0f32).round().max(2.0) as u32 / 2;
        for _ in 0..(half + half - 1) {
            state.advance(Change::Jump);
        }
        let before = state.plan().return_gain;
        assert!(before > 0.8, "expected a late recovery gain, got {before}");
        state.retarget(700.0, Change::Jump);
        state.advance(Change::Jump);
        let after = state.plan().return_gain;
        let step = (after - before).abs();
        assert!(
            step <= 2.0 / half as f32,
            "gain stepped {step} from {before} to {after}"
        );
    }

    #[test]
    fn every_change_law_reaches_an_exact_endpoint() {
        for change in [Change::Repitch, Change::Fade, Change::Jump] {
            let mut state = TimeState::new(100.0, 8_000.0);
            state.retarget(700.0, change);
            for _ in 0..1000 {
                state.advance(change);
            }
            assert_eq!(state.current(), 700.0, "{change:?}");
            let plan = state.plan();
            assert_eq!(plan.return_gain, 1.0);
            assert!(!plan.has_second());
        }
    }
}
