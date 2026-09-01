# Neomacs performance workloads

`neomacs-perf` owns repeatable, whole-editor workloads. It complements unit
microbenchmarks and profilers: a profiler finds hot code, while this harness
replays the same realistic work and records whether a change made that work
faster without changing editor behavior.

Run the catalogued workloads through `xtask`:

```sh
cargo xtask perf list
cargo xtask perf run rust-lsp-typing
cargo xtask perf run mx-tab-completion
cargo xtask perf run bytecode-call-loop
cargo xtask perf run rust-lsp-typing --iterations 20 --frontend tui
cargo xtask perf compare rust-lsp-typing \
  --baseline-editor target/release/neomacs \
  --candidate-editor target/release-pgo/neomacs \
  --samples 5 --iterations 20
cargo xtask perf profile rust-lsp-typing \
  --profiler perf --editor target/profiling/neomacs \
  --iterations 100 --frontend tui --scope edit-loop
cargo xtask perf profile mx-tab-completion \
  --profiler perf --iterations 10 --frontend tui --scope edit-loop
cargo xtask perf profile bytecode-call-loop \
  --profiler perf --iterations 20000000 --scope edit-loop
```

The default editor is `target/release/neomacs`. Use `--editor PATH` to measure
another build. Frontend choices are `batch`, `tui`, and `gui`; each scenario
owns its frontend and iteration defaults. `rust-lsp-typing` and
`mx-tab-completion` default to a 40 by 120 TUI and run 100 edits and five
completions, respectively. `bytecode-call-loop` defaults to batch mode and
20 million calls. `--iterations` overrides any scenario default.

## Validity before timing

A process exit is not enough to make a performance sample. Every run writes a
strict JSON artifact below `./tmp/perf/<run-id>/artifact.json` with one of
three typed verdicts:

- `valid`, containing the measurements;
- `correctness-mismatch`, containing every failed invariant;
- `infrastructure-failure`, containing the launch or collection failure.

Measurements exist only in the `valid` enum variant. The Rust model therefore
cannot represent a mismatch as a usable performance sample. The CLI also exits
nonzero for both failure verdicts. There are no mismatch allowlists or output
normalizers.

Each run directory is self-contained enough to investigate: it retains the
scenario result, copied source and replay fixtures, exact package/grammar
provenance, the editor's executable SHA-256 and pdump fingerprint, the workload
snapshot SHA-256, package startup file, pinned Tree-sitter grammar, process and
GUI compositor output, and (for TUI runs) the raw ANSI byte stream.
`total_elapsed_us` includes preparation and collection;
`process-wall-time` covers only the frontend process; `workload-cpu-time`
covers the timed edit loop inside Emacs.

## Comparing two builds

`perf compare` runs both editors once per sample and reverses their order for
each odd-numbered pair. This interleaving reduces time-order bias from thermal
or background-load drift. The primary statistic is the median
scenario's primary metric: `per-edit-cpu-time` for `rust-lsp-typing`,
`per-completion-cpu-time` for `mx-tab-completion`, and
`per-bytecode-call-cpu-time` for `bytecode-call-loop`. At least three samples
per editor are required. The artifact reports the sorted raw samples, both
medians, median absolute deviation (MAD), candidate-to-baseline ratio, and
percentage change. These are descriptive measurements, not a
statistical-significance claim; use more than the default five samples for
noisy or release-critical decisions.

Comparison artifacts live below
`./tmp/perf-comparisons/<comparison-id>/comparison.json` and link every
underlying run artifact. Child measurements remain only in those linked files;
the comparison keeps their immutable editor identity and outcome. If any run
has a correctness mismatch, infrastructure failure, missing metric, invalid
value, duplicate metric, wrong unit, or wrong sample identity, the comparison
contains no statistics and the command exits nonzero. A faster incomplete or
incorrect workload can therefore never improve the reported candidate result.

## Finding native hotspots

`perf profile` runs the same scenario under Linux `perf record`, using a
999 Hz user-space CPU-clock event and 16 KiB DWARF call stacks. The default
`--scope edit-loop` starts perf disabled, then uses acknowledged sampling
boundaries around only the repeated editing operation. Use
`--scope whole-process` when startup and fixture loading are the subject. It
writes a typed `profile.json`, the linked instrumented scenario
`artifact.json`, raw `perf.data`, and a self-overhead call-graph report below
`./tmp/perf-profiles/<profile-id>`. A captured verdict requires a correct
scenario, successful report generation, and a nonzero reported sample count.

Use a symbol-preserving build when investigating optimized code:

