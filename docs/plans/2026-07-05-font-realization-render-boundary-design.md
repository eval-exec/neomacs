# Font Realization and Render Boundary Design

Date: 2026-07-05
Status: Phases 0–5 shipped for composed clusters (face-primary, per-char
fallback, shaped-cluster exact-glyph rasterization, shared `FontResolver`, and
Fontconfig/CoreText/DirectWrite candidate backends); Phase 6 seam in place
(`TextShaper`, dead second cosmic path deleted). Remaining: whole-run shaped
output where GNU composes beyond `Composite` clusters and a rustybuzz
`TextShaper`.

### Automatic native-catalog refresh (2026-09-01)

- Native change detection stays inside the existing platform adapter layout:
  Fontconfig freshness polling in `font_backend/linux/catalog.rs`, CoreText
  local and distributed registration notifications in
  `font_backend/macos/catalog.rs`, and rate-limited DirectWrite
  `GetSystemFontCollection(TRUE)` polling in
  `font_backend/windows/catalog.rs`. No platform framework object escapes its
  adapter and no additional runtime crate owns a second font lifecycle.
- Polling adapters share one typed rate-limit/fan-out state machine in
  `font/catalog.rs`; only their native snapshot operation varies. Linux holds a
  referenced Fontconfig configuration while testing freshness and, after a
  proven stale result, forces `FcInitReinitialize` rather than delegating to
  Fontconfig's independent rescan-interval scheduler. Windows snapshots the
  DirectWrite collection identity and family count together, and rebases each
  consumer's snapshot when it observes a peer-published edge so that one native
  replacement cannot be published twice.
- Native callbacks publish only to a process-wide atomic change counter. Every
  `FontMetricsService` owns an independent cursor into that counter, so the
  display host cannot consume an event before the layout engine observes it.
  Bursts coalesce to one change per service and cache mutation remains on the
  evaluator thread at a redisplay safe point. Fontconfig and DirectWrite also
  publish detected polling edges into the shared counter, covering every
  service even when the first poll refreshes process-global native state.
- `FontMetricsService` is the exhaustive layout-side invalidation owner. It
  advances a typed `FontCatalogGeneration`, replaces cosmic-text's append-only
  `FontSystem`, resets exact-file materialization state, advances the native
  backend, and clears resolver, metrics, shaping, fallback, and platform-query
  caches. `LayoutEngine` then rejects all retained visual history, face arenas,
  matrices, chrome metrics, and speculative output derived from old font
  geometry.
- Every immutable frame snapshot carries its nonzero
  `FontCatalogGeneration`. Frame materialization preserves it, child frames
  from another generation are not composited with a root, and the WGPU atlas
  treats a generation transition as a complete raster/fontdb/materializer
  boundary. This makes stale cross-thread font reuse structurally visible
  instead of relying on every cache author to remember an ad-hoc clear hook.
- In-memory consumers borrow those fields through one typed
  `FrameFontBindings` view. Render installation and retained mini-frame cloning
  therefore move the catalog generation, faces, exact fonts, per-character
  fallback, and shaped clusters as one compile-checked unit. The renderer starts
  with an empty font database and performs its first system scan only when that
  first frame binding arrives, closing the construction race without scanning
  the catalog twice.
- The design follows GNU Emacs' macOS lifecycle: the CoreText callback in
  `src/macfont.m` invalidates catalog-derived family caches and later font
  lookup reconstructs them. Neomacs makes the callback boundary stricter by
  publishing only an atomic edge and mutating caches at a redisplay safe point.
  Linux and Windows add platform-supported missed-event polling because GNU
  does not provide equivalent automatic refresh there.

### Shared exact-font materialization (2026-08-27)

- `neomacs-font-materializer` is the single file/container/opening boundary
  used by layout and WGPU rendering. It owns WOFF/WOFF2 decoding, exact
  file-face pinning, FreeType fixed-strike selection, metrics, glyph lookup,
  raster normalization, and renderer replay. Its typed exact-face cache owns
  the synthetic cosmic-text family, generation-local fontdb id, and retryable
  failure state; layout and rendering no longer maintain parallel pin caches.
  Neither consumer opens or independently classifies a raw source.
