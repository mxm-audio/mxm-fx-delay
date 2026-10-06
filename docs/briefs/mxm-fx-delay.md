# mxm-fx-delay — interface brief

**Status:** implementation brief, written before editor code. Product architecture is
`../../plans/plan-mxm-fx-delay.md` (`plans/plan-mxm-fx-delay.md` in the private archive); this answers design-system
§14 and does not reopen its DSP decisions.

## 1. Primary task

Set a repeat rhythm, choose how it moves when time changes, then shape how each generation ages.
The everyday path is Time → routing → feedback → model → output; the creative path uses the same
controls at high feedback, motion or Freeze rather than a second mode.

## 2. Controls reached for first

**Time, Feedback, Mix, Routing and Model.** Freeze is the immediate performance action beside them.
All remain directly visible on musician cards; no Advanced disclosure exists in v1.

## 3. Signal flow that must be visible

Input enters one or two delay lines, the selected read passes through model playback and loop
Low cut / High cut / Drive, and the same coloured return feeds both wet output and the routing
feedback matrix. Duck acts only on wet output; Mix is last. A compact Time display shows two repeat
positions and their movement rather than drawing a hardware head, tape reel or rack panel.

## 4. Cards and category

Five stable cards, all primary category **Effects**, in signal/workflow order:

| Key | Card | Kind | Controls / display |
|---:|---|---|---|
| 0 | Time | Timing | Time with its sync's quarter note, Change; repeat timeline and synced derived reading |
| 1 | Routing | Routing | Routing, Offset; labelled L/R source-to-repeat paths with effective times |
| 2 | Feedback | Loop | Feedback, Low cut, High cut, Drive, Freeze |
| 3 | Character | Model | Model, Character, Motion, Rate with its sync's quarter note, Shape |
| 4 | Output | Output | Duck, Mix; input/wet/output levels and clip |

Each card's floor is computed from its tree and never squeezes controls beneath it, and each card
is as wide as its floor (`plugins/mxm-fx-delay/AGENTS.md`). Cards have no
preferred group: category-first paging preserves their order and splits only where available
width/height requires.

## 5. Advanced controls

None. Every v1 control is part of the product's ordinary sound/performance surface. A future
software-added diagnostic may use disclosure, but Time, Feedback, Mix, Routing, Model and Freeze may
never move behind one.

## 6. Identity accent

Use the collection default accent rather than minting a product hue: dark `#4CC9D8`, light
`#247F91`. Measured WCAG contrast is 8.84:1 against dark `surface-1` and 4.65:1 against light
`surface-1`; against the light canvas it is 4.18:1, so it is used there as a control boundary/live
trace (3:1 requirement), not small body text. Modulation and status colors remain fixed tokens.

## 7. Live visualisations

- **Time:** effective left/right repeat positions from audio telemetry, plus motion around them. In
  Sync, a caption reads `1/8 · 250 ms`, and the Time knob reads `1/8`; without host tempo the
  caption reads `No tempo · 350 ms`.
- **Routing:** a labelled `L in` / `R in` to `L repeat` / `R repeat` path diagram makes the modes
  explicit: Standard shows linked straight paths, Dual shows separate L/R times, and Ping-pong
  crosses the feedback paths. Abstract unlabelled vectors are not sufficient.
- **Output:** input, wet and output level bars; clip remains latched until acknowledged. Freeze adds
  explicit `Held` text and activity, never hue alone.

All reserve geometry at rest, use shared visual tokens, expose semantic labels and disappear from DSP
when the editor is closed because telemetry is write-only/lossy.

## 8. Source layout removed

There is no source hardware panel: this is an original functional product. The editor also avoids
software imitations of a tape deck, early-digital rack or BBD box. Models share one task-oriented
panel; switching Model changes sound and caption, not geometry.

## 9. Responsive and quarter-4K answer

Opening starts from a measured five-card layout and is derived with
`mxm_plugin_test::opening_size::derive`; the implementation constant is not frozen by
this prose. Every derived page fits within 1920 × 1080 **physical** pixels at 100% editor zoom. The
minimum is one widest card plus gutters. At 150% and 200% the physical test window stays fixed;
cards re-page and indivisible overflow gains both-axis scrolling. No ordinary control is clipped or
hidden. Required native QA records Windows DPI 100%, 150% and 200%; Linux/macOS remain unverified on
the development machine and are reported as such. *Since the split (2026-10-06):* Linux builds and
tests run in WSL before a push, and CI builds and tests macOS on `v*` release tags or by hand;
neither is native visual QA at these scales.
