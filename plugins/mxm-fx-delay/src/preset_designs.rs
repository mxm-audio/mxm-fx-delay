//! The factory bank as readable overrides on Init. Values use panel units; enum values are their
//! declared 0-based positions. The generator writes complete recipes and omits only Mix.

use mxm_preset::Category;

pub type Design = (&'static str, Category, &'static [(&'static str, f32)]);

const REPITCH: f32 = 0.0;
const FADE: f32 = 1.0;
const JUMP: f32 = 2.0;
const DUAL: f32 = 1.0;
const PING_PONG: f32 = 2.0;
const DIGITAL: f32 = 1.0;
const TAPE: f32 = 2.0;
const SINE: f32 = 0.0;
const TRIANGLE: f32 = 1.0;
const RANDOM: f32 = 2.0;

pub const DESIGNS: &[Design] = &[
    (
        "Plain echo",
        Category::Template,
        &[("time", 0.32), ("feedback", 0.34), ("duck", 0.08)],
    ),
    (
        "Short slap",
        Category::Fx,
        &[
            ("time", 0.075),
            ("feedback", 0.12),
            ("highcut", 15_000.0),
            ("duck", 0.0),
        ],
    ),
    (
        "Voice pocket",
        Category::Fx,
        &[
            ("time", 0.145),
            ("feedback", 0.28),
            ("lowcut", 160.0),
            ("highcut", 8_500.0),
            ("duck", 0.48),
        ],
    ),
    (
        "Guitar room",
        Category::Fx,
        &[
            ("time", 0.095),
            ("feedback", 0.22),
            ("lowcut", 90.0),
            ("highcut", 7_200.0),
            ("drive", 0.18),
        ],
    ),
    (
        "Clean quarter",
        Category::Fx,
        &[
            ("sync", 1.0),
            ("time", 2.8043),
            ("feedback", 0.48),
            ("change", FADE),
        ],
    ),
    (
        "Clean eighth",
        Category::Fx,
        &[
            ("sync", 1.0),
            ("time", 0.6771),
            ("feedback", 0.39),
            ("change", JUMP),
        ],
    ),
    (
        "Dotted lift",
        Category::Fx,
        &[
            ("sync", 1.0),
            ("time", 1.8416),
            ("feedback", 0.52),
            ("duck", 0.30),
        ],
    ),
    (
        "Long clear",
        Category::Pad,
        &[
            ("time", 1.8),
            ("feedback", 0.67),
            ("lowcut", 120.0),
            ("highcut", 16_000.0),
            ("drive", 0.02),
        ],
    ),
    (
        "Wide pair",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.38),
            ("time", 0.34),
            ("feedback", 0.40),
        ],
    ),
    (
        "Wide reverse lead",
        Category::Lead,
        &[
            ("routing", DUAL),
            ("offset", -0.55),
            ("time", 0.24),
            ("feedback", 0.36),
            ("duck", 0.20),
        ],
    ),
    (
        "Twin slap",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.82),
            ("time", 0.085),
            ("feedback", 0.16),
        ],
    ),
    (
        "Stereo quarter",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.25),
            ("sync", 1.0),
            ("time", 2.8043),
            ("feedback", 0.54),
        ],
    ),
    (
        "Left first",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.68),
            ("time", 0.42),
            ("feedback", 0.46),
        ],
    ),
    (
        "Right first",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", -0.68),
            ("time", 0.42),
            ("feedback", 0.46),
        ],
    ),
    (
        "Ping rhythm",
        Category::Sequence,
        &[
            ("routing", PING_PONG),
            ("sync", 1.0),
            ("time", 0.6771),
            ("feedback", 0.57),
            ("change", JUMP),
        ],
    ),
    (
        "Ping horizon",
        Category::Pad,
        &[
            ("routing", PING_PONG),
            ("time", 0.78),
            ("feedback", 0.72),
            ("lowcut", 180.0),
            ("highcut", 9_000.0),
        ],
    ),
    (
        "Bright memory",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("time", 0.12),
            ("feedback", 0.32),
            ("character", 0.18),
            ("highcut", 18_000.0),
        ],
    ),
    (
        "Clocked vocal",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("time", 0.28),
            ("feedback", 0.44),
            ("character", 0.42),
            ("lowcut", 190.0),
            ("highcut", 10_500.0),
        ],
    ),
    (
        "Narrow memory",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("time", 0.72),
            ("feedback", 0.55),
            ("character", 0.66),
            ("highcut", 6_000.0),
        ],
    ),
    (
        "Long converter",
        Category::Drone,
        &[
            ("model", DIGITAL),
            ("time", 1.65),
            ("feedback", 0.70),
            ("character", 0.82),
            ("drive", 0.25),
            ("highcut", 4_200.0),
        ],
    ),
    (
        "Digital dotted",
        Category::Sequence,
        &[
            ("model", DIGITAL),
            ("sync", 1.0),
            ("time", 1.8416),
            ("feedback", 0.58),
            ("character", 0.52),
            ("change", JUMP),
        ],
    ),
    (
        "Digital ping",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("routing", PING_PONG),
            ("time", 0.31),
            ("feedback", 0.60),
            ("character", 0.48),
        ],
    ),
    (
        "Grainy width",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("routing", DUAL),
            ("offset", 0.48),
            ("time", 0.19),
            ("feedback", 0.38),
            ("character", 0.90),
            ("drive", 0.34),
        ],
    ),
    (
        "Clock descent",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("change", REPITCH),
            ("time", 0.95),
            ("feedback", 0.62),
            ("character", 0.72),
        ],
    ),
    (
        "Maintained tape",
        Category::Fx,
        &[
            ("model", TAPE),
            ("time", 0.34),
            ("feedback", 0.42),
            ("character", 0.16),
            ("drive", 0.12),
        ],
    ),
    (
        "Warm echo",
        Category::Fx,
        &[
            ("model", TAPE),
            ("time", 0.41),
            ("feedback", 0.50),
            ("character", 0.42),
            ("drive", 0.28),
            ("highcut", 8_000.0),
        ],
    ),
    (
        "Worn repeat",
        Category::Fx,
        &[
            ("model", TAPE),
            ("time", 0.52),
            ("feedback", 0.58),
            ("character", 0.78),
            ("drive", 0.44),
            ("highcut", 5_200.0),
        ],
    ),
    (
        "Slow transport",
        Category::Drone,
        &[
            ("model", TAPE),
            ("time", 1.9),
            ("feedback", 0.69),
            ("character", 0.70),
            ("lowcut", 160.0),
            ("highcut", 4_600.0),
        ],
    ),
    (
        "Tape dotted",
        Category::Sequence,
        &[
            ("model", TAPE),
            ("sync", 1.0),
            ("time", 1.8416),
            ("feedback", 0.55),
            ("character", 0.48),
            ("change", REPITCH),
        ],
    ),
    (
        "Tape pair",
        Category::Fx,
        &[
            ("model", TAPE),
            ("routing", DUAL),
            ("offset", 0.34),
            ("time", 0.29),
            ("feedback", 0.46),
            ("character", 0.55),
        ],
    ),
    (
        "Travelling ping",
        Category::Pad,
        &[
            ("model", TAPE),
            ("routing", PING_PONG),
            ("time", 0.66),
            ("feedback", 0.66),
            ("character", 0.62),
            ("drive", 0.32),
        ],
    ),
    (
        "Tape stop play",
        Category::Fx,
        &[
            ("model", TAPE),
            ("change", REPITCH),
            ("time", 1.25),
            ("feedback", 0.56),
            ("character", 0.58),
        ],
    ),
    (
        "Dub clean",
        Category::Fx,
        &[
            ("time", 0.46),
            ("feedback", 0.82),
            ("lowcut", 170.0),
            ("highcut", 5_800.0),
            ("drive", 0.30),
        ],
    ),
    (
        "Dub digital",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("time", 0.39),
            ("feedback", 0.86),
            ("lowcut", 240.0),
            ("highcut", 4_500.0),
            ("drive", 0.48),
            ("character", 0.65),
        ],
    ),
    (
        "Dub tape",
        Category::Fx,
        &[
            ("model", TAPE),
            ("time", 0.44),
            ("feedback", 0.88),
            ("lowcut", 210.0),
            ("highcut", 4_000.0),
            ("drive", 0.55),
            ("character", 0.72),
        ],
    ),
    (
        "Edge of orbit",
        Category::Drone,
        &[
            ("routing", PING_PONG),
            ("feedback", 0.91),
            ("time", 0.27),
            ("lowcut", 300.0),
            ("highcut", 6_400.0),
            ("drive", 0.50),
        ],
    ),
    (
        "Self field",
        Category::Drone,
        &[
            ("model", DIGITAL),
            ("feedback", 0.96),
            ("time", 0.18),
            ("lowcut", 450.0),
            ("highcut", 3_800.0),
            ("drive", 0.72),
            ("character", 0.82),
        ],
    ),
    (
        "Dark throw",
        Category::Fx,
        &[
            ("model", TAPE),
            ("feedback", 0.79),
            ("time", 0.73),
            ("lowcut", 320.0),
            ("highcut", 2_600.0),
            ("drive", 0.46),
            ("duck", 0.0),
        ],
    ),
    (
        "Ducked dub",
        Category::Fx,
        &[
            ("model", TAPE),
            ("feedback", 0.84),
            ("time", 0.36),
            ("lowcut", 190.0),
            ("highcut", 4_800.0),
            ("drive", 0.38),
            ("duck", 0.72),
        ],
    ),
    (
        "Bright runaway",
        Category::Fx,
        &[
            ("feedback", 0.94),
            ("time", 0.105),
            ("lowcut", 650.0),
            ("highcut", 14_000.0),
            ("drive", 0.64),
        ],
    ),
    (
        "Clean chorus",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.22),
            ("time", 0.018),
            ("feedback", 0.12),
            ("motion", 0.42),
            ("rate", 0.48),
            ("shape", SINE),
        ],
    ),
    (
        "Triangle double",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", -0.18),
            ("time", 0.028),
            ("feedback", 0.18),
            ("motion", 0.55),
            ("rate", 0.72),
            ("shape", TRIANGLE),
        ],
    ),
    (
        "Random width",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.30),
            ("time", 0.065),
            ("feedback", 0.34),
            ("motion", 0.62),
            ("rate", 0.26),
            ("shape", RANDOM),
        ],
    ),
    (
        "Digital orbit",
        Category::Fx,
        &[
            ("model", DIGITAL),
            ("routing", DUAL),
            ("offset", 0.45),
            ("time", 0.11),
            ("feedback", 0.42),
            ("motion", 0.50),
            ("rate", 0.65),
            ("shape", SINE),
            ("character", 0.58),
        ],
    ),
    (
        "Tape drift",
        Category::Pad,
        &[
            ("model", TAPE),
            ("time", 0.58),
            ("feedback", 0.59),
            ("motion", 0.35),
            ("rate", 0.09),
            ("shape", RANDOM),
            ("character", 0.76),
        ],
    ),
    (
        "Tape flutter",
        Category::Fx,
        &[
            ("model", TAPE),
            ("time", 0.16),
            ("feedback", 0.38),
            ("motion", 0.48),
            ("rate", 5.2),
            ("shape", TRIANGLE),
            ("character", 0.64),
        ],
    ),
    (
        "Moving ping",
        Category::Sequence,
        &[
            ("routing", PING_PONG),
            ("time", 0.23),
            ("feedback", 0.57),
            ("motion", 0.44),
            ("rate", 0.38),
            ("shape", SINE),
        ],
    ),
    (
        "Random memory",
        Category::Drone,
        &[
            ("model", DIGITAL),
            ("time", 1.1),
            ("feedback", 0.68),
            ("motion", 0.58),
            ("rate", 0.07),
            ("shape", RANDOM),
            ("character", 0.70),
        ],
    ),
    (
        "Fast shimmer echo",
        Category::Fx,
        &[
            ("routing", DUAL),
            ("offset", 0.60),
            ("time", 0.045),
            ("feedback", 0.25),
            ("motion", 0.70),
            ("rate", 8.0),
            ("shape", SINE),
            ("highcut", 11_000.0),
        ],
    ),
    (
        "Slow horizon",
        Category::Pad,
        &[
            ("model", TAPE),
            ("routing", DUAL),
            ("offset", 0.52),
            ("time", 2.4),
            ("feedback", 0.73),
            ("motion", 0.46),
            ("rate", 0.025),
            ("shape", RANDOM),
            ("character", 0.82),
            ("duck", 0.28),
        ],
    ),
];

