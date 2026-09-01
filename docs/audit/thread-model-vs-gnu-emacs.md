# Thread Model Audit

**Date**: 2026-03-30
**Status**: thread-state accessors now move closer to GNU on `main`, but the
core scheduler/liveness model is still architecturally unlike GNU Emacs.

## GNU Emacs Design

GNU thread semantics live in one C-owned runtime:

- `src/thread.c`
- `test/src/thread-tests.el`

Important GNU ownership points:

- `make-thread` creates a real live thread.
- `thread-live-p` is tied to actual live thread state via `thread_live_p`.
- `all-threads` returns only live threads.
- `main-thread` is a real Lisp variable initialized from C with `DEFVAR_LISP`.
- `thread-buffer-disposition` and `thread-set-buffer-disposition` read and
  write per-thread state on `struct thread_state`.
- `thread--blocker` returns the object a blocked thread is currently waiting
  on through `event_object`.
- `thread-signal` can interrupt a thread blocked in `mutex-lock`,
  `condition-wait`, or `thread-join`.

The important design conclusion is that GNU does not treat threads as an API
shim. The visible API is backed by real scheduler-owned state.

## Neomacs Current Design

Neomacs thread semantics currently live in
[thread module](../../crates/neovm-core/src/emacs_core/runtime/threads/mod.rs),
with builtin registration in
[native builtins](../../crates/neovm-core/src/emacs_core/lisp/native/builtins/mod.rs)
and some older gaps still surfaced through stubs.

The key architectural mismatch is explicit in the file header:

- Neomacs simulates threads in a single-threaded VM.
- `make-thread` stores state and runs the function immediately.
- there is no real blocked-thread state, scheduler handoff, or wakeup path
  equivalent to GNU `thread.c`.

That means Neomacs can expose some GNU thread APIs, but many of them are still
approximations unless they are pure thread-state accessors.

## Fixed In This Audit Pass

This audit separated "cheap state-accessor mismatches" from "real scheduler
mismatches" and fixed the first category.

### `main-thread` is now a real runtime binding

GNU initializes `main-thread` from C during startup. Neomacs had docstrings and
Lisp callers for `main-thread`, but no runtime binding was set.

`main-thread` is now synchronized from the thread manager into the evaluator in:

- [evaluator module](../../crates/neovm-core/src/emacs_core/runtime/eval/mod.rs)

That is the right boundary: `main-thread` is runtime-owned thread state, not a
Lisp helper.

### `thread-buffer-disposition` is now real thread state

GNU stores buffer disposition on each thread and exposes it directly through
`thread-buffer-disposition` / `thread-set-buffer-disposition`.

Neomacs previously exposed both entry points as stubs returning `nil`. They now
use real thread state in:

- [thread module](../../crates/neovm-core/src/emacs_core/runtime/threads/mod.rs)
- [native builtins](../../crates/neovm-core/src/emacs_core/lisp/native/builtins/mod.rs)

This includes the GNU constraint that non-`nil` disposition cannot be assigned
to the main thread.

### `all-threads` now uses live-thread filtering

GNU `all-threads` returns live threads only. Neomacs had been returning
"threads not yet joined", which is a different contract and could expose dead
threads indefinitely.

Neomacs now filters `all-threads` through live-thread status instead of joined
status in:

- [thread module](../../crates/neovm-core/src/emacs_core/runtime/threads/mod.rs)

This does not solve Neomacs's larger liveness mismatch, but it does make the
API boundary more honest and closer to GNU.

## Remaining Confirmed Mismatches

These are real GNU mismatches that this audit confirmed but did not try to
paper over.

### `make-thread` / `thread-live-p` are still fundamentally wrong

GNU `make-thread` returns a live thread and `thread-live-p` observes that live
thread until it exits.

Neomacs runs the thread body synchronously inside `make-thread`, so many
threads are already finished by the time Lisp receives the thread object. This
breaks the real GNU intent behind tests like:

- `threads-live`
- `threads-basic`
- `threads-signal-early`
- `threads-condvar-wait`

This is not a one-line bug. It is the consequence of the current thread model.

### `thread--blocker` is still a stub

GNU stores the current blocker object in `event_object` and updates it while a
thread is blocked on:

- `thread-join`
- `mutex-lock`
- `condition-wait`

Neomacs still returns `nil` unconditionally for
`thread--blocker`, because it does not yet model blocked thread state.

### `thread-signal` still cannot emulate GNU interruption semantics

GNU `thread-signal` can wake or interrupt a blocked target thread.

Neomacs can currently:

- signal the current thread immediately
- overwrite the terminal error of another simulated thread

It cannot actually interrupt a live blocked wait, because there is no real
blocked thread to wake.

### Buffer-disposition side effects are still incomplete

This pass fixes the accessor/state boundary, not the full behavior of buffer
death relative to running threads.

GNU uses buffer disposition to decide what happens when a thread's current
buffer is killed. Neomacs still does not have full GNU-equivalent behavior for:

- protecting a running thread's current buffer
- signaling `thread-buffer-killed`
- silent completion for `'silently`

That needs real integration between thread state and buffer-kill paths.

## Refactor Direction

The correct direction is to follow GNU ownership more closely:

1. Keep `ThreadManager` as the semantic owner of Lisp-visible thread state.
2. Stop treating `make-thread` as a synchronous compatibility shim.
3. Add explicit blocked-thread state to thread objects, including blocker
   ownership.
4. Make `thread-join`, `mutex-lock`, and `condition-wait` update that blocked
   state in one shared path.
5. Route `thread-signal` through that same runtime so it can wake blocked
   waits, not just rewrite terminal errors.
6. Integrate buffer-kill behavior with per-thread `buffer_disposition` instead
   of keeping the value as a disconnected accessor.

The concrete implementation plan for that refactor now lives in
[2026-03-30-thread-c-runtime-refactor.md](/home/exec/Projects/github.com/eval-exec/neomacs/docs/plans/2026-03-30-thread-c-runtime-refactor.md).

The important constraint is: do not fake GNU by making accessors lie. If
Neomacs remains synchronous internally for a while, the runtime should at least
report that honestly instead of exposing dead threads as live ones.

## Verification Notes

GNU references used during this audit:

- `src/thread.c`
- `test/src/thread-tests.el`

One direct GNU probe that was used for the accessor contract:

```elisp
(list
 (eq main-thread (current-thread))
 (thread-buffer-disposition main-thread)
 (thread-buffer-disposition (make-thread #'ignore nil 'silently))
 (thread-set-buffer-disposition (make-thread #'ignore) t)
 (condition-case err
     (thread-set-buffer-disposition main-thread t)
   (error (car err))))
```

Observed GNU result:

```elisp
(t nil silently t wrong-type-argument)
```

Neomacs verification for this pass used:

- `cargo fmt --all`
- `cargo check`
- focused `cargo nextest` with redirected output for thread runtime and VM
  coverage
