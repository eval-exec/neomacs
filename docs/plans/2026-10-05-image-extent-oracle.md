# Image extent: one answer, one place

Status: partially landed (PR #477). Phases 0–2 and the frame-sharing half of
phase 4 are in. Phase 3 landed its interface half — the raw conversions are
private and the public surface is typed — while the newtype half and the
load-keyed half of phase 4 remain open and are described below.

## The bug that prompted this

An animated image used as the leftmost tab-bar item moved the whole tab bar one
pixel left and right at the animation rate. The image was fine — a well-formed
24-frame GIF with uniform full-canvas frames — and so was the file's own size:
`image-size` reported the same extent for every frame.

What moved was the *reserved* extent. The catalog reserves a slot before the
header lands, the header probe then reports the image's real geometry, and the
two disagreed by one pixel for a scaled spec: `:height 24` at layout scale 0.8
is `24 * 0.8 = 19.2`, which the reservation rounded to 19 and the probe ceiled
to 20. Because each animation frame is its own `ImageResolveRequest`, the
disagreement re-ran on every frame swap instead of once per image.

## What the shape of the code allowed

One question — *what is this image's extent?* — was answered by three producers
in two crates:

| producer | knowledge it has | where |
|---|---|---|
| `placeholder_image_extent` | the size spec only | `crates/neomacs/src/image_catalog.rs` |
| `probe_image_layout` | the encoded header | `neomacs-renderer-wgpu/src/image_probe.rs` |
| `realized_geometry` | the decoded frame | `neomacs-renderer-wgpu/src/image_cache.rs` |

They must agree, and three separate comments say so — *"A pending layout built
from it therefore cannot move when the pixels land"*, *"a layout resolved from
the header must equal the layout the decode reports"*, *"Both resolutions must
produce the identical extent: they are the same `resolve_geometry` call over the
same native size"*. Nothing enforced it. `resolve_geometry` was correct,
available, and optional; the placeholder hand-composed the same conversion from
lower-level helpers and picked a different rounding. The guard test compared two
of the three producers, and only for an unscaled spec — where the rounding
cannot differ. A `tracing::warn!` (`report_header_disagreement`) watched one pair
for disagreement, which is a diagnostic, not an invariant.

The deeper causes, in order of how much they cost:

1. **The interface was parameterized by who is asking, not by what is known.**
   Three callers with three knowledge states each re-derived the answer, instead
   of one resolver taking the knowledge state as input.
2. **Coordinate spaces are all `u32`.** Native/spec, GNU image-pixel, logical
   layout and device/raster pixels are distinguished only by prose and field
   names, so choosing the wrong conversion is a silent one-pixel drift —
   invisible at most sizes, and only visible at all when something downstream
   inherits it.
3. **Provisional and final values share a type.** A reservation is a guess that
   must be replaced; nothing marked it as such, so "the slot must not move" was
   a rule each call site kept or forgot on its own.
4. **Geometry is keyed per request, and the request carries the frame index.**
   The extent is a property of the *load* (`image_load_identity` is format +
   file, deliberately without `:index`), but it was computed per request, so
   every per-request transient became a per-frame visual artifact.

## Intended shape

One module answers the question; everything else consumes or supplies inputs to
it.

```
resolve_extent(load, spec, realization, rotation) -> Extent        // final
resolve_provisional(spec, realization, rotation) -> Provisional<Extent>

Provisional::finalize(final: Extent) -> SlotChange                 // Unchanged | Moved
```

- **Seam**: the geometry module is pure — it needs a native extent and a
  knowledge state, nothing else — so it lives with the domain types
  (`neomacs-display-protocol`). The header probe and the decoder are *adapters*
  at that seam: they differ in how they learn the native extent, which is the
  only variation worth a seam.
- **Depth**: callers learn two functions. Header probing, decoding, aspect
  fallback, clamps, rotation, rounding and caching are implementation.
- **Locality**: a rounding question is answered in one file, by one function,
  with one policy — instead of by whichever helper a call site happened to pick.

## Phases

| phase | change | class of bug removed | status |
|---|---|---|---|
| 0 | placeholder resolves through `resolve_geometry` | this instance | ✅ PR #477 |
| 1 | `resolve_provisional` owns the reservation, behind the same resolution as the probe and the decode | duplicate computation | ✅ PR #477 |
| 2 | `ProvisionalExtent` / `SlotChange`; the refinement site classifies and counts moves | invisible movement | ✅ PR #477 |
| 3 | typed coordinate spaces; raw per-space helpers stop being public API | unit confusion | half: helpers private, typed `report_extent`/`raster_extent`/`layout_extent` in their place; newtypes open |
| 4a | resolved geometry keyed without the frame index, shared by every frame | per-frame refinement | ✅ PR #477 |
| 4b | geometry keyed by load identity, frames selecting pixels only (with the animated-visual/media-clock work) | per-frame transients generally | open |

### Phase 3, concretely

Landed on this branch: `layout_dimension`, `raster_dimension` and
`image_pixel_dimension` are private to the geometry module, and the public
surface answers in named spaces (`report_extent`, `raster_extent`,
`layout_extent`). `ResolvedImageMetadata::from_layout` collapsed to one call,
and the four external test call sites went through the typed conversions. A
caller can no longer pick a rounding, because there is no exported helper left
to pick — the mistake needs a deliberate construction.

Still open: `NativePx`, `ImagePx`, `LayoutPx`, `DevicePx` as
`#[repr(transparent)]` newtypes, so even a deliberate construction cannot mix
the spaces. That is what turns "unexported" into "unrepresentable", and it is a
mechanical migration of the geometry module's internals.

### Phase 4b, concretely

`ImageResolveRequest` carries `frame: ImageFrameIndex`, and it is both the cache
key for slots and the key the header probe is filed under. Geometry should be
keyed on the load identity (format + source + spec + realization + rotation)
with frames selecting pixels through the media clock, so that a frame swap is a
pixel operation and never a geometry question. This overlaps the
`animated_visual` / `media_clock` work; do them together.

## Invariants to keep

- A spec that pins both axes reserves exactly what it resolves to, under every
  scale, rotation and native size
  (`provisional_extent_equals_resolved_geometry_when_both_axes_are_pinned`).
- A spec that leaves an axis to the native size is the *only* case where a
  reservation may move (`provisional_extent_moves_only_where_the_native_aspect_is_unknown`).
- A slot moves at most once per reservation: the refinement site is the only
  place a move happens, and `AsyncImageCatalog::slot_moves` counts it. Zero is
  the expected count for a pinned spec; anything else is a disagreement, and the
  counter is how it gets noticed instead of being paid for at animation rates.
- Frames of one image share one resolved geometry
  (`a_later_frame_reserves_the_geometry_an_earlier_frame_resolved`).
- Header geometry equals decoded geometry, over formats × specs × rotations ×
  realizations (`header_layout_equals_the_decoded_layout_for_every_probed_format`).

## Open questions

- Should `PendingImage` carry a `ProvisionalExtent` rather than a bare
  `ImageLayoutExtent`, so "this slot is a reservation" is a type fact rather
  than a comment at the one site that finalizes it?
- Should the slot-move counter be exported (a metric, or an assertion under
  `debug_assertions`) so a disagreement fails CI rather than a log line?
- `AtMost` clamps are applied unscaled in `desired_intrinsic` while the probe
  scales the native extent first, so a `:max-height` spec can still disagree
  by more than a pixel. Out of scope here; it wants its own decision about
  which side matches GNU.
