# Issue #359: glyph coverage and perceived font weight

Report: https://github.com/eval-exec/neomacs/issues/359

## Reference and reproduction

Read GNU Emacs `src/ftcrfont.c` first: Fontconfig prepares the font pattern,
Cairo creates the scaled font, and `ftcrfont_draw` supplies the foreground
and calls `cairo_show_glyphs`. The controlled GNU capture uses identical
30px glyphs with black/white foreground and background exchanged. Its 2,631
antialiased channel samples are exactly complementary (maximum error 0/255).

Before the fix, the new `neomacs-gui-tests` regression failed with maximum
error 98/255. This reproduced with DejaVu Sans Mono and the reporter's IBM
Plex Sans, and with Fontconfig explicitly selecting grayscale or RGB LCD
masks. Native keyboard input triggers redisplay and capture; no assertion
relies only on the layout snapshot.

This isolates a coverage/compositing defect from font selection, point size,
hinting and the separate SVG-tag sizing issue #360.

## Cause and design

Face colors reach the renderer in linear RGB and the render attachment is
sRGB. The old LCD shader interpolated linear RGB directly, producing overly
light dark-on-light antialias edges. The grayscale shader used a separate
approximate-gamma calculation and hardware alpha blend. These paths did not
implement GNU Cairo's encoded-RGB coverage composition.

The shared `glyph_coverage.wgsl` shader now converts the foreground/background
to encoded RGB, applies coverage once, then converts back to linear RGB for
the sRGB attachment. Grayscale and LCD fragment entry points differ only in
mask sampling. Foreground opacity remains separate until composition.

`CoverageGlyphVertex` requires both foreground and background. A
`GlyphCoverageMask` enum chooses the shader entry point for ordinary and
stencil pipelines. The shared geometry helpers and typed coverage vertex
arena serve main-frame and child-frame text; color glyphs retain their image
material. Clipping and row-reuse use the same typed vertices.

This is a buffer-text coverage correction, not a claim of pixel-identical
FreeType/Swash rasterization. Font hinting and rasterizer differences remain.
The separate foreground-only overlay text shader is outside this change.

## Regression evidence

Artifacts are under `tmp/neomacs-359/`: `gui-red.log`, `ibm-red.log`,
`mask-modes-red.log`, and the GNU `gnu.png`/`gnu.json` probe. The committed
GUI test checks grayscale and LCD rendering on a 3840x2160 headless output,
with monochrome and colored foreground/background pairs. It checks actual
antialiased pixels for complementary coverage, allowing two encoded levels
of rounding. A shader validation test covers both fragment entry points.

The first 4K attempt timed out before the capture command: tightly packed
virtual-keyboard events were not reliably delivered during startup. The
fixture now primes the keyboard and paces events. Both modes then passed
with DejaVu Sans Mono and IBM Plex Sans. The test also checks that LCD masks
produce colored fringes and grayscale masks remain achromatic. The display
is 3840x2160; the captured application surface is 3836x2133 (Sway decorations
occupy the difference).

Measured after the shader correction: both fonts and both mask modes have
maximum error 0/255 for monochrome pairs and 1/255 for colored pairs, down
from the original 98/255 monochrome error. This fixes compositing rather than
artificially increasing font weight.

Run the regression after `cargo xtask fresh-build --release`:

```sh
TMPDIR="$PWD/tmp" cargo nextest run -p neomacs-gui-tests --test glyph_coverage --test-threads 1
```

Sway, wtype and DejaVu Sans Mono are prerequisites. `NEOMACS_GUI_WTYPE` can
select a wtype executable; `NEOMACS_COVERAGE_FONT` and `FONTCONFIG_FILE` allow
the IBM Plex Sans comparison without changing the desktop configuration.

## Final verification

- Final `cargo xtask fresh-build --release`: passed, including Lisp runtime.
- Renderer unit tests: 683 passed (`renderer-final.log`).
- GUI integration set: 14 passed (`gui-final.log`), covering both new 4K
  coverage tests, desktop-font startup/HiDPI/4K/8K geometry, SVG remapping,
  Org silent selection and child-frame sizing.
- IBM Plex Sans on the final executable: both coverage tests passed again
  (`ibm-final.log`).
- Rust formatting and `git diff --check`: passed.

The separate pre-existing `org_line_spacing` test failure is documented in
`2026-09-19-org-silent-selection.md`; the passing GUI set above does not
include that failing test.
