# AGENTS.md — crates/mxm-fx-delay-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Framework-free DSP for `mxm-fx-delay`: Clean, Vintage digital and Tape engines over Standard,
Dual and Ping-pong routing, with explicit Repitch, Fade and Jump time changes. Evidence and product
boundary are `../../plans/plan-mxm-fx-delay.md` (`plans/plan-mxm-fx-delay.md` in the private archive) and
`research:effects/delay-effects.md`.

# Ownership

Owns `src/` and `Cargo.toml`. Host tempo, nice-plug parameters, presets, telemetry and editor behavior
belong to `plugins/mxm-fx-delay`. No plugin-framework type belongs here.

`routed_times` is public so a shell can label the pair it is about to command — an idle editor has
no processed sample to read back — without keeping a second copy of the Offset law free to drift.

# Local Contracts

## One medium, one recursive path

Each line owns one store, one write/record process and one feedback return. Fade evaluates old and
new read/playback projections over that store under convex weights; it never duplicates storage or
feedback. A projection is a stereo/vector endpoint, so Dual may make two reads per endpoint while
each line still has at most old and new. Rapid Fade retargeting completes the live two-endpoint
transition and retains only the latest pending target — including a command back to the endpoint
currently being faded to, which cancels an older queued one instead of reading as a no-op. A held
Freeze still crossfades a Fade's two rounded endpoints rather than holding the old address and
stepping to the new one at promotion.

Repitch moves one read relationship to an exact endpoint. Jump ramps heard wet and feedback return
to exact zero, changes address there, then ramps back. That envelope is applied once, downstream of
the loop's stateful colour, and the feedback return inherits the same envelope rather than its
square: applied before the loop filters, a populated one-pole rings straight through the address
change the envelope exists to hide. Retargeting mid-Jump restarts the ramp from the gain actually
being emitted. Offset is a time command in Dual and uses the same selected law; it is inert in
Standard and Ping-pong.

## Model coupling

- Clean adds no model colour beyond measured fractional interpolation and the user loop processors.
- Vintage digital quantises the recirculating write and couples a longer effective clock period to
  lower playback bandwidth. It is a generic architecture model, not a named unit.
- Tape writes through a bounded record stage; delay is head distance divided by speed, and speed
  drives playback loss plus deterministic linked/common and channel-local transport movement. It is
  a generic moving-medium model, not a named machine.

Fade's overlap is deliberately nonphysical: the new target owns the one write clock/record transport
while the outgoing projection retains a virtual prior read/playback law for the finite overlap.

## Feedback, Freeze and activity

Loop HPF, LPF and Drive are per-pass and feed both heard wet and feedback. Feedback reaches unity
near the top and the monotonic saturator bounds above-unity operation.

Arming waits for something to capture, not for a *current* input: an armed Freeze resolves into
capture on stored history as well as on arriving input. Checking arriving input alone strands Freeze
armed over a one-sample impulse, or over the input re-recorded when a topology replacement re-arms
Freeze underneath it.

Freeze first captures one recurrence (one line lap in Standard/Dual, a full two-lap left→right→left
cycle in Ping-pong), then closes injection and uses rounded integer reads with unity routing while
bypassing conversion, filtering, drive, wear and stochastic variation. User and inherent motion
settle out across the Freeze crossfade; explicit Time changes still use the selected Change law.
Empty Freeze is armed and parked until input arrives, including a one-sample first input. Release
returns the held field to the ordinary loop.

**Model does not disturb a held field; Routing replaces it** (owner, 2026-09-18). While the field is
fully held, Model is inaudible — held reads bypass playback colour and the recirculating write
bypasses the record stage — so a Model change settles the active model with no crossfade and no
clear. Clearing there deleted the captured sound for no audible benefit. Routing is a different
case: a lap means something else under Ping-pong and the held return is still routed, so a Routing
change keeps the ordinary topology crossfade and the clear that comes with it.

The eligibility test is taken **after this sample's Freeze state is settled**, not at the top of the
callback where it would read the previous sample. Both edges of the hold are otherwise one-sample
races, including the sample the hold fade completes on: a crossfade started there is no longer owed,
and `topology.gain` would duck the held output and attenuate what the field recirculates into
itself, baking a hole into the frozen loop. Freeze released on the same sample as a Model change takes the
ordinary crossfade: the ordinary path becomes audible that sample and owes one. A Model change that
lands while the capture is still running keeps its crossfade — Model is audible then — but **does
not clear**, so an edit a few milliseconds early is not destructive when the same edit a few
milliseconds later is not. Once Freeze is engaged, a Model-only replacement never clears. The crossfade helper
returns its endpoints exactly, or a fully held field would still leak the ordinary path into its own
last bits and a Model change would alter samples it cannot be heard in. Mix zero
wins, clears logically and is bit-exact dry.

Activity keeps valid history independently of the current tap. A quiet engine waits the maximum
possible hidden journey before logical clear and parking. Sustaining feedback or a nonempty held
field is not reported as a finite tail.

A topology replacement is the exception to that wait: its crossfade's zero crossing clears both
lines, so there is no hidden journey left to cover. The outgoing topology's last emitted sample is
not history and must not revive activity, and an emptied engine parks at once over silence rather
than reporting itself as an endless tail at unity feedback.

The clear lands after this sample's input has already been recorded into the outgoing line, so that
input is re-recorded into the incoming one. Otherwise every Model or Routing change made while audio
plays would punch a one-sample hole in the input, and the erased sample would still be counted as
history the engine no longer holds. The outgoing feedback return is not carried over: it belonged to
the line that no longer exists.

