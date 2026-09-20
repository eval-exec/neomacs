# Org Superstar circle font selection

## Reproduction

The release editor with the user's configuration, followed by `C-c o j`, opens
an Org journal whose default face is remapped by `buffer-face-mode`:

```elisp
((default (:family "Noto Sans CJK SC") default))
```

The frame default family is JetBrains Mono Nerd Font, and
`use-default-font-for-symbols` is nil. Org Superstar composes the second heading
star into U+25CB WHITE CIRCLE using `(composition (1 1 [9675]))`.

Reproduced on private Xvfb `:97`, with separate temporary copies of the journal
for Neomacs and GNU Emacs. The minimized fixture is `** Circle` with the same
composition, bold heading face, and default-face remapping. Coloring only the
bullet magenta makes its ink bounds measurable independently from the text.

| Before the fix | Actual font at 14px | Circle ink bounds |
|---|---|---|
| Neomacs | JetBrains Mono Nerd Font Bold | 9 × 9px |
| GNU Emacs | Noto Sans CJK SC Bold | 12 × 14px |

Changing GNU's U+25CB fontset rule to explicitly request JetBrains makes its
circle 9 × 9px too. Removing `buffer-face-mode` in both applications also makes
both circles 9 × 9px. These controls isolate font selection from font size and
composition geometry.

## GNU source and oracle

Studied the local source at
`/home/exec/Projects/github.com/emacs-mirror/emacs`:

- `src/xfaces.c:realize_gui_face` (around 6305): retains the base fontset rules
  while realizing the face's merged attributes.
- `src/fontset.c:fontset_find_font` (around 660): calls `font_find_for_lface`
  with `face->lface` and the selected fontset rule's spec.
- `src/font.c:font_find_for_lface` (around 3306): if the spec family is nil,
  takes the family from `attrs[LFACE_FAMILY_INDEX]`.

The GNU GUI oracle, with a Noto buffer default and a family-less Unicode rule,
confirms:

| Inline face family | Fontset rule family | ASCII font | Circle font |
|---|---|---|---|
| inherited | unspecified | Noto | Noto |
| JetBrains | unspecified | JetBrains | JetBrains |
| inherited | JetBrains | Noto | JetBrains |
| JetBrains | JetBrains | JetBrains | JetBrains |

A base fontset's identity/rules and an effective face's family are distinct
concepts. The bug was representing the former as `fontset_base_family`, a
second family string copied from the frame default. The resolver received
that stale family for non-ASCII glyphs even after face remapping.

## Test first and design

The frame-layout regression first failed with:

```text
left: "JetBrainsMono Nerd Font"
right: "Noto Sans CJK SC"
```

The test reaches composition, remapping, character lookup, and the exact font
binding published for rendering. Passing the effective family corrected it.
The subsequent refactor removes the duplicate family from face state, layout,
protocol, rendering, cache identity, and display-host font queries.

`FontFamilySource::{Fontset, Face}` expresses the actual GNU distinction with
exhaustive Rust matching. One effective family enters selection; a fontset
spec can explicitly override it. No Org-specific scaling is introduced.
See the effective-font-family section of
[the face design](../design/face-realization-and-cursor-geometry.md).

Regression command:

```sh
cargo nextest run -p neomacs-layout-engine --lib -E 'test(font_selection::)'
```

The three cases cover composed and plain circles, inline family selection,
changing buffer remappings on a reused layout engine, and adding/removing an
explicit fontset override. They require JetBrains Mono Nerd Font and Noto Sans
CJK SC, matching the GUI reproduction.

## Verification

- Three new frame-layout regressions pass.
- Lisp font and face-remapping suite: 74 passed.
- Display protocol and glyph-atlas suite: 816 passed.
- Formatting, diff whitespace, and native font dependency boundary checks pass.
- Full layout suite: 2,386 passed, one failed, three skipped.
- The failure,
  `display_row_glyph_measurer_builds_measured_complex_text_run_plan`, is also
  present on unchanged commit `83c58958c4` in a separate worktree. Both runs
  produce Arabic shaping cluster offsets `[(0, 0), (1, 2), (3, 6)]` where the
  test expects a separate `(2, 4)` cluster. It is outside this fix.

- `cargo xtask fresh-build --release` completed successfully, including the
  Lisp bytecode refresh.
- The rebuilt `target/release/neomacs`, with the user configuration on private
  Xvfb `:97`, opened the copied journal via `C-c o j`. Its published U+25CB font
  is Noto Sans CJK SC Bold, 14px. `font-at` agrees with that family and size.
- Repeating the original pixel assertion on the minimized Org Superstar
  fixture now passes: **Neomacs 12 × 14px; GNU Emacs 12 × 14px**.
- Private GUI processes and Xvfb were closed without saving their buffers.

Local diagnostic logs, oracle forms, snapshots, and screenshots are retained
under `/tmp/neomacs-circle-fix/`. Journal snapshots contain private content and
are not committed. The disposable baseline worktree was removed.
