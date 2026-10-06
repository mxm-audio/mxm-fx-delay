//! Framework-free DSP for `mxm-fx-delay`.
//!
//! The clean line uses a bounded cubic fractional read. The early-digital model couples delay,
//! effective conversion rate and bandwidth. The tape model treats delay as head distance divided by
//! transport speed and couples that speed to playback loss and deterministic transport motion. The
//! equations follow the techniques named in `research:effects/delay-effects.md`; no third-party
//! implementation was consulted or ported.

mod delay;
mod filters;
mod modulation;

pub use delay::{Change, MAX_DELAY_S, MIN_DELAY_S};

use delay::{DelayLine, TimeState};
use filters::{LoopFilter, OnePole};
use modulation::Modulator;

pub const MIN_SAMPLE_RATE: f32 = 1_000.0;
pub const MAX_SAMPLE_RATE: f32 = 768_000.0;
const QUIET_LEVEL: f32 = 1.0e-7;
const QUIET_HOLD_S: f32 = 0.5;
const FREEZE_FADE_S: f32 = 0.025;
const TOPOLOGY_FADE_S: f32 = 0.015;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Routing {
    Standard,
    Dual,
    PingPong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    Clean,
    VintageDigital,
    Tape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Sine,
    Triangle,
    Random,
}

