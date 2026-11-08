# Whole-host oracle run

`scripts/oracle-full-run.sh` runs the complete `neovm-oracle-tests` package
once, on one host, against that host's own pinned GNU Emacs. It is the
reproducible form of the ad-hoc oracle batches: same binary class, same oracle
mode, same watchdog sizing, and the same two-phase scheduling for the
GC-stress tests.

```sh
# inside nix develop (provides NEOVM_LIBFAKETIME_SO), at a batch tip that has
# already run: cargo xtask fresh-build --release
scripts/oracle-full-run.sh [LOGDIR]
```

`LOGDIR` defaults to `target/oracle-full-run/<UTC timestamp>` and receives
`header.txt`, `phase1.log`, `phase2.log` and `summary.txt`.

## Required configuration

| Input | Value | Why |
|---|---|---|
| `NEOVM_BINARY_PATH` | `target/release/neomacs` with `neomacs.pdump` beside it | The release **pdump** binary. Debug binaries have no pdump and boot through loadup: 10-30x slower per oracle child (measured in batch 2f: trivial oracle passes 7-20 s under the debug binary vs 0.8-2.2 s with release) and they SIGABRT on arm64 deep recursion before the JIT stack guard can fire. The script rejects `/debug/` paths and a missing `.pdump`. |
| `NEOVM_FORCE_ORACLE_PATH` | the pinned GNU Emacs **31.1** binary | must be `emacs-31.1` = `a360712c9d272d950d8d8255ef74570f7e90b7d9`. The script refuses any other version rather than silently re-baselining against the wrong oracle. |
| `NEOVM_ORACLE_MODE` | `verify` (default) | `verify` runs GNU **on this host**, asserts the checked-in expectation still matches it, and compares every Neomacs row against that live GNU result. `snapshot` is the fast CI default and never spawns GNU; using it for a whole-host run compares a macOS/arm64 host against a Linux-recorded expectation. The script refuses `snapshot`. |
| `NEOVM_LIBFAKETIME_SO` | libfaketime shared object | the frozen-clock oracle tests (org-agenda today lines, capture timestamps) preload it in both engines. The nix devShell exports it. |
| `TMPDIR` | `/tmp` when unset | bounded per-case temp roots; long workspace paths are what the oracle sandbox avoids for unix sockets. |

The script records the host, tree HEAD (or "unknown" for a remote copy
without `.git`), oracle version, and the sha256 of both the binary and its
pdump in `header.txt`, so a result is attributable to an exact artifact pair.

## Two-phase scheduling and the GC-stress slow group

Fifteen oracle tests run `inlined_call_semantics::check()`, which evaluates a
~39-row engine matrix including six `NEOVM_GC_STRESS=1` rows. Stress mode
collects at **every allocation-bearing safe point**, and the rows are
inherently expensive: measured on the release pdump binary in `verify` mode,
one whole test takes **857 s** (one stress row alone replays to the exact
expected value in ~236 s). The measured cost is linear in allocation-bearing
safe points — 1k/10k/50k conses take 1.2/7.8/34.8 s, i.e. ~0.7 ms per
collection — and independent of JIT on/off, JIT threshold, compile size and
compile count; a trivial `(byte-compile ...)` executes ~279k such safe points
(~135 s of it in `require 'bytecomp`). The time is GC root seeding and
marking, not the Lisp workload.

The default profile therefore gives the 15 tests a 2400 s watchdog (2.8x the
measured 857 s) and isolates them with `threads-required = "num-test-threads"`;
that is correct but serializes ~3.6 CPU-hours if a plain full-suite run
reaches them. The recipe splits them out:

1. **Phase 1** — everything else, default profile.
2. **Phase 2** — the 15 tests under the `oracle-slow` profile: same 2400 s
   watchdog, but a bounded 2-slot reservation (up to 12 concurrent on the
   24-thread pool), which finishes the group in tens of minutes on a whole
   12-core host.

`ORACLE_FULL_PHASE=1` or `=2` runs a single phase, which keeps a remote job
inside its host slot's time cap; phase 2 reuses the test binaries phase 1
built. The exact slow-group filter is the same in `.config/nextest.toml` and
the script: the `inlined_call_semantics::` module,
`backtrace_frame_semantics::oracle_inline_` (o1/o2), and
`debug_on_next_call::oracle_inline_` (o3/o4).

## Per-family watchdogs

`.config/nextest.toml` carries the per-family overrides, with the measurement
that sized each one in its comment. The rule that matters when editing them:
**nextest applies only the FIRST matching override**, so a narrow family rule
must sit above the package-wide `package(neovm-oracle-tests)` rule (60 s
single-form hang watchdog). The heavy oracle families currently overridden
are minimax, ucs-normalize / mixed-int-float / consp-atom-listp, the full
regex sweep, the window-tree generative test, treesit grammar loading, and the
GC-stress matrix group.

Watchdog sizing is evidence-based: derive it from the measured release time
and leave a bounded margin large enough for contention, not an open-ended
value. `--final-status-level slow` in phase 1 lists every test that crossed
the 60 s slow threshold with its duration, which is the measurement to use
when a new family needs an override.

## Knob audit: no vacuous engine rows

Every engine knob the oracle matrices set has a production reader. In
particular `NEOVM_JIT` is live: `jit_runtime_enabled()` (`jit/mod.rs`) gates
tier-up at `bytecode/vm.rs` (`BytecodeTierPolicy::for_process` and the OSR
gate), and on the release pdump binary `NEOVM_JIT_STATS_FILE` shows
`compiles=839 native_entries=84276 dispatch=84698/85718` with `NEOVM_JIT=1`
versus `compiles=0 native_entries=0 dispatch=0/0` with `NEOVM_JIT=0`. A
kill-switch cannot change Lisp *values*, so "same result" is not evidence of a
no-op. The only names the oracle crate references without a runtime reader are
harness/fixture knobs with their own consumers (`NEOVM_ORACLE_*`,
`NEOVM_BINARY_PATH`, `NEOVM_LIBFAKETIME_SO`, `NEOVM_WINDOW_PROP_CASES`,
`NEOVM_ACTIVE_MODE_LINE_RESULT`, the `NEOVM_TEST_*` strings inside test data).
Watchdog sizing must not treat any of the engine rows as vacuous.

## Limitations

- The GC-stress group is dominated by collect-at-every-safe-point cost that is
  inherent to the stress mode, not a superlinear engine bug. Until the matrix
  or the stress mode changes, phase 2 is a tens-of-minutes cost on a whole
  host and a multi-hour cost when serialized. Trimming duplicate GC-stress
  rows is a coverage decision for the oracle owners, not a watchdog change.
- A run with a non-31.1 `emacs` is refused; use the pinned build.
- The script does not build anything. Run `cargo xtask fresh-build --release`
  first at the batch tip, and keep the tree and its pdump matched.
