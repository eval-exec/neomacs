//! Cosmic-text based font metrics service for the layout engine.
//!
//! This module provides font measurement using cosmic-text, the same font
//! system used by the render thread for glyph rasterization. By using the
//! same font resolution logic for both measurement and rendering, we
//! guarantee that character widths computed during layout match the actual
//! rendered glyph widths — eliminating gaps and overlaps caused by the
//! C fontconfig and cosmic-text resolving different font files.

use crate::font::frame_metrics::{FrameFontDomain, GraphicFontSizePx};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Style, Weight};
use neomacs_display_protocol::types::FaceId;
use neomacs_font_materializer::FontFileCache;

/// Defensive wrapper around cosmic_text's raw-float API. Frame publication
/// validates graphic sizes before this backend boundary; the clamp remains for
/// lower-level shaping helpers that do not publish frame geometry.
fn safe_metrics(font_size: f32, line_height: f32) -> cosmic_text::Metrics {
    cosmic_text::Metrics::new(font_size.max(1.0), line_height.max(1.0))
}
use neomacs_display_protocol::font::{
    FontBackendKind, FontFileAsset, FontOutlineAsset, FontReplay, FontSlantKind, ResolvedFont,
    ResolvedFontAdvance, ResolvedFontId, ResolvedFontIdentity, ResolvedGlyph,
};
#[cfg(test)]
#[cfg(target_os = "linux")]
use neovm_core::face::FontWeight;
use neovm_core::face::{FontSlant, FontWidth};
// Every map in this module is an internal cache keyed by non-adversarial data
// (font-metrics keys, chars, family names) and looked up per char / per glyph
// during layout. Use FxHash, not std SipHash: the per-char resolved-font and
// char-width hashing was a chunk of the SipHash cost in a Doom scroll profile.
use rustc_hash::FxHashMap as HashMap;

fn swash_file_replay(identity: &ResolvedFontIdentity) -> Option<FontReplay> {
    let asset = FontFileAsset::from_identity(identity)?;
    Some(FontReplay::Swash {
        asset: FontOutlineAsset::File(asset),
    })
}
use ttf_parser::Face as TtfFace;

#[cfg(target_os = "linux")]
fn platform_foundry_for_file(file: &str) -> Option<String> {
    crate::font::fontconfig::foundry_for_file(file)
}

#[cfg(not(target_os = "linux"))]
fn platform_foundry_for_file(_file: &str) -> Option<String> {
    None
}

/// Font metrics returned for a given face configuration.
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Baseline offset from the top of the line box.
    pub ascent: f32,
    /// Distance from the baseline to the bottom of the line box.
    pub descent: f32,
    /// Total font height in pixels.
    pub line_height: f32,
    /// Default character width (space character width for monospace)
    pub char_width: f32,
    /// Advance of the primary font's space glyph.  This is distinct from
    /// `char_width` for proportional fonts and remains the face font's value
    /// even when a concrete space character would be shaped by a fallback.
    pub space_width: f32,
}

#[derive(Debug, Clone, Copy)]
struct FontVerticalMetrics {
    ascent: f32,
    descent: f32,
    line_height: f32,
}

/// Provenance of one metric observation.
///
/// Keeping fallback provenance beside the values prevents frame publication
/// from reconstructing (and potentially changing) how those values arose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontMetricSource {
    OpenedFontProbe,
    SelectedFontProbe,
    SelectedFontTables,
    GlyphBoxFallback,
}

#[derive(Debug, Clone, Copy)]
struct FontVerticalObservation {
    metrics: FontVerticalMetrics,
    /// Complete advances when the same backend probe supplied them.
    advances: Option<FontAdvanceMetrics>,
    effective_size: Option<GraphicFontSizePx>,
    source: FontMetricSource,
}

/// A single selection-and-measurement result cached as one unit.
#[derive(Debug, Clone, Copy)]
struct FontMetricObservation {
    metrics: FontMetrics,
    effective_size: Option<GraphicFontSizePx>,
    source: FontMetricSource,
}

struct CosmicPrimaryProbe {
    file: String,
    face_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricConfidence {
    Validated,
    Degraded,
}

#[derive(Debug, Clone, Copy)]
struct FontAdvanceMetrics {
    space_width: f32,
    average_width: f32,
    max_width: f32,
    fixed_pitch: bool,
}

impl FontAdvanceMetrics {
    fn from_ascii_widths(measured_space_width: f32, ascii_widths: &[f32; 128]) -> Self {
        let mut total = 0.0;
        let mut count = 0;
        let mut min_width = f32::INFINITY;
        let mut max_width = 0.0f32;

        for width in ascii_widths[32..127].iter().copied() {
            if width.is_finite() && width > 0.0 {
                total += width;
                count += 1;
                min_width = min_width.min(width);
                max_width = max_width.max(width);
            }
        }

        let space_width = if measured_space_width.is_finite() && measured_space_width > 0.0 {
            measured_space_width
        } else {
            ascii_widths[32]
        };
        let average_width = if count > 0 { total / count as f32 } else { 0.0 };
        let min_width = if count > 0 { min_width } else { 0.0 };

        let tolerance = max_width.max(1.0) * 0.02;
        let fixed_pitch = count > 0 && (max_width - min_width).abs() <= tolerance.max(0.25);

        Self {
            space_width,
            average_width,
            max_width,
            fixed_pitch,
        }
    }

    fn from_font_probe(metrics: crate::font::probe::FontPxMetrics) -> Self {
        let space_width = metrics.space_width.max(0) as f32;
        let average_width = metrics.average_width.max(0) as f32;
        let max_width = metrics.max_width.max(0) as f32;
        let tolerance = max_width.max(1.0) * 0.02;
        let fixed_pitch = valid_advance(max_width)
            && valid_advance(average_width)
            && (max_width - average_width).abs() <= tolerance.max(0.25);
        Self {
            space_width,
            average_width,
            max_width,
            fixed_pitch,
        }
    }

    fn monospace_column_width(self, minimum_width: f32) -> Option<FrameColumnWidth> {
        if valid_advance(self.max_width) && self.max_width >= minimum_width {
            return Some(if self.fixed_pitch {
                FrameColumnWidth::validated(self.max_width)
            } else {
                FrameColumnWidth::degraded(self.max_width)
            });
        }
        if valid_advance(self.average_width) && self.average_width >= minimum_width {
            return Some(FrameColumnWidth::validated(self.average_width));
        }
        if valid_advance(self.space_width) && self.space_width >= minimum_width {
            return Some(FrameColumnWidth::validated(self.space_width));
        }
        None
    }

    fn proportional_column_width(self) -> Option<FrameColumnWidth> {
        if valid_advance(self.average_width) {
            return Some(FrameColumnWidth::validated(self.average_width));
        }
        if valid_advance(self.space_width) {
            return Some(FrameColumnWidth::validated(self.space_width));
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct FrameColumnWidth {
    pixels: f32,
    confidence: MetricConfidence,
}

impl FrameColumnWidth {
    fn from_advances(prefer_monospace: bool, font_size: f32, advances: FontAdvanceMetrics) -> Self {
        let fallback = Self::degraded((font_size * 0.6).max(1.0));
        let selected = if prefer_monospace {
            advances.monospace_column_width(font_size * 0.5)
        } else {
            advances.proportional_column_width()
        };

        selected
            .filter(|width| valid_advance(width.pixels))
            .unwrap_or(fallback)
    }

    fn validated(pixels: f32) -> Self {
        Self {
            pixels,
            confidence: MetricConfidence::Validated,
        }
    }

    fn degraded(pixels: f32) -> Self {
        Self {
            pixels,
            confidence: MetricConfidence::Degraded,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FrameCellMetrics {
    column_width: f32,
    line_height: f32,
    ascent: f32,
    descent: f32,
    confidence: MetricConfidence,
}

impl FrameCellMetrics {
    fn derive(
        prefer_monospace: bool,
        font_size: f32,
        vertical: FontVerticalMetrics,
        advances: FontAdvanceMetrics,
    ) -> Self {
        let column = FrameColumnWidth::from_advances(prefer_monospace, font_size, advances);
        Self {
            column_width: column.pixels,
            line_height: vertical.line_height,
            ascent: vertical.ascent,
            descent: vertical.descent,
            confidence: column.confidence,
        }
    }
}

fn derive_observed_frame_cell_metrics(
    prefer_monospace: bool,
    requested_size: f32,
    effective_size: Option<GraphicFontSizePx>,
    vertical: FontVerticalMetrics,
    advances: FontAdvanceMetrics,
) -> FrameCellMetrics {
    FrameCellMetrics::derive(
        prefer_monospace,
        effective_size
            .map(GraphicFontSizePx::get)
            .unwrap_or(requested_size),
        vertical,
        advances,
    )
}

/// One atomic graphic-frame geometry publication.
///
/// The effective opened size travels with the metrics derived from it, so a
/// caller cannot publish retained width/height beside an unrealized requested
/// size.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GraphicFrameCellGeometry {
    pub(crate) font_size: GraphicFontSizePx,
    pub(crate) metrics: FontMetrics,
}

/// Frame cell geometry has two non-overlapping domains in GNU redisplay.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FrameCellGeometry {
    Graphic(GraphicFrameCellGeometry),
    TerminalCell,
}

fn valid_advance(width: f32) -> bool {
    width.is_finite() && width > 0.0
}

fn fontdb_face_file(face: &fontdb::FaceInfo) -> Option<String> {
    match &face.source {
        fontdb::Source::Binary(_) => None,
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
            Some(path.display().to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedFontInfo {
    /// Canonical exact realization shared with layout and the renderer.  Do
    /// not flatten this into file/index fields: native selectors and variable
    /// coordinates are part of the identity too.
    pub resolved: ResolvedFont,
    pub foundry: Option<String>,
    /// Emacs's selector slant is richer than the renderer's three-way slant
    /// (it includes reverse slants), so retain it beside the canonical record.
    pub slant: FontSlant,
    /// Metrics of this exact opened font at the selected pixel size.  Lisp
    /// font objects and layout consume the same realization record instead of
    /// independently reopening the file.
    pub metrics: crate::font::probe::FontPxMetrics,
    /// Driver glyph index from the same selected face.  Keeping it on the
    /// selection answer prevents `font-at` from resolving the character a
    /// second time through a potentially different fallback path.
    pub glyph_code: Option<u32>,
}

/// One shaped glyph produced by [`FontMetricsService::shape_run`]: the
/// resolved font glyph plus its position, advance, and the byte range of
/// the source text it covers (its cluster). This is neomacs's layout-side
/// equivalent of a GNU lglyph (CODE / WIDTH / cluster FROM..TO) — the
/// per-glyph output of running HarfBuzz-class shaping over a text run.
/// It is the building block of the composed glyph rows that contextual
/// scripts (Arabic, Indic) and programming ligatures need.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    /// Resolved font the glyph belongs to (after fallback).
    pub font_id: fontdb::ID,
    /// Glyph index within `font_id`.
    pub glyph_id: u16,
    /// Pen x offset of the glyph within the run, in pixels from the origin.
    pub x: f32,
    /// Pen y offset (baseline-relative), in pixels.
    pub y: f32,
    /// Horizontal advance of the glyph, in pixels.
    pub x_advance: f32,
    /// Start byte index (inclusive) of the source cluster in the shaped text.
    pub cluster_start: usize,
    /// End byte index (exclusive) of the source cluster in the shaped text.
    pub cluster_end: usize,
}

/// Cache key for font metrics lookups.
/// Groups: (family, weight, italic, font_size_centipx)
/// font_size is stored as integer centipixels (size * 100) to avoid float key issues.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct MetricsCacheKey {
    family: String,
    weight: u16,
    italic: bool,
    font_size_centipx: i32,
    device_scale_bits: u32,
    fontset_generation: u64,
}

#[derive(Debug, Clone)]
struct ResolvedCharFont {
    family: String,
    weight: u16,
    slant: FontSlant,
    /// Complete backend answer, when selection crossed the platform seam.
    /// Its identity is authoritative; `family` remains selector metadata.
    platform: Option<crate::font_backend::PlatformFontMatch>,
}

/// One exact, generation-local font selection shared by metadata, layout, and
/// frame publication.
///
/// [`ResolvedFont`] is the durable renderer-facing projection and intentionally
/// carries only the three slants the rasterizer understands. `selector_slant`
/// retains Emacs's richer selector result (including reverse slants) for
/// `font-at`, while `fontdb_id` keeps measurement on the exact local face
/// without looking it up again.
#[derive(Debug, Clone)]
struct LayoutFontHandle {
    font: ResolvedFont,
    /// Which GNU lookup tier produced this handle for the requesting
    /// character. A property of the request, not of the font: GNU keeps one
    /// font object per entity and pixel size (font.c `font_open_entity`
    /// returns the already-open object from `FONT_OBJLIST_INDEX`), whether
    /// fontset.c `face_for_char` returned the ASCII face's font or a fontset
    /// tier found the same font again. It therefore stays off
    /// [`ResolvedFont`], whose id-keyed frame table holds one record per
    /// instance.
    resolution: FontResolutionSource,
    selector_slant: FontSlant,
    source: LayoutFontSource,
    px_metrics: Option<crate::font::probe::FontPxMetrics>,
    /// GNU-compatible per-glyph advances observed at the frame's physical
    /// device size.  Stored separately from logical aggregate metrics so the
    /// coordinate domains cannot be mixed accidentally.
    device_ascii_advances: Option<std::sync::Arc<crate::font::probe::DeviceAsciiAdvances>>,
}

/// How a font request was answered. Distinguishing the tiers keeps traces and
/// the direct-glyph measurement path able to tell a primary-face answer from
/// a fontset fallback that happened to land on the same font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontResolutionSource {
    /// The realized face's primary font (GNU `face->font`).
    FacePrimary,
    /// Chosen via fontset / per-character coverage fallback.
    FontsetFallback,
}

#[derive(Debug, Clone)]
enum LayoutFontSource {
    Swash(fontdb::ID),
    FreeTypeBitmap(neomacs_font_materializer::OpenedFont),
}

fn resolved_font_advance(
    spacing: neomacs_display_protocol::font::FixedFontSpacing,
    metrics: Option<crate::font::probe::FontPxMetrics>,
) -> ResolvedFontAdvance {
    match (spacing, metrics) {
        (
            neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell,
            Some(metrics),
        ) => ResolvedFontAdvance::fixed_cell(metrics.max_width as f32),
        _ => ResolvedFontAdvance::PerGlyph,
    }
}

/// Complete identity of one metrics-bearing protocol font entry.
///
/// A durable source identity alone is insufficient: one file can realize at
/// several sizes or fixed strikes, and [`ResolvedFont`] carries metrics for
/// exactly one of those instances.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ResolvedFontInstanceKey {
    identity: ResolvedFontIdentity,
    replay: FontReplay,
    pixel_size_bits: u32,
}

#[derive(Clone, Copy, Debug)]
enum PlatformFontDbFace {
    /// Re-registered from a decoded/in-memory source; the platform identity
    /// remains authoritative because fontdb cannot reconstruct its path.
    Pinned(fontdb::ID),
    /// Found by exact path/index in the existing font database.
    FileBacked(fontdb::ID),
}

impl PlatformFontDbFace {
    fn id(self) -> fontdb::ID {
        match self {
            Self::Pinned(id) | Self::FileBacked(id) => id,
        }
    }
}

#[cfg(test)]
impl LayoutFontSource {
    fn fontdb_id(&self) -> Option<fontdb::ID> {
        match self {
            Self::Swash(id) => Some(*id),
            Self::FreeTypeBitmap(_) => None,
        }
    }
}

impl MetricsCacheKey {
    fn new(
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
        device_scale: neomacs_display_protocol::geometry::DeviceScale,
    ) -> Self {
        Self::new_at_fontset_generation(
            family,
            weight,
            italic,
            font_size,
            device_scale,
            neovm_core::emacs_core::fontset::fontset_generation(),
        )
    }

    fn new_at_fontset_generation(
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
        device_scale: neomacs_display_protocol::geometry::DeviceScale,
        fontset_generation: u64,
    ) -> Self {
        Self {
            family: family.to_string(),
            weight,
            italic,
            font_size_centipx: (font_size * 100.0) as i32,
            device_scale_bits: device_scale.get().to_bits(),
            fontset_generation,
        }
    }
}

/// Primary family opened for the ASCII half of a realized face.
#[derive(Clone, Copy, Debug)]
pub struct PrimaryFontFamily<'a>(&'a str);

impl<'a> PrimaryFontFamily<'a> {
    pub const fn new(family: &'a str) -> Self {
        Self(family)
    }
}

/// Family from which the realized face's non-ASCII fontset is derived.
#[derive(Clone, Copy, Debug)]
pub struct FontsetBaseFamily<'a>(&'a str);

impl<'a> FontsetBaseFamily<'a> {
    pub const fn new(family: &'a str) -> Self {
        Self(family)
    }
}

/// One frame-realized face/fontset selection context.
///
/// GNU keeps the ASCII face font and its derived fontset as different pieces
/// of realized state. Carrying both in one type prevents character lookup
/// from accidentally treating an inline `:family` as a replacement for the
/// frame's base fontset. The family newtypes make swapping those inputs a
/// compile-time error.
#[derive(Clone, Copy, Debug)]
pub struct RealizedFaceFontSelection<'a> {
    primary_family: &'a str,
    fontset_base_family: &'a str,
    weight: u16,
    italic: bool,
    font_size: f32,
}

impl<'a> RealizedFaceFontSelection<'a> {
    pub fn new(
        primary_family: PrimaryFontFamily<'a>,
        fontset_base_family: FontsetBaseFamily<'a>,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Self {
        Self {
            primary_family: primary_family.0,
            fontset_base_family: fontset_base_family.0,
            weight,
            italic,
            font_size,
        }
    }