- Materialization is capability-driven, not suffix-driven at the public API.
  The implementation sniffs WOFF/WOFF2, BDF, PCF, gzip-wrapped PCF, and SFNT
  table content before considering a suffix hint; renaming a font cannot
  silently change which adapter owns it.
  `FontReplay::Swash { asset }` owns either an exact file face or immutable
  standalone SFNT bytes; `FontReplay::FreeTypeBitmap { asset, strike,
  sampling, spacing }` owns the exact file face, fixed strike, texture
  sampling rule, and GNU spacing policy. An outline replay without a source,
  or a bitmap replay without a file, is therefore unrepresentable. PCF, compressed
  PCF, BDF, and OTB all use the same FreeType bitmap adapter on Unix and
  Windows (bundled FreeType on Windows).
  Process-local font handles never enter frame state.
- Platform discovery and replayable realization are separate types.
  `PlatformFontCandidate` may carry a cheap native locator while shared GNU
  policy scores candidates. Only the winning candidate is finalized into a
  `PlatformFontMatch`, which must own a `FontOutlineAsset`. A URL-less native
  winner is copied into immutable OpenType bytes only after selection: CoreText
  reconstructs a standalone SFNT from its tables, while DirectWrite reads a
  selected one-file TrueType/OpenType outline through the loader stream exposed
  by `dwrote`. DirectWrite's typed format gate follows Microsoft's
  [`DWRITE_FONT_FACE_TYPE`](https://learn.microsoft.com/en-us/windows/win32/api/dwrite/ne-dwrite-dwrite_font_face_type)
  contract; URL-less bitmap, vector-FON, raw-CFF, and Type 1 streams are rejected
  until the shared materializer has typed replay plans for them. Layout
  and rendering then pin the same reference-counted bytes in their independent
  fontdb instances; no CoreText or DirectWrite object crosses a thread or
  display boundary, and publishing a frame clones an `Arc` rather than copying
  the font. Multi-file DirectWrite faces remain explicitly unsupported.
- CoreText and DirectWrite each own a bounded, weak native-byte interner for
  their current catalog generation. Concurrent finalization converges on one
  allocation, but cache metadata never keeps font data alive. Advancing the
  resolver's catalog generation clears the interner, so a replaced process
  font cannot alias live bytes from an older catalog; old frame snapshots keep
  their own immutable `Arc` alive until rendering releases it.
- `ResolvedFontId` interns the complete metrics-bearing instance: durable
  source identity, replay plan/strike, and the **effective opened logical
  size**, not merely the request. One source file realized at different sizes
  or strikes cannot alias one protocol table entry. Native candidate discovery
  and resolver caches include requested logical size and device scale, so a
  fixed 13px entity cannot win a request for the same family's 26px entity.
  Fixed-size eligibility and scoring mirror GNU `font.c`: reject requests more
  than two times from an available entity, compare integer pixels, and cap the
  doubled distance at 127.
- Bitmap glyph ids use the full FreeType `u32` domain. Layout publishes exact
  `(font, glyph, advance)` bindings for visible scalars and simple-copy
  composition clusters; rendering consumes those bindings without another
  charmap lookup or semantic font selection. The actual binding also enters
  atlas and row-reuse cache identity.
- Fixed strikes are selected in device pixels and reported back in logical
  pixels. Their normalized masks retain physical strike dimensions and use a
  dedicated nearest-neighbor WGPU sampler; outline/color glyphs retain linear
  sampling, including when one logical composition contains both source kinds.
  CPU scaling is not used to disguise a wrong strike choice. The
  default fixed-font line height is occupied ascent + descent, matching GNU's
  default `:minspace` policy; the request type explicitly represents the
  native-height alternative.
- Fontdb source outcomes distinguish loaded-with-family,
  loaded-without-family, intentionally unsupported bitmap containers, and
  actual I/O/decode/rejection failures. Supported bitmap fonts bypass fontdb;
  real failures remain warnings. WOFF decoding preserves the requested face
  index and the pinned in-memory face id, so renderer and layout replay the
  same decoded source rather than substituting a sibling SFNT file.
- Binary test fonts are not tracked. The dev-only `neomacs-test-fonts` crate
  downloads release/commit-pinned sources during tests, verifies archive and
  per-font SHA-256 digests, serializes concurrent setup with a file lock, and
  caches them only under ignored `./tmp/font-fixtures`. A failed download or
  integrity check fails the test instead of silently skipping coverage.

## 0. Amendments from implementation (2026-07-05)

Decisions made while shipping Phases 0–2 and 3a:

- **Phase 2 warning scope.** Per-character coverage fallback (CJK/emoji)
  remains a render-side decision until glyph-level resolved fonts exist, so
  the "unresolved GUI text" warning and counter
  (`unresolved_face_text_total`) fire only when a face has neither a
  resolved identity nor the C-FFI `font_file_path` bridge — not whenever
  per-char fallback engages. Per-char fallback is traced separately under
  the `font_boundary` target.
- **`ResolvedFontId` scoping (answers §15).** Ids are interned from the complete
  realized instance by the
  layout-side `FontMetricsService` and are stable for the service's
  lifetime (the interner survives `clear_caches`, append-only). Frame
  snapshots carry the subset used. Renderer caches must key on the
  *identity* (or a resolver-lifetime id whose incoming mapping is validated
  and whose reuse invalidates renderer caches), never an unchecked raw id.
- **Phase 3a evolved into an exact fontset projection.** Frames carry
  `char_fonts: face_id → visible scalar → (ResolvedFontId, ResolvedGlyphId,
  advance)`. This is GNU-shaped—fontset lookup is `(face, char) → opened
  font`—but makes the result renderable without repeating lookup. Cache keys
  include the exact binding because a character code alone cannot distinguish
  two changed fontset answers. Composition clusters additionally carry their
  layout-produced glyph stream.
- **Realization seam.** Face- and char-level realization run as one pass
  (`realize_frame_fonts`) at `LayoutEngine::finish_frame_output`, after
  all install paths have filled the frame state — no per-call-site
  plumbing. Measured steady-state cost ~37µs/frame at 40 faces (release),
  on the evaluator thread; probe test
  `realize_frame_fonts_steady_state_cost_probe` keeps it measurable.
- **Phase 3b parity guard (constraint on §8/§12).** GNU does NOT shape
  plain Latin text: it takes per-char metrics from the font driver, and
  shaping happens only through the composition machinery
  (`composition-function-table`, auto-composition). Shaped runs must be
  emitted only where GNU would compose; `ShapeOptions` for ordinary text
  must not introduce ligatures/kerning GNU doesn't apply, or column
  metrics diverge from GNU. This is also the performance strategy: the
  hot path for ordinary text never enters the shaper.
- **Enforcement.** `real_gui_smoke` gates on: every face in the frame
  snapshot carries `default_resolved_font_id`, and every referenced id
  exists in the frame's font table.

### Exact platform matches (2026-07-15)

- `FontBackend` returns complete `FontCandidate` values to the shared
  `FontResolver`, never a preselected path. The winning
  `PlatformFontMatch` includes Fontconfig's full `FC_INDEX` (including
  named-instance bits) and `FC_FONT_VARIATIONS` coordinates on Linux.
