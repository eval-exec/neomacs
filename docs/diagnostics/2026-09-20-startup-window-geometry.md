# Configured startup and window geometry

The original macOS startup stopped with `Failed to apply resizing #<window 1
on *scratch*>`. Advice on `window-resize-apply` captured a parent of height
20 pixels with two children each staged at 20 pixels. The rejection happened
in zoom-mode during warning-buffer display. This is a real geometry failure,
not a compiled-Lisp version mismatch.

## Reproduction and GNU source

Two independently reproducible defects contribute to the geometry problem:

* Graphical `display-pixel-width/height` returned 80 and 25. The user's macOS
  configuration multiplies these by 0.6 and 0.7 and requests a 48 by 17 pixel
  text area. A trace captured the resulting root window at three pixels high.
* After a pixelwise one-pixel window resize, `window-resize-apply-total`
  reconstructed pixel bounds from rounded character totals, losing the pixel.
  GNU Emacs preserves it. Small parent/child allocations can be damaged by
  this same conversion, although the original trace does not identify every
  intermediate mutation that produced its invalid tree.

GNU `src/nsfns.m` display queries delegate to `src/nsterm.m`, which unions
NSScreen rectangles in logical points. X queries in `src/xfns.c` use device
pixels. Neomacs already receives physical monitor snapshots before loading
user init; these are the appropriate source for graphical display queries.
Winit's macOS monitor position converts logical CGDisplay coordinates using
that monitor's scale, so reversing this conversion before taking the union
also handles monitors with different scales.

GNU `src/window.c:window_resize_apply_total` changes only character totals and
character origins. It retains `new_total`; it never writes pixel bounds.
`Fwindow_resize_apply_total` does the same for the minibuffer. The optional
`floor` and `ceiling` forms of `window-total-height/width` independently measure
pixel dimensions; their default forms return the assigned character totals.

GNU also clamps requested frame sizes in `src/frame.c:adjust_frame_size` and
validates split constraints before mutation in `src/window.c:split_window_internal`.
Those are distinct safeguards; this patch does not claim to port them.

## Design

The desktop module owns the platform coordinate convention, expressed as an
exhaustively matched enum. It computes the monitor union and reports missing
or invalid geometry instead of inventing terminal dimensions for a GUI.

Window nodes separately own assigned character dimensions and pixel bounds.
Character assignment only commits character dimensions and origins, including
the minibuffer. An unassigned dimension is explicit (`Option`); pixel changes
invalidate the affected assignment, while font-metric resynchronization
invalidates both. Existing tree cloning also preserves the character state.
This keeps the two resize passes independent without adding another parallel
window tree or a second host protocol.

## Regression evidence

The display dimension test failed with 80 instead of 3024 on Linux. The
character-assignment test failed because pixel geometry changed. The rounded
query test failed with `(3 4 nil nil)` instead of `(3 4 t t)`.

Before rebuilding the editor, both new real GUI regressions failed and the
same one-pixel resize fixture passed in GNU Emacs 31.1. After the core changes,
all 633 selected window and display tests passed using cargo nextest.

Detailed before/after artifacts and build logs are under `tmp/resize-review/`.

To rerun the focused GUI regression after a fresh release build:

```sh
TMPDIR="$PWD/tmp" cargo nextest run -p neomacs-gui-tests \
  --test display_window_geometry --test-threads 1
```

Linux uses an isolated Xvfb display and additionally runs GNU GUI Emacs as an
oracle. macOS uses the logged-in desktop and runs the two Neomacs scenarios.
The fixture exits with an explicit failure if either the display query or
pixelwise resize contract is violated; it does not require the user's config
or zoom-mode to expose these defects.

## Configured macOS startup after the fix

The release build completed through `cargo xtask fresh-build --release`.
Both GUI regressions passed on yibie-et. A new instance launched without `-Q`
loaded `/Users/chenyibin/.emacs.d/init.el` and reached a post-init timer with:

```elisp
(:display (1512 982)
 :frame (930 735)
 :root (16 36 930 695)
 :init-error nil
 :theme-colors ("#ffffff" "#FEF8DE")
 :theme-loaded t)
```

The captured Messages, Warnings, and runtime log contain no resizing failure;
no Backtrace buffer was present at capture. Config source files still produce
lexical-binding/deprecation warnings, and fontdb still reports the malformed
Sauce Code Pro font. These are not treated as fixed. The probe itself also
produced one lexical-binding warning. The earlier configured instance was left
running; only our earlier diagnostic instance was stopped.

Artifacts are in `tmp/resize-review/fixed-startup/` on the Mac and copied to
`tmp/resize-review/macos-fixed-startup/` locally. This verifies startup and Lisp
geometry through the real GUI, not a screenshot assertion: macOS denied the
separate SSH `screencapture` attempt.

## Final verification

Both fresh release builds completed (Linux retains `--features webview`).
All runs below used cargo nextest:

* Core window/display suite: 633 passed.
* Additional frame/font suite: 71 passed.
* Linux GUI geometry and existing resize-presentation suite: 8 passed,
  including the GNU oracle; none skipped.
* macOS GUI geometry suite: 2 passed; none skipped.
* Linux TUI split/resize selection: 6 passed. This includes both split axes,
  switching windows, opening a file in the right split, and two terminal resize
  scenarios. The TUI suite has one combined test target named `tui`.

Rustfmt checks for changed Rust files and `git diff --check` passed. The initial
TUI command used nonexistent per-file test targets and was corrected before
this successful run. No test failure was hidden or converted into a skip.
