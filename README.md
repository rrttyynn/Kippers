# Kippers

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL_3.0-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Language-Rust_1.88+-orange.svg)](https://www.rust-lang.org/)
[![Protocol: UCI](https://img.shields.io/badge/Protocol-UCI-brightgreen.svg)](https://en.wikipedia.org/wiki/Universal_Chess_Interface)
[![Release: 0.0.1](https://img.shields.io/badge/Release-v0.0.1-purple.svg)](https://github.com/rrttyynn/Kippers)

> A streamlined, deterministic UCI chess engine powered by NNUE, derived from [Reckless](https://github.com/codedeliveryservice/Reckless) 0.10.0-dev.

**Kippers 0.0.1** focuses on raw search efficiency, predictability, and minimal operational overhead. By stripping optional extensions, heavy build-time tooling, and unstable background-thread mechanics (such as pondering), it provides a clean, self-contained, and reproducible chess engine for analysis and match play.

---

## Highlights

- **Embedded NNUE Evaluation**: Embeds evaluation network weights directly into the binary at compile time.
- **Deterministic Single-PV Search**: Permanently pinned to `MultiPV 1` to eliminate branch dilution and focus computational budget on the critical path.
- **Lazy SMP Scaling**: Efficient lockless multi-threading scaling from 1 core up to maximum system capacity.
- **Variant Support**: Native support for both Standard Chess and Chess960 (Fischer Random Chess).
- **Zero Bloat**: Stripped of Syzygy tablebases, NUMA thread binders, runtime tuning daemons, and ponder race conditions to ensure maximum scheduling stability.
- **Native SIMD Vectorization**: Automatically selects optimal CPU instruction extensions (AVX2, AVX-512, NEON) at compile time.

---

## Quick Start

### Prerequisites

- **Rust**: `1.88.0` or higher (`cargo` included)
- **C Linker**: `gcc`, `clang`, or MSVC
- **Make** *(optional, for Makefile workflow)*
- **curl** *(required if network weights are not provided locally)*

### Building from Source

#### Option 1: Using `cargo` (Cross-platform)

```bash
# Build optimized release binary
cargo build --release --locked --bin kippers

# Binary output: ./target/release/kippers (or kippers.exe on Windows)

```

#### Option 2: Using `make` (Optimized for host CPU)

```bash
# Compiles with native CPU architecture optimizations
make

# Binary output: ./kippers

```

### Network Weights & Offline Builds

During compilation, the build script (`build.rs`) embeds the evaluation network into the executable:

* **Default Behavior**: If `networks/v60-7f587dfb.nnue` is missing, `build.rs` automatically downloads it from the [RecklessNetworks](https://github.com/codedeliveryservice/RecklessNetworks) repository using `curl`.
* **Offline Build**: To build without internet access, place `v60-7f587dfb.nnue` into the `networks/` folder manually, or specify the file path via the `EVALFILE` environment variable:
```bash
EVALFILE=/absolute/path/to/v60-7f587dfb.nnue make

```



---

## UCI Configuration

Kippers follows the Universal Chess Interface (UCI) standard. Configure these options directly inside your chess interface:

| Option | Default | Range / Values | Description |
| --- | --- | --- | --- |
| `Hash` | `512` | `1` – `262144` (MiB) | Transposition table size in megabytes. |
| `Threads` | `1` | `1` – `Max Cores` | Number of worker threads used for Lazy SMP search. |
| `MoveOverhead` | `50` | `0` – `2000` (ms) | Safety time margin to absorb GUI/OS communication jitter. |
| `Minimal` | `false` | `true` / `false` | Suppresses verbose intermediate search depth output. |
| `UCI_Chess960` | `false` | `true` / `false` | Enables Chess960 castling rules and notation. |
| `Clear Hash` | *Button* | — | Instantly purges the transposition table. |

---

## Diagnostic & CLI Commands

In addition to standard UCI commands (`uci`, `isready`, `position`, `go`, `stop`, `quit`), Kippers includes lightweight diagnostics for benchmarking and validation:

| Command | Syntax | Description |
| --- | --- | --- |
| **Board Display** | `d` | Visualizes the current board in ASCII format alongside its FEN string. |
| **Static Eval** | `eval` | Displays the static NNUE score and estimated material breakdown. |
| **Move Validation** | `perft <depth>` | Runs perft move generation tests up to the specified ply. |
| **Benchmark** | `bench [Hash] [Threads] [Depth]` | Executes standard test suite (Default: `bench 16 1 12`). |

---

## GUI Integration

Kippers works with modern UCI-compatible chess GUIs:

* **Arena**
* **BanksiaGUI**
* **Cutechess / Cutechess-cli**
* **Nibbler**

When configuring match runners, ensure the GUI's **Ponder** setting is disabled, as Kippers is dedicated exclusively to active-turn computation.

---

## Credits & License

* **Engine Maintenance**: [Quedanomo](https://www.google.com/search?q=https://github.com/rrttyynn)
* **Original Reckless Authors**: Arseniy Surkov, Shahin M. Shahin, Styx, and upstream contributors.
* **Community & Ecosystem**: Special thanks to Stockfish, OpenBench, Fastchess, Bullet, PlentyChess, Ethereal, Berserk, and the Chess Programming Wiki.

Distributed under the [GNU Affero General Public License v3.0 (AGPL-3.0)](https://www.google.com/search?q=LICENSE).

```

```
