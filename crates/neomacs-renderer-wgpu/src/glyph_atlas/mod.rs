//! Glyph texture atlas for wgpu GPU rendering
//!
//! Caches rasterized glyphs on shared atlas texture pages.

pub mod allocator;
mod bitmap_fonts;
pub mod pages;
pub mod types;

pub use types::{
    AnyAtlasEntry, GlyphAtlasError, GlyphAtlasHandle, GlyphMaterialKind, SubpixelRequest,
};

use neomacs_display_protocol::types::FaceId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use cosmic_text::{
    Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Style, SubpixelBin, Weight,
};

use bitmap_fonts::BitmapFontReplayCache;
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::font::{
    CharFontTable, FontCatalogGeneration, FontReplay, FontSlantKind, FrameFontBindings,
    ResolvedFont, ResolvedFontId, ResolvedFontTable, ResolvedGlyph, ShapedClusterTable,
};
use neomacs_font_materializer::FontFileCache;
use neomacs_layout_engine::font::subpixel::{FontconfigSubpixelOrder, default_subpixel_order};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Format, Transform, Vector};

/// A per-glyph render *request*: everything the caller knows about one glyph to
/// draw, including which `face_id` asked for it (used downstream for the
/// default-font-metrics check and colour). This is NOT the atlas cache key --
/// the atlas keys on [`GlyphIdentity`] (via [`GlyphKey::identity`]), which
/// excludes `face_id`. See [`GlyphIdentity`] for why.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GlyphKey {
    /// Character code
    pub charcode: u32,
    /// Face ID that requested this glyph. Carried for `key_uses_default_font_metrics`
    /// and colour resolution; deliberately NOT part of the atlas identity.
    pub face_id: FaceId,
    /// Font size in pixels (for text-scale-increase support)
    /// Using u32 bits of f32 for hashing
    pub font_size_bits: u32,
    /// Realized font identity -- the stable discriminator the atlas keys on.
    ///
    /// Renderer glyph caches live across redisplay frames, while frame face IDs
    /// can be reused for different realized fonts after face remapping changes.
    /// GNU's redisplay keeps realized face metrics and drawing font coupled; this
    /// key keeps Neomacs' persistent atlas coupled to the same font identity.
    pub font_identity: u64,
    /// Subpixel X bin (fractional physical-pixel offset baked into rasterization)
    pub x_bin: SubpixelBin,
    /// Subpixel Y bin (fractional physical-pixel offset baked into rasterization)
    pub y_bin: SubpixelBin,
}

/// The atlas cache identity of a single glyph: exactly the inputs the rasterized
/// mask depends on -- character, font size, realized font, and subpixel bin.
///
/// It deliberately EXCLUDES `face_id`. The mask is a coverage bitmap; the face's
/// colour and decorations (underline/box/strike) are applied later at draw time
/// (vertex colour / separate passes), never baked in. Frame `face_id`s are also
/// remapped per redisplay -- the same visual face gets a different id across
/// frames -- so keying on `face_id` re-rasterized the SAME glyph at the SAME font
/// every frame (~65% miss rate, ~800 re-rasterizations + GPU uploads per frame
/// during scroll). Keying on the stable `font_identity` collapses every face that
/// shares a font+size onto one cached mask, matching GNU (whose font glyph cache
/// is keyed by the font object + glyph code, not the Lisp face).
///
/// Deriving `Hash`/`Eq` here is the safety property: any new field added to the
/// mask identity must be added to this struct, and the compiler then forces
/// [`GlyphKey::identity`] to populate it -- there is no hand-written `Hash`/`Eq`
/// to silently fall out of sync.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct GlyphIdentity {
    pub charcode: u32,
    pub font_size_bits: u32,
    pub font_identity: u64,
    pub x_bin: SubpixelBin,
    pub y_bin: SubpixelBin,
}

impl GlyphKey {
    /// The atlas cache identity for this request (drops `face_id`).
    pub fn identity(&self) -> GlyphIdentity {
        // Exhaustive destructure (no `..`): adding a field to GlyphKey fails to
        // compile HERE until the author classifies it as identity (move it into
        // GlyphIdentity below) or request-only (bind it to `_`, like face_id).
        // This makes the mask-identity contract compiler-enforced in BOTH
        // directions -- a field added where callers construct (GlyphKey) cannot
        // be silently dropped from the cache key.
        let GlyphKey {
            charcode,
            face_id: _,
            font_size_bits,
            font_identity,
            x_bin,
            y_bin,
        } = self;
        GlyphIdentity {
            charcode: *charcode,
            font_size_bits: *font_size_bits,
            font_identity: *font_identity,
            x_bin: *x_bin,
            y_bin: *y_bin,
        }
    }
}

/// A render *request* for a composed (multi-codepoint) glyph -- grapheme
/// clusters like emoji ZWJ sequences or combining diacritics. Like [`GlyphKey`]
/// this is not the cache key; the atlas keys on [`ComposedGlyphIdentity`] (via
/// [`ComposedGlyphKey::identity`]).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ComposedGlyphKey {
    /// The full text of the composed grapheme cluster
    pub text: Box<str>,
    /// Face ID that requested this glyph; not part of the atlas identity.
    pub face_id: FaceId,
    /// Font size in pixels (using u32 bits of f32 for hashing)
    pub font_size_bits: u32,
    /// Realized font identity; see [`GlyphKey::font_identity`].
    pub font_identity: u64,
    /// Exact layout-published font/glyph/position stream, when available.
    pub glyph_stream_identity: Option<ResolvedGlyphStreamIdentity>,
    /// Subpixel X bin (fractional physical-pixel offset baked into rasterization)
    pub x_bin: SubpixelBin,
    /// Subpixel Y bin (fractional physical-pixel offset baked into rasterization)
    pub y_bin: SubpixelBin,
}

/// The atlas cache identity of a composed glyph. Excludes `face_id` for the same
/// reason as [`GlyphIdentity`]; derives `Hash`/`Eq` for the same safety property.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ComposedGlyphIdentity {
    pub text: Box<str>,
    pub font_size_bits: u32,
    pub font_identity: u64,
    pub glyph_stream_identity: Option<ResolvedGlyphStreamIdentity>,
    pub x_bin: SubpixelBin,
    pub y_bin: SubpixelBin,
}

impl ComposedGlyphKey {
    /// The atlas cache identity for this request (drops `face_id`).
    pub fn identity(&self) -> ComposedGlyphIdentity {
        // Exhaustive destructure -- see `GlyphKey::identity`: a new field on
        // ComposedGlyphKey must be classified (into the identity, or bound to `_`)
        // or this fails to compile.
        let ComposedGlyphKey {
            text,
            face_id: _,
            font_size_bits,
            font_identity,
            glyph_stream_identity,
            x_bin,
            y_bin,
        } = self;
        ComposedGlyphIdentity {
            text: text.clone(),
            font_size_bits: *font_size_bits,
            font_identity: *font_identity,
            glyph_stream_identity: *glyph_stream_identity,
            x_bin: *x_bin,
            y_bin: *y_bin,
        }
    }
}

/// Stable identity of one layout-published composed-glyph stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedGlyphStreamIdentity(u64);

pub fn resolved_glyph_stream_identity(glyphs: &[ResolvedGlyph]) -> ResolvedGlyphStreamIdentity {
    let mut hasher = DefaultHasher::new();
    glyphs.len().hash(&mut hasher);
    for glyph in glyphs {
        glyph.resolved_font_id.hash(&mut hasher);
        glyph.glyph_id.hash(&mut hasher);
        glyph.x.to_bits().hash(&mut hasher);
        glyph.y.to_bits().hash(&mut hasher);
        glyph.x_advance.to_bits().hash(&mut hasher);
        glyph.cluster_start.hash(&mut hasher);
        glyph.cluster_end.hash(&mut hasher);
    }
    ResolvedGlyphStreamIdentity(hasher.finish())
}

/// Identity of the complete font projection installed for one frame draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct FrameFontBindingsIdentity(pub(crate) u64);

fn frame_font_bindings_identity(
    faces: &HashMap<FaceId, Face>,
    fonts: &ResolvedFontTable,
    char_fonts: &CharFontTable,
    shaped_clusters: &ShapedClusterTable,
) -> FrameFontBindingsIdentity {
    let mut hasher = DefaultHasher::new();

    let mut face_entries: Vec<_> = faces.iter().collect();
    face_entries.sort_unstable_by_key(|(face_id, _)| face_id.get());
    for (face_id, face) in face_entries {
        face_id.hash(&mut hasher);
        face.default_resolved_font_id.hash(&mut hasher);
    }

    let mut font_entries: Vec<_> = fonts.iter().collect();
    font_entries.sort_unstable_by_key(|(id, _)| **id);
    for (id, font) in font_entries {
        id.hash(&mut hasher);
        font.identity.hash(&mut hasher);
        font.replay.hash(&mut hasher);
        font.family.hash(&mut hasher);
        font.weight.hash(&mut hasher);
        font.slant.hash(&mut hasher);
        font.width.hash(&mut hasher);
        font.pixel_size.to_bits().hash(&mut hasher);
        font.ascent_px.to_bits().hash(&mut hasher);
        font.descent_px.to_bits().hash(&mut hasher);
        font.space_advance_px.to_bits().hash(&mut hasher);
        font.glyph_advance.hash(&mut hasher);
        font.source.hash(&mut hasher);
    }

    let mut char_faces: Vec<_> = char_fonts.iter().collect();
    char_faces.sort_unstable_by_key(|(face_id, _)| face_id.get());
    for (face_id, by_char) in char_faces {
        face_id.hash(&mut hasher);
        let mut chars: Vec<_> = by_char.iter().collect();
        chars.sort_unstable_by_key(|(ch, _)| **ch);
        for (ch, glyph) in chars {
            ch.hash(&mut hasher);
            glyph.resolved_font_id.hash(&mut hasher);
            glyph.glyph_id.hash(&mut hasher);
            glyph.advance_px.to_bits().hash(&mut hasher);
        }
    }

    let mut shaped_faces: Vec<_> = shaped_clusters.iter().collect();
    shaped_faces.sort_unstable_by_key(|(face_id, _)| face_id.get());
    for (face_id, by_text) in shaped_faces {
        face_id.hash(&mut hasher);
        let mut clusters: Vec<_> = by_text.iter().collect();
        clusters.sort_unstable_by_key(|(start, _)| *start);
        for (text, glyphs) in clusters {
            text.hash(&mut hasher);
            resolved_glyph_stream_identity(glyphs).hash(&mut hasher);
        }
    }

    FrameFontBindingsIdentity(hasher.finish())
}

