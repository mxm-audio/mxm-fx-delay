//! Permanent parameter definitions and the one Time law shared by audio and editor.

use std::sync::{Arc, RwLock};

use mxm_fx_delay_dsp::{Change, MAX_DELAY_S, MIN_DELAY_S, Model, Routing, Shape};
use mxm_preset::PresetIdentity;
use nice_plug::prelude::*;

pub const DEFAULT_TIME_S: f32 = 0.35;
pub const DEFAULT_MIX: f32 = 0.35;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeChoice {
    #[id = "repitch"]
    #[name = "Repitch"]
    Repitch,
    #[id = "fade"]
    #[name = "Fade"]
    Fade,
    #[id = "jump"]
    #[name = "Jump"]
    Jump,
}

impl ChangeChoice {
    pub const fn dsp(self) -> Change {
        match self {
            Self::Repitch => Change::Repitch,
            Self::Fade => Change::Fade,
            Self::Jump => Change::Jump,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingChoice {
    #[id = "standard"]
    #[name = "Standard"]
    Standard,
    #[id = "dual"]
    #[name = "Dual"]
    Dual,
    #[id = "ping-pong"]
    #[name = "Ping-pong"]
    PingPong,
}

impl RoutingChoice {
    pub const fn dsp(self) -> Routing {
        match self {
            Self::Standard => Routing::Standard,
            Self::Dual => Routing::Dual,
            Self::PingPong => Routing::PingPong,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelChoice {
    #[id = "clean"]
    #[name = "Clean"]
    Clean,
    #[id = "vintage-digital"]
    #[name = "Vintage digital"]
    VintageDigital,
    #[id = "tape"]
    #[name = "Tape"]
    Tape,
}

impl ModelChoice {
    pub const fn dsp(self) -> Model {
        match self {
            Self::Clean => Model::Clean,
            Self::VintageDigital => Model::VintageDigital,
            Self::Tape => Model::Tape,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeChoice {
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    #[id = "random"]
    #[name = "Random"]
    Random,
}

impl ShapeChoice {
    pub const fn dsp(self) -> Shape {
        match self {
            Self::Sine => Shape::Sine,
            Self::Triangle => Shape::Triangle,
            Self::Random => Shape::Random,
        }
    }
}

/// **Time's tempo sync** (`plans/plan-tempo-sync-controls.md`): 1/64 to a whole note on the
/// collection's one ladder, the top the longest. This plugin's own fourteen-step table was exactly this
/// span, so every stored Time position keeps its division. What a tempo cannot reach inside the
/// public delay range is clamped, never rescaled.
pub const TIME_SYNC: mxm_tempo::Ladder = mxm_tempo::Ladder::new(
    mxm_tempo::Span::new(mxm_tempo::Division::SixtyFourth, mxm_tempo::Division::Whole),
    mxm_tempo::Direction::Time,
);

/// **Motion Rate's tempo sync**: every LFO's ladder, the top the fastest.
pub const RATE_SYNC: mxm_tempo::Ladder =
    mxm_tempo::Ladder::new(mxm_tempo::Span::LFO, mxm_tempo::Direction::Rate);

/// The public delay range, in the ladder's unit.
const DELAY_BOUNDS: (f64, f64) = (MIN_DELAY_S as f64, MAX_DELAY_S as f64);

pub fn time_range() -> FloatRange {
    FloatRange::Skewed {
        min: MIN_DELAY_S,
        max: MAX_DELAY_S,
        factor: FloatRange::skew_factor(-2.0),
    }
}

type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

fn percent_to_string() -> ValueToString {
    Arc::new(|value| format!("{:.0} %", value * 100.0))
}

fn string_to_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value / 100.0)
    })
}

fn bipolar_percent_to_string() -> ValueToString {
    Arc::new(|value| {
        let percent = value * 100.0;
        if percent.round() == 0.0 {
            "0 %".to_owned()
        } else {
            format!("{percent:+.0} %")
        }
    })
}

fn string_to_bipolar_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value / 100.0)
    })
}

fn time_to_string() -> ValueToString {
    Arc::new(|seconds| {
        let milliseconds = seconds * 1_000.0;
        if milliseconds.round() >= 1_000.0 {
            format!("{seconds:.2} s")
        } else {
            format!("{milliseconds:.0} ms")
        }
    })
}

fn string_to_time() -> StringToValue {
    Arc::new(|text| {
        let lower = text.trim().to_ascii_lowercase();
        if let Some(value) = lower.strip_suffix("ms") {
            value.trim().parse::<f32>().ok().map(|v| v / 1_000.0)
        } else {
            lower.trim_end_matches('s').trim().parse::<f32>().ok()
        }
    })
}

fn frequency_to_string() -> ValueToString {
    Arc::new(|value| {
        if value.round() >= 1_000.0 {
            format!("{:.2} kHz", value / 1_000.0)
        } else {
            format!("{value:.0} Hz")
        }
    })
}

fn string_to_frequency() -> StringToValue {
    Arc::new(|text| {
        let lower = text.trim().to_ascii_lowercase();
        if let Some(value) = lower.strip_suffix("khz") {
            value.trim().parse::<f32>().ok().map(|v| v * 1_000.0)
        } else {
            lower.trim_end_matches("hz").trim().parse::<f32>().ok()
        }
    })
}

fn rate_to_string() -> ValueToString {
    Arc::new(|value| {
        if (value * 100.0).round() >= 100.0 {
            format!("{value:.2} Hz")
        } else {
            format!("{value:.3} Hz")
        }
    })
}

fn string_to_rate() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .to_ascii_lowercase()
            .trim_end_matches("hz")
            .trim()
            .parse::<f32>()
            .ok()
    })
}

