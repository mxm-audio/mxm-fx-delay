//! `mxm-fx-delay` — the collection's general-purpose clean, vintage-digital and tape delay.

macro_rules! plugin_name {
    () => {
        "mxm-fx-delay"
    };
}

pub const NAME: &str = plugin_name!();
pub const CLAP_ID: &str = concat!("dk.mxm.", plugin_name!());

pub mod editor;
pub mod params;
pub mod preset;
pub mod preset_designs;
pub mod telemetry;

use std::sync::Arc;

use mxm_fx_delay_dsp::{Controls, Engine, routed_times};
use nice_plug::prelude::*;
use params::MxmFxDelayParams;
use telemetry::Telemetry;

pub struct MxmFxDelay {
    pub params: Arc<MxmFxDelayParams>,
    telemetry: Arc<Telemetry>,
    engine: Engine,
    sample_rate: f32,
    input_channels: usize,
    /// Motion Rate as its sync resolved it for this block, or `None` for the free rate.
    synced_rate_hz: Option<f32>,
}

impl Default for MxmFxDelay {
    fn default() -> Self {
        Self {
            params: Arc::new(MxmFxDelayParams::default()),
            telemetry: Telemetry::shared(),
            engine: Engine::new(48_000.0).expect("48 kHz is supported"),
            sample_rate: 48_000.0,
            input_channels: 1,
            synced_rate_hz: None,
        }
    }
}

impl MxmFxDelay {
    fn target_controls(&self, tempo: Option<f64>) -> Controls {
        let params = &self.params;
        Controls {
            time_s: params.target_time(tempo),
            change: params.change.value().dsp(),
            routing: params.routing.value().dsp(),
            offset: params.offset.value(),
            feedback: params.feedback.value(),
            low_cut_hz: params.low_cut.value(),
            high_cut_hz: params.high_cut.value(),
            drive: params.drive.value(),
            freeze: params.freeze.value(),
            model: params.model.value().dsp(),
            character: params.character.value(),
            motion: params.motion.value(),
            rate_hz: self.rate_hz(),
            shape: params.shape.value().dsp(),
            duck: params.duck.value(),
            mix: params.mix.value(),
        }
    }

    fn controls(&self, tempo: Option<f64>) -> Controls {
        let params = &self.params;
        Controls {
            time_s: params.target_time(tempo),
            change: params.change.value().dsp(),
            routing: params.routing.value().dsp(),
            offset: params.offset.value(),
            feedback: params.feedback.smoothed.next(),
            low_cut_hz: params.low_cut.smoothed.next(),
            high_cut_hz: params.high_cut.smoothed.next(),
            drive: params.drive.smoothed.next(),
            freeze: params.freeze.value(),
            model: params.model.value().dsp(),
            character: params.character.smoothed.next(),
            motion: params.motion.smoothed.next(),
            rate_hz: self.rate_hz(),
            shape: params.shape.value().dsp(),
            duck: params.duck.smoothed.next(),
            mix: params.mix.smoothed.next(),
        }
    }

    /// Motion Rate in force: its division while synced to a tempo, the knob otherwise. Rate is
    /// unsmoothed either way — it is configuration that preserves the source's phase.
    fn rate_hz(&self) -> f32 {
        self.synced_rate_hz
            .unwrap_or_else(|| self.params.rate.value())
    }

    /// Advances every smoothed parameter without processing audio.
    ///
    /// A parked block returns before [`Self::controls`], so without this the smoothers stop
    /// tracking wall-clock while the engine sleeps. A Mix moved to exactly zero during silence
    /// would then still be mid-ramp when audio returns, and the first block would attenuate dry and
    /// run wet processing even though the parameter is Off — which is exactly what the bit-exact
    /// dry contract forbids.
    fn advance_smoothers(&self, steps: u32) {
        if steps == 0 {
            return;
        }
        let params = &self.params;
        let _ = params.feedback.smoothed.next_step(steps);
        let _ = params.low_cut.smoothed.next_step(steps);
        let _ = params.high_cut.smoothed.next_step(steps);
        let _ = params.drive.smoothed.next_step(steps);
        let _ = params.character.smoothed.next_step(steps);
        let _ = params.motion.smoothed.next_step(steps);
        let _ = params.duck.smoothed.next_step(steps);
        let _ = params.mix.smoothed.next_step(steps);
    }

