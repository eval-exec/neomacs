# Frontend Event Queue Design

**Status:** Accepted

## Problem

Neomacs currently transports renderer acknowledgements, host state changes, and
user commands through `keyboard::InputEvent`.  Their semantics are then
rediscovered independently by several wildcard-based classifiers:

- whether an event satisfies `input-pending-p`;
- whether it interrupts `while-no-input`;
- whether a wait request handles it internally;
- whether it resets idle time;
- whether handling it requires redisplay.

`PresentationRetired` exposed the failure mode.  It is a renderer lifecycle
acknowledgement, but after it was added to the common event enum it inherited
the default behavior of command input.  A redisplay retired the preceding
presentation, the acknowledgement interrupted `while-no-input`, and the next
input-wait iteration redisplayed again.  The captured runtime trace recorded
265 retirement acknowledgements, 266 redisplays, and 19 false
`throw-on-input` cycles before the first real key.

The root problem is not the missing match arm alone.  Event meaning is part of
the interface, but that interface is spread across unrelated callers.

## GNU Emacs reference model

GNU Emacs keeps one ordered keyboard buffer but distinguishes three semantic
categories:

1. command input, such as keys and mouse actions;
2. Lisp-visible special events, such as focus and monitor changes;
3. non-user-visible internal events, such as selection protocol traffic.

`process_special_events` removes and applies non-user-visible events before
`input-pending-p` examines the remaining buffer.  `is_ignored_event` determines
which visible special events may be ignored by `while-no-input`.  `swallow_events`
does not redisplay merely because it consumed internal traffic.

References:

- <https://github.com/emacs-mirror/emacs/blob/master/src/keyboard.c>
- `process_special_events`, `swallow_events`, `readable_events`, and
  `is_ignored_event` in that file.

Neomacs should preserve this observable model while extending it for renderer
presentation lifetimes, which GNU does not have.

## Goals

- Preserve FIFO ordering between presented pointer observations and
  presentation retirement.
- Make every frontend event's semantic category an exhaustive decision.
- Consume internal traffic before GNU pending-input decisions.
- Give `input-pending-p`, `while-no-input`, waits, idle tracking, and
  `read_char` one source of truth.
- Keep renderer lifecycle details out of the command loop.
- Make the module interface the natural regression-test surface.

## Non-goals

- Do not create independent physical queues for commands and control traffic.
  Merging them would require sequence numbers to recover ordering.
- Do not expose renderer acknowledgements as Lisp events.
- Do not redesign GNU's Lisp-visible special-event semantics.
- Do not make `PresentationRetired` request a frame publication.

## Domain model

The transport remains one ordered stream.  Its events have one exhaustive,
implementation-private semantic classification:

```rust
enum EventVisibility {
    Command,
    LispSpecial,
    Internal,
}
```

The public transport name should eventually become `FrontendEvent` so
`InputEvent` can no longer imply that every variant is user input.  Nested
families may make construction clearer:

```rust
enum FrontendEvent {
    Command(CommandInput),
    LispSpecial(SpecialEvent),
    Internal(InternalEvent),
}
```

This type split is subordinate to the ordering invariant: all variants still
occupy the same FIFO.

### Scheduler policy refinement (September 17, 2026)

Issue 391 exposed a second invalid combination: `PresentedRegion` was marked
non-command input but was neither internal nor serviceable during a wait.
Pointer motion could therefore strand all subsequent native resize events
until an unrelated click completed the input wait.

The implemented classification now uses `FrontendEventSemantics`:

```rust
enum FrontendEventSemantics {
    Command,
    Internal(InternalFrontendEvent),
    MouseMotion,
    ServiceDuringWait,
    SpecialInput {
        pending: PendingInputPolicy,
        interrupts: bool,
        service_during_wait: bool,
    },
}
```

`PendingInputPolicy` permits always-readable or explicitly filterable Lisp
input, but has no `Never` variant. Unconditionally non-command events must
instead supply an internal action or select `ServiceDuringWait`. Mouse motion
is readable when `track-mouse` is enabled and serviced during waits otherwise.
GNU's existing filtering behavior for focus/other visible special events is
unchanged; this is not a claim that filtered Lisp input can never defer work.

The exhaustive transport classification constructs the internal action itself.
The queue no longer classifies an event as internal and then relies on a
second, wildcard-based match to discover its handler. Adding an internal
action also requires updating the exhaustive VM-owned dispatcher.

`PresentedRegion` is an internal action. It records a hit only if its frame's
active presentation matches, without calling Lisp, stopping idle time,
interrupting `while-no-input`, or requesting redisplay. Its following mouse
action retains its own input semantics. Observation, action, presentation
lifecycle, and resize events retain their existing FIFO order.

The same investigation found that input waits reused the filtered pending
query. During an X11 border drag, a filtered focus-out event could remain
ahead of presentation acknowledgements and subsequent resize events. The
first resize sometimes advanced only because the legacy resize synchronizer
temporarily defers leading focus events; it cannot cross the acknowledgement.

`FrontendInputQuery` now makes the caller's operation explicit:

```rust
enum FrontendInputQuery {
    Readable,
    Pending(InputPendingFilter),
}
```

