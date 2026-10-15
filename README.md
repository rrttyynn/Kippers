# Kippers 0.0.1

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
- Dynamic time management, plus fixed time, depth, and node limits.
- Native SIMD implementations selected at compile time for the target CPU.
- Configurable hash size, search thread count, and move overhead.

Syzygy tablebases, WebAssembly, NUMA management, runtime parameter tuning,
debug statistics, and the `compiler` and `speedtest` commands are not included.
Ponder mode is not implemented.

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

The original network weights are not included in this repository. During compilation, the build script downloads `v60-7f587dfb.nnue` from the upstream RecklessNetworks repository using `curl` if the file is missing and `EVALFILE` is unset. Alternatively, set `EVALFILE` to the path of a compatible local network file. The selected network is embedded in the executable during compilation.

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

## Diagnostic Commands

| Command | Description |
| --- | --- |
| `d` | Display the current board and FEN. |
| `eval` | Display the static evaluation and estimated piece contributions. |
| `perft <depth>` | Count legal move sequences to the requested depth. |
| `bench [Hash] [Threads] [Depth]` | Run the built-in position benchmark. |

The benchmark defaults remain 16 MiB, one search thread, and depth 12,
independently of the normal UCI defaults.

## Attribution

Kippers modifications are maintained by Quedanomo. This distribution contains
feature removals, engine identification changes, and changes to the default
hash size and move overhead. The README and source distribution were updated
on October 15, 2026.

Reckless authors include Arseniy Surkov, Shahin M. Shahin, Styx, and the
upstream contributors. Credit is retained for the upstream code and hardware
contributors, engine testers, OpenBench, Fastchess, Bullet, Stockfish,
PlentyChess, Ethereal, Berserk, and Chess Programming Wiki.

## License

Kippers is distributed under the GNU Affero General Public License, version 3.
See [LICENSE](LICENSE) for the complete license text.
