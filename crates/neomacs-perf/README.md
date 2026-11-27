# Neomacs performance workloads

`neomacs-perf` owns repeatable, whole-editor workloads. It complements unit
microbenchmarks and profilers: a profiler finds hot code, while this harness
replays the same realistic work and records whether a change made that work
faster without changing editor behavior.

## Which number decides

**Wall and CPU time are not verdicts. Gate on instruction counts and CPU
cycles; read wall time as a secondary signal.**

Wall time is hostage to whatever else the machine is doing -- another session's
build, a browser, a second perf run, the concurrent GC and renderer threads
competing for the pinned core. `--cpu N` pins the editor to one logical CPU but
does NOT isolate it: other work still schedules there, and last-level cache and
memory bandwidth are shared machine-wide regardless of pinning. Instruction
counts are deterministic and immune to all of it. Collect them with
`--hardware-counters`.

This is not theoretical. In a single day of campaign work, timing runs produced
three confident wrong conclusions that instruction counts would have settled
immediately:

- a change was called a 52% regression and reverted, on a comparison that
  straddled a rebase -- the regression was upstream's, not the change's;
- a real 53% regression was first dismissed as machine noise;
- a genuinely contended run was reported as a valid measurement.

**How to tell a real regression from a loaded machine.** `perf compare`
INTERLEAVES the two editors, which makes the baseline a control:

| symptom | reading |
|---|---|
| baseline in its usual band, tight MAD; only the candidate moved | real effect |
| BOTH sides inflated and both MADs blown out | contention -- discard the run |
| structural counters identical but timing moved a lot | look outside the subsystem you edited, including at what you merged |

Also: `cat /proc/loadavg` before and after every timing run, and never compare a
number against one taken from a different merge-base. Re-establish the baseline
immediately after a rebase, before measuring anything else.

**Instructions alone can mislead; read them WITH cycles.** A change that trades
instruction count for memory traffic moves the two in opposite directions. The
interpreter frame stack (`f939e1e2c`) is the worked example: it executes **8.4%
MORE instructions** and takes **10.4% FEWER cycles**, because two Vec pushes with
capacity checks cost more instructions than one 64-byte write, while removing a
48-byte load+store per call and the copy-back per return removes the stalls that
dominated. Gating on instructions alone would have called that a regression.

Both counters are stable enough to trust: across two runs an hour apart, on a
box whose load average differed by 2x, GNU's instruction count moved by 73 out
of 7,420,822,395 and its cycle count by 0.05%. Wall time over the same pair
moved 17%.

Structural counters -- `LayoutStats` frame classes, `composition_bytes_scanned`,
`buffer_snapshots_built` -- are load-insensitive too, and answer a different and
often better question: did the code path change at all? Check those before
believing a timing delta.

Run the catalogued workloads through `xtask`:

```sh
cargo xtask perf list
cargo xtask perf run rust-lsp-typing
cargo xtask perf run mx-tab-completion
cargo xtask perf run mx-tab-completion-steady
cargo xtask perf run scrolling
cargo xtask perf run bytecode-call-loop
cargo xtask perf run editing-simulation
cargo xtask perf run startup
cargo xtask perf run sustained-editing
cargo xtask perf run gui-input-latency
cargo xtask perf run org-editing
cargo xtask perf run magit-status
cargo xtask perf run large-file-editing
cargo xtask perf run indentation
cargo xtask perf run regex-search
cargo xtask perf run sustained-native-video \
  --video-file target/perf-inputs/4k60.mp4
# The sustained-native-video scenario additionally needs GStreamer discovery
# compiled into the harness. `cargo xtask perf` launches this crate with
# --no-default-features so a GStreamer-free host can still run every other
# scenario; for this one, invoke the harness directly:
#   cargo run -p neomacs-perf --features native-video -- \
#     run sustained-native-video --video-file target/perf-inputs/4k60.mp4
cargo xtask perf run rust-lsp-typing --iterations 20 --frontend tui
cargo xtask perf run editing-simulation --cpu 3 \
  --require-governor performance --hardware-counters
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
cargo xtask perf suite standard \
  --baseline-editor /opt/neomacs-perf/baseline/emacs \
  --candidate-editor target/release/neomacs \
  --samples 9 --cpu 3 --require-governor performance \
  --hardware-counters
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
GUI compositor output, and (for TUI runs) the raw ANSI byte stream. Run and
input-provenance artifacts also record the operating system, architecture,
kernel, CPU model, allowed and selected CPUs, scaling governor,
`perf_event_paranoid`, CI status, editor kind, and editor capabilities. The
capability probe distinguishes native compilation, Tree-sitter, and dynamic
module support, plus video, webview, and embedded-terminal product features,
so artifacts expose materially different builds beyond their path names.
`total_elapsed_us` includes preparation and collection;
`process-wall-time` covers only the frontend process; `workload-cpu-time`
covers the timed edit loop inside Emacs; and `workload-wall-time` measures that
same loop on the wall clock.

The GUI adapter gives Neomacs a native Wayland connection and exposes the same
headless Weston session to X11-only GNU Emacs builds through Xwayland. The
session is `neomacs-infra`'s `WestonBenchSession`: a harness-owned
compositor with a private `XDG_RUNTIME_DIR`, a pinned locale, and a
deterministic teardown, retried on a fresh compositor when a launch flakes.
The editor preserves `VK_DRIVER_FILES`, `VK_ICD_FILENAMES`, and
`VK_ADD_DRIVER_FILES` in its otherwise restricted environment and records them
in input provenance. These loader settings matter on hosts such as NixOS:
dropping them can silently select software rendering and inflate GUI timings.
The `mx-tab-completion` fixture installs 1,024 identically named no-op commands
before measurement, then performs a real `M-x`, TAB, completion-window,
selection, and minibuffer-exit lifecycle over that controlled namespace.

The Linux-only `sustained-native-video` scenario is different from the
portable GUI workloads: it uses the caller's real Wayland or X11 session so
the selected physical GPU, native decoder, compositor import, and display
cadence remain part of the measurement. Supply a locally retained 3840x2160,
60/1 or 60000/1001 fps input with `--video-file`; the harness discovers and
validates that media contract, hashes the file before and after the run, and
records its absolute path, size, display environment, editor identity, fixture
hash, embedded harness revision, build-time harness-input and runtime source cleanliness,
checkout revision, invocation, and harness-executable hash in the run
provenance. The embedded and checkout revisions must match. Acceptance runs
require an optimized Neomacs profile and a harness built and run from a clean
tracked source tree. The artifact also records the explicit GStreamer
plugin catalog used with the fresh benchmark HOME, so decoding cannot depend on
a cached per-user registry. The GUI frontend dimensions define the requested video
presentation size (1920x1080 by default), not a synthetic display size. The
harness maximizes the real window and refuses to start decoding unless the
actual window body can contain that presentation. Its default 300 observation
ticks give a 30-second sample after warmup.

A valid native-video run must identify the instantiated hardware GStreamer
decoder and plugin, the exact Vulkan adapter/driver and DRM render node, and a
hardware decoder that reports that same DRM device. This deliberately does not
claim that a downstream output allocation belongs to the decoder or is on the
same device. The independently observed compositor-import evidence must also
show NV12 or P010 frames reaching wgpu as borrowed native surfaces. A valid run
therefore requires wgpu composition, positive
decode/import/submission/presentation activity, bounded pool occupancy, and no
GPU-blit or CPU-upload fallback. The artifact reports decode and presentation
rates, p50/p95/p99/max presentation intervals, workload CPU and wall time,
timestamp-query GPU pass time when the adapter supports it, dropped/replaced/
backpressured frame counts, video GPU memory, and surface-pool allocation,
reuse, pressure, and high-water counters. Timestamp queries use a bounded
asynchronous pool and are enabled only for this explicit benchmark, so normal
playback has no query or readback cost.

## Comparing two builds

`perf compare` runs both editors once per sample and reverses their order for
each odd-numbered pair. This interleaving reduces time-order bias from thermal
or background-load drift. By default `perf compare` summarizes the scenario's
primary timing metric (`per-edit-cpu-time` for `rust-lsp-typing`,
`per-completion-cpu-time` for `mx-tab-completion`, `per-bytecode-call-cpu-time`
for `bytecode-call-loop`, per-operation/per-edit wall time for the editor
workloads); pass `--hardware-counters --gate-metric instructions` to gate on
retired user-space instructions while reporting wall/CPU time and cycles as
`[non-gating]` context (which is what `perf suite standard` does for all 22
convertible rows). At least three samples per editor are required. The artifact reports the sorted raw samples, both
medians, median absolute deviation (MAD), candidate-to-baseline ratio, and
percentage change. These are descriptive measurements, not a
statistical-significance claim; use more than the default five samples for
noisy or release-critical decisions.

Comparison artifacts live below
`./tmp/perf-comparisons/<comparison-id>/comparison.json` and link every
underlying run artifact. Child measurements remain only in those linked files;
the comparison keeps their immutable editor identity and outcome. Native-video
comparisons additionally require identical content hashes, discovered media,
fixture, presentation target, display environment, GStreamer catalog, GPU
timing mode, decoder/plugin, Vulkan adapter/driver, exact DRM node, and display
rate for every child; file paths are not used as content identity. If any run
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
`perf_event_paranoid` policy. TUI profiling uses an app-only hook inside the
harness's private PTY and does not attach to an existing tmux session. GUI
profiling likewise uses the GUI runner's app-only hook, so Weston and retry
bookkeeping do not pollute Neomacs attribution.

## Controlled hosts and hardware counters

Use `--cpu N` to pin only the editor process (not the PTY driver or Weston) to
one Linux logical CPU. `--require-governor NAME` rejects the run before launch
unless that CPU reports the requested scaling governor. The harness observes
host state; it never changes privileged global governor settings.

`--hardware-counters` wraps only the editor in `perf stat`. The default
`--counter-scope edit-loop` uses the same acknowledged enable/disable gate as
native profiling; `--counter-scope whole-process` includes startup.
`edit-loop-main-thread` and `whole-process-main-thread` count only the editor's
main thread (`perf stat --no-inherit`; batch and native-GUI routes only). In
batch mode that is the eval thread, so work moved to another thread (the
concurrent GC, a background JIT worker) is excluded: pair it with a two-core
affinity (`taskset -c 2,4 cargo xtask perf run ...` without `--cpu`) and the
fixture's `elapsed_wall_us` to see an eval-thread gain, and keep the all-thread
scope on one pinned core for the cost side. Valid run
artifacts gain typed counts for cycles, instructions, page faults, branch
misses, cache misses, L1 data-cache load misses, and data-TLB load misses, plus
the raw `hardware-counters.csv`. If perf omits or cannot support any requested
event, the run is an infrastructure failure rather than a zero-valued sample.

The editor-workload, Rust typing and scrolling fixtures have an optional diagnostic for GC parity
inside an acknowledged edit-loop counter window:

| Knob | Values and default | Behavior |
| --- | --- | --- |
| `NEOMACS_PERF_GC_WINDOW_FILE` | Absolute output path; unset by default | Writes a separate JSON file with editor PID, scenario, iteration count, four `gcs-done` snapshots and enable/disable acknowledgements after sampling stops. Requires the acknowledged sampling gate; preserves visible sustained editing when enabled. |

The GC delta is `before_disable - after_enable` only when both acknowledgements
are true, `before_enable == after_enable`, and
`before_disable == after_disable`. A collection during either handshake makes
the exact counter-window GC comparison incomplete. The existing result's
`gcs_done_start`, `gcs_done_end`, and `gcs_done_delta` still enclose both
handshakes and the edit loop; their current format stays unchanged. The
absolute `gcs_done` remains process context. Neither boundary set alone
establishes collection parity for the acknowledged counter window. Retain the sidecar hash and its recorded environment path with input
provenance; match its PID to the same editor's final stats when available.
The diagnostic does not change scenario workloads or the result schema, and
results with this instrumentation must be compared within the same configured
capture protocol.

## Editor workflow scenarios

The shared `editor-workloads.el` fixture promotes the former exploratory
editing simulation into the same correctness-gated protocol as the original
three scenarios. Every workflow records an exact operation count, elapsed CPU
time, initial/final buffer checksums, point restoration, and major-mode
identity. The promoted simulation emits typed timings for its original mode,
fontification, search, typing, replacement, indentation, kill/yank,
undo/redo, incremental search, buffer-switch, comment, counting, and motion
phases. Narrower workflows emit their applicable subset.

- `editing-simulation` is the full 13-phase exploratory simulation promoted
  unchanged in workload shape over a real Emacs Lisp source file.
- `startup` measures the clean process lifecycle plus only the minimal
  correctness reporter; it does not open or fontify the shared source fixture.
- `sustained-editing` repeats `self-insert-command`, line fontification,
  forced redisplay, deletion, and a second redisplay through a TUI by default.
- `gui-input-latency` measures each synthetic `self-insert-command` through
  forced redisplay completion under headless Weston and reports wall-clock
  p50, p95, and p99 latency. This is a portable command-dispatch-to-redisplay
  proxy shared with GNU Emacs; it does not claim OS-event ingress or compositor
  presentation feedback.
- `org-editing` changes a TODO state, aligns a table, and refontifies a
  deterministic Org document.
- `magit-status` prepares a deterministic Git repository and loads the
  revision-pinned Magit package graph before refreshing status.
- `org-journal-open` reproduces the real-user yearly org-journal stall: it
  loads the revision-pinned `org-journal`, `org-superstar`, and `git-gutter`
  package graph, opens a synthetic journal at the original's scale (~4,300
  lines) inside a Git repository whose base commit predates today's entry,
  and measures the find-file, fontify, and settle phases of each open. Pass
  `--journal-file PATH` to run against a copy of a real yearly journal (for
  example the preserved `2026-bug.org` repro file); the source is only ever
  read, and the harness re-verifies its hash after the run.
- `large-file-editing`, `indentation`, and `regex-search` isolate their named
  workloads over committed or deterministically generated source.

## Harness layout

`src/harness.rs` is the engine: editor launch, the sampling gate, capture,
artifact publication, and a one-line dispatch. Every scenario family lives in
its own child module below `src/harness/scenarios/` — preparation, result
schema, invariants, measurements, and provenance manifest together — and
`src/architecture_test.rs` pins the registry and module ceilings so the
engine cannot silently reabsorb scenario work.

### Reproducing `org-journal-open` on another machine

The scenario needs no file from the original report. On a fresh clone:

- the three packages are cloned from GitHub at the revisions pinned in
  `crates/neomacs-melpa-test-support/melpa-package-lock.tsv` on first run and
  cached under `tmp/melpa/`;
- the yearly journal is generated deterministically by the harness (constant
  seed, sized above the original report's file) into each run directory —
  `tmp/` is git-ignored, so generated journals are never tracked — and is
  retained there as the `source-fixture` artifact.

```sh
cargo xtask fresh-build --release
cargo xtask perf run org-journal-open --editor target/release/neomacs
cargo xtask fresh-build --profile profiling
cargo xtask perf profile org-journal-open --profiler perf \
    --editor target/profiling/neomacs --scope edit-loop --iterations 3