/// Plain per-sample controls. Every public seam sanitises these values again; a plugin wrapper is
/// not part of this crate's numeric safety argument.
#[derive(Debug, Clone, Copy)]
pub struct Controls {
    pub time_s: f32,
    pub change: Change,
    pub routing: Routing,
    pub offset: f32,
    pub feedback: f32,
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub drive: f32,
    pub freeze: bool,
    pub model: Model,
    pub character: f32,
    pub motion: f32,
    pub rate_hz: f32,
    pub shape: Shape,
    pub duck: f32,
    pub mix: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            time_s: 0.35,
            change: Change::Fade,
            routing: Routing::Standard,
            offset: 0.0,
            feedback: 0.42,
            low_cut_hz: 40.0,
            high_cut_hz: 12_000.0,
            drive: 0.12,
            freeze: false,
            model: Model::Clean,
            character: 0.35,
            motion: 0.0,
            rate_hz: 0.25,
            shape: Shape::Sine,
            duck: 0.12,
            mix: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildError {
    InvalidSampleRate,
    CapacityOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FreezeState {
    Off,
    Armed,
    Capturing(u32),
    Held,
    Releasing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopologyStage {
    Stable,
    Down,
    Up,
}

struct Topology {
    active_model: Model,
    active_routing: Routing,
    desired_model: Model,
    desired_routing: Routing,
    stage: TopologyStage,
    gain: f32,
    step: f32,
}

impl Topology {
    fn new(sample_rate: f32) -> Self {
        Self {
            active_model: Model::Clean,
            active_routing: Routing::Standard,
            desired_model: Model::Clean,
            desired_routing: Routing::Standard,
            stage: TopologyStage::Stable,
            gain: 1.0,
            step: 1.0 / (TOPOLOGY_FADE_S * sample_rate).round().max(1.0),
        }
    }

    fn settle(&mut self, model: Model, routing: Routing) {
        self.active_model = model;
        self.active_routing = routing;
        self.desired_model = model;
        self.desired_routing = routing;
        self.stage = TopologyStage::Stable;
        self.gain = 1.0;
    }

    /// Replaces the model with no crossfade, for the one case where it cannot be heard: a fully
    /// held Freeze field, whose reads and recirculating write both bypass the model stages.
    fn settle_model(&mut self, model: Model) {
        self.active_model = model;
        self.desired_model = model;
    }

    fn request(&mut self, model: Model, routing: Routing) {
        self.desired_model = model;
        self.desired_routing = routing;
        if (model, routing) != (self.active_model, self.active_routing) {
            match self.stage {
                TopologyStage::Stable | TopologyStage::Up => self.stage = TopologyStage::Down,
                TopologyStage::Down => {}
            }
        }
    }

    /// Returns true exactly on the sample after wet reached zero and topology may be replaced.
    fn advance(&mut self) -> bool {
        match self.stage {
            TopologyStage::Stable => false,
            TopologyStage::Down => {
                self.gain = (self.gain - self.step).max(0.0);
                if self.gain == 0.0 {
                    self.active_model = self.desired_model;
                    self.active_routing = self.desired_routing;
                    self.stage = TopologyStage::Up;
                    true
                } else {
                    false
                }
            }
            TopologyStage::Up => {
                if (self.desired_model, self.desired_routing)
                    != (self.active_model, self.active_routing)
                {
                    self.stage = TopologyStage::Down;
                    return false;
                }
                self.gain = (self.gain + self.step).min(1.0);
                if self.gain == 1.0 {
                    self.stage = TopologyStage::Stable;
                }
                false
            }
        }
    }
}

/// The complete stereo processor. Construction is the only allocating operation.
pub struct Engine {
    sample_rate: f32,
    lines: [DelayLine; 2],
    times: [TimeState; 2],
    playback: [[OnePole; 2]; 2],
    second_projection_active: [bool; 2],
    loop_filters: [LoopFilter; 2],
    modulation: Modulator,
    topology: Topology,
    freeze_state: FreezeState,
    freeze_mix: f32,
    freeze_step: f32,
    detector: f32,
    activity: f32,
    quiet_samples: u64,
    history_active: bool,
    parked: bool,
    wow_phase: f64,
    wow_rng: [u64; 2],
    wow_local: [f32; 2],
    wow_target: [f32; 2],
    wow_until_target: u32,
    last_effective_times: [f32; 2],
}

impl Engine {
    pub fn new(sample_rate: f32) -> Result<Self, BuildError> {
        if !sample_rate.is_finite() || !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
            return Err(BuildError::InvalidSampleRate);
        }
        let line_0 = DelayLine::new(sample_rate).ok_or(BuildError::CapacityOverflow)?;
        let line_1 = DelayLine::new(sample_rate).ok_or(BuildError::CapacityOverflow)?;
        let initial = Controls::default().time_s as f64 * sample_rate as f64;
        Ok(Self {
            sample_rate,
            lines: [line_0, line_1],
            times: [
                TimeState::new(initial, sample_rate),
                TimeState::new(initial, sample_rate),
            ],
            playback: [[OnePole::default(); 2]; 2],
            second_projection_active: [false; 2],
            loop_filters: [LoopFilter::default(); 2],
            modulation: Modulator::new(sample_rate),
            topology: Topology::new(sample_rate),
            freeze_state: FreezeState::Off,
            freeze_mix: 0.0,
            freeze_step: 1.0 / (FREEZE_FADE_S * sample_rate).round().max(1.0),
            detector: 0.0,
            activity: 0.0,
            quiet_samples: 0,
            history_active: false,
            parked: true,
            wow_phase: 0.0,
            wow_rng: [0x5441_5045_4c45_4654, 0x5441_5045_5249_4748],
            wow_local: [0.0; 2],
            wow_target: [0.0; 2],
            wow_until_target: 1,
            last_effective_times: [Controls::default().time_s; 2],
        })
    }

    /// Settles discrete controls and clears every history. Use after activation/state restoration;
    /// live edits go through [`process`](Self::process)'s finite transitions.
    pub fn reset(&mut self, controls: Controls) {
        self.topology.settle(controls.model, controls.routing);
        self.modulation.settle_shape(controls.shape);
        self.clear_audio(&controls);
        self.parked = true;
    }

    /// Processes one stereo sample. `mono_input` means the left sample is the sole host input; the
    /// right argument is ignored for injection and dry is copied to both outputs.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], mono_input: bool, controls: Controls) -> [f32; 2] {
        let input_l = finite(input[0]);
        let input_r = if mono_input {
            input_l
        } else {
            finite(input[1])
        };
        let dry = [input_l, input_r];
        let mix = unit(controls.mix);

        // A parked engine has no audio to protect, so discrete edits settle instead of crossfading.
        // Requesting a topology fade here would leave the fade pending across the whole silence and
        // then run it over the first input the engine records — clearing that input mid-fade.
        // A fully held field cannot hear Model. Held reads bypass playback colour and the
        // recirculating write bypasses the record stage, so the only thing a Model crossfade would
        // do here is clear the line and destroy the captured sound — a control that is inaudible
        // while frozen would delete the frozen sound when touched. Routing is not the same case: a
        // lap means something different under Ping-pong, and the held return is still routed, so a
        // Routing change keeps the ordinary crossfade and the clear that comes with it.
        // A running engine's topology decision is taken further down, once this sample's Freeze
        // state is known. Reading last sample's would misjudge both edges of the hold.
        if self.parked {
            self.topology.settle(controls.model, controls.routing);
            self.modulation.settle_shape(controls.shape);
        } else {
            self.modulation
                .set_shape(controls.shape, self.topology.active_routing);
        }

        if mix == 0.0 {
            if !self.parked || self.history_active {
                self.clear_audio(&controls);
            }
            self.parked = true;
            return dry;
        }

        let input_peak = input_l.abs().max(input_r.abs());
        if self.parked {
            if input_peak == 0.0 {
                // Nothing is stored and nothing is arriving. An engaged Freeze is armed here and
                // stays parked until real input turns up: running the full path on silence would
                // burn a core forever for a capture that cannot happen yet.
                self.freeze_state = if controls.freeze {
                    FreezeState::Armed
                } else {
                    FreezeState::Off
                };
                return dry;
            }
            self.parked = false;
            self.settle_times(&controls);
        }

        let routing = self.topology.active_routing;
        let base_times = target_times(controls.time_s, controls.offset, routing);
        for (line, seconds) in self.times.iter_mut().zip(base_times) {
            line.retarget(seconds as f64 * self.sample_rate as f64, controls.change);
        }

        let command = self
            .modulation
            .next(controls.rate_hz, routing, self.sample_rate);
        self.update_freeze(controls.freeze, input_peak, base_times, routing);
        let freeze_target = f32::from(self.freeze_state == FreezeState::Held);
        if self.freeze_mix < freeze_target {
            self.freeze_mix = (self.freeze_mix + self.freeze_step).min(freeze_target);
        } else if self.freeze_mix > freeze_target {
            self.freeze_mix = (self.freeze_mix - self.freeze_step).max(freeze_target);
            if self.freeze_mix == 0.0 && self.freeze_state == FreezeState::Releasing {
                self.freeze_state = FreezeState::Off;
            }
        }

        // The topology decision belongs here, after this sample's Freeze state is settled, because
        // both edges of the hold are one-sample races otherwise. Taken at the top of the callback it
        // reads the *previous* sample: a Model change on the sample the hold fade completes would
        // start a crossfade that is no longer owed, and `topology.gain` would then duck the held
        // output and, worse, attenuate what the field recirculates into itself.
        //
        // While the field is fully held, Model is inaudible — held reads bypass playback colour and
        // the recirculating write bypasses the record stage — so it settles with no crossfade and no
        // clear. A control that cannot be heard while frozen must not disturb the frozen sound.
        // Routing is not the same case: a lap means something different under Ping-pong and the held
        // return is still routed, so a Routing change keeps the ordinary crossfade.
        let field_is_held =
            controls.freeze && self.freeze_state == FreezeState::Held && self.freeze_mix == 1.0;
        if field_is_held
            && self.topology.stage == TopologyStage::Stable
            && controls.routing == self.topology.active_routing
        {
            self.topology.settle_model(controls.model);
        } else {
            self.topology.request(controls.model, controls.routing);
        }
        let model = self.topology.active_model;
        let inherent = self.tape_motion(unit(controls.character), model);
        let motion_depth = unit(controls.motion) * if model == Model::Tape { 0.06 } else { 0.03 };

        let mut ordinary = [0.0; 2];
        let mut held = [0.0; 2];
        let mut plans = [self.times[0].plan(), self.times[1].plan()];
        for channel in 0..2 {
            // Freeze captures a recurrence, not a transport that keeps wandering underneath it.
            // Explicit Time changes still reach `TimeState`; only ongoing modulation settles out.
            let modulation =
                (command[channel] * motion_depth + inherent[channel]) * (1.0 - self.freeze_mix);
            plans[channel].first =
                modulated_delay(plans[channel].first, modulation, self.sample_rate);
            plans[channel].second =
                modulated_delay(plans[channel].second, modulation, self.sample_rate);
            self.last_effective_times[channel] =
                (plans[channel].first / self.sample_rate as f64) as f32;

            if plans[channel].has_second() && !self.second_projection_active[channel] {
                self.playback[channel][1] = self.playback[channel][0];
                self.second_projection_active[channel] = true;
            }

            let raw_first = self.lines[channel].read(plans[channel].first);
            let raw_second = self.lines[channel].read(plans[channel].second);
            let first = playback_model(
                model,
                raw_first,
                plans[channel].first,
                unit(controls.character),
                self.sample_rate,
                &mut self.playback[channel][0],
            );
            let second = if plans[channel].has_second() {
                playback_model(
                    model,
                    raw_second,
                    plans[channel].second,
                    unit(controls.character),
                    self.sample_rate,
                    &mut self.playback[channel][1],
                )
            } else {
                first
            };
            let playback = plans[channel].blend(first, second);
            ordinary[channel] = self.loop_filters[channel].process(
                playback,
                finite(controls.low_cut_hz).clamp(10.0, 8_000.0),
                band_limited(controls.high_cut_hz, 200.0, self.sample_rate),
                self.sample_rate,
            );
            ordinary[channel] = loop_saturator(ordinary[channel], unit(controls.drive));
            // The Jump envelope lands here, once, downstream of every stateful stage, so the
            // address change really is heard against silence. It is applied to `ordinary` itself so
            // the feedback return below inherits exactly the same envelope, not its square.
            ordinary[channel] *= plans[channel].return_gain;

            // Held reads are integer and bypass every ageing stage. A Time edit still selects a new
            // place in the captured field, but an untouched field neither loses interpolation energy
            // nor traverses colour again. A Fade still crossfades its two endpoints while held:
            // reading only `first` would hold the old address for the whole transition and then
            // jump to the new one at promotion, which is the click Fade exists to avoid.
            let held_first = self.lines[channel].read(plans[channel].first.round());
            let held_second = if plans[channel].has_second() {
                self.lines[channel].read(plans[channel].second.round())
            } else {
                held_first
            };
            held[channel] =
                plans[channel].blend(held_first, held_second) * plans[channel].return_gain;
        }

        let mut wet = [0.0; 2];
        for channel in 0..2 {
            wet[channel] =
                lerp(ordinary[channel], held[channel], self.freeze_mix) * self.topology.gain;
        }

        let feedback = feedback_gain(controls.feedback);
        let ordinary_return = feedback_matrix(ordinary, routing, feedback);
        let held_return = feedback_matrix(held, routing, 1.0);
        let mut return_signal = [0.0; 2];
        for channel in 0..2 {
            // `ordinary` and `held` already carry the Jump envelope, so it is not applied again
            // here: the heard wet and the feedback return must share one envelope, not gain².
            return_signal[channel] = lerp(
                ordinary_return[channel],
                held_return[channel],
                self.freeze_mix,
            ) * self.topology.gain;
        }

        let injection_open = match self.freeze_state {
            FreezeState::Held => 1.0 - self.freeze_mix,
            FreezeState::Releasing => 1.0 - self.freeze_mix,
            _ => 1.0,
        };
        let injection = injection_for(routing, mono_input, dry, injection_open);

        for channel in 0..2 {
            let sum = finite(injection[channel] + return_signal[channel]);
            let recorded = if self.freeze_mix == 1.0 {
                sum
            } else {
                let coloured = record_model(model, sum, unit(controls.character));
                lerp(coloured, sum, self.freeze_mix)
            };
            self.lines[channel].write(recorded);
        }

        let duck_gain = self.duck_gain(input_peak, unit(controls.duck));
        let mut output = [0.0; 2];
        for channel in 0..2 {
            output[channel] = finite((1.0 - mix) * dry[channel] + mix * duck_gain * wet[channel]);
        }

        let promoted = [
            self.times[0].advance(controls.change),
            self.times[1].advance(controls.change),
        ];
        for channel in 0..2 {
            if promoted[channel] {
                self.playback[channel][0] = self.playback[channel][1];
                self.playback[channel][1].reset();
                self.second_projection_active[channel] = false;
            } else if !plans[channel].has_second() {
                self.second_projection_active[channel] = false;
            }
        }

        let routing_before = self.topology.active_routing;
        let replaced_topology = self.topology.advance();
        // Once Freeze is engaged the user has committed to keeping this material, so a Model-only
        // replacement does not clear it — during a capture or a hold alike. The crossfade still
        // runs, because Model *is* audible while the capture is ramping and owes one; only the
        // clear is skipped. Deciding this here rather than when the edit arrived is what makes the
        // outcome independent of where in the capture the edit landed, instead of destroying the
        // field for an edit that was a few milliseconds early. Routing still clears: a lap means
        // something different under another routing.
        let field_survives_replacement = replaced_topology
            && self.topology.active_routing == routing_before
            && controls.freeze
            && matches!(
                self.freeze_state,
                FreezeState::Capturing(_) | FreezeState::Held
            );
        if replaced_topology && !field_survives_replacement {
            self.clear_audio(&controls);
            // The clear above erased the sample written into the outgoing topology's line earlier
            // in this call, so re-record the current input into the incoming one. Dropping it would
            // punch a one-sample hole into every Model or Routing change made while audio is
            // playing, and counting it as activity while it is gone would leave an empty engine
            // claiming history it does not hold. The old topology's feedback return is not carried
            // over: it belonged to the line that no longer exists.
            let model = self.topology.active_model;
            let character = unit(controls.character);
            let reinjected = injection_for(self.topology.active_routing, mono_input, dry, 1.0);
            for (line, sample) in self.lines.iter_mut().zip(reinjected) {
                line.write(record_model(model, sample, character));
            }
            // The new topology holds exactly what was just re-recorded and nothing else, so there
            // is no hidden journey left to wait out. Without input it parks immediately rather than
            // running the full horizon over silence or, at unity feedback, reporting an empty
            // engine as an endless tail.
            self.parked = input_peak == 0.0;
        }
        // Whatever the outgoing topology was still emitting when its crossfade reached zero was
        // cleared above, so it is no longer history and must not resurrect the activity that keeps
        // an empty engine awake. The re-recorded input is accounted for by `input_peak`. A field
        // that survived the replacement was not cleared, so its wet is still real history.
        let activity_wet = if replaced_topology && !field_survives_replacement {
            [0.0; 2]
        } else {
            wet
        };
        self.update_activity(input_peak, activity_wet, &controls);
        output
    }

    pub fn is_parked(&self) -> bool {
        self.parked
    }

    pub fn is_sustaining(&self, controls: Controls) -> bool {
        (self.freeze_state == FreezeState::Held && self.history_active)
            || (feedback_gain(controls.feedback) >= 1.0 && self.history_active)
    }

    pub fn remaining_tail_seconds(&self, controls: Controls) -> Option<f32> {
        if self.is_sustaining(controls) {
            return None;
        }
        if self.parked {
            return Some(0.0);
        }
        let delay = self.last_effective_times[0].max(self.last_effective_times[1]);
        let gain = feedback_gain(controls.feedback).clamp(0.0001, 0.999);
        let laps = (QUIET_LEVEL.ln() / gain.ln()).max(1.0);
        // Once the current tap is quiet, a later Time edit may still expose older valid samples.
        // Keep the host awake through the same maximum hidden-history horizon used by parking.
        Some((delay * (laps + 1.0) + MAX_DELAY_S + QUIET_HOLD_S).min(120.0))
    }

    pub fn effective_times(&self) -> [f32; 2] {
        self.last_effective_times
    }

    pub fn activity(&self) -> f32 {
        self.activity
    }

    pub fn freeze_is_held(&self) -> bool {
        self.freeze_state == FreezeState::Held
    }

    fn settle_times(&mut self, controls: &Controls) {
        let targets = target_times(
            controls.time_s,
            controls.offset,
            self.topology.active_routing,
        );
        for (state, seconds) in self.times.iter_mut().zip(targets) {
            state.settle(seconds as f64 * self.sample_rate as f64);
        }
        self.last_effective_times = targets;
    }

    fn clear_audio(&mut self, controls: &Controls) {
        for line in &mut self.lines {
            line.clear();
        }
        for pair in &mut self.playback {
            pair[0].reset();
            pair[1].reset();
        }
        for filter in &mut self.loop_filters {
            filter.reset();
        }
        self.second_projection_active = [false; 2];
        self.modulation.reset();
        self.modulation.settle_shape(controls.shape);
        self.freeze_state = if controls.freeze {
            FreezeState::Armed
        } else {
            FreezeState::Off
        };
        self.freeze_mix = 0.0;
        self.detector = 0.0;
        self.activity = 0.0;
        self.quiet_samples = 0;
        self.history_active = false;
        self.wow_phase = 0.0;
        self.wow_rng = [0x5441_5045_4c45_4654, 0x5441_5045_5249_4748];
        self.wow_local = [0.0; 2];
        self.wow_target = [0.0; 2];
        self.wow_until_target = 1;
        self.settle_times(controls);
    }

    fn update_freeze(
        &mut self,
        requested: bool,
        input_peak: f32,
        times: [f32; 2],
        routing: Routing,
    ) {
        if !requested {
            if matches!(self.freeze_state, FreezeState::Held) || self.freeze_mix > 0.0 {
                self.freeze_state = FreezeState::Releasing;
            } else {
                self.freeze_state = FreezeState::Off;
            }
            return;
        }

        match self.freeze_state {
            FreezeState::Off | FreezeState::Releasing => {
                if self.history_active || input_peak > QUIET_LEVEL {
                    self.freeze_state =
                        FreezeState::Capturing(self.capture_samples(times, routing));
                } else {
                    self.freeze_state = FreezeState::Armed;
                }
            }
            FreezeState::Armed => {
                // Arming waits for something to capture, not specifically for a *current* input.
                // Checking `input_peak` alone leaves Freeze armed forever over history that
                // arrived while it was armed and then stopped — a one-sample impulse, or the input
                // re-recorded when a topology replacement re-armed Freeze underneath it. This is
                // the same condition the `Off` arm above uses, and the asymmetry was the defect.
                if self.history_active || input_peak > QUIET_LEVEL {
                    self.freeze_state =
                        FreezeState::Capturing(self.capture_samples(times, routing));
                }
            }
            FreezeState::Capturing(remaining) => {
                self.freeze_state = if remaining <= 1 {
                    FreezeState::Held
                } else {
                    FreezeState::Capturing(remaining - 1)
                };
            }
            FreezeState::Held => {}
        }
    }

    fn capture_samples(&self, times: [f32; 2], routing: Routing) -> u32 {
        let laps = if routing == Routing::PingPong {
            2.0
        } else {
            1.0
        };
        (laps * times[0].max(times[1]) * self.sample_rate)
            .ceil()
            .clamp(1.0, u32::MAX as f32) as u32
    }

    fn duck_gain(&mut self, input_peak: f32, depth: f32) -> f32 {
        // The detector follows the input whatever the depth is. Freezing it at zero depth leaves a
        // stale loud envelope behind, so turning Duck back up over silence would duck against a
        // level that stopped existing minutes ago.
        let time = if input_peak > self.detector {
            0.004
        } else {
            0.180
        };
        let coefficient = 1.0 - (-1.0 / (time * self.sample_rate as f64)).exp() as f32;
        // `finite` also snaps subnormals: this detector never stops running while Freeze is held, so
        // its decay tail would otherwise settle into subnormal arithmetic and stay there.
        self.detector = finite(self.detector + coefficient * (input_peak - self.detector));
        if depth == 0.0 {
            return 1.0;
        }
        let reduction = self.detector / (self.detector + 0.08);
        (1.0 - 0.9 * depth * reduction).clamp(0.05, 1.0)
    }

    fn update_activity(&mut self, input_peak: f32, wet: [f32; 2], controls: &Controls) {
        let peak = input_peak.max(wet[0].abs()).max(wet[1].abs());
        let time = if peak > self.activity { 0.001 } else { 0.080 };
        let coefficient = 1.0 - (-1.0 / (time * self.sample_rate as f64)).exp() as f32;
        self.activity = finite(self.activity + coefficient * (peak - self.activity));
        if input_peak > QUIET_LEVEL || wet[0].abs().max(wet[1].abs()) > QUIET_LEVEL {
            self.quiet_samples = 0;
            self.history_active = true;
        } else {
            self.quiet_samples = self.quiet_samples.saturating_add(1);
            let horizon = ((MAX_DELAY_S + QUIET_HOLD_S) * self.sample_rate) as u64;
            if self.quiet_samples > horizon
                && self.freeze_state != FreezeState::Held
                && self.activity < QUIET_LEVEL
            {
                // Park on the controls actually in force. Rebuilding `Controls::default()` here
                // would settle the modulator to the default Shape and drop an engaged Freeze's
                // armed state, so the next excitation would wake with controls nobody selected.
                self.clear_audio(controls);
                self.parked = true;
            }
        }
    }

    fn tape_motion(&mut self, character: f32, model: Model) -> [f32; 2] {
        if model != Model::Tape || character == 0.0 {
            return [0.0; 2];
        }
        self.wow_phase += 0.23 / self.sample_rate as f64;
        self.wow_phase -= self.wow_phase.floor();
        if self.wow_until_target == 0 {
            for channel in 0..2 {
                self.wow_target[channel] = random_signed(&mut self.wow_rng[channel]);
            }
            self.wow_until_target = (0.12 * self.sample_rate) as u32;
        }
        self.wow_until_target -= 1;
        let drift_coefficient = 1.0 - (-1.0 / (0.08 * self.sample_rate as f64)).exp() as f32;
        for channel in 0..2 {
            self.wow_local[channel] +=
                drift_coefficient * (self.wow_target[channel] - self.wow_local[channel]);
        }
        let common = (self.wow_phase * core::f64::consts::TAU).sin() as f32;
        [
            character * (0.0035 * common + 0.0018 * self.wow_local[0]),
            character * (0.0035 * common + 0.0018 * self.wow_local[1]),
        ]
    }
}

/// The commanded per-line times for a routing, before motion and before any Change law.
///
/// Public so a host shell can label the pair it is about to ask for — an idle editor has no
/// processed sample to read back — without keeping a second copy of the law that would be free to
/// drift from this one.
#[inline]
pub fn routed_times(time_s: f32, offset: f32, routing: Routing) -> [f32; 2] {
    target_times(time_s, offset, routing)
}

#[inline]
fn target_times(time_s: f32, offset: f32, routing: Routing) -> [f32; 2] {
    let time = finite(time_s).clamp(MIN_DELAY_S, MAX_DELAY_S);
    if routing != Routing::Dual {
        return [time; 2];
    }
    // Geometric symmetry keeps Time as the centre of the pair while Offset selects which side
    // leads. At the ends the two lines are one octave apart.
    let ratio = 2.0f32.powf(finite(offset).clamp(-1.0, 1.0) * 0.5);
    [
        (time / ratio).clamp(MIN_DELAY_S, MAX_DELAY_S),
        (time * ratio).clamp(MIN_DELAY_S, MAX_DELAY_S),
    ]
}

#[inline]
fn modulated_delay(delay: f64, modulation: f32, sample_rate: f32) -> f64 {
    let minimum = MIN_DELAY_S as f64 * sample_rate as f64;
    let maximum = MAX_DELAY_S as f64 * sample_rate as f64;
    (delay * (1.0 + finite(modulation) as f64)).clamp(minimum, maximum)
}

#[inline]
fn feedback_gain(value: f32) -> f32 {
    // Unity at about 92.6% of travel; the top remains a bounded performance region.
    unit(value) * 1.08
}

/// Where the host input enters the lines. Ping-pong starts on the left from a mono source; every
/// other case injects each channel into its own line. `gate` is Freeze's injection door.
#[inline]
fn injection_for(routing: Routing, mono_input: bool, input: [f32; 2], gate: f32) -> [f32; 2] {
    match routing {
        Routing::PingPong if mono_input => [input[0] * gate, 0.0],
        _ => [input[0] * gate, input[1] * gate],
    }
}

#[inline]
fn feedback_matrix(wet: [f32; 2], routing: Routing, gain: f32) -> [f32; 2] {
    match routing {
        Routing::Standard | Routing::Dual => [wet[0] * gain, wet[1] * gain],
        Routing::PingPong => [wet[1] * gain, wet[0] * gain],
    }
}

#[inline]
fn playback_model(
    model: Model,
    input: f32,
    delay_samples: f64,
    character: f32,
    sample_rate: f32,
    state: &mut OnePole,
) -> f32 {
    match model {
        Model::Clean => input,
        Model::VintageDigital => {
            let seconds = delay_samples as f32 / sample_rate;
            let range_loss = (seconds / 0.25).max(1.0).sqrt();
            let cutoff = band_limited(
                18_000.0 * (1.0 - 0.55 * character) / range_loss,
                900.0,
                sample_rate,
            );
            state.low_pass(input, cutoff, sample_rate)
        }
        Model::Tape => {
            let seconds = delay_samples as f32 / sample_rate;
            let speed = (0.35 / seconds.max(MIN_DELAY_S)).clamp(0.08, 8.0);
            let cutoff = band_limited(
                (5_000.0 + 11_000.0 * speed.sqrt()) * (1.0 - 0.45 * character),
                1_200.0,
                sample_rate,
            );
            state.low_pass(input, cutoff, sample_rate)
        }
    }
}

/// A model's voice cutoff, kept inside the band the current rate can carry.
///
/// A model's nominal floor is a musical choice made at ordinary rates; the anti-image ceiling is
/// physics. At the low end of the accepted 1–768 kHz range the floor can exceed the ceiling, so the
/// ceiling wins and the model simply runs as dark as the rate allows. Ordering these as a bare
/// `clamp(floor, ceiling)` panics instead: `f32::clamp` requires `min <= max`.
#[inline]
fn band_limited(cutoff_hz: f32, floor_hz: f32, sample_rate: f32) -> f32 {
    let ceiling = sample_rate * 0.45;
    finite(cutoff_hz).clamp(floor_hz.min(ceiling), ceiling)
}

#[inline]
fn record_model(model: Model, input: f32, character: f32) -> f32 {
    let input = finite(input);
    match model {
        Model::Clean => loop_saturator(input, 0.0),
        Model::VintageDigital => {
            let bits = (16.0 - 8.0 * character).round().clamp(8.0, 16.0) as i32;
            let levels = ((1u32 << (bits - 1)) - 1) as f32;
            let limited = loop_saturator(input * (1.0 + 0.4 * character), character);
            (limited * levels).round() / levels
        }
        Model::Tape => loop_saturator(input * (1.0 + 1.5 * character), 0.25 + 0.75 * character),
    }
}

#[inline]
fn loop_saturator(input: f32, drive: f32) -> f32 {
    let x = finite(input) * (1.0 + 3.0 * unit(drive));
    let magnitude = x.abs();
    let bounded = if magnitude <= 1.0 {
        x
    } else {
        x.signum() * (1.0 + (magnitude - 1.0) / (1.0 + magnitude - 1.0))
    };
    bounded / (1.0 + 3.0 * unit(drive))
}

#[inline]
fn random_signed(state: &mut u64) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    ((x >> 40) as f32 / ((1u32 << 24) - 1) as f32) * 2.0 - 1.0
}

