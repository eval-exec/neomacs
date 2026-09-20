# Org line-spacing regression: stale capture race

The existing GUI test failed at `fixture background must be colored`: the
layout JSON described the Org buffer, but the `before` PNG still showed an
empty `*scratch*` window. This reproduced both with the shell's verbose logs
and with `RUST_LOG=warn`, in about 7.7 seconds.

## Reference and controlled probes

GNU `src/dispnew.c:Fredisplay` performs redisplay (except during a keyboard
macro); `lisp/emacs-lisp/timer.el:timer-event-handler` preserves the current
buffer around callbacks. Neomacs's `builtin_redisplay` also calls redisplay;
its GUI callback submits layout work to a separate rendering thread.
A completed layout snapshot is not evidence that a newer PNG exists.

Ranked hypotheses were stale timer buffer/window state, missing renderer
submission, and a capture/presentation race. Existing logs showed Org layouts
being submitted, with commit-to-present latency approaching three seconds.
The fixture copied the live PNG after only 0.5 seconds and advanced its phase
every second, regardless of whether the prior stage had painted.

Two independent probes narrowed the cause:

- Changing only Weston to Sway made the original assertions pass (7.8s).
- Keeping Weston and slowing phase/capture timers made them pass (19.8s).

Neither changed production code. The final change retains Weston and removes
the assumption that elapsed time means presentation has completed.

## Test synchronization

Each Lisp stage writes its layout snapshot and waits for a Rust acknowledgment.
The Rust capture driver waits, with a deadline, for a complete PNG whose Org
block sample has that stage's expected background. The region is explicitly
red, distinct from the gray Org block. It saves the exact validated PNG bytes
before acknowledging the stage, preventing another race with the live PNG
writer. Incomplete JSON/PNG writes are retried. Artifacts are unique per run.

`CaptureStage::{Before, Selected, EmptyCursor}` makes stage names and expected
paint exhaustive. Existing row-height/position assertions and the complete
three-row background continuity checks remain. A spacing gap still fails;
failure to paint the region now times out with its stage and artifact path.
This is a test-driver correction, not a production Org or renderer patch.

Logs and diagnostic probes are under `tmp/org-line-spacing/`: `baseline.log`,
`quiet.log`, `sway-only.log`, `weston-slow.log`, and `handshake.log`.

## Verification

- `cargo xtask fresh-build --release` after rebasing onto upstream: passed.
- Original test: red twice; controlled Sway and slow-Weston probes: green.
- Handshake regression on the fresh executable: three consecutive passes,
  approximately 10.2 seconds each (`final-1.log` through `final-3.log`).
- Related Org silent-selection and both 4K glyph-coverage tests: 3 passed.
- Selected-stage image inspected: continuous red region through blank rows.
- Rust formatting and diff whitespace checks passed.
