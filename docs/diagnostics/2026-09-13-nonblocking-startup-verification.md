# Nonblocking startup verification

Base: `7757f965a`, the pushed live-font continuation. Scope and GNU ordering
research: [implementation plan](../plans/2026-09-13-nonblocking-startup.md).

The native thread enters winit while the evaluator loads its runtime and opens
the initial font. It waits through native event dispatch, without a polling
timer. The renderer is constructed only when font-owned geometry and native
surface permission are both available. `PreparedGuiFrame` still owns the
opened font through logical frame installation.

Readiness success, preparation failure, and evaluator unwind publish before
waking the loop. A retained evaluator lifetime guard also exposes completion
after readiness while native surfaces are unavailable. In that case the joined
evaluator determines successful exit, nonzero exit, or panic. Native-loop loss
during pending preparation exits unsuccessfully without joining a worker that
may be stuck in external I/O.

## Regression evidence

The public executable test holds an actual Fontconfig read pending with a
private FIFO. Opening its nonblocking writer confirms the reader is present;
the test keeps the writer open without bytes, then drops only its private
Weston session. The previous executable timed out after 20 seconds. This uses
no production delay, startup-log assertion, patched compositor, or real desktop
setting mutation.

- Red: `nonblocking-red.log`, with original process output retained in
  `nonblocking-red-process/`.
- Control: releasing the same pending read reaches the smoke fixture's Lisp
  state and rendered PNG. This also passed on the prior executable before the
  new release build (`nonblocking-resume-baseline.log`).
- Explicit GUI Lisp startup exits cover both status 0 and status 23.

All logs named here are under
`target/diagnostics/issue-360/startup-fonts/`.

## Checks

The final selected runtime/bootstrap suite passed **1,018 tests**, with 251
tests skipped or excluded by selection. The initial run
had 1,017 passes and one stale clock-read allowance: removing bootstrap's old
timer also removed its clock read. The obsolete allowance was deleted, the
three time-discipline guards passed, and the full selected suite passed again.

```sh
cargo nextest run -p neomacs-display-runtime -p neomacs --lib --test-threads 8 \
  -E 'package(=neomacs-display-runtime)|(package(=neomacs)&(test(bootstrap)|test(platform_fonts)|test(input_bridge)))' \
  --no-fail-fast
cargo check -p neomacs
cargo check -p neomacs --no-default-features
cargo fmt --all -- --check
git diff --check
```

Logs: `nonblocking-final-focused.log`, `nonblocking-final-check.log`,
`nonblocking-final-no-default-check.log`, `nonblocking-final-fmt.log`.
All checks above passed. Existing compiler warnings remain.

The final production source was built with `cargo xtask fresh-build --release`,
including byte compilation and generated Lisp artifacts. Release compilation
took 7m 56s; the full xtask completed successfully. Log:
`nonblocking-final-build.log`. Executable/dump fingerprint:

```text
6C0B920D3D2E4D785A65AFFA54B645909E655F4B63E56E681FA45CFE175ED1D5
```

`lisp/ldefs-boot.el` is unchanged, SHA-256:

```text
094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138
```

An earlier build (`nonblocking-build.log`) was deliberately stopped when review
identified the evaluator-lifetime edge; it is not a successful build checkpoint.

The first native suite with two test threads passed 45 of 46 tests, including
all six startup controls. `hidpi_scale_change_never_shrinks_the_logical_frame`
lost its Weston peer connection before its final assertion. The captured
stderr reports connection reset by peer; no protocol error was captured and
the compositor log does not establish why it exited. Original artifacts are
in `nonblocking-scale-peer-reset/`. This resembles the previously recorded
peer resets but is not diagnosed as the same cause.

The unchanged scale test passed in isolation (`nonblocking-scale-isolated.log`).
The cancellation regression changed from a 20-second timeout to a successful
test in 0.160 seconds; that is evidence of prompt failure handling under this
test environment, not a general startup-performance benchmark.

The final serial GUI/harness run passed **46 tests, zero skipped**: six startup
controls, fifteen live-font tests, nine other baseline GUI/GNU controls, and
sixteen harness tests. Log: `nonblocking-final-gui-serial.log`.

```sh
FONTCONFIG_FILE=$PWD/target/diagnostics/issue-360/startup-fonts/deps/fonts.conf \
NEOMACS_GUI_SVG_LIB_DIR=$PWD/target/diagnostics/issue-360/startup-fonts/deps/svg-lib \
NEOMACS_GUI_SVG_TAG_MODE_DIR=$PWD/target/diagnostics/issue-360/startup-fonts/deps/svg-tag-mode \
cargo nextest run -p neomacs-gui-tests \
  --test desktop_font_updates --test desktop_font_startup --test startup_failure \
  --test frame_resize_oracle --test harness_contract \
  --run-ignored all --test-threads 1 --no-fail-fast
```

The private font/package dependencies and GNU oracle are the same as the
[live-font verification](2026-09-13-live-font-verification.md). The startup
implementation did not change after the final successful build and tests.

Publication integration rebased the unchanged startup patch over an incoming
frame-decoder test (`e377e4a7c`), followed by three incoming JIT commits ending
at `04d787b5d`. The first integration passed 13 selected frame tests
(`nonblocking-push-rebase.log`). The full release/GUI evidence above describes
the source before those incoming JIT changes. Their integration passed 258
selected JIT/bootstrap tests (`nonblocking-push-jit-rebase.log`, including the
JIT/VM loop benchmark).

## Review

Standards: the new test helper now uses the documented `CARGO_WORKSPACE_DIR`
input. No remaining actionable ownership or standards finding.

Spec: the retained evaluator lifetime and distinct completed-evaluator outcome
resolve the two unavailable-surfaces failure/exit-status findings. No remaining
finding in the bounded recheck.

## Limits

Native Neomacs validation is Linux Wayland; GNU startup ordering was checked in
the supplied read-only checkout. The Linux tests do not manufacture a native
surface-permission suspension, so that intermediate state's termination logic
is source-reviewed. macOS and Windows execution remain unverified here.

This change removes the readiness rendezvous on the native thread. It does not
make wgpu adapter/device initialization asynchronous, cancel arbitrary blocked
evaluator I/O, or change the existing monitor rendezvous on the evaluator.