```

Package preparation byte-compiles with a GNU Emacs oracle. If the pinned
oracle checkout is absent or broken on that machine, point the oracle at any
working GNU Emacs and drop the parity pin for the session:
`NEOMACS_PARITY_REFERENCE=none NEOMACS_MELPA_ORACLE_EMACS=<emacs> cargo xtask perf …`.

## Thresholded suites and history

`perf suite standard` runs interleaved comparisons for the 23 portable catalogued
scenarios. The physical-display `sustained-native-video` workload remains an
explicit standalone run or comparison because it requires a retained input
asset and a controlled GPU/display host.

**Gate metric vs. reported context:**
- **22 of the 23 `standard` rows gate on retired user-space instructions
  (`MetricName::Instructions`)**, requiring `--hardware-counters` and rejecting
  any run that omits instructions rather than falling back to wall time:
  - 21 rows use the requested edit-loop counter scope (`CounterScope::EditLoop`):
    the 6 TUI rows (`rust-lsp-typing`, `rust-lsp-typing-heavy`,
    `mx-tab-completion`, `mx-tab-completion-steady`, `scrolling`,
    `sustained-editing`), the bytecode VM loop (`bytecode-call-loop`), and the
    14 batch editor-workload rows (`editing-simulation`, `org-editing`,
    `org-editing-heavy`, `magit-status`, `magit-status-compiled`,
    `magit-status-heavy`, `org-journal-open`, `org-journal-open-compiled`,
    `large-file-editing`, `indentation`, `regex-search`, `file-open`,
    `process-output`, `lsp-json-rpc`).
  - `startup` overrides its counter scope to `CounterScope::WholeProcess` (or
    `WholeProcessMainThread` when main-thread-only was requested) because real
    startup completes before the edit-loop gate opens.
  - Every instruction-gated row preserves the scenario's wall/CPU time and
    `CpuCycles` (with full baseline/candidate medians, MAD spreads, and paired
    deltas) in `context_metrics` and prints both `GATE` and `CONTEXT [non-gating]`
    on its own output line.
- **Non-convertible duration-property rows** (not representable by retired
  instruction count):
  - `gui-input-latency` (`SuiteGateMetric::NonGatingReport` in `standard`):
    measures the per-keystroke command-dispatch-to-redisplay `p99` tail-latency
    distribution across 1,000 `self-insert-command` + `font-lock-ensure` +
    `(redisplay t)` steps under headless Weston (capturing GC, redisplay, and
    GPU-submission tail stalls). Because clean A/A spanned `73.0%–411.6%`
    (`(max - min) / min * 100`) on `Matrix`, it is **removed from timing
    gating** and kept in `standard` as a correctness/state assertion (rejecting
    the suite if any edit or redisplay fails) while printing
    `REPORT [non-gating: input-to-redisplay-latency]` with median, MAD, and
    full sample span (`span=...%`) plus hardware-counter context.
  - `sustained-native-video` (`P99VideoPresentationInterval`, standalone):
    measures 60 Hz video decode and presentation cadence (vsync intervals).
  - Outside `neomacs-perf`: `neomacs-gui-tests`'s `continuous_120hz_scroll_*`
    (120 Hz trackpad presentation latency & <= 3.25-interval cadence) and
    `*_presents_while_command_is_stalled` / `*_have_confirmed_input_latency`
    (render-thread stall deadlines and live compositor receipt latency).

Each instruction-gated row computes its budget and drift ratchet directly in
code (`InstructionCalibration::budget_percent()` and `drift_budget_percent()`
in `src/suite.rs`) from multi-run clean A/A maxima on `Matrix` (Intel i9-14900K,
Linux `6.18.64`, CPU 0, `performance` governor; 2 full-suite `n=5` runs at
`loadavg` `53.9->25.7` and `27.6->9.4`, plus `n=10` runs on the 5 bimodal rows
at `loadavg` `20.8->31.7`, verified by two additional clean-tip `n=5` suites
including live `--previous-suite` drift at `loadavg` `52.3->37.9` and
`37.9->22.9`, both exiting `suite_rc=0`):

- `budget = ceil_to_0.5%(max(1.0%, 10 * max_rel_mad, 2 * max_span))`
- `drift  = ceil_to_0.5%(max(0.5%, 2 * max_aa_drift, max_span))` (when `< budget`)
- The `1.0%` tier floor is an explicit noise-independent engineering policy
  tolerance for normal compiler/codegen churn and instruction-for-cycle
  trade-offs, shared identically by `SuiteId::Standard` and
  `SuiteId::OrgEditingGuard`.

**Honest detection envelope:** This suite detects regressions above each row's
calibrated instruction budget (folding in all out-of-sample clean A/A runs) —
12 tight rows detect `> 1.0%`, 3 rows detect `> 1.5%`, `scrolling` detects
`> 2.0%`, `mx-tab-completion-steady` detects `> 2.5%`, and 5 bimodal /
whole-process rows only detect `> 4.0%–32.5%` (`rust-lsp-typing-heavy`,
`magit-status-heavy`, `sustained-editing`, `mx-tab-completion`, `startup`).

*Mutation isolation note:* Live RED proofs use two strictly separated throwaway
builds on `da4a1d4e58` so no hot-loop selector check contaminates other groups:
(1) a standalone `Op::Call(n)`-only build (`+6.02%` on `bytecode-call-loop`,
zero selector branch), and (2) a non-`Op::Call` template covering only the 5
low-frequency sites (`re-search-forward`, `self-insert-command`, `scroll-up`,
`main()`, `call-process`) whose inactive `slot=0` control measures `+0.00%` on
`bytecode-call-loop` and `+0.02%` on `org-editing`. Running the full 23-row
`perf suite standard` against `re-search-forward` alone fails (`suite_rc=1`)
strictly on the 4 regex-exercising rows (`org-editing` `+3.67%`,
`org-editing-heavy` `+1.61%`, `editing-simulation` `+2.69%`, `regex-search`
`+12.10%`) while `bytecode-call-loop` stays `-0.00%` (PASS).

| Scenario | Gate | Budget (drift) | Calibration A/A span (max MAD) | Clean-tip verification A/A span (`run1`/`run2`) | Smallest isolated live RED proof (zero `Op::Call` cross-talk) |
| --- | --- | ---: | ---: | ---: | --- |
| `bytecode-call-loop` | Instructions | 1.0% (—) | 0.000% (0.0001%) | 0.000% / 0.000% | `+6.02%` (standalone `Op::Call(n)` build; `-0.00%` in `g1` `suite_rc=1`; `+0.99%`/`+1.01%` unit-tested) |
| `org-editing` | Instructions | 1.0% (0.5%) | 0.068% (0.0073%) | 0.033% / 0.099% | `+1.62%` (`re-search-forward`; `+3.67%` in `suite_rc=1`) |
| `org-editing-heavy` | Instructions | 1.0% (0.5%) | 0.075% (0.0259%) | 0.050% / 0.082% | `+1.61%` (`re-search-forward`; `+1.61%` in `suite_rc=1`) |
| `editing-simulation` | Instructions | 1.0% (—) | 0.036% (0.0059%) | 0.025% / 0.017% | `+2.69%` (`re-search-forward` `suite_rc=1`) |
| `regex-search` | Instructions | 1.0% (0.5%) | 0.188% (0.0443%) | 0.130% / 0.121% | `+12.10%` (`re-search-forward` `suite_rc=1`) |
| `process-output` | Instructions | 1.0% (0.5%) | 0.148% (0.0498%) | 0.141% / 0.140% | `+1.72%` (`call-process`) |
| `magit-status` | Instructions | 1.0% (0.5%) | 0.191% (0.0640%) | 0.188% / 0.105% | `+1.68%` (`call-process`) |
| `large-file-editing` | Instructions | 1.0% (0.5%) | 0.034% (0.0085%) | 0.042% / 0.016% | large regressions only (no dedicated isolated live RED proof) |
| `indentation` | Instructions | 1.0% (0.5%) | 0.074% (0.0278%) | 0.097% / 0.052% | large regressions only (no dedicated isolated live RED proof) |
| `file-open` | Instructions | 1.0% (0.5%) | 0.037% (0.0145%) | 0.028% / 0.061% | large regressions only (no dedicated isolated live RED proof) |
| `org-journal-open` | Instructions | 1.0% (0.5%) | 0.116% (0.0101%) | 0.058% / 0.128% | large regressions only (no dedicated isolated live RED proof) |
| `org-journal-open-compiled` | Instructions | 1.0% (0.5%) | 0.157% (0.0514%) | 0.048% / 0.165% | large regressions only (no dedicated isolated live RED proof) |
| `rust-lsp-typing` | Instructions | 1.5% (—) | 0.620% (0.0567%) | 0.620% / 0.282% | `+2.20%` (`self-insert-command`; large regressions only for `< 1.5%`) |
| `magit-status-compiled` | Instructions | 1.5% (0.5%) | 0.335% (0.1442%) | 0.303% / 0.201% | large regressions only (no dedicated isolated live RED proof) |
| `lsp-json-rpc` | Instructions | 1.5% (0.5%) | 0.375% (0.1364%) | 0.207% / 0.077% | large regressions only (no dedicated isolated live RED proof) |
| `scrolling` | Instructions | 2.0% (—) | 0.649% (0.1706%) | 0.548% / 0.823% | `+2.85%` (`scroll-up`) |
| `mx-tab-completion-steady` | Instructions | 2.5% (—) | 0.470% (0.2065%) | 0.333% / 0.470% | `+3.80%` (`self-insert-command`, large regressions only) |
| `rust-lsp-typing-heavy` | Instructions | 4.0% (—) | 1.710% (0.3960%) | 1.383% / 0.325% | `+5.73%` (`self-insert-command`, large regressions only) |
| `magit-status-heavy` | Instructions | 6.0% (4.0%) | 2.427% (0.5535%) | 1.302% / 1.389% | `+8.32%` (`call-process`, large regressions only) |
| `sustained-editing` | Instructions | 11.0% (—) | 5.285% (0.1554%) | 5.285% / 3.647% | `+14.61%` (`self-insert-command`, large regressions only) |
| `mx-tab-completion` (cold) | Instructions | 13.0% (—) | 4.865% (1.2655%) | 2.261% / 2.689% | large regressions only (cold warm-up; steady row detects `> 2.5%`) |
| `startup` (`WholeProcess`) | Instructions | 32.5% (—) | 16.134% (2.9247%) | 11.052% / 16.134% | `+35.71%` (`main()`, large regressions only) |
| `gui-input-latency` | `NonGatingReport` | non-gating | 188.4% / 73.0% | 244.2% / 411.6% | non-gating (state assertion only; rejects failed redisplays) |

The suite passes only when every child comparison is valid (including
correctness/state checks on `gui-input-latency`), every gating budget is finite
and positive (failing closed to `SuiteVerdict::Rejected` on any `NaN`,
infinite, non-positive, or metric-mismatched budget/summary), and no candidate
median exceeds its calibrated budget or drift ratchet.
`tmp/perf-suites/<suite-id>/suite.json` records every threshold, gate summary,
non-gating wall/CPU context summary (including `ProcessWallTime` on
CPU-time-primary rows so all 22 converted rows carry wall-time spread),
observed percentage change, `skipped_drifts`, and immutable child comparison
path. Pass `--previous-suite PATH` to retain lineage to an earlier suite
artifact. The new suite copies that predecessor beside `suite.json` and records
its SHA-256, suite ID, and source path; missing, malformed, or unknown-schema
history is rejected. Drift ratchets verify matching suite ID, baseline editor,
counter scope, `gate_metric`, and iteration count, recording every skipped row
explicitly in `skipped_drifts` and printing a stdout `warning =` line so stale
or mismatched history never silently disarms drift detection.

The `neomacs performance suite` GitHub Actions workflow runs only on a
`self-hosted, linux, x64, neomacs-perf` runner. That runner supplies the
immutable baseline binary, isolated CPU, and governor policy through workflow
inputs or `NEOMACS_PERF_*` repository variables. It builds the candidate,
enforces the host contract, collects edit-loop counters for the suite plus a
dedicated whole-process startup-counter comparison, and uploads all run,
comparison, and suite artifacts for 90 days. Hosted shared runners are
deliberately excluded from regression decisions.

A dedicated `org-editing-guard` suite (`perf suite org-editing-guard`) gates
`org-editing` and `org-editing-heavy` directly on retired user-space
instructions (`MetricName::Instructions`) with the same `+1.0%` calibrated
budget against the frozen `v0.0.21` release asset pinned in
`.github/perf-baselines/v0.0.21.json`. Both `--hardware-counters` and
`--baseline-manifest .github/perf-baselines/v0.0.21.json` are required at the
`SuiteRequest` level (omitting either rejects the suite before launching any
editor, and any sample missing `MetricName::Instructions` is rejected rather
than falling back to wall time). The manifest verifies the SHA-256 and byte
size of both the baseline binary and its sibling portable dump before running,
threads canonical absolute paths for BOTH baseline and candidate editors
through verify, provenance, and execution, binds `--counter-scope` strictly to
`edit-loop`, resolves `perf` (and `taskset` when `--cpu` is set) to
unambiguous canonical absolute paths while rejecting `PATH` shadowing,
constructs a closed empty inherited environment for BOTH baseline and
candidate arms (no `LD_LIBRARY_PATH`, `PATH`, `NEOMACS_RUNTIME_ROOT`, or
`NEOVM_*`/`NEOMACS_*` knobs forwarded), and set-compares the full recorded
`passthrough_environment` alongside canonical executable paths, baseline
executable SHA-256/size, and loaded portable dump SHA-256/size on every
sample. `SuiteId::Standard` uses calibration-derived drift ratchets (`0.5%` on
tight rows, `4.0%` on `magit-status-heavy`) and leaves environment forwarding
untouched.

### Re-pinning the `org-editing-guard` baseline and REQUIRED-check contract

The official GitHub Release `v0.0.21` tarball pinned in
`.github/perf-baselines/v0.0.21.json` was compiled with `--profile release-pgo`,
whereas `.github/workflows/neomacs-perf.yml` builds candidates with plain
`cargo xtask fresh-build --release` (non-PGO). Measured on `Matrix`
(`Intel i9-14900K`, `N = 9`, `CounterScope::EditLoop`, `provenance = local-UNTRUSTED`),
the pure PGO-vs-non-PGO codegen offset on identical source is `+12.76%`
(`org-editing`) and `+12.58%` (`org-editing-heavy`) — more than 12× the `+1.0%`
budget. `org-editing-guard` therefore remains **INFORMATIONAL** until the
controlled `[self-hosted, linux, x64, neomacs-perf]` runner pins a same-profile
plain `--release` baseline using `scripts/ci/build-perf-baseline.sh`:

```sh
scripts/ci/build-perf-baseline.sh \
  --ref 7b3fa9d698ba41fdbabccd9bd3e547a0afa7164a \
  --version v0.0.21-release \
  --out-dir /opt/neomacs-perf/v0.0.21-release \
  --manifest .github/perf-baselines/v0.0.21.json