pub fn glyph_font_identity(face: Option<&Face>) -> u64 {
    let Some(face) = face else {
        return 0;
    };

    let mut hasher = DefaultHasher::new();
    face.font_family.hash(&mut hasher);
    face.fontset_base_family.hash(&mut hasher);
    face.font_file_path.hash(&mut hasher);
    face.font_weight.hash(&mut hasher);
    face.font_size.to_bits().hash(&mut hasher);
    face.attributes.bits().hash(&mut hasher);
    // Layout-resolved font identity: two faces with identical request
    // fields but different realized fonts must not share cached glyphs.
    face.default_resolved_font_id.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum GlyphRenderMode {
    Alpha,
    Subpixel,
}

enum SingleCharGlyph {
    Resolved(ResolvedGlyph),
    MissingPrimaryAscii { advance_width: f32 },
}

enum BitmapCharRasterization {
    NotBitmap,
    Rasterized(RasterizeResult),
    Missing { advance_width: f32 },
    Failed,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CachedGlyphKey {
    glyph: GlyphIdentity,
    mode: GlyphRenderMode,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct CachedComposedGlyphKey {
    glyph: ComposedGlyphIdentity,
    mode: GlyphRenderMode,
}

/// Result of rasterizing a glyph or text sequence.
pub struct RasterizeResult {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel data. Alpha masks use R8; color and subpixel glyphs use RGBA.
    pub pixel_data: Vec<u8>,
    /// Bearing X (offset from origin, physical pixels)
    pub bearing_x: f32,
    /// Bearing Y (offset from baseline, physical pixels)
    pub bearing_y: f32,
    /// The single valid interpretation of `pixel_data`.
    pub pixel_kind: GlyphPixelKind,
    /// Horizontal advance width (physical pixels)
    pub advance_width: f32,
    /// Texture filtering required by the realized glyph source.
    pub sampling: neomacs_display_protocol::font::GlyphSampling,
}

/// Mutually exclusive pixel representations produced by glyph rasterization.
///
/// An enum makes invalid states such as “both color and subpixel”
/// unrepresentable at the layout/render boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphPixelKind {
    AlphaMask,
    ColorRgba,
    SubpixelRgba,
}

struct CachedAtlasGlyph {
    entry: AnyAtlasEntry,
    advance_width: f32,
    last_accessed: u64,
}

struct CachedComposedGlyph {
    /// Sampling-homogeneous atlas parts. A mixed bitmap/outline/color cluster
    /// must retain each realized source's texture filtering.
    handles: Vec<GlyphAtlasHandle>,
    last_accessed: u64,
}

/// Rasterize GNU's `font_not_found_p` representation: a one-pixel empty
/// rectangle occupying the missing glyph's full advance and line height.
fn rasterize_missing_glyph_box(
    advance_width: f32,
    line_height: f32,
    ascent: f32,
    scale_factor: f32,
) -> RasterizeResult {
    let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let advance_width = if advance_width.is_finite() && advance_width > 0.0 {
        advance_width
    } else {
        1.0
    };
    let width = (advance_width * scale).round().max(1.0) as u32;
    let height = (line_height * scale).round().max(1.0) as u32;
    let mut pixel_data = vec![0; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            if x == 0 || x + 1 == width || y == 0 || y + 1 == height {
                pixel_data[(y * width + x) as usize] = 255;
            }
        }
    }
    RasterizeResult {
        width,
        height,
        pixel_data,
        bearing_x: 0.0,
        bearing_y: ascent.max(0.0) * scale,
        pixel_kind: GlyphPixelKind::AlphaMask,
        advance_width: advance_width * scale,
        sampling: neomacs_display_protocol::font::GlyphSampling::Linear,
    }
}

/// One rasterized sub-glyph awaiting compositing.
struct SubGlyph {
    bearing_x: f32,
    bearing_y: f32,
    width: u32,
    height: u32,
    pixel_data: Vec<u8>,
    pixel_kind: GlyphPixelKind,
    advance_width: f32,
}

struct SampledSubGlyph {
    glyph: SubGlyph,
    sampling: neomacs_display_protocol::font::GlyphSampling,
}

fn normalize_subpixel_mask(
    mask: &[u8],
    pixel_count: usize,
    order: FontconfigSubpixelOrder,
) -> Vec<u8> {
    if mask.len() == pixel_count * 4 {
        let mut rgba = mask.to_vec();
        if matches!(
            order,
            FontconfigSubpixelOrder::Bgr | FontconfigSubpixelOrder::VBgr
        ) {
            for chunk in rgba.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }
        }
        return rgba;
    }

    if mask.len() == pixel_count * 3 {
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for chunk in mask.chunks_exact(3) {
            let (r, g, b) = if matches!(
                order,
                FontconfigSubpixelOrder::Bgr | FontconfigSubpixelOrder::VBgr
            ) {
                (chunk[2], chunk[1], chunk[0])
            } else {
                (chunk[0], chunk[1], chunk[2])
            };
            let alpha = r.max(g).max(b);
            rgba.extend_from_slice(&[r, g, b, alpha]);
        }
        return rgba;
    }

    tracing::warn!(
        "glyph_atlas: unexpected subpixel mask size: {} bytes for {} pixels",
        mask.len(),
        pixel_count
    );
    mask.to_vec()
}

use pages::{GlyphAtlasPages, PageAllocResult};
use types::*;

/// Wgpu-based glyph atlas for text rendering
pub struct WgpuGlyphAtlas {
    // FxHashMap, not std SipHash: these are looked up once per glyph every
    // frame (95%+ hit rate) with an internal, non-adversarial key -- the
    // per-glyph hash was ~a fifth of the SipHash cost in a Doom scroll profile.
    atlas_cache: FxHashMap<CachedGlyphKey, CachedAtlasGlyph>,
    atlas_composed_cache: FxHashMap<CachedComposedGlyphKey, CachedComposedGlyph>,
    // Keys proven to be ordinary whitespace (no atlas entry by design).
    // Without this memo every space glyph re-ran the missing-primary-ascii
    // probe — two table lookups plus font_system.get_font plus a swash
    // charmap query — on every draw (~thousands per frame). The verdict is
    // stable per key: for ASCII it depends only on the face's
    // default_resolved_font_id, which glyph_font_identity hashes into the
    // key. Cleared with the other caches (resolved-id reuse, metrics, DPI).
    whitespace_skip: FxHashSet<CachedGlyphKey>,
    atlas_pages: GlyphAtlasPages,
    atlas_config: GlyphAtlasConfig,
    font_system: FontSystem,
    scale_context: ScaleContext,
    bind_group_layout: wgpu::BindGroupLayout,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    default_font_size: f32,
    default_line_height: f32,
    scale_factor: f32,
    interned_families: HashSet<&'static str>,
    generation: u64,
    frame_number: u64,
    cached_char_width: Option<f32>,
    cached_font_ascent: Option<f32>,
    font_file_cache: FontFileCache,
    /// Reopens layout-recorded fixed strikes without rerunning font selection.
    bitmap_font_cache: Option<BitmapFontReplayCache>,
    subpixel_order: FontconfigSubpixelOrder,
    pub(crate) cache_hits_this_frame: usize,
    pub(crate) cache_misses_this_frame: usize,
    pub(crate) page_evictions_this_frame: usize,
    /// Monotonic count of UV-invalidating events (page eviction/reset and
    /// whole-atlas clears). Cached vertices embed atlas UVs, so any consumer
    /// caching tessellated output MUST key on this and drop its cache when it
    /// moves (row-reuse's mandatory atlas-generation key).
    eviction_generation: u64,
    /// Layout-resolved font table for the frames this atlas draws, installed
    /// per frame by the render pass. `Face::default_resolved_font_id` indexes
    /// into it; the exact-font text path reads the answer from here instead
    /// of re-running semantic selection.
    frame_fonts: ResolvedFontTable,
    /// Layout-resolved per-character fallback fonts (`face_id → repr char →
    /// font id`), installed alongside `frame_fonts`. Consulted before any
    /// render-side `match_font_for_char`.
    frame_char_fonts: CharFontTable,
    /// Layout-shaped composed clusters (`face_id → cluster text → resolved
    /// glyphs`). Consulted before re-shaping cluster text. Cleared per
    /// render pass like `frame_char_fonts` (face-id keyed).
    frame_shaped_clusters: ShapedClusterTable,
    frame_font_bindings_identity: FrameFontBindingsIdentity,
    /// Last layout catalog generation installed into this atlas.
    font_catalog_generation: Option<FontCatalogGeneration>,
    /// Cache: resolved font identity → this FontSystem's fontdb face id.
    /// Valid for the fontdb's lifetime (fonts are only ever appended by
    /// priming); dropped by [`Self::clear`] with the rest of the caches.
    resolved_fontdb_ids: HashMap<ResolvedFontId, Option<fontdb::ID>>,
    /// Total GUI text lookups whose face had no layout-resolved font and no
    /// font-file bridge — i.e. the renderer had to make a semantic font
    /// decision on its own (design §10 "emergency fallback"). Must stay 0
    /// for normal GUI text.
    unresolved_face_text_total: u64,
    /// Face ids already warned about, so the emergency path logs once per
    /// face instead of per glyph.
    unresolved_face_warned: HashSet<FaceId>,
}

/// Whether the single-char cmap fast path in [`WgpuGlyphAtlas::rasterize_glyph`]
/// is enabled. On by default; set `NEOMACS_GLYPH_FASTPATH=0` to force every
/// glyph through full cosmic-text shaping (for A/B measurement or if a font
/// ever needs isolated-codepoint GSUB substitution). Read once per process.
fn glyph_fast_path_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEOMACS_GLYPH_FASTPATH").as_deref() != Ok("0"))
}

impl WgpuGlyphAtlas {
    /// Create a new wgpu glyph atlas
    pub fn new(device: &wgpu::Device) -> Self {
        // Create bind group layout for glyph texture + sampler
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Glyph Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Linear Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Nearest Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let atlas_config = GlyphAtlasConfig::default_for_device(device);

        Self {
            atlas_cache: FxHashMap::default(),
            atlas_composed_cache: FxHashMap::default(),
            whitespace_skip: FxHashSet::default(),
            atlas_pages: GlyphAtlasPages::new(atlas_config),
            atlas_config,
            // System-font discovery can be expensive. Production drawing
            // binds a layout snapshot before touching this database, so keep
            // construction cheap and perform exactly one scan at that first
            // typed frame boundary.
            font_system: FontSystem::new_with_locale_and_db(
                "en-US".to_owned(),
                fontdb::Database::new(),
            ),
            scale_context: ScaleContext::new(),
            bind_group_layout,
            linear_sampler,
            nearest_sampler,
            default_font_size: 13.0,
            default_line_height: 17.0,
            scale_factor: 1.0,
            interned_families: HashSet::new(),
            generation: 0,
            frame_number: 0,
            cached_char_width: None,
            cached_font_ascent: None,
            font_file_cache: FontFileCache::new(),
            bitmap_font_cache: BitmapFontReplayCache::new().ok(),
            subpixel_order: default_subpixel_order(),
            frame_fonts: ResolvedFontTable::new(),
            frame_char_fonts: CharFontTable::new(),
            frame_shaped_clusters: ShapedClusterTable::new(),
            frame_font_bindings_identity: FrameFontBindingsIdentity::default(),
            font_catalog_generation: None,
            resolved_fontdb_ids: HashMap::new(),
            unresolved_face_text_total: 0,
            unresolved_face_warned: HashSet::new(),
            cache_hits_this_frame: 0,
            cache_misses_this_frame: 0,
            page_evictions_this_frame: 0,
            eviction_generation: 0,
        }
    }

    /// Create a new wgpu glyph atlas with a specific scale factor for HiDPI
    pub fn new_with_scale(device: &wgpu::Device, scale_factor: f32) -> Self {
        let mut atlas = Self::new(device);
        atlas.scale_factor = scale_factor;
        atlas
    }

