use mxm_fx_delay::MxmFxDelay;
use mxm_fx_delay::params::{ChangeChoice, ModelChoice, RoutingChoice, ShapeChoice};
use mxm_fx_delay_dsp::{Change, Controls, Engine, Model, Routing, Shape};
use nice_plug::params::{InternalParamMut, Param};
use nice_plug::prelude::{EnumParam, FloatParam};

fn set_float(param: &FloatParam, value: f32) {
    unsafe {
        let _ = param._internal_set_normalized_value(param.preview_normalized(value));
        param._internal_update_smoother(48_000.0, true);
    }
}

fn set_enum<E: nice_plug::params::enums::Enum + PartialEq>(param: &EnumParam<E>, value: E) {
    unsafe {
        let _ = param._internal_set_normalized_value(param.preview_normalized(value));
    }
}

fn smooth(plugin: &MxmFxDelay) {
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

#[test]
fn every_topology_and_change_law_survives_live_retargeting() {
    for model in [
        ModelChoice::Clean,
        ModelChoice::VintageDigital,
        ModelChoice::Tape,
    ] {
        for routing in [
            RoutingChoice::Standard,
            RoutingChoice::Dual,
            RoutingChoice::PingPong,
        ] {
            for change in [
                ChangeChoice::Repitch,
                ChangeChoice::Fade,
                ChangeChoice::Jump,
            ] {
                let mut plugin = MxmFxDelay::default();
                smooth(&plugin);
                set_float(&plugin.params.mix, 1.0);
                set_float(&plugin.params.feedback, 0.88);
                set_float(&plugin.params.motion, 0.9);
                set_float(&plugin.params.offset, 0.92);
                set_enum(&plugin.params.model, model);
                set_enum(&plugin.params.routing, routing);
                set_enum(&plugin.params.change, change);
                assert!(plugin.prepare_for_test(48_000.0, 2));

                let mut left = vec![0.0; 256];
                let mut right = vec![0.0; 256];
                left[0] = 0.8;
                right[0] = -0.5;
                let mut maximum_step = 0.0f32;
                let mut previous = [0.0f32; 2];
                for block in 0..240 {
                    if block > 0 {
                        left.fill(0.0);
                        right.fill(0.0);
                    }
                    set_float(
                        &plugin.params.time,
                        if block % 2 == 0 { 0.006 } else { 5.5 },
                    );
                    if block % 3 == 0 {
                        set_enum(
                            &plugin.params.shape,
                            match (block / 3) % 3 {
                                0 => ShapeChoice::Sine,
                                1 => ShapeChoice::Triangle,
                                _ => ShapeChoice::Random,
                            },
                        );
                    }
                    let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
                    let _ = plugin.process_block_for_test(&mut channels, Some(121.0));
                    for index in 0..left.len() {
                        assert!(left[index].is_finite() && right[index].is_finite());
                        maximum_step = maximum_step
                            .max((left[index] - previous[0]).abs())
                            .max((right[index] - previous[1]).abs());
                        previous = [left[index], right[index]];
                    }
                }
                assert!(
                    maximum_step < 2.5,
                    "{model:?} {routing:?} {change:?}: {maximum_step}"
                );
            }
        }
    }
}

#[test]
fn supported_rates_and_block_boundaries_stay_finite_and_bounded() {
    for rate in [
        1_000.0, 8_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0, 384_000.0, 768_000.0,
    ] {
        for frames in [1, 7, 63, 256, 1024, 8192] {
            let mut plugin = MxmFxDelay::default();
            smooth(&plugin);
            set_float(&plugin.params.feedback, 0.99);
            set_float(&plugin.params.drive, 1.0);
            assert!(plugin.prepare_for_test(rate, 1));
            let mut left = vec![0.0; frames];
            let mut right = vec![0.0; frames];
            left[0] = 1.0;
            let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
            let _ = plugin.process_block_for_test(&mut channels, None);
            assert!(left.iter().chain(&right).all(|sample| sample.is_finite()));
            assert!(left.iter().chain(&right).all(|sample| sample.abs() <= 4.01));
        }
    }
}

#[test]
#[ignore = "release-only comparative budget; run with --release --ignored --nocapture"]
fn clean_path_cpu_budget_is_recorded_against_a_copy_baseline() {
    use std::time::Instant;

    let frames = 1_000_000usize;
    let source: Vec<f32> = (0..frames)
        .map(|n| (n as f32 * 0.017).sin() * 0.2)
        .collect();
    let mut copy_l = vec![0.0; frames];
    let mut copy_r = vec![0.0; frames];
    let started = Instant::now();
    copy_l.copy_from_slice(&source);
    copy_r.copy_from_slice(&source);
    std::hint::black_box((&copy_l, &copy_r));
    let baseline = started.elapsed();

    let mut plugin = MxmFxDelay::default();
    smooth(&plugin);
    assert!(plugin.prepare_for_test(48_000.0, 1));
    let mut left = source;
    let mut right = vec![0.0; frames];
    let started = Instant::now();
    let mut channels: Vec<&mut [f32]> = vec![&mut left, &mut right];
    let _ = plugin.process_block_for_test(&mut channels, None);
    std::hint::black_box(channels);
    let delay = started.elapsed();
    let ratio = delay.as_secs_f64() / baseline.as_secs_f64();
    let realtime_share = delay.as_secs_f64() / (frames as f64 / 48_000.0);
    eprintln!(
        "one quiet release run, 1,000,000 mono frames at 48 kHz: copy {baseline:?}; clean delay \
         {delay:?}; copy ratio {ratio:.2}; one-core realtime share {:.2}%",
        realtime_share * 100.0
    );
    assert!(
        ratio < 250.0,
        "clean path exceeded the copy-relative ceiling"
    );
    assert!(realtime_share < 0.05, "clean path exceeded 5% of one core");
}

#[test]
#[ignore = "release-only maximum-Fade budget; run with --release --ignored --nocapture"]
fn tape_dual_fade_cpu_budget_is_recorded_at_the_integrated_ceiling() {
    use std::time::Instant;

    let frames = 1_000_000usize;
    let mut engine = Engine::new(48_000.0).unwrap();
    let mut controls = Controls {
        time_s: 0.02,
        change: Change::Fade,
        routing: Routing::Dual,
        offset: 1.0,
        feedback: 1.0,
        low_cut_hz: 800.0,
        high_cut_hz: 4_000.0,
        drive: 1.0,
        model: Model::Tape,
        character: 1.0,
        motion: 1.0,
        rate_hz: 20.0,
        shape: Shape::Random,
        duck: 1.0,
        mix: 1.0,
        ..Controls::default()
    };
    engine.reset(controls);
    let started = Instant::now();
    let mut peak = 0.0f32;
    for sample in 0..frames {
        if sample % 257 == 0 {
            controls.time_s = if controls.time_s < 1.0 { 6.0 } else { 0.02 };
        }
        let output = engine.process(
            [if sample % 997 == 0 { 0.4 } else { 0.0 }, -0.2],
            false,
            controls,
        );
        peak = peak.max(output[0].abs()).max(output[1].abs());
    }
    std::hint::black_box(peak);
    let elapsed = started.elapsed();
    let realtime_share = elapsed.as_secs_f64() / (frames as f64 / 48_000.0);
    eprintln!(
        "one quiet release run, 1,000,000 stereo frames at 48 kHz, Tape + Dual + continuously \
         retargeted Fade: {elapsed:?}; one-core realtime share {:.2}%",
        realtime_share * 100.0
    );
    assert!(
        realtime_share < 0.05,
        "maximum Fade exceeded 5% of one core"
    );
}
