# Profiling Neomacs

Neomacs supports the GNU Emacs Lisp profiler interface and native Rust
profilers. Use the Lisp profiler to attribute work to Lisp functions. Use a
native profiler to inspect the evaluator, GC, layout, renderer, and system
calls together.

## Repeatable whole-editor workloads

Use the typed performance harness when the question is whether a realistic
editor operation became faster or slower:

```sh
cargo xtask perf list
cargo xtask perf run rust-lsp-typing --iterations 20
cargo xtask perf compare rust-lsp-typing \
  --baseline-editor target/release/neomacs \
  --candidate-editor target/release-pgo/neomacs \
  --samples 5 --iterations 20
cargo xtask perf profile rust-lsp-typing \
  --profiler perf --editor target/profiling/neomacs \
  --iterations 100 --frontend tui --scope edit-loop
```

Every attempt writes a structured bundle below `./tmp/perf`. Measurements are
published only when the fixture's correctness invariants pass; mismatches and
infrastructure failures are retained as failed artifacts and make the command
exit nonzero. See [`neomacs-perf/README.md`](../crates/neomacs-perf/README.md) for the
workload contract, collected files, and metric definitions.

Use `perf profile` when timings show a regression but do not explain it. The
Linux-only command samples the editor/application process tree and retains both
raw `perf.data` and a text hotspot report below `./tmp/perf-profiles`. Prefer a
`profiling` build, which preserves symbols without replacing the release
binary:

```sh
cargo xtask fresh-build --profile profiling
```

Profile runs are deliberately diagnostic: instrumentation changes timing, so
their metadata carries no comparable measurements and cannot be fed into
`perf compare`. The linked scenario artifact still proves the same correctness
invariants passed. The default `--scope edit-loop` starts perf disabled and
uses acknowledged boundaries around only the repeated editing operation, after
package preparation, editor startup, fixture loading, and initial redisplay.
Use `--scope whole-process` when those startup phases are the investigation
target. Missing acknowledgements and invalid boundary sequences reject the
profile instead of silently widening its scope.

## Emacs Lisp profiler

The standard commands and functions are available:

```elisp
(profiler-start 'cpu)       ; also accepts 'mem or 'cpu+mem
;; Run the workload.
(profiler-stop)
(profiler-report)
```

The lower-level `profiler-cpu-*` and `profiler-memory-*` primitives return the
same hash-table shape consumed by GNU `profiler.el`. `profiler-log-size` and
`profiler-max-stack-depth` default to 10000 and 16.

CPU profiling uses per-thread CPU time and samples cooperatively at Lisp call
boundaries. This avoids asynchronous signal handlers entering the Rust
runtime. A long-running function is charged when it next crosses a call
boundary. Memory profiling measures bytes reported at Neomacs' managed Lisp
object-allocation points. It does not include arbitrary Rust allocations or
every later capacity change in an object's backing storage.

Set `HEAP_REPORT_DIR` on the Doom RSS harness to save one exact
`neomacs--heap-layout-stats` snapshot per run alongside the Linux process
metrics. This separates managed live data and arena fragmentation from
allocator and non-GC Rust storage without changing the measured binary:

```sh
HEAP_REPORT_DIR=target/profiling/doom-heap-layout \
  scripts/profile-doom-memory.sh
```

Set `SMAPS_REPORT_DIR` to retain the matching full `/proc/PID/smaps` snapshots.
Those snapshots distinguish anonymous heap and allocator mappings from the
binary, pdump, shared libraries, fonts, and other file-backed mappings.

For configurations that continue loading from idle timers after command-line
processing, set `PROFILE_DELAY_SECONDS` to delay the explicit GC, heap snapshot,
and readiness marker until that work has settled. The delay is intentionally
opt-in because it is included in the harness's startup timestamp.

Set `NEOMACS_EAGER_GNU_BYTECODE=1` to keep validated GNU bytecode decoded at
load time. This diagnostic switch provides a same-binary control for measuring
the resident-memory and execution-latency effects of deferred decoding.

## Native sampling

The `profiling` Cargo profile keeps release optimizations and native debug
symbols:

```sh
cargo build --profile profiling -p neomacs
```

On Linux, record the complete process with `perf` or Samply:

```sh
perf record --call-graph dwarf ./target/profiling/neomacs -Q
perf report

samply record ./target/profiling/neomacs -Q
```