    fn same_fontset(family: &'a str, weight: u16, italic: bool, font_size: f32) -> Self {
        Self::new(
            PrimaryFontFamily::new(family),
            FontsetBaseFamily::new(family),
            weight,
            italic,
            font_size,
        )
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RealizedFaceFontCacheKey {
    primary: MetricsCacheKey,
    fontset_base_family: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SymbolFontPolicyKey {
    use_primary_font: bool,
    char_script_table_identity: Option<usize>,
    char_script_table_generation: u64,
}

#[derive(Clone, Debug)]
struct SymbolFontPolicy {
    key: SymbolFontPolicyKey,
    symbol_ranges: Vec<(u32, u32)>,
}

impl Default for SymbolFontPolicy {
    fn default() -> Self {
        Self {
            key: SymbolFontPolicyKey {
                use_primary_font: false,
                char_script_table_identity: None,
                char_script_table_generation: 0,
            },
            symbol_ranges: Vec::new(),
        }
    }
}

impl SymbolFontPolicy {
    fn uses_primary_font_for(&self, ch: char) -> bool {
        if !self.key.use_primary_font {
            return false;
        }
        let codepoint = ch as u32;
        let candidate = self
            .symbol_ranges
            .partition_point(|(_, end)| *end < codepoint);
        self.symbol_ranges
            .get(candidate)
            .is_some_and(|(start, end)| (*start..=*end).contains(&codepoint))
    }
}

/// Whether GNU's live symbol-font inputs changed effective font selection.
///
/// Keeping this distinct from the char-table mutation generation prevents an
/// unrelated char-table write from forcing layout and font-cache invalidation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolFontPolicyUpdate {
    Unchanged,
    Changed,
}

impl SymbolFontPolicyUpdate {
    #[must_use]
    pub const fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

/// Upper bound on `shaped_run_cache` entries before it is cleared, mirroring
/// GNU's bounded composition cache. Shaped runs are typically words or
/// property spans, so a frame's working set stays well under this; clearing on
/// overflow keeps memory bounded without per-entry LRU bookkeeping.
const SHAPED_RUN_CACHE_CAP: usize = 8192;

/// Cosmic-text based font metrics service.
///
/// Runs on the Emacs/layout thread. Shared selection is owned by
/// [`crate::font::resolver::FontResolver`], which asks a
/// [`crate::font_backend::FontBackend`] only for native candidates.
/// `FontSystem` materializes the selected exact file/index for shaping.
pub struct FontMetricsService {
    /// Typed native-catalog generation published with every realized frame.
    /// Only [`Self::synchronize_font_catalog`] advances it.
    font_catalog: crate::font::catalog::FontCatalog,
    font_system: FontSystem,
    /// Exact bitmap-font opener shared with the renderer. Backend handles stay
    /// inside the materializer and never enter the display protocol.
    bitmap_materializer: Option<neomacs_font_materializer::FontMaterializer>,
    /// Scale of the frame currently being laid out. It participates in every
    /// realization cache key because fixed-strike selection happens in device
    /// pixels while all published metrics remain logical.
    device_scale: neomacs_display_protocol::geometry::DeviceScale,
    /// Cache: face attrs → ASCII advance widths (chars 0-127)
    ascii_cache: HashMap<MetricsCacheKey, [f32; 128]>,
    /// Cache: complete realized face/fontset selection → single-char width.
    /// GNU's `use-default-font-for-symbols` branch can select the ASCII
    /// primary before fontset fallback, so both families belong in this key.
    char_cache: HashMap<(RealizedFaceFontCacheKey, char), f32>,
    /// Cache: face attrs → font metrics (ascent, descent, etc.)
    metrics_cache: HashMap<MetricsCacheKey, FontMetricObservation>,
    /// Interned font family strings for cosmic-text Attrs (requires 'static)
    interned_families: HashMap<String, &'static str>,
    /// Cache for pre-loading font files and resolving fontdb family names
    font_file_cache: FontFileCache,
    /// Cache: (face, run text) → shaped glyphs. A run is shaped by BOTH the
    /// measure pass (wrap/cursor advance) and the render pass (glyph
    /// production); this makes the second a cache hit so cosmic-text shapes
    /// each (run, face) once instead of twice. Keyed on the same integer-centipx
    /// face identity as the advance caches, so two runs with identical text but
    /// different faces never share an entry.
    ///
    /// Like the `ascii_cache`/`char_cache`/`metrics_cache`, entries are only
    /// valid for the current fontdb generation: `clear_caches` drops them on a
    /// font change, but `prime_file` (reachable from `resolve_family` /
    /// `font_request_for_char`) can load a font mid-session WITHOUT invalidating
    /// the cache. Production primes a face's file before shaping it, so a stale
    /// entry does not arise in practice; this matches the existing advance
    /// caches' unstated contract. The cached `font_id`/`glyph_id` are likewise
    /// only valid for that generation — production consumers read only
    /// `cluster_start`/`x_advance` (see `DisplayTextRunClusterAdvances`), so a
    /// stale entry degrades to a stale advance, never a wrong rasterized glyph.
    /// Do NOT thread the cached glyph ids into rasterization without adding
    /// fontdb-generation keying (cf. `font_match::resolve_weight_in_family`,
    /// which folds `db().len()` into its key for exactly this reason).
    shaped_run_cache: HashMap<(MetricsCacheKey, String), Vec<ShapedGlyph>>,
    /// Entry cap for `shaped_run_cache` before clear-on-overflow. Defaults to
    /// `SHAPED_RUN_CACHE_CAP`; lowered by tests to exercise the overflow path.
    shaped_run_cache_cap: usize,
    /// Number of actual cosmic-text shaping invocations (`shaped_run_cache`
    /// misses). Lets tests prove the measure/render double-shape is deduped.
    n_shape_calls: usize,
    /// Shared GNU-compatible fontset policy and scoring over the active
    /// platform candidate backend (design §7).
    font_resolver: crate::font::resolver::FontResolver,
    /// Shaping engine behind the TextShaper seam (design §8).
    shaper: Box<dyn crate::text_shaper::TextShaper>,
    /// Cache: face attrs → the face's resolved primary font. Same generation
    /// contract as the other caches: cleared by `clear_caches`.
    resolved_face_font_cache: HashMap<MetricsCacheKey, Option<LayoutFontHandle>>,
    /// Interner: complete realized instance → stable [`ResolvedFontId`]. NOT
    /// cleared by `clear_caches`: ids stay stable for the service's lifetime
    /// so consecutive frame snapshots reference the same font by the same id.
    /// Renderer caches key on the identity anyway, so a stale id can never
    /// alias a glyph to the wrong font.
    resolved_font_ids: HashMap<ResolvedFontInstanceKey, ResolvedFontId>,
    /// The one [`ResolvedFont`] record published under each interned id, so
    /// no later resolution path can publish a second record for an id the
    /// frame table already carries. Dropped on a native catalog advance
    /// (the same boundary on which the renderer drops its font tables and a
    /// file replaced in place gets fresh metrics), never on `clear_caches`.
    /// GNU: `font_clear_cache` closes the objects in `FONT_OBJLIST_INDEX`
    /// while `font_open_entity` otherwise hands back the open object.
    resolved_font_records: HashMap<ResolvedFontId, ResolvedFont>,
    /// Cache: (realized face/fontset selection, char) → the exact selected
    /// font. Same generation contract as the other caches: cleared by
    /// `clear_caches`.
    resolved_char_font_cache: HashMap<(RealizedFaceFontCacheKey, char), Option<LayoutFontHandle>>,
    /// Cache: (face attrs, cluster text) → shaped glyphs with interned font
    /// identities. Same generation contract; clear-on-overflow like
    /// `shaped_run_cache`.
    // A `type` alias for this cache value would not materially aid readability.
    #[allow(clippy::type_complexity)]
    resolved_cluster_cache: HashMap<
        (RealizedFaceFontCacheKey, String),
        Option<(Vec<ResolvedGlyph>, Vec<ResolvedFont>)>,
    >,
    /// Cache: complete face selection request → the synthetic family to use when
    /// fontconfig's chosen file differs from cosmic-text's own pick
    /// (`Some`), or `None` when they agree (the common case, no pinning).
    /// See [`Self::pinned_primary_family`].
    primary_pin_cache: HashMap<MetricsCacheKey, Option<&'static str>>,
    /// Complete platform answers for primary faces.  Keeping the exact match
    /// here prevents later stages from degrading it back into a file-only
    /// request or independently repeating platform selection.
    primary_match_cache: HashMap<MetricsCacheKey, Option<crate::font_backend::PlatformFontMatch>>,
    /// GNU `face_for_char`'s primary-font-before-fontset rule for characters
    /// classified as `symbol`. This owns numeric ranges, never a Lisp object.
    symbol_font_policy: SymbolFontPolicy,
}

/// Whether primary-font pinning is enabled (default on). Pinning routes the
/// primary font through fontconfig's file choice — matching GNU/`find-font`,
/// which prefer a variable font over a same-family static face. Set
/// `NEOMACS_DISABLE_FONT_PIN` to fall back to cosmic-text/fontdb selection.
fn font_pin_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("NEOMACS_DISABLE_FONT_PIN").is_none())
}

impl Default for FontMetricsService {
    fn default() -> Self {
        Self::new()
    }
}

impl FontMetricsService {
    /// Create a new FontMetricsService.
    ///
    /// This scans the system font database, which can take tens of
    /// milliseconds. Should be lazily initialized on first use.
    pub fn new() -> Self {
        // Install native catalog observation before taking the font-system
        // snapshot. A change racing after this point is then visible through
        // the backend's per-service cursor and triggers a safe-point rebuild.
        let font_resolver = crate::font::resolver::FontResolver::platform_default();
        tracing::info!("FontMetricsService: initializing cosmic-text FontSystem");
        let font_system = FontSystem::new();
        tracing::info!("FontMetricsService: FontSystem ready");
        Self {
            font_catalog: crate::font::catalog::FontCatalog::default(),
            font_system,
            bitmap_materializer: neomacs_font_materializer::FontMaterializer::new().ok(),
            device_scale: neomacs_display_protocol::geometry::DeviceScale::new(1.0)
                .expect("one is a valid device scale"),
            ascii_cache: HashMap::default(),
            char_cache: HashMap::default(),
            metrics_cache: HashMap::default(),
            interned_families: HashMap::default(),
            font_file_cache: FontFileCache::new(),
            shaped_run_cache: HashMap::default(),
            shaped_run_cache_cap: SHAPED_RUN_CACHE_CAP,
            n_shape_calls: 0,
            font_resolver,
            shaper: crate::text_shaper::default_text_shaper(),
            resolved_face_font_cache: HashMap::default(),
            resolved_font_ids: HashMap::default(),
            resolved_font_records: HashMap::default(),
            resolved_char_font_cache: HashMap::default(),
            resolved_cluster_cache: HashMap::default(),
            primary_pin_cache: HashMap::default(),
            primary_match_cache: HashMap::default(),
            symbol_font_policy: SymbolFontPolicy::default(),
        }
    }

