//! Color glyph rasterization for faces fontdb materializes but Swash cannot
//! paint.
//!
//! Swash's color-outline source only understands COLR version 0's layer
//! records.  A version 1 face keeps its paints in `BaseGlyphList`, and its
//! emoji glyphs usually have empty `glyf` outlines, so a COLRv1 font draws
//! *nothing*: not the color layers, not a monochrome fallback.  This module
//! traverses the paint graph with ttf-parser and rasterizes it with tiny-skia,
//! producing the placement and pixel conventions the Swash-backed path
//! produces, so callers cannot tell the two apart.  `SVG ` table documents —
//! the other source Swash never paints — render through usvg and resvg with
//! the same conventions.
//!
//! The face cache belongs to the caller: rasterization is asked per glyph, and
//! the table directory of a face is read once per asset, not once per glyph.

mod colr;
mod svg;

use crate::raster_sources::{ColorGlyphSource, SfntFaceRasterSources, classify_sfnt_face};
use neomacs_display_protocol::font::FontOutlineAsset;
use std::collections::HashMap;
use std::sync::Arc;

/// A rasterized color glyph, positioned relative to the pen origin.
///
/// `left`/`top` follow the same convention Swash's placement uses: `left` is
/// the horizontal offset from the pen to the bitmap's left column, and `top`
/// is the offset from the baseline to the bitmap's top row, positive upwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorGlyphRaster {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    /// Straight-alpha sRGB RGBA, row-major, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
    /// The source that painted it, for tracing and tests.
    pub source: ColorGlyphSource,
}

/// Variation axis tag, as OpenType spells it.
pub use ttf_parser::Tag;

/// Everything one glyph rasterization depends on.
#[derive(Clone, Copy, Debug)]
pub struct ColorGlyphRequest<'a> {
    pub glyph_id: u16,
    /// Device pixels, including the device scale factor.
    pub px_size: f32,
    /// CPAL palette index.
    pub palette: u16,
    /// Text foreground; palette index `0xFFFF` resolves to it.
    pub foreground: [u8; 4],
    /// Sub-pixel bin offsets in device pixels.
    pub offset: (f32, f32),
    /// Variable-font settings as (axis tag, user value).
    pub variations: &'a [(Tag, f32)],
}

impl Default for ColorGlyphRequest<'_> {
    fn default() -> Self {
        Self {
            glyph_id: 0,
            px_size: 16.0,
            palette: 0,
            foreground: [0, 0, 0, 255],
            offset: (0.0, 0.0),
            variations: &[],
        }
    }
}

/// Failures that are not per-glyph: the source could not be opened at all.
/// A well-formed source that simply paints nothing for a glyph is `Ok(None)`.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ColorGlyphError {
    #[error("failed reading color font source {path}: {reason}")]
    Read { path: String, reason: String },
    #[error("color font source {path} has no face at index {face_index}")]
    MissingFace { path: String, face_index: u32 },
}

/// Classify one file face from its table directory, without reading its
/// payload.
///
/// `None` covers "not an SFNT container" and "no such face index", both of
/// which mean this rasterizer owns nothing in the file.
fn classify_file(
    path: &str,
    face_index: u32,
) -> Result<Option<SfntFaceRasterSources>, ColorGlyphError> {
    use std::io::Read;
    let read_error = |error: std::io::Error| ColorGlyphError::Read {
        path: path.to_owned(),
        reason: error.to_string(),
    };
    let mut file = std::fs::File::open(path).map_err(read_error)?;
    let mut header = Vec::with_capacity(12);
    (&mut file)
        .take(12)
        .read_to_end(&mut header)
        .map_err(read_error)?;
    match crate::fontdb::classify_sfnt_stream(&mut file, &header, face_index).map_err(read_error)? {
        crate::fontdb::StreamedSfntSource::Sfnt(sources) => Ok(sources),
        crate::fontdb::StreamedSfntSource::NotSfnt => Ok(None),
    }
}

/// Key for one exact source face, mirroring the identity the display side
/// uses for pinning: durable path or verified memory payload, plus face index.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ColorFaceKey {
    File { path: String, face_index: u32 },
    Memory { key: String, face_index: u32 },
}

impl ColorFaceKey {
    fn from_asset(asset: &FontOutlineAsset) -> Self {
        match asset {
            FontOutlineAsset::File(asset) => Self::File {
                path: asset.path().to_owned(),
                face_index: asset.face_index(),
            },
            FontOutlineAsset::Memory(asset) => Self::Memory {
                key: asset.key().to_owned(),
                face_index: asset.face_index(),
            },
        }
    }
}

/// Bytes plus the face index they must be parsed at.
#[derive(Clone, Debug)]
struct ColorFaceBytes {
    bytes: Arc<Vec<u8>>,
    face_index: u32,
    sources: SfntFaceRasterSources,
}

impl ColorFaceBytes {
    fn face(&self) -> Option<ttf_parser::Face<'_>> {
        ttf_parser::Face::parse(&self.bytes, self.face_index).ok()
    }
}

