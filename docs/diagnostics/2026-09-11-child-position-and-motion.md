# Child-frame placement and modified mouse motion

## Reproduction and GNU reference

On Linux/headless Wayland, opening the reported Rust playground `src/main.rs`
with the user's configuration starts rust-analyzer. `lsp-ui-doc-show` at `main`
produces the real `fn main()` hover. The package requests `(left . (+ 248))`
and `(top . (+ 545))`, but the rendered child remains at `(-1, -1)`.
The reduced GUI fixture creates at `(10, 20)`, applies those parameters,
and observes `(10, 20)` in the frame snapshot on the unfixed binary.

GNU sources studied in `/home/exec/Projects/github.com/emacs-mirror/emacs`:

- `src/frame.c`: `gui_set_frame_parameters`, `gui_figure_window_size`,
  `frame_float`, and `tty_child_pos_param`. `(+ N)` is absolute, including
  negative N. Negative integers, `(- N)`, and the symbol `-` express far-edge
  placement. Fractions use the available space after accounting for frame size.
- `src/keyboard.c`: `make_lispy_movement` always uses `mouse-movement`, without
  keyboard modifiers. `lucid_event_type_list_p` distinguishes event-type lists
  from position-bearing mouse events.
- `src/keymap.c`: `Fsingle_key_description` handles Lucid events and character
  ranges, then extracts `EVENT_HEAD` before formatting ordinary events.

Replaying the reported `C-mouse-movement` event through `key-description` and
the isolated GUI's command loop reproduces `KEY must be an integer, cons,
symbol, or string`. GNU's builtin prints `C-<mouse-movement>` for that input.
The native-input conversion test separately catches the incorrect modifier
prefix; no synthetic binding is installed to conceal that error.

## Design

`emacs_core/display/frame/position.rs` owns Lisp position decoding and geometry
resolution. `FramePositionSpec` has private construction around a closed enum:
absolute, far-edge, or proportional intent. Native-sized integer ranges and
fractions are checked at admission. Creation and parameter updates share the
same resolver, after size/font changes, and publish numeric parent-local
coordinates. The renderer does not decode Lisp or know about LSP.

Top-level desktop placement/workarea support is not added by this fix. Without
a parent extent, the existing offset/zero-fraction behavior is preserved; this
does not claim complete native top-level placement support across platforms.
The separate GNU TTY creation path is also not migrated by this GUI fix.

Mouse-motion construction has no modifier argument, unlike button/key gesture
construction. Input transport can still retain modifier state for other
consumers. Key-description handles a position-bearing event's head without
interpreting its payload as a Lucid modifier list, or stripping nested wrappers
recursively. Existing character-range and Lucid-event behavior is retained.

## Agreed tests

- `crates/neomacs-gui-tests/tests/child_frame_position.rs` and its Lisp fixture:
  actual frame snapshots for creation, update, signed absolute positions,
  far-edge positions, proportional positions, and simultaneous resize/move.
- `crates/neovm-core/src/keyboard_test.rs`: native motion input to Lisp events,
  with no modifiers, Control, and all modifiers, retaining position payloads.
- `crates/neovm-core/src/emacs_core/lisp/native/builtins/tests/keymaps.rs`:
  public Lisp key-description behavior for the reported event.

Run GUI coverage with a matching binary/pdump:

```sh
NEOMACS_GUI_TEST_BACKEND=wayland cargo nextest run -p neomacs-gui-tests \
  --test child_frame_position --run-ignored all
