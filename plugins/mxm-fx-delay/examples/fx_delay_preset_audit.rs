//! Render-derived factory-bank descriptors. This writes no audio; redirect the table for review.

use mxm_fx_delay::params::MxmFxDelayParams;
use mxm_fx_delay::preset::{FACTORY_FILES, Preset};
use mxm_fx_delay_dsp::{Change, Controls, Engine, Model, Routing, Shape};
use nice_plug::prelude::Param;

const RATE: f32 = 48_000.0;

fn main() {
    let params = MxmFxDelayParams::default();
    println!("name\tfirst_ms\tpeak\ttail_rms\tstereo_rms\t3k_to_300_db");
    let mut descriptors = Vec::new();
    for (_, json) in FACTORY_FILES {
        let preset = Preset::parse(json, mxm_fx_delay::CLAP_ID).expect("factory JSON");
        let controls = controls(&params, &preset);
        let render = render(controls);
        let first = render
            .iter()
            .position(|frame| frame[0].abs().max(frame[1].abs()) > 1e-5)
            .map_or(0.0, |sample| sample as f32 * 1_000.0 / RATE);
        let left: Vec<_> = render.iter().map(|frame| frame[0]).collect();
        let right: Vec<_> = render.iter().map(|frame| frame[1]).collect();
        let difference: Vec<_> = render.iter().map(|frame| frame[0] - frame[1]).collect();
        let tail = &left[(RATE as usize * 2)..];
        let peak = mxm_measure::level::peak(&left).expect("finite render");
        let tail_rms = mxm_measure::level::rms(tail).expect("nonempty finite tail");
        let stereo = mxm_measure::level::rms(&difference).expect("finite stereo vector");
        let low = mxm_measure::spectrum::component_amplitude(&left, 300.0, RATE as f64)
            .expect("finite render");
        let high = mxm_measure::spectrum::component_amplitude(&left, 3_000.0, RATE as f64)
            .expect("finite render");
        let tilt = mxm_measure::convert::amplitude_db(high / low.max(1e-12));
        // Keep right alive in the audit even when the difference is zero; a non-finite right must
        // fail rather than disappear from left-only descriptors.
        mxm_measure::level::peak(&right).expect("finite right render");
        println!(
            "{}\t{first:.1}\t{peak:.4}\t{tail_rms:.6}\t{stereo:.6}\t{tilt:.2}",
            preset.name
        );
        descriptors.push((
            preset.name,
            [
                f64::from(first.max(1.0)).ln(),
                f64::from(peak),
                tail_rms.max(1e-8).log10(),
                stereo.max(1e-8).log10(),
                tilt,
            ],
        ));
    }
    reject_descriptor_neighbours(&descriptors);
}

/// A render descriptor is deliberately coarse: it can reject two files that make the same sound,
/// not certify that two sounds are musically distinct. Threshold 0.20 was chosen after inspecting
/// the first complete bank; the closest accepted pair is printed on every run for review.
fn reject_descriptor_neighbours(descriptors: &[(String, [f64; 5])]) {
    let count = descriptors.len() as f64;
    let mut mean = [0.0; 5];
    for (_, descriptor) in descriptors {
        for index in 0..5 {
            mean[index] += descriptor[index] / count;
        }
    }
    let mut deviation = [0.0; 5];
    for (_, descriptor) in descriptors {
        for index in 0..5 {
            deviation[index] += (descriptor[index] - mean[index]).powi(2);
        }
    }
    for value in &mut deviation {
        *value = (*value / (count - 1.0)).sqrt().max(1e-12);
    }

    let mut closest = (f64::INFINITY, "", "");
    for right in 0..descriptors.len() {
        for left in 0..right {
            let distance = (0..5)
                .map(|index| {
                    ((descriptors[right].1[index] - descriptors[left].1[index]) / deviation[index])
                        .powi(2)
                })
                .sum::<f64>()
                .sqrt();
            if distance < closest.0 {
                closest = (distance, &descriptors[left].0, &descriptors[right].0);
            }
        }
    }
    eprintln!(
        "closest render descriptors: {:?} and {:?}, distance {:.3}",
        closest.1, closest.2, closest.0
    );
    assert!(
        closest.0 >= 0.20,
        "descriptor-near duplicate: {:?} and {:?} at {:.3}",
        closest.1,
        closest.2,
        closest.0
    );
}

fn value(preset: &Preset, id: &str) -> f32 {
    preset
        .params
        .get(id)
        .unwrap_or_else(|| panic!("{} lacks {id}", preset.name))
        .v
}

fn enum_index(preset: &Preset, id: &str) -> usize {
    (value(preset, id) * 2.0).round().clamp(0.0, 2.0) as usize
}

fn controls(params: &MxmFxDelayParams, preset: &Preset) -> Controls {
    let free_time = params.time.preview_plain(value(preset, "time"));
    let sync = value(preset, "sync") >= 0.5;
    let time_s = if sync {
        mxm_fx_delay::params::TIME_SYNC
            .division(
                value(preset, "time"),
                120.0,
                f64::from(mxm_fx_delay_dsp::MIN_DELAY_S),
                f64::from(mxm_fx_delay_dsp::MAX_DELAY_S),
            )
            .seconds(120.0) as f32
    } else {
        free_time
    };
    Controls {
        time_s,
        change: [Change::Repitch, Change::Fade, Change::Jump][enum_index(preset, "change")],
        routing: [Routing::Standard, Routing::Dual, Routing::PingPong]
            [enum_index(preset, "routing")],
        offset: params.offset.preview_plain(value(preset, "offset")),
        feedback: params.feedback.preview_plain(value(preset, "feedback")),
        low_cut_hz: params.low_cut.preview_plain(value(preset, "lowcut")),
        high_cut_hz: params.high_cut.preview_plain(value(preset, "highcut")),
        drive: params.drive.preview_plain(value(preset, "drive")),
        freeze: false,
        model: [Model::Clean, Model::VintageDigital, Model::Tape][enum_index(preset, "model")],
        character: params.character.preview_plain(value(preset, "character")),
        motion: params.motion.preview_plain(value(preset, "motion")),
        rate_hz: params.rate.preview_plain(value(preset, "rate")),
        shape: [Shape::Sine, Shape::Triangle, Shape::Random][enum_index(preset, "shape")],
        duck: params.duck.preview_plain(value(preset, "duck")),
        mix: 1.0,
    }
}

fn render(controls: Controls) -> Vec<[f32; 2]> {
    let mut engine = Engine::new(RATE).expect("audit rate supported");
    engine.reset(controls);
    let frames = RATE as usize * 4;
    (0..frames)
        .map(|sample| {
            let time = sample as f32 / RATE;
            let input = if sample < RATE as usize {
                let harmonic: f32 = (1..=10)
                    .map(|index| {
                        (core::f32::consts::TAU * 300.0 * index as f32 * time).sin() / index as f32
                    })
                    .sum();
                0.12 * harmonic
            } else {
                0.0
            };
            engine.process([input, input], false, controls)
        })
        .collect()
}