/// Rasterizes color glyphs for exact source faces.
///
/// One instance is owned by whichever component rasterizes glyphs; it holds
/// per-face bytes so a glyph never re-reads a font file.
#[derive(Debug, Default)]
pub struct ColorGlyphRasterizer {
    /// `None` marks a face no source in this module can paint, so a repeated
    /// miss costs a hash lookup instead of a file read or a table walk.
    faces: HashMap<ColorFaceKey, Option<ColorFaceBytes>>,
}

impl ColorGlyphRasterizer {
    /// An empty rasterizer; faces are opened and classified on demand.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop every cached face.  Called at font-generation boundaries, where a
    /// path may now resolve to different bytes.
    pub fn clear(&mut self) {
        self.faces.clear();
    }

    /// Whether `asset` carries a color source this rasterizer can paint.
    ///
    /// Opens and classifies the face once; the answer then rides the face
    /// cache, so a caller that must decide something per face (such as
    /// whether a glyph's raster can depend on the text foreground) pays for
    /// the table walk only once.
    pub fn has_color_sources(&mut self, asset: &FontOutlineAsset) -> bool {
        self.ensure_face(asset).is_some()
    }

    /// Open, classify, and cache one exact source face.
    ///
    /// `None` means no source in this module can paint the face, including
    /// the failure cases, which stay cached so a repeated miss costs a hash
    /// lookup rather than another file read.
    fn ensure_face(&mut self, asset: &FontOutlineAsset) -> Option<&ColorFaceBytes> {
        let key = ColorFaceKey::from_asset(asset);
        if let std::collections::hash_map::Entry::Vacant(entry) = self.faces.entry(key.clone()) {
            let opened = match Self::open_color_face(asset) {
                Ok(face) => face,
                Err(error) => {
                    tracing::warn!(
                        target: "font_boundary",
                        %error,
                        "layered color glyph source is unavailable"
                    );
                    None
                }
            };
            entry.insert(opened);
        }
        self.faces.get(&key).and_then(Option::as_ref)
    }

    /// Rasterize `request` from `asset`.
    ///
    /// Returns `Ok(None)` when the face carries no color source this module
    /// implements, or when the source paints nothing for this glyph; the
    /// caller then falls back to monochrome outlines.
    pub fn rasterize(
        &mut self,
        asset: &FontOutlineAsset,
        request: &ColorGlyphRequest<'_>,
    ) -> Result<Option<ColorGlyphRaster>, ColorGlyphError> {
        if !request.px_size.is_finite() || request.px_size <= 0.0 {
            return Ok(None);
        }
        let Some(face) = self.ensure_face(asset) else {
            return Ok(None);
        };

        for source in face.sources.color_glyph_sources() {
            match source {
                ColorGlyphSource::Colr => {
                    if let Some(raster) = colr::paint_colr_glyph(face, request) {
                        return Ok(Some(raster));
                    }
                }
                ColorGlyphSource::SvgDocuments => {
                    if let Some(parsed) = face.face()
                        && let Some(raster) = svg::paint_svg_glyph(&parsed, request)
                    {
                        return Ok(Some(raster));
                    }
                }
                // Embedded color strikes rasterize through Swash's source
                // chain, which the caller drives.
                ColorGlyphSource::EmbeddedColorBitmaps => {}
            }
        }
        Ok(None)
    }

    fn open_color_face(
        asset: &FontOutlineAsset,
    ) -> Result<Option<ColorFaceBytes>, ColorGlyphError> {
        let (path, bytes, face_index) = match asset {
            FontOutlineAsset::File(asset) => {
                let path = asset.path().to_owned();
                let face_index = asset.face_index();
                // Classify from the table directory first: the overwhelming
                // majority of faces carry no color source, and reading a
                // multi-megabyte font to learn that is avoidable work.
                if classify_file(&path, face_index)?
                    .is_none_or(|sources| sources.color_glyph_sources().next().is_none())
                {
                    return Ok(None);
                }
                let bytes = std::fs::read(&path).map_err(|error| ColorGlyphError::Read {
                    path: path.clone(),
                    reason: error.to_string(),
                })?;
                (path, Arc::new(bytes), face_index)
            }
            FontOutlineAsset::Memory(asset) => (
                asset.key().to_owned(),
                asset.shared_bytes(),
                asset.face_index(),
            ),
        };
        let Some(sources) = classify_sfnt_face(&bytes[..], face_index) else {
            return Ok(None);
        };
        if sources.color_glyph_sources().next().is_none() {
            return Ok(None);
        }
        let face = ColorFaceBytes {
            bytes,
            face_index,
            sources,
        };
        if face.face().is_none() {
            return Err(ColorGlyphError::MissingFace { path, face_index });
        }
        Ok(Some(face))
    }
}

#[cfg(test)]
#[path = "color_glyph/tests/color_glyph_test.rs"]
mod tests;

#[cfg(test)]
#[path = "color_glyph/tests/support.rs"]
mod test_support;

#[cfg(test)]
#[path = "color_glyph/tests/svg_test.rs"]
mod svg_tests;

#[cfg(test)]
#[path = "color_glyph/tests/synthetic_colr_test.rs"]
mod synthetic_colr_tests;
