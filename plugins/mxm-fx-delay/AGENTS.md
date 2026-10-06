# AGENTS.md — plugins/mxm-fx-delay

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The `mxm-fx-delay` CLAP effect: permanent parameters and identity, host tempo adaptation, factory
presets, control map, telemetry and five-card editor around
[`crates/mxm-fx-delay-dsp`](../../crates/mxm-fx-delay-dsp/AGENTS.md).

Product architecture: `../../plans/plan-mxm-fx-delay.md` (`plans/plan-mxm-fx-delay.md` in the private archive). UI
brief: [`../../docs/briefs/mxm-fx-delay.md`](../../docs/briefs/mxm-fx-delay.md).

# Ownership

Owns `Cargo.toml`, `README.md`, `control-map.json`, `presets/`, `examples/`, `tests/`, `host-tests/`
and `src/`; its licence is the repository's root `LICENSE` (there is no per-plugin `LICENSE` since
the split, 2026-10-06). DSP remains in the framework-free product crate; visual geometry remains in
`mxm-ui`.

# Local Contracts

## Permanent identity and parameters

- Product and bundle name: `mxm-fx-delay`; CLAP id: `dk.mxm.mxm-fx-delay`.
- The sole crate-local name literal is `plugin_name!` in `src/lib.rs`; `bundler.toml` is the only
  external duplicate.
- Permanent parameter ids are: `time`, `sync`, `change`, `routing`, `offset`, `feedback`, `lowcut`,
  `highcut`, `drive`, `freeze`, `model`, `character`, `motion`, `rate`, `ratesync`, `shape`, `duck`,
  `mix`. Never rename or recycle one.

Time stores 5 ms–8 s in every mode. **Its sync is the collection's one tempo-sync interface**
(`plans/plan-tempo-sync-controls.md`, `plugins/AGENTS.md`): `sync` on, its normalised knob position
selects a division on `params::TIME_SYNC` — 1/64 to a whole note on the shared `mxm-tempo` ladder,
exactly the fourteen-step table this plugin had, so every stored position keeps its division —
clamped without rescaling to the range reachable at the current finite positive host tempo. The
tempo is taken as given, with no floor on slow transports, so a division's time is the one the host
asked for and the "nothing fits eight seconds" case below about 0.469 BPM is real rather than
unreachable. Without a tempo it falls back to free Time; it never assumes a tempo or retains a stale
one. Change owns every Time/tempo/Offset command, synced or free. `sync` reads On/Off like every
sync in the collection, and still parses the old Sync/Free words.

Motion `rate` has the same pair: `ratesync` on, its modulated position picks a division on
`params::RATE_SYNC`, every LFO's ladder with the top the fastest. `MxmFxDelayParams::synced_rate`
resolves it once a block into the plugin's `synced_rate_hz`, which stands in for the knob.

Time and Offset are unsmoothed because they are commands routed through the Change law, and Rate is
unsmoothed configuration that preserves source phase. **Every other continuous control is a signal
and is smoothed**, Character included: it moves playback bandwidth, record gain, saturation and
quantisation depth on every sample, so reading it unsmoothed changes the whole audio path at once.
Smoothers advance on the parked path too, or a Mix moved to Off during silence is still mid-ramp
when audio returns and the first block is not bit-exact dry. That alone is not enough: while parked,
an exact Off **snaps** rather than crossfades, because a sleeping host can update Mix and resume
straight into audio with no silent callback in between, and a parked engine has nothing playing for
a crossfade to protect.

## Layout, Off and activity

The explicit layouts are mono input → stereo output and stereo input → stereo output. Stereo input
is never summed. Mono Standard is centred; Dual supplies width; Ping-pong begins left and alternates.
There is no note port and no developer MIDI channel.

Mix is a linear crossfade. At exact zero output is bit-exact dry, all history is logically cleared
once and DSP parks. Re-engaging starts empty. A finite tail reports `Tail(n)` recomputed from current
controls; above-unity history and nonempty Freeze report `Normal`; empty and Off report `Normal`
while parked. Activation accepts 1–768 kHz and allocates the full eight-second line at every accepted
rate; the DSP contract owns the measured memory envelope.

Changing Model while a Freeze field is held leaves the field alone; changing Routing replaces it.
The DSP contract owns the reason.

## Presets

Init is the complete engaged default and includes Mix. Factory recipes omit **only Mix**, explicitly
set Freeze off and use `Instrument::is_factory_preserved("mix")`; the shared resolver accepts that
omission only from trusted `Origin::Factory`. User/bank saves remain complete. Loaded identity takes
its baseline from all live parameters after apply, including preserved Mix.

Fifty factory sounds cover clean utility, stereo and ping-pong, vintage digital, tape, dub and
motion. `src/preset_designs.rs` is the reviewable panel-unit source table and owns the generation
law in `preset_designs::generate`; `examples/fx_delay_build_presets.rs` only writes what it returns,
so a test can compare the committed files against a fresh generation. It must: counts, uniqueness
and completeness all pass on stale files, so nothing else notices a design edited without rerunning
the generator. `examples/fx_delay_preset_audit.rs`
renders the bank, prints first-repeat/level/tail/stereo/spectral descriptors and rejects exact or
near descriptor duplicates. No name claims a maker, model or BBD.

