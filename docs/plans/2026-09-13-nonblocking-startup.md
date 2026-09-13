# Nonblocking native startup

The user approved this continuation after the live-font commits were pushed.
The native event loop must run while the evaluator prepares its initial font;
the first native window must still use geometry from that exact opened font.

GNU's `src/pgtkfns.c` selects and validates the font at
`pgtk_default_font_parameter` before `gui_figure_window_size` and
`xg_create_frame_widgets`. `lisp/startup.el` initializes the window system
before `frame-initialize`. These establish the ordering to preserve, but GNU
does not supply Neomacs's separate evaluator/winit-thread lifecycle.
The source checkout was read-only at `a360712c9d272d950d8d8255ef74570f7e90b7d9`.

Use the already approved public executable/GUI/Lisp test seam. The first
regression holds a real Fontconfig file read pending using a private FIFO,
then terminates only the test's private Weston instance. Neomacs must observe
display loss and exit unsuccessfully without waiting for the font read.
The existing binary fails by timing out (20 seconds), recorded in
`target/diagnostics/issue-360/startup-fonts/nonblocking-red.log`.

The native lifecycle will distinguish waiting for preparation, preparation
ready but native surfaces unavailable, running, and startup failure. A
consuming readiness reply wakes winit after publishing geometry or failure;
dropping it during unwind also wakes winit. After success, an evaluator-owned
lifetime guard keeps completion observable until native surface creation is
allowed. A completed evaluator is joined for its authoritative exit status or
panic, rather than treating a successful early `kill-emacs` as startup failure.
Pending startup uses native wait,
with no periodic polling and no window/GPU allocation. Normal rendering starts
only after both preparation and native surface permission are available.
Commands remain queued until that point. Lisp values and opened font handles
remain evaluator-owned. A native-loop exit during pending preparation must
not join a worker that may be blocked in an external font read. An evaluator
failure still joins and propagates its original panic or typed diagnostic.

Existing missing-image, empty-font, initial-grid, scale/resize, and live-font
GUI controls will verify preserved behavior. Rebuild via the user-required
`cargo xtask fresh-build --release`. Asynchronous GPU initialization and
interrupting arbitrary evaluator-side I/O are outside this change.

Review caught and resolved two related intermediate-state cases: evaluator
panic after readiness but before surface permission, and successful early exit
in that same interval. This native surface-permission timing is reviewed from
source; the Linux GUI controls do not manufacture an unavailable-surfaces state.
The tests additionally resume the pending font read and require rendered GUI
output, and check explicit Lisp startup exit statuses 0 and 23.

Completed: 1,018 selected runtime/bootstrap tests, default and no-default-feature
compile checks, and a full fresh release build passed. The final serial native
GUI/harness suite passed all 46 tests with no skips. The first parallel run's
single compositor peer reset, isolated rerun, build fingerprint, commands and
remaining limits are recorded in the
[verification record](../diagnostics/2026-09-13-nonblocking-startup-verification.md).
