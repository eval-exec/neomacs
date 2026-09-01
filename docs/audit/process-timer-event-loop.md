# Process / Timer / Event Loop Audit

**Date**: 2026-03-30
**Status**: Neomacs now has a shared wait/service path, a real `callproc`
owner, shared callback envelopes for both process callbacks and timer
callbacks, GNU-shaped merged ordering between ordinary and idle GNU Lisp
timers, regression coverage for the mixed GNU-Lisp-timer /
internal-Rust-timer / process-callback order, and GNU-compatible
`sit-for`/`input-pending-p` `NODISP` behavior. Phase 9 risk is now narrower:
mostly the remaining shared-wait-path redisplay/input competition outside the
already-covered `sit-for` cases.

## GNU Emacs Design

Primary GNU ownership:

- `src/process.c`
- `src/keyboard.c`
- `src/atimer.c`
- `src/callproc.c`
- `src/timefns.c`

The important design point is that GNU does not treat subprocess I/O, timers,
and input waiting as separate utility layers.

- `accept-process-output` in `process.c` delegates to one runtime owner:
  `wait_reading_process_output`.
- `wait_reading_process_output` runs timer checks, waits for process/network
  state, notices input, and services filters/sentinels.
- Lisp timers are owned by the keyboard/input side through `timer_check`,
  `detect_input_pending_run_timers`, and `do_pending_atimers`.
- Process filters and sentinels run under a shared async-callback envelope that
  preserves current buffer, match data, and `waiting_for_user_input_p`, and
  inhibits quit while the callback runs.

Relevant GNU references:

