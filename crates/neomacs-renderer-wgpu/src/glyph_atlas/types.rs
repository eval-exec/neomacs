//! Type-safe atlas primitives for the glyph atlas refactor.
//!
//! These types enforce invariants at construction time so the renderer
//! cannot accidentally use zero-size rectangles, mismatched materials,
//! or stale atlas coordinates.
//!
//! No behavior change — these types are introduced alongside the existing
//! per-glyph texture path and will be wired in during later steps.

use neomacs_display_protocol::font::GlyphSampling;
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU32;

// ---------------------------------------------------------------------------
// Material marker types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlphaMask;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubpixelMask;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorRgba;

mod private {
    pub trait Sealed {}
    impl Sealed for super::AlphaMask {}
    impl Sealed for super::SubpixelMask {}
    impl Sealed for super::ColorRgba {}
}

pub trait GlyphMaterial: private::Sealed + 'static {
    const KIND: GlyphMaterialKind;
    const TEXTURE_FORMAT: wgpu::TextureFormat;
    const BYTES_PER_PIXEL: u32;
}

impl GlyphMaterial for AlphaMask {
    const KIND: GlyphMaterialKind = GlyphMaterialKind::AlphaMask;
    const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
    const BYTES_PER_PIXEL: u32 = 1;
}

impl GlyphMaterial for SubpixelMask {
    const KIND: GlyphMaterialKind = GlyphMaterialKind::SubpixelMask;
    const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    const BYTES_PER_PIXEL: u32 = 4;
}

impl GlyphMaterial for ColorRgba {
    const KIND: GlyphMaterialKind = GlyphMaterialKind::ColorRgba;
    const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
    const BYTES_PER_PIXEL: u32 = 4;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GlyphMaterialKind {
    AlphaMask,
    SubpixelMask,
    ColorRgba,
}

impl GlyphMaterialKind {
    pub fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::AlphaMask => AlphaMask::TEXTURE_FORMAT,
            Self::SubpixelMask => SubpixelMask::TEXTURE_FORMAT,
            Self::ColorRgba => ColorRgba::TEXTURE_FORMAT,
        }
    }

    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::AlphaMask => AlphaMask::BYTES_PER_PIXEL,
            Self::SubpixelMask => SubpixelMask::BYTES_PER_PIXEL,
            Self::ColorRgba => ColorRgba::BYTES_PER_PIXEL,
        }
    }
}

// ---------------------------------------------------------------------------
// Page ID
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId<M: GlyphMaterial> {
    raw: NonZeroU32,
    _marker: PhantomData<M>,
}

impl<M: GlyphMaterial> PageId<M> {
    pub fn new(raw: NonZeroU32) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    pub fn get(self) -> u32 {
        self.raw.get()
    }
}

impl<M: GlyphMaterial> fmt::Debug for PageId<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageId")
            .field("raw", &self.raw)
            .field("material", &M::KIND)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageToken<M: GlyphMaterial> {
    index: NonZeroU32,
    generation: u32,
    _marker: PhantomData<M>,
}

impl<M: GlyphMaterial> PageToken<M> {
    pub fn new(index: NonZeroU32, generation: u32) -> Self {
        Self {
            index,
            generation,
            _marker: PhantomData,
        }
    }

    pub fn index(self) -> u32 {
        self.index.get()
    }

    pub fn generation(self) -> u32 {
        self.generation
    }

    pub fn page_id(self) -> PageId<M> {
        PageId::new(self.index)
    }
}