```sh
cargo xtask fresh-build --profile profiling
cargo xtask perf profile rust-lsp-typing --profiler perf --iterations 100
```

The profiling command defaults to `target/profiling/neomacs`; pass
`--editor target/release/neomacs` to inspect an existing release build. Profile
metadata deliberately contains no timing measurements, and comparison commands
cannot consume profile artifacts. The linked scenario run still records its
instrumented timing for diagnosis, but that timing is not comparable to a
normal run.

For edit-loop profiles, package preparation, editor startup, fixture loading,
and the initial redisplay finish before sampling begins. A Rust gate forwards
the fixture's `enable` and `disable` boundaries to perf and replies only after
perf acknowledges each transition; an invalid transition, missing
acknowledgement, or incomplete sequence rejects the profile as an
infrastructure failure. `perf` must be installed and permitted by the host's
`perf_event_paranoid` policy. TUI profiling uses an app-only hook inside
the harness's private PTY and does not attach to an existing tmux session. GUI
profiling likewise uses the GUI runner's app-only hook, so Weston and retry
bookkeeping do not pollute Neomacs attribution.

## `rust-lsp-typing`

This workload reproduces the heavy Rust edit path that originally exposed
slow `treesit-node-at` behavior. It opens a committed full-sized source snapshot
derived from `crates/xtask/src/main.rs`,
uses `rust-ts-mode` with a revision-pinned Rust grammar, loads the locked
MELPA `lsp-mode`, replays captured diagnostics through LSP Mode, and applies
four visible diagnostic overlays derived from LSP Mode's accepted workspace
diagnostics. Every iteration invokes `self-insert-command` to insert `j` and removes `j`
between `PathBuf` and the comma, fontifies the edited line, and forces a
redisplay after each edit.

The run is rejected unless all of these remain true:

- the requested iteration count completed;
- the major mode is `rust-ts-mode` and the active parser language is Rust;
- LSP Mode loaded and the expected diagnostic overlays remain present;
- final buffer text and point are exactly unchanged.

The captured replay deliberately avoids a live rust-analyzer process, network
timing, and project discovery. It still exercises Neomacs, Tree-sitter,
fontification, LSP Mode's diagnostic update, overlays, layout, and the selected
frontend on every edit.

## `mx-tab-completion`

This workload reproduces the expensive empty-prefix command-completion path.
Each iteration sends the real command-loop input `M-x TAB`, waits until the
`*Completions*` window is presentable, then selects the no-op command `ignore`
and exits the minibuffer normally. The CPU timer covers candidate generation,
creation of the completion buffer and window, and a forced redisplay. It stops
before the oracle walks the returned candidates.

The run is rejected unless all of these remain true:

- the requested number of completion-help calls completed;
- `*Completions*` became visible in `completion-list-mode`;
- the returned candidates include `execute-extended-command` and `find-file`;
- every iteration returns the same non-empty candidate count (recorded in the
  artifact so cross-editor workload cardinality differences remain visible);
- the completion window disappeared after the command was selected;
- minibuffer depth and the originally selected buffer were restored.

No package preparation or live external service is involved. This keeps the
workload focused on the built-in obarray completion and display path that GNU
Emacs and Neomacs both execute for an empty `M-x TAB`.

## `bytecode-call-loop`

This workload isolates Tier-0 bytecode-to-bytecode call and return overhead.
It byte-compiles a one-argument identity function and a fixed-count caller,
warms both functions, collects garbage, then opens the acknowledged profiling
gate only around the repeated call loop. The harness owns `NEOVM_JIT=0`, so the
same catalog entry cannot silently switch execution tiers as local environment
variables change.

The run is rejected unless all of these remain true:

- the requested number of bytecode calls completed exactly;
- the caller and callee are byte-code functions;
- the final result is the known result `1`;
- the harness requested the Tier-0 interpreter.

The default 20 million calls provide enough work for stable comparisons and
native sampling. Use a smaller `--iterations` value for a smoke test. This is
an interpreter-core micro-workload inside the whole-editor harness: it is
useful for attributing VM call/return regressions, but it does not represent
redisplay, package, or interactive editing performance.

## Adding a workload

Add a `ScenarioId` and `ScenarioSpec` (including the scenario-owned iteration
default and primary metric), committed fixtures, a harness adapter, and strict
result validation. Keep the identity enum closed: an unknown name must fail
rather than silently creating a new time series. Put unit tests in a separate
`_test.rs` file and run them with `cargo nextest`, never `cargo test`.

Use [`docs/profiling.md`](../../docs/profiling.md) for native and Lisp attribution
after a repeatable workload identifies a regression.
