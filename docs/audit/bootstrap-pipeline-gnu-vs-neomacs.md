# Bootstrap Pipeline Audit: GNU Emacs vs Neomacs

**Date**: 2026-03-28

## Executive Summary

Neomacs loads the **exact same `loadup.el`** from the GNU Emacs lisp/ directory,
but that does **not** mean it produces an equivalent dumped state. The current
design is similar at the top-level file-loading layer, yet it still diverges
from GNU Emacs in bootstrap construction details such as builtin registration,
post-cache repair work, startup shims, and some function/object shapes.

So the right conclusion is:

- the top-level Lisp bootstrap path is intentionally close to GNU Emacs
- the current bootstrap is **not** equal to GNU Emacs by construction
- semantic parity must be established by differential tests, not inferred from
  "same `loadup.el`"

---

## Phase 1: Bootstrap/Dump Phase (loadup.el)

### GNU Emacs

1. `temacs` binary starts with minimal C core
2. C `main()` sets `Vtop_level = "(load \"loadup.el\")"` and enters `Frecursive_edit()`
3. `loadup.el` runs with `dump-mode` set (`"pdump"` or `"pbootstrap"`)
4. Loads ~80+ .el files in exact sequence
5. Platform-specific files gated by `(featurep 'x)`, `(featurep 'pgtk)`, etc.
6. `(dump-emacs-portable "emacs.pdmp")` serializes state
7. `kill-emacs` terminates temacs

### Neomacs (`neomacs-bin/src/main.rs` + `neovm-core/src/emacs_core/lisp/load/mod.rs`)

1. Rust `main()` calls `create_bootstrap_evaluator_cached_with_features(&["neomacs"])`
   (`load.rs:2673`)
2. On first run: loads **the same `loadup.el`** from `lisp/` directory
   (`load.rs:2596-2598`)
3. Sets `dump-mode` to `nil` (not `"pdump"`) so loadup.el skips the
   dump/kill-emacs section
4. loadup.el runs with `(featurep 'x)` → **false** (neomacs feature, not x)
5. After loadup.el completes, **explicitly loads** `term/common-win` and
   `term/neo-win` (`load.rs:2636-2646`)
6. Saves result to `.pdump` cache file for subsequent runs

### Verdict: PARTIALLY COMPATIBLE

The core bootstrap loads the same top-level file, but several construction
details still differ in ways that can matter semantically:

| Aspect | GNU Emacs | Neomacs | Compatibility Risk |
|--------|-----------|---------|-------------------|
| `dump-mode` | `"pdump"` / `"pbootstrap"` during dump | `nil` during bootstrap load | **MEDIUM** — this changes which `loadup.el` branches execute and interacts with eval-depth and startup defaults |
| Window system in loadup | `(featurep 'x)` / backend features select platform Lisp | `(featurep 'x)` is typically false for neomacs bootstrap | **MEDIUM** — `term/common-win` / `term/neo-win` are loaded separately after `loadup.el` |
| Builtin registration after cached bootstrap | Builtins are present as part of dumped runtime construction | Cached bootstrap is repaired in Rust after load | **MEDIUM** — Neomacs re-runs builtin registration and runtime surface repair after cache restore |
| Native compilation | `(featurep 'native-compile)` gates trampoline setup | No native comp | **LOW** — neomacs doesn't support native comp yet |
| `Snarf-documentation` | Runs during dump flow, with fallback handling | May fail silently in the non-dumping bootstrap path | **MEDIUM** — see below |

---

## Phase 2: Post-Dump Runtime Startup

### GNU Emacs (after pdump load)

1. C `main()` restores from pdump
2. Sets `command-line-args`, `invocation-name`, etc.
3. Enters `Frecursive_edit()`
4. `recursive_edit` evaluates `Vtop_level` → `(normal-top-level)`
5. `normal-top-level` → `command-line`
6. `command-line` sequence:
   - Sets locale (`set-locale-environment`)
   - Loads `site-start.el` (site-run-file)
   - Loads `early-init.el`
   - Package activation (`package-activate-all`)
   - `window-system-initialization` dispatches to
     `(cl-defmethod ... (window-system x))` → `term/x-win.el`'s method
   - Runs `before-init-hook`
   - `frame-initialize` — creates GUI frame
   - Loads user init file (`~/.emacs.d/init.el`)
   - Sets `after-init-time`, runs `after-init-hook`
   - `command-line-1` processes remaining args
   - Runs `emacs-startup-hook`, `term-setup-hook`
   - `frame-notice-user-settings`
   - Runs `window-setup-hook`
   - Displays splash screen