```

GUI tests stay out of `tui-tests`. No test is stored in `/tmp`; that directory
is used only for command logs. Native GUI verification is Linux Wayland only.

## Verification on the fixed build

- The same final GUI fixture fails on the existing release binary and passes
  on the rebuilt debug binary, including all six placement scenarios.
- 388 selected window/keyboard/key-description tests pass; the final expanded
  key-description run passes all 10 selected cases.
- Original Rust file + user configuration + initialized rust-analyzer:
  LSP requests `(248, 532)` and both `frame-position` and the redisplay snapshot
  report `(248, 532)`, with the real `fn main()` hover content.
- The fixed GUI formats the reported event as `C-<mouse-movement>` and dispatches
  an ordinary `mouse-movement` through the LSP binding without the type error.

The GUI fixture waits for the initial native configure before positioning
children. Its first expanded run exposed a fixture race: parent dimensions
changed from `664x682` to `658x667` after placement. Far-edge/proportional
requests use geometry at application time, not a promise to follow future
parent resizes. Waiting before construction preserves that GNU contract;
the placement assertions themselves were not relaxed.

The verification binary and matching pdump are isolated under
`target/diagnostics/lsp-child-placement/fixed/`. The release artifacts were
not replaced; no autoload generation or Lisp byte compilation was run.

Subsequent release verification: `cargo build -p neomacs --release` and the
matching raw pdump step succeeded. The release binary passed the same six
Wayland placement scenarios, formatted the reported mouse event correctly,
and dispatched ordinary LSP motion. With the original Rust file and user
configuration, the hover requested `(248, 533)` and the release frame snapshot
reported `(248, 533)`. No autoload generation or byte compilation was run;
`ldefs-boot.el` remained unchanged.

## Follow-up: posframe's bottom-right handler (issue 120)

The report at
<https://github.com/eval-exec/neomacs/issues/120#issuecomment-5571041480>
uses `posframe-poshandler-frame-bottom-right-corner`. Running the same example
from `/tmp/a.el`, with local posframe 20260527.857 and `-Q`, reproduced the
missing popup on Linux/headless Wayland even after the first placement fix.
The buffer and redisplay glyphs contained `TestABC`, but the child was placed
at `(-1, -36)` instead of `(584, 589)` for a 658x667 parent and 73x42 child.

This is a different public entry point into the same placement semantics.
Posframe intentionally calls `set-frame-position` with negative edge offsets.
The previous fix covered creation and `modify-frame-parameters` but left that
builtin writing the offsets directly into resolved coordinate fields.
Calling the parameter API and then the positioning API on the same child
confirmed that only the latter bypassed resolution.

GNU reference:

- `src/frame.c:4662`, `Fset_frame_position`: GUI negative coordinates mean
  right/bottom-relative placement; TTY children retain literal coordinates.
- `src/w32term.c`, `w32_set_offset` and `w32_calc_absolute_position`: mark
  negative axes and resolve them using the parent and child extents.
- `src/frame.c:4704`, `Fset_frame_size_and_position_pixelwise`: negative values
  are signed absolute coordinates, including the fallback through `(+ N)`
  parameters. Sharing a resolver must not erase this distinction.

The position module now admits strict integer requests through the closed
`FrameCoordinateOrigin::{SignedEdges, Absolute}` enum. `FramePositionSpec`
keeps its private representation and validates native coordinate bounds.
All four GUI callers (creation, parameters, positioning, compound pixelwise
size/position) use `apply_frame_position`; neither integer builtin writes
resolved coordinates directly. Rendering still consumes numeric placement,
not Lisp expressions or package-specific conventions. This is a shared
placement implementation, not a posframe special case. Existing top-level
desktop/workarea limitations remain out of scope.

Verification:

- The GUI regression was run before changing production code: actual
  `(-1, -36)`, expected `(535, 575)` for its fixture geometry.
- The fixed debug binary passes the expanded nine-case GUI fixture, including
  posframe-style negative offsets, mixed signs/zero, and signed absolute
  compound pixelwise placement.
- 312 selected frame/window tests pass via `cargo nextest`, including the
  out-of-line `frame_position_test.rs` coordinate-range check. Its expected
  errors were obtained directly from GNU Emacs.
- Re-running the real `/tmp/a.el` on the fixed debug binary produces a visible
  child at `(584, 589)` and a redisplay snapshot containing `TestABC` there.
  Artifacts are under `target/diagnostics/issue-120-comment/fixed/`.

The issue reporter used Windows. These results reproduce and fix the shared
coordinate bug on Linux Wayland; they do not claim Windows-native validation.

Final release verification: `cargo build -p neomacs --release` succeeded in
7m 05s. Its freshly generated matching pdump starts normally. The release
passes the nine-case GUI regression and the original `/tmp/a.el` example:
the snapshot contains `TestABC` at `(584, 589)`. Three focused core tests were
also rerun successfully after the final resolver rename. No byte compilation
or autoload regeneration was performed, and `ldefs-boot.el` is unchanged.
Release logs are `/tmp/neomacs-posframe-release-{build,pdump,gui}.log`; the
original-example snapshot is
`target/diagnostics/issue-120-comment/release-a-el-frame.json`.
The standalone headless run logged one Vulkan surface-capability
`ERROR_SURFACE_LOST_KHR` during startup, then continued through the example,
snapshot assertion, and normal shutdown without a panic. The verification
here asserts redisplay geometry/glyphs, not a compositor screenshot.