- File path, an opaque backend-native face selector, and canonicalized
  non-default variation coordinates form one `ResolvedFontIdentity` and one
  renderer-cache identity. Fontconfig's selector is deliberately kept intact:
  its low 16 bits select a collection face and its high bits select a FreeType
  named instance. The raw selector is private behind explicit FreeType and
  file-parser accessors, so consumers cannot accidentally interchange them.
- Consumers translate identity only at their capability boundary. FreeType
  receives the full Fontconfig selector, while fontdb/ttf-parser receive
  `ResolvedFontIdentity::file_face_index()`. The current cosmic/swash boundary
  exactly replays resolved weight and slant; named instances with other
  non-default axes explicitly take the resolved fallback path until cosmic can
  accept an arbitrary variation tuple. An encoded named-instance selector is
  never treated as a collection index.
- A materializable platform selection is authoritative. Layout first proves
  that a shared materializer can open the exact file face, then preserves the
  platform identity, replay plan, and PostScript name; the renderer reopens
  that same face and exact fixed strike or decoded outline source. Unsupported
  capabilities take an explicit semantic fallback. Neither side silently
  substitutes the first face in a collection or publishes an identity the
  renderer cannot open.
- Semantic family, weight, and slant remain useful request/reporting metadata,
  but they are not permitted to reconstruct drawable truth after realization.

## 1. Problem Statement

Neomacs currently has two semantic font-selection paths for GUI text:

```text
Evaluator / layout thread
  face attributes, font-at, find-font, metrics, row layout
  -> FontMetricsService / fontconfig / fontset-like policy
  -> Lisp-visible and layout-visible selected font information

Render thread
  FrameGlyph + Face family/weight/slant/size
  -> glyph_atlas::face_to_attrs_for_text
  -> fontconfig fallback + cosmic-text/fontdb family selection
  -> rasterized glyphs
```