```

The builder clones the pinned commit cleanly (`git clone --shared`), runs
`cargo xtask fresh-build --release` inside a logged allowlisted build
environment (`pgo_profile = "none"`, `opt-level = 3`, `lto = true`,
`codegen-units = 1`, `debug = false`, `strip = "none"`, Linux `features = video`,
`rustflags = [-Cllvm-args=-align-all-functions=6, -Clink-arg=-Wl,--build-id, -Clink-arg=-Wl,-z,max-page-size=0x200000, -Clink-arg=-Wl,-z,common-page-size=0x200000]`),
records `build_profile`, `rustc_version`, `pgo_profile`, `codegen_settings`,
and exact binary/pdump SHA-256 digests in the manifest, verifies them with
`scripts/ci/verify-perf-baseline.sh`, and installs the manifest atomically
(`0644`).

**Prerequisites to flip `org-editing-guard` from INFORMATIONAL to REQUIRED:**

1. Provision and register a dedicated idle `[self-hosted, linux, x64, neomacs-perf]`
   Linux runner with `perf`, `perf_event_paranoid <= 1`, isolated CPU pinned to
   `performance` governor, and `loadavg_1m < 2.0` (`provenance = calibrated`).
2. Run `scripts/ci/build-perf-baseline.sh` on that runner to install
   `/opt/neomacs-perf/v0.0.21-release`, commit the runner-pinned
   `.github/perf-baselines/v0.0.21.json`, and configure repository variables
   `NEOMACS_PERF_V0021_BASELINE_EDITOR`, `NEOMACS_PERF_CPU`,
   `NEOMACS_PERF_GOVERNOR=performance`, and `NEOMACS_PERF_SAMPLES=9`.
3. Observe at least two consecutive green `provenance = calibrated` runs on
   `main` confirming positive headroom below `+1.0%` on both rows (never widen
   `+1.0%` to fit a measurement).
4. Add `pull_request` trigger coverage for `org-editing-guard` in
   `.github/workflows/neomacs-perf.yml` and enable the check as required in
   GitHub branch protection.

**False-positive triage and escape hatch (once REQUIRED):**

1. Re-run the failed job once (`gh run rerun --failed`) and inspect the uploaded
   `suite.json` and child `comparison.json` artifacts (`Instructions`,
   `CpuCycles`, `PerOperationWallTime`, baseline median stability, `loadavg`,
   `provenance`). If `verdict = rejected` (infrastructure/governor/perf fault)
   or the baseline arm shifted out of band, repair the runner and re-run —
   never widen `+1.0%`.
2. If a Rust toolchain upgrade shifts baseline-vs-candidate codegen, re-run
   `scripts/ci/build-perf-baseline.sh` on the controlled runner with the new
   `rustc` and commit the updated manifest in the toolchain-bump PR.
3. For a verified intentional instruction-for-cycle trade-off (fewer cycles and
   wall time at `> +1.0%` instructions) or an emergency security/correctness
   hotfix, **only a repository maintainer / release lead** may merge via
   GitHub branch-protection admin override (`gh pr merge --admin`), recording
   the `suite.json` artifact URL, cycle/wall-time evidence, and a follow-up
   re-baseline issue. Never add a workflow skip input, environment-variable
   bypass, or ad hoc threshold bump.

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

## `mx-tab-completion-steady`

The cold row's 5 timed completions sit right on top of per-process warm-up:
JIT tier entry for the completion machinery, font and obarray caches. On one
machine that made its run-to-run medians swing 12.7k-16.9k microseconds per
completion -- noise larger than the row's own 8% budget. This row runs the
same fixture, but the fixture first performs an untimed warm-up pass (five
completions, before the sampling gate) and only then opens the timing window
around the measured calls. The result proves the warm-up ran
(`warmup-completion-help-calls`); the cold row proves none ran. The two rows
are complementary: `mx-tab-completion` measures the first-completion user
experience including warm-up, `mx-tab-completion-steady` measures the
steady-state cost that warm-up otherwise buries. Never substitute one for
the other.

## `scrolling`

Page scrolling -- real `scroll-up-command`/`scroll-down-command`, the
commands C-v/M-v run -- over a deterministic 2,400-line buffer carrying the
faces a real session shows: weights, slants, boxes, spell-check waves, CJK
lines, truncation-length lines. Scrolling is the one editing gesture where
the window start moves, and no other row times it. Two phases are timed
apart: a COLD pass that displays every line for the first time (paying JIT,
layout, and face-realisation warm-up) and WARM passes down and back up over
already-laid-out rows. One operation is one scroll command with a forced
redisplay; the artifact records both phase timings and the per-command cost.
The buffer's checksum, point, and window start must survive every run.

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

## `cjk-font-lock-pulse`

A GUI frame shows a font-locked buffer of sixty distinct CJK characters.
Each iteration applies `pulse-iterations` calls to `set-face-background`
(10, the default in `lisp/pulse.el`) and calls `redisplay` after each
write, so layout sees the face change. `pulse-tick` does those writes
from a timer; this row applies them synchronously and forces the
redisplay in the timed lambda. The run is rejected unless font-lock time
is positive, the buffer still has at least sixty distinct CJK characters,
the face-write count and the redisplay count are each iterations times
10, and `pos-visible-in-window-p` showed a CJK character after every
redisplay.

This row is not in the standard suite. It has no measured baseline, and
an unmeasured row would move the board's geometric mean. It does not
report a fallback-key count.

## `process-output-carryover`

Complements `process-output` (which uses synchronous `call-process` over
one complete block) by covering asynchronous `make-process` pipe I/O,
Lisp `:filter` dispatch, sticky EOL resolution (`utf-8` -> `utf-8-dos`),
and partial-run carryover (`coding->carryover`).

Before the timed window opens, the fixture builds a ~255 KiB UTF-8/CRLF
byte stream and slices it into 64 deterministic unibyte chunks, each
`<= 4096` bytes (`PIPE_BUF` atomic write size) so every chunk lands in
one `read_process_output` call. The 63 chunk boundaries cycle across
split 2-byte (`é`), 3-byte (`あ`), and 4-byte (`😀`) UTF-8 sequences
plus trailing-CR DOS EOL lookahead (`\r | \n` once chunk 0 locks
`utf-8-dos`). Each iteration spawns a pipe `cat` subprocess with
`process-adaptive-read-buffering` bound to `nil`, streams the 64 chunks
in lockstep (`process-send-string` + `accept-process-output` until the
filter receives that chunk's exact expected character count and inserts
it into a temporary buffer), and verifies `last-coding-system-used`,
decoded character count, and buffer MD5 against the reference decode.

The run is rejected unless `type-phase-time` is positive,
`process-filter-calls` equals `iterations * 64`,
`process-carryover-boundaries` equals `iterations * 63`, and
`process-carryover-verified` is true. This row stays out of the standard
suite until it has a measured baseline so it does not move the board's
geometric mean.

## `scrolling-long-lines` and `scrolling-long-lines-guard`

Pins the Issue #702 long-line truncated scrolling band identified in the
pinned `emacs-31.1` `magit-log -n65536 --graph --decorate` region (lines
35729..44529), where `C-v 65536` measured 15.563 ms/key vs GNU Emacs 31.1
5.152 ms/key (3.02x; `overlay_live_records` = 961,030 over 200 frames) on
`8471a2c0d1` while `C-n 65536` in the same 71,459-overlay buffer stayed flat
at 1.38x GNU.

Before the timed window opens, the fixture constructs a deterministic
2,400-line buffer (120 repetitions of a 20-line length profile with exact
mean 179.30 chars, 55.0% strictly > 200 columns, max 319 chars, and dense
alternating-face `| ` graph-lane prefixes), sets `truncate-lines = t`,
`word-wrap = nil`, `hl-line-mode = 1`, a 30-column right margin, and one
`before-string` `(display ((margin right-margin) ...))` overlay per line on
a 200x50 TUI. Both CPU time and `per-operation-wall-time` bracket strictly the
cold + warm scroll phases (after buffer setup, initial `(redisplay t)`,
`(garbage-collect)`, and sampling-gate `enable`). It runs one cold pass and
`iterations` (default 2) warm down/up passes of `scroll-up-command` /
`scroll-down-command` + `(redisplay t)` with `pre-command-hook` /
`post-command-hook`, verifying buffer checksum, point/window-start
restoration, and command counts.

**Scope boundary:** `scrolling-long-lines` runs in `fundamental-mode` with
pre-propertized lines so it cleanly isolates and guards the Issue #702
layout / truncation / horizon-exhaustion / right-edge-marker / overlay-route
slice (`84.60%` of instructions and `52.08%` of cycles on `C-v 65536`). It
intentionally does **not** run `jit-lock` fontification during scrolling and
therefore does **not** guard the `RowsFontificationDemand::ensure` CoW-clone /
`IntervalTree` reallocation hotspot (`58.99%` of cache misses / `31.67%` of
cycles on 71k-line Magit logs), which requires a large-buffer lazy-fontification
/ cache-miss check or the 71k-line Magit matrix.

**Cohort load gate & guard hardening:** `scrolling-long-lines` and
`scrolling-long-lines-guard` enforce the calibrated-cohort rule: `neomacs-perf`
records `/proc/loadavg` (`1m 5m 15m`) in `HostProvenance`, `ComparisonArtifact`,
and `SuiteArtifact`, and refuses with a non-zero exit before launching any
editor when 1-minute loadavg is `>= 2.00` unless `--allow-local` is passed
explicitly. Any `--allow-local` invocation stamps `provenance = local-UNTRUSTED`
on all emitted artifacts and CLI summaries so loaded local runs cannot be
mistaken for calibrated idle-host (`treeroot`) baselines. In addition,
`SuiteId::ScrollingLongLinesGuard` enables guard provenance hardening
(`resolve_guard_tools` canonical `perf`/`taskset` resolution with `PATH`
shadowing rejection, canonical candidate resolution, narrowed TUI tool `PATH`,
and stripping of all `NEOVM_*`/`NEOMACS_*` runtime overrides; passing
`--baseline-manifest` further strips loader library paths via the full
manifest-pinned guard contract).

This row stays out of `SuiteId::Standard` so it does not move the standard
board's geometric mean. Regression gating uses `SuiteId::ScrollingLongLinesGuard`
(`cargo xtask perf suite scrolling-long-lines-guard --hardware-counters --counter-scope edit-loop`),
which gates on `MetricName::Instructions` with a **+2.0%** maximum regression
budget. Retired user-space instructions on this deterministic `EditLoop` are
load-insensitive (< 0.1% run-to-run variance, vs 5-17% wall-time wobble on
loaded TUI runs), so +2.0% catches real long-line layout/horizon/right-edge/overlay
regressions without generating flakes.

See [`docs/performance/2026-11-26-issue-702-long-line-scroll-attribution.md`](../../docs/performance/2026-11-26-issue-702-long-line-scroll-attribution.md)
for the full per-symbol cycles/instructions/cache-miss attribution and
baseline evidence.

## Adding a workload

Add a `ScenarioId` and `ScenarioSpec` (including the scenario-owned iteration
default and primary metric), committed fixtures, a harness adapter, and strict
result validation. Keep the identity enum closed: an unknown name must fail
rather than silently creating a new time series. Put unit tests in a separate
`_test.rs` file and run them with `cargo nextest`, never `cargo test`.

Use [`docs/profiling.md`](../../docs/profiling.md) for native and Lisp attribution
after a repeatable workload identifies a regression.
