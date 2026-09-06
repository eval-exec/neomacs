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
`mx-tab-completion` fixture installs 1,024 identically named no-op commands
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
or background-load drift. The primary statistic is the median of the
scenario's primary metric: `per-edit-cpu-time` for `rust-lsp-typing`,
`per-completion-cpu-time` for `mx-tab-completion`, and
`per-bytecode-call-cpu-time` for `bytecode-call-loop`. The promoted editor
workflows use per-operation wall time, and sustained editing uses per-edit wall
time, so the thresholded suite tracks the campaign's wall-clock objective. At
least three samples per editor are required. The artifact reports the sorted raw samples, both
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
native profiling; `--counter-scope whole-process` includes startup. Valid run
artifacts gain typed counts for cycles, instructions, page faults, branch
misses, cache misses, L1 data-cache load misses, and data-TLB load misses, plus
the raw `hardware-counters.csv`. If perf omits or cannot support any requested
event, the run is an infrastructure failure rather than a zero-valued sample.

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
- `large-file-editing`, `indentation`, and `regex-search` isolate their named
  workloads over committed or deterministically generated source.

## Thresholded suites and history

`perf suite standard` runs interleaved comparisons for the portable catalogued
scenarios. The physical-display `sustained-native-video` workload remains an
explicit standalone run or comparison because it requires a retained input
asset and a controlled GPU/display host. Each suite scenario owns an explicit
maximum regression percentage. The
suite passes only when every child comparison is valid and no candidate median
exceeds its budget. `tmp/perf-suites/<suite-id>/suite.json` records every
threshold, observed percentage change, and immutable child comparison path.
The standard budgets are 5% for `bytecode-call-loop`, 10% for `magit-status`,
12% for `startup`, 15% for `gui-input-latency`, and 8% for every other
scenario. Pass `--previous-suite PATH` to retain lineage to an earlier suite
artifact. The new suite copies that predecessor beside `suite.json` and records
its SHA-256, suite ID, and source path; missing, malformed, or unknown-schema
history is rejected.

The `neomacs performance suite` GitHub Actions workflow runs only on a
`self-hosted, linux, x64, neomacs-perf` runner. That runner supplies the
immutable baseline binary, isolated CPU, and governor policy through workflow
inputs or `NEOMACS_PERF_*` repository variables. It builds the candidate,
enforces the host contract, collects edit-loop counters for the suite plus a
dedicated whole-process startup-counter comparison, and uploads all run,
comparison, and suite artifacts for 90 days. Hosted shared runners are
deliberately excluded from regression decisions.

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
