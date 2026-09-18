# Display motion and viewport ownership

Interactive motion and redisplay must interpret the same display rows. A
buffer newline is not necessarily a screen-line boundary: invisibility can
remove it, replacement strings can add several rows at one source position,
and an image can occupy more pixels than the entire window body.

GNU's reference implementations are `Fvertical_motion` in `src/indent.c`,
`move_it_by_lines` in `src/xdisp.c`, and `window_scroll_pixel_based` in
`src/window.c`. GNU deliberately uses a different `compute_motion` engine
in batch mode; this design does not erase that distinction.

## Ownership

```text
Lisp motion / scrolling command                        VM thread
  │
  ├─ xdisp/motion: source motion and pixel-page decisions
  │    └─ WindowLayoutQueryScope::Rows { start, count }
  │         └─ existing layout-query adapter
  │              └─ canonical layout engine → owned row geometry
  │
  └─ WindowScrollUpdate: commit point + start + pixel offset
       └─ ordinary redisplay → immutable presentation → renderer
```

- `neovm-core/emacs_core/display/xdisp/motion` owns motion semantics.
  `measurement.rs` owns safe source backtracking and expanding row coverage;
  `paging.rs` plans graphical scrolling; `policy.rs` captures Lisp scrolling
  preferences. The existing `indent` and `window_cmds` subsystems retain their
  Lisp builtin registrations.
- `neovm-core/window` owns the typed query boundary, freshness witnesses,
  marker-backed viewport update, and geometry returned to core consumers.
- `neomacs-layout-engine` remains the sole interactive row producer. It
  interprets fonts, overlays, replacements, wrapping and image metrics for
  both measurement and presentation. It does not decide scroll policy.
- Runtime wiring installs the existing synchronous query adapter. `Context`
  and any Lisp fontification callbacks stay on the VM thread. Neither the
  renderer nor a platform event thread may evaluate Lisp.

Native GUI and browser use these same core and layout paths. This does not
require a platform-specific scrolling algorithm, feature flag or extra crate.

## Measurement contract

`WindowLayoutQueryScope` distinguishes a viewport query from a row-bounded
measurement. `Rows` takes a one-based source position and a nonzero row count.
It projects its start and budget into local layout parameters without changing
live window markers, publishing a presentation, or updating `window-end`.
Its accessible-region semantics preserve labeled restrictions.

Retained rows can answer a motion only when their canonical freshness witness
matches. Insufficient retained or newly measured coverage means measure more;
it is not evidence that the accessible buffer boundary was reached. The
resolver distinguishes completed motion, a proven accessible boundary and
insufficient rows. Coverage grows geometrically and checks for Lisp quit.
Errors from an installed row producer are errors, not permission to silently
switch to a less capable text scanner. Batch/startup without that adapter
retains the existing source-based path.

A row's span is a drawing extent, not a line extent. A line truncated on the
right ends its row where the drawing stopped -- the row's `maxpos` is
`it->current.pos` for that case (`find_row_edges`, src/xdisp.c:25269) -- so a
position beyond the right margin falls *between* rows rather than inside one:
measured in a 160-column window over 300 characters plus a newline, the rows are
`1..159` and `302..302`. GNU answers such a position by rewinding to the start
of the origin's line, walking forward to it, and backtracking a line when that
walk overshoots a line truncated on the right (`it.line_wrap == TRUNCATE &&
it.current_x >= it.last_visible_x`, src/indent.c:2393-2400). From rows, the same
answer is the row whose line the origin is still on, bounded by the next row's
start. Without that widening no row contains the origin, the measured rows are
judged to exhaust their coverage at end of buffer, and the resolver refuses the
motion -- which is what left `C-e` unable to reach the end of a truncated line,
and `window-hscroll` unable to follow point to it. The graphical scrolling
planner asks the same question of the same rows and answers it with the same
rule, at the three lookups that take a point: the pixel origin, the point's row
in the measured viewport, and the point-visible test on a candidate viewport. A
point it cannot place reads as one that has left the window, so the plan either
recenters around a point already on a visible row or fails with "Scroll origin is
outside measured source coverage".