    /// Get the bind group layout for glyph textures
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Rasterize text (single char or multi-codepoint sequence) and return pixel data.
    ///
    /// Returns a `RasterizeResult` containing pixel data and metrics:
    /// - `AlphaMask`: `pixel_data` is R8 alpha.
    /// - `SubpixelRgba`: `pixel_data` is background-aware RGBA coverage.
    /// - `ColorRgba`: `pixel_data` is an RGBA image.
    ///
    /// Instrumented to debug font resolution mismatches (e.g. weight 700 vs 800)
    /// between requested face attrs and the concrete font selected by cosmic-text.
    #[tracing::instrument(
        level = "trace",
        skip(self, face),
        fields(
            text = %text,
            req_family = tracing::field::Empty,
            req_weight = tracing::field::Empty,
            req_italic = tracing::field::Empty,
            req_size = tracing::field::Empty
        )
    )]
    fn rasterize_text(
        &mut self,
        text: &str,
        face: Option<&Face>,
        x_bin: SubpixelBin,
        y_bin: SubpixelBin,
        enable_subpixel: bool,
    ) -> Option<RasterizeResult> {
        let req_family = face.map(|f| f.font_family.as_str()).unwrap_or("monospace");
        let req_weight = face.map(|f| f.font_weight).unwrap_or(400);
        let req_italic = face
            .map(|f| {
                f.attributes
                    .contains(neomacs_display_protocol::face::FaceAttributes::ITALIC)
            })
            .unwrap_or(false);
        let req_size = effective_font_size(face.map(|f| f.font_size), self.default_font_size);

        let span = tracing::Span::current();
        span.record("req_family", tracing::field::display(req_family));
        span.record("req_weight", tracing::field::display(req_weight));
        span.record("req_italic", tracing::field::display(req_italic));
        span.record("req_size", tracing::field::display(req_size));

        // Create attributes from face
        let attrs = self.face_to_attrs_for_text(text, face);

        // Use font_size from face if available, otherwise default
        let font_size = req_size;

        // Create metrics with the face's font size
        let line_height = font_size * 1.3;
        let metrics = Metrics::new(font_size, line_height);

        // Lay the cluster out on a single unbounded line. A finite width here
        // makes cosmic-text apply line alignment, and the default alignment for
        // an RTL run is to the RIGHT edge of that width — which shoves a shaped
        // Arabic/Hebrew word to the right of its own texture (it then renders
        // displaced from its cell). No width bound = glyphs start at the origin
        // regardless of direction, matching FontMetricsService::shape_run.
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, None, None);
        buffer.set_text(
            &mut self.font_system,
            text,
            &attrs,
            cosmic_text::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // For multi-glyph sequences (e.g. emoji ZWJ), we need to composite
        // all sub-glyphs into a single texture. Collect them first.
        let mut sub_glyphs: Vec<SubGlyph> = Vec::new();

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let advance_w = glyph.w * self.scale_factor;
                let physical_glyph = glyph.physical((0.0, 0.0), self.scale_factor);
                let mut cache_key = physical_glyph.cache_key;
                cache_key.x_bin = x_bin;
                cache_key.y_bin = y_bin;

                // Instrumentation for font resolution: log requested attrs and
                // the concrete selected font face/glyph per shaped glyph.
                if tracing::enabled!(tracing::Level::DEBUG) {
                    let cluster = text.get(glyph.start..glyph.end).unwrap_or("<?>");
                    if let Some(face_info) = self.font_system.db().face(cache_key.font_id) {
                        let selected_family = face_info
                            .families
                            .first()
                            .map(|(name, _lang)| name.as_str())
                            .unwrap_or("<unknown>");
                        tracing::debug!(
                            req_family = %req_family,
                            req_weight = req_weight,
                            req_italic = req_italic,
                            req_size = req_size,
                            glyph_cluster = %cluster,
                            glyph_start = glyph.start,
                            glyph_end = glyph.end,
                            glyph_id = cache_key.glyph_id,
                            glyph_advance = glyph.w,
                            cache_weight = cache_key.font_weight.0,
                            font_id = ?cache_key.font_id,
                            selected_family = %selected_family,
                            selected_postscript = %face_info.post_script_name,
                            selected_weight = face_info.weight.0,
                            selected_style = ?face_info.style,
                            "cosmic selected glyph font"
                        );
                    } else {
                        tracing::debug!(
                            req_family = %req_family,
                            req_weight = req_weight,
                            req_italic = req_italic,
                            req_size = req_size,
                            glyph_cluster = %cluster,
                            glyph_start = glyph.start,
                            glyph_end = glyph.end,
                            glyph_id = cache_key.glyph_id,
                            glyph_advance = glyph.w,
                            cache_weight = cache_key.font_weight.0,
                            font_id = ?cache_key.font_id,
                            "cosmic selected glyph font (face_info missing)"
                        );
                    }
                }

                if let Some(image) = self.render_cache_key_image(cache_key, enable_subpixel) {
                    let width = image.placement.width;
                    let height = image.placement.height;

                    if width == 0 || height == 0 {
                        continue;
                    }

                    // Position the bitmap at the glyph's PEN position within the
                    // run (physical_glyph.x) plus its bitmap left-bearing. Using
                    // only the bearing stacks every glyph of a multi-glyph
                    // composite at the origin, collapsing a shaped Arabic/Indic
                    // run (or base + combining marks) into ~one glyph width.
                    // Single glyphs have pen x == 0, so they are unaffected.
                    let bearing_x = physical_glyph.x as f32 + image.placement.left as f32;
                    let bearing_y = image.placement.top as f32;

                    let font_family_str = face.map(|f| f.font_family.as_str()).unwrap_or("(none)");
                    tracing::debug!(
                        "rasterize_text: text='{}' glyph U+{:04X} font='{}' content={:?} size={}x{}",
                        text,
                        glyph.start,
                        font_family_str,
                        image.content,
                        width,
                        height
                    );

                    let (pixel_data, pixel_kind) =
                        self.image_sub_glyph_payload(&image, width, height, enable_subpixel);

                    sub_glyphs.push(SubGlyph {
                        bearing_x,
                        bearing_y,
                        width,
                        height,
                        pixel_data,
                        pixel_kind,
                        advance_width: advance_w,
                    });
                }
            }
        }

        Self::composite_sub_glyphs(
            sub_glyphs,
            neomacs_display_protocol::font::GlyphSampling::Linear,
        )
    }

    /// Convert a rendered swash image into a sub-glyph payload
    /// (pixel data + color/subpixel classification).
    fn image_sub_glyph_payload(
        &self,
        image: &cosmic_text::SwashImage,
        width: u32,
        height: u32,
        enable_subpixel: bool,
    ) -> (Vec<u8>, GlyphPixelKind) {
        match image.content {
            cosmic_text::SwashContent::Mask => (image.data.clone(), GlyphPixelKind::AlphaMask),
            cosmic_text::SwashContent::Color => (image.data.clone(), GlyphPixelKind::ColorRgba),
            cosmic_text::SwashContent::SubpixelMask => {
                if self.render_mode(enable_subpixel) == GlyphRenderMode::Subpixel {
                    (
                        normalize_subpixel_mask(
                            &image.data,
                            (width as usize) * (height as usize),
                            self.subpixel_order,
                        ),
                        GlyphPixelKind::SubpixelRgba,
                    )
                } else {
                    let alpha: Vec<u8> = image
                        .data
                        .chunks(3)
                        .map(|chunk| {
                            ((chunk[0] as u16 + chunk[1] as u16 + chunk[2] as u16) / 3) as u8
                        })
                        .collect();
                    (alpha, GlyphPixelKind::AlphaMask)
                }
            }
        }
    }

    /// Composite rasterized sub-glyphs into one texture: the shared tail of
    /// `rasterize_text` and `rasterize_resolved_cluster`.
    fn composite_sub_glyphs(
        sub_glyphs: Vec<SubGlyph>,
        sampling: neomacs_display_protocol::font::GlyphSampling,
    ) -> Option<RasterizeResult> {
        if sub_glyphs.is_empty() {
            return None;
        }

        // Single glyph: return directly (common case for single chars and
        // composed emoji that the font renders as a single glyph)
        if sub_glyphs.len() == 1 {
            if let Some(glyph) = sub_glyphs.into_iter().next() {
                return Some(RasterizeResult {
                    width: glyph.width,
                    height: glyph.height,
                    pixel_data: glyph.pixel_data,
                    bearing_x: glyph.bearing_x,
                    bearing_y: glyph.bearing_y,
                    pixel_kind: glyph.pixel_kind,
                    advance_width: glyph.advance_width,
                    sampling,
                });
            } else {
                return None;
            }
        }

        // Multiple sub-glyphs: composite into a single RGBA texture.
        // Find bounding box of all sub-glyphs.
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut any_color = false;
        let mut any_subpixel = false;
        let mut total_advance: f32 = 0.0;

        for glyph in &sub_glyphs {
            min_x = min_x.min(glyph.bearing_x);
            max_x = max_x.max(glyph.bearing_x + glyph.width as f32);
            min_y = min_y.min(-glyph.bearing_y); // bearing_y is distance from baseline (positive = up)
            max_y = max_y.max(-glyph.bearing_y + glyph.height as f32);
            any_color |= glyph.pixel_kind == GlyphPixelKind::ColorRgba;
            any_subpixel |= glyph.pixel_kind == GlyphPixelKind::SubpixelRgba;
            total_advance += glyph.advance_width;
        }

        let total_w = (max_x - min_x).ceil() as u32;
        let total_h = (max_y - min_y).ceil() as u32;

        if total_w == 0 || total_h == 0 {
            return None;
        }

        // All sub-glyphs are plain alpha masks (e.g. an Arabic/Indic shaped
        // run, or a base char with combining marks): composite them into a
        // single-channel COVERAGE MASK so the draw path tints it by the face
        // foreground color, exactly like a single mask glyph. Baking them into
        // an RGBA buffer and flagging it "color" (the mixed-content path below)
        // would render them untinted-white — invisible on a light background.
        if !any_color && !any_subpixel {
            let mut mask = vec![0u8; (total_w * total_h) as usize];
            for glyph in &sub_glyphs {
                let ox = (glyph.bearing_x - min_x).round() as i32;
                let oy = (-glyph.bearing_y - min_y).round() as i32;
                for py in 0..glyph.height {
                    for px in 0..glyph.width {
                        let dx = ox + px as i32;
                        let dy = oy + py as i32;
                        if dx < 0 || dy < 0 || dx >= total_w as i32 || dy >= total_h as i32 {
                            continue;
                        }
                        let src_idx = (py * glyph.width + px) as usize;
                        if src_idx >= glyph.pixel_data.len() {
                            continue;
                        }
                        let sa = glyph.pixel_data[src_idx] as u32;
                        if sa == 0 {
                            continue;
                        }
                        let dst = (dy as u32 * total_w + dx as u32) as usize;
                        let da = mask[dst] as u32;
                        // Alpha-over of coverage (glyphs may overlap at joins).
                        mask[dst] = (sa + da * (255 - sa) / 255) as u8;
                    }
                }
            }
            return Some(RasterizeResult {
                width: total_w,
                height: total_h,
                pixel_data: mask,
                bearing_x: min_x,
                bearing_y: -min_y,
                pixel_kind: GlyphPixelKind::AlphaMask,
                advance_width: total_advance,
                sampling,
            });
        }

        // Composite all sub-glyphs into a single RGBA buffer
        let bpp = 4u32; // always RGBA for composited result
        let mut composite = vec![0u8; (total_w * total_h * bpp) as usize];

        for glyph in &sub_glyphs {
            let ox = (glyph.bearing_x - min_x).round() as i32;
            let oy = (-glyph.bearing_y - min_y).round() as i32;

            for py in 0..glyph.height {
                for px in 0..glyph.width {
                    let dx = ox + px as i32;
                    let dy = oy + py as i32;
                    if dx < 0 || dy < 0 || dx >= total_w as i32 || dy >= total_h as i32 {
                        continue;
                    }
                    let dst_idx = ((dy as u32 * total_w + dx as u32) * bpp) as usize;
                    if glyph.pixel_kind != GlyphPixelKind::AlphaMask {
                        // RGBA source
                        let src_idx = ((py * glyph.width + px) * 4) as usize;
                        if src_idx + 3 < glyph.pixel_data.len() {
                            let sa = glyph.pixel_data[src_idx + 3] as u32;
                            if sa > 0 {
                                // Alpha composite (premultiplied)
                                let da = composite[dst_idx + 3] as u32;
                                let inv_sa = 255 - sa;
                                composite[dst_idx] = ((glyph.pixel_data[src_idx] as u32 * sa
                                    + composite[dst_idx] as u32 * inv_sa)
                                    / 255)
                                    as u8;
                                composite[dst_idx + 1] =
                                    ((glyph.pixel_data[src_idx + 1] as u32 * sa
                                        + composite[dst_idx + 1] as u32 * inv_sa)
                                        / 255) as u8;
                                composite[dst_idx + 2] =
                                    ((glyph.pixel_data[src_idx + 2] as u32 * sa
                                        + composite[dst_idx + 2] as u32 * inv_sa)
                                        / 255) as u8;
                                composite[dst_idx + 3] = (sa + da * inv_sa / 255) as u8;
                            }
                        }
                    } else {
                        // Alpha mask source — treat as white text with alpha
                        let src_idx = (py * glyph.width + px) as usize;
                        if src_idx < glyph.pixel_data.len() {
                            let sa = glyph.pixel_data[src_idx] as u32;
                            if sa > 0 {
                                let da = composite[dst_idx + 3] as u32;
                                let inv_sa = 255 - sa;
                                composite[dst_idx] =
                                    ((255 * sa + composite[dst_idx] as u32 * inv_sa) / 255) as u8;
                                composite[dst_idx + 1] =
                                    ((255 * sa + composite[dst_idx + 1] as u32 * inv_sa) / 255)
                                        as u8;
                                composite[dst_idx + 2] =
                                    ((255 * sa + composite[dst_idx + 2] as u32 * inv_sa) / 255)
                                        as u8;
                                composite[dst_idx + 3] = (sa + da * inv_sa / 255) as u8;
                            }
                        }
                    }
                }
            }
        }

        // For composited result with mixed content, always use color (RGBA)
        Some(RasterizeResult {
            width: total_w,
            height: total_h,
            pixel_data: composite,
            bearing_x: min_x,
            bearing_y: -min_y,
            pixel_kind: if any_color || (sub_glyphs.len() > 1 && !any_subpixel) {
                GlyphPixelKind::ColorRgba
            } else {
                GlyphPixelKind::SubpixelRgba
            },
            advance_width: total_advance,
            sampling,
        })
    }

    /// Composite only adjacent glyphs with the same texture-filtering policy.
    /// Keeping these as separate atlas parts is what lets a fixed monochrome
    /// glyph stay nearest-neighbor while an outline or color neighbor remains
    /// linear inside the same logical composition.
    fn composite_sampled_sub_glyphs(
        sub_glyphs: Vec<SampledSubGlyph>,
    ) -> Option<Vec<RasterizeResult>> {
        let total_advance = sub_glyphs
            .iter()
            .map(|sub| sub.glyph.advance_width)
            .sum::<f32>();
        let mut runs: Vec<(neomacs_display_protocol::font::GlyphSampling, Vec<SubGlyph>)> =
            Vec::new();
        for sub in sub_glyphs {
            if let Some((_, glyphs)) = runs
                .last_mut()
                .filter(|(sampling, _)| *sampling == sub.sampling)
            {
                glyphs.push(sub.glyph);
            } else {
                runs.push((sub.sampling, vec![sub.glyph]));
            }
        }
        let results = runs
            .into_iter()
            .filter_map(|(sampling, glyphs)| {
                Self::composite_sub_glyphs(glyphs, sampling).map(|mut result| {
                    result.advance_width = total_advance;
                    result
                })
            })
            .collect::<Vec<_>>();
        (!results.is_empty()).then_some(results)
    }

    /// FAST PATH for a single, non-composed character. Prefer layout's exact
    /// `(font, glyph, advance)` answer; older/unresolved frame producers fall
    /// back to deriving the glyph id from the exact font's cmap. Hand the
    /// result to [`Self::rasterize_resolved_cluster`], skipping the per-glyph
    /// cosmic-text `Buffer::new` + `shape_until_scroll` that `rasterize_text`
    /// pays on every atlas miss (measured ~6% of scroll CPU: `Buffer::new`
    /// 19% of `rasterize_text`, shaping 6%). This mirrors what Zed/GPUI and the
    /// design's "Phase 3" glyph-level resolved fonts do — carry the glyph id to
    /// rasterization instead of re-shaping.
    ///
    /// Correctness: the font is chosen from the SAME sources the shaping path
    /// uses (`face_to_attrs_for_text`) — the layout's per-char resolved font
    /// (`frame_char_fonts`, populated for CJK/emoji/symbol fallback) first,
    /// then the face's primary resolved font — so the concrete font matches.
    /// For non-ASCII, a cmap miss returns `None` so emergency shaping can find
    /// a covering fallback. ASCII is different: GNU `face_for_char` keeps it
    /// on the realized primary face without a coverage lookup, and xdisp draws
    /// an empty rectangle when that face lacks the glyph. Preserve that policy
    /// as [`SingleCharGlyph::MissingPrimaryAscii`] instead of silently changing
    /// fonts. Isolated single codepoints in the resolved font take no GSUB
    /// context substitution, so the cmap glyph id equals the shaped one.
    /// Composed clusters never reach here (they route through
    /// `frame_shaped_clusters`/`rasterize_resolved_cluster` already).
    fn try_fast_single_char_glyph(
        &mut self,
        c: char,
        face: Option<&Face>,
    ) -> Option<SingleCharGlyph> {
        let face = face?;
        if let Some(published) = self.resolved_glyph_for_char(c, face) {
            return Some(SingleCharGlyph::Resolved(ResolvedGlyph {
                resolved_font_id: published.resolved_font_id,
                glyph_id: published.glyph_id,
                x: 0.0,
                y: 0.0,
                x_advance: published.advance_px,
                cluster_start: 0,
                cluster_end: c.len_utf8() as u32,
            }));
        }
        let resolved_font = self.resolved_font_for_char(c, face)?;
        let resolved_font_id = resolved_font.id;
        let weight = resolved_font.weight;
        let published_space_advance = resolved_font.space_advance_px;
        let published_glyph_advance = resolved_font.glyph_advance;
        let font_id = self.local_fontdb_id_for(resolved_font_id)?;
        // Use the exact font instance `render_cache_key_image` will rasterize
        // (same id + weight), so the glyph id is guaranteed to belong to it.
        let font = self.font_system.get_font(font_id, fontdb::Weight(weight))?;
        let swash = font.as_swash();
        let glyph_id = swash.charmap().map(c);
        if glyph_id == 0 {
            if c.is_ascii() {
                let font_size = effective_font_size(Some(face.font_size), self.default_font_size);
                let advance_width =
                    if published_space_advance.is_finite() && published_space_advance > 0.0 {
                        published_space_advance
                    } else {
                        swash.glyph_metrics(&[]).scale(font_size).advance_width(0)
                    };
                return Some(SingleCharGlyph::MissingPrimaryAscii { advance_width });
            }
            return None;
        }
        // Logical-pixel advance (pre scale_factor, matching the ResolvedGlyph
        // contract). The main text path positions glyphs from the layout, but
        // the atlas advance feeds the ui-overlay width path (ui_overlays.rs), so
        // fill it from the font's own hmetrics rather than leaving it zero.
        let font_size = effective_font_size(Some(face.font_size), self.default_font_size);
        let x_advance = swash
            .glyph_metrics(&[])
            .scale(font_size)
            .advance_width(glyph_id);
        Some(SingleCharGlyph::Resolved(ResolvedGlyph {
            resolved_font_id,
            glyph_id: glyph_id.into(),
            x: 0.0,
            y: 0.0,
            x_advance: published_glyph_advance.resolve(x_advance),
            cluster_start: 0,
            cluster_end: c.len_utf8() as u32,
        }))
    }

    fn rasterize_missing_primary_ascii(&self, advance_width: f32, face: &Face) -> RasterizeResult {
        let line_height = match face.font_ascent + face.font_descent {
            height if height > 0 => height as f32,
            _ => self.default_line_height,
        };
        let ascent = if face.font_ascent > 0 {
            face.font_ascent as f32
        } else {
            line_height * 0.8
        };
        rasterize_missing_glyph_box(advance_width, line_height, ascent, self.scale_factor)
    }

    fn try_rasterize_bitmap_char(
        &mut self,
        c: char,
        face: Option<&Face>,
    ) -> BitmapCharRasterization {
        let published = face.and_then(|face| self.resolved_glyph_for_char(c, face));
        let Some(font) = face
            .and_then(|face| {
                published
                    .and_then(|glyph| self.frame_fonts.get(&glyph.resolved_font_id))
                    .or_else(|| self.resolved_font_for_char(c, face))
            })
            .filter(|font| matches!(&font.replay, FontReplay::FreeTypeBitmap { .. }))
            .cloned()
        else {
            return BitmapCharRasterization::NotBitmap;
        };
        let Some(cache) = self.bitmap_font_cache.as_mut() else {
            tracing::warn!(
                target: "font_boundary",
                identity = %font.identity.stable_key,
                "renderer bitmap materializer is unavailable"
            );
            return BitmapCharRasterization::Failed;
        };
        let rasterized = match published {
            Some(glyph) => cache.rasterize_glyph(&font, glyph.glyph_id).map(Some),
            None => cache.rasterize_char(&font, c),
        };
        match rasterized {
            Ok(Some(glyph)) => BitmapCharRasterization::Rasterized(glyph),
            Ok(None) => BitmapCharRasterization::Missing {
                advance_width: font.space_advance_px,
            },
            Err(error) => {
                tracing::warn!(
                    target: "font_boundary",
                    identity = %font.identity.stable_key,
                    ?error,
                    "renderer could not replay layout's exact bitmap font"
                );
                BitmapCharRasterization::Failed
            }
        }
    }

    /// Rasterize a single glyph and return pixel data (convenience wrapper)
    fn rasterize_glyph(
        &mut self,
        c: char,
        face: Option<&Face>,
        x_bin: SubpixelBin,
        y_bin: SubpixelBin,
        enable_subpixel: bool,
    ) -> Option<RasterizeResult> {
        match self.try_rasterize_bitmap_char(c, face) {
            BitmapCharRasterization::Rasterized(glyph) => return Some(glyph),
            BitmapCharRasterization::Missing { advance_width } if c.is_ascii() => {
                return Some(self.rasterize_missing_primary_ascii(advance_width, face?));
            }
            BitmapCharRasterization::Missing { .. } | BitmapCharRasterization::Failed => {
                return None;
            }
            BitmapCharRasterization::NotBitmap => {}
        }
        let fast_path_enabled = glyph_fast_path_enabled();
        // ASCII coverage policy is correctness, not an optimization: even
        // when the cmap fast path is disabled for A/B testing, inspect the
        // primary face so a missing glyph cannot escape into fallback shaping.
        if c.is_ascii() || fast_path_enabled {
            match self.try_fast_single_char_glyph(c, face) {
                Some(SingleCharGlyph::Resolved(glyph)) if fast_path_enabled => {
                    let font_size =
                        effective_font_size(face.map(|f| f.font_size), self.default_font_size);
                    return self
                        .rasterize_resolved_cluster(
                            &[glyph],
                            font_size,
                            x_bin,
                            y_bin,
                            enable_subpixel,
                        )
                        .and_then(|mut parts| (parts.len() == 1).then(|| parts.remove(0)));
                }
                Some(SingleCharGlyph::Resolved(_)) => {}
                Some(SingleCharGlyph::MissingPrimaryAscii { advance_width }) => {
                    let face = face?;
                    return Some(self.rasterize_missing_primary_ascii(advance_width, face));
                }
                None => {}
            }
        }
        self.rasterize_text(&c.to_string(), face, x_bin, y_bin, enable_subpixel)
    }

    fn render_cache_key_image(
        &mut self,
        cache_key: cosmic_text::CacheKey,
        enable_subpixel: bool,
    ) -> Option<cosmic_text::SwashImage> {
        let render_mode = self.render_mode(enable_subpixel);
        let font = self
            .font_system
            .get_font(cache_key.font_id, cache_key.font_weight)?;

        let variable_width = font
            .as_swash()
            .variations()
            .find_by_tag(swash::Tag::from_be_bytes(*b"wght"));

        let mut scaler = self
            .scale_context
            .builder(font.as_swash())
            .size(f32::from_bits(cache_key.font_size_bits))
            .hint(!cache_key.flags.contains(CacheKeyFlags::DISABLE_HINTING));
        if let Some(variation) = variable_width {
            scaler = scaler.variations(std::iter::once(swash::Setting {
                tag: swash::Tag::from_be_bytes(*b"wght"),
                value: f32::from(cache_key.font_weight.0)
                    .clamp(variation.min_value(), variation.max_value()),
            }));
        }
        let mut scaler = scaler.build();

        let offset = if cache_key.flags.contains(CacheKeyFlags::PIXEL_FONT) {
            Vector::new(
                cache_key.x_bin.as_float().round() + 1.0,
                cache_key.y_bin.as_float().round(),
            )
        } else {
            Vector::new(cache_key.x_bin.as_float(), cache_key.y_bin.as_float())
        };

        let format = if render_mode == GlyphRenderMode::Subpixel {
            Format::Subpixel
        } else {
            Format::Alpha
        };

        Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        .format(format)
        .offset(offset)
        .transform(if cache_key.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
            Some(Transform::skew(
                Angle::from_degrees(14.0),
                Angle::from_degrees(0.0),
            ))
        } else {
            None
        })
        .render(&mut scaler, cache_key.glyph_id)
    }

    /// Reset the per-character table at the start of a render pass.
    ///
    /// `frame_char_fonts` is keyed by frame face id, and face ids can be
    /// reused for DIFFERENT realized faces across frames, so entries must
    /// not outlive the render pass that installed them. `frame_fonts` is
    /// keyed by identity-stable ids and persists.
    pub fn begin_frame_fonts(&mut self) {
        self.frame_char_fonts.clear();
        self.frame_shaped_clusters.clear();
        self.frame_font_bindings_identity = FrameFontBindingsIdentity::default();
    }

    /// Install a frame's layout-resolved font tables before its text draws.
    ///
    /// Font ids are interned by the layout-side resolver and stable across
    /// frames, so entries are upserted (a re-sent id always carries the same
    /// identity; overwriting also heals a hypothetical resolver restart).
    /// Call [`Self::set_current_frame_fonts`] at frame draw boundaries; the
    /// face-id-keyed tables are only valid for the current frame.
    pub(crate) fn install_frame_fonts(
        &mut self,
        faces: &HashMap<FaceId, Face>,
        fonts: &ResolvedFontTable,
        char_fonts: &CharFontTable,
        shaped_clusters: &ShapedClusterTable,
    ) {
        if fonts.iter().any(|(id, incoming)| {
            self.frame_fonts
                .get(id)
                .is_some_and(|existing| existing != incoming)
        }) {
            // ResolvedFontId is stable within one layout resolver. If a
            // resolver restart reuses an id, every cache that hashed or mapped
            // that id belongs to the old identity and must be invalidated
            // before the replacement becomes visible.
            tracing::warn!(
                target: "font_boundary",
                "resolved font id was reused for a different realized instance; clearing renderer font caches"
            );
            self.clear();
        }
        for (id, font) in fonts {
            self.frame_fonts.insert(*id, font.clone());
        }
        for (face_id, by_char) in char_fonts {
            let entry = self.frame_char_fonts.entry(*face_id).or_default();
            for (ch, glyph) in by_char {
                entry.entry(*ch).or_insert(*glyph);
            }
        }
        for (face_id, by_text) in shaped_clusters {
            let entry = self.frame_shaped_clusters.entry(*face_id).or_default();
            for (text, glyphs) in by_text {
                entry.entry(text.clone()).or_insert_with(|| glyphs.clone());
            }
        }
        self.frame_font_bindings_identity =
            frame_font_bindings_identity(faces, fonts, char_fonts, shaped_clusters);
    }

    /// Replace face-id-keyed font bindings with the frame currently being
    /// drawn.
    ///
    /// `frame_fonts` itself is keyed by stable resolved-font ids and can
    /// accumulate safely within one catalog generation. `frame_char_fonts` and
    /// `frame_shaped_clusters` are keyed by frame-local face ids, so drawing a
    /// child frame must not inherit the parent's bindings for the same numeric
    /// face id.
    pub fn set_current_frame_fonts(&mut self, bindings: FrameFontBindings<'_>) {
        self.synchronize_font_catalog_generation(bindings.catalog_generation);
        self.begin_frame_fonts();
        self.install_frame_fonts(
            bindings.faces,
            bindings.fonts,
            bindings.char_fonts,
            bindings.shaped_clusters,
        );
    }

    fn synchronize_font_catalog_generation(&mut self, incoming: FontCatalogGeneration) {
        match self.font_catalog_generation.replace(incoming) {
            Some(previous) if previous == incoming => return,
            Some(previous) => {
                tracing::info!(
                    target: "font_catalog",
                    previous = previous.get(),
                    current = incoming.get(),
                    "rebuilding renderer font state for native catalog change"
                );
            }
            None => {
                // The render thread constructs its FontSystem independently
                // from layout. Rebuild on the first bound frame too, closing
                // the race where an OS catalog change lands between those two
                // snapshots even when layout still labels its first frame as
                // the initial generation.
                tracing::debug!(
                    target: "font_catalog",
                    current = incoming.get(),
                    "binding renderer font state to its first frame catalog"
                );
            }
        }
        self.clear();
        self.font_system = FontSystem::new();
        self.font_file_cache = FontFileCache::new();
        self.bitmap_font_cache = BitmapFontReplayCache::new().ok();
        self.frame_fonts.clear();
        self.frame_char_fonts.clear();
        self.frame_shaped_clusters.clear();
        self.unresolved_face_warned.clear();
        self.subpixel_order = default_subpixel_order();
    }

    /// Total emergency (unresolved-face) text lookups so far; see field doc.
    pub fn unresolved_face_text_total(&self) -> u64 {
        self.unresolved_face_text_total
    }

    fn face_resolved_font(&self, face: &Face) -> Option<&ResolvedFont> {
        face.default_resolved_font_id
            .and_then(|id| self.frame_fonts.get(&id))
    }

    fn resolved_font_for_char(&self, c: char, face: &Face) -> Option<&ResolvedFont> {
        let id = self
            .frame_char_fonts
            .get(&face.id)
            .and_then(|by_char| by_char.get(&c))
            .map(|glyph| glyph.resolved_font_id)
            .or(face.default_resolved_font_id)?;
        self.frame_fonts.get(&id)
    }

    fn resolved_glyph_for_char(
        &self,
        c: char,
        face: &Face,
    ) -> Option<neomacs_display_protocol::font::ResolvedCharGlyph> {
        self.frame_char_fonts
            .get(&face.id)
            .and_then(|by_char| by_char.get(&c).copied())
    }

    /// Stable raster identity for a visible character. Layout's exact
    /// font/glyph answer participates directly, so row reuse and atlas lookup
    /// cannot hit a mask produced for an older fallback binding.
    pub fn glyph_font_identity_for_char(&self, face: Option<&Face>, c: char) -> u64 {
        let mut hasher = DefaultHasher::new();
        glyph_font_identity(face).hash(&mut hasher);
        if let Some(face) = face
            && let Some(glyph) = self.resolved_glyph_for_char(c, face)
        {
            glyph.resolved_font_id.hash(&mut hasher);
            glyph.glyph_id.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn glyph_stream_identity_for_composed(
        &self,
        face: Option<&Face>,
        text: &str,
    ) -> Option<ResolvedGlyphStreamIdentity> {
        let face = face?;
        self.frame_shaped_clusters
            .get(&face.id)?
            .get(text)
            .map(|glyphs| resolved_glyph_stream_identity(glyphs))
    }

    pub(crate) fn frame_font_bindings_identity(&self) -> FrameFontBindingsIdentity {
        self.frame_font_bindings_identity
    }

    /// Build cosmic Attrs that replay a layout-resolved font verbatim: the
    /// same `select_cosmic_family` family mapping and the already-effective
    /// weight the layout probe used, so cosmic-text picks the identical
    /// fontdb face the layout metrics came from. Primes the identity's font
    /// file first. No fontconfig, no weight re-resolution: the render thread
    /// makes no semantic font decision here.
    fn pin_outline_as_family(
        &mut self,
        asset: &neomacs_display_protocol::font::FontOutlineAsset,
    ) -> Option<&'static str> {
        self.font_file_cache
            .pin_exact_asset(&mut self.font_system, asset)
            .ok()
            .map(neomacs_font_materializer::PinnedFontFace::family)
    }

    fn exact_attrs_for_resolved_font(&mut self, font: &ResolvedFont) -> Option<Attrs<'static>> {
        let FontReplay::Swash { asset } = &font.replay else {
            return None;
        };
        // Every resolved file face is exact, not merely variable fonts: TTC
        // faces and same-family static files are equally capable of being
        // re-selected incorrectly by semantic attributes.
        let synthetic = self.pin_outline_as_family(asset)?;
        let mut attrs = Attrs::new()
            .family(Family::Name(synthetic))
            .weight(Weight(font.weight));
        attrs = match font.slant {
            FontSlantKind::Normal => attrs,
            FontSlantKind::Italic => attrs.style(Style::Italic),
            FontSlantKind::Oblique => attrs.style(Style::Oblique),
        };
        Some(attrs)
    }

    fn record_emergency_font_fallback(&mut self, face: &Face, reason: &'static str) {
        self.unresolved_face_text_total += 1;
        if self.unresolved_face_warned.insert(face.id) {
            tracing::warn!(
                target: "font_boundary",
                face_id = face.id.get(),
                family = %face.font_family,
                weight = face.font_weight,
                lisp_name = face.lisp_name.as_deref().unwrap_or(""),
                reason,
                "resolved GUI font could not be replayed; using emergency font fallback"
            );
        }
    }

    fn intern_family(&mut self, family: &str) -> &'static str {
        if let Some(&existing) = self.interned_families.get(family) {
            existing
        } else {
            let leaked: &'static str = Box::leak(family.to_string().into_boxed_str());
            self.interned_families.insert(leaked);
            leaked
        }
    }

    fn face_to_attrs_for_text(&mut self, text: &str, face: Option<&Face>) -> Attrs<'static> {
        let mut attrs = Attrs::new();

        if let Some(f) = face {
            let requested_italic = f
                .attributes
                .contains(neomacs_display_protocol::face::FaceAttributes::ITALIC);
            let resolved = self.face_resolved_font(f).cloned();

            // Prime the exact font file in fontdb, but keep the family name that
            // Fontconfig/Emacs already selected so TTC collections stay stable.
            // The layout-resolved identity wins; `font_file_path` is the C-FFI
            // bridge for faces realized outside the Rust layout engine.
            let prime_path = match resolved.as_ref() {
                Some(font) => font
                    .replay
                    .file_asset()
                    .map(|asset| asset.path().to_owned()),
                None => f.font_file_path.clone(),
            };
            if let Some(ref path) = prime_path {
                let _ = self.font_file_cache.prime_file(&mut self.font_system, path);
            }
            let mut effective_family = f.font_family.clone();
            let mut effective_weight = f.font_weight;
            let mut effective_style = if requested_italic {
                Some(Style::Italic)
            } else {
                None
            };

            // Choose the font by what the cluster's presentation actually
            // requires, not by a selector char's own font.
            //
            // Representative-char policy shared with the layout side (emoji
            // presentation via U+FE0F, else first glyph-bearing non-ASCII
            // char) so both threads make the same fallback decision.
            let repr_char =
                neomacs_layout_engine::composition::representative_char_for_cluster(text);
            if let Some(ch) = repr_char {
                // Exact layout bindings normally return below. If one is
                // missing or cannot be materialized, preserve GNU's split
                // realized-face semantics: non-ASCII fallback starts from
                // the base fontset, never from an inline ASCII-only family.
                effective_family = f.fontset_base_family_or_primary().to_owned();
                // Layout-resolved per-char fallback: replay the exact font
                // the measurement pass selected for this (face, char).
                if let Some(font) = self
                    .frame_char_fonts
                    .get(&f.id)
                    .and_then(|by_char| by_char.get(&ch))
                    .and_then(|binding| self.frame_fonts.get(&binding.resolved_font_id))
                    .cloned()
                {
                    if let Some(attrs) = self.exact_attrs_for_resolved_font(&font) {
                        return attrs;
                    }
                    effective_family = font.family;
                    effective_weight = font.weight;
                    effective_style = match font.slant {
                        FontSlantKind::Normal => None,
                        FontSlantKind::Italic => Some(Style::Italic),
                        FontSlantKind::Oblique => Some(Style::Oblique),
                    };
                    self.record_emergency_font_fallback(f, "exact char font is not openable");
                }
                // Diagnosed boundary violation for a character whose exact
                // layout-selected font was absent or could not be replayed.
                // Preserve the already published base family for a
                // deterministic best-effort glyph. The renderer must never
                // repeat semantic platform selection: doing so could produce
                // glyph IDs from a different face than layout measured.
                tracing::trace!(
                    target: "font_boundary",
                    face_id = f.id.get(),
                    family = %effective_family,
                    ch = %ch,
                    "render-side exact font missing"
                );
            } else if let Some(font) = resolved.as_ref().cloned() {
                // Exact face-primary path: no per-char fallback needed and
                // layout resolved this face's font — replay it verbatim.
                if let Some(attrs) = self.exact_attrs_for_resolved_font(&font) {
                    return attrs;
                }
                effective_family = font.family;
                effective_weight = font.weight;
                effective_style = match font.slant {
                    FontSlantKind::Normal => None,
                    FontSlantKind::Italic => Some(Style::Italic),
                    FontSlantKind::Oblique => Some(Style::Oblique),
                };
                self.record_emergency_font_fallback(f, "exact primary font is not openable");
            }

            if resolved.is_none() && prime_path.is_none() {
                // Emergency fallback: GUI text reached the render thread with
                // no resolved font identity and no font-file bridge, so the
                // semantic selection below is the renderer's own decision
                // (design §10). Normal GUI text must never take this path.
                self.record_emergency_font_fallback(f, "layout published no font identity");
            }

            attrs = match neomacs_layout_engine::font::font_match::select_cosmic_family(
                &self.font_system,
                &effective_family,
            ) {
                neomacs_layout_engine::font::font_match::CosmicFamilySelection::Name(family) => {
                    let interned = self.intern_family(family);
                    attrs.family(Family::Name(interned))
                }
                neomacs_layout_engine::font::font_match::CosmicFamilySelection::Monospace => {
                    attrs.family(Family::Monospace)
                }
                neomacs_layout_engine::font::font_match::CosmicFamilySelection::Serif => {
                    attrs.family(Family::Serif)
                }
                neomacs_layout_engine::font::font_match::CosmicFamilySelection::SansSerif => {
                    attrs.family(Family::SansSerif)
                }
            };

            // Font weight: clamp to the closest available weight in this family,
            // so missing weights stay within-family instead of jumping to unrelated
            // common fallback families.
            let effective_weight =
                neomacs_layout_engine::font::font_match::resolve_weight_in_family(
                    &self.font_system,
                    &effective_family,
                    effective_weight,
                    effective_style.is_some(),
                );
            if effective_weight != f.font_weight || effective_style.is_some() != requested_italic {
                tracing::debug!(
                    "font normalize: family='{}' requested_weight={} resolved_weight={} requested_italic={} resolved_style={:?}",
                    effective_family,
                    f.font_weight,
                    effective_weight,
                    requested_italic,
                    effective_style
                );
            }
            attrs = attrs.weight(Weight(effective_weight));

            // Font style (italic)
            if let Some(style) = effective_style {
                attrs = attrs.style(style);
            }
        } else {
            // An unstyled renderer-only path has no semantic request to
            // resolve. Use cosmic's deterministic generic without consulting
            // any platform catalog.
            attrs = attrs.family(Family::Monospace);
        }

        attrs
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        tracing::debug!(
            target: "glyph_atlas_invalidate",
            single = self.atlas_cache.len(),
            composed = self.atlas_composed_cache.len(),
            "atlas clear(): all cached glyphs dropped"
        );
        self.atlas_cache.clear();
        self.atlas_composed_cache.clear();
        self.whitespace_skip.clear();
        self.atlas_pages.clear();
        self.cached_char_width = None;
        self.cached_font_ascent = None;
        self.resolved_fontdb_ids.clear();
        // A missing/replaced platform font can become available after the
        // render-side generation reset. Keep successful fontdb registrations,
        // but let the shared materializer retry only its failed observations.
        self.font_file_cache.retry_failed_exact_faces();
        self.eviction_generation = self.eviction_generation.wrapping_add(1);
    }

    /// Monotonic generation that advances whenever any atlas UVs may have
    /// been invalidated (page eviction/reset, atlas clear).
    pub fn eviction_generation(&self) -> u64 {
        self.eviction_generation
    }

    /// Revalidate a previously returned entry and pin its page for this
    /// frame. Returns false when the entry's page was evicted/reset since the
    /// entry was created — its UVs are stale and any cached vertices built
    /// from it must be re-tessellated.
    pub fn revalidate_and_pin(&mut self, entry: AnyAtlasEntry) -> bool {
        self.pin_entry_page(entry).is_ok()
    }

    /// Map a layout-resolved font identity to this FontSystem's own fontdb
    /// face id, priming the font file if needed. Layout-side fontdb ids are
    /// generation-local to the LAYOUT FontSystem and never cross the
    /// boundary; the durable identity (file path + face index) does.
    fn local_fontdb_id_for(&mut self, resolved_font_id: ResolvedFontId) -> Option<fontdb::ID> {
        if let Some(&cached) = self.resolved_fontdb_ids.get(&resolved_font_id) {
            return cached;
        }
        let Some(font) = self.frame_fonts.get(&resolved_font_id).cloned() else {
            // Not cached: the table entry may arrive with a later frame.
            return None;
        };
        let FontReplay::Swash { asset } = &font.replay else {
            self.resolved_fontdb_ids.insert(resolved_font_id, None);
            return None;
        };
        let found = self
            .font_file_cache
            .pin_exact_asset(&mut self.font_system, asset)
            .ok()
            .map(neomacs_font_materializer::PinnedFontFace::fontdb_id);
        self.resolved_fontdb_ids.insert(resolved_font_id, found);
        found
    }

    /// Rasterize a layout-shaped cluster from its exact (font, glyph id)
    /// pairs — no shaping, no font selection on the render thread. Positions
    /// and advances arrive in logical pixels and are scaled here.
    fn rasterize_resolved_cluster(
        &mut self,
        glyphs: &[ResolvedGlyph],
        font_size: f32,
        x_bin: SubpixelBin,
        y_bin: SubpixelBin,
        enable_subpixel: bool,
    ) -> Option<Vec<RasterizeResult>> {
        if glyphs.is_empty() {
            return None;
        }
        let scale = self.scale_factor;
        let mut sub_glyphs: Vec<SampledSubGlyph> = Vec::new();
        for glyph in glyphs {
            let font = self.frame_fonts.get(&glyph.resolved_font_id)?.clone();
            match &font.replay {
                FontReplay::Swash { .. } => {
                    let font_id = self.local_fontdb_id_for(glyph.resolved_font_id)?;
                    let cache_key = cosmic_text::CacheKey {
                        font_id,
                        glyph_id: glyph.glyph_id.as_u16()?,
                        font_size_bits: (font_size * scale).to_bits(),
                        x_bin,
                        y_bin,
                        font_weight: fontdb::Weight(font.weight),
                        flags: CacheKeyFlags::empty(),
                    };
                    let image = self.render_cache_key_image(cache_key, enable_subpixel)?;
                    let width = image.placement.width;
                    let height = image.placement.height;
                    if width == 0 || height == 0 {
                        continue;
                    }
                    let pen_x = (glyph.x * scale).round();
                    let bearing_x = pen_x + image.placement.left as f32;
                    let bearing_y = (glyph.y * scale).round() + image.placement.top as f32;
                    let (pixel_data, pixel_kind) =
                        self.image_sub_glyph_payload(&image, width, height, enable_subpixel);
                    sub_glyphs.push(SampledSubGlyph {
                        glyph: SubGlyph {
                            bearing_x,
                            bearing_y,
                            width,
                            height,
                            pixel_data,
                            pixel_kind,
                            advance_width: glyph.x_advance * scale,
                        },
                        sampling: neomacs_display_protocol::font::GlyphSampling::Linear,
                    });
                }
                FontReplay::FreeTypeBitmap { .. } => {
                    let image = self
                        .bitmap_font_cache
                        .as_mut()?
                        .rasterize_glyph(&font, glyph.glyph_id)
                        .ok()?;
                    if image.width == 0 || image.height == 0 {
                        continue;
                    }
                    sub_glyphs.push(SampledSubGlyph {
                        glyph: SubGlyph {
                            bearing_x: (glyph.x * scale).round() + image.bearing_x,
                            bearing_y: (glyph.y * scale).round() + image.bearing_y,
                            width: image.width,
                            height: image.height,
                            pixel_data: image.pixel_data,
                            pixel_kind: image.pixel_kind,
                            advance_width: glyph.x_advance * scale,
                        },
                        sampling: image.sampling,
                    });
                }
            }
        }
        Self::composite_sampled_sub_glyphs(sub_glyphs)
    }

    /// Rasterize a composed cluster from the layout-published shaped table,
    /// if this (face, text) was shaped layout-side.
    fn rasterize_shaped_cluster_if_published(
        &mut self,
        text: &str,
        face: Option<&Face>,
        x_bin: SubpixelBin,
        y_bin: SubpixelBin,
        enable_subpixel: bool,
    ) -> Option<Vec<RasterizeResult>> {
        let face = face?;
        let glyphs = self.frame_shaped_clusters.get(&face.id)?.get(text)?.clone();
        let font_size = effective_font_size(Some(face.font_size), self.default_font_size);
        self.rasterize_resolved_cluster(&glyphs, font_size, x_bin, y_bin, enable_subpixel)
    }

    /// Update the scale factor and clear the cache so glyphs are
    /// re-rasterized at the new DPI.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        if (self.scale_factor - scale_factor).abs() > 0.001 {
            self.scale_factor = scale_factor;
            self.atlas_cache.clear();
            self.atlas_composed_cache.clear();
            self.whitespace_skip.clear();
            self.atlas_pages.clear();
            tracing::info!(
                "Glyph atlas: scale factor -> {}, cache cleared",
                scale_factor
            );
        }
    }

    /// Get the number of cached glyphs
    pub fn len(&self) -> usize {
        self.atlas_cache.len() + self.atlas_composed_cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.atlas_cache.is_empty() && self.atlas_composed_cache.is_empty()
    }

    /// Get the default font size
    pub fn default_font_size(&self) -> f32 {
        self.default_font_size
    }

    /// Get the default line height
    pub fn default_line_height(&self) -> f32 {
        self.default_line_height
    }

    /// Get the default font's character width (logical pixels).
    /// Measured from the first rasterized face_id=0 glyph.
    /// Falls back to `font_size * 0.6` until a glyph is rasterized.
    pub fn default_char_width(&self) -> f32 {
        self.cached_char_width
            .unwrap_or(self.default_font_size * 0.6)
    }

    /// Get the default font's ascent (logical pixels).
    /// Measured from the first rasterized face_id=0 glyph.
    /// Falls back to `font_size * 0.8` until a glyph is rasterized.
    pub fn default_font_ascent(&self) -> f32 {
        self.cached_font_ascent
            .unwrap_or(self.default_font_size * 0.8)
    }

    /// Set font metrics
    pub fn set_metrics(&mut self, font_size: f32, line_height: f32) {
        if (self.default_font_size - font_size).abs() > 0.1
            || (self.default_line_height - line_height).abs() > 0.1
        {
            // A metrics change nukes EVERY cached glyph. That is correct when
            // the user changes font size; it is a silent performance disaster
            // when two render paths alternate slightly different metrics.
            // Log the transition so a periodic full re-rasterization is
            // attributable instead of invisible (a typing profile once showed
            // every glyph re-rasterized 48x at steady state with no log line
            // pointing here).
            tracing::debug!(
                target: "glyph_atlas_invalidate",
                old_size = self.default_font_size,
                new_size = font_size,
                old_line_height = self.default_line_height,
                new_line_height = line_height,
                "atlas cleared: default metrics changed"
            );
            self.default_font_size = font_size;
            self.default_line_height = line_height;
            // Clear cache when metrics change
            self.clear();
        }
    }

    /// Advance the frame generation counter.
    /// Call once per frame before rendering.
    /// Also evicts stale composed glyphs (not accessed for 60+ frames).
    pub fn advance_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.frame_number = self.frame_number.wrapping_add(1);
        self.cache_hits_this_frame = 0;
        self.cache_misses_this_frame = 0;
        self.page_evictions_this_frame = 0;
        self.atlas_pages.begin_frame();
        if self.atlas_composed_cache.len() > 1024 {
            let cutoff = self.generation.saturating_sub(60);
            self.atlas_composed_cache
                .retain(|_, v| v.last_accessed >= cutoff);
        }
    }

    pub fn subpixel_enabled(&self) -> bool {
        self.subpixel_order.allows_horizontal_subpixel()
    }

    fn render_mode(&self, enable_subpixel: bool) -> GlyphRenderMode {
        if enable_subpixel && self.subpixel_enabled() {
            GlyphRenderMode::Subpixel
        } else {
            GlyphRenderMode::Alpha
        }
    }

    fn render_mode_from_request(&self, request: SubpixelRequest) -> GlyphRenderMode {
        match request {
            SubpixelRequest::Enabled if self.subpixel_enabled() => GlyphRenderMode::Subpixel,
            _ => GlyphRenderMode::Alpha,
        }
    }

    fn rasterize_result_to_pixels(
        result: &RasterizeResult,
    ) -> Result<RasterizedGlyphPixels, GlyphAtlasError> {
        let size = PixelSize::new(result.width, result.height).ok_or(GlyphAtlasError::ZeroSize)?;
        let pixels = match result.pixel_kind {
            GlyphPixelKind::ColorRgba => RasterizedGlyphPixels::Color {
                size,
                rgba_srgb: result.pixel_data.clone(),
            },
            GlyphPixelKind::SubpixelRgba => RasterizedGlyphPixels::Subpixel {
                size,
                rgba: result.pixel_data.clone(),
            },
            GlyphPixelKind::AlphaMask => RasterizedGlyphPixels::Alpha {
                size,
                bytes: result.pixel_data.clone(),
            },
        };
        pixels.validated()
    }

    fn upload_to_atlas_page(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        content_rect: AtlasContentRect,
        pixel_data: &[u8],
        glyph_w: u32,
        glyph_h: u32,
        bytes_per_pixel: u32,
    ) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: content_rect.x(),
                    y: content_rect.y(),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixel_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(glyph_w * bytes_per_pixel),
                rows_per_image: Some(glyph_h),
            },
            wgpu::Extent3d {
                width: glyph_w,
                height: glyph_h,
                depth_or_array_layers: 1,
            },
        );
    }

    fn remove_alpha_page_entries(&mut self, evicted_id: PageId<AlphaMask>) {
        self.atlas_cache
            .retain(|_, v| !(matches!(v.entry, AnyAtlasEntry::Alpha(e) if e.page() == evicted_id)));
        self.atlas_composed_cache.retain(|_, v| {
            !v.handles.iter().any(
                |handle| matches!(handle.entry, AnyAtlasEntry::Alpha(e) if e.page() == evicted_id),
            )
        });
    }

    fn remove_subpixel_page_entries(&mut self, evicted_id: PageId<SubpixelMask>) {
        self.atlas_cache.retain(
            |_, v| !(matches!(v.entry, AnyAtlasEntry::Subpixel(e) if e.page() == evicted_id)),
        );
        self.atlas_composed_cache.retain(
            |_, v| {
                !v.handles.iter().any(
                    |handle| matches!(handle.entry, AnyAtlasEntry::Subpixel(e) if e.page() == evicted_id),
                )
            },
        );
    }

    fn remove_color_page_entries(&mut self, evicted_id: PageId<ColorRgba>) {
        self.atlas_cache
            .retain(|_, v| !(matches!(v.entry, AnyAtlasEntry::Color(e) if e.page() == evicted_id)));
        self.atlas_composed_cache.retain(|_, v| {
            !v.handles.iter().any(
                |handle| matches!(handle.entry, AnyAtlasEntry::Color(e) if e.page() == evicted_id),
            )
        });
    }

    fn allocate_alpha_with_eviction(
        &mut self,
        size: PixelSize,
        device: &wgpu::Device,
    ) -> Result<PageAllocResult<AlphaMask>, GlyphAtlasError> {
        if !self.atlas_config.can_fit(size) {
            return Err(GlyphAtlasError::GlyphTooLarge);
        }

        if let Some(allocation) = self.atlas_pages.allocate_alpha(
            size,
            device,
            &self.bind_group_layout,
            &self.linear_sampler,
            &self.nearest_sampler,
            self.frame_number,
        ) {
            return Ok(allocation);
        }

        let victim =
            self.atlas_pages
                .lru_unpinned_alpha()
                .ok_or(GlyphAtlasError::AllPagesPinned {
                    material: GlyphMaterialKind::AlphaMask,
                })?;
        let (evicted_id, _) = self
            .atlas_pages
            .reset_alpha_page(victim, self.frame_number)
            .ok_or(GlyphAtlasError::PageBudgetExhausted {
                material: GlyphMaterialKind::AlphaMask,
            })?;

        tracing::debug!(
            target: "glyph_atlas_invalidate",
            ?evicted_id,
            frame = self.frame_number,
            "alpha atlas page evicted (working set exceeds page budget)"
        );
        self.remove_alpha_page_entries(evicted_id);
        self.page_evictions_this_frame += 1;
        self.eviction_generation = self.eviction_generation.wrapping_add(1);
        self.atlas_pages
            .allocate_alpha(
                size,
                device,
                &self.bind_group_layout,
                &self.linear_sampler,
                &self.nearest_sampler,
                self.frame_number,
            )
            .ok_or(GlyphAtlasError::PageBudgetExhausted {
                material: GlyphMaterialKind::AlphaMask,
            })
    }

    fn allocate_subpixel_with_eviction(
        &mut self,
        size: PixelSize,
        device: &wgpu::Device,
    ) -> Result<PageAllocResult<SubpixelMask>, GlyphAtlasError> {
        if !self.atlas_config.can_fit(size) {
            return Err(GlyphAtlasError::GlyphTooLarge);
        }

        if let Some(allocation) = self.atlas_pages.allocate_subpixel(
            size,
            device,
            &self.bind_group_layout,
            &self.linear_sampler,
            &self.nearest_sampler,
            self.frame_number,
        ) {
            return Ok(allocation);
        }

        let victim =
            self.atlas_pages
                .lru_unpinned_subpixel()
                .ok_or(GlyphAtlasError::AllPagesPinned {
                    material: GlyphMaterialKind::SubpixelMask,
                })?;
        let (evicted_id, _) = self
            .atlas_pages
            .reset_subpixel_page(victim, self.frame_number)
            .ok_or(GlyphAtlasError::PageBudgetExhausted {
                material: GlyphMaterialKind::SubpixelMask,
            })?;

        self.remove_subpixel_page_entries(evicted_id);
        self.page_evictions_this_frame += 1;
        self.eviction_generation = self.eviction_generation.wrapping_add(1);
        self.atlas_pages
            .allocate_subpixel(
                size,
                device,
                &self.bind_group_layout,
                &self.linear_sampler,
                &self.nearest_sampler,
                self.frame_number,
            )
            .ok_or(GlyphAtlasError::PageBudgetExhausted {
                material: GlyphMaterialKind::SubpixelMask,
            })
    }

    fn allocate_color_with_eviction(
        &mut self,
        size: PixelSize,
        device: &wgpu::Device,
    ) -> Result<PageAllocResult<ColorRgba>, GlyphAtlasError> {
        if !self.atlas_config.can_fit(size) {
            return Err(GlyphAtlasError::GlyphTooLarge);
        }

        if let Some(allocation) = self.atlas_pages.allocate_color(
            size,
            device,
            &self.bind_group_layout,
            &self.linear_sampler,
            &self.nearest_sampler,
            self.frame_number,
        ) {
            return Ok(allocation);
        }

        let victim =
            self.atlas_pages
                .lru_unpinned_color()
                .ok_or(GlyphAtlasError::AllPagesPinned {
                    material: GlyphMaterialKind::ColorRgba,
                })?;
        let (evicted_id, _) = self
            .atlas_pages
            .reset_color_page(victim, self.frame_number)
            .ok_or(GlyphAtlasError::PageBudgetExhausted {
                material: GlyphMaterialKind::ColorRgba,
            })?;

        self.remove_color_page_entries(evicted_id);
        self.page_evictions_this_frame += 1;
        self.eviction_generation = self.eviction_generation.wrapping_add(1);
        self.atlas_pages
            .allocate_color(
                size,
                device,
                &self.bind_group_layout,
                &self.linear_sampler,
                &self.nearest_sampler,
                self.frame_number,
            )
            .ok_or(GlyphAtlasError::PageBudgetExhausted {
                material: GlyphMaterialKind::ColorRgba,
            })
    }

    fn rasterize_result_to_atlas_entry(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        result: &RasterizeResult,
    ) -> Result<AnyAtlasEntry, GlyphAtlasError> {
        let pixels = Self::rasterize_result_to_pixels(result)?;
        let size = pixels.size();
        let page_size = self.atlas_config.page_size;
        let metrics = GlyphMetrics {
            bearing_x: result.bearing_x,
            bearing_y: result.bearing_y,
            advance_width: result.advance_width,
        };

        match pixels {
            RasterizedGlyphPixels::Alpha { bytes, .. } => {
                let PageAllocResult {
                    page_id,
                    generation,
                    allocation,
                } = self.allocate_alpha_with_eviction(size, device)?;
                let page = self.atlas_pages.alpha_page(page_id).ok_or(
                    GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::AlphaMask,
                        page: page_id.get(),
                    },
                )?;
                Self::upload_to_atlas_page(
                    queue,
                    &page.texture,
                    allocation.content_rect,
                    &bytes,
                    size.width(),
                    size.height(),
                    AlphaMask::BYTES_PER_PIXEL,
                );
                let uv = UvRect::from_content_rect(allocation.content_rect, page_size);
                Ok(AnyAtlasEntry::Alpha(AtlasEntry::new_with_sampling(
                    page_id,
                    generation,
                    allocation.content_rect,
                    uv,
                    metrics,
                    result.sampling,
                )))
            }
            RasterizedGlyphPixels::Subpixel { rgba, .. } => {
                let PageAllocResult {
                    page_id,
                    generation,
                    allocation,
                } = self.allocate_subpixel_with_eviction(size, device)?;
                let page = self.atlas_pages.subpixel_page(page_id).ok_or(
                    GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::SubpixelMask,
                        page: page_id.get(),
                    },
                )?;
                Self::upload_to_atlas_page(
                    queue,
                    &page.texture,
                    allocation.content_rect,
                    &rgba,
                    size.width(),
                    size.height(),
                    SubpixelMask::BYTES_PER_PIXEL,
                );
                let uv = UvRect::from_content_rect(allocation.content_rect, page_size);
                Ok(AnyAtlasEntry::Subpixel(AtlasEntry::new_with_sampling(
                    page_id,
                    generation,
                    allocation.content_rect,
                    uv,
                    metrics,
                    result.sampling,
                )))
            }
            RasterizedGlyphPixels::Color { rgba_srgb, .. } => {
                let PageAllocResult {
                    page_id,
                    generation,
                    allocation,
                } = self.allocate_color_with_eviction(size, device)?;
                let page = self.atlas_pages.color_page(page_id).ok_or(
                    GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::ColorRgba,
                        page: page_id.get(),
                    },
                )?;
                Self::upload_to_atlas_page(
                    queue,
                    &page.texture,
                    allocation.content_rect,
                    &rgba_srgb,
                    size.width(),
                    size.height(),
                    ColorRgba::BYTES_PER_PIXEL,
                );
                let uv = UvRect::from_content_rect(allocation.content_rect, page_size);
                Ok(AnyAtlasEntry::Color(AtlasEntry::new_with_sampling(
                    page_id,
                    generation,
                    allocation.content_rect,
                    uv,
                    metrics,
                    result.sampling,
                )))
            }
        }
    }

    fn pin_entry_page(&mut self, entry: AnyAtlasEntry) -> Result<(), GlyphAtlasError> {
        match entry {
            AnyAtlasEntry::Alpha(e) => {
                let generation = self
                    .atlas_pages
                    .alpha_page(e.page())
                    .map(|p| p.generation)
                    .ok_or(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::AlphaMask,
                        page: e.page().get(),
                    })?;
                if !e.matches_generation(generation) {
                    return Err(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::AlphaMask,
                        page: e.page().get(),
                    });
                }
                self.atlas_pages.pin_alpha(e.page(), self.frame_number);
            }
            AnyAtlasEntry::Subpixel(e) => {
                let generation = self
                    .atlas_pages
                    .subpixel_page(e.page())
                    .map(|p| p.generation)
                    .ok_or(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::SubpixelMask,
                        page: e.page().get(),
                    })?;
                if !e.matches_generation(generation) {
                    return Err(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::SubpixelMask,
                        page: e.page().get(),
                    });
                }
                self.atlas_pages.pin_subpixel(e.page(), self.frame_number);
            }
            AnyAtlasEntry::Color(e) => {
                let generation = self
                    .atlas_pages
                    .color_page(e.page())
                    .map(|p| p.generation)
                    .ok_or(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::ColorRgba,
                        page: e.page().get(),
                    })?;
                if !e.matches_generation(generation) {
                    return Err(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::ColorRgba,
                        page: e.page().get(),
                    });
                }
                self.atlas_pages.pin_color(e.page(), self.frame_number);
            }
        }
        Ok(())
    }

    pub fn get_or_create_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &GlyphKey,
        face: Option<&Face>,
        subpixel: SubpixelRequest,
    ) -> Option<GlyphAtlasHandle> {
        match self.get_or_create_atlas_result(device, queue, key, face, subpixel) {
            Ok(handle) => Some(handle),
            Err(err) => {
                if !matches!(err, GlyphAtlasError::Whitespace) {
                    tracing::warn!(?err, charcode = key.charcode, "glyph atlas lookup failed");
                }
                None
            }
        }
    }

    fn get_or_create_atlas_result(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &GlyphKey,
        face: Option<&Face>,
        subpixel: SubpixelRequest,
    ) -> Result<GlyphAtlasHandle, GlyphAtlasError> {
        let c =
            char::from_u32(key.charcode).ok_or(GlyphAtlasError::InvalidCharCode(key.charcode))?;
        let mut glyph_identity = key.identity();
        glyph_identity.font_identity = self.glyph_font_identity_for_char(face, c);
        let cache_key = CachedGlyphKey {
            glyph: glyph_identity,
            mode: self.render_mode_from_request(subpixel),
        };

        if let Some(cached) = self.atlas_cache.get_mut(&cache_key) {
            cached.last_accessed = self.generation;
            self.cache_hits_this_frame += 1;
            let handle = GlyphAtlasHandle {
                entry: cached.entry,
                advance_width: cached.advance_width,
            };
            self.pin_entry_page(handle.entry)?;
            return Ok(handle);
        }

        let enable_subpixel = matches!(subpixel, SubpixelRequest::Enabled);
        // Keep the ordinary-whitespace fast rejection. U+0020 is exceptional
        // only when GNU's primary-ASCII policy leaves it missing: that case
        // draws the missing-glyph box directly. Normal spaces never shape or
        // allocate an atlas entry — so the verdict is memoized per key, or
        // every space in every frame re-runs the font probe.
        let whitespace_result = if c.is_whitespace() {
            if self.whitespace_skip.contains(&cache_key) {
                return Err(GlyphAtlasError::Whitespace);
            }
            let bitmap = self.try_rasterize_bitmap_char(c, face);
            match bitmap {
                BitmapCharRasterization::Missing { advance_width } if c.is_ascii() => {
                    let face = face.ok_or(GlyphAtlasError::RasterizeFailed)?;
                    Some(self.rasterize_missing_primary_ascii(advance_width, face))
                }
                BitmapCharRasterization::Failed => {
                    return Err(GlyphAtlasError::RasterizeFailed);
                }
                BitmapCharRasterization::Rasterized(_)
                | BitmapCharRasterization::Missing { .. } => {
                    if c.is_ascii() {
                        self.whitespace_skip.insert(cache_key);
                    }
                    return Err(GlyphAtlasError::Whitespace);
                }
                BitmapCharRasterization::NotBitmap => {
                    match self.try_fast_single_char_glyph(c, face) {
                        Some(SingleCharGlyph::MissingPrimaryAscii { advance_width }) => {
                            let face = face.ok_or(GlyphAtlasError::RasterizeFailed)?;
                            Some(self.rasterize_missing_primary_ascii(advance_width, face))
                        }
                        _ => {
                            // Memoize ASCII only: its primary-font result is
                            // stable across this realized face. Non-ASCII
                            // whitespace can still engage semantic fallback
                            // for legacy producers without an exact binding.
                            if c.is_ascii() {
                                self.whitespace_skip.insert(cache_key);
                            }
                            return Err(GlyphAtlasError::Whitespace);
                        }
                    }
                }
            }
        } else {
            None
        };
        tracing::debug!(
            target: "glyph_atlas_invalidate",
            charcode = key.charcode,
            ch = %c,
            font_size_bits = key.font_size_bits,
            font_identity = key.font_identity,
            x_bin = ?key.x_bin,
            y_bin = ?key.y_bin,
            mode = ?cache_key.mode,
            cache_len = self.atlas_cache.len(),
            "single-glyph atlas MISS"
        );
        let result = match whitespace_result {
            Some(result) => result,
            None => self
                .rasterize_glyph(c, face, key.x_bin, key.y_bin, enable_subpixel)
                .ok_or(GlyphAtlasError::RasterizeFailed)?,
        };

        if result.width == 0 || result.height == 0 {
            return Err(GlyphAtlasError::ZeroSize);
        }

        if self.cached_char_width.is_none()
            && key_uses_default_font_metrics(key, self.default_font_size)
        {
            self.cached_char_width = Some(result.advance_width / self.scale_factor);
            self.cached_font_ascent = Some(result.bearing_y / self.scale_factor);
        }

        let entry = self.rasterize_result_to_atlas_entry(device, queue, &result)?;

        let handle = GlyphAtlasHandle {
            entry,
            advance_width: result.advance_width,
        };

        self.atlas_cache.insert(
            cache_key,
            CachedAtlasGlyph {
                entry,
                advance_width: result.advance_width,
                last_accessed: self.generation,
            },
        );
        self.cache_misses_this_frame += 1;

        Ok(handle)
    }

    pub fn get_or_create_composed_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        face_id: FaceId,
        font_size_bits: u32,
        face: Option<&Face>,
        x_bin: SubpixelBin,
        y_bin: SubpixelBin,
        subpixel: SubpixelRequest,
    ) -> Option<Vec<GlyphAtlasHandle>> {
        match self.get_or_create_composed_atlas_result(
            device,
            queue,
            text,
            face_id,
            font_size_bits,
            face,
            x_bin,
            y_bin,
            subpixel,
        ) {
            Ok(handle) => Some(handle),
            Err(err) => {
                tracing::warn!(?err, text, "composed glyph atlas lookup failed");
                None
            }
        }
    }

    fn get_or_create_composed_atlas_result(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        face_id: FaceId,
        font_size_bits: u32,
        face: Option<&Face>,
        x_bin: SubpixelBin,
        y_bin: SubpixelBin,
        subpixel: SubpixelRequest,
    ) -> Result<Vec<GlyphAtlasHandle>, GlyphAtlasError> {
        let key = ComposedGlyphKey {
            text: text.into(),
            face_id,
            font_size_bits,
            font_identity: glyph_font_identity(face),
            glyph_stream_identity: self.glyph_stream_identity_for_composed(face, text),
            x_bin,
            y_bin,
        };
        let cache_key = CachedComposedGlyphKey {
            glyph: key.identity(),
            mode: self.render_mode_from_request(subpixel),
        };

        if let Some(cached) = self.atlas_composed_cache.get_mut(&cache_key) {
            cached.last_accessed = self.generation;
            self.cache_hits_this_frame += 1;
            let handles = cached.handles.clone();
            for handle in &handles {
                self.pin_entry_page(handle.entry)?;
            }
            return Ok(handles);
        }

        let enable_subpixel = matches!(subpixel, SubpixelRequest::Enabled);
        tracing::debug!(
            target: "glyph_atlas_invalidate",
            text,
            face_id = ?key.face_id,
            font_size_bits = key.font_size_bits,
            font_identity = key.font_identity,
            x_bin = ?key.x_bin,
            y_bin = ?key.y_bin,
            mode = ?cache_key.mode,
            cache_len = self.atlas_composed_cache.len(),
            "composed atlas MISS"
        );
        // Prefer the layout-shaped exact glyphs; re-shape the cluster text
        // only when this (face, text) wasn't published (render-side
        // fallback, e.g. renderer-owned chrome text).
        let results = self
            .rasterize_shaped_cluster_if_published(text, face, x_bin, y_bin, enable_subpixel)
            .or_else(|| {
                self.rasterize_text(text, face, x_bin, y_bin, enable_subpixel)
                    .map(|result| vec![result])
            })
            .ok_or(GlyphAtlasError::RasterizeFailed)?;

        let mut handles = Vec::with_capacity(results.len());
        for result in results {
            if result.width == 0 || result.height == 0 {
                return Err(GlyphAtlasError::ZeroSize);
            }
            let entry = self.rasterize_result_to_atlas_entry(device, queue, &result)?;
            self.pin_entry_page(entry)?;
            handles.push(GlyphAtlasHandle {
                entry,
                advance_width: result.advance_width,
            });
        }

        self.atlas_composed_cache.insert(
            cache_key,
            CachedComposedGlyph {
                handles: handles.clone(),
                last_accessed: self.generation,
            },
        );
        self.cache_misses_this_frame += 1;

        Ok(handles)
    }

    pub fn atlas_bind_group(
        &self,
        entry: AnyAtlasEntry,
    ) -> Result<&wgpu::BindGroup, GlyphAtlasError> {
        match entry {
            AnyAtlasEntry::Alpha(e) => {
                let page = self.atlas_pages.alpha_page(e.page()).ok_or(
                    GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::AlphaMask,
                        page: e.page().get(),
                    },
                )?;
                if !e.matches_generation(page.generation) {
                    return Err(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::AlphaMask,
                        page: e.page().get(),
                    });
                }
                Ok(match e.sampling() {
                    neomacs_display_protocol::font::GlyphSampling::Linear => {
                        &page.linear_bind_group
                    }
                    neomacs_display_protocol::font::GlyphSampling::Nearest => {
                        &page.nearest_bind_group
                    }
                })
            }
            AnyAtlasEntry::Subpixel(e) => {
                let page = self.atlas_pages.subpixel_page(e.page()).ok_or(
                    GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::SubpixelMask,
                        page: e.page().get(),
                    },
                )?;
                if !e.matches_generation(page.generation) {
                    return Err(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::SubpixelMask,
                        page: e.page().get(),
                    });
                }
                Ok(match e.sampling() {
                    neomacs_display_protocol::font::GlyphSampling::Linear => {
                        &page.linear_bind_group
                    }
                    neomacs_display_protocol::font::GlyphSampling::Nearest => {
                        &page.nearest_bind_group
                    }
                })
            }
            AnyAtlasEntry::Color(e) => {
                let page = self.atlas_pages.color_page(e.page()).ok_or(
                    GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::ColorRgba,
                        page: e.page().get(),
                    },
                )?;
                if !e.matches_generation(page.generation) {
                    return Err(GlyphAtlasError::StaleAtlasEntry {
                        material: GlyphMaterialKind::ColorRgba,
                        page: e.page().get(),
                    });
                }
                Ok(match e.sampling() {
                    neomacs_display_protocol::font::GlyphSampling::Linear => {
                        &page.linear_bind_group
                    }
                    neomacs_display_protocol::font::GlyphSampling::Nearest => {
                        &page.nearest_bind_group
                    }
                })
            }
        }
    }

    pub fn atlas_page_counts(&self) -> (usize, usize, usize) {
        self.atlas_pages.page_counts()
    }
}

fn key_uses_default_font_metrics(key: &GlyphKey, default_font_size: f32) -> bool {
    if key.face_id != FaceId::new(0) {
        return false;
    }
    let font_size = f32::from_bits(key.font_size_bits);
    font_size == 0.0 || (font_size - default_font_size).abs() <= 0.1
}

/// Resolve the effective font size for shaping/rasterization.
///
/// A face `font_size` of `0.0` is the "unspecified" sentinel meaning "use the
/// frame default" — the same convention `key_uses_default_font_metrics`
/// encodes (a minibuffer/echo-area face, for instance, inherits the frame
/// default font and carries size 0). cosmic-text panics ("line height cannot
/// be 0") the moment it is handed a zero line height, so a zero, negative, or
/// absent size must resolve to the default here rather than flow into
/// `Metrics::new`.
fn effective_font_size(face_size: Option<f32>, default_font_size: f32) -> f32 {
    match face_size {
        Some(size) if size.is_finite() && size > 0.0 => size,
        _ => default_font_size,
    }
}

#[cfg(test)]
#[path = "glyph_atlas_test.rs"]
mod tests;