This makes the display pipeline capable of answering one font to Lisp/layout code
and drawing another font on screen. The immediate symptom is that Treemacs text can
look too wide or otherwise unlike GNU Emacs even when Lisp-level font oracle checks
appear plausible.

The current display protocol already has a partial bridge:

- `Face::font_file_path: Option<String>`
- renderer glyph-cache identity hashes `font_file_path`
- glyph atlas can prime a font file when the path is present

But pure-Rust layout normally drops that identity:

- `DisplayRowFace::from_resolved` sets `font_file_path: None`
- `FrameGlyphBuffer::synthesize_face` sets `font_file_path: None`

So `None` is the common GUI text path, and the render thread performs a fresh
semantic font decision from family/weight/slant.

## 2. Design Principle

Semantic text realization must happen before the render thread.

The render thread may ask:

```text
How do I rasterize this exact resolved font/glyph?
```

The render thread must not ask:

```text
Which font should this face or character use?
```

This is stricter than "pass a font file to the renderer." Font selection and
text shaping are coupled. Emoji variation selectors, CJK fallback, Arabic/Indic
shaping, ligatures, and fontset fallback can all change glyph IDs, cluster
boundaries, positions, and advances. If layout selects one font but the renderer
reshapes from family/weight/style, the bug class remains.

## 3. Target Architecture

Long term, the pipeline should be:

```text
Face attrs + text + frame/fontset context
        |
        v
Shared Emacs-compatible font policy
        |
        v
Platform font backend
  Linux: fontconfig
  macOS: CoreText
  Windows: DirectWrite
        |
        v
ResolvedFontIdentity + ResolvedFont metrics
        |
        v
Exact-font shaper
  HarfBuzz / rustybuzz / platform shaper behind one trait
        |
        v
Resolved shaped glyph stream
        |
        v
Render thread
  open/cache exact font handles
  rasterize exact glyph IDs
  manage atlas/GPU resources
```

The shared policy layer owns GNU/Emacs compatibility. Platform backends only
enumerate/open candidate fonts and provide exact font metadata.

## 4. Ownership Split

Evaluator/layout owns:

- face inheritance, remapping, and face IDs
- fontset lookup and fallback order
- the point at which generic-family aliases and alternative families are tried
- alternative font family and registry alists
- fontconfig/CoreText/DirectWrite candidate enumeration through backend traits
- GNU-compatible candidate scoring
- character-coverage requirements and fallback decisions
- weight/slant/width normalization
- exact font identity creation
- shaping into glyph IDs, cluster ranges, offsets, and advances
- Lisp-visible APIs: `font-at`, `find-font`, `face-font`, `font-info`
- row metrics and cursor/layout geometry

Render thread owns:

- exact font-handle cache
- glyph rasterization for `(font identity, glyph id, size, variation coords)`
- glyph atlas allocation and eviction
- subpixel bins and mask format decisions
- color glyph upload
- GPU buffers, draw ordering, clipping, effects, and compositing

Render thread does not own:

- fontconfig/CoreText/DirectWrite fallback decisions
- family alias resolution
- fontset lookup
- weight/slant/width substitute selection
- emoji/CJK fallback choice
- shaping by family/weight/style request

## 5. Core Types

### 5.1 FontRequest

`FontRequest` is an input to the resolver. It should not cross the render boundary
as drawable truth.

```rust
pub struct FontRequest {
    pub frame_id: DisplayFrameId,
    pub face_id: u32,
    pub family: Option<String>,
    pub foundry: Option<String>,
    pub registry: Option<String>,
    pub weight: FontWeight,
    pub slant: FontSlant,
    pub width: FontWidth,
    pub pixel_size: f32,
    pub dpi: f32,
    pub character: Option<char>,
    pub script: Option<Script>,
}
```

### 5.2 ResolvedFontIdentity

