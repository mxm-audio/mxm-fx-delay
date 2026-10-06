//! T15 — `mxm-fx-delay` through the player's ordinary discovery and serial effect chain.
//!
//! Product DSP remains in its own crate. These checks hold the shipping boundary: the bundle is
//! classified as an effect, its sidecar map loads, the default sound delays, the tail outlives the
//! source, and player bypass restores dry audio to the bit.

use mxm_player_harness::app_harness;
use mxm_player_harness::harness;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use harness::{CHANNELS, Harness, mxm_mono_01};
use mxm_player::events::input::Payload;

const SOURCE_ID: &str = "dk.mxm.mxm-mono-01";
const EFFECT_ID: &str = "dk.mxm.mxm-fx-delay";
const BLOCK: usize = 256;
const SAMPLE_RATE: f32 = 48_000.0;
const DEFAULT_TIME_S: f32 = 0.35;
// Long enough for the default repeats plus the eight-second hidden-history horizon to park.
const BLOCKS: usize = 3_400;

fn delay() -> Option<PathBuf> {
    let path = app_harness::workspace_root().join("target/bundled/mxm-fx-delay.clap");
    if path.exists() {
        Some(path)
    } else {
        eprintln!(
            "skipping: {} is missing — run `cargo xtask bundle mxm-fx-delay --release`",
            path.display()
        );
        None
    }
}

#[test]
fn ordinary_discovery_classifies_the_bundle_and_loads_its_map() {
    let Some(bundle) = delay() else {
        return;
    };
    let directory = bundle.parent().unwrap().to_owned();
    let mut app = app_harness::AppHarness::new("fx-delay-discovery", vec![directory]);
    app.harness.state_mut().rescan();
    app.run();
    let state = app.state();
    let found = state
        .found
        .iter()
        .find(|found| found.id == EFFECT_ID)
        .unwrap_or_else(|| panic!("ordinary discovery did not find {EFFECT_ID}"));
    assert!(
        found.effect,
        "the one-input bundle was not classified as an effect"
    );
    assert!(
        app.app().control_map().knows_instrument(EFFECT_ID),
        "the control-map sidecar was not loaded"
    );
}

fn render(with_effect: bool, bypassed: bool) -> Option<Vec<f32>> {
    let source = mxm_mono_01()?;
    let effect = delay()?;
    let chain: Vec<(&Path, &str)> = if with_effect {
        vec![(effect.as_path(), EFFECT_ID)]
    } else {
        vec![]
    };
    let mut harness = Harness::with_fx(&source, SOURCE_ID, 1, &chain).expect("the chain builds");
    if let Some(effect) = harness.fx.first() {
        effect.bypassed.store(bypassed, Ordering::Relaxed);
    }

    let mut output = Vec::with_capacity(BLOCKS * BLOCK * CHANNELS);
    for block in 0..BLOCKS {
        if block == 2 {
            assert!(harness.push_at(
                0,
                0,
                Payload::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: 100.0 / 127.0,
                },
            ));
        }
        if block == 28 {
            assert!(harness.push_at(
                0,
                0,
                Payload::NoteOff {
                    channel: 0,
                    key: 60,
                    velocity: 0.0,
                },
            ));
        }
        output.extend_from_slice(harness.render(BLOCK));
    }
    harness.shutdown();
    Some(output)
}

fn peak_frames(interleaved: &[f32], frames: std::ops::Range<usize>) -> f32 {
    interleaved[frames.start * CHANNELS..frames.end * CHANNELS]
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

fn last_sounding_frame(interleaved: &[f32]) -> Option<usize> {
    interleaved
        .chunks(CHANNELS)
        .enumerate()
        .rev()
        .find(|(_, frame)| frame.iter().any(|sample| *sample != 0.0))
        .map(|(frame, _)| frame)
}

#[test]
fn default_delay_is_audible_finite_and_outlives_the_source() {
    let Some(dry) = render(false, false) else {
        return;
    };
    let wet = render(true, false).expect("the bundle existed above");
    assert!(wet.iter().all(|sample| sample.is_finite()));
    assert_ne!(wet, dry, "the loaded effect must change the signal");
    let dry_end = last_sounding_frame(&dry).unwrap();
    let wet_end = last_sounding_frame(&wet).unwrap();
    assert!(wet_end > dry_end, "the delay tail must outlive its input");
    let delay = (DEFAULT_TIME_S * SAMPLE_RATE) as usize;
    assert!(
        peak_frames(&wet, dry_end + delay / 2..dry_end + delay * 2) > 1e-5,
        "no repeat arrived around the declared default delay"
    );
    let end = BLOCKS * BLOCK;
    assert!(
        wet_end < end - 4 * BLOCK,
        "the delay did not park by frame {wet_end} of {end}"
    );
    assert!(
        wet[(end - 2 * BLOCK) * CHANNELS..]
            .iter()
            .all(|sample| *sample == 0.0),
        "a finite tail must end in exact silence"
    );
    let first = dry
        .chunks(CHANNELS)
        .position(|frame| frame.iter().any(|sample| *sample != 0.0))
        .unwrap();
    assert!(wet[..first * CHANNELS].iter().all(|sample| *sample == 0.0));
}

#[test]
fn player_bypass_restores_dry_audio_to_the_bit() {
    let Some(dry) = render(false, false) else {
        return;
    };
    let bypassed = render(true, true).expect("the bundle existed above");
    assert!(dry.iter().any(|sample| *sample != 0.0));
    assert_eq!(bypassed, dry);
}

/// The effect's own control map, read through the player's `ControlMap` as a host reads it. These
/// were MXM Player's (`tests/t5_control_map.rs`) until the collection was split into one repository
/// per product (2026-10-06); the player names no product any more.
mod control_map {
    use mxm_player::control_map::ControlMap;
    use mxm_player::control_map::schema;
    use std::path::PathBuf;

    #[test]
    fn the_general_delay_reuses_delay_roles_and_fills_the_reserved_freeze_slot() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../control-map.json");
        let mut map = ControlMap::shipped();
        map.load_instrument_map(&path)
            .expect("mxm-fx-delay's map loads");

        let id = "dk.mxm.mxm-fx-delay";
        for (role, parameter) in [
            ("fx.delay", "mix"),
            ("fx.delay_time", "time"),
            ("fx.delay_feedback", "feedback"),
            ("fx.delay_spread", "offset"),
            ("fx.delay_wobble", "motion"),
            ("fx.delay_wobble_rate", "rate"),
            ("fx.delay_freeze", "freeze"),
        ] {
            assert_eq!(
                map.param_for(id, role),
                Some(schema::hash_param_id(parameter)),
                "{role}"
            );
        }
        assert!(map.param_for(id, "fx.delay_line").is_none());
        assert!(map.param_for(id, "fx.delay_bias").is_none());

        let standard = schema::shipped();
        let page = standard
            .pages
            .iter()
            .find(|page| page.name == "Delay")
            .expect("the standard has the Delay page");
        assert_eq!(
            page.slots.last().and_then(Option::as_deref),
            Some("fx.delay_freeze")
        );
    }
}