    pub fn set_device_scale(
        &mut self,
        device_scale: neomacs_display_protocol::geometry::DeviceScale,
    ) {
        self.device_scale = device_scale;
    }

    #[must_use]
    pub const fn font_catalog_generation(
        &self,
    ) -> neomacs_display_protocol::font::FontCatalogGeneration {
        self.font_catalog.generation()
    }

    /// Apply one coalesced native catalog edge at an evaluator-thread safe
    /// point. Rebuilding `FontSystem` is intentional: fontdb only appends when
    /// asked to load system fonts, which would keep removed/replaced files and
    /// stale generation-local ids alive indefinitely.
    #[must_use]
    pub fn synchronize_font_catalog(&mut self) -> crate::font::catalog::FontCatalogUpdate {
        let change = self.font_resolver.poll_catalog_change();
        let update = self.font_catalog.observe(change);
        if let crate::font::catalog::FontCatalogUpdate::Advanced { previous, current } = update {
            tracing::info!(
                target: "font_catalog",
                previous = previous.get(),
                current = current.get(),
                "rebuilding layout font state for native catalog change"
            );
            self.font_system = FontSystem::new();
            self.font_file_cache = FontFileCache::new();
            self.clear_caches();
            // Records carry metrics read from the files; a replaced file must
            // republish under its stable id, as the renderer rebuilds its
            // tables for this generation.
            self.resolved_font_records.clear();
        }
        update
    }

    /// Synchronize GNU's `use-default-font-for-symbols` selection input.
    ///
    /// The identity/generation check keeps steady-state redisplay O(1). A real
    /// policy change replaces the live Lisp table with owned ranges and drops
    /// all character-selection caches before reuse.
    #[must_use]
    pub fn synchronize_symbol_font_policy(
        &mut self,
        use_primary_font: bool,
        char_script_table: Option<neovm_core::emacs_core::Value>,
    ) -> SymbolFontPolicyUpdate {
        let key = SymbolFontPolicyKey {
            use_primary_font,
            char_script_table_identity: char_script_table.map(|table| table.bits()),
            char_script_table_generation:
                neovm_core::emacs_core::fontset::char_script_table_generation(),
        };
        if self.symbol_font_policy.key == key {
            return SymbolFontPolicyUpdate::Unchanged;
        }
        let symbol_ranges = use_primary_font
            .then(|| {
                neovm_core::emacs_core::fontset::symbol_script_ranges(char_script_table.as_ref())
            })
            .unwrap_or_default();
        let selection_changed = self.symbol_font_policy.key.use_primary_font != use_primary_font
            || self.symbol_font_policy.symbol_ranges != symbol_ranges;
        self.symbol_font_policy = SymbolFontPolicy { key, symbol_ranges };
        if !selection_changed {
            return SymbolFontPolicyUpdate::Unchanged;
        }
        self.clear_caches();
        SymbolFontPolicyUpdate::Changed
    }

    /// Enumerate families through the same native backend that owns layout
    /// font selection on this platform.
    pub fn list_font_families(&self) -> Vec<crate::font_backend::FontFamilyName> {
        self.font_resolver.list_families()
    }

    pub fn resolve_font_entity(
        &self,
        query: &crate::font::resolver::FontEntityQuery,
    ) -> Option<crate::font::resolver::ResolvedFontEntity> {
        self.font_resolver.resolve_entity(query)
    }

    pub fn open_font_entity(
        &self,
        query: &crate::font::resolver::FontEntityQuery,
        pixel_size: u32,
    ) -> Option<crate::font::resolver::OpenedFontEntity> {
        self.font_resolver.open_entity(query, pixel_size)
    }

    #[must_use]
    pub(crate) const fn device_scale(&self) -> neomacs_display_protocol::geometry::DeviceScale {
        self.device_scale
    }

    fn cache_key(
        &self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> MetricsCacheKey {
        MetricsCacheKey::new(family, weight, italic, font_size, self.device_scale)
    }

    fn realized_face_font_cache_key(
        &self,
        selection: RealizedFaceFontSelection<'_>,
    ) -> RealizedFaceFontCacheKey {
        RealizedFaceFontCacheKey {
            primary: self.cache_key(
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            ),
            fontset_base_family: selection.fontset_base_family.to_owned(),
        }
    }

    fn selection_size(&self, font_size: f32) -> crate::font_backend::FontSelectionSize {
        crate::font_backend::FontSelectionSize::new(font_size, self.device_scale)
    }

    /// Turn a fontdb face into the same durable identity vocabulary used by
    /// the active platform backend. This matters when shaping itself chooses
    /// a fallback: the renderer must not receive a misleading Fontconfig
    /// identity on CoreText or DirectWrite builds.
    fn fontdb_face_identity(
        &self,
        file: Option<&str>,
        face_index: u32,
        postscript_name: Option<String>,
        fallback_name: &str,
    ) -> ResolvedFontIdentity {
        let backend = self.font_resolver.backend_kind();
        match file {
            Some(path) if backend == FontBackendKind::Fontconfig => {
                ResolvedFontIdentity::from_file(path, face_index, postscript_name)
            }
            Some(path) => ResolvedFontIdentity::from_platform_file_with_variations(
                backend,
                path,
                face_index,
                postscript_name,
                Vec::new(),
            ),
            None => ResolvedFontIdentity::from_memory(
                backend,
                format!(
                    "mem:{}#{face_index}",
                    postscript_name.as_deref().unwrap_or(fallback_name)
                ),
                face_index,
                postscript_name,
            ),
        }
    }

    /// Register the exact font FILE (a specific face index of it) under a
    /// unique synthetic fontdb family, so `Attrs::family(Family::Name(..))`
    /// selects THAT face verbatim instead of cosmic-text re-picking among
    /// every face that shares the real family name.
    ///
    /// This is how we pin the file fontconfig chose: GNU/fontconfig prefer a
    /// variable font's named instance over a same-family static face, but
    /// cosmic-text/fontdb would pick the static exact-weight face. We load
    /// the file, clone the target face's metadata under a synthetic family,
    /// and drop the freshly-loaded originals so the real family name is not
    /// duplicated. Cached per `(file, index)`; returns the interned
    /// synthetic family name, or `None` if the file can't be loaded.
    fn pin_file_as_family(&mut self, file: &str, face_index: u32) -> Option<&'static str> {
        self.font_file_cache
            .pin_exact_face(&mut self.font_system, file, face_index)
            .ok()
            .map(neomacs_font_materializer::PinnedFontFace::family)
    }

    fn pin_outline_as_family(&mut self, asset: &FontOutlineAsset) -> Option<&'static str> {
        self.font_file_cache
            .pin_exact_asset(&mut self.font_system, asset)
            .ok()
            .map(neomacs_font_materializer::PinnedFontFace::family)
    }

    /// Resolve the effective font family name for a face.
    ///
    /// If `font_file_path` is provided, pre-loads the exact font file into fontdb
    /// while preserving the exact family name that Fontconfig selected.
    pub fn resolve_family(&mut self, emacs_family: &str, font_file_path: Option<&str>) -> String {
        if let Some(path) = font_file_path {
            let _ = self.font_file_cache.prime_file(&mut self.font_system, path);
        }
        emacs_family.to_string()
    }

    fn intern_family(&mut self, family: &str) -> &'static str {
        if let Some(&existing) = self.interned_families.get(family) {
            existing
        } else {
            let leaked: &'static str = Box::leak(family.to_string().into_boxed_str());
            self.interned_families.insert(family.to_string(), leaked);
            leaked
        }
    }