### Neomacs (`neomacs-bin/src/main.rs`)

1. Rust `main()` creates evaluator from pdmp cache
2. `bootstrap_buffers()` — creates *scratch*, *Messages*, minibuffer, first
   frame
3. `apply_runtime_startup_state()` — minimal post-dump setup (icons, scratch
   mode, closure filter)
4. `configure_gnu_startup_state()` — sets `command-line-args`, `window-system`,
   `initial-window-system`, `terminal-frame`, `invocation-name`, etc.
5. Spawns render thread (winit/wgpu)
6. Sets up input bridge and redisplay callback
7. Enters `evaluator.recursive_edit()`
8. `recursive_edit` evaluates `top-level` → `(normal-top-level)` → **same
   `command-line` function from startup.el**

### Verdict: Same top-level Lisp startup path, but not yet proven equal

The critical insight: **Neomacs runs the same `startup.el` `command-line`
function**. The `top-level` variable points to `normal-top-level` just like GNU
Emacs. The divergence is in what happens *before* entering that Lisp code and
in what runtime state has been preconstructed for it.

---

## Compatibility Risk Analysis

### 1. `window-system-initialization` Timing — MEDIUM RISK

GNU: `command-line` calls `window-system-initialization` which calls
`x-open-connection`.
Neomacs: `window-system-initialization` dispatches to `neo-win.el`'s
`cl-defmethod` which calls `x-open-connection` (a Rust builtin stub). The
render thread is already running.

**Status**: Intended to be compatible, but this still needs differential
testing. Running the same top-level Lisp method is not enough if the precreated
frame/display state differs.

### 2. Frame Pre-Creation vs Lazy Creation — MEDIUM RISK

GNU: `command-line` creates the first GUI frame via `frame-initialize` after
`window-system-initialization`.
Neomacs: `bootstrap_buffers()` creates the frame **before** `recursive_edit`.
When `command-line` runs `frame-initialize`, it may find a pre-existing frame.

**Risk**: `frame-initialize` in startup.el may behave differently if a frame
already exists. Looking at the code, `frame-initialize` uses
`make-initial-frame` which calls `make-frame` — Neomacs's
`PrimaryWindowDisplayHost` handles this by "adopting" the pre-existing window.
**Should be OK** but could cause edge-case issues with frame parameters.

### 3. `inhibit-startup-screen` Forced to `t` — LOW RISK

Neomacs hardcodes `inhibit-startup-screen` to `t` in
`configure_gnu_startup_state` (main.rs:1177). The comment says "its fill-region
is extremely slow through with_mirrored_evaluator."

**Risk**: Users won't see the splash screen. This is intentional and documented.

### 4. `terminal-frame` / `frame-initial-frame` — MEDIUM RISK

GNU: Creates a non-visible TTY "terminal frame" and a visible GUI frame.
`terminal-frame` is the TTY frame.
Neomacs: `ensure_gnu_startup_terminal_frame` creates a hidden non-GUI frame as
`terminal-frame` (main.rs:1189-1237). The GUI frame is `frame-initial-frame`.

**Risk**: The frame ordering and visibility semantics should match, but the
exact frame lifecycle (delete/recreate during startup) could differ.

### 5. `site-start.el` / `early-init.el` / Init File Loading — INTENDED SAME PATH

These are all handled by `startup.el`'s `command-line` function, which Neomacs
runs from the same GNU Lisp sources. But this still depends on Neomacs matching
GNU's loader semantics, CLI argument forwarding, and startup variable state.

### 6. Hook Execution Order — INTENDED SAME ORDER

Since Neomacs runs the same `startup.el` code, the intended hook order is:

1. `before-init-hook`
2. `after-init-hook`
3. `emacs-startup-hook` + `term-setup-hook`
4. `window-setup-hook`

### 7. Package Initialization — INTENDED SAME PATH

`package-activate-all` runs during `command-line` if
`package-enable-at-startup` is non-nil. Same code path.

### 8. GC Disabled During Startup — LOW/MEDIUM RISK

Neomacs sets `evaluator.set_gc_threshold(usize::MAX)` before entering
recursive_edit (main.rs:659), disabling GC during startup. GNU Emacs runs GC
normally.