`ReadCommandInput` selects `Readable`, like GNU's
`detect_input_pending_run_timers` without `READABLE_EVENTS_FILTER_EVENTS`.
Filtered pending-input probes select `Pending` and retain their existing
ignore rules. Focus remains Lisp-visible and is handled in order by the
reader; it is not reclassified as internal or stripped of its Lisp hooks.
The wait-policy match chooses the query exhaustively, so a new wait mode
cannot silently inherit the wrong filtering operation.

The Android/Wasm branch additionally has `ReadControl` for ordered IME
snapshot/selection and persistence requests. The native and portable waits
explicitly return to the reader for that queue-head category without marking
it as command input. `PresentedRegion` remains internal on every frontend;
both native and portable command waits use `FrontendInputQuery::Readable`.

`PresentationRetired` is `Internal`.  Its policy is fixed:

| Semantic property | Value |
|---|---|
| Lisp-visible | no |
| Pending command input | no |
| Interrupts `while-no-input` | no |
| Resets idle time | no |
| Requests redisplay | no |
| State transition | release the evaluator's presentation snapshot |

## Ownership

- The renderer owns which presentation is currently visible and emits an
  ordered retirement acknowledgement when it can no longer display or hit-test
  an older presentation.
- The evaluator owns the Lisp values retained by presentation snapshots.
- The frontend event queue owns transport order, semantic classification,
  internal-event processing, and the answer to pending-input queries.
- The command loop consumes Lisp-visible events but does not interpret renderer
  lifecycle messages.

## Deep module and seam

Introduce a `FrontendEventQueue` module at the seam between the frontend input
bridge and evaluator scheduling.  Its interface should describe GNU operations,
not per-event flags:

```rust
enum PendingQuery {
    CommandWait,
    InputPendingP { filter_events: bool },
    WhileNoInput,
}

struct ServiceOutcome {
    pending: bool,
    redisplay_needed: bool,
    host_activity: bool,
}

impl FrontendEventQueue {
    fn service_ready(
        &mut self,
        evaluator: &mut Context,
        query: PendingQuery,
    ) -> Result<ServiceOutcome, Flow>;

    fn next_lisp_event(
        &mut self,
        evaluator: &mut Context,
    ) -> Result<Option<Value>, Flow>;
}
```

The exact Rust spelling may change during implementation, but the interface
must keep event-specific knowledge private.  Callers choose the GNU operation
they are performing; they do not inspect event variants or combine policy
flags themselves.

Inside `service_ready`, the module:

1. drains and applies ready `Internal` events in FIFO order;
2. accumulates explicit effects from their handlers;
3. examines the first remaining visible events according to `PendingQuery`;
4. returns the scheduling result without exposing classification details.

The classification match must be exhaustive.  It must not contain a wildcard
that silently treats a future variant as command input or internal traffic.

## Pipeline

```text
render/platform frontend
        |
        | ordered FrontendEvent stream
        v
input bridge adapter
        |
        v
FrontendEventQueue
        |
        +-- drain Internal events
        |      `-- PresentationRetired: release snapshot, no redisplay
        |
        +-- apply/query LispSpecial events using GNU filtering
        |
        `-- expose Command events to read_char/key sequence handling
```

Internal processing must happen before:

- setting `quit-flag` for `throw-on-input`;
- answering `input-pending-p`;
- deciding that command input should preempt another timer batch;
- stopping or restarting idle timers;
- entering the blocking frontend wait.

## Effects and error handling

Internal handlers return effects rather than performing incidental scheduling.
For example, `LayoutInvalidated` can request redisplay, while
`PresentationRetired` cannot.  This prevents a `None` Lisp event from being
mistaken for "restart the whole read loop and redisplay."

A malformed or unknown presentation retirement is idempotent: removing an
already-retired snapshot succeeds without visible effects.  Reliable delivery
is still required so retained Lisp roots do not leak indefinitely.

Non-local Lisp control flow is possible only while processing Lisp-visible
events.  Internal renderer handlers must not evaluate Lisp.

## Tests at the module interface

The central contract suite must cover:

- retirement does not satisfy `input-pending-p`;
- retirement does not interrupt `while-no-input`;
- retirement does not preempt a timer batch as command input;
- retirement does not reset idle time;
- retirement does not request redisplay;
- retirement releases the presentation's Lisp roots;
- pointer-before-retirement resolves against the captured snapshot;
- retirement-before-pointer rejects the stale pointer;
- internal events ahead of a key are drained without reordering the key;
- every event variant has an explicit semantic classification.

One GUI integration regression should publish a frame, receive its retirement,
and assert that the evaluator returns to a blocked input wait without publishing
another unchanged frame.

## Migration

1. Introduce the queue module and move the existing classifiers behind its
   interface without changing behavior.
2. Add exhaustive classification and contract tests for every current event.
3. Add internal-event draining modeled after GNU `process_special_events`.
4. Classify `PresentationRetired` as internal and route snapshot release through
   the drain.
5. Replace direct classifier use in `input-pending-p`, `throw-on-input`, wait
   requests, idle tracking, and `read_char` with queue operations.
6. Remove the old scattered classifiers.
7. Rename/split the transport enum once all callers cross the new seam.

Each migration step should preserve FIFO order and be independently testable.
