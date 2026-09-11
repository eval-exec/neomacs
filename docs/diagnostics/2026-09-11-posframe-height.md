# Posframe reserves a nonexistent mode-line row

## Reproduction and GNU reference

The original `/tmp/a.el` example creates a `TestABC` posframe with a three-pixel
border. On Linux/headless Wayland, the release after the positioning fix gave
it 36 pixels of text height (two 18-pixel lines), or 42 pixels including borders.
Even `(posframe-show "*HeightMinimal*" :string "TestABC")` reproduces the extra
line without borders or a position handler.

The same original example in GNU Emacs's local GUI build, under Xvfb as a
reference, gives one 20-pixel line plus six border pixels. Fonts differ, but
the count of text lines is the comparison, not the absolute font height.
No GNU source was modified. Neomacs native verification is Wayland only.

Immediately before posframe calls `fit-frame-to-buffer-1`, Neomacs reports:

```text
frame-text-height: 18
window-pixel-height: 18
window-body-height: 0
mode-line-format: nil
window-text-pixel-size: (56 . 18)
```

Text measurement is correct, and an explicit one-line resize succeeds.
The fault is the body-height fallback subtracting an unconditional mode-line
row before the child has a redisplay snapshot. GNU's fitting code preserves
the apparent text-minus-body difference, adding this phantom 18-pixel row to
the correctly measured text. Refitting after redisplay returns to one line.

GNU references studied:

- `src/window.c`, `window_body_height`: deduct actual tab/header/mode lines,
  horizontal scroll-bar area and bottom divider.
- `src/window.c`, `window_wants_mode_line`, `window_wants_header_line`,
  `window_wants_tab_line`: live buffer/window formats, `none` overrides,
  minibuffer exclusion and available-height predicates.
- `src/window.h`, `WINDOW_MODE_LINE_HEIGHT`: absent mode line has zero height.
- `lisp/window.el`, `fit-frame-to-buffer-1`: preserve text-minus-body space.

## Design

- `neovm-core/src/window/chrome.rs` owns `WindowChromePresence`. Its private
  fields are resolved using the typed `WindowLayoutVariable` symbol IDs;
  queries use the existing closed `WindowChromeLine` enum. Buffer-slot values
  include defaults. Layout and query code share the same presence rules.
- `display/window_cmds/body_geometry.rs` owns body/chrome pixel-height queries.
  Before materialized redisplay regions exist, it uses synchronous window
  bounds, live chrome presence, recorded positive heights or canonical line
  estimates, and scroll-bar/divider areas. Body-height and text-height no
  longer have independent one-row fallback formulas. Chrome-height builtins
  use the same calculation instead of reporting one pixel for an absent line.
- `neomacs-layout-engine/src/neovm_bridge.rs` consumes the shared presence type;
  its three duplicate predicates are removed. Face-specific rendered chrome
  height measurement remains in the layout engine.

Completed redisplay regions retain their existing authority for pixel queries.
This change does not redesign retained-region freshness after arbitrary later
mutations or claim exact face-specific measurement before first layout.

## Tests and verification

- `gui-tests/fixtures/child-frame-sizing.el` and `tests/child_frame_sizing.rs`
  exercise the real fitting API before the first child redisplay. On the old
  release the assertion fails with two rows instead of one; on the fixed debug
  build it passes with one row and exactly six border pixels.
- `window_cmds/tests/body_geometry_test.rs` checks absent chrome, three active
  lines, `none` suppression, window overrides, and agreement with the individual
  chrome-height queries. The absent-chrome query test was also observed red
  before its implementation change.
- The real `/tmp/a.el` on the fixed debug build gives 18 pixels of text, 24
  outer pixels and position `(584, 607)`. Its frame snapshot is under
  `target/diagnostics/posframe-height/fixed/`.
- 314 selected core frame/window tests and 56 selected layout chrome tests
  pass. The cached-height fixture explicitly requests its header and tab
  lines; its original measured-height assertions are unchanged.

Tests are repository files; `/tmp/neomacs-height-*.log` contains run logs only.
All Rust test runs use `cargo nextest`, not `cargo test`.

Final release verification: `cargo build -p neomacs --release` succeeded in
7m 00s; a matching pdump was generated without byte compilation or autoload
generation. `ldefs-boot.el` is unchanged. Both release GUI tests pass (nine
position cases plus the one-line sizing/content case). The original `/tmp/a.el`
also passes its snapshot assertion: `TestABC`, one row, 24 outer pixels,
position `(584, 607)`. Artifacts are `target/diagnostics/posframe-height/release-*`.
The headless startup logs a Vulkan surface-capability `ERROR_SURFACE_LOST_KHR`
before continuing normally; verification is of redisplay geometry/glyphs,
not a compositor screenshot. Windows was not tested.