#[derive(Params)]
pub struct MxmFxDelayParams {
    #[id = "time"]
    pub time: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "change"]
    pub change: EnumParam<ChangeChoice>,
    #[id = "routing"]
    pub routing: EnumParam<RoutingChoice>,
    #[id = "offset"]
    pub offset: FloatParam,
    #[id = "feedback"]
    pub feedback: FloatParam,
    #[id = "lowcut"]
    pub low_cut: FloatParam,
    #[id = "highcut"]
    pub high_cut: FloatParam,
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "freeze"]
    pub freeze: BoolParam,
    #[id = "model"]
    pub model: EnumParam<ModelChoice>,
    #[id = "character"]
    pub character: FloatParam,
    #[id = "motion"]
    pub motion: FloatParam,
    #[id = "rate"]
    pub rate: FloatParam,
    /// Motion Rate's tempo sync: its position picks a division of the host's tempo.
    #[id = "ratesync"]
    pub rate_sync: BoolParam,
    #[id = "shape"]
    pub shape: EnumParam<ShapeChoice>,
    #[id = "duck"]
    pub duck: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,

    #[persist = "preset"]
    pub preset: RwLock<PresetIdentity>,
}

impl Default for MxmFxDelayParams {
    fn default() -> Self {
        let percent = |name, default| {
            FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(percent_to_string())
                .with_string_to_value(string_to_percent())
        };
        Self {
            time: FloatParam::new("Time", DEFAULT_TIME_S, time_range())
                .with_value_to_string(time_to_string())
                .with_string_to_value(string_to_time()),
            // Read On/Off like every tempo sync in the collection; the old words still parse.
            sync: BoolParam::new("Time sync", false).with_string_to_value(Arc::new(
                |text| match text.trim().to_ascii_lowercase().as_str() {
                    "sync" | "on" | "true" => Some(true),
                    "free" | "off" | "false" => Some(false),
                    _ => None,
                },
            )),
            change: EnumParam::new("Change", ChangeChoice::Fade),
            routing: EnumParam::new("Routing", RoutingChoice::Standard),
            offset: FloatParam::new(
                "Offset",
                0.0,
                FloatRange::SymmetricalSkewed {
                    min: -1.0,
                    max: 1.0,
                    factor: FloatRange::skew_factor(-0.5),
                    center: 0.0,
                },
            )
            .with_value_to_string(bipolar_percent_to_string())
            .with_string_to_value(string_to_bipolar_percent()),
            feedback: percent("Feedback", 0.42),
            low_cut: FloatParam::new(
                "Low cut",
                40.0,
                FloatRange::Skewed {
                    min: 10.0,
                    max: 8_000.0,
                    factor: FloatRange::skew_factor(-1.6),
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(frequency_to_string())
            .with_string_to_value(string_to_frequency()),
            high_cut: FloatParam::new(
                "High cut",
                12_000.0,
                FloatRange::Skewed {
                    min: 200.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(frequency_to_string())
            .with_string_to_value(string_to_frequency()),
            drive: percent("Drive", 0.12),
            freeze: BoolParam::new("Freeze", false),
            model: EnumParam::new("Model", ModelChoice::Clean),
            // Character moves playback bandwidth, record gain, saturation and quantisation depth
            // on every sample, so it is a signal like Feedback or Drive and smooths like one. Only
            // Time and Offset (commands routed through the Change law) and Rate (configuration that
            // preserves source phase) are deliberately unsmoothed.
            character: percent("Character", 0.35),
            motion: percent("Motion", 0.0),
            rate: FloatParam::new(
                "Rate",
                0.25,
                FloatRange::Skewed {
                    min: 0.01,
                    max: 20.0,
                    factor: FloatRange::skew_factor(-1.8),
                },
            )
            .with_value_to_string(rate_to_string())
            .with_string_to_value(string_to_rate()),
            rate_sync: BoolParam::new("Rate sync", false),
            shape: EnumParam::new("Shape", ShapeChoice::Sine),
            duck: percent("Duck", 0.12),
            mix: percent("Mix", DEFAULT_MIX),
            preset: RwLock::new(PresetIdentity::none()),
        }
    }
}

impl MxmFxDelayParams {
    /// The division Time's position picks at `tempo`, clamped into the public delay range.
    pub fn division_at(&self, tempo: f64) -> mxm_tempo::Division {
        let (lo, hi) = DELAY_BOUNDS;
        TIME_SYNC.division(self.time_position(), tempo, lo, hi)
    }

    /// The delay Time asks for: its division synced to a usable tempo, and the free knob otherwise.
    pub fn target_time(&self, tempo: Option<f64>) -> f32 {
        let (lo, hi) = DELAY_BOUNDS;
        TIME_SYNC
            .resolve(self.sync.value(), tempo, self.time_position(), lo, hi)
            .map_or_else(
                || self.time.value().clamp(MIN_DELAY_S, MAX_DELAY_S),
                |seconds| seconds as f32,
            )
    }

    /// Motion Rate while its sync follows the host, or `None` for its free rate: the modulated
    /// position picks a division on the LFO ladder. Resolved once a block by the plugin.
    pub fn synced_rate(&self, tempo: Option<f64>) -> Option<f32> {
        let rate = &self.rate;
        RATE_SYNC
            .resolve(
                self.rate_sync.value(),
                tempo,
                rate.modulated_normalized_value(),
                f64::from(rate.preview_plain(0.0)),
                f64::from(rate.preview_plain(1.0)),
            )
            .map(|hz| hz as f32)
    }

    /// Time's **modulated** position across its travel, which is what picks a division: a host's
    /// modulation picks the division a moved knob would (the editor reads the unmodulated one).
    fn time_position(&self) -> f32 {
        use nice_plug::prelude::Param as _;
        self.time.modulated_normalized_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_engaged_complete_and_not_frozen() {
        let params = MxmFxDelayParams::default();
        assert!(params.mix.value() > 0.0);
        assert!(params.feedback.value() > 0.0);
        assert!(!params.freeze.value());
        assert_eq!(params.model.value(), ModelChoice::Clean);
    }

    #[test]
    fn sync_selection_follows_knob_position_and_survives_tempo() {
        let params = MxmFxDelayParams::default();
        let at_120 = params.division_at(120.0);
        let at_90 = params.division_at(90.0);
        assert_eq!(at_120, at_90);
        assert_ne!(at_120.seconds(120.0), at_90.seconds(90.0));
    }

    #[test]
    fn unreachable_divisions_clamp_without_rescaling_the_middle() {
        let (lo, hi) = DELAY_BOUNDS;
        let position = time_range().normalize(DEFAULT_TIME_S);
        let at_ordinary = TIME_SYNC.division(position, 120.0, lo, hi);
        let at_fast = TIME_SYNC.division(position, 10_000.0, lo, hi);
        assert!(TIME_SYNC.span.contains(at_ordinary));
        assert!(at_fast.seconds(10_000.0) >= lo);
        assert!(TIME_SYNC.reachable(10_000.0, lo, hi).contains(at_fast));
    }

    #[test]
    fn a_slow_transport_is_followed_rather_than_floored_to_one_bpm() {
        use mxm_tempo::Division;
        // The contract accepts every finite positive tempo. A `tempo.max(1.0)` floor reported the
        // wrong time for slower transports instead of the one the host asked for.
        assert!((Division::SixtyFourth.seconds(0.5) - 7.5).abs() < 1.0e-4);
        assert!((Division::Quarter.seconds(30.0) - 2.0).abs() < 1.0e-4);
        assert!((Division::Whole.seconds(120.0) - 2.0).abs() < 1.0e-4);

        // Slower than about 0.469 BPM even a 1/64 exceeds eight seconds, so nothing fits and the
        // shortest division is the only sensible offer. That arm was unreachable behind the floor.
        let (lo, hi) = DELAY_BOUNDS;
        let window = TIME_SYNC.reachable(0.25, lo, hi);
        assert_eq!(window.divisions(), [Division::SixtyFourth]);
        assert!(Division::SixtyFourth.seconds(0.25) > hi);

        // Whatever Sync offers, the delivered time stays inside the public range.
        let params = MxmFxDelayParams::default();
        for tempo in [0.25, 0.5, 1.0, 30.0, 120.0, 10_000.0] {
            let seconds = params.target_time(Some(tempo));
            assert!(
                (MIN_DELAY_S..=MAX_DELAY_S).contains(&seconds),
                "{tempo} BPM produced {seconds} s"
            );
        }
    }

    /// **Motion Rate's sync picks a division and is inert without a tempo**
    /// (`plans/plan-tempo-sync-controls.md`): off, or with no tempo, the knob's own hertz stand; on
    /// at 120 bpm the ends are the LFO ladder's ends, the top the fastest.
    #[test]
    fn rate_sync_picks_a_division_and_is_inert_without_a_tempo() {
        use mxm_tempo::Division;
        use nice_plug::params::InternalParamMut;
        let params = MxmFxDelayParams::default();
        fn set<P: InternalParamMut>(param: &P, normalized: f32) {
            unsafe {
                let _ = param._internal_set_normalized_value(normalized);
            }
        }

        set(&params.rate, 1.0);
        assert_eq!(
            params.synced_rate(Some(120.0)),
            None,
            "off is the free rate"
        );
        set(&params.rate_sync, 1.0);
        assert_eq!(params.synced_rate(None), None, "no tempo is the free rate");

        let top = params.synced_rate(Some(120.0)).expect("synced at a tempo");
        set(&params.rate, 0.0);
        let bottom = params.synced_rate(Some(120.0)).expect("synced at a tempo");
        let (lo, hi) = (
            f64::from(params.rate.preview_plain(0.0)),
            f64::from(params.rate.preview_plain(1.0)),
        );
        assert!(
            top > bottom,
            "the top of a rate is the fastest: {bottom} to {top}"
        );
        let fastest = Division::ThirtySecond.hz(120.0).clamp(lo, hi) as f32;
        let slowest = Division::FourBars.hz(120.0).clamp(lo, hi) as f32;
        assert!((top - fastest).abs() < 1e-4, "{top} against {fastest}");
        assert!(
            (bottom - slowest).abs() < 1e-4,
            "{bottom} against {slowest}"
        );
    }

    /// **Host modulation picks the division**, as a moved knob would: the synced time follows the
    /// modulated position, not the one the knob was set to.
    #[test]
    fn host_modulation_of_time_picks_the_division() {
        use nice_plug::params::InternalParamMut;
        let params = MxmFxDelayParams::default();
        unsafe {
            let _ = params.sync._internal_set_normalized_value(1.0);
            let _ = params.time._internal_set_normalized_value(0.2);
        }
        let set_at = params.target_time(Some(120.0));
        unsafe {
            let _ = params.time._internal_modulate_value(0.6);
        }
        let (lo, hi) = DELAY_BOUNDS;
        let modulated = TIME_SYNC.division(0.8, 120.0, lo, hi).seconds(120.0) as f32;
        assert_eq!(params.target_time(Some(120.0)), modulated);
        assert!(modulated > set_at, "modulation up picked no longer delay");
    }

    /// **Time sync picks a division and is inert without a tempo**: off or tempo-less it is the free
    /// knob; on at 120 bpm its ends are the ladder's, 1/64 at the bottom and a whole note at the top.
    #[test]
    fn time_sync_picks_a_division_and_is_inert_without_a_tempo() {
        use mxm_tempo::Division;
        use nice_plug::params::InternalParamMut;
        let params = MxmFxDelayParams::default();
        fn set<P: InternalParamMut>(param: &P, normalized: f32) {
            unsafe {
                let _ = param._internal_set_normalized_value(normalized);
            }
        }
        let free = params.time.value();
        assert_eq!(
            params.target_time(Some(120.0)),
            free,
            "off is the free time"
        );
        set(&params.sync, 1.0);
        assert_eq!(params.target_time(None), free, "no tempo is the free time");

        set(&params.time, 0.0);
        let bottom = params.target_time(Some(120.0));
        set(&params.time, 1.0);
        let top = params.target_time(Some(120.0));
        assert!((f64::from(bottom) - Division::SixtyFourth.seconds(120.0)).abs() < 1e-6);
        assert!((f64::from(top) - Division::Whole.seconds(120.0)).abs() < 1e-6);
    }

    #[test]
    fn parameter_text_is_idempotent_through_the_host_conversion() {
        let params = MxmFxDelayParams::default();
        let continuous: [&FloatParam; 11] = [
            &params.time,
            &params.offset,
            &params.feedback,
            &params.low_cut,
            &params.high_cut,
            &params.drive,
            &params.character,
            &params.motion,
            &params.rate,
            &params.duck,
            &params.mix,
        ];
        for param in continuous {
            for step in 0..=1000 {
                let normalized = step as f32 / 1000.0;
                let first = param.normalized_value_to_string(normalized, true);
                let reparsed = param
                    .string_to_normalized_value(&first)
                    .unwrap_or_else(|| panic!("{} rejected {first:?}", param.name()));
                let second = param.normalized_value_to_string(reparsed, true);
                assert_eq!(first, second, "{} at {normalized}", param.name());
            }
        }
    }
}