    fn prepare(&mut self, sample_rate: f32, input_channels: usize) -> bool {
        // Forget the last activation's tempo too: nice-plug resets right after activating, and a
        // division resolved from a tempo the host may since have changed would seed the engine.
        self.telemetry.forget_tempo();
        // A restored state is resolved afresh by the next block: activation must not seed the
        // engine with the division the previous state was synced to.
        self.synced_rate_hz = None;
        let Ok(mut engine) = Engine::new(sample_rate) else {
            return false;
        };
        self.sample_rate = sample_rate;
        self.input_channels = input_channels.clamp(1, 2);
        engine.reset(self.target_controls(None));
        self.engine = engine;
        true
    }

    pub fn prepare_for_test(&mut self, sample_rate: f32, input_channels: usize) -> bool {
        self.prepare(sample_rate, input_channels)
    }

    pub fn process_block_for_test(
        &mut self,
        channels: &mut [&mut [f32]],
        tempo: Option<f64>,
    ) -> ProcessStatus {
        self.process_block(channels, tempo)
    }

    fn process_block(&mut self, channels: &mut [&mut [f32]], tempo: Option<f64>) -> ProcessStatus {
        let Some(first) = channels.first() else {
            return ProcessStatus::Normal;
        };
        let samples = first.len();
        let mono = self.input_channels == 1 || channels.len() < 2;
        let inputs = self.input_channels.min(channels.len());
        let mut has_input = false;
        let mut input_peak = 0.0f32;
        for channel in &mut channels[..inputs] {
            for sample in &mut channel[..samples] {
                if !sample.is_finite() || sample.abs() < f32::MIN_POSITIVE {
                    *sample = 0.0;
                } else {
                    has_input = true;
                    input_peak = input_peak.max(sample.abs());
                }
            }
        }

        // Motion Rate's sync, once a block (`plans/plan-tempo-sync-controls.md`).
        self.synced_rate_hz = self.params.synced_rate(tempo);
        let target = self.target_controls(tempo);
        // While parked there is no audio to protect, so an exact Off takes effect at once instead
        // of crossfading into it. The crossfade exists to keep a live Mix move from clicking, and a
        // parked engine has nothing playing to click. Without this a host that sets Mix to Off while
        // asleep and resumes straight into audio — with no silent callback in between, so
        // `advance_smoothers` below never runs — still hears the first 20 ms attenuated.
        if self.engine.is_parked() && self.params.mix.value() == 0.0 {
            self.params.mix.smoothed.reset(0.0);
        }
        if !has_input && self.engine.is_parked() {
            if mono && channels.len() > 1 {
                let (left, rest) = channels.split_at_mut(1);
                rest[0][..samples].copy_from_slice(&left[0][..samples]);
            }
            self.advance_smoothers(samples as u32);
            // Publish the routed pair, not one time twice: Dual prints separate L/R times, and an
            // idle editor is exactly when someone is setting Offset and reading them.
            self.telemetry.publish(
                0.0,
                0.0,
                0.0,
                tempo,
                routed_times(target.time_s, target.offset, target.routing),
                0.0,
                false,
            );
            return ProcessStatus::Normal;
        }

        let mut output_peak = 0.0f32;
        let mut wet_peak = 0.0f32;
        for index in 0..samples {
            let controls = self.controls(tempo);
            let dry_l = channels[0][index];
            let dry_r = if mono { dry_l } else { channels[1][index] };
            let output = self.engine.process([dry_l, dry_r], mono, controls);
            let dry_gain = 1.0 - controls.mix;
            wet_peak = wet_peak
                .max((output[0] - dry_gain * dry_l).abs())
                .max((output[1] - dry_gain * dry_r).abs());
            output_peak = output_peak.max(output[0].abs()).max(output[1].abs());
            channels[0][index] = output[0];
            if channels.len() > 1 {
                channels[1][index] = output[1];
            }
        }
        if channels.len() > 2 {
            let (left, rest) = channels.split_at_mut(1);
            for channel in rest.iter_mut().skip(1) {
                channel[..samples].copy_from_slice(&left[0][..samples]);
            }
        }

        self.telemetry.publish(
            input_peak,
            wet_peak,
            output_peak,
            tempo,
            self.engine.effective_times(),
            self.engine.activity(),
            self.engine.freeze_is_held(),
        );

        let target = self.target_controls(tempo);
        if has_input || self.engine.is_parked() || self.engine.is_sustaining(target) {
            ProcessStatus::Normal
        } else {
            let seconds = self.engine.remaining_tail_seconds(target).unwrap_or(0.0);
            ProcessStatus::Tail(
                (seconds * self.sample_rate)
                    .clamp(0.0, u32::MAX as f32)
                    .ceil() as u32,
            )
        }
    }
}

