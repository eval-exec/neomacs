//! Which raster sources one SFNT face carries, read from its table directory.
//!
//! Two consumers share this classification and must never disagree about it:
//!
//! * container materialization (`fontdb`) decides whether fontdb/Swash can
//!   materialize the face at all, or whether only the exact FreeType replay
//!   path owns it;
//! * glyph rasterization decides which color source to try first for a glyph.
//!
//! Reading the table directory is the only way to answer either question:
//! suffixes and Fontconfig's `FC_SCALABLE` hint both lie about bitmap-only and
//! color-table fonts.

/// One color source a scalable face can rasterize a glyph from.
///
/// [`SfntFaceRasterSources::color_glyph_sources`] yields these in the order a
/// rasterizer must prefer them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorGlyphSource {
    /// Layered color outlines from `COLR`/`CPAL`.
    ///
    /// Both table versions share one source: version 1 adds paint graphs and
    /// may fall back to version 0 layer records per glyph, so one traversal
    /// covers the whole table.
    Colr,
    /// Embedded color strike bitmaps: `CBDT`/`CBLC` or `sbix`.
    EmbeddedColorBitmaps,
    /// Per-glyph SVG documents from the `SVG ` table.
    SvgDocuments,
}

/// Raster sources carried by one SFNT face.
///
/// Everything here is derived from table tags and lengths, so the value is
/// cheap to compute, copy, and hold per materialized face.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SfntFaceRasterSources {
    outline: bool,
    color_outlines: bool,
    color_bitmap_strikes: bool,
    monochrome_bitmap_strikes: bool,
    svg_documents: bool,
}

impl SfntFaceRasterSources {
    /// Monochrome scalable outlines (`glyf`/`CFF `/`CFF2`).
    #[must_use]
    pub const fn has_outline(self) -> bool {
        self.outline
    }

    /// Whether the face carries a usable layered-color table.
    ///
    /// `COLR` without `CPAL` has no palette to paint from and is not a usable
    /// color source, matching what every rasterizer accepts.  The `COLR`
    /// version is deliberately not part of this answer: one painter serves
    /// both, so nothing downstream may branch on it.
    #[must_use]
    pub const fn has_color_outlines(self) -> bool {
        self.color_outlines
    }

    /// `CBDT`/`CBLC` or `sbix` color strikes.
    #[must_use]
    pub const fn color_bitmap_strikes(self) -> bool {
        self.color_bitmap_strikes
    }

    /// `EBDT`/`EBLC` or `bdat`/`bloc` monochrome strikes.
    #[must_use]
    pub const fn monochrome_bitmap_strikes(self) -> bool {
        self.monochrome_bitmap_strikes
    }

    /// Per-glyph SVG documents from the `SVG ` table.
    #[must_use]
    pub const fn svg_documents(self) -> bool {
        self.svg_documents
    }

    /// Whether fontdb/Swash owns this face.
    ///
    /// Scalable outlines and color strikes both rasterize through fontdb.  A
    /// face with only monochrome strikes has no scalable outline to fall back
    /// on, and Swash cannot materialize its fixed strikes, so the exact
    /// FreeType replay path owns it.
    #[must_use]
    pub const fn swash_owned(self) -> bool {
        self.outline || self.color_outlines || self.color_bitmap_strikes
    }

    /// Color sources in rasterization preference order.
    ///
    /// Layered color outlines win over embedded bitmaps and SVG documents,
    /// matching FreeType/CoreText and the CSS color-font fallback order.  The
    /// monochrome outline is not a color source: callers fall back to it after
    /// exhausting this list.
    pub fn color_glyph_sources(self) -> impl Iterator<Item = ColorGlyphSource> {
        [
            self.color_outlines.then_some(ColorGlyphSource::Colr),
            self.color_bitmap_strikes
                .then_some(ColorGlyphSource::EmbeddedColorBitmaps),
            self.svg_documents.then_some(ColorGlyphSource::SvgDocuments),
        ]
        .into_iter()
        .flatten()
    }
}

/// Classify one SFNT face by reading its table directory.
///
/// Handles standalone faces and `ttcf` collections; returns `None` when the
/// bytes are not an SFNT face, which callers treat as "let fontdb decide".
#[must_use]
pub fn classify_sfnt_face(bytes: &[u8], face_index: u32) -> Option<SfntFaceRasterSources> {
    let directory = if bytes.starts_with(b"ttcf") {
        let count = read_be_u32(bytes, 8)?;
        if face_index >= count {
            return None;
        }
        read_be_u32(bytes, 12 + face_index as usize * 4)? as usize
    } else if face_index == 0 {
        0
    } else {
        return None;
    };
    classify_table_directory(bytes, directory)
}

/// Classify a face from its table directory records.
///
/// `directory` points at the 12-byte table-directory header followed by the
/// 16-byte records, either inside a whole font or inside a buffer holding just
/// the directory (the streaming container path reads one face's directory out
/// of a potentially large collection).  Only tags and lengths are consulted,
/// so a buffer that carries only the directory classifies identically.
#[must_use]
pub fn classify_table_directory(bytes: &[u8], directory: usize) -> Option<SfntFaceRasterSources> {
    let count = read_be_u16(bytes, directory + 4)? as usize;
    let mut sources = SfntFaceRasterSources::default();
    let mut has_color_bitmap_data = false;
    let mut has_color_bitmap_location = false;
    let mut has_monochrome_bitmap_data = false;
    let mut has_monochrome_bitmap_location = false;
    let mut has_cpal = false;
    let mut has_sbix = false;
    for index in 0..count {
        let record = directory + 12 + index * 16;
        let tag: [u8; 4] = bytes.get(record..record + 4)?.try_into().ok()?;
        let length = read_be_u32(bytes, record + 12)?;
        match &tag {
            b"EBDT" | b"bdat" if length != 0 => has_monochrome_bitmap_data = true,
            b"EBLC" | b"bloc" if length != 0 => has_monochrome_bitmap_location = true,
            b"CBDT" if length != 0 => has_color_bitmap_data = true,
            b"CBLC" if length != 0 => has_color_bitmap_location = true,
            b"sbix" if length != 0 => has_sbix = true,
            b"CPAL" if length != 0 => has_cpal = true,
            b"COLR" if length != 0 => sources.color_outlines = true,
            b"SVG " if length != 0 => sources.svg_documents = true,
            b"glyf" | b"CFF " | b"CFF2" if length != 0 => sources.outline = true,
            _ => {}
        }
    }
    sources.color_outlines &= has_cpal;
    sources.color_bitmap_strikes = (has_color_bitmap_data && has_color_bitmap_location) || has_sbix;
    sources.monochrome_bitmap_strikes =
        has_monochrome_bitmap_data && has_monochrome_bitmap_location;
    Some(sources)
}

fn read_be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
#[path = "raster_sources/tests/raster_sources_test.rs"]
mod tests;
