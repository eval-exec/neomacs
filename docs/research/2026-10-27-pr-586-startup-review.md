# PR #586: initial GUI frame lifecycle review

Investigated 2026-10-27. All filesystem work for this review ran through `ssh treeroot`. GNU Emacs source was read first, at `a360712c9d272d950d8d8255ef74570f7e90b7d9`; its checkout remained read-only. The reviewed Neomacs PR head was `1c62eea9afcc09fe1122ef707f3d84b27b7a6562`, with merge base `b5030a45f6cbe9c0435f77eea4ab9b0df31c0272`. This note records source and primary-document research, not new runtime or test results.

## The GNU contract

GNU documents early init as preceding GUI initialization, and regular init as following it. This makes frame defaults and creation hooks legitimate early-init configuration while GUI-dependent operations need suitable later hooks. The contract does not say every display API is usable in early init. [Early Init File](https://www.gnu.org/software/emacs/manual/html_node/emacs/Early-Init-File.html), [Startup Summary](https://www.gnu.org/software/emacs/manual/html_node/elisp/Startup-Summary.html).

The pinned implementation loads early init, initializes packages and the selected window system, runs `before-init-hook`, and then invokes `frame-initialize` for ordinary startup before loading regular init. Batch startup does not take the graphical branch, and daemon startup skips opening-frame creation here. A test of the GUI lifecycle therefore needs an interactive graphical child; running only a batch expression cannot establish the issue. [startup.el](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/startup.el#L1530-L1610).

GNU's initial C frame is attached to the initial terminal, placed in the frame list, and given terminal default foreground/background values. It is not a precreated graphical frame. [frame.c](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L1422-L1474).

`frame-initialize` creates an opening frame only when no minibuffer frame other than `terminal-frame` already exists. Its parameters put `initial-frame-alist` before `default-frame-alist`. It then copies the startup environment and removes the initial terminal. Crucially, the guard also preserves a frame explicitly created by user code in early init; fixing #417 must not prohibit such creation or force a second opening frame. [frame.el](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L414-L447).

The manual describes parameter inheritance and the hooks around `make-frame`. The implementation dispatches the before hook, calls the backend creation function, performs common frame setup, calls `frame-after-make-frame`, and finally dispatches the after hook with the newly created frame. Tests should inspect that argument: `make-frame` itself does not promise to select the returned graphical frame, although a window manager may select it. [Creating Frames](https://www.gnu.org/software/emacs/manual/html_node/elisp/Creating-Frames.html), [frame.el hook dispatch](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L1127-L1180).

`initial-frame-alist` takes precedence for the opening frame, while `default-frame-alist` provides defaults for all frame creation. The current pinned variable documentation recommends setting initial parameters in early init; regular-init changes happen after initial creation and can only be reconciled later. This distinction matters when interpreting manual text that says Emacs can subsequently apply changed initial parameters to an existing frame. [Initial Frame Parameters](https://www.gnu.org/software/emacs/manual/html_node/elisp/Initial-Parameters.html), [pinned variable documentation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L139-L163).

## Font and visibility ordering

The X and NS Lisp backend methods both use `x-create-frame-with-faces`. It creates the underlying frame invisibly, recalculates frame-local faces, applies delayed font/color settings and global new-frame face defaults, and only then requests visibility. Its unwind cleanup deletes an incompletely initialized frame. Therefore creation, face completion, visibility, and selection are distinct operations even in GNU; a fix that merely moves an early bootstrap allocation later can still miss the face/geometry boundary. [faces.el](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/faces.el#L2215-L2270), [X backend](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/term/x-win.el#L1351-L1352), [NS backend](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/term/ns-win.el#L884-L885).

The X primitive registers font drivers and resolves the default frame font before computing geometry. It initializes faces before deriving pixel dimensions, creates the native window afterward, and processes requested visibility later. This supports using opened-font metrics rather than an estimated startup font to determine initial native geometry. [xfns.c font opening](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfns.c#L5133-L5160), [geometry/native creation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfns.c#L5253-L5320), [visibility](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfns.c#L5436-L5478).

The macOS NS primitive follows the same broad order: select the platform default font, initialize faces, calculate geometry, construct the native view, then honor visibility and make a visible window key. This is source evidence for the reported platform, not execution coverage on macOS. [nsfns.m font/faces](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsfns.m#L1323-L1406), [native view](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsfns.m#L1470-L1523), [visibility](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsfns.m#L1547-L1565).

## Reading the proposed Neomacs abstraction

The PR represents primary routing as `Awaiting`, `Adopted(FrameId)`, or `Retired`, rather than interpreting a frame identifier as a readiness boolean. The initial evaluator-side status separately owns waiting, running, and closed states. Rust enums provide a way to attach data only to the state that needs it and require consumers to handle the variants. This is a suitable distinction between a logical Lisp frame and primary native-window assignment. [PR primary routing](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs/src/main.rs#L1260-L1289), [PR evaluator owner](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs/src/startup_frame.rs#L1-L101), [Rust enums](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html).

The evaluator stack owns the strong `Rc` holding the startup status; display hosts receive a `Weak` completion capability. Completing readiness consumes the waiting reply into an active lifetime, leaving the sender connected until the evaluator owner exits. Dropping the reply disconnects before waking the event loop. This prevents a retained host from keeping startup alive merely by owning the sender, while still letting the native side distinguish prepared geometry from an evaluator that has already departed. These are observed ownership choices in the PR; their liveness still needs cancellation and unwind tests. [PR owner/publisher](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs/src/startup_frame.rs#L14-L99), [PR channel lifetime](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs-display-runtime/src/render_thread/startup.rs#L31-L85), [Rust Drop](https://doc.rust-lang.org/std/ops/trait.Drop.html).

The `#[must_use]` marker documents and diagnoses accidental disposal; it is not by itself a hard proof that every caller retains a value for the intended duration. The meaningful ownership restriction is the private strong owner plus non-owning publishers, with lexical scope chosen correctly. Rust reference lifetime checking likewise establishes reference validity, not the application's event-loop progress or the correctness of its chosen scope. [PR owner scope](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs/src/main.rs#L3982-L4018), [Rust lifetime contracts](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html), [Rust diagnostics attributes](https://doc.rust-lang.org/reference/attributes/diagnostics.html#the-must_use-attribute).

## Design boundaries and verification implications

The following are research conclusions or explicit review questions, not reproduced defects:

- Keep Lisp responsible for the semantic creation path, parameter precedence, and hooks. Native realization should consume the resulting frame request instead of manufacturing a second startup frame. This follows from GNU's `frame-initialize`/`make-frame` ownership described above.
- Keep display-connection metadata, logical frame existence, prepared geometry, native surface permission, GPU initialization, and first presentation distinct. Publishing monitor metadata without allocating a window is a Neomacs architecture choice that supports pre-frame connection queries; GNU does not promise that every GUI query works in early init. The PR already distinguishes surface permission and evaluator outcome. [PR native phases](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs-display-runtime/src/render_thread/startup.rs#L109-L160).
- Preserve the invisible faces-wrapper path and raw visible `x-create-frame` path independently. The former has delayed face/font completion; the latter does not pass through the Lisp wrapper. A single blanket readiness moment needs evidence for both. [PR completion boundaries](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs#L7239-L7317), [GNU faces wrapper](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/faces.el#L2215-L2270).
- Explicit early-init frame creation, invisible initial frames, and caught creation errors must not produce a duplicate primary window or strand the native loop. Normal shutdown after GPU initialization must not erase a native event-loop error. The latter is the purpose of the PR's separate shutdown-result follow-up. [PR native result classification](https://github.com/eval-exec/neomacs/blob/1c62eea9afcc09fe1122ef707f3d84b27b7a6562/crates/neomacs-display-runtime/src/render_thread/startup.rs#L129-L175).
- A portable regression should record early-init frame kinds, before/after creation hooks, regular-init state, creation-time defaults, and the supplied after-hook frame. Native readiness/presentation needs a separate platform test. Snapshotting only settled colors cannot prove correct startup ordering, and recording only hook names cannot prove correct font-owned geometry. These test recommendations are deductions from the GNU boundaries above.

The GNU checkout was used read-only. The source-research stage above preceded the runtime investigation below; the following sections record the current reproductions and verification.


## Reproduced behavior and review fixes

The runtime investigation below followed the source research. All commands ran on `treeroot`; GNU source remained unchanged. The baseline was current main `c1ea6adf391cd1f9bd4061438614c7a8e2d17576`, whose tree equals the tested #595 branch. PR #586 was rebased privately from `1c62eea9afcc09fe1122ef707f3d84b27b7a6562` onto that baseline before fixes.

The `tmp/pr586/` logs, JSON observations, PNG captures, cached binaries, and other private-host artifacts cited below are unpublished and are not available in this repository. Their paths and hashes identify local evidence only. Reproducible focused commands are included below; the checked-in [startup GUI tests](../../crates/neomacs-gui-tests/tests/issue_417_startup.rs) and fixtures provide the test source.

The original #417 lifecycle regression failed twice on main with a passing GNU control. At early-init, GNU reported only a terminal frame; Neomacs already reported a selected graphical frame alongside its terminal. Main skipped the before/after frame-creation hooks and regular init still observed a white background rather than the requested green. The original PR correctly moves logical creation into Lisp startup: after rebase and a fresh matching debug dump, both ordinary X11 lifecycle cases passed. Evidence is in `tmp/pr586/current-main-startup{,-repeat}-red.log`, `main-repro-summary.json`, and `pr-head-startup-green.log`.

Further tests exposed three independent review defects:

1. **Native error classification after GPU recovery.** A previously running session could lose its GPU during recovery. Evaluator shutdown then replaced its historical running outcome with successful startup cancellation, suppressing a native event-loop error. A focused test first failed, then passed after an exhaustive outcome match allowed cancellation only from `Preparing`. Recovery is synchronous in this implementation; this regression exercises result classification rather than simulating an asynchronous driver failure. Evidence: `gpu-recovery-{red,green}.log`.
2. **Retiring a terminal for an invisible GUI frame.** The new invisible-initial-frame GUI case passed on GNU but Neomacs signaled the sole-visible-frame error before normal init. GNU `other_frames` explicitly permits deleting a terminal if an independent graphical frame exists, even if invisible; hiding the terminal has a different contract. Three public Lisp tests first produced one failure and two passing controls. The shared helper now takes `OtherFramePurpose::{MakeInvisible, Delete { force_non_nil }}` instead of an ambiguous boolean. All three then passed. Evidence: `invisible-initial-frame-red.log`, `terminal-frame-policy-{red,green}.log`. [GNU deletion rule](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L2496-L2590).
3. **Logical invisibility lost at native allocation.** After correcting terminal deletion, the same GUI test reached after-init: Lisp reported `nil` visibility, but X11 reported an editor-owned mapped window. The existing completion channel carried only dimensions, and native creation used default visibility. The fix carries `InitialWindowPreparation { size, visibility }`, using the existing closed visibility enums and an exhaustive core-to-native conversion. Geometry observations remain independent of visibility. Each native frame retains desired visibility outside its allocated/pending lifecycle, including secondary windows and recreation, so creation attributes honor the intent before allocation. Evidence for the failing native behavior: `invisible-after-terminal-policy.log`.

The channel deliberately carries only sendable native data. `GuiFrameHostRequest` also contains evaluator-owned Lisp strings and stays on the evaluator thread. Explicit visibility in every request makes omitted producers a compiler error. Completion still consumes the one-shot reply and preserves its evaluator-owned lifetime.

An intermediate GUI rerun passed all ten then-existing test functions after rebuilding the editor and producing its matching runtime image, before the deferred-constructor retirement case was added. The native invisibility regression observes both the hidden state and subsequent mapping after `make-frame-visible`, rather than treating Lisp visibility alone as evidence.


## Additional verification findings

The first full startup GUI run passed seven of ten test functions. Both normal Wayland startup and caught-error recovery reached a pre-existing diagnostic bug: the compositor exposed a surface without `COPY_SRC`, the capability check disabled first-frame capture, but continuous debug capture remained requested. The GUI harness enables both modes. The continuous path attempted an invalid GPU copy and panicked. The bounded fix now disables both requests when capture is unsupported; an eight-case unit matrix verifies request/capability combinations. It leaves ordinary rendering enabled. This is separate from the Lisp startup ordering defect. Evidence: `gui-startup-final.log` and `native-unit-with-readback-final.log` (157/157 pass).

The invalid-frame test also exposed an incorrect timing boundary. Its ten-second process-wide deadline expired in cold `neo-preload.el` loading, before the malformed frame constructor ran. The fixture now writes an atomic marker from an appended `before-init-hook`, immediately before GNU startup calls `frame-initialize`. The observer permits 45 seconds for cold startup and separately requires process termination within five seconds of the marker's file timestamp. It retains terminal-only state, nonzero status, early-only durable trace, and absence-of-presentation assertions. This changes the measurement boundary rather than granting the failing constructor more time.

The live oracle controls passed 3/3 against the pinned GNU executable (`NEOVM_ORACLE_MODE=verify`). The selected window-command core group passed 384/388; a separate frame cycling/selection group passed 19/19. All nine initial-frame/terminal-deletion tests passed. The native group passed 157/157, including the new recovery, visibility ownership, and capture capability tests. Application startup/primary-frame tests passed 76/77.

Four core failures were reproduced, without rebuilding, using the cached pre-PR test binary from the tree-equivalent main control. Three assert bytecompiled arity-error payloads while this worktree loads interpreted `window.el`; the fourth expects two buffer-list hook calls but gets four. The unchanged application test additionally requires `mail/rfc6068.elc`, while this source-only worktree contains `rfc6068.el`. A public batch `locate-library` probe against the cached main runtime also resolves `.el`. These baseline failures and the source-artifact expectation remain recorded as verification limitations. Evidence: `core-unit-complete.log`, `frame-unit-final.log`, `core-unit-baseline-control.log`, `app-unit-final.log`, and `app-mail-baseline-control.log`.

The original PR's failed Linux/Windows compilation gates were also compared with its exact mergebase: compiler error fingerprints and affected source blobs match. Cargo deny already failed on that base; its additional advisory appeared with an unchanged lockfile. This does not make failed checks successful: fresh checks on the rebased tree are reported separately. Detailed job links and normalized fingerprints are in `tmp/pr586/original-ci/summary.md` and `summary.json`.


Before the deferred-constructor retirement case was added, an intermediate complete startup GUI rerun passed 10/10 then-existing test functions (`gui-startup-with-readback-final.log`), including GNU/X11 ordering, Neomacs X11 and Wayland ordering, successful and unsuccessful early exits on both backends, caught constructor errors, fatal constructor errors, no mapped X11 window during early-init, and GNU/Neomacs invisible-frame mapping transitions. The constructor failure test passed its separate five-second post-marker deadline on both backends. Wayland startup no longer panics when diagnostic capture is unsupported.


## Retiring a deferred raw constructor

A final paired GNU/Neomacs test creates an invisible raw `x-create-frame` before normal frame initialization, deletes it, and then observes the replacement opening frame. GNU passed; Neomacs initially exited with `terminal-live-p` for the replacement. A focused public API unit test independently failed because deleting the raw graphical frame removed its terminal handle and connection parameters (`graphical-connection-red.log`).

GNU toolkit builds retain an empty X/PGTK terminal connection after the last frame is deleted. Neomacs's native event loop likewise owns its graphical connection independently of Lisp frame count. Automatic deletion now uses the existing `TerminalOutputMethod` enum: graphical connections remain registered, while initial and termcap terminal cleanup keeps its previous behavior. Explicit `delete-terminal` still closes the connection and deletes its frames; the new unit test checks that boundary as well. [GNU frame/terminal lifetime](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L2970-L3002).

A second ownership problem accompanied the connection failure: deleting an unpublished primary frame permanently marked its routing slot retired. The display host now discards that pending request and returns the slot to `Awaiting`, retaining the one-shot readiness capability for a replacement. Native title, geometry, fullscreen, decorations, and hidden/iconified visibility changes remain staged until finalization. This prevents queued commands belonging to a discarded frame from changing a later window. Published primary-frame retirement retains its existing routing behavior.

`GuiFrameHostRequest::for_frame` now captures the common finalization fields from the live frame, including decorations, replacing the duplicated completion snapshots noted by Copilot. `InitialWindowPreparation` carries decorations alongside visibility before native allocation. Font-driven resize dimensions remain staged separately from the final parameter snapshot, and resize requests preserve an already staged fullscreen setting.

The strengthened GUI case also iconifies the raw frame and changes its fullscreen/decorations before deletion, then checks the replacement's actual hidden-to-mapped transition. The same paired test passed after the fixes (`retired-invisible-frame-green.log`, 18.227 seconds). It observes native visibility; it does not query window-manager fullscreen/decorations state. The focused connection/initial-lifecycle group passed 10/10, and the final broad core run passed 404/408 with all four failing assertions exactly matching the measured main control (`core-final-complete.log` and its JSON summary).

The broad TUI run passed 9/11. Both tab-line failures were reproduced using the cached main runtime with the same unchanged test harness (`tui-tab-baseline-control.log`): startup readiness was not observed before the existing deadline, and the other probe inspected echoed command text instead of settled window contents. A final startup/visibility/resize smoke run against the final executable is recorded separately.


## Final review scope

| Axis | Result |
| --- | --- |
| Standards | The inspected changes use closed routing/outcome/terminal-kind enums, exhaustive projections, evaluator-owned Lisp data, and one-shot native preparation. The shared completion constructor addresses Copilot's optional duplication finding. No remaining documented coding-standard violation was identified. |
| Spec | The original early-init violation was reproduced against main and GNU. Regression-first fixes cover startup error classification, hidden initial frames, terminal retirement, optional unsupported surface capture, and cancellation of an unpublished raw constructor. Final test results and their measured baseline failures are recorded below. |

Execution coverage is Linux debug builds with X11 and headless Wayland. GNU's NS source was inspected, but macOS and Windows were not executed. GPU error classification and pending ownership transitions have unit coverage; native device-loss/canceled-worker surface recreation was not exercised end to end. Native visibility was observed directly on X11; window-manager fullscreen/decorations state was not queried by the retirement case.


Final execution on the frozen implementation:

| Check | Result |
| --- | --- |
| Core frame/window commands | 404/408 passed; four assertion payloads exactly match the independently run main control |
| Native startup/window/communication/readback | 157/157 passed |
| Application startup/primary routing | 76/77 passed; unchanged assertion requires missing `rfc6068.elc` |
| Live GNU oracle | 3/3 passed |
| Complete startup GUI suite | 11/11 passed (GNU, X11, Wayland, including the deferred-constructor retirement case) |
| Final TTY startup/visibility/resize smoke | 3/3 passed |
| Broader recorded TUI selection | 9/11 passed; both failures reproduced on main under the same recording configuration |

The final X11 surface capture contains the requested RGB `(0, 255, 0)` as its dominant background (503,655 of 563,220 pixels, 89.42%). This confirms actual rendered output alongside the Lisp parameter and hook assertions. Capture SHA256: `c26e8ba2447c382642b171529eb43ed9af5e1aa2cb45936ca2d1c767361b2508`; observation: `tmp/pr586/x11-rendered-background-observation.json`.

Useful focused commands (with the matching built runtime, GNU executable, and headless display tools configured):

```sh
cargo nextest run -p neovm-core --lib -E 'test(terminal_frame_deletion_tests::) | test(initial_gui_lifecycle_test::)'
cargo nextest run -p neomacs-gui-tests --test gui -E 'test(issue_417_startup::)' --test-threads 1
NEOVM_ORACLE_MODE=verify cargo nextest run -p neovm-oracle-tests --test oracle -E 'test(oracle_startup_frame_lifecycle::)'
```


Both tab-line failures were additionally reproduced with recording and input pacing disabled on the final runtime and on cached main (`tui-tabs-default-final.log`, `tui-tabs-default-baseline.log`). The per-window assertion is identical in that mode: zero observed tab lines versus one expected. The startup case remains blank before its existing readiness deadline. These controls support classifying them as baseline/source-environment failures rather than new startup regressions.

Final Linux gates passed: dependency coherence; `cargo check -p neomacs --no-default-features`; the CI's filtered `--no-default-features` capability test (1/1, with eight core test binaries compiled); `cargo check --workspace --all-targets`; formatting; and whitespace checks. The filtered capability assertion is in the SQLite module; its build also verifies the harnesses compile with JIT disabled. Existing warnings remain in unchanged code.


## Premerge regressions found on published head 6e0ad91c

The earlier verification table describes the published 6e0ad91c snapshot. The
merge was held after new review and CI evidence exposed the following additional
regressions. Local paths below are unpublished workspace evidence, not attached
PR artifacts.

- The native loop did not consume the 64-slot render-command transport during
  evaluator preparation. A public GUI fixture issuing 256 alpha updates stalled
  before readiness. The loop now drains transport fairly and retains FIFO
  commands until the renderer can apply them. The paired GNU/Neo regression
  failed before the fix and passed afterwards.
- `window-system` remained the startup TTY value after GUI selection. GNU
  `data.c:kboard_for_bindings` forwards through the selected frame's keyboard;
  dynamic bindings restore their captured keyboard owner. Neo now preserves
  that model with generation-checked owners, rooted parked slots, and captured
  binding ownership. Host reassignment of the initial frame also explicitly
  synchronizes ownership. Focused Core tests passed 61/61 and separate
  thread/projection tests passed 25/25 before the final font/history changes.
- Native input arriving during GPU discovery was discarded. A Vulkan manifest
  FIFO holds discovery while the harness focuses the actual window and sends
  `z`. The test failed with `GPU-KEYBOARD-FAIL` after successful llvmpipe GPU
  completion, then passed after input became owned by the creation attempt.
  Only validated replayable events enter the FIFO; close/destruction remain
  synchronous, cancellation drops the attempt, and pointer replay waits for
  scene ingestion. Native unit selection passed 162/162. Evidence:
  `pending-gpu-keyboard-red.log`, `premerge-live-input-font-tab.log`, and
  `premerge-native-complete.log` under `tmp/pr586/`.
- GNU `xfaces.c:set_lface_from_font` retains the opened font in the frame-local
  default face. Neo seeded derived attributes but relied on `font-parameter`
  as a fallback. GNU `faces.el` legitimately clears that user-request marker;
  the opened font then became unspecified. The native Wayland contract
  reproduced the exact CI diagnostic, `Initial frame has no opened default
  font`, before the fix. The face now owns its typed `LFaceAttr::Font` value.
  The new opened-font identity/metrics unit test and the live native Wayland font/resize contract now pass.
- `old-selected-window` retained the deleted startup terminal window even
  after redisplay. GNU `window.c:window_change_record` advances this history
  after window-change callbacks. Both Neo redisplay policies now share the
  selection-recording boundary, preserving initial/batch behavior and the
  pinned Context layout. Both public Core tests failed before the fix; the
  live flat-button tab-line test independently reproduced the stale face.
  Both policy unit tests and the live flat-button GUI case now pass.
- Capture fixtures that exited after two seconds could finish before native
  pixels existed. They now wait for a newly published nonempty capture while
  the harness retains its existing process deadline. Startup settings passed
  5/5 with the matching runtime and GNU control.

The GUI keyboard experiment uses an isolated Arch software Vulkan package
verified against the repository SHA-256
`c099e085f778b3183b22a3ab81b7200dd013855c4bcbc7bb5c5189ed045311bf`;
no system driver was installed. The subsequent combined runtime uses the
repository test profile, which optimizes the interpreter, and a freshly
produced matching pdump.

Additional primary references:
[GNU window-change history](https://www.gnu.org/software/emacs/manual/html_node/elisp/Window-Hooks.html),
[upstream winit event ownership and surface-size writers](https://rust-windowing.github.io/winit/winit/event/enum.WindowEvent.html),
and the pinned
[winit event definitions](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-core/src/event.rs).

CI triage also established that the ARM64 installer ownership-check timeout
occurs on main with identical scripts. Windows focus activation succeeded in
CI but the initial key never reached Lisp, so that failure is treated as an
input regression rather than dismissed as desktop infrastructure.


### Premerge verification after repair

| Check | Result |
| --- | --- |
| Core frame/window/font/forwarding/redisplay | 536/540 passed; all four assertion payloads match main |
| Admission and thread/projection behavior | 54/54 passed |
| Native startup/window/input/transport | 162/162 passed |
| Application startup/primary routing | 76/77 passed; same missing `rfc6068.elc` assertion as main |
| Live GNU startup oracle | 3/3 passed |
| TUI startup/visibility/resize | 3/3 passed |
| Initial combined GUI regression run | 40/44 passed; each of the four failures investigated below |
| Opened-font native Wayland contract | 1/1 passed after its semantic RED reproduction |
| Native menu first-presentation witness | 3/3 passed: unchanged CJK input case plus two parser tests |
| MELPA git-gutter-fringe | Complete result/cleanup value matches; snapshot still differs only in local stderr |

The GUI selection includes all 22 cases newly failing in the published head's
CI, the complete 12-case issue-417 suite, and ten startup-failure cases. Its
remaining failures were isolated with a main runtime built in the same
repository test profile, with a matching pdump and an identical tracked tree
to main `c1ea6adf391cd1f9bd4061438614c7a8e2d17576`.

The desktop font fixture requires Ubuntu Mono. The remote host substituted
Noto Sans Mono with different metrics. An isolated official Arch
`ttf-ubuntu-font-family` package (SHA256
`8d84962b5372e5a331fd9afa3aab0569b558bb4e78e8c1dfcc5e0c16b6527e86`)
supplied the prerequisite without a system installation. The GNU control,
main control, and repaired Neo fixture all pass with those fonts.

Both small-buffer stalled-scroll presentation tests reproduce the exact same
`no projected pixels with native presentation while evaluator is stalled`
assertion on optimized main and the repaired branch. Their input does arrive;
they are distinguished from the published head's earlier input-loss failures.
Evidence: `main-optimized-gui-controls.log` and
`fixed-optimized-gui-controls.log`.

The CJK menu fixture published Lisp state before the nested Wayland compositor
had mapped its content surface. The synthetic click happened before any button
could be delivered. Main's earlier GPU initialization masked that assumption.
The fixture now identifies the primary toplevel surface and waits for its
non-null buffer commit and its own compositor frame callback before input.
Cursor/sync callbacks and bufferless initial commits cannot satisfy it. Popup
assertions, coordinates, and their existing per-stage deadlines are unchanged.
The same CJK case failed before this witness and passed afterwards.

MELPA's former `window-system` mismatch is gone. Token comparison of the entire
expected and actual git-gutter-fringe payload finds one remaining difference:
its final stderr string contains three source-only terminal-layer loading
messages and two local Mesa DRI3 warnings, instead of an empty string. All
result data, bitmap rows, geometry, and cleanup fields match. This local
snapshot test is therefore not reported as fully passing; compiled-runtime CI
is still required. Evidence: `melpa-final-differences.json`.

Post-repair Linux gates passed: dependency coherence, the minimal editor build,
the no-default-features capability control (1/1 with eight Core test binaries
compiled), workspace all-target checks, formatting, and whitespace checks.