/// A crossfade whose endpoints are exact.
///
/// `a + (b - a) * 1.0` is not bit-exactly `b` in IEEE754, so without these two cases a fully held
/// Freeze leaks the ordinary path into the last bits of its own output — which makes "held reads
/// bypass conversion, filtering, drive and wear" true only to about eight decimal places, and lets
/// a Model change that cannot be heard still alter the samples.
#[inline]
fn lerp(a: f32, b: f32, mix: f32) -> f32 {
    let mix = unit(mix);
    if mix == 0.0 {
        return finite(a);
    }
    if mix == 1.0 {
        return finite(b);
    }
    finite(a + (b - a) * mix)
}

#[inline]
pub(crate) fn finite(value: f32) -> f32 {
    if value.is_finite() && value.abs() >= f32::MIN_POSITIVE {
        value
    } else {
        0.0
    }
}

#[inline]
fn unit(value: f32) -> f32 {
    finite(value).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::new(8_000.0).unwrap()
    }

    fn render_impulse(engine: &mut Engine, controls: Controls, samples: usize) -> Vec<[f32; 2]> {
        (0..samples)
            .map(|n| {
                let input = if n == 0 { [1.0, 0.0] } else { [0.0; 2] };
                engine.process(input, false, controls)
            })
            .collect()
    }

    #[test]
    fn invalid_rates_are_refused_and_range_edges_build() {
        assert!(matches!(
            Engine::new(f32::NAN),
            Err(BuildError::InvalidSampleRate)
        ));
        assert!(matches!(
            Engine::new(1.0),
            Err(BuildError::InvalidSampleRate)
        ));
        assert!(Engine::new(MIN_SAMPLE_RATE).is_ok());
        assert!(Engine::new(MAX_SAMPLE_RATE).is_ok());
    }

    #[test]
    fn mix_zero_is_bit_exact_dry_and_clears_history() {
        let mut engine = engine();
        let mut controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            ..Controls::default()
        };
        for n in 0..1000 {
            let _ = engine.process([if n == 0 { 0.8 } else { 0.0 }, 0.0], false, controls);
        }
        controls.mix = 0.0;
        for n in 0..32 {
            let input = [(n as f32 * 0.17).sin(), (n as f32 * 0.11).cos()];
            assert_eq!(engine.process(input, false, controls), input);
        }
        controls.mix = 1.0;
        for _ in 0..1000 {
            assert_eq!(engine.process([0.0; 2], false, controls), [0.0; 2]);
        }
    }

    #[test]
    fn mono_and_stereo_dry_paths_are_channel_correct() {
        let mut engine = engine();
        let controls = Controls {
            mix: 0.0,
            ..Controls::default()
        };
        assert_eq!(engine.process([0.2, -0.4], true, controls), [0.2, 0.2]);
        assert_eq!(engine.process([0.2, -0.4], false, controls), [0.2, -0.4]);
    }

    #[test]
    fn clean_integer_delay_places_the_first_repeat_exactly() {
        let mut engine = engine();
        let controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            feedback: 0.0,
            low_cut_hz: 10.0,
            high_cut_hz: 3_500.0,
            ..Controls::default()
        };
        engine.reset(controls);
        let rendered = render_impulse(&mut engine, controls, 100);
        // Loop filters add no delay line position ambiguity: energy starts at the requested age.
        // `time_s` is f32, so 10 ms at 8 kHz can land a fraction below sample 80. The direct
        // DelayLine test above owns exact integer ages; the integrated law must place the onset in
        // the one-sample interpolation support around the requested point.
        assert!(rendered[..79].iter().all(|frame| frame[0] == 0.0));
        assert!(
            rendered[79][0].abs().max(rendered[80][0].abs()) > 0.1,
            "{:?} {:?}",
            rendered[79],
            rendered[80]
        );
    }

    #[test]
    fn routing_preserves_anti_correlated_stereo_and_ping_pong_crosses() {
        let base = Controls {
            mix: 1.0,
            time_s: 0.01,
            feedback: 0.6,
            low_cut_hz: 10.0,
            high_cut_hz: 3_500.0,
            ..Controls::default()
        };
        let mut standard = engine();
        standard.reset(base);
        let first = standard.process([0.7, -0.7], false, base);
        assert_eq!(first, [0.0; 2]);
        let mut at_repeat = [0.0; 2];
        for _ in 0..80 {
            at_repeat = standard.process([0.0; 2], false, base);
        }
        assert!(at_repeat[0] * at_repeat[1] < 0.0, "{at_repeat:?}");

        let ping_controls = Controls {
            routing: Routing::PingPong,
            ..base
        };
        let mut ping = engine();
        ping.reset(ping_controls);
        let output = render_impulse(&mut ping, ping_controls, 180);
        assert!(output[80][0].abs() > output[80][1].abs());
        assert!(output[160][1].abs() > output[160][0].abs());
    }

    #[test]
    fn offset_is_inert_outside_dual_and_splits_dual_symmetrically() {
        let mut controls = Controls {
            offset: 1.0,
            ..Controls::default()
        };
        assert_eq!(
            target_times(controls.time_s, controls.offset, Routing::Standard),
            [controls.time_s; 2]
        );
        let dual = target_times(controls.time_s, controls.offset, Routing::Dual);
        assert!(dual[0] < controls.time_s && dual[1] > controls.time_s);
        let product = dual[0] * dual[1];
        assert!((product - controls.time_s * controls.time_s).abs() < 1e-5);
        controls.routing = Routing::PingPong;
        assert_eq!(
            target_times(controls.time_s, controls.offset, controls.routing),
            [controls.time_s; 2]
        );
    }

    #[test]
    fn non_finite_audio_and_controls_recover_without_poisoning_the_loop() {
        let mut engine = engine();
        let mut controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            ..Controls::default()
        };
        controls.feedback = f32::NAN;
        controls.low_cut_hz = f32::INFINITY;
        controls.high_cut_hz = f32::NEG_INFINITY;
        controls.motion = f32::NAN;
        let _ = engine.process([f32::NAN, f32::INFINITY], false, controls);
        controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            ..Controls::default()
        };
        for n in 0..10_000 {
            let output = engine.process([if n == 0 { 0.2 } else { 0.0 }, 0.0], false, controls);
            assert!(output[0].is_finite() && output[1].is_finite());
        }
    }

    #[test]
    fn all_models_are_bounded_above_unity() {
        for model in [Model::Clean, Model::VintageDigital, Model::Tape] {
            let mut engine = engine();
            let controls = Controls {
                model,
                mix: 1.0,
                time_s: 0.005,
                feedback: 1.0,
                drive: 1.0,
                character: 1.0,
                ..Controls::default()
            };
            engine.reset(controls);
            for n in 0..200_000 {
                let output = engine.process([if n == 0 { 1.0 } else { 0.0 }, 0.0], false, controls);
                assert!(
                    output[0].is_finite() && output[0].abs() <= 2.1,
                    "{model:?} {output:?}"
                );
            }
        }
    }

    #[test]
    fn every_model_processes_at_every_accepted_rate() {
        // A model's cutoff floor is a musical choice; the anti-image ceiling is the rate's. At the
        // bottom of the accepted range the floor exceeds the ceiling, and ordering those as a bare
        // `clamp(floor, ceiling)` panics on the audio thread rather than simply running dark.
        for rate in [MIN_SAMPLE_RATE, 1_500.0, 8_000.0, 48_000.0, MAX_SAMPLE_RATE] {
            for model in [Model::Clean, Model::VintageDigital, Model::Tape] {
                for routing in [Routing::Standard, Routing::Dual, Routing::PingPong] {
                    let mut engine = Engine::new(rate).unwrap();
                    let controls = Controls {
                        model,
                        routing,
                        mix: 1.0,
                        time_s: 0.05,
                        feedback: 0.7,
                        character: 1.0,
                        high_cut_hz: 18_000.0,
                        ..Controls::default()
                    };
                    engine.reset(controls);
                    for n in 0..64 {
                        let input = if n == 0 { [0.7, -0.7] } else { [0.0; 2] };
                        let output = engine.process(input, false, controls);
                        assert!(
                            output[0].is_finite() && output[1].is_finite(),
                            "{rate} {model:?} {routing:?} {output:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn jump_reaches_exact_silence_at_the_address_change_with_populated_colour() {
        // The de-click envelope has to sit downstream of the loop's stateful stages. Applied before
        // them, a one-pole with populated state keeps ringing straight through the address change.
        let mut engine = engine();
        let controls = Controls {
            mix: 1.0,
            change: Change::Jump,
            model: Model::Tape,
            character: 1.0,
            time_s: 0.05,
            feedback: 0.85,
            drive: 0.8,
            duck: 0.0,
            ..Controls::default()
        };
        engine.reset(controls);
        for n in 0..4_000 {
            let _ = engine.process([if n < 64 { 0.8 } else { 0.0 }, 0.0], false, controls);
        }
        let moved = Controls {
            time_s: 0.2,
            ..controls
        };
        let mut silent_samples = 0;
        for _ in 0..1_000 {
            let output = engine.process([0.0; 2], false, moved);
            if output[0] == 0.0 && output[1] == 0.0 {
                silent_samples += 1;
            }
        }
        assert!(
            silent_samples > 0,
            "Jump never reached exact silence at the address change"
        );
    }

    #[test]
    fn a_held_field_crossfades_a_fade_time_edit_instead_of_jumping() {
        let mut engine = engine();
        let running = Controls {
            mix: 1.0,
            change: Change::Fade,
            time_s: 0.05,
            duck: 0.0,
            motion: 0.0,
            ..Controls::default()
        };
        let holding = Controls {
            freeze: true,
            ..running
        };
        engine.reset(running);
        // Fill the line with structure, then hold it.
        for n in 0..4_000 {
            let value = ((n as f32) * 0.21).sin() * 0.6;
            let _ = engine.process([value, -value], false, running);
        }
        for _ in 0..4_000 {
            let _ = engine.process([0.0; 2], false, holding);
        }
        assert!(engine.freeze_is_held());

        let moved = Controls {
            time_s: 0.12,
            ..holding
        };
        // Assert the blend itself rather than a step size. A held field recirculates losslessly, so
        // its two projections can hold similar values and any amplitude threshold ends up measuring
        // the field's own loop seam instead of the transition.
        let mut checked = 0;
        for _ in 0..2_000 {
            let plan = engine.times[0].plan();
            if !plan.has_second() {
                let _ = engine.process([0.0; 2], false, moved);
                continue;
            }
            let first = engine.lines[0].read(plan.first.round());
            let second = engine.lines[0].read(plan.second.round());
            let expected = plan.blend(first, second);
            let output = engine.process([0.0; 2], false, moved);
            assert!(
                (output[0] - expected).abs() < 1e-6,
                "held output {} is not the crossfade {expected} of {first} and {second}",
                output[0]
            );
            checked += 1;
        }
        // Reading only the first projection would hold the old address for the whole transition and
        // then step to the new one at promotion, which is the click Fade exists to avoid.
        assert!(checked > 100, "the held Fade never ran: {checked} samples");
    }

    #[test]
    fn discrete_edits_while_parked_settle_and_do_not_erase_the_waking_input() {
        let mut engine = engine();
        let clean = Controls {
            mix: 1.0,
            time_s: 0.05,
            feedback: 0.0,
            duck: 0.0,
            ..Controls::default()
        };
        engine.reset(clean);
        assert!(engine.is_parked());

        // Change model and routing while parked, then wake with one impulse.
        let moved = Controls {
            model: Model::Tape,
            routing: Routing::PingPong,
            ..clean
        };
        let rendered: Vec<[f32; 2]> = (0..2_000)
            .map(|n| engine.process([if n == 0 { 1.0 } else { 0.0 }, 0.0], false, moved))
            .collect();
        let repeat = rendered
            .iter()
            .skip(1)
            .map(|frame| frame[0].abs().max(frame[1].abs()))
            .fold(0.0f32, f32::max);
        // A topology fade left pending across the silence would ramp down over the waking input and
        // clear the line underneath it, so the first repeat would never arrive.
        assert!(repeat > 0.1, "the waking input was erased: peak {repeat}");
    }

    #[test]
    fn a_held_field_survives_a_model_change_and_is_replaced_by_a_routing_change() {
        // Owner decision, 2026-09-18: Model must not destroy a held field. While fully held, Model
        // is inaudible — held reads bypass playback colour and the recirculating write bypasses the
        // record stage — so clearing the line on a Model change deleted the captured sound for no
        // audible benefit. Routing genuinely changes what a lap means and still replaces it.
        fn held_engine() -> (Engine, Controls) {
            let mut engine = engine();
            let running = Controls {
                mix: 1.0,
                time_s: 0.05,
                feedback: 0.4,
                duck: 0.0,
                motion: 0.0,
                ..Controls::default()
            };
            let frozen = Controls {
                freeze: true,
                ..running
            };
            engine.reset(running);
            for n in 0..4_000 {
                let value = ((n as f32) * 0.21).sin() * 0.6;
                let _ = engine.process([value, -value], false, running);
            }
            for _ in 0..4_000 {
                let _ = engine.process([0.0; 2], false, frozen);
            }
            assert!(engine.freeze_is_held());
            (engine, frozen)
        }

        // Model is not merely preserved, it is inaudible: the two renders agree sample for sample.
        let render = |model: Option<Model>| -> Vec<[f32; 2]> {
            let (mut engine, frozen) = held_engine();
            let controls = match model {
                Some(model) => Controls { model, ..frozen },
                None => frozen,
            };
            (0..3_000)
                .map(|_| engine.process([0.0; 2], false, controls))
                .collect()
        };
        let untouched = render(None);
        assert!(
            untouched.iter().any(|frame| frame[0].abs() > 0.01),
            "the held field was silent, so this proves nothing"
        );
        assert_eq!(
            render(Some(Model::Tape)),
            untouched,
            "a Model change disturbed a held field"
        );
        assert_eq!(render(Some(Model::VintageDigital)), untouched);

        // Routing still replaces the field.
        let (mut engine, frozen) = held_engine();
        let rerouted = Controls {
            routing: Routing::PingPong,
            ..frozen
        };
        for _ in 0..4_000 {
            let _ = engine.process([0.0; 2], false, rerouted);
        }
        assert!(
            !engine.freeze_is_held(),
            "a Routing change must replace the held field"
        );
    }

    #[test]
    fn the_held_model_shortcut_respects_both_edges_of_the_hold() {
        // Both edges of the hold were once decided by luck: the eligibility test read
        // `freeze_state` and `freeze_mix` at the top of the callback, where they still describe the
        // previous sample. It is taken after they settle now, and these two cases hold it there.
        let running = Controls {
            mix: 1.0,
            time_s: 0.05,
            feedback: 0.4,
            duck: 0.0,
            motion: 0.0,
            ..Controls::default()
        };
        let frozen = Controls {
            freeze: true,
            ..running
        };

        // Releasing edge: Freeze off and a Model change on the same sample. The ordinary path
        // becomes audible immediately, so the model owes the ordinary crossfade — it must not take
        // the held shortcut and swap instantly underneath the release.
        let mut probe = engine();
        probe.reset(running);
        for n in 0..4_000 {
            let value = ((n as f32) * 0.21).sin() * 0.6;
            let _ = probe.process([value, -value], false, running);
        }
        for _ in 0..4_000 {
            let _ = probe.process([0.0; 2], false, frozen);
        }
        assert!(probe.freeze_is_held());
        let released_and_recoloured = Controls {
            freeze: false,
            model: Model::Tape,
            ..frozen
        };
        let _ = probe.process([0.0; 2], false, released_and_recoloured);
        assert_ne!(
            probe.topology.stage,
            TopologyStage::Stable,
            "a Model change released on the same sample skipped the topology crossfade"
        );

        // Capturing edge: a Model change landing while the capture is still ramping starts a
        // crossfade that reaches its zero a few milliseconds later, by which time the field is
        // fully held. The field must survive that, or the same gesture is destructive purely
        // because the edit was a few milliseconds early.
        for early in [1usize, 40, 200] {
            let mut probe = engine();
            probe.reset(running);
            for n in 0..4_000 {
                let value = ((n as f32) * 0.21).sin() * 0.6;
                let _ = probe.process([value, -value], false, running);
            }
            // Engage Freeze, then change Model `early` samples into the capture.
            for _ in 0..early {
                let _ = probe.process([0.0; 2], false, frozen);
            }
            let recoloured = Controls {
                model: Model::Tape,
                ..frozen
            };
            for _ in 0..8_000 {
                let _ = probe.process([0.0; 2], false, recoloured);
            }
            assert!(
                probe.freeze_is_held(),
                "a Model change {early} samples into the capture destroyed the field"
            );
            let level = (0..800)
                .map(|_| {
                    let out = probe.process([0.0; 2], false, recoloured);
                    out[0].abs().max(out[1].abs())
                })
                .fold(0.0f32, f32::max);
            assert!(
                level > 0.01,
                "the field survived in name only at {early}: peak {level}"
            );
        }
    }

    #[test]
    fn a_model_change_on_the_sample_the_hold_completes_starts_no_crossfade() {
        // The narrowest edge of all: `freeze_state` is already `Held` but `freeze_mix` is one step
        // short of 1.0. Judged on the previous sample that looked un-held, so a crossfade started
        // that was no longer owed by the time the sample ended — and because `return_signal` carries
        // `topology.gain` and a frozen field records its own return, the fade was written into the
        // field as a permanent hole rather than a passing dip.
        let running = Controls {
            mix: 1.0,
            time_s: 0.05,
            feedback: 0.4,
            duck: 0.0,
            motion: 0.0,
            ..Controls::default()
        };
        let frozen = Controls {
            freeze: true,
            ..running
        };

        // Advance to the exact sample on which the hold fade will reach 1.0.
        let at_the_completing_sample = || {
            let mut probe = engine();
            probe.reset(running);
            for n in 0..4_000 {
                let value = ((n as f32) * 0.21).sin() * 0.6;
                let _ = probe.process([value, -value], false, running);
            }
            for _ in 0..20_000 {
                if probe.freeze_state == FreezeState::Held
                    && probe.freeze_mix < 1.0
                    && probe.freeze_mix + probe.freeze_step >= 1.0
                {
                    return probe;
                }
                let _ = probe.process([0.0; 2], false, frozen);
            }
            panic!("never reached the completing sample of the hold fade");
        };

        let recoloured = Controls {
            model: Model::Tape,
            ..frozen
        };
        let mut probe = at_the_completing_sample();
        let _ = probe.process([0.0; 2], false, recoloured);
        assert_eq!(
            probe.topology.stage,
            TopologyStage::Stable,
            "a Model change on the completing sample started a crossfade it does not owe"
        );
        assert_eq!(probe.topology.active_model, Model::Tape);

        // And the field is untouched: identical to the same engine never asked to change Model.
        let render = |controls: Controls| -> Vec<[f32; 2]> {
            let mut probe = at_the_completing_sample();
            (0..3_000)
                .map(|_| probe.process([0.0; 2], false, controls))
                .collect()
        };
        let untouched = render(frozen);
        assert!(
            untouched.iter().any(|frame| frame[0].abs() > 0.01),
            "the held field was silent, so this proves nothing"
        );
        assert_eq!(
            render(recoloured),
            untouched,
            "a Model change on the completing sample disturbed the field"
        );
    }

    #[test]
    fn replacing_the_topology_parks_the_emptied_engine_and_reports_no_tail() {
        // The crossfade's zero crossing clears both lines. The pre-clear wet value must not then
        // resurrect activity, or an engine with nothing in it stays awake for the whole
        // hidden-history horizon and, at unity feedback, reports itself as an endless tail.
        for freeze in [false, true] {
            let mut engine = engine();
            let running = Controls {
                mix: 1.0,
                time_s: 0.05,
                feedback: 1.0,
                duck: 0.0,
                freeze,
                ..Controls::default()
            };
            engine.reset(Controls {
                freeze: false,
                ..running
            });
            for n in 0..8_000 {
                let value = if n < 256 { 0.9 } else { 0.0 };
                let _ = engine.process([value, -value], false, running);
            }
            assert!(!engine.is_parked(), "freeze {freeze}: never woke");

            // Now change Model and Routing over silence and let the crossfade complete.
            let moved = Controls {
                model: Model::Tape,
                routing: Routing::PingPong,
                ..running
            };
            for _ in 0..4_000 {
                let _ = engine.process([0.0; 2], false, moved);
            }
            assert!(
                engine.is_parked(),
                "freeze {freeze}: the emptied engine did not park"
            );
            assert!(
                !engine.is_sustaining(moved),
                "freeze {freeze}: an empty engine reported itself as sustaining"
            );
        }
    }

    #[test]
    fn input_landing_on_the_replacement_sample_survives_into_the_new_topology() {
        // The write into the outgoing line happens before the crossfade's zero crossing clears it.
        // An input arriving on exactly that sample is therefore written and immediately erased —
        // which drops it from the audio and, worse, still counts it as history the engine no longer
        // holds. Place an impulse on precisely that sample and require its repeat.
        let mut engine = engine();
        let clean = Controls {
            mix: 1.0,
            time_s: 0.05,
            feedback: 0.0,
            duck: 0.0,
            ..Controls::default()
        };
        engine.reset(clean);
        for n in 0..1_000 {
            let _ = engine.process([if n < 8 { 0.5 } else { 0.0 }, 0.0], false, clean);
        }

        let moved = Controls {
            model: Model::Tape,
            routing: Routing::Dual,
            ..clean
        };
        let mut placed = false;
        let mut rendered = Vec::new();
        for _ in 0..2_000 {
            // The next call replaces the topology when this step drives the gain to zero.
            let replacing = engine.topology.stage == TopologyStage::Down
                && engine.topology.gain - engine.topology.step <= 0.0;
            let input = if replacing { [1.0, 1.0] } else { [0.0; 2] };
            placed |= replacing;
            rendered.push(engine.process(input, false, moved));
        }
        assert!(placed, "the topology never reached its replacement sample");

        let repeat = rendered
            .iter()
            .map(|frame| frame[0].abs().max(frame[1].abs()))
            .fold(0.0f32, f32::max);
        assert!(
            repeat > 0.1,
            "the input on the replacement sample was dropped: peak {repeat}"
        );
        // The engine genuinely holds that input, so it must be awake and say so.
        assert!(!engine.is_parked());
    }

    #[test]
    fn freeze_captures_input_that_arrived_on_the_replacement_sample() {
        // The replacement re-arms Freeze after `update_freeze` has already run for that sample, so
        // the re-recorded input is history that Freeze never saw arrive. Arming must still resolve
        // into a capture, or a one-sample input landing there is repeated but never held.
        let mut engine = engine();
        let running = Controls {
            mix: 1.0,
            time_s: 0.05,
            feedback: 0.3,
            duck: 0.0,
            ..Controls::default()
        };
        let frozen = Controls {
            freeze: true,
            ..running
        };
        engine.reset(running);
        for n in 0..1_000 {
            let _ = engine.process([if n < 8 { 0.5 } else { 0.0 }, 0.0], false, running);
        }

        let moved = Controls {
            model: Model::VintageDigital,
            routing: Routing::Dual,
            ..frozen
        };
        let mut placed = false;
        for _ in 0..1_000 {
            let replacing = engine.topology.stage == TopologyStage::Down
                && engine.topology.gain - engine.topology.step <= 0.0;
            let input = if replacing { [1.0, 1.0] } else { [0.0; 2] };
            placed |= replacing;
            let _ = engine.process(input, false, moved);
        }
        assert!(placed, "the topology never reached its replacement sample");

        // No further input at all: the only thing to capture is what landed on that one sample.
        for _ in 0..8_000 {
            let _ = engine.process([0.0; 2], false, moved);
        }
        assert!(
            engine.freeze_is_held(),
            "Freeze stayed armed over history it already held: {:?}",
            engine.freeze_state
        );
    }

    #[test]
    fn duck_follows_the_input_even_at_zero_depth() {
        // Duck's detector is a level over time. Freezing it while the depth is zero leaves a stale
        // loud envelope, so turning Duck back up over silence ducks against a level that is gone.
        let mut engine = engine();
        let ducking = Controls {
            mix: 1.0,
            duck: 1.0,
            time_s: 0.05,
            feedback: 0.0,
            ..Controls::default()
        };
        let idle = Controls {
            duck: 0.0,
            ..ducking
        };
        engine.reset(ducking);

        // Charge the detector with loud input while Duck is engaged...
        for _ in 0..2_000 {
            let _ = engine.process([0.9, 0.9], false, ducking);
        }
        assert!(
            engine.detector > 0.5,
            "the detector never charged: {}",
            engine.detector
        );

        // ...then automate Duck to zero and go quiet for well over its 180 ms release.
        for _ in 0..20_000 {
            let _ = engine.process([0.0; 2], false, idle);
        }
        assert!(
            engine.detector < 1.0e-3,
            "the detector kept a stale envelope at zero depth: {}",
            engine.detector
        );

        // Re-engaging Duck over silence must not suppress anything.
        let gain = engine.duck_gain(0.0, 1.0);
        assert!(gain > 0.99, "Duck suppressed a silent input by {gain}");
    }

    #[test]
    fn an_armed_freeze_with_no_input_stays_parked() {
        let mut engine = engine();
        let controls = Controls {
            mix: 1.0,
            freeze: true,
            ..Controls::default()
        };
        engine.reset(controls);
        for _ in 0..200_000 {
            let output = engine.process([0.0; 2], false, controls);
            assert_eq!(output, [0.0; 2]);
        }
        assert!(
            engine.is_parked(),
            "an armed Freeze with nothing to capture must not run the full path on silence"
        );
        // It still captures the moment real input arrives.
        for n in 0..8_000 {
            let _ = engine.process([if n < 32 { 0.9 } else { 0.0 }, 0.0], false, controls);
        }
        assert!(!engine.is_parked());
    }

    #[test]
    fn recursive_state_does_not_settle_into_subnormal_arithmetic() {
        let mut engine = engine();
        let controls = Controls {
            mix: 1.0,
            duck: 1.0,
            time_s: 0.02,
            feedback: 0.3,
            model: Model::Tape,
            character: 1.0,
            freeze: true,
            ..Controls::default()
        };
        engine.reset(Controls {
            freeze: false,
            ..controls
        });
        for n in 0..2_000 {
            let _ = engine.process(
                [if n < 32 { 0.9 } else { 0.0 }, 0.0],
                false,
                Controls {
                    freeze: false,
                    ..controls
                },
            );
        }
        // A held Freeze keeps the engine running forever, so every recursive accumulator must reach
        // exact zero once its input decays rather than grinding on in the subnormal range.
        for _ in 0..400_000 {
            let _ = engine.process([0.0; 2], false, controls);
        }
        assert!(engine.freeze_is_held());
        assert_eq!(engine.detector, 0.0, "duck detector held a subnormal");
    }

    #[test]
    fn the_documented_activation_envelope_matches_the_allocation() {
        // The contract publishes this envelope; keep the arithmetic that produced it in the tests
        // so widening a stored field cannot silently falsify it.
        let slot = core::mem::size_of::<f32>() + core::mem::size_of::<u32>();
        assert_eq!(slot, 8, "one sample plus one generation tag per slot");
        for (rate, documented) in [(48_000.0f64, 5.9f64), (192_000.0, 23.4), (768_000.0, 93.8)] {
            let slots = (rate * MAX_DELAY_S as f64).ceil() + 4.0;
            let mib = 2.0 * slots * slot as f64 / (1024.0 * 1024.0);
            assert!(
                (mib - documented).abs() < 0.1,
                "{rate} Hz: {mib} MiB allocated, contract publishes {documented} MiB"
            );
        }
    }

    #[test]
    fn the_fractional_read_is_measured_against_linear_interpolation() {
        // `DelayLine::read` claims an advantage over linear interpolation. Measure it over a
        // frequency and fraction sweep rather than at one condition, and assert only what the
        // measurement supports: the cubic's advantage is in retained magnitude, and it grows with
        // frequency. Its delay accuracy is excellent but is *not* better than linear's, which is
        // why the doc comment on `read` claims magnitude and bounded overshoot and nothing else.
        const RATE: f64 = 48_000.0;
        const N: usize = 8_192;

        /// One complex DFT bin: amplitude and phase at `hz`.
        fn bin(x: &[f32], hz: f64, rate: f64) -> (f64, f64) {
            let w = core::f64::consts::TAU * hz / rate;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (n, sample) in x.iter().enumerate() {
                let phase = w * n as f64;
                re += *sample as f64 * phase.cos();
                im -= *sample as f64 * phase.sin();
            }
            let n = x.len() as f64;
            (2.0 * (re * re + im * im).sqrt() / n, im.atan2(re))
        }

        // A short age keeps the interpolator's behaviour identical while making the measurement
        // tractable; the integer part of the age does not enter the kernel.
        const AGE_BASE: f64 = 4.0;
        let mut worst_magnitude_ratio = f64::INFINITY;
        let mut smallest_magnitude_gain = f64::INFINITY;
        let mut worst_delay_error = 0.0f64;
        let mut delay_points = 0;

        for target_hz in [1_000.0, 3_000.0, 6_000.0, 9_000.0, 12_000.0] {
            let hz = mxm_measure::stimulus::periodic_frequency(N, target_hz, RATE);
            let source = mxm_measure::stimulus::periodic_sine(N, hz, RATE, 0.5);
            let reference = bin(&source, hz, RATE);
            let w = core::f64::consts::TAU * hz / RATE;

            for fraction in [0.25, 0.5, 0.75] {
                let age = AGE_BASE + fraction;
                let mut cubic = Vec::with_capacity(N);
                let mut linear = Vec::with_capacity(N);
                let mut line = DelayLine::new(RATE as f32).unwrap();
                for sample in &source {
                    line.write(*sample);
                    cubic.push(line.read(age));
                    linear.push(
                        ((1.0 - fraction) as f32) * line.read(age.floor())
                            + (fraction as f32) * line.read(age.floor() + 1.0),
                    );
                }

                let cubic_bin = bin(&cubic, hz, RATE);
                let linear_bin = bin(&linear, hz, RATE);
                assert!(
                    cubic_bin.0 > linear_bin.0,
                    "{hz} Hz frac {fraction}: cubic {} did not beat linear {}",
                    cubic_bin.0,
                    linear_bin.0
                );
                worst_magnitude_ratio = worst_magnitude_ratio.min(cubic_bin.0 / reference.0);
                smallest_magnitude_gain = smallest_magnitude_gain.min(cubic_bin.0 / linear_bin.0);

                // Age 1 is the sample just written, so an age of `age` is `age - 1` of latency. The
                // phase only implies that unambiguously while it stays inside half a turn.
                let expected = age - 1.0;
                if w * expected < core::f64::consts::PI * 0.9 {
                    let implied = -mxm_measure::spectrum::princarg(cubic_bin.1 - reference.1) / w;
                    worst_delay_error = worst_delay_error.max((implied - expected).abs());
                    delay_points += 1;
                }
            }
        }

        // 6 kHz at fraction 0.75 already exceeds half a turn, so eight of the fifteen
        // sweep points carry an unambiguous delay measurement.
        assert!(
            delay_points >= 8,
            "only {delay_points} unambiguous delay points"
        );
        assert!(
            worst_delay_error < 0.02,
            "worst fractional-delay error was {worst_delay_error} samples"
        );
        // 12 kHz at a half-sample read is the hardest point in the sweep.
        assert!(
            worst_magnitude_ratio > 0.85,
            "worst band loss was {:.1}%",
            100.0 * (1.0 - worst_magnitude_ratio)
        );
        assert!(
            smallest_magnitude_gain > 1.001,
            "cubic's smallest magnitude advantage over linear was {smallest_magnitude_gain}"
        );
    }

    #[test]
    fn repitch_travels_in_pitch_where_fade_does_not() {
        // The crate's headline claim is that the three change laws are audibly different. Repitch
        // moves the read relationship, so the repeat must shift in pitch while it travels; Fade
        // crossfades two fixed reads, so it must not.
        const RATE: f64 = 48_000.0;
        const HZ: f64 = 1_000.0;
        let measure = |change: Change| {
            let mut engine = Engine::new(RATE as f32).unwrap();
            let controls = Controls {
                mix: 1.0,
                change,
                time_s: 0.1,
                feedback: 0.0,
                duck: 0.0,
                motion: 0.0,
                low_cut_hz: 10.0,
                high_cut_hz: 18_000.0,
                ..Controls::default()
            };
            engine.reset(controls);
            let source = mxm_measure::stimulus::sine(24_000, HZ, RATE, 0.6);
            for sample in &source {
                let _ = engine.process([*sample, *sample], false, controls);
            }
            // Command the change, then listen across the whole 25 ms transition.
            let moved = Controls {
                time_s: 0.105,
                ..controls
            };
            let window: Vec<f32> = (0..1_200)
                .map(|n| engine.process([source[n], source[n]], false, moved)[0])
                .collect();
            mxm_measure::pitch::frequency_by_crossings(&window, RATE).unwrap()
        };

        let repitched = measure(Change::Repitch);
        let faded = measure(Change::Fade);
        assert!(
            repitched < HZ * 0.95,
            "Repitch did not travel: {repitched} Hz against {HZ} Hz"
        );
        assert!(
            (faded - HZ).abs() < HZ * 0.05,
            "Fade shifted pitch: {faded} Hz against {HZ} Hz"
        );
    }

    #[test]
    fn finite_tail_bound_includes_history_a_later_time_edit_can_expose() {
        let mut engine = engine();
        let controls = Controls {
            mix: 1.0,
            ..Controls::default()
        };
        engine.reset(controls);
        let _ = engine.process([0.8, 0.0], false, controls);
        let remaining = engine.remaining_tail_seconds(controls).unwrap();
        assert!(remaining > MAX_DELAY_S + QUIET_HOLD_S, "{remaining}");
    }

    #[test]
    fn duck_hides_output_without_erasing_the_tail() {
        let controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            feedback: 0.8,
            duck: 1.0,
            ..Controls::default()
        };
        let mut ducked = engine();
        ducked.reset(controls);
        let mut plain = engine();
        let plain_controls = Controls {
            duck: 0.0,
            ..controls
        };
        plain.reset(plain_controls);
        for n in 0..500 {
            let input = if n < 100 { [0.8; 2] } else { [0.0; 2] };
            let a = ducked.process(input, false, controls);
            let b = plain.process(input, false, plain_controls);
            if n == 90 {
                assert!(a[0].abs() < b[0].abs());
            }
        }
        let mut duck_tail = 0.0f32;
        let mut plain_tail = 0.0f32;
        for _ in 0..2000 {
            duck_tail = duck_tail.max(ducked.process([0.0; 2], false, controls)[0].abs());
            plain_tail = plain_tail.max(plain.process([0.0; 2], false, plain_controls)[0].abs());
        }
        assert!(duck_tail > 0.0 && plain_tail > 0.0);
    }

    #[test]
    fn freeze_arms_on_silence_captures_later_and_rejects_late_input() {
        let mut engine = engine();
        let mut controls = Controls {
            freeze: true,
            mix: 1.0,
            time_s: 0.01,
            feedback: 0.4,
            ..Controls::default()
        };
        engine.reset(controls);
        for _ in 0..200 {
            assert_eq!(engine.process([0.0; 2], false, controls), [0.0; 2]);
        }
        for n in 0..500 {
            let _ = engine.process([if n == 0 { 0.8 } else { 0.0 }, 0.0], false, controls);
        }
        assert!(engine.freeze_is_held());
        let before = engine.activity();
        for _ in 0..500 {
            let _ = engine.process([0.9, -0.9], false, controls);
        }
        assert!(engine.activity().is_finite() && before.is_finite());
        controls.freeze = false;
        for _ in 0..1000 {
            let _ = engine.process([0.0; 2], false, controls);
        }
        assert!(!engine.freeze_is_held());
    }

    #[test]
    fn freeze_switch_on_a_single_sample_captures_and_ping_pong_waits_a_full_cycle() {
        let mut engine = engine();
        let mut controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            freeze: false,
            duck: 0.0,
            ..Controls::default()
        };
        engine.reset(controls);
        assert_eq!(engine.capture_samples([0.01; 2], Routing::Standard), 80);
        assert_eq!(engine.capture_samples([0.01; 2], Routing::PingPong), 160);

        controls.freeze = true;
        let _ = engine.process([0.8, 0.0], false, controls);
        for _ in 0..400 {
            let _ = engine.process([0.0; 2], false, controls);
        }
        assert!(engine.freeze_is_held());
    }

    #[test]
    fn held_tape_field_is_periodic_and_rejects_late_input() {
        let controls = Controls {
            model: Model::Tape,
            mix: 1.0,
            time_s: 0.01,
            feedback: 0.6,
            character: 1.0,
            motion: 1.0,
            freeze: true,
            duck: 0.0,
            low_cut_hz: 10.0,
            high_cut_hz: 3_500.0,
            ..Controls::default()
        };
        let mut control = engine();
        let mut challenged = engine();
        control.reset(controls);
        challenged.reset(controls);
        for sample in 0..600 {
            let input = [if sample == 0 { 0.8 } else { 0.0 }, 0.0];
            assert_eq!(
                control.process(input, false, controls),
                challenged.process(input, false, controls)
            );
        }
        assert!(control.freeze_is_held() && challenged.freeze_is_held());

        let mut held = Vec::new();
        for sample in 0..240 {
            let a = control.process([0.0; 2], false, controls);
            let b = challenged.process(
                if sample == 0 { [0.9, -0.9] } else { [0.0; 2] },
                false,
                controls,
            );
            assert_eq!(a, b, "late input entered the captured field at {sample}");
            held.push(a);
        }
        assert_eq!(&held[80..160], &held[160..240]);
    }

    #[test]
    fn reset_is_deterministic_and_block_boundaries_are_not_state() {
        let controls = Controls {
            model: Model::Tape,
            routing: Routing::Dual,
            motion: 0.7,
            shape: Shape::Random,
            mix: 1.0,
            time_s: 0.02,
            ..Controls::default()
        };
        let render = |engine: &mut Engine| {
            engine.reset(controls);
            (0..5000)
                .map(|n| engine.process([if n == 0 { 0.5 } else { 0.0 }, 0.0], false, controls))
                .collect::<Vec<_>>()
        };
        let mut engine = engine();
        let a = render(&mut engine);
        let b = render(&mut engine);
        assert_eq!(a, b);
    }

    #[test]
    fn model_and_routing_churn_stays_finite_and_reaches_latest_topology() {
        let mut engine = engine();
        let mut controls = Controls {
            mix: 1.0,
            time_s: 0.01,
            ..Controls::default()
        };
        for n in 0..20_000 {
            if n % 17 == 0 {
                controls.model = match (n / 17) % 3 {
                    0 => Model::Clean,
                    1 => Model::VintageDigital,
                    _ => Model::Tape,
                };
                controls.routing = match (n / 17) % 3 {
                    0 => Routing::Standard,
                    1 => Routing::Dual,
                    _ => Routing::PingPong,
                };
            }
            let output = engine.process([if n == 0 { 0.5 } else { 0.0 }, 0.0], false, controls);
            assert!(output[0].is_finite() && output[1].is_finite());
        }
        controls.model = Model::Tape;
        controls.routing = Routing::Dual;
        for _ in 0..1000 {
            let _ = engine.process([0.0; 2], false, controls);
        }
        assert_eq!(engine.topology.active_model, Model::Tape);
        assert_eq!(engine.topology.active_routing, Routing::Dual);
        assert_eq!(engine.topology.stage, TopologyStage::Stable);
    }
}