Duck's detector follows the input at every depth, including zero. A detector frozen while the depth
is zero holds a stale envelope, so raising Duck again over silence would duck against a level that
no longer exists.

## Realtime and numeric rules

- Rust MSRV is 1.87. Normal/build dependencies are empty; `mxm-measure` is dev-only.
- Construction is the only allocation point. Every sample method is allocation-, lock-, log- and
  I/O-free.
- Delay clears use per-cell generations and do not rewrite sample-rate-sized storage. The tag is a
  wrapping `u32`: `valid` is what makes a clear correct, so the tag is defence in depth and its
  width is an allocation decision, not a correctness one.
- A model's cutoff floor is a musical choice and the anti-image ceiling belongs to the rate. Where
  the two cross at the bottom of the accepted range the ceiling wins and the model runs dark.
  Ordering them as a bare `clamp(floor, ceiling)` panics on the audio thread instead.
- Host block boundaries are not model state. Random state resets to fixed seeds. Shape retargeting
  starts from the command actually being emitted, so rapid automation cannot jump or grow a queue.
- Public sample-rate range is 1–768 kHz; invalid rates are refused. Public delay range is 5 ms to
  8 s at every accepted rate. Two lines' sample and generation stores are eight bytes per slot over
  `ceil(rate * 8 s) + 4` slots: about 5.9 MiB at 48 kHz, 23.4 MiB at 192 kHz and 93.8 MiB at the
  hostile 768 kHz ceiling. This is the activation envelope chosen to keep the full public delay
  range and pass host-rate validation honestly, and a test recomputes it from the stored field
  widths so widening one cannot silently falsify the figures.
- Non-finite audio and controls are neutralised at public seams before recursive arithmetic.
  Subnormal state snaps to zero without adding noise — the accumulator itself, not a copy of it on
  the way out, or the recursion keeps doing subnormal arithmetic for as long as it runs.
- A parked engine settles discrete edits instead of crossfading them: there is no audio to protect,
  and a topology fade left pending across the silence would run over the first input recorded on
  wake and clear the line underneath it. Parking preserves the live controls. An armed Freeze with
  nothing to capture stays parked until real input arrives.
- Audio is `f32`; delay position, phase and recursive coefficients use `f64` where precision
  compounds. Delay age is split into integer and fractional parts before ring wrapping.

## Chosen constants

The 1–768 kHz accepted-rate and 5 ms–8 s public delay ranges; 25 ms time/Freeze fades; 15 ms topology and Shape fades; the feedback
law reaching unity at about 92.6%; the model bit-depth, bandwidth, saturation and transport laws;
duck detector times; quiet threshold/hold; and all random seeds are chosen for this implementation.
They are not measurements of any commercial product. Replace them only with measurement and update
this contract.

# Work Guidance

- Measure integrated delay behavior; a compiling line does not prove pitch travel, repeats, colour,
  freeze or stability.
- Keep interpolation, model and routing implementations product-local until another honest product
  demonstrates a shared API.
- Carry the delay-history, finite-transition, non-finite-recovery and late-attractor review prompts
  from [`../../docs/code-review-notes.md`](https://github.com/mxm-audio/mxm-kit/blob/main/docs/code-review-notes.md).

# Verification

```bash
cargo test -p mxm-fx-delay-dsp
cargo clippy -p mxm-fx-delay-dsp --all-targets
cargo tree -e normal,build -p mxm-fx-delay-dsp
cargo +1.87.0 build -p mxm-fx-delay-dsp
cargo +1.87.0 test -p mxm-fx-delay-dsp
```

Tests retain integer/fractional reads, constant-time invalidation, finite endpoints and bounded Fade
projection count, routing vectors, Offset laws, model boundedness, duck placement, Freeze arming,
full Ping-pong capture, late-input rejection and held periodicity, exact Off/reset, deterministic and
rapidly retargeted motion, topology churn, accepted rates and non-finite recovery.

They also retain the repairs that a compiling engine hid: every model and routing **processed** at
the minimum, maximum and ordinary rates rather than merely constructed; latest-command ownership
across a queued Fade; Jump gain continuity when Time is retargeted during recovery, and exact
silence at the address change with populated loop colour; a held field crossfading a Fade instead of
stepping at promotion; parked discrete edits settling without erasing the waking input; an armed
Freeze staying parked on silence; recursive accumulators reaching exact zero rather than subnormal
arithmetic; and the published activation envelope recomputed from the stored field widths.

They also retain a held field surviving a Model change bit for bit while a Routing change replaces
it, all three edges of the hold — a same-sample release still crossfading, a Model change landing one
sample into the capture still keeping the field, and a Model change on the sample the hold fade
completes starting no crossfade at all — an emptied topology parking without reporting a tail, an input landing on exactly
the replacement sample surviving into the new topology *and* being captured by an engaged Freeze
with no later input, and Duck's detector following the input at zero depth.

Two proofs are measured with `mxm-measure` rather than asserted, because a compiling line proves
neither:

- The fractional read, swept over five frequencies to 12 kHz and three fractions. The cubic keeps
  more of the band than linear interpolation of the same line at **every** point, and the margin
  grows with frequency; its worst retention is 86% at 12 kHz/48 kHz. Its fractional-delay accuracy
  is within 0.02 samples wherever the phase is unambiguous, but is **not** better than linear's —
  the source comment previously claimed a phase advantage over linear, and the measurement does not
  support it. The claim is now magnitude and bounded overshoot only.
- Repitch shifts the repeat's pitch while Fade leaves it alone.

# Child DOX Index

No child `AGENTS.md` files.
