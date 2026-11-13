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
| `NEOMACS_BIN` | the same release binary | the oracle package also contains MELPA-support probes: the treesit grammar family resolves its runtime through `neomacs_binary()` = `NEOMACS_BIN`, else `<workspace>/target/release/neomacs`. With the binary in an external `CARGO_TARGET_DIR`, the fallback does not exist and all 32 treesit tests fail to spawn (`Launch NotFound`) before any comparison. The script exports the artifact under both names (and `NEOMACS_RUNTIME_ROOT` for consistency with CI). |
| `NEOVM_LIBFAKETIME_SO` | libfaketime shared object | the frozen-clock oracle tests (org-agenda today lines, capture timestamps) preload it in both engines. The nix devShell exports it. |
| `TMPDIR` | `/tmp` when unset | bounded per-case temp roots; long workspace paths are what the oracle sandbox avoids for unix sockets. |

The script records the host, tree HEAD (or "unknown" for a remote copy
without `.git`), oracle version, and the sha256 of both the binary and its
pdump in `header.txt`, so a result is attributable to an exact artifact pair.

## Fresh-build attestation (the stamp)

A binary and the Lisp tree it loads must come from the same
`cargo xtask fresh-build` run. A sync can silently strip the built `.elc`
files (an rsync `--delete` removes build outputs that are not in the source
worktree), and the editor then loads Lisp **sources** instead of the compiled
product. Every observable that depends on load-time macroexpansion changes:
gensym counters (`cps-current-state-279` vs `-5`, from the two engines
macroexpanding different numbers of forms), `gv`/`cl-evenp` definitions, and
`documentation-property` results (a compiled file carries a lazy
`("file.elc" . POS)` doc reference; a source load does not). A whole-host run
against such a tree reports "divergences" that are artifacts of the sync.

`cargo xtask fresh-build` now writes a stamp beside the binary
(`<binary>.fresh-build-stamp`) recording:

- `binary_sha256` — the SHA-256 of the binary the recipe points
  `NEOVM_BINARY_PATH` at;
- `lisp_manifest_sha256` — the **generated-Lisp manifest**: every non-dotfile
  regular file under `<runtime-root>/lisp`, sorted by relative path, hashed as
  `path\0<file-sha256>\n`. It covers sources and `.elc` alike and is
  content-based on purpose: `rsync -a` preserves timestamps, so mtimes cannot
  be evidence.

Three consumers enforce it:

1. `scripts/oracle-full-run.sh` runs `cargo xtask oracle-preflight` before
   anything else and refuses to start on a missing/mismatched stamp, a changed
   manifest, or a preloaded `.el` from `loadup.el` with no `.elc` beside it.
2. The `neovm-oracle-tests` harness verifies the same stamp at startup, on the
   path every oracle comparison goes through (`neomacs_binary_path()`), so an
   ad-hoc `cargo nextest run -p neovm-oracle-tests -E ...` is covered too.
3. The TUI oracle harness verifies it in `assert_neomacs_dumped_runtime`,
   next to the existing pdump attestation. The Lisp root there is
   `NEOMACS_RUNTIME_ROOT/lisp` when that variable selects an out-of-tree or
   staged runtime, and the workspace's `lisp/` otherwise.
4. The MELPA support resolver (`neomacs-melpa-test-support::neomacs_binary`,
   the `NEOMACS_BIN` runtime handed to MELPA and treesit probes) verifies the
   same stamp before returning the binary, so an ad-hoc nextest filter over a
   MELPA-backed oracle family cannot spawn an unattested editor either.  The
   runtime root there is `NEOMACS_RUNTIME_ROOT/lisp`, else the workspace's.

The loadup requirement is taken from `lisp/loadup.el` with GNU's own
`src/Makefile.in` rule (the parser is shared with `xtask`), never from a glob,
so the refusal names the file the build should have produced. Checking the
whole tree would cost a full hash of the 140 MiB Lisp tree plus the 80 MiB
binary in *every* test process — thousands of processes per suite — so the
first process of a nextest run performs the full check and the verdict is
cached in `<binary>.fresh-build-stamp.verified`, keyed by the stamp's own
SHA-256 and by `NEXTEST_RUN_ID`. A new run id or a new stamp re-verifies. The
cache is a cost optimization, not the authority: `cargo xtask
oracle-preflight` always verifies fully.

### The tracked `ldefs-boot.el` restore

`lisp/ldefs-boot.el` is the one tracked file the build regenerates. It is the
bootstrap autoload fallback that `loadup.el` uses only while `loaddefs.el`
does not exist yet:

```elisp
(condition-case nil
    (load "loaddefs")
  (file-error
   (load "ldefs-boot.el")))
```

The final dump runs `loadup` after `loaddefs.el` has been generated, so the
dump consumes the generated `loaddefs.el`, never the regenerated
`ldefs-boot.el`; restoring the tracked bytes after the dump cannot change
runtime behaviour. GNU's own comment in `loadup.el` keeps the tracked copy as
the repository's bootstrap snapshot (`admin/update_autogen` refreshes it
periodically, not every build). `cargo xtask fresh-build` therefore restores
the bytes it found before the generators, *after* the final dump and *before*
writing the stamp, so the stamp describes the checkout tree rather than a
generated bootstrap file. The restore needs no `.git` (a remote lane copy may
have none, or one whose HEAD is a different tree): the bytes the build started
from are the tracked bytes in a clean checkout. The local build scripts'
`git checkout -- lisp/ldefs-boot.el` is now a no-op, and `build-wt.sh` warns
if it ever is not. A deliberate post-stamp edit of `ldefs-boot.el` makes the
preflight and the harnesses refuse (pinned by a unit test).

`NEOVM_SKIP_FRESH_BUILD_STAMP=<non-empty>` is the one documented override. It
exists for deliberate experiments with a runtime the build did not produce
(a hand-made dump, a partially built tree). The value is echoed to stderr by
the verifier, and the recipe logs it, so a run log always records that the
runtime was unattested. Nothing in CI or the oracle recipes sets it.

CI note: `linux.yml` and the macOS oracle job run `cargo xtask fresh-build
--release` in the same job that then runs the oracle suite, so the stamp is
present and matches by construction. The sharded suite consumes the
`neomacs-test-runtime` artifact, which is built by a fresh-build job; that tar
now packages `target/release/neomacs.fresh-build-stamp` beside the binary and
its pdump, and every unpack step asserts the stamp is there. The harness check
is a no-op there and refuses only a sync-stripped or hand-assembled tree.

## Two-phase scheduling and the GC-stress slow group

Fifteen oracle tests run `inlined_call_semantics::check()`, which evaluates a
~39-row engine matrix including six `NEOVM_GC_STRESS=1` rows. Stress mode
collects at **every allocation-bearing safe point**, so those rows cost the
number of Lisp safe points the form executes. That cost is a property of the
**tree**: on a correctly fresh-built tree (byte-compiled Lisp — the
attestation below refuses anything else) all 15 tests measured **28.8-48.2 s
each on treeroot** in VERIFY mode, **15/15 in 100.2 s** with six running
concurrently. On a source-only tree (no `.elc`, the state a sync accident
produces) the same tests take **857 s** locally, because loading
`bytecomp.el` as source executes ~279k extra safe points; that is exactly the
tree state the fresh-build attestation now refuses, so it is not the case the
watchdog must cover.

The per-collection cost was measured independently and is linear: 1k/10k/50k
conses under stress take 1.2/7.8/34.8 s, i.e. ~0.70/0.73/0.69 ms per
allocation-bearing safe point, independent of JIT on/off, JIT threshold,
compile size and compile count; a perf profile attributes >95% of it to GC
root seeding and marking, not the Lisp workload. (`NEOVM_JIT=0` runs the same
rows identically; `NEOVM_GC_STRESS_MAJOR_EVERY=1000000` removes ~42% of the
cost by eliminating the every-8th major.)

The default profile gives the 15 tests a **240 s** watchdog (5x the
measured fresh-built worst 48.2 s, with room for contention and slower hosts)
and isolates them with `threads-required = "num-test-threads"`. The recipe
splits them out:

1. **Phase 1** — everything else, default profile.
2. **Phase 2** — the 15 tests under the `oracle-slow` profile: same 240 s
   watchdog, but a bounded 2-slot reservation (up to 12 concurrent on the
   24-thread pool), which finishes the measured group in about a minute on a
   12-core host.

`ORACLE_FULL_PHASE=1` or `=2` runs a single phase, which keeps a remote job
inside its host slot's time cap; phase 2 reuses the test binaries phase 1
built.  Phase 1's nextest "skipped" count is exactly the `-E`-filtered slow
group (15 tests), not feature skips: `cargo nextest list` with the slow filter
returns those 15 names and the complement returns 39709, so the package's
39724 tests are provably covered as 39709 (phase 1) + 15 (phase 2). The exact slow-group filter is the same in `.config/nextest.toml` and
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