`ResolvedFontIdentity` is an exact, platform-openable identity. Do not define this
as "file path only"; macOS and Windows may need native descriptors or stable
backend keys.

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ResolvedFontIdentity {
    pub backend: FontBackendKind,
    pub stable_key: String,
    pub file_path: Option<PathBuf>,
    pub face_index: u32,
    pub postscript_name: Option<String>,
    pub collection_index: Option<u32>,
    pub variation_coords: Vec<FontVariationCoord>,
}
```

Linux can usually populate `file_path + face_index`. macOS can use CoreText font
URLs/descriptors. Windows can use DirectWrite face identity, with file paths only
when reliably available.

### 5.3 ResolvedFont

`ResolvedFont` is the resolver's canonical answer for a concrete font instance.

```rust
pub struct ResolvedFont {
    pub id: ResolvedFontId,
    pub identity: ResolvedFontIdentity,
    pub replay: FontReplay,
    pub family: String,
    pub full_name: Option<String>,
    pub postscript_name: Option<String>,
    pub weight: FontWeight,
    pub slant: FontSlant,
    pub width: FontWidth,
    pub pixel_size: f32,
    pub metrics: FontMetrics,
    pub source: FontResolutionSource,
}
```

`source` should distinguish primary face font, fontset fallback, emoji fallback,
platform fallback, and emergency fallback. This is important for traces and oracle
debugging.

### 5.4 ResolvedGlyph

`ResolvedGlyph` is the renderable unit. It is already past semantic selection and
shaping.

```rust
pub struct ResolvedGlyph {
    pub resolved_font_id: ResolvedFontId,
    pub glyph_id: u32,
    pub cluster_start: usize,
    pub cluster_end: usize,
    pub x_offset: f32,
    pub y_offset: f32,
    pub x_advance: f32,
    pub y_advance: f32,
}
```

### 5.5 ShapedTextRun

`ShapedTextRun` is the strongest display contract between layout and render.

```rust
pub struct ShapedTextRun {
    pub text: Box<str>,
    pub text_range: Range<usize>,
    pub face_id: u32,
    pub glyphs: Vec<ResolvedGlyph>,
    pub direction: TextDirection,
}
```

For simple ASCII this is still one glyph per character. For complex clusters it
can contain multiple glyphs and cluster mappings.

## 6. Display Protocol Shape

Frame state should carry a resolved font table:

```rust
pub struct FrameDisplayState {
    pub fonts: HashMap<ResolvedFontId, ResolvedFont>,
    pub faces: HashMap<u32, Face>,
    pub text_runs: Vec<DisplayTextRun>,
    // existing backgrounds, cursors, images, videos, borders, etc.
}
```

`Face` remains the visual face record: colors, decorations, default font id,
metrics, Lisp name. It should stop being treated as a render-time font request.

```rust
pub struct Face {
    pub id: u32,
    pub foreground: Color,
    pub background: Color,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub default_resolved_font_id: Option<ResolvedFontId>,
    // existing decoration fields
}
```

Final renderable text should reference shaped glyphs:

```rust
pub struct DisplayTextRun {
    pub window_id: DisplayWindowId,
    pub row_role: GlyphRowRole,
    pub clip_rect: Option<Rect>,
    pub slot_range: Range<DisplaySlotId>,
    pub face_id: u32,
    pub x: f32,
    pub y: f32,
    pub baseline: f32,
    pub glyphs: Vec<ResolvedGlyph>,
}
```

During migration, existing `FrameGlyph::Char` can remain. Add resolved font
identity first at the face table, then introduce run/glyph-level shaped output.

## 7. Platform Backend Abstraction

The resolver should be shared. Candidate enumeration and opening should be
platform-specific.

```rust
pub trait FontBackend {
    fn kind(&self) -> FontBackendKind;
    fn resolve_family(&self, generic_or_concrete: &str) -> String;
    fn family_prefers_monospace(&self, family: &str) -> bool;
    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate>;
    fn design_metrics(&self, selected: &PlatformFontMatch)
        -> Option<PlatformFontDesignMetrics>;
}
```

`FontCandidate` carries exact identity and style/width/spacing metadata. After
shared scoring, the backend attaches native design-unit metrics to the single
winning match; metrics are never computed for every enumerated candidate.
CoreText and DirectWrite therefore do not reopen a native selection through
FreeType merely to populate layout metrics. Opening
the selected identity and supplying exact font bytes remain capabilities of the
shared file/index materialization layer until native renderer handles are
needed for formats that do not expose a durable local file.

Backend implementations:

- `LinuxFontBackend`: fontconfig candidate list, file/index identities.
- `MacFontBackend`: CoreText descriptors, font URLs, collection indexes, native
  descriptors where paths are insufficient.
- `WindowsFontBackend`: DirectWrite families/faces, axis metadata, native face
  identity where paths are insufficient.

The shared resolver performs Emacs-compatible ordering and scoring over
`FontCandidate`.

## 8. Shaper Abstraction

Do not expose `cosmic-text` as the architectural contract. Hide shaping behind a
trait that consumes an exact `ResolvedFont`.

```rust
pub trait TextShaper {
    fn shape_run(
        &mut self,
        font_store: &mut FontStore,
        font: &ResolvedFont,
        text: &str,
        options: ShapeOptions,
    ) -> Result<Vec<ResolvedGlyph>, ShapeError>;
}
```

The initial implementation may use current `cosmic-text` machinery if it can be
constrained to exact font identity. The long-term implementation should prefer
HarfBuzz/rustybuzz over exact font bytes/face index so layout and renderer never
re-run high-level font selection.

## 9. Renderer API

Renderer glyph atlas APIs should move away from `Face`.

Current shape:

```rust
get_or_create_atlas(key: &GlyphKey, face: Option<&Face>, ...)
```

Target shape:

```rust
get_or_create_glyph(
    font: &ResolvedFont,
    glyph: &ResolvedGlyph,
    pixel_size: f32,
    subpixel: SubpixelRequest,
) -> Option<GlyphAtlasHandle>
```

Atlas cache key should be:

```rust
pub struct GlyphAtlasKey {
    pub resolved_font_id: ResolvedFontId,
    pub glyph_id: u32,
    pub pixel_size_bits: u32,
    pub variation_key: VariationKey,
    pub x_bin: SubpixelBin,
    pub y_bin: SubpixelBin,
    pub render_mode: GlyphRenderMode,
}
```

It should not contain family/weight/slant as selection inputs. Those are already
resolved into `ResolvedFontId`.

## 10. Handling Unresolved Fonts

Unresolved GUI text should become abnormal.

Migration policy:

1. Initially warn when GUI text reaches renderer without a resolved font identity.
2. Add a counter and trace target for unresolved/emergency fallback usage.
3. Gate tests on zero unresolved normal text in representative GUI snapshots.
4. Eventually reject unresolved text in debug builds.

Emergency fallback should be explicit:

```rust
fn emergency_unresolved_font_fallback(face: &Face, text: &str) -> ResolvedFont {
    tracing::error!(
        face_id = face.id,
        family = %face.font_family,
        text = %text,
        "unresolved GUI text reached render thread; using emergency font fallback"
    );
    // keep UI alive, but mark result as FontResolutionSource::EmergencyFallback
}
```

The normal renderer path must not call fontconfig/CoreText/DirectWrite to answer
"which font should this use?"

## 11. Compatibility With GNU Emacs

GNU Emacs has multiple callers, but they converge on realized face/font objects:

```text
face attributes / font spec
  -> font_find_for_lface
  -> font_open_for_lface
  -> realized face->font