On Windows, build the same profile and record the process with Windows
Performance Recorder, then inspect the ETL trace in Windows Performance
Analyzer. This captures CPU scheduling, native stacks, allocation providers,
file I/O, and GPU activity when the corresponding WPR profiles are enabled.

```powershell
wpr -start CPU -filemode
.\target\profiling\neomacs.exe -Q
wpr -stop neomacs.etl
wpa neomacs.etl
```

## Native heap profiling with TiKV jemalloc

Neomacs uses mimalloc by default. For an ordinary jemalloc build, disable the
default allocator and select the `jemalloc` feature:

```sh
cargo build --release -p neomacs --no-default-features \
  --features video,jit,jemalloc
```

The `mimalloc` and `jemalloc` features are mutually exclusive. The separate
`jemalloc-profiling` feature includes `jemalloc` and compiles jemalloc's heap
profiler. Keep full native debug information in an isolated target directory
so `jeprof` can symbolize allocation stacks:

```sh
CARGO_TARGET_DIR=target/jemalloc-profile \
CARGO_PROFILE_RELEASE_DEBUG=true \
CARGO_PROFILE_RELEASE_STRIP=none \
  cargo build --release -p neomacs --no-default-features \
    --features video,jit,jemalloc-profiling

RUST_LOG=warn cargo xtask fresh-build --release --skip-build \
  --no-byte-compile --bin-dir target/jemalloc-profile/release
```

TiKV jemalloc uses a private `_rjem_` symbol prefix, so its configuration
variable is `_RJEM_MALLOC_CONF`, not `MALLOC_CONF`. Enable sampling and periodic
64 MiB allocation-interval dumps like this:

```sh
mkdir -p target/profiling
_RJEM_MALLOC_CONF="prof:true,prof_active:true,lg_prof_sample:19,lg_prof_interval:26,stats_print:true,prof_prefix:$PWD/target/profiling/neomacs-heap" \
  target/jemalloc-profile/release/neomacs
```

`stats_print:true` writes jemalloc's allocated, active, metadata, resident, and
mapped totals when Neomacs exits normally. Those totals help distinguish live
heap allocations from allocator fragmentation and retained pages when RSS is
larger than the `jeprof` in-use total.

Analyze a dump against the exact binary that produced it:

```sh
jeprof --inuse_space --text target/jemalloc-profile/release/neomacs \
  target/profiling/neomacs-heap.<pid>.<sequence>.heap
```