- [process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L4865)
- [process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L5332)
- [process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L6518)
- [process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L7789)
- [keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L4718)
- [keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L12009)
- [atimer.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/atimer.c#L451)

## Neomacs Current Design

The equivalent Neomacs ownership is substantially more unified now:

- the shared wait/service owner lives in
  [process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L896)
- `accept-process-output` routes through it in
  [process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L5462)
- `sleep-for` routes through it in
  [timer module](../../crates/neovm-core/src/emacs_core/system/timer/mod.rs#L608)
- keyboard/input waits already share its timer and process service helpers in
  [keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L3231)
  and
  [keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L3235)
  and `read_char` now stages ready input ahead of that service path in
  [keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L2344)

The remaining design difference is narrower: Neomacs still has both GNU-shaped
Lisp timers and a Rust `TimerManager`, but the Rust timer surface is currently
an internal compatibility layer rather than a published Lisp-visible timer
API. Final GNU parity therefore depends less on public timer semantics than on
making sure those internal timers do not perturb the observable ordering of
GNU Lisp timers, process callbacks, or input wakeups inside the shared wait
path.

## Confirmed Findings

### `accept-process-output`, `sleep-for`, and keyboard waits now share one wait path, but cross-source ordering still needs audit

GNU runs timers from the shared wait path unless `just_wait_proc < 0`, i.e.
unless the caller used integer `JUST-THIS-ONE`. See
[process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L5475),
[keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L4718),
and [atimer.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/atimer.c#L453).

Neomacs no longer has the earlier split where `accept-process-output` and
`sleep-for` each owned a separate polling loop. Both now route through the
shared wait/service path in
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L1120)
and
[timer module](../../crates/neovm-core/src/emacs_core/system/timer/mod.rs#L611).
That closed the earlier starvation bug and the older `PROCESS` /
`JUST-THIS-ONE` mismatch.

The mixed GNU Lisp timer / internal Rust timer / process-callback order is now
locked down by coverage: Neomacs runs GNU Lisp timers first, then internal Rust
timers, then process filters/sentinels in the same wait cycle, which matches
GNU's `timer_check` followed by timerfd/atimer callbacks and then process-fd
reads in `wait_reading_process_output`.

The remaining GNU risk is narrower now: keyboard/input break semantics and
redisplay/input competition across the shared wait path are not yet as tightly
locked down as the timer/process ordering itself.
The shared wait path also now routes window-close transport events through the
same delete-frame special-event handling as direct `read_char`, instead of
quitting immediately on that path.
It also now honors `throw-on-input` before dispatching those close events,
which matches the direct `read_char` path more closely.
The default `while-no-input-ignore-events` bootstrap value now also includes
`monitors-changed`, matching GNU's default ignore set more closely.
And `input-pending-p`/`while-no-input` filtering now actually honors that
ignore set for `monitors-changed` and `select-window`, instead of only
special-casing focus events.
`input-pending-p` also once again stages one unread host event from the
runtime input channel before checking pending input, so real host keypresses
are visible there instead of only already-staged queue entries.
Mouse-move visibility is also now gated more like GNU:
`input-pending-p` ignores raw host `MouseMove` unless `track-mouse` is
non-`nil`, and direct `read-char` now returns a real `mouse-movement` event
only when `track-mouse` is enabled instead of treating motion as generic
pending input while never surfacing it to Lisp reads.
That mouse-motion path now also updates one shared runtime mouse-position
owner: wait-path and direct-reader `MouseMove` handling record the last pixel
position, `display--update-for-mouse-movement` now follows GNU's 3-argument
ABI `(FRAME X Y)` instead of the old local 2-argument stub, and
`mouse-position` / `mouse-pixel-position` now read from that shared state
instead of acting like placeholders. So the remaining Phase 9 mouse-motion
work is no longer basic state ownership; it is only finer-grained idle/help
semantics on top of the now-correct runtime surface.
The low-level help path is also closer to GNU now: mouse updates queue
internal `help-echo` events on the shared unread-event path,
`input-pending-p` ignores those internal events, direct `read-char` consumes
them before returning user input, and `help-echo-inhibit-substitution` is now
honored on help strings. Neomacs also avoids trying to autoload `help.el`
from this low-level path when `substitute-command-keys` is still only present
as a startup autoload; it falls back to the core substitution implementation
instead. And when `mouse-fixup-help-message` is loaded on the normal runtime
surface, the low-level help path now runs that fixup before either echo-area
display or `show-help-function`, which matches GNU's `show_help_echo`
ordering more closely. That boundary is also now less host-runtime-specific:
the fixup no longer depends on there being a live input receiver, only on the
help string and the function being present, which is closer to GNU's C path.

### The shared wait path now services non-user-visible host input before polling timers/processes

GNU `wait_reading_process_output` calls `swallow_events` when it notices
window-system input while `read_kbd` is zero, so non-user-visible input-side
state such as resize handling is not starved behind `accept-process-output`
or `sleep-for`. See
[process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L5926)
and
[keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L4642).

Neomacs previously had no equivalent on the shared wait path: it polled only
timers and processes, so pending resize events could remain unapplied until a
later explicit keyboard read or a direct frame-size query. The shared
wait/service owner now services wait-path-safe host input before timers and
process callbacks, which closes that starvation case for `accept-process-output`
and `sleep-for`. The current ownership lives in
[keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L2179)
and
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L1144).
Neomacs now also re-checks that same special-input path immediately after each
blocking process wait, so a resize arriving during the wait itself is still
applied before `accept-process-output` or `sleep-for` returns on timeout.
And when the shared wait path is being used in a `do_display` context,
Neomacs now folds resize dirtiness into the same redisplay decision as timer
firing, which restores the GNU-style redraw behavior for the `read-char`
wait path after a resize is serviced there.
That boundary is now a little tighter: monitor-change events no longer force an
extra redisplay by themselves on the shared wait path. GNU's
`swallow_events`/`process_special_events` path runs
`display-monitors-changed-functions`, but redisplay from that helper is still
driven by timer runs, process activity, or other display dirtiness, not by the
monitor-change event alone.

### GNU ordinary and idle timer ordering now follows `timer_check_2` more closely

GNU does not fire all ordinary timers and then all idle timers. `timer_check_2`
merges those two sorted lists and chooses the timer that is due first in its
own time domain; if both are ripe, it picks the more-overdue timer. See
[keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L4727).

Neomacs previously serviced `timer-list` and `timer-idle-list` in two separate
passes, which could invert GNU order whenever an idle timer was more overdue
than an ordinary timer. The shared wait path now recomputes the next due GNU
timer one callback at a time and merges ordinary vs idle order using the same
“more overdue wins” rule in
[keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L3300)
and
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L896).

### Process callbacks now share one async callback envelope, but it is still a translated design

GNU process filters and sentinels preserve important dynamic state when they
run:

- current buffer is restored afterwards
- match data is preserved/restored
- `waiting_for_user_input_p` is restored
- quit is inhibited while the callback runs
- `last_nonmenu_event` is rebound
- `deactivate-mark` is restored afterwards

See
[process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L6518)
and [process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L7789).

Neomacs now routes process filters, sentinels, and client network `"open\n"`
sentinel delivery through one shared helper in
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L950).
That helper now preserves current buffer, match data, `waiting-for-user-input-p`,
`inhibit-quit`, `last-nonmenu-event`, and `deactivate-mark` across both
`accept-process-output` and other process callback entry points.

So the earlier “plain `eval.apply` on one path, protected callback on another”
mismatch is closed. The remaining difference from GNU is not split behavior;
it is that Neomacs still expresses this as a Rust-side translated helper rather
than GNU’s exact `read_process_output_call` / `internal_condition_case_1` /
`record_unwind_current_buffer` control flow.