font-at / display / fallback
  -> same realization machinery
```

Neomacs should mirror that property, even though it has a render thread. The
render thread may own GPU resources and thread-local font handles, but it should
consume the same realized font result that Lisp-visible APIs report.

The desired invariant:

```text
(font-at ...)
(find-font ...)
layout advances
actual rendered glyphs
```

must agree on resolved font identity for normal text.

## 12. Migration Plan

### Phase 0: Instrument Current Divergence

- Add trace logs when layout emits GUI `Face` records with `font_file_path == None`.
- Add trace logs when `glyph_atlas::face_to_attrs_for_text` invokes fallback or
  `match_font_for_char`.
- Capture requested face fields and actual selected font file/postscript from the
  renderer for Treemacs repro frames.

### Phase 1: Carry Face-Level Resolved Font Identity

- Add `ResolvedFontId`, `ResolvedFontIdentity`, and `ResolvedFont` to
  `neomacs-display-protocol`.
- Add `FrameDisplayState::fonts`.
- Add `Face::default_resolved_font_id`.
- Populate default face font identity from the existing layout/binary
  `FontMetricsService` resolution path.
- Preserve `font_file_path` as a temporary Linux bridge, but do not treat it as
  the final abstraction.

### Phase 2: Make Renderer Font Selection Emergency-Only

- Change renderer code to prefer `ResolvedFontId` over family/weight/slant.
- Move current `glyph_atlas::face_to_attrs_for_text` semantic fallback behind
  `emergency_unresolved_font_fallback`.
- Warn or error when normal GUI text lacks resolved identity.
- Update glyph cache identity to use `ResolvedFontId` and font generation.

### Phase 3: Introduce Exact Shaped Runs

- Add `ShapedTextRun` / `ResolvedGlyph` display protocol types.
- Teach layout text-run measurement to retain glyph IDs, cluster ranges, offsets,
  advances, and `ResolvedFontId`.
- Emit shaped runs for GUI text while retaining existing char glyphs for fallback
  and incremental migration.
- Add tests proving the shaped run used for layout is the run passed to render.

### Phase 4: Rasterize Exact Glyph IDs

- Change glyph atlas APIs to consume `ResolvedFont + ResolvedGlyph`.
- Open/cache exact platform font handles by `ResolvedFontIdentity`.
- Rasterize exact glyph IDs.
- Remove normal renderer calls to `fontconfig`, `font_match`, and family-based
  `cosmic-text` selection.

### Phase 5: Cross-Platform Font Backends

- [x] Extract Linux candidate discovery behind `FontconfigBackend`.
- [x] Add backend trait and shared `FontResolver`.
- [x] Add `CoreTextBackend` using CoreText descriptors/cascade lists.
- [x] Add `DirectWriteBackend` using DirectWrite families/fallback identities.
- [x] Keep fontset ordering and candidate scoring shared and platform-neutral.
- [x] Carry native design-unit metrics and locale/direction fallback context.

### Phase 6: Replace High-Level Cosmic Contract

- Hide any remaining `cosmic-text` usage behind `TextShaper`.
- Prefer HarfBuzz/rustybuzz shaping over exact font bytes/face index.
- Delete duplicated layout/render `FontSystem` semantic selection.

## 13. Testing Strategy

Unit tests:

- `ResolvedFontIdentity` equality and hash stability.
- Font candidate scoring independent of backend.
- `Face` with resolved default font survives frame protocol round trip.
- Glyph atlas key changes when `ResolvedFontId`, glyph id, variation coords, or
  subpixel bins change.

Integration tests:

- `font-at` result file/postscript equals frame text run `ResolvedFont`.
- `find-font` result agrees with resolver candidate chosen for matching face.
- Treemacs-like bold/regular Noto/monospace rows emit non-`None`
  `ResolvedFontId`.
- Emoji variation-selector clusters use emoji resolved font identity.
- CJK fallback emits glyph-level fallback `ResolvedFontId`, not face default.

Oracle tests:

- Compare GNU Emacs and Neomacs `font-at` / `find-font` at Lisp level.
- Verify renderer trace for the same text uses the same resolved identity.
- Treat renderer emergency fallback count as a failure for normal GUI oracle runs.

Visual/regression tests:

- Treemacs sample frame screenshot.
- Mixed ASCII/CJK/emoji frame screenshot.
- Text-scale and face-remap sample.
- Variable font weight sample, if available.

## 14. Non-Goals

- Do not make the render thread own GNU-compatible font policy.
- Do not make `file_path` the only durable font identity.
- Do not require every platform to expose a stable filesystem path.
- Do not remove `cosmic-text` before the resolver/shaper abstraction exists.
- Do not mix unrelated display row or render-thread refactors into this migration.

## 15. Open Questions

- Should the initial shaped-run protocol coexist with `FrameGlyph::Char`, or
  should it replace char glyphs for GUI text immediately?
- Should `ResolvedFontId` be frame-local, process-global, or generation-scoped?
  Frame-local is simplest for protocol snapshots; process-global may improve
  renderer cache reuse.
- Should shaping live in `neomacs-layout-engine` or a new `neomacs-font` crate?
  A new crate may be cleaner once macOS/Windows backends arrive.
- How much GNU font scoring should live in `neovm-core` versus layout/font crate?
  Lisp-visible APIs need access, but render/layout should not depend on evaluator
  internals unnecessarily.
- Can `cosmic-text` be constrained to exact font identity well enough for Phase 3,
  or should the first shaped-run implementation use HarfBuzz/rustybuzz directly?

## 16. Success Criteria

- Renderer normal text path has no semantic font selection.
- GUI text frame snapshots carry resolved font identities.
- `font-at`, `find-font`, layout metrics, and rendered glyphs agree on font
  identity for normal text.
- Renderer emergency fallback usage is zero in standard GUI oracle runs.
- The design supports Linux/fontconfig, macOS/CoreText, and Windows/DirectWrite
  without making Linux file paths the protocol contract.
