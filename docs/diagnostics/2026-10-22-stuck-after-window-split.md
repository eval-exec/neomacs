# Frozen display after splitting a window: live-process investigation

Investigated on 2026-10-22, approximately 16:24–16:30 Asia/Shanghai.
Branch: `investigate/neomacs-stuck-2026-10-22`.
Source checkpoint: `90b21578c9d728e12768e5b340c2b95ec718b3ad`.

## Finding

The existing Neomacs process is responsive to Lisp evaluation, but cannot
produce a valid frame presentation with its current split-window layout and
mode line. Its evaluator repeatedly logs:

```text
rejecting invalid frame presentation error=Spatial(StringPositionOutsideSemanticRegion)
```

The strongest live isolation is an A/B/A test: the original configuration
fails; temporarily removing `mode-line-format-right-align` from the Rust
buffer's mode line succeeds; restoring it fails again. The user recalled
that the freeze began when attempting to split a window. The live window tree
confirms that the split completed in editor state.

This establishes a presentation-validation failure associated with the
configured right-aligned mode-line content. It does **not** yet identify the
individual offending glyph, its coordinates, or the precise producer defect.
No application code was changed and no fix is claimed.

## Process and session

| Item | Observed value |
| --- | --- |
| PID | `466761` |
| Command | `./target/release/neomacs` |
| Start time | 2026-10-22 16:15:38 +08:00 |
| Executable modification time | 2026-10-21 23:05:32 +08:00 |
| Working directory | This repository |
| Display environment | `WAYLAND_DISPLAY=wayland-0`, `DISPLAY=:0` |
| Lisp server socket | `/run/user/1000/emacs/neomacs` |
| Standard output / error | Both redirected to `/dev/null` |
| Frames | Visible `F1`; hidden child frame named `posframe` |
| Selected window | Window 10, displaying `impls.rs` |
| Other ordinary windows | Window 1, also `impls.rs`; window 6, Treemacs |

The checked-out source is the investigation reference, not a verified build
revision of the already-running executable. No rebuild or restart occurred.

The tree has a Treemacs sidebar and two side-by-side Rust windows. Reported
pixel edges were:

```text
window 6:  (0,    21,  160, 1196)  Treemacs
window 1:  (160,  21, 1068, 1196)  impls.rs
window 10: (1068, 21, 1975, 1196)  impls.rs, selected
```

## Evidence against a process-wide hang

Three consecutive socket evaluations of
`(list :alive (emacs-pid) (recursion-depth))` returned
`(:alive 466761 0)` in **6 ms, 4 ms, and 4 ms**. The active minibuffer was nil
and `inhibit-redisplay` was nil. The last command was `keyboard-quit`.

Two GDB captures were taken and detached. The first captured the evaluator
waiting in its ordinary input-reading path:

```text
polling::Poller::wait
ProcessManager::wait_for_backend_events
Context::wait_reading_process_output
Context::read_char_event_with_timeout_policy
Context::read_key_sequence_with_options
Context::command_loop_2
...
neomacs::run_gui_evaluator_worker
```

The main thread was in `polling::Poller::wait`, through
`calloop::sys::Poll::poll` and Winit's `EventLoop::run_app_on_demand`.
The frame publisher and input bridge were waiting on channel selection;
the captured stacks do not show a blocked GPU submission or a busy evaluator.
These are samples, not a proof that an earlier transient stall never occurred.

Synchronous `window-end WINDOW t` and `posn-at-point` queries succeeded for
all three ordinary windows. Whole-frame presentation failed even though these
individual window queries worked.

## Repeated failing probe and recovered diagnostics

This command repeatedly returned exit status 1:

```sh
timeout 5s emacsclient \
  -s /run/user/1000/emacs/neomacs \
  -e '(neomacs--frame-snapshot t (quote text))'
```

```text
*ERROR*: frame snapshot: no frame produced display state
```

The selected-frame variant failed with the same message. This is an
agent-runnable signal for the current broken frame-production state; it is
not a standalone reproduction of the original split operation or a GPU pixel
assertion.

Because stdout was `/dev/null`, the relevant tracing output was recovered
without changing the logger, using a bounded syscall trace while requesting
another snapshot:

```sh
timeout 12s strace -f -tt -s 4096 -e trace=write,writev \
  -p 466761 -o tmp/stuck/redisplay-writes.txt
```

The trace recorded four `Spatial(StringPositionOutsideSemanticRegion)`
presentation rejections at local times 16:27:59.651, 16:27:59.740,
16:27:59.832, and 16:28:01.981. They came from evaluator thread `466799`.
It also recorded chrome-metric retries for window 10, followed by rejection,
and later `redisplay skipped: visible state unchanged` and input waiting.
The metric retries alone do not establish a convergence bug.

## Controlled isolation in the live session

