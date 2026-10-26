# Issue #395: native focus and the editing frame

Research for [issue #395](https://github.com/eval-exec/neomacs/issues/395),
2026-10-26. The report used revision `908a8e7dd5ef18c9324c30ad7cb7e409a92fcc38`
on Windows; the maintainer also reported reproduction on Wayland. GNU source
was studied first at local revision
`a360712c9d272d950d8d8255ef74570f7e90b7d9`. Its checkout remained read-only.
Source/history findings and fresh runtime results are separated below. Native
Windows verification remains pending; Linux results do not establish it.

## GNU's contract

Native keyboard focus and the Lisp selected frame are separate state. GNU can
select a frame temporarily without changing the window manager's focus;
focus notifications are asynchronous and may arrive repeatedly or in an
unexpected order. Therefore a request to raise or focus a window is not proof
that its contents have become the VM's editing target.
[Input Focus](https://www.gnu.org/software/emacs/manual/html_node/elisp/Input-Focus.html),
[Focus Events](https://www.gnu.org/software/emacs/manual/html_node/elisp/Focus-Events.html).

`make-frame-command`, bound to `C-x 5 2`, calls `make-frame` on a graphical
terminal. It selects the new frame explicitly only on a text terminal.
Graphical creation leaves selection to subsequent native focus/input handling.
In contrast, `select-frame-set-input-focus` explicitly selects the Lisp frame,
raises it, and asks the backend for native focus. Changing graphical
`make-frame` to select unconditionally would conflate those contracts.
[Pinned frame commands](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L936-L946),
[explicit selection and focus](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L1264-L1283),
[Creating Frames](https://www.gnu.org/software/emacs/manual/html_node/elisp/Creating-Frames.html).

GNU's Windows backend maps the originating `HWND` to its Emacs frame before
emitting `FOCUS_IN_EVENT` or `FOCUS_OUT_EVENT`. PGTK similarly attaches the
frame to its focus events. Both backends report identity; they do not directly
mutate the evaluator's selected window and current buffer.
[Windows focus handling](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/w32term.c#L3275-L3334),
[PGTK focus handling](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/pgtkterm.c#L5717-L5766).

The ordinary keyboard reader provides an independent safeguard: when an input
event originates in another frame, it returns a `switch-frame` event first
and leaves the original key queued. The key must not execute against the old
selected frame merely because a focus notification was delayed. Focus
redirection is applied before that comparison.
[Input-origin switching](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/keyboard.c#L4348-L4383).

`handle-switch-frame` preserves the prefix argument and runs
`mouse-leave-buffer-hook` before invoking the shared selection operation.
`do_switch_frame` ignores deleted/tooltip destinations and preserves same-frame
behavior; its tracking argument distinguishes explicit selection from an
input-driven switch. Selecting a window also establishes its buffer and point.
[Switch transaction](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L1922-L2008),
[public selection primitives](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L2099-L2139),
[Selecting Windows](https://www.gnu.org/software/emacs/manual/html_node/elisp/Selecting-Windows.html).

## Actual toolkit and platform boundary

The workspace uses winit `0.31.0-beta.3`, patched to fork revision
`2290f91c602f8efdf102972457a25e8b6e36f90c`. The latest 0.30 documentation is
not the contract of this build. In the pinned code, `WindowEvent::Focused`
reports gained/lost focus and notes that newly created windows ordinarily
receive focus from the system later. `KeyboardInput` also identifies synthetic
held-key replay on X11 and Windows. Such replay must not manufacture a fresh
self-insert command during a focus transition.
[Pinned event contract](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-core/src/event.rs#L178-L207).

The fork's Win32 backend handles `WM_SETFOCUS`/`WM_KILLFOCUS` and sends
`Focused(true/false)` when active focus changes. `focus_window` requests native
activation only for a visible, non-minimized window that is not already in
the foreground. Its public API documents Wayland focus requests as unsupported.
Windows itself restricts foreground activation. A generic "focus requested"
boolean cannot replace an observed focus transition or an input event's source.
[Pinned Windows event handling](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-win32/src/event_loop.rs#L1177-L1199),
[Windows message cases](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-win32/src/event_loop.rs#L2422-L2437),
[pinned focus API](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-core/src/window.rs#L1429-L1446),
[WM_SETFOCUS](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-setfocus),
[SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow).

## Existing diagnosis and fixes

The report's revision contains a stateless `builtin_handle_switch_frame` that
validates the argument and returns `nil`, without selecting the destination.
That matches the historical trace: the frontend/reader supplied the correct
secondary frame, but the following self-insert still ran in the primary frame.
[Reported stub](https://github.com/eval-exec/neomacs/blob/908a8e7dd5ef18c9324c30ad7cb7e409a92fcc38/crates/neovm-core/src/emacs_core/lisp/native/builtins/stubs.rs#L806-L849).

History already contains a test-first sequence for this issue:

- [`3e9a029b8a`](https://github.com/eval-exec/neomacs/commit/3e9a029b8acc56c345b0b6a69a556424be3a5170)
  adds a native X11 regression and records three failures in NeoMacs against
  three successful GNU controls before changing production code.
- [`556f19cbe4`](https://github.com/eval-exec/neomacs/commit/556f19cbe4e54d6d057162c93b069dbdf1a083fe)
  moves input-driven selection into the shared VM transaction, including the
  selected window, current buffer, point/history updates, and redirection policy.

The existing [diagnostic note](../diagnostics/2026-09-16-frame-focus-selection.md)
also records subsequent ordering/cache and synthetic-press regressions. These
are historical evidence; current native behavior still requires fresh checks.

## Design and verification implications

Keep native window identity, native focus observation, input origin, and VM
selection distinct. Map a native window to its frame at the event boundary and
carry that identity with each key. Routing a queued key according to a later
mutable "focused frame" variable can send it to the wrong destination.
Frame/window/buffer/point selection belongs in one VM-owned transaction,
shared by explicit selection and input-driven switching.

Current code already has distinct `FrameId`/`WindowId` types and a closed
`FrameFocusTracking::{FollowSelection, Preserve}` enum for policy. Exhaustive
matching checks those policy choices at compile time. Preserve those types and
the existing transaction rather than adding backend-specific selection logic
or a second focus authority. If a boundary currently uses a zero numeric
sentinel for "current frame", an explicit target enum is a useful future seam,
but requires runtime evidence before expanding this issue's scope.
[Current selection module](../../crates/neovm-core/src/emacs_core/display/frame/selection.rs),
[frame types and tracking](../../crates/neovm-core/src/window/mod.rs).

Fresh verification should use two frames with different buffers, real
`C-x 5 2`, native activation of each window, and typing immediately after each
transition. Assert both the native focus and destination buffer contents;
matching screenshots of a shared buffer cannot detect the reported bug.
Exercise switches in both directions, key-origin switching without a fresh
focus notification, queued events for a deleted frame, prefix preservation,
redirection, and held-key transfer without duplicate insertion.

Unit and live GNU oracle controls can verify the shared selection transaction.
TUI tests verify Lisp frame-selection contracts and terminal focus sequences,
not Windows `HWND` activation or Wayland seat delivery. Linux X11/Wayland native
results do not establish that a Windows build has been run. Windows coverage
must be stated separately and, when available, observe native focus and actual
keyboard destination there rather than counting a cross-build as runtime proof.


## Fresh reproduction and an additional current defect

Re-registering the reported revision's historical stateless handler in the
current input-channel regression reproduces the selection failure: both native
focus and key-origin cases leave `FrameId(4294967296)` selected instead of the
intended `FrameId(4294967297)`. The temporary replay preserves the old validation
and no-op body, with one symbol-name API adaptation for the current value model.
This is a historical-handler replay in a real Context, not execution of the
old Windows binary. Removing that override restores the current handler; the
537 relevant selection/input/window-command tests pass. The historical handler
and original extracted body stay under `tmp/issue395/`, outside shipped tests.

Fresh GNU and NeoMacs native X11 runs pass creation, focus switching and
held-key transfer. The new private-Sway test also observes automatic activation
of the window created by real `C-x 5 2` before typing into it, then switches OS
focus between distinct buffers. Sway IPC uses the session's held `/proc` runtime
path so the long repository path cannot exceed Unix socket limits. One early
run did not observe a new window; five independent prefix probes plus exact
traced and untraced reruns succeeded. This was not treated as proof of a new
production prefix bug.

The new real-PTY test uncovered a separate current defect: after `C-x 5 2`,
`C-x 5 o` did not change the selected virtual frame. GNU produced primary `pa`
and secondary `nb`; NeoMacs kept the secondary selected and produced primary
`p` and secondary `nab`. State-file observations confirmed that this was not
merely stale terminal rendering. Both frames were live, visible and on the
same terminal.

`builtin_next_frame` and `builtin_previous_frame` were still placeholders:
they validated an optional frame and returned the selected frame regardless
of their arguments. GNU's `other-frame` therefore had no candidate to advance
to. The TUI regression failed before production changes and remains the
end-to-end RED/GREEN check for this additional defect.

## Frame traversal implementation

GNU `src/frame.c:2278–2470` supplies one candidate predicate and forward/backward
walks over the frame list. Candidates must share the origin's input ownership,
respect `no-other-frame`, and satisfy the minibuffer/visibility policy. When
none qualifies, traversal returns its origin even if that origin fails the
filter. Querying a next/previous frame does not itself select it.
[Pinned implementation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L2278-L2470),
[pinned Lisp manual](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/doc/lispref/frames.texi#L3016-L3086),
[interaction parameter contract](https://www.gnu.org/software/emacs/manual/html_node/elisp/Frame-Interaction-Parameters.html).

The implementation replaces the two placeholders with one private cycling
module. Closed `Direction` and `CandidateFilter` enums distinguish ordinary,
visible, visible-or-iconified, window-specific and unrestricted candidates.
Window filters retain `WindowId` and an optional recorded `FrameId` owner.
Deleting a window does not discard its owner identity; deleting the owner frame
does not turn that retained identity into a live frame. `None` means ownership
was not recorded. Explicit origins remain separate from the current selection.

`FrameManager::frame_list` now supplies deterministic newest-first creation
order rather than HashMap iteration order. Frame IDs are allocated by a
monotonically increasing counter without reuse, so descending IDs preserve
creation order without another mutable order ledger. The runtime has one
keyboard per terminal identity; traversal uses that existing ownership model
and keeps graphical and text-terminal domains distinct.

The initial ten public Lisp regressions cover ordering, both directions, wraparound,
explicit origins, selection purity, all candidate filters, symbol identity,
fixnum zero versus float zero, terminal isolation, fallback, and invalid
origins. The first broad run passed 751 of 752 tests; the remaining test used
`cadr`, unavailable in its minimal Context. Its assertion now uses primitive
`car`/`cdr`; no production behavior was relaxed.

## Deleted windows retain ownership identity

The reviewed traversal added one more failed-test-first correction. GNU accepts
an existing window object as the `MINIFRAME` filter even after that window has
been deleted. Window validity and ownership are separate facts. In the
three-frame fixture, the following public Lisp form must still admit
`cycle-third`, which owned the deleted window:

```elisp
(let ((doomed (split-window (frame-root-window cycle-third) nil 'right)))
  (delete-window doomed)
  (list (windowp doomed)
        (window-valid-p doomed)
        (eq (next-frame cycle-first doomed) cycle-third)
        (eq (previous-frame cycle-first doomed) cycle-third)))
```

GNU returns `(t nil t t)`. Before recording ownership at deletion, the new
traversal returned `(t nil nil nil)`; the regression failed before the
ownership fix. Looking only through currently valid windows lost the filter's
owner. The corrected `DeletedWindowRecord` captures `owner: Option<FrameId>`
before removing a window or frame; `window_owner_frame_id` consults that
record after the live-tree lookup. The regression now returns `(t nil t t)`.

`Some(owner)` records identity, not a liveness certificate. It can retain the
ID of a frame that has itself been deleted. `None` means the owner is unknown;
it must not substitute an arbitrary live frame. Candidate traversal continues
to enumerate live frames, independently of the retained owner identity.
[GNU candidate filter](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L2278-L2367),
[ownership regression](../../crates/neovm-core/src/emacs_core/display/frame/tests/cycling_test.rs),
[retained window records](../../crates/neovm-core/src/window/mod.rs).

Final review expanded this regression to removed internal parents and subtree
descendants. GNU preserves the owner of a parent discarded when its surviving
child becomes the root; it also retains the parameters of deleted descendants.
Both new tests failed before the correction: the parent lost its owner, and
the descendant parameter lookup signaled a window type error. The private tree
deletion helper now passes every actually removed, owned `Window` to one
retention callback, including wrappers discarded during recombination. The
callback records owner, old buffer and epoch, and moves parameters before the
window is dropped. Surviving nodes are not copied; there is no whole-frame
snapshot or separate predicted list of removals.
The two expanded cases failed before this correction; all 136 cycling and
window tests passed afterward.

## Final local verification and Windows limit

The product was rebuilt and its runtime images regenerated after the changes.
Its startup smoke returned `42`. All Rust test runs used `cargo nextest`.

| Layer | Final result |
| --- | --- |
| Broad core frame/keyboard/window suite (default features) | 1,552 passed |
| Cycling/window tests with desktop test features | 136 passed |
| Full display-runtime unit suite | 1,183 passed, five skipped |
| Live GNU oracle on the rebuilt product | One passed |
| Paired GNU/NeoMacs real-PTY frame-switching test | One passed |
| Native X11 and paced Wayland frame-focus tests | Two passed |
| Formatting and workflow lint | Passed |
| Windows native driver | Native CI verification pending |

The final GUI checks use real native input and distinct frame buffers. The
Wayland driver paces the prefix chord and keys; its test observes compositor
activation of the newly created frame before typing, without selecting that
frame from Lisp. Linux checks do not prove Windows activation or routing.
The final X11 check initially missed its 12-second fixture-readiness deadline
while running concurrently with the full runtime suite. No input had been
sent; its fixture state appeared afterward. An isolated rerun of the same
test and rebuilt product passed in three seconds. The earlier timeout is
retained in `tmp/issue395/gui-verified.log`, alongside the isolated result in
`tmp/issue395/x11-verified.log`.

A Windows-only `native_display` child module now uses PID-owned HWNDs,
`GetForegroundWindow` and real `SendInput` for the original `C-x 5 2` sequence
and subsequent focus switches. It asserts automatic new-frame activation,
buffer contents and selected-frame identity, uses bounded process/polling
lifetimes, and fails explicitly when native focus cannot be observed instead
of treating that as a successful skip.
[Windows driver](../../crates/neomacs-gui-tests/tests/native_display/frame_focus_windows.rs),
[native workflow](../../.github/workflows/native-display-contract.yml).

Local cross-check attempts did not reach compilation of that driver. The Nix
compiler first lacked the Windows target's `core` crate; the rustup-target
attempt then failed in `ring` while the cross C compiler encountered Linux
headers. These are cross-toolchain limitations, not a passed Windows build or
runtime test. Actual native Windows CI remains the required evidence for that
platform.