Mutable string prefixes are captured by value, not just Lisp object identity.
The shared `LayoutPrefixInputs` projection records effective buffer-local
`line-prefix` and `wrap-prefix` string bytes, multibyteness, identity and string
text-property revision. Redisplay skipping, retained-geometry freshness,
in-flight validation and incremental row reuse all include that projection.
Changing a prefix with `aset` or changing its text properties need not modify
the buffer itself, but can still change every row's source-to-pixel mapping.
Owned bytes are shared on clone; no mutable Lisp string is retained by this
projection. Stretch-space prefixes also capture the first direct operand of
each supported geometry property, using the same `DisplaySpaceKey` enum as
geometry evaluation. In-place plist changes therefore invalidate layout even
when the prefix cons retains its identity. Unknown keys and shadowed duplicate
operands do not affect this projection. Arithmetic pixel expressions, absolute
pixel lengths and scaled lengths are captured as an immutable token stream,
with an iterative, cycle-aware traversal. No live Lisp references are retained.
Prefix strings capture `(space ...)` display-property payloads by interval,
using the same `SpaceInput` as non-string prefixes. The shared
`DisplayPropertySpecs` decoder handles single specs, lists, vectors and outer
`disable-eval` wrappers; capture retains spec order, other-spec identities and
the evaluation flag as well. Its list walk has allocation-free cycle detection.
Each captured spec separates an optional `when` condition identity from its
payload. The shared `display_spec_when_parts` decoder unwraps exactly once,
matching GNU `xdisp.c:handle_single_display_spec`; capture then uses the existing
space/string/image input projections. Literal `t`/`nil` changes and mutable
wrapped payloads therefore participate in freshness, including literal `t`
under `disable-eval`. No Lisp runs during capture. Arbitrary condition results
remain the responsibility of pre-walk evaluation and its existing no-body-reuse
policy, not a claim that condition identity describes every Lisp dependency.
Mutating a width, arithmetic operand or container element therefore no longer
relies on the string property revision changing. This is conservative capture
of syntactic inputs, not evaluation of which spec wins. Unrelated property
payloads are not traversed. Replacement strings share `StringContentInput` with
top-level prefixes: identity, bytes, multibyteness and property revision are
captured by value. Capture does not follow replacement strings' own `display`
properties recursively, matching GNU's suppression of recursive replacement.
Face and font-lock-face properties on both kinds of strings capture ordered face
lists, known attributes (using `LFaceAttr`), scalar/string operands and `:inherit`
references. The iterative capture records shared/cyclic inheritance without
resolving named faces or evaluating filters. Named-face definitions still use the
existing face revision. GUI-measured regressions cover height changes; terminal
cell geometry is not evidence for font-size invalidation. Box and underline
plists share `DecorationProperty` with the face parser; capture includes supported
operands, mutable color bytes and box width-pair components. Presentation tests
compare the published glyph faces against fresh layout, since decoration changes
need not move source-position geometry. This does not change existing box-width
realization semantics. Filtered faces capture the flat predicate list by identity
(GNU `xfaces.c:evaluate_face_filter` uses `EQ` for window operands) and traverse
the wrapped face tail with the same cycle-safe face capture. This includes the
current resolver's window-system extension without evaluating predicates during
capture or changing its filter semantics. Prefix and replacement-string tests
cover both nested face mutations and changes to the filter's operands.
Inline stipple bitmaps capture their three `(WIDTH HEIGHT DATA)` operands,
including owned data bytes. Published-face tests compare retained and fresh
bitmap patterns; pixel geometry alone cannot detect stale bitmap contents.
This follows the input shape documented by GNU `xfaces.c:Fbitmap_spec_p` without
changing Neomacs' current bitmap validation. Bitmap-file content changes still
need resource invalidation rather than Lisp input capture.