Each variation used a temporary Lisp `let` binding inside the selected
window's buffer, then requested a snapshot. Both Rust windows share that
buffer. The bindings unwound after each request; no buffer contents or window
topology were edited. Successful results below mean that a text snapshot was
returned, not that compositor pixels were independently verified.

| Temporary variation | Result |
| --- | --- |
| Original mode line | Fails |
| `tab-line-format = nil` in Rust buffer | Fails |
| `header-line-format = nil` in Rust buffer | Fails |
| `mode-line-format = nil` in Rust buffer | Passes |
| Mode line containing an empty string | Passes |
| Mode line containing `probe` | Passes |
| Mode line containing 300 plain `x` characters | Passes |
| Original mode line with only `mode-line-format-right-align` removed | Passes |
| Original mode line with only `mode-line-modes` removed | Passes |
| Original mode line with only `mode-line-misc-info` removed | Passes |
| Plain `probe` after `mode-line-format-right-align` | Passes |
| 300 plain `x` characters after right alignment | Passes |
| Only right alignment plus original modes and miscellaneous tail | Fails |
| Same original tail without right alignment | Passes |

The reduced failing mode line, still dependent on this buffer's live variables,
is:

```elisp
("" mode-line-format-right-align mode-line-modes mode-line-misc-info)
```

Thus right alignment alone and long plain text alone are insufficient to
reproduce this failure. The original right-aligned tail and its formatting or
geometry matter. The active alignment edge was `window`; both active and
inactive mode-line face heights were reported as `1.0`.

Final A/B/A verification:

```text
original:                 error, no frame produced display state  (87 ms)
without right-align:      success, snapshot length 8498          (93 ms)
original restored:        error, no frame produced display state (118 ms)
final state:              PID 466761 alive, 3 ordinary windows,
                          impls.rs selected, right-align restored
```

## Relevant code path and interpretation

1. [run_frame_snapshot](../../crates/neovm-core/src/emacs_core/display/xdisp/mod.rs)
   forces redisplay and invokes the frontend snapshot callback.
2. [collect_snapshot_states](../../crates/neomacs/src/frame_layout.rs)
   asks the redisplay runtime to prepare each target frame. If none succeeds,
   it emits the observed `no frame produced display state` error.
3. [LayoutEngine](../../crates/neomacs-layout-engine/src/engine.rs) calls
   `PresentationComposer::compose`. On the captured spatial error it rejects
   the attempt, retires the presentation, and returns without committing the
   new retained frame state.
4. [PresentationSpatialPlan::compile](../../crates/neomacs-layout-engine/src/presentation/spatial.rs)
   builds semantic window regions and chrome-string positions, then validates
   them together through `PresentedHitIndex::from_parts_with_strings`.
5. [The hit-index validator](../../crates/neomacs-display-protocol/src/presented_pointer.rs)
   requires every chrome-string rectangle to fit entirely inside a semantic
   region with the same window and region kind. A missing matching owner or
   a rectangle extending outside it produces the exact captured error.

[mode--line-format-right-align](../../lisp/bindings.el) measures the following
mode-line content with `string-pixel-width` and creates a stretch space using
`:align-to`. The producer and validator therefore need scrutiny at the boundary
between measured mode-line layout, materialized string glyph bounds, and the
window's published mode-line region after a split.

The confirmed failure mechanism is that the new presentation is rejected
before publication while editor state remains responsive. This explains a
display that appears frozen after the split. The exact geometry defect could
involve measurement, clipping, coordinate conversion, or stale region data;
the present evidence does not distinguish those possibilities.

## Follow-up and limits

The next useful probe is to record the **first rejected string position**:
window ID, chrome kind, string character index, glyph rectangle, and matching
semantic rectangle. Capture that directly before the validator returns its
error. Compare the two Rust windows and the temporary no-right-align case.
This should distinguish an absent owner from an overflow and identify the
amount and axis of any mismatch.

A regression should exercise the real frame composer with split windows and
the reduced right-aligned tail. The existing test
`presented_hit_index_rejects_string_position_outside_its_typed_chrome_area`
checks the validator, but does not show why this live layout generated invalid
input. Disabling that validation would discard an established invariant.

No fresh-session reproduction, pre-split comparison, split/unsplit experiment,
native keyboard injection, compositor screenshot, or source patch was made.
The split association is based on the user's recollection and the observed
post-split state. Configuration-specific content and fonts remain part of the
reproduction context. `*Messages*` also contained LSP build-script failures
and an earlier Treemacs sentinel error; neither is established as causal.

Raw local captures remain under `tmp/stuck/`: GDB thread stacks, liveness
timings, window queries, syscall writes, and ablation results. They are not
added to the branch; this report includes the relevant findings without
embedding the user's source-buffer contents or full configuration dumps.

The process remains running with its original mode-line configuration and
split layout. All GDB and strace attachments have ended. No PR was created.