### Timer callbacks now preserve GNU-visible state more closely

GNU's timer path preserves `deactivate-mark` in `timer_check_2`, while
`timer-event-handler` itself preserves current buffer with `save-current-buffer`.
See
[keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L4818)
and
[timer.el](/home/exec/Projects/github.com/emacs-mirror/emacs/lisp/emacs-lisp/timer.el#L283).

Neomacs now routes both GNU-shaped timer callbacks and Rust timer callbacks
through a shared helper in
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L982)
that preserves current buffer, `deactivate-mark`, and `inhibit-quit` while the
callback runs. That closes the earlier observable mismatch where a timer fired
from `accept-process-output` or `sleep-for` could leave `deactivate-mark`
clobbered afterwards.

### Same-cycle process output + exit now matches GNU more closely

GNU can observe process output and terminal status changes in the same wait
cycle, so a short-lived process can trigger filter and sentinel delivery from
one `accept-process-output` call. See
[process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L5575)
and
[process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L7863).

Neomacs now re-checks child exit after reading process output in
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L1117),
which closes the earlier gap where the sentinel for a short-lived child could
be deferred to a second `accept-process-output` call.

### `read_char` now gives ready input higher priority than timer/process callbacks

GNU's `wait_reading_process_output` and `read_char` path break back to input
when keyboard input is ready instead of continuing to service subprocess
callbacks ahead of that input. See
[process.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/process.c#L5895)
and
[keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L2489).

Neomacs previously fired timers and polled process output even after a ready
host input event had already arrived. `read_char` now stages immediately ready
input from the host queue/channel before the shared wait-path service and no
longer runs timer/process callbacks after receiving a real input event but
before returning it. The current ownership lives in
[keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L2344).
That closes the concrete mismatch where a due timer or process filter could run
ahead of a ready keypress.

### Interactive `read-event` / `read-char` timeouts now support GNU `sit-for`

GNU `sit-for` is implemented in Lisp in
[subr.el](/home/exec/Projects/github.com/emacs-mirror/emacs/lisp/subr.el#L3790)
and depends on `read-event` honoring its `SECONDS` argument on the interactive
path.

Neomacs previously ignored `SECONDS` once `read-event` or `read-char` fell
through to the runtime input path, which meant GNU `sit-for` could not time
out correctly in interactive mode. `read-event`, `read-char`, and
`read-char-exclusive` now route timeout values through the shared keyboard wait
path in
[lread module](../../crates/neovm-core/src/emacs_core/lisp/lread/mod.rs#L435),
[reader module](../../crates/neovm-core/src/emacs_core/lisp/reader/mod.rs#L2120),
and
[keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L2570).
Neomacs also no longer treats "top-level command loop not currently running"
as an automatic `quit` for these direct Lisp readers; only an actual shutdown
request now aborts the wait. That matches GNU more closely, because
`read-event` / `read-char` timeouts are valid outside the top-level command
loop as long as the interactive terminal is still alive.

That closes the specific Phase 9 blocker where GNU `sit-for` would hang
instead of returning `t` after the timeout elapsed.

### `input-pending-p t` now gives already pending input priority over timers

GNU `sit-for` first checks `detect_input_pending_run_timers`, which only runs
timers when input is not already pending. If input is already available,
`sit-for` returns `nil` immediately instead of firing timers first. See
[dispnew.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/dispnew.c#L6957)
and
[keyboard.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c#L12009).

Neomacs previously called `fire_pending_timers` at the start of
`input-pending-p t`, before checking unread or staged input. That could make a
due timer run even though GNU `sit-for` would have returned `nil` immediately
because input was already pending. `input-pending-p` now checks pending input
first, and only fires timers if no input is already available, in
[reader module](../../crates/neovm-core/src/emacs_core/lisp/reader/mod.rs#L1697).
That closes the remaining Phase 9 bug where `sit-for` could run timers ahead
of queued input.

### `input-pending-p t` no longer forces redisplay when it runs timers

GNU `Finput_pending_p` runs due timers through `get_input_pending` with
`READABLE_EVENTS_DO_TIMERS_NOW`, but it does not take the separate
`detect_input_pending_run_timers(do_display)` redisplay path. This matters for
GNU `sit-for`, because `(sit-for 0 t)` must honor `NODISP` even when due
timers fire before the zero-second fast path returns.

Neomacs previously called `fire_pending_timers()` directly from
`input-pending-p t`, which forced a redisplay whenever a due timer fired. That
meant `(sit-for 0 t)` could redisplay even though GNU would not. Neomacs now
services timers from `input-pending-p` without forcing redisplay, which
restores GNU `NODISP` behavior for this path. The current ownership lives in
[reader module](../../crates/neovm-core/src/emacs_core/lisp/reader/mod.rs#L1728),
with regression coverage in
[timer tests](../../crates/neovm-core/src/emacs_core/system/timer/tests/mod.rs#L389).

### Timer ownership is still split between GNU-shaped Lisp timers and a Rust timer manager

GNU’s ordinary and idle timers are part of the keyboard/event-loop contract.
Neomacs currently has two timer worlds:

- GNU Lisp timer vectors handled by `timer-event-handler` in
  [keyboard.rs](../../crates/neovm-core/src/keyboard.rs#L3143)
- Rust `TimerManager` entries in
  [timer module](../../crates/neovm-core/src/emacs_core/system/timer/mod.rs#L20)

This split is acceptable as migration scaffolding because the Rust timer
surface is not currently published as a GNU-visible Lisp API: the
`run-at-time` / `run-with-timer` / `run-with-idle-timer` helpers in
[timer module](../../crates/neovm-core/src/emacs_core/system/timer/mod.rs#L467)
exist for compatibility and tests, but are intentionally not registered as
builtins in
[native builtins](../../crates/neovm-core/src/emacs_core/lisp/native/builtins/mod.rs#L1434)
so that GNU `timer.el` remains the public timer owner.

So the remaining GNU risk is narrower than a full “two public timer APIs”
problem. The important constraint is that internal Rust timers must not
perturb the observable ordering of GNU Lisp timers, process callbacks, input
wakeups, or `sit-for` / `accept-process-output` behavior.

### Synchronous subprocess ownership is now mostly `callproc.c`-shaped

GNU keeps synchronous subprocess invocation in `src/callproc.c`, separate from
`process.c` but still part of the same process/timer/input boundary. It also
honors `DISPLAY` by redisplaying while output is inserted. See
[callproc.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/callproc.c#L272)
and
[callproc.c](/home/exec/Projects/github.com/emacs-mirror/emacs/src/callproc.c#L510).

Neomacs now has a real synchronous subprocess owner in:

- [callproc module](../../crates/neovm-core/src/emacs_core/system/callproc/mod.rs#L1)

`call-process`, `process-file`, `process-lines`, `call-process-region`, and the
shell-command variants now execute through that module, while
[process module](../../crates/neovm-core/src/emacs_core/system/process/mod.rs#L4002)
has been reduced to builtin delegation plus asynchronous-process ownership.
GNU-style `DISPLAY` redisplay is now also requested after synchronous output is
routed into a buffer destination.

The remaining gap is narrower than before: `callproc` still reuses some generic
string/region/process-I/O helpers from `process.rs`, so the file split is not
yet as self-contained as GNU `callproc.c` versus `process.c`.

## Coverage Gaps

Current neomacs coverage in `process_test.rs` is mostly argument contract and
rooting coverage. It does not yet lock down the high-risk shared-wait-path
semantics, especially:

- timer ordering when GNU Lisp timers and Rust timers are simultaneously due
- the remaining `sleep-for` / `sit-for` parity details once redisplay and
  pending input compete in the same cycle
- chunked synchronous subprocess insertion and redisplay fidelity versus GNU
  `callproc.c`

## Refactor Direction

The GNU-shaped direction is:

1. Introduce one `Context`-owned wait/service entry point for:
   - process output
   - process status transitions
   - sentinels
   - filters
   - GNU timers
   - Rust timers during migration
   - input wakeups / timeout calculation
2. Make `accept-process-output`, `sleep-for`, and keyboard wait paths call that
   same entry point.
3. Treat the `PROCESS` and `JUST-THIS-ONE` arguments as wait policy only, not
   as permission to create a separate semantic runtime.
4. Move callback execution behind one async envelope that restores:
   - current buffer
   - match data
   - waiting/input state
   - quit/debug policy as needed
5. Keep `neovm-core` as the semantic owner. Pollers, worker/runtime tasks, and
   frontend wakeups should remain transport mechanisms only.

## What To Audit Next Inside Phase 9

The next concrete source-level audit order should be:

1. GNU-Lisp-timer vs Rust-timer ordering once ordinary-vs-idle ordering is fixed
2. the remaining `sleep-for` / `sit-for` parity details once redisplay and
   pending input compete in the same cycle
3. chunked synchronous subprocess insertion/redisplay fidelity versus GNU
   `callproc.c`
4. focused differential coverage for the shared wait-path cases listed above

## Conclusion

The highest-priority Phase 9 problem is no longer ownership. Neomacs now has a
shared wait path and a real `callproc` boundary. The remaining work is exact
GNU ordering and callback behavior within that shared runtime.
