# Issue #417: initial frame creation belongs to Lisp startup

Investigated 2026-10-23. GNU Emacs was studied first at local revision `a360712c9d272d950d8d8255ef74570f7e90b7d9`; its checkout was used for analysis only. The baseline analysis refers to NeoMacs revision `b5030a45f6`. The implemented design is described separately; neither section substitutes for recorded runtime verification.

## GNU's observable contract

The early init file precedes package and GUI initialization; regular init runs after GUI initialization. GUI-dependent customization in early init must use suitable startup hooks rather than assume a graphical selected frame. [Early Init File](https://www.gnu.org/software/emacs/manual/html_node/emacs/Early-Init-File.html).

The startup sequence runs window-system initialization and `before-init-hook`, creates the initial graphical frame when appropriate, initializes its faces/chrome, then loads normal init and eventually runs `after-init-hook`. Batch and daemon startup do not create this graphical frame. [Startup Summary](https://www.gnu.org/software/emacs/manual/html_node/elisp/Startup-Summary.html), [pinned startup implementation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/startup.el#L1530-L1610).

`frame-initialize` creates an opening frame only when no minibuffer frame other than `terminal-frame` already exists. It combines `initial-frame-alist` ahead of `default-frame-alist`, invokes `make-frame`, copies the startup environment, and deletes the initial terminal frame. The guard intentionally preserves a GUI frame that user code created earlier. [Pinned frame initialization](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L414-L447).

`make-frame` runs `before-make-frame-hook` before creating the frame and calls `after-make-frame-functions` with the new frame afterward. Installing those hooks in early init makes them observe opening-frame creation. Tests should inspect the supplied frame in the after hook: `make-frame` itself does not select the new frame, although the WM may do so. [Creating Frames](https://www.gnu.org/software/emacs/manual/html_node/elisp/Creating-Frames.html), [pinned hook dispatch](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L1127-L1179).

`default-frame-alist` supplies defaults when a frame is created; changing it does not modify existing frames. The separate post-init application of `initial-frame-alist` explains why late changes may produce a visible appearance adjustment. Creation-time defaults and later user-setting reconciliation are distinct operations. [Initial Frame Parameters](https://www.gnu.org/software/emacs/manual/html_node/elisp/Initial-Parameters.html).

## Baseline NeoMacs lifecycle and existing seams

The baseline GUI worker prepares a font, publishes initial native dimensions, installs a graphical bootstrap frame, and sets up GNU startup variables before entering `recursive_edit`. `configure_gnu_startup_state` creates `Fstartup-tty` alongside the opening GUI frame and sets `frame-initial-frame`/`default-minibuffer-frame` to the latter. `initialize_reused_gui_startup_frame` explicitly compensates for bypassing GNU's creation path by running key-map and face-finalization side effects. [Baseline worker setup](https://github.com/eval-exec/neomacs/blob/b5030a45f6/crates/neomacs/src/main.rs), [baseline font-owned preparation](https://github.com/eval-exec/neomacs/blob/b5030a45f6/crates/neomacs/src/startup_frame.rs).

Consequently, a hidden native window alone would not solve the reported Lisp contract: the graphical logical frame still makes GNU's guard skip `make-frame`, so the frame hooks and early creation-time defaults remain bypassed. Filtering public frame enumeration would hide the same ownership defect.

There is already a usable creation seam. `term/neo-win.el` implements `frame-creation-function` using `x-create-frame-with-faces`; `x_create_frame_impl` eventually calls the display host. `PrimaryWindowDisplayHost::realize_gui_frame` already distinguishes first-primary adoption from additional native windows, and `opening_gui_frame_pending` is available before adoption. [Neo window-system adapter](../../lisp/term/neo-win.el), [core creation](../../crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs), [display-host interface](../../crates/neovm-core/src/emacs_core/display/display_host/mod.rs).

## Baseline design conclusions

The following are design inferences, not GNU implementation requirements for Rust host threading:

- Bootstrap only the initial terminal frame as the Lisp-visible frame. Keep the intended GUI backend in `initial-window-system`; the selected startup frame remains a terminal until Lisp requests GUI creation.
- Install the GUI host before running GNU startup. Let `frame-initialize` → `make-frame` → `x-create-frame-with-faces` own initial graphical creation, parameter precedence, hooks, faces and terminal deletion, using the same path as later frames.
- Represent first-native-frame preparation as owned enum state rather than independent booleans: pending readiness reply, prepared request/owned font, and adopted frame with its native lifetime. Publish first-window readiness only after final creation parameters and opened-font dimensions are available. Keep Lisp values and font handles on the evaluator thread.
- Continue pumping the OS main event loop while the evaluator runs early init or prepares a font. Pending startup must not allocate a window/GPU merely to let early init proceed, and the native thread must not wait for evaluator work that itself needs native dispatch. This preserves the existing [nonblocking startup contract](../plans/2026-09-13-nonblocking-startup.md) and [thread ownership rules](../plans/2026-04-26-gui-main-thread-evaluator-worker-design.md).
- Preserve typed failure and exit outcomes. Dropping a pending reply after early `kill-emacs 0` is graceful evaluator completion, not necessarily preparation failure. Native display loss while a worker is blocked in external I/O must still terminate without waiting for that worker.

Two baseline implementation details required explicit auditing. The input bridge rewrote native resize frame ID zero to the original bootstrap frame ID; a genuinely new opening GUI frame needs its actual adopted identity. Also, `x_create_frame_impl` captured geometry before explicit font synchronization and later sent those captured dimensions to `realize_gui_frame`; first native allocation must instead use the final opened-font geometry. Old adoption ignored requested dimensions, masking this distinction.

## Implemented ownership and integration

GUI startup now bootstraps only the initial terminal frame and installs the graphical display host before running GNU startup Lisp. `initial-window-system` preserves the intended backend while the initial frame remains terminal. GNU's own `frame-initialize` and `make-frame` perform opening graphical creation, parameter precedence, frame hooks and terminal retirement. The pre-startup adoption path is no longer the production GUI entry point.

`DisplayHost::prepare_initial_gui_frame` resolves defaults when Lisp requests the first top-level GUI frame. Its `InitialGuiFrameDefaults` bundles graphical display identity, the opened font's identity/metrics/height, and initial dimensions. Core frame creation installs that font and seeds the frame-local default-face baseline before GNU's face wrapper applies delayed user specifications. This avoids inheriting terminal-cell geometry or treating unspecified face attributes as permission to replace the opened font. Font objects remain in evaluator-owned frame parameters; no Lisp object crosses to the native thread. [Creation and font baseline](../../crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs), [host contract](../../crates/neovm-core/src/emacs_core/display/display_host/mod.rs).

The display host collects geometry changes while the initial logical frame is being finalized. `finish_gui_frame_creation`, reached through `frame-after-make-frame` or explicit visibility, publishes the final native dimensions after delayed face/font settings. A raw visible `x-create-frame` completes after its font and faces are initialized; the Lisp faces wrapper requests an invisible raw frame and retains deferred completion. Later frames retain ordinary creation and resize paths. `PrimaryFrame::{Awaiting, Adopted, Retired}` makes the primary assignment explicit; the input bridge obtains the adopted frame identity instead of routing first native resize events to the retired terminal.

`InitialWindowOwner` retains `Waiting`, `Running` or `Closed` readiness state on the evaluator stack. Hosts hold only a weak `InitialWindowPublisher`, so deliberately retaining the evaluator for process exit cannot retain the startup reply or active native lifetime. Completion consumes readiness once; a dropped owner wakes the native loop even if its host remains allocated. [Typed completion ownership](../../crates/neomacs/src/startup_frame.rs).

Display metadata is independent of frame allocation. While waiting for first-frame readiness, the native event loop samples monitors through `ActiveEventLoop::available_monitors`, publishes shared metadata and wakes the worker's bounded metadata reader. The evaluator installs that snapshot before init can query graphical dimensions. This requires neither a hidden native window nor GPU/surface permission; it avoids a readiness cycle between Lisp frame creation and native metadata. Ordinary monitor refresh reuses the same publication helper. [Window-free metadata publication](../../crates/neomacs-display-runtime/src/render_thread/startup.rs), [shared monitor collector](../../crates/neomacs-display-runtime/src/render_thread/lifecycle.rs).

Preparation errors become Lisp errors first, so user handlers can recover. Only an error escaping the startup command-loop boundary calls `fail_pending_gui_startup` to publish terminal preparation failure. Successful early `kill-emacs`, a requested nonzero exit, explicit preparation failure and native interruption retain distinct outcomes. The native loop also recognizes evaluator shutdown during GPU initialization, avoiding a race between a queued shutdown command and the stack owner's later drop. Native display loss during blocked evaluator I/O still uses the non-joining interruption path. [Startup error boundary](../../crates/neovm-core/src/emacs_core/runtime/eval/command_loop.rs), [native outcome handling](../../crates/neomacs-display-runtime/src/render_thread/startup.rs).

## Acceptance evidence to collect

Use the actual executable with isolated init directories and GNU as the oracle. At early-init entry, assert that only the initial terminal frame exists and `display-graphic-p` is nil. Set distinguishable initial/default parameters and install the two frame hooks there. Assert hook ordering and inspect the new frame argument's background before normal init. By `init.el` and `after-init-hook`, require the graphical frame and creation-time background, with the startup terminal gone.

Controls should include explicit initial fonts/geometry, user-created frames during early init, errors or nonzero exit during startup, successful early `kill-emacs`, ordinary `-Q`, batch, TUI and daemon startup, and the existing blocked-font/native-display-loss regression. X11 and Wayland execution provide Linux evidence; they do not establish AppKit behavior on macOS. Record the actual reproduced state and final test results separately from these source recommendations.

## Executed reproduction and verification

The regression was reproduced against baseline `b5030a45f6` with a rebuilt development executable and matching runtime images. The GNU X11 control passed; NeoMacs X11 and Wayland both failed the first early-init observation because a graphical frame was already exposed. The new fixture installs frame hooks and sets the green default background in early-init, then observes regular init and subsequent startup hooks. A separate X11 file barrier checks the actual visible native window tree while early-init remains active.

Additional failing regressions drove corrections for desktop-font face initialization, monitor dimensions during init, invalid initial-frame failure status, and raw visible-frame completion. The raw-frame unit test was executed through nextest's build-reuse support against the compiled pre-fix test binary, then passed after rebuilding with the fix.

The final GUI pass used `target/issue583/debug/neomacs`, built with `cargo build -p neomacs --features video`, with freshly regenerated bootstrap/final runtime images. The runtime-generation command used `--skip-build --no-byte-compile`; generated `loaddefs.el` was then byte-compiled explicitly. The stale-bytecode guard remained enabled. These are development-build results, not release-build measurements.

All test runs used `cargo nextest`. Focused verification passed:

| Coverage | Passed |
| --- | ---: |
| Core window/frame unit tests | 379 |
| Frame builtin bytecode-dispatch control | 1 |
| Application startup/frame unit tests | 73 |
| Native startup/frame-window unit tests | 73 |
| Live GNU frame/common-window oracle tests | 3 |
| TUI resize, frame visibility and init tab-line tests | 8 |
| GUI startup lifecycle, startup failures and font/HiDPI controls | 20 |

The GUI cases include GNU X11 comparison, NeoMacs X11 and Wayland, creation-time background/hooks, positive monitor dimensions during init, successful/nonzero early exits, an invalid initial frame, recovery from a caught malformed frame request, blocked font loading, native display loss, GPU-startup cancellation/resize, explicit CLI fonts and HiDPI geometry restoration. Linux display tests used isolated Xvfb/Weston sessions. The original macOS platform was not available for execution.