// ---------------------------------------------------------------------------
// Coordinate newtypes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSize {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl PixelSize {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }

    pub fn width(self) -> u32 {
        self.width.get()
    }

    pub fn height(self) -> u32 {
        self.height.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasAllocationRect {
    x: u32,
    y: u32,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl AtlasAllocationRect {
    pub fn new(x: u32, y: u32, width: NonZeroU32, height: NonZeroU32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn x(self) -> u32 {
        self.x
    }
    pub fn y(self) -> u32 {
        self.y
    }
    pub fn width(self) -> u32 {
        self.width.get()
    }
    pub fn height(self) -> u32 {
        self.height.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasContentRect {
    x: u32,
    y: u32,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl AtlasContentRect {
    pub fn new(x: u32, y: u32, width: NonZeroU32, height: NonZeroU32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn x(self) -> u32 {
        self.x
    }
    pub fn y(self) -> u32 {
        self.y
    }
    pub fn width(self) -> u32 {
        self.width.get()
    }
    pub fn height(self) -> u32 {
        self.height.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvRect {
    min: [f32; 2],
    max: [f32; 2],
}

impl UvRect {
    pub fn new(min: [f32; 2], max: [f32; 2]) -> Self {
        Self { min, max }
    }

    pub fn min(self) -> [f32; 2] {
        self.min
    }
    pub fn max(self) -> [f32; 2] {
        self.max
    }

    pub fn from_content_rect(content: AtlasContentRect, page_size: u32) -> Self {
        let ps = page_size as f32;
        Self {
            min: [content.x as f32 / ps, content.y as f32 / ps],
            max: [
                (content.x + content.width()) as f32 / ps,
                (content.y + content.height()) as f32 / ps,
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Glyph metrics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphMetrics {
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance_width: f32,
}

// ---------------------------------------------------------------------------
// Atlas entry (copyable handle from atlas lookup)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasEntry<M: GlyphMaterial> {
    page: PageId<M>,
    generation: u32,
    rect: AtlasContentRect,
    uv: UvRect,
    metrics: GlyphMetrics,
    sampling: GlyphSampling,
}

impl<M: GlyphMaterial> AtlasEntry<M> {
    pub fn new(
        page: PageId<M>,
        generation: u32,
        rect: AtlasContentRect,
        uv: UvRect,
        metrics: GlyphMetrics,
    ) -> Self {
        Self::new_with_sampling(page, generation, rect, uv, metrics, GlyphSampling::Linear)
    }

    pub fn new_with_sampling(
        page: PageId<M>,
        generation: u32,
        rect: AtlasContentRect,
        uv: UvRect,
        metrics: GlyphMetrics,
        sampling: GlyphSampling,
    ) -> Self {
        Self {
            page,
            generation,
            rect,
            uv,
            metrics,
            sampling,
        }
    }

    pub fn page(self) -> PageId<M> {
        self.page
    }
    pub fn generation(self) -> u32 {
        self.generation
    }
    pub fn matches_generation(self, generation: u32) -> bool {
        self.generation == generation
    }
    pub fn token(self) -> PageToken<M> {
        PageToken::new(NonZeroU32::new(self.page.get()).unwrap(), self.generation)
    }
    pub fn rect(self) -> AtlasContentRect {
        self.rect
    }
    pub fn uv(self) -> UvRect {
        self.uv
    }
    pub fn metrics(self) -> GlyphMetrics {
        self.metrics
    }
    pub fn sampling(self) -> GlyphSampling {
        self.sampling
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AnyAtlasEntry {
    Alpha(AtlasEntry<AlphaMask>),
    Subpixel(AtlasEntry<SubpixelMask>),
    Color(AtlasEntry<ColorRgba>),
}

impl AnyAtlasEntry {
    pub fn material_kind(self) -> GlyphMaterialKind {
        match self {
            Self::Alpha(_) => GlyphMaterialKind::AlphaMask,
            Self::Subpixel(_) => GlyphMaterialKind::SubpixelMask,
            Self::Color(_) => GlyphMaterialKind::ColorRgba,
        }
    }

    pub fn advance_width(self) -> f32 {
        match self {
            Self::Alpha(e) => e.metrics().advance_width,
            Self::Subpixel(e) => e.metrics().advance_width,
            Self::Color(e) => e.metrics().advance_width,
        }
    }

    pub fn page_id_value(self) -> (GlyphMaterialKind, u32) {
        match self {
            Self::Alpha(e) => (GlyphMaterialKind::AlphaMask, e.page().get()),
            Self::Subpixel(e) => (GlyphMaterialKind::SubpixelMask, e.page().get()),
            Self::Color(e) => (GlyphMaterialKind::ColorRgba, e.page().get()),
        }
    }

    pub fn binding_id_value(self) -> (GlyphMaterialKind, u32, GlyphSampling) {
        let (material, page) = self.page_id_value();
        (material, page, self.sampling())
    }

    pub fn sampling(self) -> GlyphSampling {
        match self {
            Self::Alpha(e) => e.sampling(),
            Self::Subpixel(e) => e.sampling(),
            Self::Color(e) => e.sampling(),
        }
    }

    pub fn rect(self) -> AtlasContentRect {
        match self {
            Self::Alpha(e) => e.rect(),
            Self::Subpixel(e) => e.rect(),
            Self::Color(e) => e.rect(),
        }
    }

    pub fn uv(self) -> UvRect {
        match self {
            Self::Alpha(e) => e.uv(),
            Self::Subpixel(e) => e.uv(),
            Self::Color(e) => e.uv(),
        }
    }

    pub fn metrics(self) -> GlyphMetrics {
        match self {
            Self::Alpha(e) => e.metrics(),
            Self::Subpixel(e) => e.metrics(),
            Self::Color(e) => e.metrics(),
        }
    }
}

// ---------------------------------------------------------------------------
// Rasterized glyph pixels
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum RasterizedGlyphPixels {
    Alpha { size: PixelSize, bytes: Vec<u8> },
    Subpixel { size: PixelSize, rgba: Vec<u8> },
    Color { size: PixelSize, rgba_srgb: Vec<u8> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphAtlasError {
    Whitespace,
    InvalidCharCode(u32),
    RasterizeFailed,
    ZeroSize,
    GlyphTooLarge,
    PageBudgetExhausted {
        material: GlyphMaterialKind,
    },
    AllPagesPinned {
        material: GlyphMaterialKind,
    },
    PixelDataLength {
        material: GlyphMaterialKind,
        expected: usize,
        actual: usize,
    },
    StaleAtlasEntry {
        material: GlyphMaterialKind,
        page: u32,
    },
}

impl fmt::Display for GlyphAtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Whitespace => write!(f, "glyph is whitespace"),
            Self::InvalidCharCode(code) => write!(f, "invalid char code: {code}"),
            Self::RasterizeFailed => write!(f, "glyph rasterization failed"),
            Self::ZeroSize => write!(f, "glyph has zero size"),
            Self::GlyphTooLarge => write!(f, "glyph is larger than atlas page"),
            Self::PageBudgetExhausted { material } => {
                write!(f, "atlas page budget exhausted for {material:?}")
            }
            Self::AllPagesPinned { material } => {
                write!(f, "all atlas pages pinned for {material:?}")
            }
            Self::PixelDataLength {
                material,
                expected,
                actual,
            } => write!(
                f,
                "pixel buffer length mismatch for {:?}: expected {}, got {}",
                material, expected, actual
            ),
            Self::StaleAtlasEntry { material, page } => {
                write!(f, "stale atlas entry for {material:?} page {page}")
            }
        }
    }
}

impl std::error::Error for GlyphAtlasError {}

impl RasterizedGlyphPixels {
    pub fn validated(self) -> Result<Self, GlyphAtlasError> {
        let (size, bytes, bpp, material) = match &self {
            Self::Alpha { size, bytes } => (size, bytes, 1u32, GlyphMaterialKind::AlphaMask),
            Self::Subpixel { size, rgba } => (size, rgba, 4u32, GlyphMaterialKind::SubpixelMask),
            Self::Color { size, rgba_srgb } => {
                (size, rgba_srgb, 4u32, GlyphMaterialKind::ColorRgba)
            }
        };
        let expected = (size.width() * size.height() * bpp) as usize;
        if bytes.len() != expected {
            return Err(GlyphAtlasError::PixelDataLength {
                material,
                expected,
                actual: bytes.len(),
            });
        }
        Ok(self)
    }

    pub fn material(&self) -> GlyphMaterialKind {
        match self {
            Self::Alpha { .. } => GlyphMaterialKind::AlphaMask,
            Self::Subpixel { .. } => GlyphMaterialKind::SubpixelMask,
            Self::Color { .. } => GlyphMaterialKind::ColorRgba,
        }
    }

    pub fn size(&self) -> PixelSize {
        match self {
            Self::Alpha { size, .. } => *size,
            Self::Subpixel { size, .. } => *size,
            Self::Color { size, .. } => *size,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Alpha { bytes, .. } => bytes,
            Self::Subpixel { rgba, .. } => rgba,
            Self::Color { rgba_srgb, .. } => rgba_srgb,
        }
    }
}

// ---------------------------------------------------------------------------
// Subpixel / raster mode enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubpixelRequest {
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphRasterMode {
    Alpha,
    Subpixel,
}

// ---------------------------------------------------------------------------
// Atlas config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphAtlasConfig {
    pub page_size: u32,
    pub padding: u32,
    pub max_pages_per_material: usize,
}

impl GlyphAtlasConfig {
    pub const DEFAULT_PAGE_SIZE: u32 = 2048;
    pub const DEFAULT_PADDING: u32 = 1;
    pub const DEFAULT_MAX_PAGES: usize = 8;

    pub fn default_for_device(_device: &wgpu::Device) -> Self {
        Self {
            page_size: Self::DEFAULT_PAGE_SIZE,
            padding: Self::DEFAULT_PADDING,
            max_pages_per_material: Self::DEFAULT_MAX_PAGES,
        }
    }

    pub fn max_content_size(self) -> u32 {
        self.page_size.saturating_sub(2 * self.padding)
    }

    pub fn can_fit(self, size: PixelSize) -> bool {
        let max_content = self.max_content_size();
        size.width() <= max_content && size.height() <= max_content
    }
}

impl Default for GlyphAtlasConfig {
    fn default() -> Self {
        Self {
            page_size: Self::DEFAULT_PAGE_SIZE,
            padding: Self::DEFAULT_PADDING,
            max_pages_per_material: Self::DEFAULT_MAX_PAGES,
        }
    }
}

// ---------------------------------------------------------------------------
// Page allocation result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageAllocationResult<M: GlyphMaterial> {
    Allocated {
        page: PageId<M>,
        rect: AtlasContentRect,
    },
    NeedNewPage,
    GlyphTooLarge,
}

// ---------------------------------------------------------------------------
// Glyph atlas handle (copyable metadata from cache lookup)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct GlyphAtlasHandle {
    pub entry: AnyAtlasEntry,
    pub advance_width: f32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "types/tests/types_test.rs"]
mod tests;
