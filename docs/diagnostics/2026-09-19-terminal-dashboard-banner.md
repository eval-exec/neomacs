# Issue 383: terminal dashboard banner

The issue screenshots show `[img]` in Neomacs where GNU Emacs shows a
purple Unicode Braille Emacs logo. This is the separate `emacs-dashboard`
package's `banners/logo-braille.txt`, not GNU's built-in startup screen.

## GNU and package behavior

- `emacs-dashboard/dashboard-widgets.el`, `dashboard-choose-banner`: the
  `logo` option supplies both the PNG and the Braille text when PNG decoding
  is available.
- `dashboard-insert-banner` inserts the text, then covers it with the image
  `display` property. Conditional `line-prefix` and `wrap-prefix` specs center
  either representation. The text is deliberately retained for terminal frames.
- GNU `src/xdisp.c`, `handle_single_display_spec` (local reference near line
  6512): the `valid_p` expression requires `FRAME_WINDOW_P` for image and
  xwidget replacements, including after stripping a margin wrapper. A terminal
  therefore continues through the original text, including its newlines.
- GNU `lisp/image.el`, `image-type-available-p`, tests decoder availability.
  It does not mean the current frame can display an image. GNU
  `lisp/frame.el`, `display-images-p`, separately checks the display type.

Sources: <https://github.com/eval-exec/neomacs/issues/383>,
<https://github.com/emacs-dashboard/dashboard/blob/master/dashboard-widgets.el>,
<https://github.com/emacs-dashboard/dashboard/blob/master/banners/logo-braille.txt>.

## Reproduction and cause

The `neomacs-tui-tests` regression
`terminal_image_property_preserves_multiline_text_banner` inserts a small
multiline Braille banner, applies an image display property, and compares
GNU and Neomacs through their PTY character grids.

Before the fix, GNU displayed the text and Neomacs displayed
`[img]Welcome to Emacs!`. The test failed in 2.46 seconds, demonstrating that
the covered newlines disappeared as well as the banner.

The actual package was also replayed on PTYs using the repository's cached
dashboard revision `176d641a55543bda1f0c7506fb954702350c1857`, with
`dashboard-startup-banner` set to `logo` and `dashboard-insert-banner` called
in an empty buffer. GNU emitted 420 Braille characters; the unfixed Neomacs
emitted none and an `[img]` placeholder. The rebuilt Neomacs emitted the same
420 Braille characters, without the placeholder. This reproduces on Linux;
the reporter's complete macOS configuration was not available.

The layout classifier had no frame target. It classified the image as a
replacement; the source walker consumed its full span; media resolution then
produced the `[img]` placeholder. Fixing only the placeholder would still lose
the source text and would mishandle subsequent alternatives in a spec list.

## Design

`DisplayPropertyTarget::{Graphical, Terminal}` is an explicit input to
classification. Terminal graphical-media specs are rejected before replacement
ownership is assigned. Margin contents use the same classifier. Both buffer and
Lisp-string sources carry the target, and conditional-spec evaluation uses the
same target when deciding whether a replacement ends string traversal.

Window snapshots require a target at construction, so preflight invisibility
and row-route decisions agree with the source walker. Decoder or display-host
availability is not used as a proxy for frame identity. Graphical image-load
fallback behavior remains separately represented.

Run the terminal regression against a freshly built executable:

```sh
cargo xtask fresh-build --release
NEOMACS_TUI_NEOMACS_BIN="$PWD/target/release/neomacs" \
  cargo nextest run -p neomacs-tui-tests \
  -E 'test(terminal_image_property_preserves_multiline_text_banner)'
```

## Verification

- `cargo xtask fresh-build --release` completed successfully, including Lisp
  byte-compilation and dump generation. Final verification uses its
  `target/release/neomacs`, not the earlier standalone debug build.
- Both new TUI regressions fail against the unfixed release executable and
  pass against the fresh-build release executable. They compare the complete display
  with GNU, including the styled multiline banner and image alternatives on
  buffer text, margin specs, and conditional overlay strings.
- Seven related TUI checks pass against the fresh-build release runtime in
  2.96 seconds, including dashboard-style centering and existing
  display-variable/replacement-order checks. The actual dashboard reproduction
  was repeated: GNU and this release both emit 420 Braille characters and no
  `[img]` placeholder.
- Full layout suite: 2,382 passed, two failed, three skipped. The two failures
  are `layout_frame_rust_aligns_dired_filenames_after_nerd_icon_tabs` and
  `layout_frame_rust_nerd_font_alias_icon_uses_resolved_monospace_cell_width`.
  Both resolve Cascadia where they expect JetBrains. Both were rerun against
  unchanged `HEAD` (`06338ef5e9`) and failed there too.
- Existing media tests now explicitly create graphical frames. Pixel-geometry
  fixtures avoid host font measurement, and clipping expectations use the
  graphical text area rather than the old terminal border/column assumptions.
- `cargo check -p neomacs-layout-engine`, formatting, and `git diff --check`
  pass. The stale, ignored `lisp/neomacs-video.elc` was regenerated from source
  to allow bootstrap tests to run.
