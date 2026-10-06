# mxm-fx-delay

A general-purpose stereo delay with three original architectures:

- **Clean** — transparent fractional-delay repeats.
- **Vintage digital** — fixed-memory/variable-clock range, conversion and bandwidth coupling.
- **Tape** — moving-medium speed, playback loss and transport motion coupling.

Standard, Dual and Ping-pong routing share explicit Repitch, Fade and Jump time-change laws. The
feedback loop has per-pass cuts and drive, bounded above-unity operation, output-only ducking and a
Freeze topology that holds one captured recurrence without ageing it again.

Mix is a dry/wet crossfade; zero is bit-exact Off and clears the line. Factory sounds preserve the
current Mix so insert/send balance does not change while browsing.

```bash
cargo xtask bundle mxm-fx-delay --release
```

The bundle is written to `target/bundled/mxm-fx-delay.clap` with its control map beside it.

## Licence

MIT — see [`LICENSE`](LICENSE). The implementation and constants are original; this plugin does not
claim to reproduce a named machine.
