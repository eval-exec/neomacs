# #390: display teardown before close confirmation

## Reproduction

On Linux/X11, launch `-Q`, visit a new file, insert text without saving, and
send the top-level window a `WM_DELETE_WINDOW` client message. The old runtime
exits its render event loop immediately. Lisp then tries to show the Unsaved
Buffers dialog and reports:

```
failed to show popup menu: sending on a disconnected channel
```

The process remains alive. Two isolated Xvfb runs reproduced the exact error;
a clean-buffer control exited successfully. This reproduces the cross-platform
shutdown failure, not Windows's specific ghost-window presentation.

`cargo nextest run -p neomacs-gui-tests --test close_confirmation` captured a
passing clean-close control and a failing unsaved-close regression before the
production change. The failing assertion names #390 and preserves artifacts
under `./tmp/neomacs-gui-tests/close-*`.

## GNU reference and ownership

GNU `src/xterm.c` converts `WM_DELETE_WINDOW` into `DELETE_WINDOW_EVENT`; it
does not destroy the window there. `lisp/frame.el:handle-delete-frame` decides
whether to delete a frame or invoke `save-buffers-kill-emacs`. The latter, in
`lisp/files.el`, can show save/discard/cancel dialogs before termination.

Neomacs already handed the event to Lisp, but its live-window event handler
also exited the entire render loop for the primary frame, or immediately
removed a secondary native frame. Those actions preempted the Lisp decision.

A close *request* now only notifies Lisp. Accepted Lisp frame deletion still
uses `WindowCommand::DestroyWindow`; evaluator exit still drives final display
shutdown. Actual native destruction remains a terminal event. Cancelling GPU
startup before a usable display exists retains its existing terminal behavior.

`RenderShutdownReason` is the small typed boundary for irreversible teardown:
`EvaluatorShutdown`, `NativeWindowDestroyed`, or `StartupCancelled`. The private
optional reason replaces a public shutdown boolean. There is deliberately no
ordinary native-close-request variant. A separate pending-close state would
incorrectly latch cancellation unless Lisp explicitly acknowledged every abort;
no such state is needed because Lisp already owns the decision and can receive
subsequent requests.

## Regression coverage

The final native Wayland tests use an isolated Sway compositor. Its IPC sends
`xdg_toplevel.close`; wtype supplies keyboard events, and grim records actual
confirmation pixels. They require:

- Clean close exits successfully.
- Unsaved close maps a real popup, Cancel preserves buffer contents and
  modification state, and a subsequent keyboard command executes.
- A second close can discard changes and exit without creating the file.
- A secondary frame remains native and usable when its Lisp special-event
  handler declines deletion.

The initial X11 regression caught the exact disconnected-channel error before
production changes. After the fix it exposed a separate existing limitation:
the runtime's X11 backend does not implement native popups. Thus the complete
save/cancel/discard test uses Wayland, matching the existing native-menu tests.
The original X11 replay no longer reports a disconnected channel; it instead
reports the independent unsupported-popup error. This fix does not claim to
implement X11 popup support or reproduce Windows's ghost-window presentation.

Frames are selected by explicit fixture names and editor PID. Editor processes
and display sessions are owned by RAII guards; artifacts stay under `./tmp/`.
A harmless modifier press activates each new virtual keyboard before meaningful
input: the first press can otherwise arrive only in `wl_keyboard.enter`'s
held-key array when Sway switches devices.

Verification: all 535 render-thread tests passed, the full Linux
`cargo xtask fresh-build --release` completed, and all three native close tests
passed (clean exit; save/cancel/discard; secondary-frame close declined by Lisp).
Eight existing native-menu and shutdown/startup GUI tests also passed.
Both code-review axes reported no remaining actionable findings. Native
Windows runtime verification remains pending.
