# SVG tag dimensions and face-remapping precedence (#360)

## Reproduction and scope

[The issue comment](https://github.com/eval-exec/neomacs/issues/360#issuecomment-5631329783)
contains different SVG source, not merely different rendered pixels:
GNU's box is 73.26 × 25.56, while Neomacs's is 61.05 × 21.30.
The SVG text font size and baseline are identical between the reported files.

The local reproduction uses the installed svg-lib 0.3 and svg-tag-mode
20241021.1341, a DejaVu Sans Mono font opened at 25 pixels, and
`(face-remap-add-relative 'default :height 1.2)`. IBM Plex Mono is not installed
on this machine. GNU Emacs runs under Xvfb at 96 DPI; Neomacs runs under a
headless Weston Wayland session.

| Measurement | Unremapped, both editors | Remapped GNU | Remapped old Neomacs |
| --- | --- | --- | --- |
| Window font width | 15 | 18 | 15 |
| Window font height | 30 | 36 | 30 |
| Generated SVG width | 61.05 | 73.26 | 61.05 |
| Generated SVG height | 21.30 | 25.56 | 21.30 |

The package sizes the box with `(4 + 0.07) * (window-font-width)` and
`0.71 * (window-font-height)` (plus `line-spacing` when set). The explicit
SVG text font is opened separately through `font-info`. This explains how
the box can be wrong while the text's declared font size stays unchanged.
Rounding of actual font metrics means a 1.2 face multiplier does not always
produce an exact 1.2 ratio: the separate 20px-font probe yields 12×24 → 14×29.

This reproduces the exact **outer dimensions** with a controlled font and
remapping. The reporter's complete face/remapping/DPI configuration and package
versions are not known, so it does not establish that their installation took
this precise path, nor reproduce their IBM Plex Mono glyph rendering.

## GNU source and root cause

Reference checkout: `/home/exec/Projects/github.com/emacs-mirror/emacs`.
It was read only; no modifications or upstream submissions were made.

- `lisp/window.el`, `window-font-width` / `window-font-height`: query
  `font-info` for `face-font`, honoring remapped faces. Canonical frame cells
  are fallback measurements, not a substitute for the remapped window font.
- `src/xfaces.c`, `Fface_font`: returns the realized named face's font.
- `get_lface_attributes`: applies `face-remapping-alist`, with self-reference
  suppression so the original definition can provide the base.
- `merge_face_ref`: merges the tail of a face-reference list first. Earlier
  entries have higher priority; relative heights compose over the base.

Neomacs's evaluator-side FaceTable merged remapping entries forwards. In
`((default (:height 1.2) default))`, the trailing original `default` replaced
the already-enlarged height with its absolute unscaled height. Existing host
traces show an Absolute(150) request, rather than Absolute(180), before native
font lookup. Thus neither the native font cache nor the SVG rasterizer is the
cause of this reproduced failure.

## Design

`crates/neovm-core/src/face/remapping.rs` owns the parsed remapping and its
evaluation. The existing FaceTable public interfaces stay unchanged.
Two previously duplicated traversal loops now share one reverse-order fold.
The closed `RemappingBase` enum distinguishes named queries' default/inherited
base from text layers' specified-attribute-only contributions. Exhaustive
matches preserve that distinction without boolean flags or string modes.

The fix is in face composition, not in svg-tag-mode, font pixel scaling,
or a hard-coded 1.2 multiplier. Relative/absolute `FaceHeight` semantics remain
typed. This change does not replace layout's richer window-filtered face
resolver or expand support for other remapping-reference forms.

## Tests and verification

The public Lisp regression failed before the change: `face-font` reported
20px instead of 24px after remapping. The real GUI SVG regression also failed
before the change: all window-font and box dimensions remained unscaled.

Core tests live in `display/xfaces/tests/font_remapping_test.rs` and exercise
the real Lisp evaluator with a deterministic native-font-host adapter. They
cover applying/removing relative remapping, relative/absolute precedence,
and the corresponding `font-at` named-face path. GNU independently returned
36, 20, and 40 pixels for the three precedence cases.

The GUI test lives in `neomacs-gui-tests/tests/svg_tag_font_remapping.rs` with
its Lisp fixture under `fixtures/`. It uses the actual installed SVG packages,
checks GNU's known box dimensions, verifies the explicit SVG text font and
canonical frame cells stay unchanged, and checks removal restores dimensions.
It is explicitly ignored in normal runs because it needs a built binary/pdump,
Weston, DejaVu Sans Mono, and both packages. No package is silently downloaded.

```sh
cargo nextest run -p neovm-core -E 'test(xfaces::) | test(face::) | test(font::tests::font_at) | test(window_cmds::tests::body)'
cargo nextest run -p neomacs-layout-engine -E 'test(remap)'
NEOMACS_GUI_SVG_LIB_DIR=/path/to/svg-lib \
NEOMACS_GUI_SVG_TAG_MODE_DIR=/path/to/svg-tag-mode \
cargo nextest run -p neomacs-gui-tests --test svg_tag_font_remapping --run-ignored all
```

Validation: 205 selected core tests, an additional font/window group of 384
tests (with some overlap), and 23 layout remapping tests pass. These are
targeted runs, not a new full neovm-core suite run.

`cargo build -p neomacs --release` succeeded in 7m57s. The executable was
sealed with xtask's normalized SHA-256 algorithm, then the matching pdump was
generated using its `neomacs-temacs` role copy with
`--batch -l loadup --temacs=pdump`. No byte compilation or autoload generation
was run; `lisp/ldefs-boot.el` retained its original checksum.

The rebuilt release passes the exact-dimension Wayland GUI regression and
the original standalone probe. Both report 15×30 → 18×36 window font cells
and 61.05×21.30 → 73.26×25.56 SVG boxes. The test also passes in GNU Emacs.
There was no verification on the reporter's Xorg setup, macOS, or Windows.

Logs: `/tmp/neomacs-360-core-red.log`, `/tmp/neomacs-360-core-final.log`,
`/tmp/neomacs-360-font-window.log`, `/tmp/neomacs-360-layout.log`,
`/tmp/neomacs-360-gui-red-exact.log`, `/tmp/neomacs-360-gui-green.log`,
`/tmp/neomacs-360-release-build.log`, `/tmp/neomacs-360-release-pdump.log`.
Local diagnostic captures are under `target/diagnostics/issue-360/`.
