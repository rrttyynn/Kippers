# Kippers 0.0.2

Kippers is a UCI chess engine maintained by Quedanomo and derived from
[Reckless](https://github.com/codedeliveryservice/Reckless) 0.10.0-dev.
It requires a UCI-compatible chess interface or match runner.

This edition retains the upstream search algorithms, NNUE evaluation, and
original network weights, while reducing optional features and build
dependencies. It does not have an independently established Elo rating.

## Features

- Standard chess and Chess960.
- Single principal variation search, permanently fixed at MultiPV 1.
- Lazy SMP parallel search, with one search thread enabled by default.
- Native UCI pondering, disabled by default.
- Dynamic time management, plus fixed time, depth, and node limits.
- Native SIMD implementations selected at compile time for the target CPU.
- Configurable hash size, search thread count, and move overhead.

Syzygy tablebases, WebAssembly, NUMA management, runtime parameter tuning,
debug statistics, and the `compiler` and `speedtest` commands are not included.

## Building

Requirements:

- Rust 1.88.0 or later, including Cargo.
- A system linker and the native toolchain required by Rust.
- Make, when using the Makefile.

Build with Make:

```sh
make
./kippers
```

Or build directly with Cargo:

```sh
cargo build --release --locked --bin kippers
./target/release/kippers
```

On Windows, the executable is named `kippers.exe`.
The Makefile and the supplied Cargo configuration target the build machine's
CPU by default. Set `RUSTFLAGS` explicitly when building for another CPU;
the resulting executable must only use instructions supported by its target.

The original network weights are not included in this repository. During
compilation, the build script downloads `v60-7f587dfb.nnue` from the upstream
RecklessNetworks repository using `curl` if the file is missing and `EVALFILE`
is unset. Alternatively, set `EVALFILE` to the path of a compatible local
network file. The selected network is embedded in the executable during
compilation.

The Makefile also provides a `make pgo` target, which requires `cargo-pgo`
and its LLVM profiling tools. Performance gains depend on the target machine
and must be measured.

## UCI Options

| Option | Default | Description |
| --- | --- | --- |
| `Hash` | 512 MiB | Transposition table size; adjustable from 1 to 262144 MiB. |
| `Threads` | 1 | Search thread count. The engine reports the supported maximum during UCI initialization. |
| `MoveOverhead` | 50 ms | Time reserved for communication and scheduling overhead; adjustable from 0 to 2000 ms. |
| `Minimal` | false | Reduce search information output. |
| `Ponder` | false | Advertise pondering availability and enable predicted-reply output. The GUI controls when pondering starts. |
| `UCI_Chess960` | false | Enable Chess960 castling notation and handling. |
| `Clear Hash` | Button | Clear the transposition table. |

Chess interfaces can override these defaults. Total process memory exceeds
the configured hash size because the engine also needs memory for evaluation,
search data, and threads. MultiPV is not adjustable; search output uses
`multipv 1`.

Example UCI session:

```text
uci
isready
ucinewgame
position startpos
go wtime 120000 btime 120000 winc 1000 binc 1000
```

The engine returns its chosen move as `bestmove`. Use `stop` to end an active
search and `quit` to exit.

## Pondering

Pondering requires a GUI or match runner that supports UCI pondering. The
engine does not start pondering on its own. Set `Ponder` to `true` in the
interface; regular search results can then include a legal predicted reply
as `bestmove <move> ponder <reply>` when the principal variation has one.

The interface sets up the position after that predicted reply and starts
`go ponder` with the usual clock arguments. While pondering, wall-clock
limits are suspended. If the opponent plays the predicted move, the interface
sends `ponderhit`: the current search continues, and its normal time budget
starts from receipt of that command. Reported search time and NPS cover the
entire search, including the pondering phase.

If the opponent plays another move, the interface sends `stop`, waits for
`bestmove`, supplies the actual position, and starts a new search. If a depth,
node, or mate limit is reached, or the position has no legal moves, the engine
waits for `ponderhit` or `stop` before returning `bestmove`. `quit` and input
closure also release this wait. An explicit `infinite` limit still requires
`stop` after a ponder hit.

Pondering can use one or multiple search threads. It uses CPU while the
opponent is thinking; it does not increase the CPU's node processing speed.
The normal search parameters and time allocation formulas are unchanged.
No fixed speedup or Elo improvement is claimed.

## Version 0.0.2

- Add native UCI `go ponder` and `ponderhit` support.
- Add a `Ponder` option, disabled by default, and legal predicted-reply output.
- Preserve commands arriving immediately after search startup and serialize
  configuration commands before readiness replies.
- Keep the 0.0.1 defaults: Hash 512 MiB, Threads 1, MoveOverhead 50 ms, and
  MultiPV permanently fixed at 1.
- Retain the original evaluation network and search parameters. The search
  implementation, evaluation, move ordering, and history code are unchanged
  from 0.0.1. Feature removals from Reckless remain in effect.
- Distribute source without bundled weights or temporary validation files.

## Diagnostic Commands

| Command | Description |
| --- | --- |
| `d` | Display the current board and FEN. |
| `eval` | Display the static evaluation and estimated piece contributions. |
| `perft <depth>` | Count legal move sequences to the requested depth. |
| `bench [Hash] [Threads] [Depth]` | Run the built-in position benchmark. |

The benchmark defaults remain 16 MiB, one search thread, and depth 12,
independently of the normal UCI defaults.

## Validation

Version 0.0.2 was validated on Linux x86-64 with Rust 1.90.0:

- 30 unit tests passed, with compilation checks for x86-64 levels v1 through v4.
- 268 deterministic search cases matched the supplied Reckless 0.10.0-dev
  source and Kippers 0.0.1 at matching settings. Reckless used MultiPV 1 with
  Syzygy disabled. The cases included both 16 MiB and 512 MiB hash settings
  and a 32-ply sequence retaining search history between moves.
- 50 depth-limited ponder searches also matched the original engine.
- Ponder protocol checks covered 1, 2, and 4 search threads, including 180
  immediate-command repetitions, terminal positions, timing transitions,
  predicted-reply output, stop, quit, and input closure. An additional 24-cycle
  prediction-hit sequence and a wrong-prediction replacement were exercised.

| Benchmark | Nodes in Reckless, Kippers 0.0.1, and Kippers 0.0.2 |
| --- | --- |
| `bench 16 1 12` | 2,837,905 |
| `bench 512 1 12` | 2,870,324 |

These checks establish agreement for the tested cases, not universal
equivalence or a measured Elo difference. No statistical strength match was
performed. Pondering deliberately adds search work when enabled by the GUI.

## Attribution

Kippers modifications are maintained by Quedanomo. This distribution contains
feature removals, engine identification changes, and changes to the default
hash size and move overhead, plus native UCI ponder support. This version was
updated on October 15, 2026.

Reckless authors include Arseniy Surkov, Shahin M. Shahin, Styx, and the
upstream contributors. Credit is retained for the upstream code and hardware
contributors, engine testers, OpenBench, Fastchess, Bullet, Stockfish,
PlentyChess, Ethereal, Berserk, and Chess Programming Wiki.

## License

Kippers is distributed under the GNU Affero General Public License, version 3.
See [LICENSE](LICENSE) for the complete license text.
