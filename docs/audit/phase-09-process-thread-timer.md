# Phase 9 Audit: Process / Thread / Timer

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/process.c`
- `src/callproc.c`
- `src/thread.c`
- `src/emacs-module.c`
- `src/atimer.c`
- `src/timefns.c`

GNU integrates these with the command loop and event system rather than
treating them as isolated utility modules.

## Neomacs source ownership

VM/core side:

- `neovm-core/src/emacs_core/system/process/mod.rs`
- `neovm-core/src/emacs_core/system/callproc/mod.rs`
- `neovm-core/src/emacs_core/system/network/mod.rs`
- `neovm-core/src/emacs_core/runtime/threads/mod.rs`
- `neovm-core/src/emacs_core/system/timer/mod.rs`
- `neovm-core/src/emacs_core/system/timefns/mod.rs`

Host/runtime side:

- `neovm-host-abi/`
- `neovm-worker/`
- runtime communications in `neomacs-display-runtime/src/thread_comm.rs`

## Audit result

Status is **under-audited and architecturally high-risk**.

Good:

- There is a real Rust process manager and timer layer.
- The code is concentrated enough to audit.
- There is now a focused thread-specific follow-up in
  [thread-model-vs-gnu-emacs.md](thread-model-vs-gnu-emacs.md).

Bad:

- GNU couples process/timer behavior tightly to its event loop.
- Neomacs uses a more distributed runtime/worker/host architecture.
- `neovm-core/src/emacs_core/runtime/threads/mod.rs` explicitly implements a simulated
  thread model where `make-thread` is an API shim rather than GNU-equal thread
  semantics.
- `neovm-core/src/emacs_core/system/timer/mod.rs` currently owns a standalone
  `Instant`-based vector scheduler rather than a GNU-shaped timer/event-loop
  integration.
- `neovm-core/src/emacs_core/system/process/mod.rs` uses a Rust `polling::Poller` and
  direct OS child/network management, while `neovm-host-abi` and
  `neovm-worker` add a separate task/affinity/runtime layer.
- That makes Lisp-visible ordering and state transitions a real source-level
  risk, even if individual APIs look plausible.
- The focused follow-up in
  [process-timer-event-loop.md](process-timer-event-loop.md)
  now shows a better ownership story than when this audit started:
  `accept-process-output` and `sleep-for` both route through a shared
  wait/service path, sync subprocess ownership lives primarily in
  `system/callproc/mod.rs`, process callbacks use one shared runtime envelope, timer
  callbacks now preserve GNU-visible state like `deactivate-mark`, and
  short-lived children now deliver filter+sentinel in the same wait cycle.
  `read_char` also now gives ready input priority over timer/process callbacks
  instead of servicing them after input arrival, and GNU ordinary-vs-idle
  timer merge ordering now follows `timer_check_2` more closely instead of
  servicing all ordinary timers before all idle timers. Interactive
  `read-event` / `read-char` timeouts now also flow through the shared wait
  path without depending on `command_loop.running`, which restores GNU
  `sit-for` / direct-reader timeout behavior, and
  `input-pending-p t` now checks for already pending input before firing due
  timers, which restores GNU `sit-for` early-return behavior in that case.
  The remaining Phase 9 risk is narrower now: Rust `TimerManager` timers are
  still internal scaffolding rather than a published GNU-visible timer API,
  but the mixed GNU-Lisp-timer / internal-Rust-timer / process-callback order
  in the shared wait path is now regression-covered, and `input-pending-p t`
  no longer violates GNU `sit-for` `NODISP` behavior by forcing redisplay when
  it runs due timers. The shared wait path now also services pending
  non-user-visible host input such as resize events before polling timers and
  processes, which closes the earlier starvation case for
  `accept-process-output` / `sleep-for`, including input that arrives during
  the blocking wait itself. The shared wait path now also restores the
  GNU-style redraw decision when resize-like special input is serviced on a
  `do_display` path, and it no longer treats monitor-change events as a
  standalone redisplay trigger there. Window-close transport events now also
  route through the same delete-frame special-event handling as direct
  `read_char`, and now honor `throw-on-input` before that dispatch as well,
  while the default `while-no-input-ignore-events` value now includes
  `monitors-changed` like GNU's default ignore set, and `input-pending-p` now
  actually filters `monitors-changed` / `select-window` through that ignore
  set instead of only special-casing focus events. It also stages one unread
  host input event before evaluating pending input, so host keypresses are
  visible to `input-pending-p` again, and raw mouse movement is now gated by
  `track-mouse` the GNU way instead of being treated as pending input while
  direct Lisp readers still discarded it. That mouse-motion path now also has
  one shared runtime position owner: direct readers and the shared wait path
  both record the last pixel position, `display--update-for-mouse-movement`
  now matches GNU's 3-argument `(FRAME X Y)` ABI, and `mouse-position` /
  `mouse-pixel-position` now read that shared state instead of placeholder
  values. Internal `help-echo` delivery is now on that same shared path:
  mouse updates queue internal `help-echo` events, `input-pending-p` ignores
  them, direct `read-char` consumes them before returning user input, and
  `help-echo-inhibit-substitution` is honored on help strings without trying
  to autoload `help.el` from the low-level reader path. And when
  `mouse-fixup-help-message` is present on the normal startup-loaded runtime
  surface, Neomacs now runs that fixup before either echo-area display or
  `show-help-function`, matching GNU's low-level help ordering more closely.
  That low-level path no longer incorrectly depends on there being a live host
  input receiver. Window-close transport
  events now route through the same
  delete-frame special-event handling as direct `read_char`,
  instead of quitting immediately on the shared wait path. The real remaining
  work is now the
  last finer-grained redisplay/input competition edges outside those covered
  wait-path cases, not the older split-owner architecture.

## Long-term ideal design

The ideal design is:

- `neovm-core` owns Lisp-visible process, timer, and thread semantics.
- Worker/runtime/host abstractions remain transport/execution mechanisms, not
  semantic owners.
- The event loop that Lisp sees must still behave like GNU even if the host
  implementation is more concurrent internally.
- If Neomacs later uses real multithreading internally, that concurrency should
  stay below the Lisp boundary unless and until a GNU-compatible Lisp contract
  is defined for it.

## Required work

- Audit process filters, sentinels, timer firing, and
  `accept-process-output` ordering against GNU.
- Keep the Phase 9 follow-up focused on remaining ordering gaps:
  the remaining shared-wait-path redisplay/input competition outside the
  already-covered `sleep-for` / `sit-for` cases.
- Re-study GNU `thread.c` before changing `threads.rs`; the current simulated
  implementation should be treated as a compatibility placeholder, not as the
  final design.
- Make host/runtime scheduling invisible at the Lisp boundary.
- Treat process/timer/thread behavior as one event-loop subsystem in the audit,
  not three separate utilities.

## Exit criteria

- Lisp-visible process/timer/thread behavior is VM-owned.
- Host/runtime scheduling does not change semantic ordering relative to GNU.
- Differential coverage exists for filters, sentinels, shell commands, timers,
  and thread-visible behavior.
