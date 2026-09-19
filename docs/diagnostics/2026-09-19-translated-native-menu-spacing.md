# Issue 381: translated native menu spacing changes on redraw

## Reproduction

The reporter's translation Lisp was run under an isolated Weston 16 Wayland
session nested in Xvfb. On the pre-fix release, Chinese Edit-menu labels overlap
on first paint. Holding Control changes character spacing and moves shortcuts
left. Twenty additional Control presses did not crash Neomacs or Weston.
The reporter's final log instead records a Wayland protocol error on wl_surface
when native menu presentation fails. That crash is not locally reproduced and
is not claimed fixed by this text-layout change.

The GUI regression is
`translated_menu_geometry_survives_control_key_redraw` in
`crates/neomacs-gui-tests/tests/native_menus.rs`. Its reduced Lisp fixture uses
Chinese and Latin labels with fixed shortcuts. It opens a real native popup,
captures settled compositor pixels, holds/releases Control, checks process
liveness, and compares the menu text region. The unfixed build changes 2,133
pixels on Control. Screenshots and logs are written under
`./tmp/neomacs-gui-tests/native-menu-*`.

## GNU reference and root cause

GNU `src/gtkutil.c:make_widget_for_menu_item` creates separate UTF-8 GTK labels
for menu text and key bindings, then packs them into a nonhomogeneous box. Text
is measured by the widget/font machinery, not by assigning one scalar one cell.

Neomacs's `menus/layout.rs` counted Unicode scalars to size panels, and
`renderer/paint/menu.rs` positioned each scalar at `index * char_width`.
CJK glyphs therefore overpainted their neighbors. The painter separately read
`WgpuGlyphAtlas::default_char_width` on each paint. This begins as
`font_size * 0.6`; the first rasterized face-0 glyph changes the cached value to
that glyph's advance. A Chinese first glyph changes the nominal width used for
all subsequent redraws, including Latin labels and shortcut alignment. Control
triggers a redraw; it does not change the menu's labels or commands.

## Design

`MeasuredMenu` privately owns the item list, optional title, and immutable
`MenuTextLayout` containing logical-pixel character positions and widths for
labels, titles, and shortcuts. It consumes the contents during measurement and
exposes only shared references, preventing later content/geometry mismatches. The menu's font/raster source supplies actual
advances before panel sizing. Both panel measurement and painting consume this
same measured menu, and `MenuPanelPaint` requires it. `MenuPanelRole` explicitly
selects root-title rendering or submenu rendering. Symbolic menu item state remains in
the existing `MenuItemKind` enum; raster-cache state cannot define geometry.

The snapshot stores its space advance explicitly for gaps and arrow gutters.
Missing/blank bitmap fallback uses this captured owner metric, never a mutable
first-glyph cache. Text-run fields are private and widths are derived during
measurement. The native submenu positioning and hit-test bounds use the measured
panel extent. Menu tooltip spacing also uses the captured space metric.

The native menu request retains the atlas populated during measurement. All
panels reuse that atlas for the session lifetime, including reopened submenus,
avoiding repeated font discovery and duplicate rasterization. Before drawing,
the atlas adopts the target surface scale; a scale change clears raster caches
without changing the immutable logical geometry.

This keeps the existing scalar rasterization boundary; it does not introduce a
new general shaping engine or claim new support for contextual scripts. A future
shaped-run implementation can replace text measurement without reintroducing
geometry decisions in the painter.

## Verification commands

```sh
cargo nextest run -p neomacs-display-runtime -E 'test(menus::session)'
cargo nextest run -p neomacs-renderer-wgpu --test offscreen_frame -E 'test(popup)'
cargo xtask fresh-build --release
cargo nextest run -p neomacs-gui-tests --test native_menus
```

Results after the fix:

- All 56 menu-session tests pass, including independently specified CJK advances,
  panel width, title offsets, and shortcut placement.
- All four offscreen popup pixel tests pass. The initial CJK test independently
  shapes a repeated real-font glyph, checks measured advances and repeated ink
  masks, and verifies visible shortcut ink with a clear column gap. Deliberately
  restoring either one-cell measurement or one-cell painting makes it fail.
- The existing native submenu GUI test passes before and after the change.
- The new GUI regression fails twice against the old executable and passes
  against the fresh-build release, with identical menu-region pixels while
  Control is held and after it is released.
- `cargo xtask fresh-build --release` completes the full runtime pipeline.
- Replaying the reporter's complete translation Lisp produces readable CJK
  labels on first paint and stable geometry after Control. Neomacs and Weston
  remain alive through 20 additional Control presses.
- Formatting and diff checks pass. All reproduction artifacts are under `./tmp/`.

Review follow-up verification: 73 focused menu/session/popup tests pass, as do
both `neomacs-gui-tests` native menu regressions after a complete release fresh
build. The reporter's full translation also retains identical menu-region pixels
while Control is held and after 20 presses. The initial-spacing regression was
checked against two deliberate mutations: one-cell measurement and one-cell
painting both fail; restoring measured geometry passes. Logs are under
`./tmp/neomacs-381/review-*` and `initial-*-red.log`.

## Follow-up: top-level menu-bar headings

The original verification missed overlap in the top-level menu bar. Dropdowns
and headings used separate render paths. `position_menu_items` still allocated
`scalar_count * frame_char_width`, while both the ordinary and compact bar
painters placed scalars at `index * atlas.default_char_width`. This under-sized
Chinese heading hit rectangles as well as overlapping the glyphs. The prior
claim of a complete #381 fix was too broad.

GNU's GTK path (`gtkutil.c:make_menu_item`) uses
`gtk_menu_item_new_with_label(utf8_label)` when there is no shortcut. GTK owns
font-based text measurement. Its non-toolkit terminal field-width convention is
not a model for GUI glyph advances.

`PositionedMenuHeading` now consumes a semantic item and measures its label
before deriving its bounds and action. Its private fields retain the same
geometry for painting, hover/click regions, and popup anchors. `MenuHeadingText`
distinguishes resolved pixel glyphs from terminal cell fields. Pixel labels carry
the exact layout-resolved font identities and glyph positions; the atlas replays
those glyphs and caches raster data without reselecting fonts or measuring text.
Both menu-bar variants share this representation. Existing terminal field and
clipping behavior is retained.

Regression coverage includes repeated real-font CJK ink masks for ordinary and
compact bars, measured heading bounds and popup anchors, and a real GUI click
on the right side of a four-CJK-character heading. The initial pixel test fails
on the original painter. The GUI click test fails on the old executable because
it invokes the next heading's command (`second` instead of `first`). Artifacts
and logs are under `./tmp/neomacs-381/heading-*`.

Verification of the heading fix: all 110 focused layout/runtime/renderer tests
pass, including terminal compatibility and resolved-font cache invalidation.
`cargo xtask fresh-build --release` completes the full pipeline. All three native
GUI tests pass against that build. The full translated configuration now shows
separated top-level Chinese headings; both the menu bar and dropdown retain
identical pixels while Control is held and after 20 presses. The before/after
screenshots are `heading-unfixed.png` and `heading-fixed.png` under the same
artifact directory. Formatting and diff checks pass.
