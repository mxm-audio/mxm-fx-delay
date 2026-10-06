//! Every factory recipe rendered against declared programme material.

use mxm_fx_delay::params::MxmFxDelayParams;
use mxm_fx_delay::{MxmFxDelay, preset};
use nice_plug::params::{InternalParamMut, Param};
use nice_plug::prelude::*;

const FS: f32 = 48_000.0;

struct ApplyingHost;

impl nice_plug::context::gui::GuiContextInner for ApplyingHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(
        &self,
        param: nice_plug::params::internals::ParamPtr,
        value: f32,
    ) {
        unsafe {
            let _ = param._internal_set_normalized_value(value);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _: PluginState) {}
}

fn settle_smoothers(params: &MxmFxDelayParams) {
    for param in [
        &params.feedback,
        &params.low_cut,
        &params.high_cut,
        &params.drive,
        &params.character,
        &params.motion,
        &params.duck,
        &params.mix,
    ] {
        unsafe { param._internal_update_smoother(FS, true) };
    }
}

fn load(text: &str, mix: f32) -> MxmFxDelay {
    let mut plugin = MxmFxDelay::default();
    unsafe {
        let _ = plugin
            .params
            .mix
            ._internal_set_normalized_value(plugin.params.mix.preview_normalized(mix));
    }
    let parsed = preset::Preset::parse(text, mxm_fx_delay::CLAP_ID).expect("factory JSON");
    let host = ApplyingHost;
    let setter = ParamSetter::new(&host);
    let (writes, problems) = parsed.resolve_from(&*plugin.params, Some(&preset::Origin::Factory));
    assert!(problems.is_empty(), "{}: {problems:?}", parsed.name);
    for (_, parameter, value) in writes {
        parameter.set(&setter, value);
    }
    assert!((plugin.params.mix.value() - mix).abs() < 1e-6);
    assert!(!plugin.params.freeze.value());
    settle_smoothers(&plugin.params);
    assert!(plugin.prepare_for_test(FS, 1));
    plugin
}

fn programme() -> Vec<f32> {
    let frames = FS as usize;
    let normaliser: f32 = (1..=10).map(|harmonic| 1.0 / harmonic as f32).sum();
    let mut input: Vec<f32> = (0..frames)
        .map(|sample| {
            let time = sample as f32 / FS;
            let wave: f32 = (1..=10)
                .map(|harmonic| {
                    (core::f32::consts::TAU * 180.0 * harmonic as f32 * time).sin()
                        / harmonic as f32
                })
                .sum();
            // The bank's declared clip audit is a harmonic note at -6 dBFS.
            0.5 * wave / normaliser
        })
        .collect();
    input.resize(frames + FS as usize * 2, 0.0);
    input
}

fn render(plugin: &mut MxmFxDelay, input: &[f32]) -> Vec<f32> {
    let mut output = Vec::with_capacity(input.len() * 2);
    for block in input.chunks(256) {
        let mut left = block.to_vec();
        let mut right = vec![0.0; block.len()];
        {
            let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
            let _ = plugin.process_block_for_test(&mut channels, Some(120.0));
        }
        for (&left, &right) in left.iter().zip(&right) {
            output.extend_from_slice(&[left, right]);
        }
    }
    output
}

#[test]
fn a_factory_recipe_identity_baseline_includes_the_preserved_live_mix() {
    let (_, text) = preset::FACTORY_FILES[0];
    let plugin = load(text, 0.63);
    let parsed = preset::Preset::parse(text, mxm_fx_delay::CLAP_ID).unwrap();
    preset::mark_loaded(&*plugin.params, &parsed.name, preset::Origin::Factory);
    assert!(!preset::loaded(&*plugin.params).is_modified());
    unsafe {
        let _ = plugin
            .params
            .mix
            ._internal_set_normalized_value(plugin.params.mix.preview_normalized(0.2));
    }
    assert!(preset::loaded(&*plugin.params).is_modified());
}

#[test]
fn all_fifty_recipes_preserve_mix_disengage_freeze_and_render_audibly() {
    let input = programme();
    for (slug, text) in preset::FACTORY_FILES {
        let mut plugin = load(text, 0.63);
        let wet = render(&mut plugin, &input);
        assert!(wet.iter().all(|sample| sample.is_finite()), "{slug}");
        let peak = wet
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            peak < 1.0,
            "{slug} clips declared programme material at {peak}"
        );

        let mut dry_plugin = load(text, 0.0);
        let dry = render(&mut dry_plugin, &input);
        let difference = wet
            .iter()
            .zip(&dry)
            .fold(0.0f32, |peak, (wet, dry)| peak.max((wet - dry).abs()));
        assert!(
            difference > 0.005,
            "{slug} is inaudible: difference {difference}"
        );
    }
}
