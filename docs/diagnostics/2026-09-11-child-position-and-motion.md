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
