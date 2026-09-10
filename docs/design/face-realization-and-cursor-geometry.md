# Face realization and cursor geometry

Follow-ups to PRs #371, #372 and #373, integrated on main in September 2026.

## Source attributes are not paint

GNU `xfaces.c` merges lface attributes before `load_face_colors` or
`realize_tty_face`. Inverse-video, terminal-default color materialization and
distant-foreground substitution can discard information. They cannot be
undone to recover the source attributes for the next merge.

`neomacs-layout-engine/src/neovm_bridge/face_colors.rs` owns the color stages:

- `FaceColorAttributes` retains source colors, their terminal-default channel,
  inverse intent and inherited distant foreground. Only this type merges.
- `RealizedFaceColors` is constructed by realization and can install paint.
  It has no merge operation.
- Private `FaceColorState` distinguishes manually initialized seed faces from
  resolver-produced faces carrying retained attributes. Face-cache equivalence
  includes those attributes, even when the current paint happens to match.

This is a color-specific extraction, not a claim that every face attribute has
already moved out of the bridge. Font and decoration realization remain there.

## One row geometry, distinct indices

GNU `xdisp.c` computes physical cursor x from the row origin plus preceding
glyph widths. Full layout, cursor reconstruction and materialization must agree
on that origin, including indentation and right-aligned RTL rows.

`neomacs-display-protocol/src/glyph_matrix/text_geometry.rs` owns the shared
horizontal geometry. The view borrows the row so it cannot change while the
mapping is consumed. `VisualTextGlyphIndex` indexes the visual glyph array,
including padding; `TextSlotColumn` indexes materialized text columns. They are
distinct types because wide glyphs and padding make their mappings different.

Cursor publication resolves completed rows as well as the earlier finalizer:
the full-layout cursor can be published after the row has already finalized.
Renderer artifacts and the evaluator physical snapshot consume the finalized
placement. The evaluator's logical iterator cursor remains separately owned.
An integer rounding override is retained only while finalization preserves its
original pixel position.

Horizontal clipping is a semantic case (`HorizontallyClipped`), not ordinary
invisible text. `GlyphProvenance::LeftTruncation` identifies a marker replacing
source text; it counts toward physical columns. A marker replacing a structural
line-number prefix retains the prefix's provenance. The replacement must not
be treated as an inserted column or removed from the EOL advance.

## Measurement capabilities

Height scaling accepts `HeightFaceMeasurement`: `ConcreteFont` carries its
required font service, while `LogicalCells` and `DeferredFont` explicitly name
why that service is not used. They no longer share an ambiguous optional input.
This preserves the current approximation on deferred paths; it does not certify
that every stored metric came from a concrete font.

## Synthetic glyphs retain window-default identity

A frame face ID identifies one immutable rendering within a published frame.
The canonical frame default and a buffer-remapped default are not interchangeable:
two windows can display different text scales in the same frame.

GNU `xdisp.c:produce_special_glyphs` calls `lookup_basic_face` for truncation
glyphs. `xfaces.c:lookup_basic_face` resolves buffer remapping before selecting
the realized face ID. Neomacs' horizontal-scroll `$` marker previously paired
the canonical default ID with remapped attributes, causing `FrameFaceConflict`
when a scaled buffer scrolled horizontally.

`display_face_policy::EffectiveWindowDefaultFace` binds the resolved attributes
to their canonical or arena-assigned identity. Its private enum variants can
only be constructed through resolution. The synthetic-marker factory accepts
this value rather than an independently supplied ID and face. The existing
frame publication conflict check remains intact; it detects invalid producers
rather than silently replacing another window's rendering.

`engine_face_identity_test.rs` exercises real buffer remapping and frame output:
a minimal scaled, horizontally scrolled window reproduces the original panic,
and mixed scaled/unscaled windows verify marker/body sizes across consecutive
redisplays with concrete font metrics. These are layout regressions, not a
claim of native GUI input or Treemacs mouse-session coverage.

## Verification and next stages

Tests live out-of-line and cover lossy face-merge sequences, terminal defaults,
RTL full/replay physical snapshots, row indentation, wide/padding index mapping,
hscroll clipping and EOL, and provenance serialization. Truncated rows remain
excluded by the incremental planner; their engine tests exercise full layout,
while geometry is also tested directly at the retained-row interface.

Further migrations should remain separate, testable changes:

1. Tie measured metrics to existing resolved font identity, size, device scale
   and catalog generation; distinguish stored estimates from measured values.
2. Make default-face advance a distinct input for fill-column placement.
3. Extend finalized-row witnesses and validated coordinate types through the
   remaining publication/pointer paths, without inventing parallel vocabularies.

Closed internal state uses Rust enums and exhaustive matching. Strum belongs at
finite policy parsing/display interfaces; it is not needed for arbitrary face
names or these internal geometry mappings. Platform selection remains outside
these portable modules.