## Editor and telemetry

Stable Effects cards are Time, Routing, Feedback, Character and Output in signal/workflow order.
They use shared dynamic paging, controls, preset app bar, keyboard cursor, themes and zoom. Time's
host value remains milliseconds/seconds; synced, the knob reads its division and the timeline's
caption shows division and effective time. Routing uses labelled input-to-repeat paths: Standard is linked and straight, Dual prints its
separate L/R times, and Ping-pong visibly crosses channels. Unlabelled vectors are not an adequate
routing explanation. Displays are software-native signal explanations, never hardware replicas.

**Every card body is a `mxm_ui::tree`** (`editor/sections.rs`): `card` describes it once, the paging
renderer (`paging::editor::show`) measures that description, and `paint` draws each leaf
through the bindings. **Floors are computed**, every frame, by `tree::card_floor` — there is no typed
floor and no declared usability minimum — and each card is as wide as its floor. The window opens
at the quarter-4K budget hugged (`REFERENCE`, `the_opening_size_is_the_budget_hugged`). Switch cells
are the parameters' own text; Freeze is the collection's on/off toggle. Knob rows
stand in the collection's knob column (`control::knob_column`) one item spacing apart. Time and
Rate each have their sync's quarter note beside them (`tree::switch_beside_knob`), and a synced
knob's column holds its free readings and every division (`binding::synced_widest`). The three displays state their sizes beside their drawing and fill
their card's width: the timeline `sections::TIMELINE_MIN` with `SPACE_2` of its own below it, the
routing diagram `sections::ROUTING_MIN` and the levels `sections::LEVELS_MIN`; only their painting
reads telemetry. The window's minimum
width is one card: the widest computed floor (Character, set by its Model cells) and the gutters.
`every_card_passes_the_tree_checks_in_every_state` runs `mxm_plugin_test::tree_checks`'s checks over
Init, Dual at eight seconds held with both syncs on and no tempo, and both syncs against a slow
tempo and at 120 bpm.

Card rectangles are not enough. The editor tests read egui's own shape list and assert that painted
labels are inside the window and not taller than they are wide, because a panel one row wide and
off screen, or a label squeezed into a column of letters, passes every fit and paging test.

`src/telemetry.rs` is the only audio-to-editor channel: bounded atomics for input/wet/output peaks,
latched clip, effective L/R times, tempo, activity and held state. Effects have no developer CC
channel. Telemetry may lose frames and never affects sound. A parked block still publishes the
**routed** time pair through `mxm_fx_delay_dsp::routed_times`, not one time twice: an idle editor is
exactly when Offset is being set and the separate Dual L/R times read.

## Control map

Mix fills existing `fx.delay`. The existing Delay page maps Time, Feedback, Offset, Motion and Rate;
Freeze fills its previously empty eighth slot through the new stepped `fx.delay_freeze` role. Model
and Character do not claim bucket-delay Line/Bias roles.

# Work Guidance

- Keep model behavior and transitions in the DSP crate. This shell converts host state and units.
- Rebundle immediately before each validator run; debug is the allocation-guard proof and release is
  the shipping profile. On the Windows development machine, one quiet release run over 1,000,000
  frames at 48 kHz measured Clean at 0.88% of one core (199× a two-channel copy) and the integrated
  Tape + Dual + continuously retargeted Fade ceiling at 1.16%; the ignored release benchmarks print
  current figures and fail above 5%. These are local regression budgets, not cross-platform claims.
- A compile or render audit does not approve character. Owner listening and native §15/DAW checks
  remain manual and must be reported as such.

# Verification

```bash
cargo test -p mxm-fx-delay
cargo clippy -p mxm-fx-delay --all-targets
MXM_PICTURES=after cargo test -p mxm-fx-delay --lib tree_pictures -- --ignored   # target/layout-tree/mxm-fx-delay/after/
cargo +1.95.0 build -p mxm-fx-delay
cargo +1.95.0 test -p mxm-fx-delay
cargo run -p mxm-fx-delay --release --example fx_delay_build_presets
cargo run -p mxm-fx-delay --release --example fx_delay_preset_audit
cargo xtask bundle mxm-fx-delay
clap-validator validate target/bundled/mxm-fx-delay.clap
cargo xtask bundle mxm-fx-delay --release
clap-validator validate target/bundled/mxm-fx-delay.clap
cargo test -p mxm-fx-delay-host-tests --test effect_chain
```

After any shared-preset change, run `mxm-preset` (in mxm-kit) and every plugin consumer as required by
its DOX. After any standard/control-map change, run the Player's control-map unit and `t5_control_map`
suites (in mxm-player) and this repository's `effect_chain` host test, whose `control_map` module
holds this plugin's map since the split. Manual gates: owner listening on insert and full-wet send,
native design-system §15 in both themes and 100/150/200%, one real DAW at a small buffer, and
Linux/macOS builds (*since the split:* checked later, together, and by CI on `v*` tags).

# Child DOX Index

No child `AGENTS.md` files.