impl Plugin for MxmFxDelay {
    const NAME: &'static str = NAME;
    const VENDOR: &'static str = "mxm";
    const URL: &'static str = "https://mxm.dk";
    const EMAIL: &'static str = "plugins@mxm.dk";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type Editor = editor::MxmFxDelayEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(self.params.clone(), self.telemetry.clone())
    }

    fn activate(
        &mut self,
        layout: &AudioIOLayout,
        config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.prepare(
            config.sample_rate,
            layout
                .main_input_channels
                .map_or(1, |channels| channels.get() as usize),
        )
    }

    fn reset(&mut self) {
        // A host resets without a callback between (a bypass, a transport restart), and a parameter
        // flush may have moved a sync meanwhile: re-resolve every sync from the parameters as they
        // stand and the last tempo seen, so nothing is seeded from the previous division.
        let tempo = self.telemetry.tempo();
        self.synced_rate_hz = self.params.synced_rate(tempo);
        self.engine.reset(self.target_controls(tempo));
    }

    /// **A project saved before the tempo syncs** restores each Off rather than keeping this
    /// instance's, and a loaded preset's baseline gains it, so the preset stays clean
    /// (`mxm_preset::add_switches_off`).
    fn filter_state(state: &mut PluginState) {
        mxm_preset::add_switches_off(state, crate::preset::TEMPO_SYNC_IDS);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.process_block(buffer.as_slice(), context.transport().tempo)
    }
}

impl ClapPlugin for MxmFxDelay {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A delay with clean, vintage digital and tape characters");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Delay,
        ClapFeature::Stereo,
    ];
}