/// Compiles [`DESIGNS`] into the complete factory recipes the bank ships.
///
/// This is the single implementation of the generation law. `examples/fx_delay_build_presets.rs`
/// writes what it returns and the bank's tests compare the shipped files against it, so a design
/// edited without rerunning the generator is caught rather than shipped: nothing else in the suite
/// notices, because counts, uniqueness and completeness all still pass on stale files.
pub fn generate() -> Vec<(String, String)> {
    use crate::params::MxmFxDelayParams;
    use crate::preset::{Preset, Value};
    use mxm_preset::Instrument;
    use std::collections::{BTreeSet, HashMap};

    let params = MxmFxDelayParams::default();
    let parameter_map: HashMap<_, _> = params.parameters().into_iter().collect();
    let mut slugs = BTreeSet::new();
    let mut generated = Vec::with_capacity(DESIGNS.len());
    for (name, category, overrides) in DESIGNS {
        assert_ne!(
            *category,
            Category::Uncategorised,
            "{name}: a shipped sound states its category"
        );
        let mut preset = Preset::capture(*name, *category, &params);
        // Factory recipes omit only Mix, which the shared resolver preserves from the live value.
        preset.params.remove("mix");
        for &(id, plain) in *overrides {
            let normalised = normalise(&params, id, plain);
            let parameter = parameter_map
                .get(id)
                .unwrap_or_else(|| panic!("{name}: unknown parameter {id}"));
            preset.params.insert(
                id.to_owned(),
                Value {
                    v: normalised,
                    text: parameter.format(normalised),
                },
            );
        }
        assert_eq!(
            preset.params.len(),
            17,
            "{name}: factory recipe is incomplete"
        );
        assert_eq!(preset.params["freeze"].v, 0.0, "{name}: Freeze must be off");
        let slug = slug(name);
        assert!(slugs.insert(slug.clone()), "duplicate slug {slug}");
        generated.push((slug, preset.to_json() + "\n"));
    }
    assert_eq!(generated.len(), 50);
    generated
}

fn normalise(params: &crate::params::MxmFxDelayParams, id: &str, value: f32) -> f32 {
    use nice_plug::prelude::Param;
    match id {
        "time" => params.time.preview_normalized(value),
        "sync" | "ratesync" | "freeze" => value.clamp(0.0, 1.0),
        "change" | "routing" | "model" | "shape" => (value / 2.0).clamp(0.0, 1.0),
        "offset" => params.offset.preview_normalized(value),
        "feedback" => params.feedback.preview_normalized(value),
        "lowcut" => params.low_cut.preview_normalized(value),
        "highcut" => params.high_cut.preview_normalized(value),
        "drive" => params.drive.preview_normalized(value),
        "character" => params.character.preview_normalized(value),
        "motion" => params.motion.preview_normalized(value),
        "rate" => params.rate.preview_normalized(value),
        "duck" => params.duck.preview_normalized(value),
        "mix" => panic!("factory presets preserve Mix"),
        _ => panic!("unknown parameter {id}"),
    }
}

/// The file stem a sound's name compiles to.
pub fn slug(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