    /// Build cosmic-text `Attrs` from face parameters.
    /// Mirrors the logic in `glyph_atlas.rs:face_to_attrs()`.
    ///
    /// When fontconfig's authoritative file for this (family, weight, slant)
    /// differs from what cosmic-text/fontdb would otherwise pick, pin
    /// fontconfig's file under a synthetic family so shaping and metrics use
    /// the same primary font GNU opens. In the common case (they agree) this
    /// is byte-identical to the plain path.
    fn build_attrs(
        &mut self,
        family: &str,
        weight: u16,
        slant: FontSlant,
        font_size: f32,
    ) -> Attrs<'static> {
        if let Some(synthetic) =
            self.pinned_primary_family(family, weight, slant.is_italic(), font_size)
        {
            let effective_weight = crate::font::font_match::resolve_weight_in_family(
                &self.font_system,
                synthetic,
                weight,
                slant.is_italic(),
            );
            let mut attrs = Attrs::new()
                .family(Family::Name(synthetic))
                .weight(Weight(effective_weight));
            if let Some(style) = font_slant_to_cosmic_style(slant) {
                attrs = attrs.style(style);
            }
            return attrs;
        }
        self.build_attrs_unpinned(family, weight, slant)
    }

    fn build_attrs_unpinned(
        &mut self,
        family: &str,
        weight: u16,
        slant: FontSlant,
    ) -> Attrs<'static> {
        let mut attrs = Attrs::new();

        attrs = match crate::font::font_match::select_cosmic_family(&self.font_system, family) {
            crate::font::font_match::CosmicFamilySelection::Name(family) => {
                let interned = self.intern_family(family);
                attrs.family(Family::Name(interned))
            }
            crate::font::font_match::CosmicFamilySelection::Monospace => {
                attrs.family(Family::Monospace)
            }
            crate::font::font_match::CosmicFamilySelection::Serif => attrs.family(Family::Serif),
            crate::font::font_match::CosmicFamilySelection::SansSerif => {
                attrs.family(Family::SansSerif)
            }
        };

        // Font weight (CSS 100-900): clamp to closest available in this family.
        let effective_weight = crate::font::font_match::resolve_weight_in_family(
            &self.font_system,
            family,
            weight,
            slant.is_italic(),
        );
        attrs = attrs.weight(Weight(effective_weight));

        // Font style
        if let Some(style) = font_slant_to_cosmic_style(slant) {
            attrs = attrs.style(style)
        }

        attrs
    }

    /// Build attributes for an already-resolved character font. Platform
    /// identity wins over semantic family metadata: if the backend supplied a
    /// file face, pin that exact collection face and replay the resolved
    /// weight/style for its named instance.
    fn build_attrs_for_resolved_char(
        &mut self,
        resolved: &ResolvedCharFont,
        font_size: f32,
    ) -> Option<Attrs<'static>> {
        if let Some(platform) = resolved.platform.as_ref() {
            let synthetic = self.pin_outline_as_family(&platform.asset)?;
            let mut attrs = Attrs::new()
                .family(Family::Name(synthetic))
                .weight(Weight(resolved.weight));
            if let Some(style) = font_slant_to_cosmic_style(resolved.slant) {
                attrs = attrs.style(style);
            }
            return Some(attrs);
        }
        Some(self.build_attrs(&resolved.family, resolved.weight, resolved.slant, font_size))
    }

    /// Replay a materialized selection without semantic font matching.
    fn build_attrs_for_materialized_font(
        &mut self,
        materialized: &LayoutFontHandle,
    ) -> Option<Attrs<'static>> {
        if matches!(materialized.source, LayoutFontSource::FreeTypeBitmap(_)) {
            return None;
        }
        if let Some(asset) = materialized.font.replay.outline_asset() {
            let synthetic = self.pin_outline_as_family(asset)?;
            let mut attrs = Attrs::new()
                .family(Family::Name(synthetic))
                .weight(Weight(materialized.font.weight));
            if let Some(style) = font_slant_to_cosmic_style(materialized.selector_slant) {
                attrs = attrs.style(style);
            }
            return Some(attrs);
        }
        Some(self.build_attrs(
            &materialized.font.family,
            materialized.font.weight,
            materialized.selector_slant,
            materialized.font.pixel_size,
        ))
    }

    fn probe_resolved_font_metrics(
        identity: &ResolvedFontIdentity,
        platform: Option<&crate::font_backend::PlatformFontMatch>,
        font_size: f32,
    ) -> Option<crate::font::probe::FontPxMetrics> {
        if let Some(metrics) = platform.and_then(|matched| matched.pixel_metrics(font_size)) {
            return Some(metrics);
        }
        let file = identity.file_path.as_deref()?;
        let explicit_weight = identity
            .variation_coords
            .iter()
            .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
            .map(|coord| coord.value());
        crate::font::probe::probe_font_px_metrics(
            file,
            identity.freetype_selector()?,
            font_size.round().max(1.0) as u32,
            explicit_weight,
        )
    }

    fn probe_resolved_font_device_ascii_advances(
        &self,
        identity: &ResolvedFontIdentity,
        font_size: f32,
    ) -> Option<std::sync::Arc<crate::font::probe::DeviceAsciiAdvances>> {
        let file = identity.file_path.as_deref()?;
        let explicit_weight = identity
            .variation_coords
            .iter()
            .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
            .map(|coord| coord.value());
        let device_pixel_size = self.selection_size(font_size).rounded_device_px();
        crate::font::probe::probe_device_ascii_advances(
            file,
            identity.freetype_selector()?,
            device_pixel_size,
            explicit_weight,
        )
        .map(std::sync::Arc::new)
    }

    fn materialized_font_has_char(&mut self, materialized: &LayoutFontHandle, ch: char) -> bool {
        match &materialized.source {
            LayoutFontSource::Swash(fontdb_id) => self
                .font_system
                .get_font(*fontdb_id, fontdb::Weight(materialized.font.weight))
                .is_some_and(|font| font.as_swash().charmap().map(ch) != 0),
            LayoutFontSource::FreeTypeBitmap(font) => font.glyph_for_char(ch).is_some(),
        }
    }

    /// The synthetic family to shape a primary (family, weight, italic)
    /// request through when fontconfig's file differs from cosmic-text's own
    /// pick, else `None` (agree → no pinning, unchanged behavior). Cached.
    fn pinned_primary_family(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<&'static str> {
        if !font_pin_enabled() {
            return None;
        }
        let key = self.cache_key(family, weight, italic, font_size);
        if let Some(&cached) = self.primary_pin_cache.get(&key) {
            return cached;
        }
        let result = self.compute_primary_pin(family, weight, italic, font_size);
        self.primary_pin_cache.insert(key, result);
        result
    }

    fn platform_primary_match(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<crate::font_backend::PlatformFontMatch> {
        let key = self.cache_key(family, weight, italic, font_size);
        if let Some(cached) = self.primary_match_cache.get(&key) {
            return cached.clone();
        }
        let requested_slant = if italic {
            FontSlant::Italic
        } else {
            FontSlant::Normal
        };
        let matched = self
            .font_resolver
            .resolve_primary(
                family,
                weight,
                requested_slant,
                FontWidth::Normal,
                self.selection_size(font_size),
            )
            .and_then(|matched| self.materialize_platform_match(matched));
        self.primary_match_cache.insert(key, matched.clone());
        matched
    }

    /// Confirm that one of the shared materializers can open a platform match
    /// before treating its identity as authoritative. Fixed bitmap faces are
    /// classified first so they never enter fontdb merely to produce an
    /// expected parse failure; outline/webfont failures remain actionable.
    fn materialize_platform_match(
        &mut self,
        matched: crate::font_backend::PlatformFontMatch,
    ) -> Option<crate::font_backend::PlatformFontMatch> {
        let weight_tag = u32::from_be_bytes(*b"wght");
        if matched
            .identity
            .variation_coords
            .iter()
            .any(|coord| coord.tag() != weight_tag)
        {
            tracing::warn!(
                target: "font_boundary",
                identity = %matched.identity.stable_key,
                "platform font uses variation axes not yet replayable by cosmic-text; using resolved fallback"
            );
            return None;
        }
        match matched.metadata.size {
            crate::font_backend::PlatformFontSize::Fixed { .. } => {
                if matched.asset.file().is_some() {
                    return Some(matched);
                }
                tracing::warn!(
                    target: "font_boundary",
                    identity = %matched.identity.stable_key,
                    "native-memory font unexpectedly advertised a fixed bitmap strike"
                );
                return None;
            }
            crate::font_backend::PlatformFontSize::Unknown => {
                tracing::warn!(
                    target: "font_boundary",
                    identity = %matched.identity.stable_key,
                    "platform font reached materialization with an unclassified size"
                );
                return None;
            }
            crate::font_backend::PlatformFontSize::Scalable => {}
        }
        if self.pin_outline_as_family(&matched.asset).is_none() {
            tracing::warn!(
                target: "font_boundary",
                identity = %matched.identity.stable_key,
                "no exact-font materializer accepted the platform font; using resolved fallback"
            );
            return None;
        }
        Some(matched)
    }

    fn open_bitmap_font(
        &self,
        matched: &crate::font_backend::PlatformFontMatch,
        font_size: f32,
    ) -> Result<
        neomacs_font_materializer::OpenedFont,
        neomacs_font_materializer::FontMaterializationError,
    > {
        self.bitmap_materializer
            .as_ref()
            .ok_or(neomacs_font_materializer::FontMaterializationError::BackendUnavailable)?
            .open(neomacs_font_materializer::FontOpenRequest {
                asset: matched.asset.file().ok_or(
                    neomacs_font_materializer::FontMaterializationError::ReplayMethodMismatch,
                )?,
                requested_layout_px: font_size,
                device_scale: self.device_scale,
                selected_device_ppem_26_6: matched.metadata.size.selected_device_ppem_26_6(),
                line_height: neomacs_font_materializer::BitmapLineHeightPolicy::GnuDefault,
                spacing: matched.metadata.fixed_spacing_policy(),
            })
    }

    /// Return this layout service's generation-local fontdb id for an exact
    /// platform match. The durable file/index identity is authoritative; no
    /// character is shaped here because a primary face may intentionally be
    /// symbols-only and contain neither ASCII nor space.
    fn fontdb_face_for_platform_match(
        &self,
        matched: &crate::font_backend::PlatformFontMatch,
    ) -> Option<PlatformFontDbFace> {
        if let Some(pinned) = self.font_file_cache.pinned_exact_asset(&matched.asset) {
            return Some(PlatformFontDbFace::Pinned(pinned.fontdb_id()));
        }
        let file = matched.asset.file()?;
        let path = file.path();
        let face_index = file.face_index();
        self.font_system.db().faces().find_map(|face| {
            let source_path = match &face.source {
                fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => path,
                fontdb::Source::Binary(_) => return None,
            };
            (face.index == face_index && source_path.as_os_str() == std::ffi::OsStr::new(path))
                .then_some(PlatformFontDbFace::FileBacked(face.id))
        })
    }

    fn compute_primary_pin(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<&'static str> {
        let platform = self.platform_primary_match(family, weight, italic, font_size)?;
        let platform_file = platform.file_path()?.to_string();
        let platform_index = platform.identity.file_face_index();
        // What file would cosmic-text/fontdb pick on its own?
        let cosmic = self.cosmic_primary_probe(family, weight, italic)?;
        // GNU's platform font backend is authoritative for the complete
        // primary identity.  A matching path is insufficient: named variable
        // instances and collection faces commonly share one file.
        if cosmic.file == platform_file && cosmic.face_index == platform_index {
            return None;
        }
        tracing::debug!(
            target: "font_boundary",
            family,
            weight,
            italic,
            platform_file = %platform_file,
            platform_index,
            cosmic_file = %cosmic.file,
            cosmic_index = cosmic.face_index,
            "primary-font pin: platform and fontdb disagree; pinning exact platform face"
        );
        self.pin_file_as_family(&platform_file, platform_index)
    }

    /// The font file cosmic-text/fontdb selects on its own for this request
    /// (probe by shaping a representative ASCII glyph, unpinned).
    #[cfg(test)]
    fn cosmic_probe_file(&mut self, family: &str, weight: u16, italic: bool) -> Option<String> {
        self.cosmic_primary_probe(family, weight, italic)
            .map(|probe| probe.file)
    }

    fn cosmic_primary_probe(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
    ) -> Option<CosmicPrimaryProbe> {
        let slant = if italic {
            FontSlant::Italic
        } else {
            FontSlant::Normal
        };
        let attrs = self.build_attrs_unpinned(family, weight, slant);
        let metrics = safe_metrics(24.0, 24.0 * 1.3);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(96.0), Some(48.0));
        buffer.set_text(
            &mut self.font_system,
            "n",
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        let font_id = buffer
            .layout_runs()
            .find_map(|run| run.glyphs.iter().next())?
            .physical((0.0, 0.0), 1.0)
            .cache_key
            .font_id;
        let face = self.font_system.db().face(font_id)?;
        Some(CosmicPrimaryProbe {
            file: fontdb_face_file(face)?,
            face_index: face.index,
        })
    }

    fn selected_font_id_and_space_width(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> (Option<fontdb::ID>, f32) {
        let attrs = self.build_attrs(
            family,
            weight,
            if italic {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
            font_size,
        );
        let metrics = safe_metrics(font_size, font_size * 1.3);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            Some(font_size * 4.0),
            Some(font_size * 2.0),
        );
        buffer.set_text(
            &mut self.font_system,
            " ",
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        for run in buffer.layout_runs() {
            if let Some(glyph) = run.glyphs.first() {
                return (
                    Some(glyph.physical((0.0, 0.0), 1.0).cache_key.font_id),
                    glyph.w,
                );
            }
        }

        (None, font_size * 0.6)
    }

    /// Shape a run of `text` with the given face attributes and return its
    /// glyphs in visual order, with positions, advances, and source-cluster
    /// byte ranges.
    ///
    /// This is the layout-side counterpart of GNU's `font-shape-gstring` and
    /// the font driver's `->shape` method: it runs cosmic-text's
    /// `Shaping::Advanced` (HarfBuzz-class) so contextual scripts (Arabic
    /// joining, Indic reordering) and ligatures resolve correctly across the
    /// whole run rather than per character. The cluster byte ranges map each
    /// glyph back to the characters it covers, which a composed glyph row
    /// needs for cursor positioning. Returns an empty vec for empty text.
    pub fn shape_run(
        &mut self,
        text: &str,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }
        let metrics_key = self.cache_key(family, weight, italic, font_size);
        let attrs = self.build_attrs(
            family,
            weight,
            if italic {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
            font_size,
        );
        self.shape_run_with_attrs(text, metrics_key, attrs, font_size)
    }

    /// Shape a run through the fontset attached to one realized face.
    ///
    /// The face's primary family owns ASCII.  A non-ASCII representative
    /// character is resolved from the face's base fontset first, and the
    /// resulting exact platform font is then used for the whole cluster. This
    /// is the shaping counterpart of [`Self::char_width_for_realized_face`];
    /// keeping both behind the same typed selection prevents measurement and
    /// finished-frame font publication from choosing different fonts.
    pub fn shape_run_for_realized_face(
        &mut self,
        text: &str,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Vec<ShapedGlyph> {
        if text.is_empty() {
            return Vec::new();
        }

        let Some(representative) = crate::composition::representative_char_for_cluster(text) else {
            return self.shape_run(
                text,
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            );
        };
        let Some(materialized) =
            self.materialized_font_for_realized_face_char(representative, selection)
        else {
            return self.shape_run(
                text,
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            );
        };
        let Some(attrs) = self.build_attrs_for_materialized_font(&materialized) else {
            // Bitmap fonts cannot enter the outline shaper. The caller's
            // composition path handles simple bitmap copies separately; for
            // a complex run, retain the explicit primary-font fallback.
            return self.shape_run(
                text,
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            );
        };
        let key = MetricsCacheKey::new(
            &materialized.font.identity.stable_key,
            materialized.font.weight,
            materialized.selector_slant.is_italic(),
            selection.font_size,
            self.device_scale,
        );
        self.shape_run_with_attrs(text, key, attrs, selection.font_size)
    }

    fn shape_run_with_attrs(
        &mut self,
        text: &str,
        metrics_key: MetricsCacheKey,
        attrs: Attrs<'static>,
        font_size: f32,
    ) -> Vec<ShapedGlyph> {
        let key = (metrics_key, text.to_string());
        if let Some(cached) = self.shaped_run_cache.get(&key) {
            return cached.clone();
        }
        self.n_shape_calls += 1;
        let glyphs = self.shaper.shape_run(
            &mut self.font_system,
            text,
            &attrs,
            font_size.max(1.0),
            font_size.max(1.0) * 1.3,
        );
        if self.shaped_run_cache.len() >= self.shaped_run_cache_cap {
            self.shaped_run_cache.clear();
        }
        self.shaped_run_cache.insert(key, glyphs.clone());
        glyphs
    }

    fn font_metrics_from_selected_face(
        &mut self,
        font_id: fontdb::ID,
        font_size: f32,
    ) -> Option<FontVerticalMetrics> {
        self.observe_selected_face_vertical_metrics(font_id, font_size)
            .map(|observation| observation.metrics)
    }

    fn observe_selected_face_vertical_metrics(
        &mut self,
        font_id: fontdb::ID,
        font_size: f32,
    ) -> Option<FontVerticalObservation> {
        let probe_target = self
            .font_system
            .db()
            .face(font_id)
            .and_then(|face| fontdb_face_file(face).map(|file| (file, face.index)));
        if let Some((file, face_index)) = probe_target {
            let pixel_size = font_size.round().max(1.0) as u32;
            if let Some(metrics) =
                crate::font::probe::probe_font_px_metrics(&file, face_index, pixel_size, None)
            {
                return Some(FontVerticalObservation {
                    metrics: FontVerticalMetrics {
                        ascent: metrics.ascent.max(0) as f32,
                        descent: metrics.descent.max(0) as f32,
                        line_height: metrics.height.max(1) as f32,
                    },
                    advances: Some(FontAdvanceMetrics::from_font_probe(metrics)),
                    effective_size: GraphicFontSizePx::new(metrics.pixel_size as f32),
                    source: FontMetricSource::SelectedFontProbe,
                });
            }
        }

        self.font_system
            .db()
            .with_face_data(font_id, |font_data, face_index| {
                let face = TtfFace::parse(font_data, face_index).ok()?;
                let units_per_em = face.units_per_em().max(1) as f32;
                let scale = font_size / units_per_em;
                // GNU GUI backends publish frame line height as the font
                // backend's integer ascent plus integer descent.  Do the
                // same here instead of trusting the typographic height table
                // or a synthetic multiplier.
                let ascent = (face.ascender() as f32 * scale).ceil().max(0.0);
                let descent = (-(face.descender() as f32) * scale).ceil().max(0.0);
                let line_height = (ascent + descent).max(1.0);

                // GNU xdisp.c prefers font-global metrics (FONT_BASE /
                // FONT_DESCENT) and only falls back to per-glyph extents for
                // pathological fonts. Reject obviously bogus table data here
                // and let the caller fall back to glyph-box probing.
                if !ascent.is_finite()
                    || !descent.is_finite()
                    || !line_height.is_finite()
                    || ascent <= 0.0
                    || descent <= 0.0
                    || line_height <= 0.0
                    || line_height > font_size * 4.0
                {
                    return None;
                }

                Some(FontVerticalObservation {
                    metrics: FontVerticalMetrics {
                        ascent,
                        descent,
                        line_height,
                    },
                    advances: None,
                    effective_size: GraphicFontSizePx::new(font_size),
                    source: FontMetricSource::SelectedFontTables,
                })
            })
            .flatten()
    }

    /// Derive a complete metric record from the already selected fontdb face.
    ///
    /// This is the portable fallback when the native/file probe is unavailable
    /// (notably memory-backed fonts and Windows).  It deliberately accepts a
    /// `fontdb::ID`, not a family selector: an opened-font query must never
    /// choose a second same-family face while trying to recover metrics.
    fn font_px_metrics_from_selected_face(
        &self,
        font_id: fontdb::ID,
        font_size: f32,
        variations: &[neomacs_display_protocol::font::FontVariationCoord],
    ) -> Option<crate::font::probe::FontPxMetrics> {
        self.font_system
            .db()
            .with_face_data(font_id, |font_data, face_index| {
                let mut face = TtfFace::parse(font_data, face_index).ok()?;
                for variation in variations {
                    let tag = variation.tag().to_be_bytes();
                    let _ =
                        face.set_variation(ttf_parser::Tag::from_bytes(&tag), variation.value());
                }

                let pixel_size = font_size.round().max(1.0) as u32;
                let units_per_em = face.units_per_em().max(1) as f32;
                let scale = pixel_size as f32 / units_per_em;
                let ascent = (face.ascender() as f32 * scale).ceil().max(0.0) as i32;
                let descent = (-(face.descender() as f32) * scale).ceil().max(0.0) as i32;
                let height = ascent + descent;

                let mut max_width = 0i32;
                let mut space_width = 0i32;
                let mut average_width = 0i64;
                let mut count = 0i64;
                for byte in 32u8..127 {
                    let glyph = face
                        .glyph_index(char::from(byte))
                        .unwrap_or(ttf_parser::GlyphId(0));
                    let width = face
                        .glyph_hor_advance(glyph)
                        .map(|advance| (advance as f32 * scale).round().max(0.0) as i32)
                        .unwrap_or(0);
                    if width <= 0 {
                        continue;
                    }
                    max_width = max_width.max(width);
                    if byte == b' ' {
                        space_width = width;
                    }
                    average_width += i64::from(width);
                    count += 1;
                }
                if count == 0 || height <= 0 {
                    return None;
                }

                Some(crate::font::probe::FontPxMetrics {
                    pixel_size,
                    height,
                    ascent,
                    descent,
                    max_width,
                    space_width,
                    average_width: (average_width / count) as i32,
                })
            })
            .flatten()
    }

    pub fn select_font_for_char(
        &mut self,
        ch: char,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<SelectedFontInfo> {
        self.select_font_for_realized_face_char(
            ch,
            RealizedFaceFontSelection::same_fontset(family, weight, italic, font_size),
        )
    }

    pub fn select_font_for_realized_face_char(
        &mut self,
        ch: char,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<SelectedFontInfo> {
        let materialized = self.materialized_font_for_realized_face_char(ch, selection)?;
        let resolved = materialized.font;
        let metrics = materialized.px_metrics?;
        let glyph_code = match &materialized.source {
            LayoutFontSource::Swash(fontdb_id) => self
                .font_system
                .db()
                .with_face_data(*fontdb_id, |font_data, face_index| {
                    TtfFace::parse(font_data, face_index)
                        .ok()?
                        .glyph_index(ch)
                        .map(|glyph| u32::from(glyph.0))
                })
                .flatten(),
            LayoutFontSource::FreeTypeBitmap(font) => {
                font.glyph_for_char(ch).map(|glyph| glyph.get())
            }
        };
        Some(SelectedFontInfo {
            foundry: resolved
                .identity
                .file_path
                .as_deref()
                .and_then(platform_foundry_for_file),
            resolved,
            slant: materialized.selector_slant,
            metrics,
            glyph_code,
        })
    }

    /// Resolve a face's primary font to an exact identity.
    ///
    /// This is the face-level half of the render-boundary design: the
    /// platform backend's exact materialized file/index identifies the
    /// primary font independently of character coverage. GNU analog:
    /// `font_open_for_lface` filling the realized `face->font` that both
    /// `font-at` and the draw path consume.
    pub fn resolved_font_for_face(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<ResolvedFont> {
        self.materialized_font_for_face(family, weight, italic, font_size)
            .map(|materialized| materialized.font)
    }

    fn materialized_font_for_face(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<LayoutFontHandle> {
        let key = self.cache_key(family, weight, italic, font_size);
        if let Some(cached) = self.resolved_face_font_cache.get(&key) {
            return cached.clone();
        }
        let materialized = self.materialize_face_font(family, weight, italic, font_size);
        self.resolved_face_font_cache
            .insert(key, materialized.clone());
        materialized
    }

    fn materialize_face_font(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<LayoutFontHandle> {
        // The platform-selected file/index is the primary face identity.
        // Only the semantic fallback path needs a representative-glyph probe.
        let resolved_family = self.resolve_family(&self.font_resolver.resolve_family(family), None);
        let platform = self.platform_primary_match(&resolved_family, weight, italic, font_size);
        let spacing = platform
            .as_ref()
            .map(|matched| matched.metadata.fixed_spacing_policy())
            .unwrap_or_else(|| {
                if self
                    .font_resolver
                    .family_prefers_monospace(&resolved_family)
                {
                    neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell
                } else {
                    neomacs_display_protocol::font::FixedFontSpacing::ProportionalOrDual
                }
            });
        let platform_px_metrics = platform
            .as_ref()
            .and_then(|matched| matched.pixel_metrics(font_size));
        if let Some(matched) = platform.as_ref()
            && matched.metadata.size.is_fixed()
        {
            return self.materialize_bitmap_font(
                matched,
                &resolved_family,
                weight,
                font_size,
                FontResolutionSource::FacePrimary,
            );
        }
        let platform_fontdb_face = match platform.as_ref() {
            Some(matched) => Some(self.fontdb_face_for_platform_match(matched)?),
            None => None,
        };
        let font_id = match platform_fontdb_face {
            Some(face) => face.id(),
            None => {
                self.selected_font_id_and_space_width(&resolved_family, weight, italic, font_size)
                    .0?
            }
        };
        let effective_weight = crate::font::font_match::resolve_weight_in_family(
            &self.font_system,
            &resolved_family,
            weight,
            italic,
        );
        let (file, face_index, postscript_name, style, stretch) = {
            let face = self.font_system.db().face(font_id)?;
            (
                fontdb_face_file(face),
                face.index,
                Some(face.post_script_name.clone()).filter(|name| !name.is_empty()),
                face.style,
                face.stretch,
            )
        };
        // fontdb Source::Binary faces have no path; key on the postscript
        // name (or family) so the identity is still durable.
        let selected_identity = self.fontdb_face_identity(
            file.as_deref(),
            face_index,
            postscript_name.clone(),
            &resolved_family,
        );
        let selected_asset = file
            .as_deref()
            .and_then(|path| FontFileAsset::new(path, face_index).map(FontOutlineAsset::File));
        let (identity, asset, postscript_name, resolved_weight, selector_slant, render_slant) =
            match platform {
                Some(platform) => {
                    if matches!(
                        platform_fontdb_face,
                        Some(PlatformFontDbFace::FileBacked(_))
                    ) && (selected_identity.file_path != platform.identity.file_path
                        || selected_identity.file_face_index()
                            != platform.identity.file_face_index())
                    {
                        tracing::error!(
                            target: "font_boundary",
                            family = %resolved_family,
                            weight,
                            italic,
                            selected = %selected_identity.stable_key,
                            platform = %platform.identity.stable_key,
                            "exact platform font could not be selected for layout"
                        );
                        return None;
                    }
                    let postscript_name = platform
                        .identity
                        .postscript_name
                        .clone()
                        .or(postscript_name);
                    let resolved_weight = platform.weight().unwrap_or(effective_weight);
                    let selector_slant = platform.slant();
                    (
                        platform.identity,
                        platform.asset,
                        postscript_name,
                        resolved_weight,
                        selector_slant,
                        font_slant_kind_from_platform(selector_slant),
                    )
                }
                None => {
                    let selector_slant = font_slant_from_fontdb(style);
                    (
                        selected_identity,
                        selected_asset?,
                        postscript_name,
                        effective_weight,
                        selector_slant,
                        font_slant_kind_from_platform(selector_slant),
                    )
                }
            };
        let px_metrics = platform_px_metrics
            .or_else(|| Self::probe_resolved_font_metrics(&identity, None, font_size))
            .or_else(|| {
                self.font_px_metrics_from_selected_face(
                    font_id,
                    font_size,
                    &identity.variation_coords,
                )
            });
        let vertical = px_metrics
            .map(|metrics| FontVerticalMetrics {
                ascent: metrics.ascent.max(0) as f32,
                descent: metrics.descent.max(0) as f32,
                line_height: metrics.height.max(1) as f32,
            })
            .or_else(|| self.font_metrics_from_selected_face(font_id, font_size));
        let glyph_advance = resolved_font_advance(spacing, px_metrics);
        let device_ascii_advances =
            self.probe_resolved_font_device_ascii_advances(&identity, font_size);
        let replay = FontReplay::Swash { asset };
        let font =
            self.intern_resolved_font(identity, replay, font_size, |id, identity, replay| {
                ResolvedFont {
                    id,
                    identity,
                    replay,
                    family: resolved_family,
                    full_name: None,
                    postscript_name,
                    // Preserve the resolved CSS weight, not the container face's
                    // metadata weight (variable fonts; cf. `select_font_for_char`).
                    weight: resolved_weight,
                    slant: render_slant,
                    width: stretch.to_number(),
                    pixel_size: font_size,
                    ascent_px: vertical.as_ref().map(|v| v.ascent).unwrap_or(0.0),
                    descent_px: vertical.as_ref().map(|v| v.descent).unwrap_or(0.0),
                    space_advance_px: px_metrics
                        .map(|metrics| metrics.space_width.max(0) as f32)
                        .unwrap_or(0.0),
                    glyph_advance,
                }
            });
        Some(LayoutFontHandle {
            font,
            resolution: FontResolutionSource::FacePrimary,
            selector_slant,
            source: LayoutFontSource::Swash(font_id),
            px_metrics,
            device_ascii_advances,
        })
    }

    fn materialize_bitmap_font(
        &mut self,
        matched: &crate::font_backend::PlatformFontMatch,
        family: &str,
        requested_weight: u16,
        font_size: f32,
        resolution: FontResolutionSource,
    ) -> Option<LayoutFontHandle> {
        let opened = match self.open_bitmap_font(matched, font_size) {
            Ok(opened) => opened,
            Err(error) => {
                tracing::warn!(
                    target: "font_boundary",
                    identity = %matched.identity.stable_key,
                    %error,
                    "failed to open the selected fixed-font entity; using resolved fallback"
                );
                return None;
            }
        };
        let observed = opened.metrics();
        let effective_size = observed.effective_layout_px;
        let px_metrics = crate::font::probe::FontPxMetrics {
            pixel_size: effective_size.round().max(1.0) as u32,
            height: observed.height_px.round().max(1.0) as i32,
            ascent: observed.ascent_px.round().max(0.0) as i32,
            descent: observed.descent_px.round().max(0.0) as i32,
            max_width: observed.max_advance_px.round().max(0.0) as i32,
            space_width: observed.space_advance_px.round().max(0.0) as i32,
            average_width: observed.average_advance_px.round().max(0.0) as i32,
        };
        let glyph_advance =
            resolved_font_advance(matched.metadata.fixed_spacing_policy(), Some(px_metrics));
        let identity = matched.identity.clone();
        let selector_slant = matched.slant();
        let replay = opened.replay();
        let font =
            self.intern_resolved_font(identity, replay, effective_size, |id, identity, replay| {
                ResolvedFont {
                    id,
                    identity,
                    replay,
                    family: family.to_owned(),
                    full_name: None,
                    postscript_name: matched.identity.postscript_name.clone(),
                    weight: matched.weight().unwrap_or(requested_weight),
                    slant: font_slant_kind_from_platform(selector_slant),
                    width: matched.metadata.width_class(),
                    pixel_size: effective_size,
                    ascent_px: observed.ascent_px,
                    descent_px: observed.descent_px,
                    space_advance_px: observed.space_advance_px,
                    glyph_advance,
                }
            });
        Some(LayoutFontHandle {
            font,
            resolution,
            selector_slant,
            source: LayoutFontSource::FreeTypeBitmap(opened),
            px_metrics: Some(px_metrics),
            device_ascii_advances: None,
        })
    }

    /// Resolve the font GNU assigns to one character under a face, as an exact
    /// interned identity.
    ///
    /// GNU's `face_for_char` returns the realized ASCII face without checking
    /// character coverage; only non-ASCII characters enter fontset fallback.
    /// Preserve that distinction here: ASCII delegates to face-primary
    /// realization, while non-ASCII runs the same per-character fallback used
    /// by measurement and pins the concrete fontdb face selected by shaping.
    pub fn resolved_font_for_char(
        &mut self,
        ch: char,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<ResolvedFont> {
        self.resolved_font_for_realized_face_char(
            ch,
            RealizedFaceFontSelection::same_fontset(family, weight, italic, font_size),
        )
    }

    pub fn resolved_font_for_realized_face_char(
        &mut self,
        ch: char,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<ResolvedFont> {
        self.materialized_font_for_realized_face_char(ch, selection)
            .map(|materialized| materialized.font)
    }

    fn materialized_font_for_realized_face_char(
        &mut self,
        ch: char,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<LayoutFontHandle> {
        if ch.is_ascii() {
            return self.materialized_font_for_face(
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            );
        }

        // GNU fontset.c `face_for_char`: with the default policy enabled, a
        // symbol covered by this realized face's ASCII font stays on that font
        // before the base fontset is consulted.
        if self.symbol_font_policy.uses_primary_font_for(ch)
            && let Some(primary) = self.materialized_font_for_face(
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            )
            && self.materialized_font_has_char(&primary, ch)
        {
            return Some(primary);
        }

        let key = (self.realized_face_font_cache_key(selection), ch);
        if let Some(cached) = self.resolved_char_font_cache.get(&key) {
            return cached.clone();
        }
        let materialized = self.materialize_char_font(ch, selection);
        self.resolved_char_font_cache
            .insert(key, materialized.clone());
        materialized
    }

    fn materialize_char_font(
        &mut self,
        ch: char,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<LayoutFontHandle> {
        let resolved = self.font_request_for_char(ch, selection);
        let spacing = resolved
            .platform
            .as_ref()
            .map(|matched| matched.metadata.fixed_spacing_policy())
            .unwrap_or_else(|| {
                if self
                    .font_resolver
                    .family_prefers_monospace(&resolved.family)
                {
                    neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell
                } else {
                    neomacs_display_protocol::font::FixedFontSpacing::ProportionalOrDual
                }
            });
        if let Some(matched) = resolved.platform.as_ref()
            && matched.metadata.size.is_fixed()
        {
            return self.materialize_bitmap_font(
                matched,
                &resolved.family,
                resolved.weight,
                selection.font_size,
                FontResolutionSource::FontsetFallback,
            );
        }
        let attrs = self.build_attrs_for_resolved_char(&resolved, selection.font_size)?;
        let metrics = safe_metrics(selection.font_size, selection.font_size * 1.3);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            Some(selection.font_size * 4.0),
            Some(selection.font_size * 2.0),
        );
        let text = String::from(ch);
        buffer.set_text(
            &mut self.font_system,
            &text,
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        let font_id = buffer
            .layout_runs()
            .find_map(|run| run.glyphs.iter().next())?
            .physical((0.0, 0.0), 1.0)
            .cache_key
            .font_id;
        if let Some(platform) = resolved.platform.as_ref() {
            let expected_font_id = self
                .font_file_cache
                .pinned_exact_asset(&platform.asset)?
                .fontdb_id();
            if font_id != expected_font_id {
                tracing::error!(
                    target: "font_boundary",
                    character = %ch.escape_unicode(),
                    family = %resolved.family,
                    selected = ?font_id,
                    expected = ?expected_font_id,
                    platform = %platform.identity.stable_key,
                    "exact platform fallback asset was not selected for layout"
                );
                return None;
            }
        }
        let (file, face_index, postscript_name, style, stretch) = {
            let face = self.font_system.db().face(font_id)?;
            (
                fontdb_face_file(face),
                face.index,
                Some(face.post_script_name.clone()).filter(|name| !name.is_empty()),
                face.style,
                face.stretch,
            )
        };
        let selected_identity = self.fontdb_face_identity(
            file.as_deref(),
            face_index,
            postscript_name.clone(),
            &resolved.family,
        );
        let selected_asset = file
            .as_deref()
            .and_then(|path| FontFileAsset::new(path, face_index).map(FontOutlineAsset::File));
        let (identity, asset, postscript_name, selector_slant, render_slant) =
            match resolved.platform.as_ref() {
                Some(platform) => (
                    platform.identity.clone(),
                    platform.asset.clone(),
                    platform
                        .identity
                        .postscript_name
                        .clone()
                        .or(postscript_name),
                    platform.slant(),
                    font_slant_kind_from_platform(platform.slant()),
                ),
                None => {
                    let selector_slant = font_slant_from_fontdb(style);
                    (
                        selected_identity,
                        selected_asset?,
                        postscript_name,
                        selector_slant,
                        font_slant_kind_from_platform(selector_slant),
                    )
                }
            };
        let px_metrics = Self::probe_resolved_font_metrics(
            &identity,
            resolved.platform.as_ref(),
            selection.font_size,
        )
        .or_else(|| {
            self.font_px_metrics_from_selected_face(
                font_id,
                selection.font_size,
                &identity.variation_coords,
            )
        });
        let vertical = px_metrics
            .map(|metrics| FontVerticalMetrics {
                ascent: metrics.ascent.max(0) as f32,
                descent: metrics.descent.max(0) as f32,
                line_height: metrics.height.max(1) as f32,
            })
            .or_else(|| self.font_metrics_from_selected_face(font_id, selection.font_size));
        let glyph_advance = resolved_font_advance(spacing, px_metrics);
        let replay = FontReplay::Swash { asset };
        let font = self.intern_resolved_font(
            identity,
            replay,
            selection.font_size,
            |id, identity, replay| ResolvedFont {
                id,
                identity,
                replay,
                family: resolved.family.clone(),
                full_name: None,
                postscript_name,
                weight: resolved.weight,
                slant: render_slant,
                width: stretch.to_number(),
                pixel_size: selection.font_size,
                ascent_px: vertical.as_ref().map(|v| v.ascent).unwrap_or(0.0),
                descent_px: vertical.as_ref().map(|v| v.descent).unwrap_or(0.0),
                space_advance_px: px_metrics
                    .map(|metrics| metrics.space_width.max(0) as f32)
                    .unwrap_or(0.0),
                glyph_advance,
            },
        );
        Some(LayoutFontHandle {
            font,
            resolution: FontResolutionSource::FontsetFallback,
            selector_slant,
            source: LayoutFontSource::Swash(font_id),
            px_metrics,
            device_ascii_advances: None,
        })
    }

    /// Shape a composed cluster and return its glyphs with exact interned
    /// font identities plus the distinct fonts they reference — the
    /// renderable payload the render thread rasterizes without re-shaping.
    ///
    /// Shapes the cluster text standalone (the same input the renderer's
    /// composed path uses), so replaying these glyphs reproduces current
    /// visual behavior with the re-selection risk removed.
    pub fn resolved_glyphs_for_cluster(
        &mut self,
        text: &str,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> Option<(Vec<ResolvedGlyph>, Vec<ResolvedFont>)> {
        self.resolved_glyphs_for_realized_face_cluster(
            text,
            RealizedFaceFontSelection::same_fontset(family, weight, italic, font_size),
        )
    }

    pub fn resolved_glyphs_for_realized_face_cluster(
        &mut self,
        text: &str,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<(Vec<ResolvedGlyph>, Vec<ResolvedFont>)> {
        if text.is_empty() {
            return None;
        }
        let key = (
            self.realized_face_font_cache_key(selection),
            text.to_string(),
        );
        if let Some(cached) = self.resolved_cluster_cache.get(&key) {
            return cached.clone();
        }
        let result = self.resolve_cluster_uncached(text, selection);
        if self.resolved_cluster_cache.len() >= self.shaped_run_cache_cap {
            self.resolved_cluster_cache.clear();
        }
        self.resolved_cluster_cache.insert(key, result.clone());
        result
    }

    fn resolve_cluster_uncached(
        &mut self,
        text: &str,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<(Vec<ResolvedGlyph>, Vec<ResolvedFont>)> {
        let representative = crate::composition::representative_char_for_cluster(text);
        let materialized = match representative {
            Some(ch) => self.materialized_font_for_realized_face_char(ch, selection),
            None => self.materialized_font_for_face(
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            ),
        };
        if crate::composition::composition_glyph_plan(text)
            == crate::composition::CompositionGlyphPlan::SimpleCopy
            && let Some(
                primary @ LayoutFontHandle {
                    source: LayoutFontSource::FreeTypeBitmap(_),
                    ..
                },
            ) = materialized.as_ref()
            && let Some(resolved) =
                self.resolve_bitmap_simple_copy_cluster(text, primary.clone(), selection)
        {
            return Some(resolved);
        }

        // Route the cluster through the same representative-char font
        // resolution the renderer applies (emoji presentation via U+FE0F →
        // the color emoji font, CJK → covering font), so e.g. an emoji
        // keycap shapes to the emoji font's single color glyph instead of
        // the face font's digit + combining-keycap parts.
        let shaped = self.shape_run_for_realized_face(text, selection);
        if shaped.is_empty() {
            return None;
        }
        let mut fonts: Vec<ResolvedFont> = Vec::new();
        let mut by_fontdb: HashMap<fontdb::ID, ResolvedFontId> = HashMap::default();
        let selected_font = materialized.as_ref().and_then(|materialized| {
            if let LayoutFontSource::Swash(fontdb_id) = &materialized.source {
                Some((*fontdb_id, &materialized.font))
            } else {
                None
            }
        });
        let mut glyphs = Vec::with_capacity(shaped.len());
        for shaped_glyph in &shaped {
            let resolved_font_id = match by_fontdb.get(&shaped_glyph.font_id) {
                Some(&id) => id,
                None => {
                    // The generation-local fontdb::ID becomes a durable
                    // identity here, immediately, in the same generation
                    // that shaped it — the conversion the ShapedGlyph docs
                    // require before any glyph id reaches rasterization.
                    let font = match selected_font {
                        Some((fontdb_id, font)) if fontdb_id == shaped_glyph.font_id => {
                            font.clone()
                        }
                        _ => self.resolved_font_from_fontdb_id(
                            shaped_glyph.font_id,
                            selection.font_size,
                        )?,
                    };
                    let id = font.id;
                    by_fontdb.insert(shaped_glyph.font_id, id);
                    if !fonts.iter().any(|f| f.id == id) {
                        fonts.push(font);
                    }
                    id
                }
            };
            glyphs.push(ResolvedGlyph {
                resolved_font_id,
                glyph_id: shaped_glyph.glyph_id.into(),
                x: shaped_glyph.x,
                y: shaped_glyph.y,
                x_advance: shaped_glyph.x_advance,
                cluster_start: shaped_glyph.cluster_start as u32,
                cluster_end: shaped_glyph.cluster_end as u32,
            });
        }
        Some((glyphs, fonts))
    }

    fn resolve_bitmap_simple_copy_cluster(
        &mut self,
        text: &str,
        primary: LayoutFontHandle,
        selection: RealizedFaceFontSelection<'_>,
    ) -> Option<(Vec<ResolvedGlyph>, Vec<ResolvedFont>)> {
        let mut pen_x = 0.0;
        let mut glyphs = Vec::with_capacity(text.chars().count());
        let mut fonts = Vec::new();
        for (cluster_start, ch) in text.char_indices() {
            let materialized = if self.materialized_font_has_char(&primary, ch) {
                primary.clone()
            } else {
                let fallback = self.materialize_char_font(ch, selection)?;
                self.materialized_font_has_char(&fallback, ch)
                    .then_some(fallback)?
            };
            let (glyph_id, x_advance) = self.simple_copy_glyph_for_char(&materialized, ch)?;
            debug_assert_ne!(glyph_id.get(), 0);
            glyphs.push(ResolvedGlyph {
                resolved_font_id: materialized.font.id,
                glyph_id,
                x: pen_x,
                y: 0.0,
                x_advance,
                cluster_start: cluster_start as u32,
                cluster_end: (cluster_start + ch.len_utf8()) as u32,
            });
            if !fonts
                .iter()
                .any(|font: &ResolvedFont| font.id == materialized.font.id)
            {
                fonts.push(materialized.font);
            }
            pen_x += x_advance;
        }
        (!glyphs.is_empty()).then_some((glyphs, fonts))
    }

    fn simple_copy_glyph_for_char(
        &mut self,
        materialized: &LayoutFontHandle,
        ch: char,
    ) -> Option<(neomacs_display_protocol::font::ResolvedGlyphId, f32)> {
        let (glyph_id, measured_advance_px) = match &materialized.source {
            LayoutFontSource::FreeTypeBitmap(opened) => {
                let glyph_id = opened.glyph_for_char(ch)?;
                let advance = opened.glyph_advance_px(glyph_id).ok()?;
                (glyph_id, advance)
            }
            LayoutFontSource::Swash(fontdb_id) => {
                let font = self
                    .font_system
                    .get_font(*fontdb_id, fontdb::Weight(materialized.font.weight))?;
                let swash = font.as_swash();
                let glyph_id = swash.charmap().map(ch);
                if glyph_id == 0 {
                    return None;
                }
                let advance = swash
                    .glyph_metrics(&[])
                    .scale(materialized.font.pixel_size)
                    .advance_width(glyph_id);
                (glyph_id.into(), advance)
            }
        };
        Some((
            glyph_id,
            materialized.font.glyph_advance.resolve(measured_advance_px),
        ))
    }

    /// Build a [`ResolvedFont`] for a concrete fontdb face chosen by
    /// shaping. Unlike the face/char resolvers (which preserve selector
    /// family/weight semantics), this records the file's own metadata: the
    /// font was picked by shaping fallback, not by a request.
    fn resolved_font_from_fontdb_id(
        &mut self,
        font_id: fontdb::ID,
        font_size: f32,
    ) -> Option<ResolvedFont> {
        let (file, face_index, postscript_name, style, stretch, family, file_weight) = {
            let face = self.font_system.db().face(font_id)?;
            (
                fontdb_face_file(face),
                face.index,
                Some(face.post_script_name.clone()).filter(|name| !name.is_empty()),
                face.style,
                face.stretch,
                face.families
                    .first()
                    .map(|(name, _)| name.clone())
                    .unwrap_or_default(),
                face.weight.0,
            )
        };
        let identity = self.fontdb_face_identity(
            file.as_deref(),
            face_index,
            postscript_name.clone(),
            &family,
        );
        let px_metrics =
            Self::probe_resolved_font_metrics(&identity, None, font_size).or_else(|| {
                self.font_px_metrics_from_selected_face(
                    font_id,
                    font_size,
                    &identity.variation_coords,
                )
            });
        let vertical = px_metrics
            .map(|metrics| FontVerticalMetrics {
                ascent: metrics.ascent.max(0) as f32,
                descent: metrics.descent.max(0) as f32,
                line_height: metrics.height.max(1) as f32,
            })
            .or_else(|| self.font_metrics_from_selected_face(font_id, font_size));
        // Prefer the platform's answer for this exact file (GNU `FC_SPACING`
        // / CoreText traits and the realized weight) over the file's own
        // metadata and the family-name heuristic, so a font first realized
        // through shaping publishes the record the face path would. Selection
        // only: the face is already open in `font_system`, so nothing is
        // pinned or re-validated here.
        let selection_size = self.selection_size(font_size);
        let platform = self
            .font_resolver
            .resolve_primary(
                &family,
                file_weight,
                font_slant_from_fontdb(style),
                FontWidth::Normal,
                selection_size,
            )
            .filter(|matched| {
                matched.identity.file_path.as_deref() == file.as_deref()
                    && matched.identity.file_face_index() == face_index
            });
        let spacing = match platform.as_ref() {
            Some(matched) => matched.metadata.fixed_spacing_policy(),
            None if self.font_resolver.family_prefers_monospace(&family) => {
                neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell
            }
            None => neomacs_display_protocol::font::FixedFontSpacing::ProportionalOrDual,
        };
        let weight = platform
            .as_ref()
            .and_then(|matched| matched.weight())
            .unwrap_or(file_weight);
        let glyph_advance = resolved_font_advance(spacing, px_metrics);
        let replay = swash_file_replay(&identity)?;
        Some(
            self.intern_resolved_font(identity, replay, font_size, |id, identity, replay| {
                ResolvedFont {
                    id,
                    identity,
                    replay,
                    family,
                    full_name: None,
                    postscript_name,
                    weight,
                    slant: font_slant_kind_from_fontdb(style),
                    width: stretch.to_number(),
                    pixel_size: font_size,
                    ascent_px: vertical.as_ref().map(|v| v.ascent).unwrap_or(0.0),
                    descent_px: vertical.as_ref().map(|v| v.descent).unwrap_or(0.0),
                    space_advance_px: px_metrics
                        .map(|metrics| metrics.space_width.max(0) as f32)
                        .unwrap_or(0.0),
                    glyph_advance,
                }
            }),
        )
    }

    /// Intern one realized instance and return its canonical record.
    ///
    /// The interner owns the record: within one catalog generation the first
    /// builder to realize a `(identity, replay, pixel_size)` key defines the
    /// [`ResolvedFont`] every later path receives for that id, so no
    /// resolution path can publish a second record under an id the frame
    /// table already carries. GNU font.c `font_open_entity` likewise hands
    /// back the font object already open for an entity at that pixel size
    /// (`FONT_OBJLIST_INDEX`) instead of opening a second one, and
    /// `font_clear_cache` closes those objects when the font set changes.
    fn intern_resolved_font(
        &mut self,
        identity: ResolvedFontIdentity,
        replay: FontReplay,
        pixel_size: f32,
        build: impl FnOnce(ResolvedFontId, ResolvedFontIdentity, FontReplay) -> ResolvedFont,
    ) -> ResolvedFont {
        let key = ResolvedFontInstanceKey {
            identity: identity.clone(),
            replay: replay.clone(),
            pixel_size_bits: pixel_size.to_bits(),
        };
        let id = match self.resolved_font_ids.get(&key) {
            Some(&id) => id,
            None => {
                // Ids start at 1; 0 stays unused so an uninitialized id is visible.
                let id = ResolvedFontId(self.resolved_font_ids.len() as u32 + 1);
                self.resolved_font_ids.insert(key, id);
                id
            }
        };
        if let Some(font) = self.resolved_font_records.get(&id) {
            return font.clone();
        }
        let mut font = build(id, identity, replay);
        font.id = id;
        self.resolved_font_records.insert(id, font.clone());
        font
    }

    /// Measure one character after platform selection has produced its exact
    /// font identity.
    fn measure_resolved_char(
        &mut self,
        ch: char,
        resolved: &ResolvedCharFont,
        font_size: f32,
    ) -> f32 {
        if let Some(matched) = resolved.platform.as_ref()
            && matched.metadata.size.is_fixed()
        {
            match self.open_bitmap_font(matched, font_size) {
                Ok(opened) => {
                    return opened
                        .glyph_for_char(ch)
                        .and_then(|glyph| opened.glyph_advance_px(glyph).ok())
                        .filter(|width| valid_advance(*width))
                        .unwrap_or(opened.metrics().space_advance_px);
                }
                Err(error) => tracing::warn!(
                    target: "font_boundary",
                    identity = %matched.identity.stable_key,
                    %error,
                    character = %ch.escape_unicode(),
                    "failed to reopen selected fixed font for measurement"
                ),
            }
        }
        let Some(attrs) = self.build_attrs_for_resolved_char(resolved, font_size) else {
            tracing::error!(
                target: "font_boundary",
                character = %ch.escape_unicode(),
                identity = resolved.platform.as_ref().map(|font| font.identity.stable_key.as_str()),
                "exact platform font could not be loaded for character measurement"
            );
            return font_size * 0.6;
        };
        let line_height = font_size * 1.3;
        let metrics = safe_metrics(font_size, line_height);

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            Some(font_size * 4.0),
            Some(font_size * 2.0),
        );

        let text = String::from(ch);
        buffer.set_text(
            &mut self.font_system,
            &text,
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        buffer
            .layout_runs()
            .find_map(|run| run.glyphs.iter().next().map(|glyph| glyph.w))
            .unwrap_or(font_size * 0.6)
    }

    /// Build the semantic shaping request for a character.
    ///
    /// This is deliberately distinct from
    /// `materialized_font_for_realized_face_char`: the request preserves the
    /// platform selector answer used to pin shaping, while materialization
    /// records the concrete font that shaping opened.
    fn font_request_for_char(
        &mut self,
        ch: char,
        selection: RealizedFaceFontSelection<'_>,
    ) -> ResolvedCharFont {
        let requested_slant = if selection.italic {
            FontSlant::Italic
        } else {
            FontSlant::Normal
        };
        if ch.is_ascii() {
            let resolved_family = self.resolve_family(
                &self.font_resolver.resolve_family(selection.primary_family),
                None,
            );
            let platform = self.platform_primary_match(
                &resolved_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            );
            // Snap to the family's available/instance weight, matching the
            // font actually opened (and what `build_attrs` renders with), so
            // `font-at` reports the opened instance's weight like GNU — e.g.
            // a semi-light request on variable Noto Sans reports light.
            let resolved_weight = platform
                .as_ref()
                .and_then(|matched| matched.weight())
                .unwrap_or_else(|| {
                    crate::font::font_match::resolve_weight_in_family(
                        &self.font_system,
                        &resolved_family,
                        selection.weight,
                        selection.italic,
                    )
                });
            let resolved_slant = platform
                .as_ref()
                .map(|matched| matched.slant())
                .unwrap_or(requested_slant);
            return ResolvedCharFont {
                family: resolved_family,
                weight: resolved_weight,
                slant: resolved_slant,
                platform,
            };
        }

        if let Some(matched) = self.font_resolver.resolve_for_char(
            selection.fontset_base_family,
            ch,
            selection.weight,
            requested_slant,
            FontWidth::Normal,
            self.selection_size(selection.font_size),
        ) {
            let resolved_family = self.resolve_family(matched.family(), matched.file_path());
            let resolved_weight = matched.weight().unwrap_or_else(|| {
                crate::font::font_match::resolve_weight_in_family(
                    &self.font_system,
                    &resolved_family,
                    selection.weight,
                    selection.italic,
                )
            });
            let resolved_slant = matched.slant();
            let platform = self.materialize_platform_match(matched);
            return ResolvedCharFont {
                weight: resolved_weight,
                family: resolved_family,
                slant: resolved_slant,
                platform,
            };
        }

        ResolvedCharFont {
            family: selection.fontset_base_family.to_string(),
            weight: selection.weight,
            slant: requested_slant,
            platform: None,
        }
    }

    /// Get the advance width for a single character.
    pub fn char_width(
        &mut self,
        ch: char,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> f32 {
        self.char_width_for_realized_face(
            ch,
            RealizedFaceFontSelection::same_fontset(family, weight, italic, font_size),
        )
    }

    pub fn char_width_for_realized_face(
        &mut self,
        ch: char,
        selection: RealizedFaceFontSelection<'_>,
    ) -> f32 {
        let key = self.cache_key(
            selection.primary_family,
            selection.weight,
            selection.italic,
            selection.font_size,
        );

        // For ASCII, check the ASCII cache first
        let cp = ch as u32;
        if cp < 128 {
            if let Some(widths) = self.ascii_cache.get(&key) {
                return widths[cp as usize];
            }
            // Fill the whole ASCII cache on miss
            let widths = self.fill_ascii_widths_inner(
                selection.primary_family,
                selection.weight,
                selection.italic,
                selection.font_size,
            );
            let w = widths[cp as usize];
            self.ascii_cache.insert(key, widths);
            return w;
        }

        // Non-ASCII: resolve the actual covering font for this character.
        // GNU's font_range starts from the selected font and advances only
        // while font_encode_char accepts each concrete character; a broad
        // Unicode script cache is too coarse for Common/emoji symbols.
        let char_key = (self.realized_face_font_cache_key(selection), ch);
        if let Some(&w) = self.char_cache.get(&char_key) {
            return w;
        }

        let materialized = self.materialized_font_for_realized_face_char(ch, selection);
        let direct_glyph = materialized.as_ref().filter(|materialized| {
            materialized.resolution == FontResolutionSource::FacePrimary
                || materialized
                    .font
                    .glyph_advance
                    .fixed_cell_advance_px()
                    .is_some()
        });
        let w = direct_glyph
            .and_then(|materialized| self.simple_copy_glyph_for_char(materialized, ch))
            .map(|(_, advance_px)| advance_px)
            .unwrap_or_else(|| {
                let resolved = self.font_request_for_char(ch, selection);
                self.measure_resolved_char(ch, &resolved, selection.font_size)
            });
        self.char_cache.insert(char_key, w);
        w
    }

    /// Fill ASCII width array (0-127) for given face attributes.
    /// Returns the cached array. Populates the cache on miss.
    pub fn fill_ascii_widths(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> [f32; 128] {
        let key = self.cache_key(family, weight, italic, font_size);
        if let Some(widths) = self.ascii_cache.get(&key) {
            return *widths;
        }

        let widths = self.fill_ascii_widths_inner(family, weight, italic, font_size);
        self.ascii_cache.insert(key, widths);
        widths
    }

    /// Internal: measure all 128 ASCII characters in a single buffer.
    fn fill_ascii_widths_inner(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> [f32; 128] {
        let mut widths = [0.0f32; 128];
        let materialized = self.materialized_font_for_face(family, weight, italic, font_size);
        let glyph_advance = materialized
            .as_ref()
            .map(|font| font.font.glyph_advance)
            .unwrap_or_default();
        if let Some(LayoutFontHandle {
            source: LayoutFontSource::FreeTypeBitmap(font),
            px_metrics,
            ..
        }) = materialized.as_ref()
        {
            let space_width = px_metrics
                .map(|metrics| metrics.space_width as f32)
                .filter(|width| valid_advance(*width))
                .unwrap_or(font_size * 0.6);
            widths[..32].fill(space_width);
            widths[127] = space_width;
            for cp in 32u32..127 {
                let ch = char::from_u32(cp).unwrap();
                widths[cp as usize] = font
                    .glyph_for_char(ch)
                    .and_then(|glyph| font.glyph_advance_px(glyph).ok())
                    .filter(|width| valid_advance(*width))
                    .map(|width| glyph_advance.resolve(width))
                    .unwrap_or(space_width);
            }
            return widths;
        }
        if let Some(device_advances) = materialized
            .as_ref()
            .and_then(|font| font.device_ascii_advances.as_deref())
        {
            debug_assert_eq!(
                device_advances.device_pixel_size(),
                self.selection_size(font_size).rounded_device_px(),
                "cached device advances must belong to this realized size"
            );
            let space_width = device_advances
                .logical_advance(' ', self.device_scale)
                .unwrap_or(font_size * 0.6);
            widths[..32].fill(space_width);
            widths[127] = space_width;
            for byte in 32u8..127 {
                widths[usize::from(byte)] = device_advances
                    .logical_advance(char::from(byte), self.device_scale)
                    .unwrap_or(space_width);
            }
            return widths;
        }
        let attrs = materialized
            .as_ref()
            .and_then(|font| self.build_attrs_for_materialized_font(font))
            .unwrap_or_else(|| {
                self.build_attrs(
                    family,
                    weight,
                    if italic {
                        FontSlant::Italic
                    } else {
                        FontSlant::Normal
                    },
                    font_size,
                )
            });
        let line_height = font_size * 1.3;
        let metrics = safe_metrics(font_size, line_height);

        // Measure each printable ASCII character individually.
        // GNU's ftcrfont driver probes every absent printable through glyph 0,
        // then xdisp uses the primary font's space width for a missing glyph.
        // Prefer that exact primary probe; semantic shaping may choose a
        // covering fallback, which is specifically wrong for ASCII.
        let measured_primary_space = materialized
            .as_ref()
            .and_then(|font| font.px_metrics)
            .map(|font| font.space_width as f32)
            .filter(|width| valid_advance(*width));
        let shaped_space_width = || {
            let mut buffer = Buffer::new(&mut self.font_system, metrics);
            buffer.set_size(
                &mut self.font_system,
                Some(font_size * 4.0),
                Some(font_size * 2.0),
            );
            buffer.set_text(
                &mut self.font_system,
                " ",
                &attrs,
                cosmic_text::Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);
            buffer
                .layout_runs()
                .find_map(|run| run.glyphs.iter().next().map(|glyph| glyph.w))
                .unwrap_or(font_size * 0.6)
        };
        let space_width = measured_primary_space.unwrap_or_else(shaped_space_width);
        // Control chars (0-31) and DEL (127) get space width
        widths[..32].fill(space_width);
        widths[127] = space_width;

        // Measure printable ASCII (32-126) using a single buffer with all chars.
        // Shape them individually to get per-character advances.
        for cp in 32u32..127 {
            let ch = char::from_u32(cp).unwrap();
            if materialized
                .as_ref()
                .is_some_and(|font| !self.materialized_font_has_char(font, ch))
            {
                widths[cp as usize] = space_width;
                continue;
            }
            if ch == ' ' {
                widths[cp as usize] = glyph_advance.resolve(space_width);
                continue;
            }
            let mut buffer = Buffer::new(&mut self.font_system, metrics);
            buffer.set_size(
                &mut self.font_system,
                Some(font_size * 4.0),
                Some(font_size * 2.0),
            );
            let text = String::from(ch);
            buffer.set_text(
                &mut self.font_system,
                &text,
                &attrs,
                cosmic_text::Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);

            widths[cp as usize] = buffer
                .layout_runs()
                .find_map(|run| run.glyphs.iter().next().map(|glyph| glyph.w))
                .map(|width| glyph_advance.resolve(width))
                .unwrap_or(space_width);
        }

        widths
    }

    /// Get font metrics (ascent, descent, line height, char width) for a face.
    pub fn font_metrics(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> FontMetrics {
        self.observe_font_metrics(family, weight, italic, font_size)
            .metrics
    }

    fn observe_font_metrics(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> FontMetricObservation {
        let key = self.cache_key(family, weight, italic, font_size);
        if let Some(observation) = self.metrics_cache.get(&key) {
            return *observation;
        }

        let primary_override = self
            .materialized_font_for_face(family, weight, italic, font_size)
            .and_then(|font| font.px_metrics);
        let (vertical, advances, effective_size, source) = if let Some(probe) = primary_override {
            (
                FontVerticalMetrics {
                    ascent: probe.ascent.max(0) as f32,
                    descent: probe.descent.max(0) as f32,
                    line_height: probe.height.max(1) as f32,
                },
                FontAdvanceMetrics::from_font_probe(probe),
                GraphicFontSizePx::new(probe.pixel_size as f32),
                FontMetricSource::OpenedFontProbe,
            )
        } else {
            let (selected_font_id, measured_space_width) =
                self.selected_font_id_and_space_width(family, weight, italic, font_size);
            let vertical = if let Some(font_id) = selected_font_id {
                self.observe_selected_face_vertical_metrics(font_id, font_size)
            } else {
                None
            };
            let vertical = vertical.unwrap_or_else(|| FontVerticalObservation {
                metrics: self
                    .glyph_box_fallback_vertical_metrics(family, weight, italic, font_size),
                advances: None,
                effective_size: GraphicFontSizePx::new(font_size),
                source: FontMetricSource::GlyphBoxFallback,
            });
            let advances = vertical.advances.unwrap_or_else(|| {
                let ascii_widths = self.fill_ascii_widths(family, weight, italic, font_size);
                FontAdvanceMetrics::from_ascii_widths(measured_space_width, &ascii_widths)
            });
            (
                vertical.metrics,
                advances,
                vertical.effective_size,
                vertical.source,
            )
        };
        let frame_cell = derive_observed_frame_cell_metrics(
            self.font_resolver.family_prefers_monospace(family),
            font_size,
            effective_size,
            vertical,
            advances,
        );
        if frame_cell.confidence == MetricConfidence::Degraded {
            tracing::debug!(
                "font_metrics: degraded frame cell width fallback for family={family:?} size={font_size}"
            );
        }
        let observation = FontMetricObservation {
            metrics: FontMetrics {
                ascent: frame_cell.ascent,
                descent: frame_cell.descent,
                line_height: frame_cell.line_height,
                char_width: frame_cell.column_width,
                space_width: advances.space_width,
            },
            effective_size,
            source,
        };

        tracing::trace!(?source, family, font_size, "observed font metrics");
        self.metrics_cache.insert(key, observation);
        observation
    }

    /// Resolve and measure the default cell of one graphic frame as a single
    /// typed publication unit.
    pub(crate) fn frame_cell_geometry(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        requested_size: f32,
        domain: FrameFontDomain,
    ) -> FrameCellGeometry {
        let Some(requested_size) = domain.graphic_size(requested_size) else {
            return FrameCellGeometry::TerminalCell;
        };
        let observation = self.observe_font_metrics(family, weight, italic, requested_size.get());
        tracing::trace!(
            source = ?observation.source,
            family,
            font_size = observation.effective_size.unwrap_or(requested_size).get(),
            "publishing graphic frame cell geometry"
        );
        FrameCellGeometry::Graphic(GraphicFrameCellGeometry {
            font_size: observation.effective_size.unwrap_or(requested_size),
            metrics: observation.metrics,
        })
    }

    fn glyph_box_fallback_vertical_metrics(
        &mut self,
        family: &str,
        weight: u16,
        italic: bool,
        font_size: f32,
    ) -> FontVerticalMetrics {
        let attrs = self.build_attrs(
            family,
            weight,
            if italic {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            },
            font_size,
        );
        let line_height = font_size * 1.3;
        let metrics = safe_metrics(font_size, line_height);

        // Fallback only: measure a representative glyph box when the selected
        // font's global tables are unavailable or obviously pathological.
        let sample = " Mg";
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(
            &mut self.font_system,
            Some(font_size * 8.0),
            Some(font_size * 2.0),
        );
        buffer.set_text(
            &mut self.font_system,
            sample,
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let mut ascent = font_size.ceil().max(1.0);
        let mut descent = (line_height.ceil() - ascent).max(0.0);
        let mut actual_line_height = (ascent + descent).max(1.0);

        if let Some(layout) = buffer.line_layout(&mut self.font_system, 0)
            && let Some(line) = layout.first()
        {
            ascent = line.max_ascent.ceil().max(1.0);
            descent = line.max_descent.ceil().max(0.0);
            actual_line_height = (ascent + descent).max(1.0);
        }

        FontVerticalMetrics {
            ascent,
            descent,
            line_height: actual_line_height,
        }
    }

    /// Clear all caches. Call when fonts change (e.g., text-scale-adjust).
    /// `resolved_font_ids` and `resolved_font_records` intentionally survive:
    /// complete instance keys are durable and ids must stay stable across
    /// generations; records are dropped only by a catalog advance (see field
    /// docs).
    pub fn clear_caches(&mut self) {
        self.ascii_cache.clear();
        self.char_cache.clear();
        self.metrics_cache.clear();
        self.shaped_run_cache.clear();
        self.resolved_face_font_cache.clear();
        self.resolved_char_font_cache.clear();
        self.resolved_cluster_cache.clear();
        self.font_resolver.clear_caches();
        self.primary_pin_cache.clear();
        self.primary_match_cache.clear();
        // A previously missing/replaced file may become available after the
        // caller refreshes fonts. Successful pins remain valid for this
        // FontSystem; failed materializations must be retryable.
        self.font_file_cache.retry_failed_exact_faces();
    }

    /// Number of times `shape_run` actually invoked cosmic-text shaping (i.e.
    /// `shaped_run_cache` misses). Used by tests to assert the measure/render
    /// double-shape is deduped to one shape per (run, face).
    #[cfg(test)]
    pub(crate) fn shape_calls(&self) -> usize {
        self.n_shape_calls
    }

    /// Lower the `shaped_run_cache` entry cap so tests can exercise the
    /// clear-on-overflow path without shaping `SHAPED_RUN_CACHE_CAP` runs.
    #[cfg(test)]
    pub(crate) fn set_shaped_run_cache_cap(&mut self, cap: usize) {
        self.shaped_run_cache_cap = cap;
    }
}

/// Realize resolved font identities for every face of a finished frame.
///
/// Runs at the engine's frame-output boundary (after all install paths have
/// filled `state.faces`), so it covers every face regardless of which layout
/// path produced it. For each face it resolves the primary font through the
/// same `FontMetricsService` that produced the face's layout metrics, then
/// publishes `Face::default_resolved_font_id`, the frame's font table, and
/// the `font_file_path` bridge the renderer already primes.
///
/// No-op when `service` is `None` (TTY frames have no GUI font realization).
pub fn realize_frame_fonts(
    state: &mut neomacs_display_protocol::glyph_matrix::FrameDisplayState,
    service: &mut Option<FontMetricsService>,
) {
    let Some(svc) = service.as_mut() else {
        return;
    };
    state.font_catalog_generation = svc.font_catalog_generation();
    // Deterministic interner allocation order across identical frames.
    let mut face_ids: Vec<FaceId> = state.faces.keys().copied().collect();
    face_ids.sort_unstable();
    for face_id in face_ids {
        let Some(face) = state.faces.get_mut(&face_id) else {
            continue;
        };
        let family = if face.font_family.is_empty() {
            "monospace"
        } else {
            face.font_family.as_str()
        };
        let italic = face.is_italic();
        match svc.resolved_font_for_face(family, face.font_weight, italic, face.font_size.max(1.0))
        {
            Some(font) => {
                face.default_resolved_font_id = Some(font.id);
                if face.font_file_path.is_none() {
                    face.font_file_path = font.identity.file_path.clone();
                }
                state.fonts.entry(font.id).or_insert(font);
            }
            None => {
                // Phase 0 divergence instrumentation: this face will reach
                // the render thread without a resolved identity and trigger
                // its independent semantic fallback.
                tracing::warn!(
                    target: "font_boundary",
                    face_id = face_id.get(),
                    family = %face.font_family,
                    weight = face.font_weight,
                    "GUI face has no resolvable primary font; renderer will re-select"
                );
            }
        }
    }

    realize_frame_char_fonts(state, svc);
}

fn protocol_face_font_selection(
    face: &neomacs_display_protocol::face::Face,
) -> RealizedFaceFontSelection<'_> {
    let primary_family = if face.font_family.is_empty() {
        "monospace"
    } else {
        face.font_family.as_str()
    };
    RealizedFaceFontSelection::new(
        PrimaryFontFamily::new(primary_family),
        FontsetBaseFamily::new(face.fontset_base_family_or_primary()),
        face.font_weight,
        face.is_italic(),
        face.font_size.max(1.0),
    )
}

/// Stamp per-character fallback fonts for the non-ASCII characters actually
/// on this frame's grid (`FrameDisplayState::char_fonts`).
///
/// For each (face, representative char) pair present in the window matrices
/// and chrome rows, resolves the covering font through the same per-char path
/// the measurement code uses and publishes the exact identity, so the render
/// thread's CJK/emoji/symbol fallback becomes a table lookup instead of its
/// own fontconfig match.
fn realize_frame_char_fonts(
    state: &mut neomacs_display_protocol::glyph_matrix::FrameDisplayState,
    svc: &mut FontMetricsService,
) {
    use neomacs_display_protocol::glyph_matrix::{GlyphRow, GlyphType};

    // Pass 1: collect the (face, repr char) pairs and composed clusters on
    // screen. Bounded by the number of distinct non-ASCII chars/clusters
    // visible, not by grid size.
    let mut wanted: Vec<(FaceId, char)> = Vec::new();
    let mut seen: std::collections::HashSet<(FaceId, char)> = std::collections::HashSet::new();
    let mut wanted_clusters: Vec<(FaceId, Box<str>)> = Vec::new();
    let mut seen_clusters: std::collections::HashSet<(FaceId, Box<str>)> =
        std::collections::HashSet::new();
    let mut collect_row = |row: &GlyphRow| {
        if !row.enabled {
            return;
        }
        for area in &row.glyphs {
            for glyph in area {
                if glyph.padding {
                    continue;
                }
                let repr = match &glyph.glyph_type {
                    GlyphType::Char { ch } => {
                        if crate::composition::is_composition_joiner(*ch) {
                            continue;
                        }
                        *ch
                    }
                    GlyphType::Composite { text } | GlyphType::AutomaticComposite { text, .. } => {
                        if seen_clusters.insert((glyph.face_id, text.clone())) {
                            wanted_clusters.push((glyph.face_id, text.clone()));
                        }
                        match crate::composition::representative_char_for_cluster(text) {
                            Some(ch) => ch,
                            None => continue,
                        }
                    }
                    _ => continue,
                };
                if seen.insert((glyph.face_id, repr)) {
                    wanted.push((glyph.face_id, repr));
                }
            }
        }
    };
    for entry in &state.window_matrices {
        for row in &entry.matrix.rows {
            collect_row(row);
        }
    }
    for band in state.frame_chrome.bands() {
        if let neomacs_display_protocol::frame_chrome::FrameChromeContent::DisplayRow(content) =
            band.content()
        {
            collect_row(content.row());
        }
    }

    // Pass 2: resolve and publish. Steady state is one cache-hit per pair.
    for (face_id, repr) in wanted {
        if state
            .char_fonts
            .get(&face_id)
            .is_some_and(|by_char| by_char.contains_key(&repr))
        {
            continue;
        }
        let Some(face) = state.faces.get(&face_id) else {
            continue;
        };
        let selection = protocol_face_font_selection(face);
        match svc.select_font_for_realized_face_char(repr, selection) {
            Some(selected) => {
                let Some(glyph_code) = selected.glyph_code else {
                    continue;
                };
                let advance_px = svc.char_width_for_realized_face(repr, selection);
                let font = selected.resolved;
                state.char_fonts.entry(face_id).or_default().insert(
                    repr,
                    neomacs_display_protocol::font::ResolvedCharGlyph {
                        resolved_font_id: font.id,
                        glyph_id: neomacs_display_protocol::font::ResolvedGlyphId::new(glyph_code),
                        advance_px,
                    },
                );
                state.fonts.entry(font.id).or_insert(font);
            }
            None => {
                tracing::trace!(
                    target: "font_boundary",
                    face_id = face_id.get(),
                    ch = %repr,
                    "no per-char fallback font resolved; renderer will re-select"
                );
            }
        }
    }

    // Pass 3: shape composed clusters and publish their exact glyphs so the
    // renderer replays them instead of re-shaping the cluster text.
    for (face_id, text) in wanted_clusters {
        if state
            .shaped_clusters
            .get(&face_id)
            .is_some_and(|by_text| by_text.contains_key(&text))
        {
            continue;
        }
        let Some(face) = state.faces.get(&face_id) else {
            continue;
        };
        let selection = protocol_face_font_selection(face);
        match svc.resolved_glyphs_for_realized_face_cluster(&text, selection) {
            Some((glyphs, fonts)) => {
                for font in fonts {
                    state.fonts.entry(font.id).or_insert(font);
                }
                state
                    .shaped_clusters
                    .entry(face_id)
                    .or_default()
                    .insert(text, glyphs);
            }
            None => {
                tracing::trace!(
                    target: "font_boundary",
                    face_id = face_id.get(),
                    cluster = %text,
                    "cluster did not shape; renderer will re-shape"
                );
            }
        }
    }
}

fn font_slant_kind_from_fontdb(style: Style) -> FontSlantKind {
    match style {
        Style::Normal => FontSlantKind::Normal,
        Style::Italic => FontSlantKind::Italic,
        Style::Oblique => FontSlantKind::Oblique,
    }
}

fn font_slant_from_fontdb(style: Style) -> FontSlant {
    match style {
        Style::Normal => FontSlant::Normal,
        Style::Italic => FontSlant::Italic,
        Style::Oblique => FontSlant::Oblique,
    }
}

fn font_slant_kind_from_platform(slant: FontSlant) -> FontSlantKind {
    match slant {
        FontSlant::Normal => FontSlantKind::Normal,
        FontSlant::Italic | FontSlant::ReverseItalic => FontSlantKind::Italic,
        FontSlant::Oblique | FontSlant::ReverseOblique => FontSlantKind::Oblique,
    }
}

fn font_slant_to_cosmic_style(slant: FontSlant) -> Option<Style> {
    match slant {
        FontSlant::Normal => None,
        FontSlant::Italic | FontSlant::ReverseItalic => Some(Style::Italic),
        FontSlant::Oblique | FontSlant::ReverseOblique => Some(Style::Oblique),
    }
}

#[cfg(test)]
#[path = "metrics_test.rs"]
mod tests;