Avoid `prof_gdump` for Neomacs startup profiling. A dump raises the process's
virtual-memory high-water mark, which can immediately trigger another dump and
cascade. Interval dumps do not have that feedback loop. See the
[jemalloc profiling options](https://jemalloc.net/jemalloc.3.html) for the
meaning of `prof`, `prof_active`, `lg_prof_sample`, and `lg_prof_interval`.

## Doom resident-memory A/B

On Linux, Neomacs defaults mimalloc arenas to commit-on-demand because it
retains substantially less resident memory after Doom startup. Set
`MIMALLOC_ARENA_EAGER_COMMIT=2` to restore mimalloc's upstream eager-commit
policy for workloads that prefer its small startup-time advantage.

After a release fresh build, compare commit-on-demand with mimalloc's Linux
eager-commit behavior against the same installed Doom configuration:

```sh
cargo xtask fresh-build --release
scripts/profile-doom-memory.sh
```

The harness interleaves five runs of each mode, waits for its readiness marker
and an explicit Lisp GC, idles for 30 seconds, and records `/proc/PID/smaps_rollup`
RSS, PSS, private, anonymous, and swap counters. Raw samples go to
`target/profiling/doom-memory.tsv`. It also writes paired startup deltas to the
adjacent `.startup-pairs.tsv` file. Override `RUNS`, `SETTLE_SECONDS`,
`PROFILE_DELAY_SECONDS`, `HEAP_REPORT_DIR`, `SMAPS_REPORT_DIR`,
`LOG_REPORT_DIR`, `GC_REPORT_DIR`, `STARTUP_TIMEOUT_SECONDS`, `NEOMACS_BIN`,
`REPORT`, or `STARTUP_REPORT` when needed.

Neomacs also caps the collector's allocation interval at 4 MiB through GNU's
`normal-top-level` and a 30-second settling window for deferred startup work.
This bounds peak arena fragmentation while leaving Lisp's explicit threshold
unchanged once startup has settled. For controlled experiments, set
`NEOVM_GC_THRESHOLD_CAP_BYTES` to measure the startup-time versus settled-RSS
curve without editing the config. This is an opt-in profiling control; normal
runs apply only the startup ceiling and otherwise honor Lisp's
`gc-cons-threshold` exactly.

Memory runs intentionally settle long enough to stabilize RSS, so use a
separate higher-repetition panel to enforce startup neutrality. This example
accepts at most a 5% median paired regression; the gate requires at least 20
A/B pairs:

```sh
RUNS=20 SETTLE_SECONDS=0 MAX_STARTUP_REGRESSION_PCT=5 \
  REPORT=target/profiling/doom-startup.tsv \
  scripts/profile-doom-memory.sh
```

For interpreter opcode frequencies, use the existing zero-default-overhead VM
instrumentation:

```sh
cargo nextest run -p neovm-core --features vm-profile --release \
  -E 'test(/vm_subr_mix_(byte_compile|fontlock)/)' --no-capture
```

The full in-tree performance panel is available at
`crates/neovm-core/scripts/run-perf-suite.sh`. It collates every bench into one
timestamped report — JIT micro-benches (interp-vs-JIT, hot-vs-cold in one
process), GC drain-kind profiles (plain-vs-pdump A/B), allocation
round-trip probes, per-builtin VM call rankings, and real-boot AOT A/B
startup samples — so before/after numbers stop being hand-assembled. The
`vm-profile` instrumentation builds in its own feature configuration,
deliberately kept OFF for the timing sections so their numbers stay honest.

## Methodology

The tools above answer "what is hot"; the discipline below is what has made
the answers trustworthy. Every rule here was learned the hard way in a real
campaign (see the case studies).

### Deterministic workloads, batch-replayed

Interactive hot paths (font-lock, redisplay motion, reader-heavy startup)
are extracted into scripted `--batch`/`--eval` drivers that replay the same
buffer and the same operations every run. Repeatable numbers, clean stacks,
no event-loop noise drowning the profile. Profiling the live GUI session is
a last resort; replaying its workload headlessly is the default.

### Fresh build before every measurement

Profile the binary you actually built. An incremental `cargo build
--release` also invalidates the pdump fingerprint (regen with
`cargo run -p xtask -- fresh-build --release --skip-build`), and a stale
binary next to freshly edited source produces confident nonsense — the
"binary-mismatch trap". Corollary: measure BEFORE pushing; a perf claim
that was never re-measured on the final build is not a claim.

### A/B with medians, one variable at a time

Every number is old-vs-new medians over repeated runs on the same machine,
changing exactly one thing. The perf-suite benches encode this
(interleaved A/B inside one process where possible, so frequency scaling
and cache state hit both sides equally).

### Timeline questions get tracing, not a profiler

When the question is "when did X stop happening" rather than "what is
hot", use the wired-up tracing subscriber with a module-scoped filter —
e.g. `RUST_LOG=neomacs_renderer_wgpu::shader_surface_cache=trace` — and
read the timestamped timeline. No code changes, no printf, and log levels
stay untouched.

## Case studies

Real findings, each invisible in code review and obvious in a profile:

- **Gap-buffer byte↔char conversion was O(n²)** under font-lock: a
  batch-replayed font-lock driver took 4 minutes on a large buffer;
  `perf` put the time in position conversion. Fixed by porting GNU
  `marker.c`'s cached-anchor scheme: 4 min → 2 s.
- **`Value == Value` is deep equality**, so `HashSet<Value>` /
  `Vec<Value>::contains` deep-compare; a symbol-materialization loop went
  O(n²) on Doom's obarray. Flamegraph made it unmissable. Identity sets
  keyed on `bits()`: Doom startup 2.2 s → 0.52 s.
- **Doom startup phase ranking** via `perf stat` PMU breakdown + dwarf
  flamegraphs: face resolution 5.4% → 0.64% (face-list cache), reader
  8.5% → 6.4% (decode cache + contiguous-slice decode).
- **pdump load**: property-free strings made self-contained, skipping the
  object-extra table — raw `load_from_dump` median −60%, measured as
  medians over repeated loads, not single runs.