Compound `:font`/`:fontset` operands are not currently consumed by the inline
face plist resolver (`Face::from_plist_realized`), unlike GNU's `merge_face_ref`.
That is a separate compatibility gap, not an observable stale-input bug on this
path; adding that support must also define its owned dependencies. Other
image/resource expressions remain audit items. Image specs in prefix-string
display properties now reuse the catalog's `ImageSpecIdentity`, rather than
maintaining a second list of image attributes in freshness capture. Geometry
regressions cover in-place margin-pair mutations, including vector display
containers on wrap prefixes. A deterministic host catalog supplies image extents;
the tests exercise real layout and query paths, not image decoding.

`ImageSpecIdentity` now owns a flat token stream for conses, ordinary vectors
and strings. String identity uses character count plus raw bytes, following GNU
`fns.c:internal_equal`; it does not require UTF-8 or retain text properties.
Iterative traversal avoids the generic equal-key depth cutoff, and flat storage
keeps hashing, comparison and destruction independent of list/vector nesting.
Active-path references terminate cycles while separately allocated acyclic
copies compare like shared subtrees. Other Lisp object kinds retain the existing
generic equal-key semantics; this is not a general cyclic-graph equality engine.
The representation is host-side machinery outside the GNU mirror, re-exported
through the unchanged catalog API. Catalog-key tests cover binary ownership,
hash lookup, deep structures and cycles; a layout regression changes a margin
after 256 image-property pairs.

Pixel-arithmetic image operands also carry `ImageSpecIdentity` inside the owned
expression stream. This covers direct image dimensions and images nested under
addition, subtraction or dotted scaling, both in stretch-space prefixes and in
prefix-string display properties. Capture does not resolve images: a catalog
fixture supplies pending extents while geometry regressions exercise retained,
full and query layout. Changing a file's contents is not a mutation of its Lisp
image specification. GNU `image.c:Fimage_flush` explicitly promises rereading
the file on the next redisplay after a flush; `uncache_image` also marks the
frame garbaged when entries are removed. This is not an automatic file-watch
contract.

Neomacs' corresponding lifecycle is already separate from spec identity:
catalog invalidation reports `ImageInvalidationResult::Changed`, and the Lisp
image builtins synchronously advance `media_generation`. This generation enters
both snapshot freshness and retained-window keys. Renderer decode/eviction
events reconcile catalog state before advancing the same generation. Decode
completion carries a load-attempt token so a stale completion cannot promote
a replacement attempt. The GUI catalog's existing tests cover eviction and
stale decode completion. A layout characterization test additionally checks
`image-flush`, clear-all, and filename-filtered clearing through real Lisp
builtins, retained/full layout and synchronous geometry queries, with unchanged
Lisp image specs. Its deterministic catalog models changed file dimensions;
it does not exercise disk reloads, decode workers or GPU texture retirement.
No extra invalidation counter or automatic file watcher is introduced.
This is not a claim of complete mutable-Lisp-graph invalidation.

Prefix glyph production must also update the walk's vertical geometry, not just
the emitted glyph row. GNU `xdisp.c:produce_stretch_glyph` computes stretch
ascent/descent and `produce_glyphs` accumulates the row maxima, including prefixes
installed by `handle_line_prefix`. Neomacs' prefix append previously emitted the
correct stretch dimensions without promoting the walk geometry, so even fresh
layout could leave the following row at the old vertical position. The row
prelude now uses the existing `include_current_row_visible_content_metrics`
operation (also used by line numbers) to reconcile both authorities after a
requested line/wrap prefix. The request gate avoids scanning row glyphs on
ordinary text steps. Tests assert a minimum 60px line separation independently
of incremental/full comparison, and cover mutable direct, image-derived and
string-contained stretch heights through redisplay and geometry queries.
The same reconciliation covers prefix images: changing the vertical component
of `:margin` now moves following rows for line and wrap prefixes. A separate
published-frame assertion checks that a 24px image with two 10px vertical
margins reserves at least 44px without overlapping the following row. It covers
both pending and ready catalog extents. This follows GNU's image glyph metrics
in `xdisp.c:produce_image_glyph`, which include both margins for an unsliced
image; sliced-image margin ownership is outside this test's scope.

