# Issue #376: selection and empty-line cursor change line spacing

## Reproduction

The reporter's Org configuration is sufficient:

```elisp
(setq default-text-properties '(line-spacing 0.14))
(custom-set-faces '(org-block ((t (:background "gray93")))))
```

Selecting several lines changes their vertical positions and leaves stripes in
the region background. Moving point onto a blank source-block line adds a gap
below that line. The geometry failure also reproduces without Org, with just
`"alpha\n\nbeta\n"`. Explicit text properties reproduce it too.

Native Neomacs reproduction used Linux/headless Weston Wayland. The GNU
reference used its GUI under Xvfb, the same 20px DejaVu Sans Mono font and the
same Org fixture. With its 24px font metrics, GNU advances 27px in every state.
Before the fix, Neomacs advances 24px normally, but 27.36px at selected
newlines or the empty newline under point. The following source-code line's
y coordinate is 144px normally, 160.8px with the multi-line selection and
147.36px with point on the empty line. Published row heights remain 24px.

Artifacts and executable diagnostic probes are in `target/diagnostics/issue-376`.
The original red loop was `bash target/diagnostics/issue-376/check.sh`.

## GNU source studied first

Reference checkout: `emacs-mirror/emacs`, revision `a360712c9d2` (read-only).

- `src/xdisp.c`, `calc_line_height_property`: resolve the newline's spacing
  property, including relative values, into integer pixels by truncation.
- `src/xdisp.c`, `produce_glyphs`: spacing contributes logical descent;
  physical glyph metrics and baseline ascent are distinct.
- `src/xdisp.c`, `compute_line_metrics` and glyph-string initialization:
  publish logical row height and use it for glyph-string height.
- `src/xterm.c`, `x_draw_glyph_string_background`: fill the glyph string's
  logical height, including space beyond the font's ink.

## Root cause

The routed text path consumes a text run but leaves its newline. On the next
iteration that remaining newline is classified as an empty row. The optimized
empty-row executor constructs a propertyless row break and then discards even
that item, calling the line-break request with inherited spacing instead.
Point/region handling can force canonical production, which does preserve the
newline property. This makes geometry depend on an optimization decision.

There are two additional shared geometry defects: fractional spacing is not
truncated, and spacing is added only to the next row's y coordinate, not to the
finished row height. The latter leaves the background/hit-test/retained-row
model unable to account for the space between rows.

The first minimal Rust fixture was tightened after an unexpected pass: equality
alone could pass when both states omitted spacing. The durable test also asserts
that configured spacing increases row height, and compares fresh with retained
output. Public Lisp editing operations are used so invalidation is exercised.

## Ownership and module design

1. `buffer_source/producer` owns newline acquisition and its properties.
   `buffer_source/row_route` optimizes text only; a remaining newline returns to
   the canonical producer. The duplicate empty-row renderer is removed.
   Row-break constructors on the optimized item source are now test-only.
2. `display_row/spacing.rs` owns `ResolvedLineSpacing`, a private-field integer
   newtype. Non-finite and negative inputs cannot reach row advancement as
   invalid extents; fractional values truncate once into logical pixels.
3. `display_row/geometry.rs` owns completion: spacing contributes to the
   finished logical row height without moving glyph ascent. The existing
   closed `DisplayRowAdvanceKind` keeps newline, visual wrap and truncation
   distinct, and `DisplayRowMeasurementMode` excludes pixel spacing from TTY
   logical-cell rows.
4. Output, background materialization, hit testing and retained-row replay keep
   consuming the same completed row metrics. No selection-specific padding,
   renderer-only patches or duplicate newline-property lookup are added.

This addresses the reproduced newline-spacing defect; it is not a claim of
complete GNU parity for every `line-height`, frame spacing or mixed-font case.

## Tests

- `src/engine_line_spacing_test.rs`: real frame-output tests for explicit
  spacing, point on an empty line, retained/fresh agreement, default relative
  spacing, whole pixels and unchanged ascent. Observed red before each fix.
- `neomacs-gui-tests/fixtures/org-line-spacing.el` and
  `tests/org_line_spacing.rs`: real Org region/empty-line transitions, all
  eleven row positions/heights, and PNG sampling for continuous region/block
  backgrounds. Observed red against the original release (24px vs 27px).
- Existing row-geometry expectations now include spacing in logical height.
  The obsolete internal counter test requiring the removed empty-row path is
  replaced by the observable frame-output regression.

Rust verification uses `cargo nextest`. The full layout suite passed 2,310
tests (three skipped), both normally and with `neo-term` enabled. The
display-protocol suite passed all 757 tests.

## Release verification

`cargo build -p neomacs --release` succeeded in 7m 08s. A matching pdump was
generated without byte compilation or autoload regeneration. The executable
fingerprint is `65CF443CF25F2DB35C7886093707C9C72EDBB3276BA9957575538A9FCB1EEF9D`.
`lisp/ldefs-boot.el` retains SHA-256
`094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`.

The repository GUI regression passes against that release, including PNG
background coverage and visible region activation/deactivation. The original
reproduction also passes: the tracked source-code line stays at y=162 in
normal, selected and empty-cursor states, exactly matching the GNU reference.
The release screenshots were inspected: no selection stripes or empty-line
background gap remain. Artifacts are in `target/diagnostics/issue-376/fixed`
and `target/neomacs-gui-tests`.

Native verification is Linux Wayland only; no other platform is claimed tested.