nice_export_clap!(MxmFxDelay);

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::params::{InternalParamMut, Param};

    fn update_smoothers(plugin: &MxmFxDelay) {
        for param in [
            &plugin.params.feedback,
            &plugin.params.low_cut,
            &plugin.params.high_cut,
            &plugin.params.drive,
            &plugin.params.character,
            &plugin.params.motion,
            &plugin.params.duck,
            &plugin.params.mix,
        ] {
            unsafe { param._internal_update_smoother(48_000.0, true) };
        }
    }

    fn set(param: &FloatParam, value: f32) {
        unsafe {
            let _ = param._internal_set_normalized_value(param.preview_normalized(value));
            param._internal_update_smoother(48_000.0, true);
        }
    }

    /// Move a parameter the way a host automating it does: retarget the smoother without resetting
    /// it, so the ramp is observable. [`set`] snaps instead, which hides every smoothing defect.
    fn automate(param: &FloatParam, value: f32) {
        unsafe {
            let _ = param._internal_set_normalized_value(param.preview_normalized(value));
            param._internal_update_smoother(48_000.0, false);
        }
    }

    fn set_routing(param: &EnumParam<params::RoutingChoice>, value: params::RoutingChoice) {
        unsafe {
            let _ = param._internal_set_normalized_value(param.preview_normalized(value));
        }
    }

    #[test]
    fn character_is_smoothed_like_every_other_continuous_control() {
        // Character moves playback bandwidth, record gain, saturation and quantisation depth every
        // sample. Read unsmoothed, a host automation step changes the whole audio path at once.
        let plugin = MxmFxDelay::default();
        update_smoothers(&plugin);
        set(&plugin.params.character, 0.0);

        // Read the settled value *before* automating: an unsmoothed parameter has already jumped by
        // the first read after the move, so a loop that starts there measures nothing.
        let mut previous = plugin.controls(None).character;
        assert_eq!(previous, 0.0);
        automate(&plugin.params.character, 1.0);

        let mut largest_step = 0.0f32;
        for _ in 0..4_000 {
            let value = plugin.controls(None).character;
            largest_step = largest_step.max((value - previous).abs());
            previous = value;
        }
        assert!(largest_step < 0.01, "Character stepped by {largest_step}");
        assert!(
            (previous - 1.0).abs() < 1.0e-6,
            "Character never reached its target: {previous}"
        );
    }

    #[test]
    fn mix_zeroed_while_parked_is_bit_exact_dry_on_the_next_audio() {
        // A parked block returns before the per-sample control read, so a Mix moved to Off during
        // silence can still be mid-ramp when audio returns, attenuating dry and running wet against
        // the bit-exact dry contract. `silent_blocks == 0` is the case a sleeping host produces: it
        // updates Mix and resumes straight into audio, so nothing ever ticks the smoother in
        // between and advancing the smoothers on the parked path cannot help.
        for silent_blocks in [0, 1, 4] {
            let mut plugin = MxmFxDelay::default();
            update_smoothers(&plugin);
            set(&plugin.params.mix, 0.35);
            assert!(plugin.prepare(48_000.0, 1));

            let mut left = vec![0.0f32; 512];
            let mut right = vec![0.0f32; 512];
            {
                let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
                plugin.process_block(&mut channels, None);
            }
            automate(&plugin.params.mix, 0.0);
            for _ in 0..silent_blocks {
                let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
                plugin.process_block(&mut channels, None);
            }

            let source: Vec<f32> = (0..512).map(|n| (n as f32 * 0.19).sin() * 0.5).collect();
            left.copy_from_slice(&source);
            right.copy_from_slice(&source);
            let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
            plugin.process_block(&mut channels, None);
            assert_eq!(
                left, source,
                "Mix was Off but the dry path was not bit-exact after {silent_blocks} silent blocks"
            );
        }
    }

    #[test]
    fn parked_telemetry_publishes_the_routed_pair_not_one_time_twice() {
        // An idle editor is exactly when Offset is being set and the L/R times read. Both signs
        // matter: Offset selects which side leads, so one sign passing proves nothing about the
        // other.
        for (offset, expect_left_leads) in [(0.6f32, true), (-0.6f32, false)] {
            let mut plugin = MxmFxDelay::default();
            update_smoothers(&plugin);
            set(&plugin.params.mix, 1.0);
            set(&plugin.params.time, 0.2);
            set(&plugin.params.offset, offset);
            set_routing(&plugin.params.routing, params::RoutingChoice::Dual);
            assert!(plugin.prepare(48_000.0, 1));

            let mut left = vec![0.0f32; 256];
            let mut right = vec![0.0f32; 256];
            let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
            plugin.process_block(&mut channels, None);

            let times = plugin.telemetry.times();
            assert!(
                (times[0] - times[1]).abs() > 1.0e-4,
                "Offset {offset} reported equal L/R times while parked: {times:?}"
            );
            assert_eq!(
                times[0] < times[1],
                expect_left_leads,
                "Offset {offset} led the wrong side: {times:?}"
            );
        }
    }

    #[test]
    fn permanent_identity_and_layouts_are_fixed() {
        assert_eq!(NAME, "mxm-fx-delay");
        assert_eq!(CLAP_ID, "dk.mxm.mxm-fx-delay");
        mxm_plugin_test::bundle::is_named(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"), NAME);
        assert_eq!(MxmFxDelay::AUDIO_IO_LAYOUTS.len(), 2);
        assert_eq!(MxmFxDelay::MIDI_INPUT, MidiConfig::None);
    }

    #[test]
    fn mix_zero_is_dry_to_the_bit_in_both_layouts() {
        for inputs in [1, 2] {
            let mut plugin = MxmFxDelay::default();
            update_smoothers(&plugin);
            set(&plugin.params.mix, 0.0);
            assert!(plugin.prepare(48_000.0, inputs));
            let source_l: Vec<f32> = (0..4096).map(|n| (n as f32 * 0.13).sin() * 0.4).collect();
            let source_r: Vec<f32> = (0..4096).map(|n| (n as f32 * 0.07).cos() * 0.3).collect();
            let mut left = source_l.clone();
            let mut right = source_r.clone();
            let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
            plugin.process_block(&mut channels, None);
            assert_eq!(left, source_l);
            assert_eq!(right, if inputs == 1 { source_l } else { source_r });
        }
    }

    /// **Activation does not seed the engine with a previous state's division**: a host restoring an
    /// unsynced state reactivates the same object, and the rate it starts at is the knob's.
    #[test]
    fn activation_forgets_the_previous_states_synced_rate() {
        let mut plugin = MxmFxDelay::default();
        update_smoothers(&plugin);
        plugin.synced_rate_hz = Some(9.0);
        assert!(plugin.prepare(48_000.0, 1));
        assert_eq!(
            plugin.target_controls(None).rate_hz,
            plugin.params.rate.value()
        );
    }

    #[test]
    fn no_tempo_falls_back_to_free_time_and_a_tail_is_reported() {
        let mut plugin = MxmFxDelay::default();
        update_smoothers(&plugin);
        set(&plugin.params.mix, 1.0);
        set(&plugin.params.time, 0.01);
        assert!(plugin.prepare(48_000.0, 1));
        let mut left = vec![0.0; 2048];
        let mut right = vec![0.0; 2048];
        left[0] = 0.8;
        {
            let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
            let status = plugin.process_block(&mut channels, None);
            assert!(matches!(status, ProcessStatus::Normal));
        }
        left.fill(0.0);
        right.fill(0.0);
        let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
        let status = plugin.process_block(&mut channels, None);
        assert!(matches!(status, ProcessStatus::Tail(_)));
        assert_eq!(plugin.params.target_time(None), plugin.params.time.value());
    }
}