**Risk**: Higher memory usage during startup, plus possible semantic exposure
for code that depends on GC timing or weak object behavior. This is not the
highest-risk item, but it is more than a pure performance note.

### 9. `function-get` Override — LOW RISK

Neomacs overrides the Elisp `function-get` with a Rust builtin
to avoid excessive eval depth during macroexpand.

**Risk**: This is a compatibility shim, not a GNU-equal construction. It should
be covered by differential tests instead of being assumed equivalent.

### 10. `load-source-file-function` — PARTIALLY VERIFIED

GNU sets `load-source-file-function` in loadup.el (line 147). Neomacs runs the
same loadup.el, and the current loader does consult it for source-file loads.

**Risk**: Source-load handling is closer than before, but recursive-load
limits, `.elc` paths, and cache shortcuts still need differential coverage.

### 11. `.elc` / `.neobc` Loading Differences — MEDIUM RISK

Neomacs supports `.elc`, does **not** support compressed `.elc.gz`, and also
adds a NeoVM-only `.neobc` cache path for `.el` source files.

**Risk**:
- `.elc.gz` incompatibility is a real loading gap
- `.neobc` must remain observationally invisible to Lisp
- byte-code vs interpreted function shape can still diverge after bootstrap

### 12. `emacs-build-number` / Repository Version — LOW RISK

GNU: loadup.el computes `emacs-build-number` from existing binaries and sets
`emacs-repository-version`.
Neomacs: loadup.el runs with `dump-mode=nil`, so the build-number section is
skipped (guarded by `(if (and (or (equal dump-mode "dump") ...)`).

**Risk**: `emacs-build-number` may be undefined. `emacs-repository-version`
won't be set. Minor — most code doesn't depend on these.

### 13. `Snarf-documentation` Skipped — MEDIUM RISK

GNU: Runs `(Snarf-documentation "DOC")` during loadup.el to map doc strings.
Neomacs: With `dump-mode=nil`, the Snarf path still runs (loadup.el:481-483
`condition-case nil (Snarf-documentation "DOC")`) but may fail silently if the
DOC file doesn't exist or the builtin doesn't work the same way.

**Risk**: Documentation strings for builtins may be missing.
`(documentation 'some-builtin)` could return nil or error.

### 14. Post-Cache Runtime Repair — MEDIUM RISK

GNU Emacs restores a dumped runtime whose builtin/function surface already has
the expected shape. Neomacs performs post-cache repair work after loading a
cached bootstrap image, including builtin registration and some runtime keymap /
window-system normalization.

**Risk**: Compatibility here depends on the repair logic staying perfectly in
sync with GNU runtime state. That is workable, but it is not equal by
construction.

---

## Summary Table

| Area | Risk | Details |
|------|------|---------|
| loadup.el top-level entry | MEDIUM | Same file, but not same bootstrap construction |
| Window system initialization | MEDIUM | Same Lisp entry point, different pre-state |
| Hook ordering | MEDIUM | Intended same via startup.el, still needs full oracle coverage |
| Init file / early-init loading | MEDIUM | Same startup path, dependent on loader/CLI parity |
| Package activation | MEDIUM | Same startup path, not independently proven |
| Frame lifecycle during startup | MEDIUM | Pre-created frame may cause edge cases |
| Cached bootstrap repair | MEDIUM | Builtins and some runtime state are repaired after cache load |
| `.elc` / `.neobc` handling | MEDIUM | `.elc` supported, `.elc.gz` unsupported, `.neobc` adds a NeoVM-only path |
| Documentation strings (Snarf) | MEDIUM | May be missing for builtins |
| Splash screen disabled | LOW | Intentional, users can override |
| GC disabled during startup | LOW/MEDIUM | Mostly performance, but timing-sensitive code can notice |
| build-number / repo version | LOW | Variables undefined, rarely used |
| `function-get` override | LOW/MEDIUM | Compatibility shim, should be tested not assumed |
| Native compilation | LOW | Not supported, but not expected |

**Bottom line**: The bootstrap pipeline is **directionally close but not yet
GNU-equal**. The same `startup.el` → `normal-top-level` → `command-line` path
does run, but Neomacs still relies on bootstrap-time and post-cache repair
logic that GNU Emacs does not need. That makes bootstrap compatibility a real
audit target, not a box that can be checked just because `loadup.el` is shared.
