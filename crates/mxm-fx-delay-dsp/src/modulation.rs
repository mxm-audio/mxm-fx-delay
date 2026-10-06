//! Deterministic modulation sources. Rate changes preserve phase; Shape changes crossfade finite
//! command signals rather than resetting the delay read.

use crate::{Routing, Shape, finite};

const SHAPE_FADE_S: f32 = 0.015;

#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        let unit = (x >> 40) as f32 / ((1u32 << 24) - 1) as f32;
        unit * 2.0 - 1.0
    }
}

#[derive(Clone, Copy)]
struct RandomSegment {
    from: f32,
    to: f32,
    rng: Rng,
}

impl RandomSegment {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let from = rng.next();
        let to = rng.next();
        Self { from, to, rng }
    }

    fn advance(&mut self) {
        self.from = self.to;
        self.to = self.rng.next();
    }

    fn at(&self, phase: f64) -> f32 {
        let x = phase as f32;
        let smooth = x * x * (3.0 - 2.0 * x);
        self.from + (self.to - self.from) * smooth
    }
}

pub struct Modulator {
    phase: f64,
    random: [RandomSegment; 2],
    current_shape: Shape,
    fade_from: [f32; 2],
    shape_position: u32,
    shape_samples: u32,
}

impl Modulator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            random: [
                RandomSegment::new(0x4d58_4d44_454c_4159),
                RandomSegment::new(0x5354_4552_454f_524e),
            ],
            current_shape: Shape::Sine,
            fade_from: [0.0; 2],
            shape_position: u32::MAX,
            shape_samples: (SHAPE_FADE_S * sample_rate).round().max(1.0) as u32,
        }
    }

    pub fn settle_shape(&mut self, shape: Shape) {
        self.current_shape = shape;
        self.fade_from = [0.0; 2];
        self.shape_position = u32::MAX;
    }

    /// Retargets one finite command transition. A new request starts at the command actually being
    /// emitted, not at either named waveform, so rapid Shape automation cannot create a jump.
    pub fn set_shape(&mut self, shape: Shape, routing: Routing) {
        if shape != self.current_shape {
            let blend = self.shape_blend();
            for channel in 0..2 {
                let target = self.value(self.current_shape, channel, routing);
                self.fade_from[channel] = if self.shape_position == u32::MAX {
                    target
                } else {
                    self.fade_from[channel] + (target - self.fade_from[channel]) * blend
                };
            }
            self.current_shape = shape;
            self.shape_position = 0;
        }
    }

    pub fn next(&mut self, rate_hz: f32, routing: Routing, sample_rate: f32) -> [f32; 2] {
        let rate = finite(rate_hz).clamp(0.01, 20.0) as f64;
        let previous_phase = self.phase;
        self.phase += rate / sample_rate as f64;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            for random in &mut self.random {
                random.advance();
            }
        }
        // A full-rate edit changes only this increment. Accumulated phase is continuous.
        debug_assert!(self.phase.is_finite() && previous_phase.is_finite());

        let mut output = [0.0; 2];
        let blend = self.shape_blend();
        for (channel, value) in output.iter_mut().enumerate() {
            let target = self.value(self.current_shape, channel, routing);
            *value = if self.shape_position == u32::MAX {
                target
            } else {
                finite(self.fade_from[channel] + (target - self.fade_from[channel]) * blend)
            };
        }

        if self.shape_position != u32::MAX {
            self.shape_position += 1;
            if self.shape_position >= self.shape_samples {
                self.shape_position = u32::MAX;
            }
        }
        output
    }

    fn shape_blend(&self) -> f32 {
        if self.shape_position == u32::MAX {
            1.0
        } else {
            (self.shape_position as f32 / self.shape_samples as f32).clamp(0.0, 1.0)
        }
    }

    fn value(&self, shape: Shape, channel: usize, routing: Routing) -> f32 {
        let linked = routing != Routing::Dual;
        match shape {
            Shape::Sine => {
                let base = (self.phase * core::f64::consts::TAU).sin() as f32;
                if !linked && channel == 1 { -base } else { base }
            }
            Shape::Triangle => {
                let base = (4.0 * (self.phase as f32 - 0.5).abs() - 1.0).clamp(-1.0, 1.0);
                if !linked && channel == 1 { -base } else { base }
            }
            Shape::Random => {
                let source = if linked { 0 } else { channel };
                self.random[source].at(self.phase)
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self {
            phase: 0.0,
            random: [
                RandomSegment::new(0x4d58_4d44_454c_4159),
                RandomSegment::new(0x5354_4552_454f_524e),
            ],
            current_shape: self.current_shape,
            fade_from: [0.0; 2],
            shape_position: u32::MAX,
            shape_samples: self.shape_samples,
        };
    }

    #[cfg(test)]
    pub fn phase(&self) -> f64 {
        self.phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_changes_do_not_reset_phase() {
        let mut modulation = Modulator::new(48_000.0);
        let _ = modulation.next(0.1, Routing::Standard, 48_000.0);
        let before = modulation.phase();
        let _ = modulation.next(20.0, Routing::Standard, 48_000.0);
        assert!(modulation.phase() > before);
        assert!((modulation.phase() - before) < 0.001);
    }

    #[test]
    fn standard_and_ping_pong_are_linked_while_dual_is_mirrored() {
        let mut linked = Modulator::new(48_000.0);
        let standard = linked.next(1.0, Routing::Standard, 48_000.0);
        assert_eq!(standard[0], standard[1]);
        let ping = linked.next(1.0, Routing::PingPong, 48_000.0);
        assert_eq!(ping[0], ping[1]);

        let mut dual = Modulator::new(48_000.0);
        let value = dual.next(1.0, Routing::Dual, 48_000.0);
        assert_eq!(value[0], -value[1]);
    }

    #[test]
    fn reset_reproduces_random_motion() {
        let mut modulation = Modulator::new(48_000.0);
        modulation.settle_shape(Shape::Random);
        let a: Vec<_> = (0..1000)
            .map(|_| modulation.next(8.0, Routing::Dual, 48_000.0))
            .collect();
        modulation.reset();
        let b: Vec<_> = (0..1000)
            .map(|_| modulation.next(8.0, Routing::Dual, 48_000.0))
            .collect();
        assert_eq!(a, b);
    }

    #[test]
    fn rapid_shape_retargeting_starts_from_the_emitted_command() {
        let mut modulation = Modulator::new(48_000.0);
        let mut previous = modulation.next(3.0, Routing::Dual, 48_000.0);
        let mut maximum_step = 0.0f32;
        for sample in 0..10_000 {
            if sample % 37 == 0 {
                let shape = match (sample / 37) % 3 {
                    0 => Shape::Sine,
                    1 => Shape::Triangle,
                    _ => Shape::Random,
                };
                modulation.set_shape(shape, Routing::Dual);
            }
            let value = modulation.next(3.0, Routing::Dual, 48_000.0);
            maximum_step = maximum_step
                .max((value[0] - previous[0]).abs())
                .max((value[1] - previous[1]).abs());
            previous = value;
        }
        assert!(maximum_step < 0.03, "command jumped by {maximum_step}");
    }
}