/// **A reset re-resolves the tempo syncs**: a sync turned off while the host held the effect
/// unprocessed does not seed the reset from the previous division, and one still on stays on it.
#[cfg(test)]
mod reset_resolves_the_syncs {
    use super::*;
    use nice_plug::params::InternalParamMut;

    #[test]
    fn a_reset_re_resolves_the_tempo_syncs() {
        let mut plugin = MxmFxDelay {
            synced_rate_hz: Some(9.0),
            ..Default::default()
        };
        plugin.reset();
        assert_eq!(
            plugin.target_controls(None).rate_hz,
            plugin.params.rate.value()
        );
        plugin
            .telemetry
            .publish(0.0, 0.0, 0.0, Some(120.0), [0.0; 2], 0.0, false);
        unsafe {
            let _ = plugin.params.rate_sync._internal_set_normalized_value(1.0);
        }
        plugin.reset();
        assert_eq!(
            Some(plugin.target_controls(None).rate_hz),
            plugin.params.synced_rate(Some(120.0))
        );
        // And Time: synced, the engine is reset at its division at the last tempo.
        unsafe {
            let _ = plugin.params.sync._internal_set_normalized_value(1.0);
        }
        plugin.reset();
        assert_eq!(
            plugin.engine.effective_times()[0],
            plugin.params.target_time(Some(120.0))
        );
    }

    /// **Reactivation forgets the old tempo**: nice-plug resets right after activating, and that
    /// reset must not resolve from the tempo the host reported before it was deactivated — the first
    /// callback's tempo is the first one used.
    #[test]
    fn reactivation_forgets_the_previous_tempo() {
        let mut plugin = MxmFxDelay::default();
        plugin
            .telemetry
            .publish(0.0, 0.0, 0.0, Some(120.0), [0.0; 2], 0.0, false);
        unsafe {
            let _ = plugin.params.rate_sync._internal_set_normalized_value(1.0);
        }
        assert!(plugin.prepare(48_000.0, 1));
        plugin.reset();
        assert_eq!(plugin.synced_rate_hz, None);
    }
}

/// What a player reads — on hover in the editor, and in a host's plugin browser — speaks to the
/// player about the sound, never about the machine or the code (`mxm_plugin_test::hover_text`).
#[cfg(test)]
mod speaks_to_the_player {
    #[test]
    fn hover_text() {
        mxm_plugin_test::hover_text::speaks_to_the_player(env!("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn host_description() {
        mxm_plugin_test::hover_text::host_description_speaks_to_the_player(env!(
            "CARGO_MANIFEST_DIR"
        ));
    }
}
