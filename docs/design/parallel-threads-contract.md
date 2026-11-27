# Opt-In Parallel Threads (`neomacs-make-parallel-thread`) — Runtime Contract

This note documents the user-visible runtime contract for opt-in parallel Lisp
threads (`neomacs-make-parallel-thread` and `neomacs-thread-parallel-p`) once an
interactive or batch session activates a parallel execution era.

## 1. Zero Cost for Default and `make-thread`-Only Sessions

Sessions that never call `neomacs-make-parallel-thread` — including single-thread
sessions and sessions using standard GNU `make-thread` — remain in
`ParallelActivationState::Inactive`. They keep Main's legacy serial JIT active,
never spawn a keeper thread, and execute only a single predicted-not-taken byte
load at the top of `command_loop_1` (zero atomics, zero locks, zero `RingWaiter`
loads, zero clock reads).

## 2. Interactive First-Call Deferral (`BetweenCommands` / D2)

In an interactive command loop (`recursive-depth == 0` and
`minibuffer-depth == 0`, outside `execute-kbd-macro` and active Lisp/VM frames):

- Calling `(neomacs-make-parallel-thread FN)` **mid-command** before a parallel
  era has been opened does **not** pause or migrate Main mid-command. Instead,
  that first thread runs as an ordinary cooperative Baton thread
  (`(neomacs-thread-parallel-p th)` returns `nil` and is never runtime-upgraded)
  and arms `PendingUpgrade`.
- At the next quiescent top-level between-commands boundary (`command_loop_1`),
  Main opens the parallel era once. Subsequent `neomacs-make-parallel-thread`
  calls are admitted as parallel owners (`(neomacs-thread-parallel-p th)` returns
  `t`).
- Nested `recursive-edit`, active minibuffers, `execute-kbd-macro` playback,
  and non-Main threads defer `PendingUpgrade` without opening the era or
  permanently refusing the session.
- Non-interactive `--batch --eval` sessions without an interactive command loop
  keep immediate era admission on the first opt-in call.
- Permanent session refusals (non-Green world, non-generational heap, or
  unsupported heap features) fall back to cooperative Baton `make-thread`
  (`(neomacs-thread-parallel-p th)` returns `nil`) without re-probing.
  Transient resource/startup failures (e.g., OS thread spawn refusal) retry on
  the next quiescent between-commands boundary.

## 3. Main-Thread Bytecode Execution While an Era Is Open (D4)

- Once a parallel era is opened, Main's legacy serial JIT is withdrawn for the
  lifetime of the `World` (until session shutdown), and Main executes bytecode
  through the bytecode interpreter. The withdrawal flag (`World::legacy_jit_withdrawn`)
  is a monotonic `AtomicBool` (`false -> true` on era commit) and remains `true`
  across terminal `green_teardown` (because `DetachedSourceCache::drop` retains
  payloads rather than restoring legacy JIT; sessions that never opened a
  parallel era keep `legacy_jit_withdrawn == false`).
- Closing the era automatically when active parallel owners drop to zero
  (`close-on-zero-owners`) is intentionally deferred; Main stays on the
  interpreter for the remainder of the session once an opt-in parallel era has
  opened.

## 4. Cooperative Fairness Scope (Q-A / Q-B)

While an interactive parallel era is `Active`:

- **Queued serial turns (Q-A):** At each top-level `command_loop_1` boundary,
  Main checks the keeper's `RingWaiter` with a single lock-free `Acquire` load.
  If a parallel owner is waiting for a serial turn, Main yields `WorldTurn`
  once at the boundary.
- **Inter-command bursts with pure-compute parallel owners (Q-B):** During
  sustained non-idle command bursts while at least one parallel owner is live,
  Main releases `WorldTurn` to `GroupTurn::Idle` for `PARALLEL_FAIRNESS_SLICE`
  (1 ms) after holding the Baton without an idle park for `BATON_BURST_QUANTUM`
  (4 ms), pausing the Baton group (including runnable sibling `make-thread`
  workers) for the slice so pure-compute parallel owners make forward progress.
- **Known non-burst limitation:** Q-B checks `BATON_BURST_QUANTUM` at the top
  of `command_loop_1`. A single uninterrupted long-running Main command that
  never returns to `command_loop_1`, or two cooperative Baton `make-thread`
  workers yielding only to each other while Main is blocked, does not cross
  `command_loop_1` and is not preempted by Q-B (deferred to a future slice).