`LayoutInvisibilityInput` captures the effective buffer's ordered invisibility
membership, including each cons entry's category and ellipsis truthiness.
In-place `setcar`/`setcdr` changes therefore participate in both the shared
freshness projection and retained-row key. This follows GNU's identity-based
membership checks; it does not deep-copy category objects or interpret non-nil
ellipsis tails beyond their truthiness.

Direct character-table writes already advance an evaluator-thread revision.
`CharTableLayoutRevision` is shared by snapshot validation and the retained-row
key, preventing rejected geometry from being rebuilt out of stale retained rows.
This retains the existing conservative invalidation across tables.
`LayoutDisplayTableInput` additionally captures reachable glyph-vector contents,
including `(character . face-id)` glyph codes, for the active buffer or standard
display table. Defaults, extra slots and parents participate. Capture walks
explicit table storage and deduplicates shared vectors; it never enumerates the
character-code space or copies buffer text. Unrelated vectors do not invalidate
layout, and restoring glyph contents restores input equality. This costs O(table
storage + unique stored glyphs) per capture; tracking only consulted mappings is
a future refinement, not a claimed constant-time path. Direct table writes still
use the conservative revision above.

Source backtracking retreats past display/invisibility spans covering a
newline before measuring forward. Producer-owned row metadata distinguishes
buffer rows, replacement newlines, replacement wraps, and before/after overlay
strings. Several physical rows may share one source anchor; collapsing them
loses the actual motion distance.

Fontification and automatic composition use the measurement's requested row
extent, not the live viewport's height. Their source-coverage estimates remain
part of the layout engine; consumers must not duplicate display interpretation.

## Plan, validate, commit

Graphical pages use measured pixel heights and GNU's default-line-height page
quantization. Partial scrolling within a tall first row is a pixel offset,
not an invented buffer position. A candidate viewport also chooses a visible
point, respecting scroll margins and screen-position preservation policy.
Consecutive scroll commands retain a typed VM-owned pixel goal, so a short
intermediate row does not permanently lose the original column. A clipped
tall point row below ordinary text is promoted to window-start before its
contents are pixel-scrolled.

Both graphical and terminal scroll plans are speculative. Fontification can edit text, move markers or
change layout while a query runs. A fresh individual query is therefore not
enough: the entire plan compares the existing canonical input witness and
captured scroll policy before and after measurement. If either changes, the
plan (including any provisional error) is discarded and retried, with a
bounded convergence limit.

Only then does `WindowScrollUpdate` validate the target identity and accessible
positions and synchronously update point, window-start, old-point when needed,
vscroll and redisplay flags. No Lisp runs inside this commit. Terminal row
scrolling uses the same transaction and marker-update owners. The pixel goal
is committed with the viewport, not during speculative queries. It is reset
when creating/restoring a VM, not serialized as application data. Smooth pixel input retains its
existing separate input policy; it must not independently reinterpret rows.

## Regression boundary

`neomacs-layout-engine/src/engine_display_motion_test.rs` tests Lisp-visible
motion against the real layout-query engine, including offscreen replacement
and overlay rows, hidden/narrowed starts, tall rows, scrolling policy,
fontification edits, and offscreen composition. Keep compatibility expectations
grounded in GNU's interactive engine: setting Lisp `noninteractive` to nil in
GNU `--batch` does not switch its underlying C engine.

These tests establish the shared architecture and covered behavior, not full
equivalence with every GNU display iterator case (for example, arbitrary
positions inside replacement strings or every bidi/continuation combination).
Add those cases at this shared boundary, not in a browser-only workaround.
