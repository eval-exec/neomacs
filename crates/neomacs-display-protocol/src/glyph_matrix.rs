//! GNU Emacs-compatible glyph matrix types for the shared display path.
//!
//! These types match the architecture of GNU Emacs's `dispextern.h`:
//! `struct glyph`, `struct glyph_row`, `struct glyph_matrix`.
//!
//! The glyph matrix is character-grid native for terminal output, but also
//! carries each glyph's realized pixel width.  GNU's `struct glyph` stores
//! `pixel_width`; GUI backends must use that rather than reconstructing every
//! glyph as one frame column.

use super::effect_config::EffectsConfig;
use super::face::{BoxVerticalEdges, Face, FaceAttributes, UnderlineStyle};
use super::frame_chrome::{FrameChrome, FrameChromeContent, PresentationId};
use super::frame_glyphs::{
    ContentTransitionHint, CursorStyle, DisplaySlotId, FrameGlyph, FrameGlyphBuffer,
    FringeBitmapData, FringeSide, GlyphRowRole, MaterializedFaceData, PhysCursor,
    PresentedWindowGeometry, WindowCursor, WindowEffectHint, WindowInfo,
};
use super::image::{ImageMargins, ImageOpaqueBackground, ImageSourceRect, RetainedImageSet};
use super::presented_pointer::PresentedPrimitiveKind;
use super::types::{
    Color, DisplayWindowId, FaceId, ImageId, Px, Rect, ResolvedBidiDirection, SurfaceId, VideoId,
    WebViewId, XwidgetId,
};
use super::xwidget_extent::{
    XwidgetContentExtent, XwidgetLayoutAdvance, XwidgetPresentationGeometry,
};
use super::{FrameSpace, GeometryPoint, GeometryRect, LogicalPixels};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::HashMap;

/// One terminal cell produced by GNU's `compose-gstring-for-terminal`.
///
/// The layout boundary resolves automatic-composition semantics while it has
/// access to Lisp's live rule table and GNU character widths.  The TTY backend
/// therefore receives a closed presentation plan instead of reinterpreting a
/// Unicode string with a second, inevitably divergent heuristic.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TerminalCompositionCell {
    pub base: char,
    pub extenders: Box<str>,
    pub width_cols: u8,
    /// Number of source characters represented by this cell.  Keeping the
    /// range length beside the cell preserves cursor/source ownership without
    /// reverse-engineering it from `extenders`.
    pub source_char_len: u16,
}

/// Complete terminal lowering of one automatic composition.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TerminalComposition {
    pub cells: Box<[TerminalCompositionCell]>,
    pub width_cols: u16,
}

impl TerminalComposition {
    /// Return the terminal cell at one visual offset without changing the
    /// cached logical-order lowering.
    #[must_use]
    pub fn visual_cell(
        &self,
        direction: ResolvedBidiDirection,
        visual_offset: usize,
    ) -> Option<&TerminalCompositionCell> {
        let logical_index =
            direction.logical_index_at_visual_offset(visual_offset, self.cells.len())?;
        self.cells.get(logical_index)
    }
}

/// What kind of content this glyph represents.
/// Matches GNU's `enum glyph_type` in `dispextern.h`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GlyphType {
    /// Regular character (including multibyte).
    Char { ch: char },
    /// Composed grapheme cluster (ligatures, emoji ZWJ, combining marks).
    Composite { text: Box<str> },
    /// A composition selected by the live Lisp
    /// `composition-function-table`.  Its terminal width comes from GNU's
    /// `compose-gstring-for-terminal`, not from Unicode grapheme width or the
    /// graphical shaper.
    AutomaticComposite {
        text: Box<str>,
        terminal: TerminalComposition,
    },
    /// Whitespace/filler — occupies `width_cols` character cells.
    Stretch { width_cols: u16 },
    /// Inline image.  Layout geometry and drawable identity travel together so
    /// an image cannot be positioned through a second, independently mutable
    /// side channel.
    Image {
        image_id: i32,
        width_cols: u16,
        source_rect: ImageSourceRect,
        margins: GlyphImageMarginsId,
        opaque_background: ImageOpaqueBackground,
    },
    /// Inline video, represented by the same row primitive that reserves its
    /// layout slot.
    Video {
        video_id: VideoId,
        width_cols: u16,
        opacity: f32,
    },
    /// Inline native/web widget.  `width_cols` and the glyph's `pixel_width`
    /// are the layout advance, which `produce_xwidget_glyph` may crop at the
    /// right edge (src/xdisp.c:32577-32579, emacs-31.0.90); `content` is the
    /// widget's own size, GNU `xw->width`/`xw->height`, which the crop never
    /// touches.
    Xwidget {
        xwidget_id: XwidgetId,
        webview_id: WebViewId,
        width_cols: u16,
        content: XwidgetContentExtent,
    },
    /// Inline shader surface (NeoMacs extension): a compositor-rendered GPU
    /// texture owned by the row primitive that reserves its layout slot.
    Surface { surface_id: i32, width_cols: u16 },
    /// Character whose visible representation is selected by
    /// `glyphless-char-display`.
    ///
    /// The presentation is part of the glyph rather than reconstructed from
    /// its width: GNU's `thin-space` is a one-pixel blank on a graphical frame
    /// but a one-cell blank on a terminal, while `empty-box` and `hex-code`
    /// have different backend projections again.
    Glyphless {
        ch: char,
        presentation: GlyphlessPresentation,
    },
}

/// A materialized, non-zero-width `glyphless-char-display` presentation.
///
/// `zero-width` is deliberately absent: it produces no glyph. Acronyms are
/// currently lowered to their explicit character run before the protocol
/// boundary. Keeping this enum closed makes it impossible for a backend to
/// mistake a thin-space glyph for the source character merely because both
/// carry the same codepoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum GlyphlessPresentation {
    ThinSpace,
    EmptyBox,
    HexCode,
}

/// GNU `enum glyph_type` discriminants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum GlyphTypeKind {
    Char = 0,
    Composite = 1,
    Glyphless = 2,
    Image = 3,
    Stretch = 4,
    Xwidget = 5,
    /// Neomacs extension; GNU currently has no distinct video glyph kind.
    Video = 6,
    /// Neomacs extension; GNU has no shader-surface glyph kind.
    Surface = 7,
}

impl GlyphTypeKind {
    pub fn from_gnu_code(code: u8) -> Option<Self> {
        (code <= Self::Xwidget as u8)
            .then(|| Self::try_from(code).ok())
            .flatten()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

impl GlyphType {
    pub fn gnu_kind(&self) -> GlyphTypeKind {
        match self {
            GlyphType::Char { .. } => GlyphTypeKind::Char,
            GlyphType::Composite { .. } | GlyphType::AutomaticComposite { .. } => {
                GlyphTypeKind::Composite
            }
            GlyphType::Glyphless { .. } => GlyphTypeKind::Glyphless,
            GlyphType::Image { .. } => GlyphTypeKind::Image,
            GlyphType::Video { .. } => GlyphTypeKind::Video,
            GlyphType::Xwidget { .. } => GlyphTypeKind::Xwidget,
            GlyphType::Surface { .. } => GlyphTypeKind::Surface,
            GlyphType::Stretch { .. } => GlyphTypeKind::Stretch,
        }
    }
}

/// Three areas within a glyph row, matching GNU's `enum glyph_row_area`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum GlyphArea {
    LeftMargin = 0,
    Text = 1,
    RightMargin = 2,
}

impl GlyphArea {
    pub const COUNT: usize = 3;
    pub const ALL: [Self; Self::COUNT] = [Self::LeftMargin, Self::Text, Self::RightMargin];

    pub fn index(self) -> usize {
        usize::from(u8::from(self))
    }

    pub fn from_gnu_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    pub fn gnu_code(self) -> u8 {
        self.into()
    }
}

/// Authoritative geometry for one of GNU's three glyph-row areas.
///
/// `bounds` determines the area's pen origin and horizontal extent. `clip`
/// preserves the presentation's vertical body band (including vscroll
/// clipping) independently of that origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphAreaGeometry {
    bounds: Rect,
    clip: Rect,
}

impl GlyphAreaGeometry {
    pub const fn new(bounds: Rect, clip: Rect) -> Self {
        Self { bounds, clip }
    }

    pub const fn bounds(self) -> Rect {
        self.bounds
    }

    pub const fn clip(self) -> Rect {
        self.clip
    }
}

/// How a glyph area obtains its horizontal pen position.
///
/// Real window margins are structural areas with independent GNU
/// `window_box_left_offset` origins. `FollowingPreviousArea` is retained only
/// for genuinely unpartitioned frame chrome. Every window row gives the right
/// area a structural origin: a configured margin when present, otherwise the
/// final matrix cell GNU reserves for a TTY vertical-border glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlyphAreaPlacement {
    FollowingPreviousArea,
    Structural(GlyphAreaGeometry),
}

/// One immutable mapping from semantic [`GlyphArea`] to presentation geometry.
///
/// Both GUI materialization and the TTY RIF consume this mapping. Keeping the
/// exhaustive area match here prevents either backend from silently flattening
/// a newly routed margin area back into ordinary text flow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphRowAreaLayout {
    left_margin: GlyphAreaPlacement,
    text: GlyphAreaPlacement,
    right_margin: GlyphAreaPlacement,
}

impl GlyphRowAreaLayout {
    pub const fn unpartitioned(bounds: Rect, clip: Rect) -> Self {
        Self {
            left_margin: GlyphAreaPlacement::FollowingPreviousArea,
            text: GlyphAreaPlacement::Structural(GlyphAreaGeometry::new(bounds, clip)),
            right_margin: GlyphAreaPlacement::FollowingPreviousArea,
        }
    }

    fn final_cell(bounds: Rect, char_width: f32) -> Rect {
        Rect::new(
            (bounds.right() - char_width).max(bounds.x),
            bounds.y,
            char_width.min(bounds.width),
            bounds.height,
        )
    }

    /// Window-wide chrome has no text-area margins, but GNU still reserves the
    /// last matrix cell as LAST_AREA for a vertical separator.  Publishing that
    /// cell structurally keeps all consumers from advancing the border past the
    /// window edge when the right-margin glyph is present.
    fn window_wide(bounds: Rect, clip: Rect, char_width: f32) -> Self {
        let right_border = Self::final_cell(bounds, char_width);
        Self {
            left_margin: GlyphAreaPlacement::FollowingPreviousArea,
            text: GlyphAreaPlacement::Structural(GlyphAreaGeometry::new(bounds, clip)),
            right_margin: GlyphAreaPlacement::Structural(GlyphAreaGeometry::new(
                right_border,
                clip,
            )),
        }
    }

    fn window_text(
        text_bounds: Rect,
        text_clip: Rect,
        left_margin: Option<Rect>,
        right_margin: Option<Rect>,
        char_width: f32,
    ) -> Self {
        let synthetic_right_border = Self::final_cell(text_bounds, char_width);
        Self {
            left_margin: left_margin
                .map(|bounds| {
                    GlyphAreaPlacement::Structural(GlyphAreaGeometry::new(bounds, bounds))
                })
                .unwrap_or(GlyphAreaPlacement::FollowingPreviousArea),
            text: GlyphAreaPlacement::Structural(GlyphAreaGeometry::new(text_bounds, text_clip)),
            right_margin: GlyphAreaPlacement::Structural(GlyphAreaGeometry::new(
                right_margin.unwrap_or(synthetic_right_border),
                right_margin.unwrap_or(text_clip),
            )),
        }
    }

    pub const fn placement(self, area: GlyphArea) -> GlyphAreaPlacement {
        match area {
            GlyphArea::LeftMargin => self.left_margin,
            GlyphArea::Text => self.text,
            GlyphArea::RightMargin => self.right_margin,
        }
    }

    /// Smallest rectangle covering every independently placed area.
    ///
    /// Incremental consumers use this to carry the complete row presentation,
    /// not just TEXT_AREA. Gaps such as a fringe are intentionally included:
    /// subsequent structural painters can overwrite them, while omitting a
    /// margin here would erase reused marginal content before diffing.
    pub fn structural_coverage(self) -> Option<Rect> {
        let mut coverage: Option<Rect> = None;
        for area in GlyphArea::ALL {
            let GlyphAreaPlacement::Structural(geometry) = self.placement(area) else {
                continue;
            };
            let bounds = geometry.bounds();
            coverage = Some(match coverage {
                None => bounds,
                Some(current) => {
                    let left = current.x.min(bounds.x);
                    let top = current.y.min(bounds.y);
                    let right = current.right().max(bounds.right());
                    let bottom = current.bottom().max(bounds.bottom());
                    Rect::new(left, top, right - left, bottom - top)
                }
            });
        }
        coverage
    }
}

/// Sentinel `charpos` for synthetic glyphs that map to no buffer position.
///
/// Glyphs appended by `extend_face_to_end_of_line` (the leading face-anchor
/// space on an empty row and the trailing background stretch) fill the
/// highlighted `:extend` background past end-of-line but cover no buffer
/// character. They carry this sentinel so cursor placement can exclude them,
/// mirroring GNU's `NILP (glyph->object)` test in `set_cursor_from_row`
/// (src/xdisp.c). A literal `0` cannot be used: real buffer text begins at
/// 0-based `charpos` `0`, so `0` is a valid position for the first glyph.
pub const NO_BUFFER_POSITION_CHARPOS: usize = usize::MAX;

/// Stable identity of a Lisp string in the producing layout session.
///
/// GNU stores the Lisp object itself in `struct glyph::object`.  The display
/// protocol cannot transport VM objects, so layout assigns the corresponding
/// source identity to a row-local [`GlyphStringSource`].  Glyphs refer to that
/// entry through [`GlyphStringSourceId`], which keeps this comparatively large
/// identity out of every cell.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct GlyphStringId(u64);

impl GlyphStringId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Exact buffer range replaced by a string-valued `display` property.
///
/// GNU recovers this association by scanning `display` properties from the
/// row bounds (`string_buffer_position_lim`).  Layout already owns the exact
/// covered range, so transporting it avoids a bounded heuristic rescan while
/// retaining GNU's separate string-index and buffer-coverage tracks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphStringBufferRange {
    start: usize,
    end: usize,
}

impl GlyphStringBufferRange {
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end: if end < start { start } else { end },
        }
    }

    pub const fn start(self) -> usize {
        self.start
    }

    pub const fn end(self) -> usize {
        self.end
    }

    pub const fn contains(self, charpos: usize) -> bool {
        self.start <= charpos && charpos < self.end
    }

    fn shifted(self, from: usize, delta: i64) -> Self {
        Self::new(
            shifted_buffer_position(self.start, from, delta),
            shifted_buffer_position(self.end, from, delta),
        )
    }
}

/// One string occurrence referenced by glyphs in a row.
///
/// GNU leaves replacement coverage out of `struct glyph` and recovers it by
/// scanning display properties.  NeoMacs already knows the exact range while
/// constructing the row, so it records that range once here.  Keeping the
/// occurrence in a side table avoids duplicating two buffer positions in every
/// glyph while preserving exact cursor and incremental-redisplay semantics.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphStringSource {
    string: GlyphStringId,
    covered_buffer: Option<GlyphStringBufferRange>,
}

impl GlyphStringSource {
    pub const fn new(string: GlyphStringId) -> Self {
        Self {
            string,
            covered_buffer: None,
        }
    }

    pub const fn replacement(
        string: GlyphStringId,
        covered_buffer: GlyphStringBufferRange,
    ) -> Self {
        Self {
            string,
            covered_buffer: Some(covered_buffer),
        }
    }

    pub const fn string(self) -> GlyphStringId {
        self.string
    }

    pub const fn covered_buffer_range(self) -> Option<GlyphStringBufferRange> {
        self.covered_buffer
    }

    pub const fn covers_buffer_charpos(self, charpos: usize) -> bool {
        match self.covered_buffer {
            Some(range) => range.contains(charpos),
            None => false,
        }
    }

    fn shift_buffer_positions(&mut self, from: usize, delta: i64) {
        if let Some(range) = self.covered_buffer {
            self.covered_buffer = Some(range.shifted(from, delta));
        }
    }
}

/// Compact row-local handle to a [`GlyphStringSource`].
///
/// The non-zero representation gives `Option<GlyphStringSourceId>` the same
/// four-byte layout, and makes an arbitrary integer unusable as a string
/// source without an explicit checked conversion.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct GlyphStringSourceId(std::num::NonZeroU32);

impl GlyphStringSourceId {
    pub fn from_index(index: usize) -> Option<Self> {
        let value = u32::try_from(index.checked_add(1)?).ok()?;
        std::num::NonZeroU32::new(value).map(Self)
    }

    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// The coordinate space and source of one glyph's position.
///
/// This is GNU's `(glyph->charpos, glyph->object)` pair as one closed value.
/// A bare integer is deliberately unavailable: consumers must distinguish a
/// buffer position, an index in a particular string, and redisplay-owned
/// output before interpreting it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GlyphProvenance {
    Buffer {
        charpos: usize,
    },
    Str {
        source: GlyphStringSourceId,
        index: usize,
    },
    Redisplay(RedisplayGlyphProvenance),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RedisplayGlyphProvenance {
    LineEnd,
    Mark,
    EmptyLineNewline { charpos: usize },
}

impl GlyphProvenance {
    pub const fn buffer(charpos: usize) -> Self {
        Self::Buffer { charpos }
    }

    pub const fn string(source: GlyphStringSourceId, index: usize) -> Self {
        Self::Str { source, index }
    }

    pub const fn line_end() -> Self {
        Self::Redisplay(RedisplayGlyphProvenance::LineEnd)
    }

    pub const fn mark() -> Self {
        Self::Redisplay(RedisplayGlyphProvenance::Mark)
    }

    pub const fn empty_line_newline(charpos: usize) -> Self {
        Self::Redisplay(RedisplayGlyphProvenance::EmptyLineNewline { charpos })
    }

    pub const fn buffer_charpos(self) -> Option<usize> {
        match self {
            Self::Buffer { charpos } => Some(charpos),
            Self::Str { .. } | Self::Redisplay(_) => None,
        }
    }

    pub const fn string_index(self) -> Option<(GlyphStringSourceId, usize)> {
        match self {
            Self::Str { source, index } => Some((source, index)),
            Self::Buffer { .. } | Self::Redisplay(_) => None,
        }
    }

    pub fn shifted_buffer_positions(self, from: usize, delta: i64) -> Self {
        match self {
            Self::Buffer { charpos } => Self::buffer(shifted_buffer_position(charpos, from, delta)),
            Self::Redisplay(RedisplayGlyphProvenance::EmptyLineNewline { charpos }) => {
                Self::empty_line_newline(shifted_buffer_position(charpos, from, delta))
            }
            Self::Str { .. } | Self::Redisplay(_) => self,
        }
    }

    pub const fn advanced_by(self, char_offset: usize) -> Self {
        match self {
            Self::Buffer { charpos } => Self::buffer(charpos.saturating_add(char_offset)),
            Self::Str { source, index } => Self::Str {
                source,
                index: index.saturating_add(char_offset),
            },
            Self::Redisplay(sentinel) => Self::Redisplay(sentinel),
        }
    }

    /// Legacy numeric stamp for diagnostics and parity snapshots only.
    /// Behavioral consumers should match the enum instead.
    pub const fn legacy_charpos(self) -> usize {
        match self {
            Self::Buffer { charpos } => charpos,
            Self::Str { index, .. } => index,
            Self::Redisplay(RedisplayGlyphProvenance::EmptyLineNewline { charpos }) => charpos,
            Self::Redisplay(RedisplayGlyphProvenance::LineEnd | RedisplayGlyphProvenance::Mark) => {
                NO_BUFFER_POSITION_CHARPOS
            }
        }
    }
}

fn shifted_buffer_position(value: usize, from: usize, delta: i64) -> usize {
    if value < from {
        return value;
    }
    (value as i64).saturating_add(delta).max(0) as usize
}

/// One character cell on screen.
/// Equivalent to GNU's `struct glyph` in `dispextern.h`.
///
/// Grid-native: no pixel coordinates. Screen position is determined by
/// the row index in `GlyphRow` and position within the area's glyph vector.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Glyph {
    /// What this glyph displays.
    pub glyph_type: GlyphType,
    /// Face ID for looking up colors, font, and decoration.
    pub face_id: FaceId,
    /// Typed source position.  The number cannot be interpreted without its
    /// buffer/string/redisplay discriminator.
    pub provenance: GlyphProvenance,
    /// Bidirectional resolved level (0 = LTR base, 1 = RTL, etc.).
    pub bidi_level: u8,
    /// True for double-width characters (CJK, etc.).
    pub wide: bool,
    /// Realized glyph advance in pixels.
    ///
    /// `0.0` means "not explicitly measured"; materialization falls back to
    /// character-grid width.  TTY backends ignore this field.
    pub pixel_width: f32,
    /// Stretch-glyph layout height in pixels.
    ///
    /// GNU's `struct glyph` stores stretch height/ascent in
    /// `glyph->u.stretch`.  These metrics contribute to the containing row's
    /// ascent and height; they are not the stretch face's paint rectangle.
    /// GNU paints every glyph string background with `row->y` and
    /// `row->height`. `0.0` means "use the containing row metrics".
    pub pixel_height: f32,
    /// Stretch-glyph layout ascent in pixels.
    ///
    /// Used with `pixel_height` while constructing the containing row.
    /// `0.0` falls back to row ascent.
    pub pixel_ascent: f32,
    /// Glyph vertical offset in pixels.
    ///
    /// Mirrors GNU `struct glyph::voffset`: negative values raise the
    /// glyph, positive values lower it.
    pub vertical_offset_px: f32,
    /// Padding glyph — second cell of a wide character.
    pub padding: bool,
    /// GNU `left_box_line_p` / `right_box_line_p` ownership for this glyph.
    /// Top and bottom box edges are inherent in the resolved boxed face.
    /// Layout publishes this source-derived fact while it still owns the
    /// iterator; transport and rendering must not infer it from visible rows.
    #[serde(default)]
    pub box_vertical_edges: BoxVerticalEdges,
    /// Layout-owned pointer appearance identity carried transactionally with
    /// the authoritative glyph through rollback, bidi reorder, and row reuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer_appearance: Option<GlyphPointerAppearanceId>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct GlyphPointerAppearanceId(std::num::NonZeroU32);

impl GlyphPointerAppearanceId {
    pub fn from_index(index: usize) -> Option<Self> {
        let value = u32::try_from(index.checked_add(1)?).ok()?;
        std::num::NonZeroU32::new(value).map(Self)
    }

    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// Compact row-local reference to one image's four-sided margin geometry.
///
/// Images are rare, while every text glyph pays for the largest [`GlyphType`]
/// payload. Keeping the full asymmetric margins in [`GlyphRow`] preserves GNU
/// image geometry without enlarging every ordinary character glyph.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct GlyphImageMarginsId(std::num::NonZeroU16);

impl GlyphImageMarginsId {
    fn from_index(index: usize) -> Option<Self> {
        let value = u16::try_from(index.checked_add(1)?).ok()?;
        std::num::NonZeroU16::new(value).map(Self)
    }

    fn index(self) -> usize {
        usize::from(self.0.get()) - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphPointerAppearance {
    pub source: GlyphPointerSourceIdentity,
    pub face_id: FaceId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphPointerSourceIdentity {
    pub kind: GlyphPointerSourceKind,
    pub source_id: u64,
    pub range_start: u64,
    pub range_end: u64,
    pub property_owner: u64,
    pub occurrence: GlyphPointerOccurrenceIdentity,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GlyphPointerSourceKind {
    Buffer,
    LispString,
    Synthetic,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GlyphPointerOccurrenceIdentity {
    Source,
    OverlayString { overlay_id: u64, after: bool },
    BufferDisplayReplacement { buffer_id: u64, anchor: u64 },
}

impl Glyph {
    #[must_use]
    pub const fn resolved_bidi_direction(&self) -> ResolvedBidiDirection {
        ResolvedBidiDirection::from_level(self.bidi_level)
    }

    /// Create a simple character glyph with default attributes.
    pub fn char(ch: char, face_id: FaceId, charpos: usize) -> Self {
        Self::char_with_provenance(ch, face_id, GlyphProvenance::buffer(charpos))
    }

    /// Create a simple character glyph with typed source provenance.
    pub fn char_with_provenance(ch: char, face_id: FaceId, provenance: GlyphProvenance) -> Self {
        Self {
            glyph_type: GlyphType::Char { ch },
            face_id,
            provenance,
            bidi_level: 0,
            wide: false,
            pixel_width: 0.0,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: false,
            box_vertical_edges: BoxVerticalEdges::Unboxed,
            pointer_appearance: None,
        }
    }

    /// Create a stretch (whitespace) glyph.
    pub fn stretch(width_cols: u16, face_id: FaceId) -> Self {
        Self::stretch_with_provenance(width_cols, face_id, GlyphProvenance::buffer(0))
    }

    /// Create a stretch glyph with typed source provenance.
    pub fn stretch_with_provenance(
        width_cols: u16,
        face_id: FaceId,
        provenance: GlyphProvenance,
    ) -> Self {
        Self {
            glyph_type: GlyphType::Stretch { width_cols },
            face_id,
            provenance,
            bidi_level: 0,
            wide: false,
            pixel_width: 0.0,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: false,
            box_vertical_edges: BoxVerticalEdges::Unboxed,
            pointer_appearance: None,
        }
    }

    /// Create a padding glyph (second cell of a wide character).
    pub fn padding_for(face_id: FaceId, charpos: usize) -> Self {
        Self::padding_with_provenance(face_id, GlyphProvenance::buffer(charpos))
    }

    /// Create a padding glyph with typed source provenance.
    pub fn padding_with_provenance(face_id: FaceId, provenance: GlyphProvenance) -> Self {
        Self {
            glyph_type: GlyphType::Char { ch: ' ' },
            face_id,
            provenance,
            bidi_level: 0,
            wide: false,
            pixel_width: 0.0,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: true,
            box_vertical_edges: BoxVerticalEdges::Unboxed,
            pointer_appearance: None,
        }
    }

    /// Return a copy with explicit GUI pixel advance.
    pub fn with_pixel_width(mut self, pixel_width: f32) -> Self {
        self.pixel_width = if pixel_width.is_finite() && pixel_width > 0.0 {
            pixel_width
        } else {
            0.0
        };
        self
    }

    /// Return a copy with explicit GNU vertical box-edge ownership.
    #[cfg(test)]
    pub(crate) fn with_box_vertical_edges(mut self, edges: BoxVerticalEdges) -> Self {
        self.box_vertical_edges = edges;
        self
    }

    /// Return a copy with explicit GUI stretch geometry.
    pub fn with_pixel_geometry(
        mut self,
        pixel_width: f32,
        pixel_height: f32,
        pixel_ascent: f32,
    ) -> Self {
        self.pixel_width = if pixel_width.is_finite() && pixel_width > 0.0 {
            pixel_width
        } else {
            0.0
        };
        self.pixel_height = if pixel_height.is_finite() && pixel_height > 0.0 {
            pixel_height
        } else {
            0.0
        };
        self.pixel_ascent = if self.pixel_height > 0.0 && pixel_ascent.is_finite() {
            pixel_ascent.max(0.0).min(self.pixel_height)
        } else {
            0.0
        };
        self
    }

    pub fn with_vertical_offset(mut self, vertical_offset_px: f32) -> Self {
        self.vertical_offset_px = if vertical_offset_px.is_finite() {
            vertical_offset_px
        } else {
            0.0
        };
        self
    }

    pub const fn with_provenance(mut self, provenance: GlyphProvenance) -> Self {
        self.provenance = provenance;
        self
    }

    /// Numeric projection retained for diagnostics and parity snapshots.
    ///
    /// Layout behavior must inspect [`Self::provenance`] instead so a string
    /// index can never be mistaken for a buffer position.
    pub const fn legacy_charpos(&self) -> usize {
        self.provenance.legacy_charpos()
    }

    /// Number of source-identity slots occupied by this displayed primitive.
    ///
    /// Pixel-sized stretch glyphs can be narrower than one nominal character
    /// cell, so their rounded `width_cols` is zero even though they still
    /// materialize as an addressable primitive.  Such a primitive owns one
    /// slot; otherwise the following glyph would receive the same
    /// [`DisplaySlotId`].  Keep every consumer of visual columns on this one
    /// rule so rendering, pointer paint, and cursor lookup cannot diverge.
    pub fn materialized_slot_span(&self) -> u16 {
        if self.padding {
            return 0;
        }
        match self.glyph_type {
            GlyphType::AutomaticComposite { ref terminal, .. } => terminal.width_cols.max(1),
            GlyphType::Stretch { width_cols } => width_cols.max(1),
            GlyphType::Image { width_cols, .. }
            | GlyphType::Video { width_cols, .. }
            | GlyphType::Xwidget { width_cols, .. }
            | GlyphType::Surface { width_cols, .. } => width_cols.max(1),
            _ if self.wide => 2,
            _ => 1,
        }
    }

    /// Pixel advance used when this glyph is materialized into a row.
    ///
    /// This is the measured counterpart of [`Self::materialized_slot_span`].
    /// Keeping both on `Glyph` lets cursor placement and presentation walk the
    /// same geometry instead of reconstructing advances from a column delta.
    pub fn materialized_pixel_advance(&self, char_width: f32) -> f32 {
        if self.pixel_width > 0.0 {
            self.pixel_width
        } else {
            match &self.glyph_type {
                GlyphType::Stretch { width_cols } => *width_cols as f32 * char_width,
                GlyphType::AutomaticComposite { terminal, .. } => {
                    terminal.width_cols as f32 * char_width
                }
                _ if self.wide => char_width * 2.0,
                _ => char_width,
            }
        }
    }

    /// Pointer-paint primitive produced when this glyph is materialized.
    ///
    /// Keep this exhaustive beside [`GlyphType`]: a pointer run may only name
    /// a primitive class the presentation protocol can actually address.
    /// Media/widget kinds without a pointer-paint protocol are deliberately
    /// absent instead of being mislabeled as text glyphs.
    pub const fn pointer_primitive_kind(&self) -> Option<PresentedPrimitiveKind> {
        match self.glyph_type {
            GlyphType::Char { .. }
            | GlyphType::Composite { .. }
            | GlyphType::AutomaticComposite { .. }
            | GlyphType::Stretch { .. }
            | GlyphType::Glyphless { .. } => Some(PresentedPrimitiveKind::Glyph),
            GlyphType::Image { .. } => Some(PresentedPrimitiveKind::Image),
            GlyphType::Video { .. } | GlyphType::Xwidget { .. } | GlyphType::Surface { .. } => None,
        }
    }
}

/// One screen row. Equivalent to GNU's `struct glyph_row`.
///
/// Contains three glyph areas (left margin, text, right margin) matching
/// GNU's layout. Row hashing enables fast diff: if hashes match, the rows
/// are likely identical; if they differ, the row needs redrawing.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphRow {
    /// Glyphs per area: [left_margin, text, right_margin].
    pub glyphs: [Vec<Glyph>; GlyphArea::COUNT],
    /// Pointer appearances referenced by compact glyph-local tokens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pointer_appearances: Vec<GlyphPointerAppearance>,
    /// Four-sided image margins referenced by compact glyph-local tokens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    image_margins: Vec<ImageMargins>,
    /// String occurrences referenced by compact glyph-local source tokens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    string_sources: Vec<GlyphStringSource>,
    /// Coalesced visual-order pointer runs derived once at row finalization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pointer_runs: Vec<GlyphPointerRun>,
    /// Row hash for fast diff. 0 = not yet computed.
    pub hash: u64,
    /// Incremental-redisplay provenance owned by this exact visual row.
    ///
    /// Keeping damage beside the row hash makes it impossible to shift a
    /// parallel damage vector out of alignment with `GlyphMatrix::rows`.
    /// Row is valid and should be displayed.
    pub enabled: bool,
    /// Semantic role: text body, mode-line, header-line, tab-line, etc.
    pub role: GlyphRowRole,
    /// Cursor column in this row, if cursor is here.
    pub cursor_col: Option<u16>,
    /// Cursor type when cursor is in this row.
    pub cursor_type: Option<CursorStyle>,
    /// Row has been truncated on the left.
    pub truncated_left: bool,
    /// Row has a continuation mark on the right.
    pub continued: bool,
    /// Row's paragraph base direction is right-to-left. GNU `reversed_p`: such
    /// rows are displayed flush to the right margin, with the empty space to
    /// the left of the leftmost glyph filled by the background. Row
    /// materialization offsets the glyphs to the right edge accordingly.
    pub reversed_p: bool,
    /// Row displays actual buffer text (not blank filler).
    pub displays_text: bool,
    /// Row ends at end of buffer.
    pub ends_at_zv: bool,
    /// This is a mode-line, header-line, or tab-line row.
    pub mode_line: bool,
    /// Row start relative to the containing row area's left edge.
    ///
    /// Mirrors GNU `struct glyph_row::x`.  Keeping this on the row makes the
    /// horizontal pen position authoritative for every consumer instead of
    /// letting non-text primitives carry an independent placement.
    #[serde(default)]
    pub pixel_x: f32,
    /// First materialized display-slot column owned by this row.
    ///
    /// Pixel placement cannot recover this value for proportionally-spaced
    /// text, so source identity travels beside `pixel_x` and is consumed by
    /// cursor, hit-test, GUI, and TTY adapters alike.
    #[serde(default)]
    pub start_col: u16,
    /// Row top relative to the containing window's origin.
    ///
    /// Mirrors GNU `struct glyph_row::y`. `height_px == 0.0` means
    /// the row still relies on legacy implicit grid placement.
    pub pixel_y: f32,
    /// Authoritative row height in pixels.
    ///
    /// Mirrors GNU `struct glyph_row::height`. `0.0` means unset.
    pub height_px: f32,
    /// Authoritative baseline ascent from row top in pixels.
    ///
    /// Mirrors GNU `struct glyph_row::ascent`. `0.0` means unset.
    pub ascent_px: f32,
    /// Buffer position at start of this row.
    pub start_charpos: usize,
    /// Buffer position at end of this row.
    pub end_charpos: usize,
    /// Fringe bitmap to draw in this row's LEFT fringe, if any. GNU records the
    /// per-row fringe bitmap on `struct glyph_row::left_fringe_bitmap`.
    pub left_fringe_bitmap: Option<FringeBitmapInfo>,
    /// Fringe bitmap to draw in this row's RIGHT fringe, if any. Reserved for
    /// the right-fringe path (not yet emitted downstream).
    pub right_fringe_bitmap: Option<FringeBitmapInfo>,
    /// Overlay-arrow bitmap for this row, drawn in the LEFT fringe.
    ///
    /// Mirrors GNU `struct glyph_row::overlay_arrow_bitmap`, which is a slot
    /// of its own rather than a second writer of `left_fringe_bitmap`: GNU
    /// draws the arrow *in addition to* the row's left fringe bitmap
    /// (`src/fringe.c` `draw_fringe_bitmap`, the trailing
    /// `if (left_p && row->overlay_arrow_bitmap != NO_FRINGE_BITMAP)`), and
    /// `fringe-bitmaps-at-pos` reports the two independently.
    pub overlay_arrow_bitmap: Option<FringeBitmapInfo>,
}

/// Per-row fringe-bitmap reference: the resolved registry index and the face id
/// used for its foreground/background colors. The actual bits live once per
/// frame in `FrameGlyphBuffer::fringe_bitmaps`, keyed by `bitmap_index`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FringeBitmapInfo {
    pub bitmap_index: u16,
    pub face_id: FaceId,
}

impl GlyphRow {
    pub fn new(role: GlyphRowRole) -> Self {
        Self {
            glyphs: std::array::from_fn(|_| Vec::new()),
            pointer_appearances: Vec::new(),
            image_margins: Vec::new(),
            string_sources: Vec::new(),
            pointer_runs: Vec::new(),
            hash: 0,
            enabled: true,
            role,
            cursor_col: None,
            cursor_type: None,
            truncated_left: false,
            continued: false,
            reversed_p: false,
            displays_text: false,
            ends_at_zv: false,
            mode_line: false,
            pixel_x: 0.0,
            start_col: 0,
            pixel_y: 0.0,
            height_px: 0.0,
            ascent_px: 0.0,
            start_charpos: 0,
            end_charpos: 0,
            left_fringe_bitmap: None,
            right_fringe_bitmap: None,
            overlay_arrow_bitmap: None,
        }
    }

    /// Compute FNV-1a hash over all glyph areas.
    /// Returns 0 for empty rows (sentinel meaning "not computed").
    pub fn compute_hash(&self) -> u64 {
        let total: usize = self.glyphs.iter().map(|a| a.len()).sum();
        if total == 0 {
            return 0;
        }

        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET;
        for area in &self.glyphs {
            for glyph in area {
                let ch_val = match &glyph.glyph_type {
                    GlyphType::Char { ch } => *ch as u64,
                    GlyphType::Composite { text } => {
                        let mut h = 0x1000_0000u64;
                        for b in text.bytes() {
                            h = h.wrapping_mul(31).wrapping_add(b as u64);
                        }
                        h
                    }
                    GlyphType::AutomaticComposite { text, terminal } => {
                        let mut h = 0x3000_0000u64;
                        for b in text.bytes() {
                            h = h.wrapping_mul(31).wrapping_add(u64::from(b));
                        }
                        h = h
                            .wrapping_mul(31)
                            .wrapping_add(u64::from(terminal.width_cols));
                        for cell in &terminal.cells {
                            h = h.wrapping_mul(31).wrapping_add(cell.base as u64);
                            for b in cell.extenders.bytes() {
                                h = h.wrapping_mul(31).wrapping_add(u64::from(b));
                            }
                            h = h.wrapping_mul(31).wrapping_add(u64::from(cell.width_cols));
                            h = h
                                .wrapping_mul(31)
                                .wrapping_add(u64::from(cell.source_char_len));
                        }
                        h
                    }
                    GlyphType::Stretch { width_cols } => 0x8000_0000 | (*width_cols as u64),
                    GlyphType::Image {
                        image_id,
                        width_cols,
                        source_rect,
                        margins: margins_id,
                        opaque_background,
                    } => {
                        0x4000_0000
                            ^ (*image_id as u64)
                            ^ u64::from(*width_cols).rotate_left(3)
                            ^ u64::from(source_rect.x().to_bits()).rotate_left(5)
                            ^ u64::from(source_rect.y().to_bits()).rotate_left(9)
                            ^ u64::from(source_rect.width().to_bits()).rotate_left(15)
                            ^ u64::from(source_rect.height().to_bits()).rotate_left(21)
                            ^ self
                                .image_margins(*margins_id)
                                .copied()
                                .unwrap_or_default()
                                .packed()
                                .rotate_left(7)
                            ^ u64::from(opaque_background.packed()).rotate_left(19)
                    }
                    GlyphType::Video {
                        video_id,
                        width_cols,
                        opacity,
                    } => {
                        0x6000_0000
                            ^ u64::from(video_id.get())
                            ^ u64::from(*width_cols).rotate_left(5)
                            ^ u64::from(opacity.to_bits()).rotate_left(29)
                    }
                    GlyphType::Xwidget {
                        xwidget_id,
                        webview_id,
                        width_cols,
                        content,
                    } => {
                        0x5000_0000
                            ^ u64::from(xwidget_id.get())
                            ^ u64::from(webview_id.get()).rotate_left(17)
                            ^ u64::from(*width_cols).rotate_left(9)
                            ^ u64::from(content.width_px().to_bits()).rotate_left(29)
                            ^ u64::from(content.height_px().to_bits()).rotate_left(41)
                    }
                    GlyphType::Surface {
                        surface_id,
                        width_cols,
                    } => 0x7000_0000 ^ (*surface_id as u64) ^ u64::from(*width_cols).rotate_left(5),
                    GlyphType::Glyphless { ch, presentation } => {
                        0x2000_0000 | (*ch as u64) | ((*presentation as u64) << 32)
                    }
                };
                hash ^= ch_val;
                hash = hash.wrapping_mul(FNV_PRIME);
                hash ^= glyph.face_id.get() as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
                hash ^= u64::from(glyph.box_vertical_edges.hash_code());
                hash = hash.wrapping_mul(FNV_PRIME);
                hash ^= glyph.pixel_width.to_bits() as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
                hash ^= glyph.pixel_height.to_bits() as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
                hash ^= glyph.pixel_ascent.to_bits() as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
                hash ^= glyph.vertical_offset_px.to_bits() as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        }
        // Placement and direction are part of a row's presentation identity.
        hash ^= self.pixel_x.to_bits() as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash ^= u64::from(self.start_col);
        hash = hash.wrapping_mul(FNV_PRIME);
        hash ^= self.reversed_p as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash
    }

    pub fn row_equal(&self, other: &GlyphRow) -> bool {
        if self.hash != 0 && other.hash != 0 && self.hash != other.hash {
            return false;
        }
        if self.reversed_p != other.reversed_p {
            return false;
        }
        if self.pixel_x != other.pixel_x || self.start_col != other.start_col {
            return false;
        }
        if self.pointer_appearances != other.pointer_appearances {
            return false;
        }
        if self.image_margins != other.image_margins {
            return false;
        }
        if self.string_sources != other.string_sources {
            return false;
        }
        if self.pointer_runs != other.pointer_runs {
            return false;
        }
        for area in GlyphArea::ALL {
            if self.glyphs[area.index()].len() != other.glyphs[area.index()].len() {
                return false;
            }
            for (a, b) in self.glyphs[area.index()]
                .iter()
                .zip(other.glyphs[area.index()].iter())
            {
                if a != b {
                    return false;
                }
            }
        }
        true
    }

    pub fn used(&self, area: GlyphArea) -> usize {
        self.glyphs[area.index()].len()
    }

    pub fn total_glyphs(&self) -> usize {
        self.glyphs.iter().map(Vec::len).sum()
    }

    pub fn intern_pointer_appearance(
        &mut self,
        appearance: GlyphPointerAppearance,
    ) -> Option<GlyphPointerAppearanceId> {
        if let Some(index) = self
            .pointer_appearances
            .iter()
            .position(|candidate| *candidate == appearance)
        {
            return GlyphPointerAppearanceId::from_index(index);
        }
        let id = GlyphPointerAppearanceId::from_index(self.pointer_appearances.len())?;
        self.pointer_appearances.push(appearance);
        Some(id)
    }

    pub fn pointer_appearance(
        &self,
        id: GlyphPointerAppearanceId,
    ) -> Option<&GlyphPointerAppearance> {
        self.pointer_appearances.get(id.index())
    }

    pub fn pointer_appearances(&self) -> &[GlyphPointerAppearance] {
        &self.pointer_appearances
    }

    /// Intern one image's asymmetric margins in this row.
    ///
    /// The returned token cannot outlive or be interpreted without this row.
    /// A row can contain at most `u16::MAX` distinct margin tuples; exceeding
    /// that bound makes the caller reject the image rather than truncate it.
    pub fn intern_image_margins(&mut self, margins: ImageMargins) -> Option<GlyphImageMarginsId> {
        if let Some(index) = self
            .image_margins
            .iter()
            .position(|candidate| *candidate == margins)
        {
            return GlyphImageMarginsId::from_index(index);
        }
        let id = GlyphImageMarginsId::from_index(self.image_margins.len())?;
        self.image_margins.push(margins);
        Some(id)
    }

    pub fn image_margins(&self, id: GlyphImageMarginsId) -> Option<&ImageMargins> {
        self.image_margins.get(id.index())
    }

    pub fn image_margins_table(&self) -> &[ImageMargins] {
        &self.image_margins
    }

    /// Register one displayed string occurrence in this row.
    ///
    /// Entries are intentionally not interned: the same Lisp string object may
    /// be displayed more than once with different replacement coverage, and a
    /// token denotes the occurrence rather than merely the object.
    pub fn push_string_source(&mut self, source: GlyphStringSource) -> Option<GlyphStringSourceId> {
        let id = GlyphStringSourceId::from_index(self.string_sources.len())?;
        self.string_sources.push(source);
        Some(id)
    }

    pub fn string_source(&self, id: GlyphStringSourceId) -> Option<&GlyphStringSource> {
        self.string_sources.get(id.index())
    }

    pub fn string_sources(&self) -> &[GlyphStringSource] {
        &self.string_sources
    }

    /// Whether this row maps `glyph` to the requested buffer character.
    ///
    /// String indices are deliberately never compared with buffer positions;
    /// their optional replacement coverage is resolved through the row-local
    /// source table instead.
    pub fn glyph_covers_buffer_charpos(&self, glyph: &Glyph, charpos: usize) -> bool {
        match glyph.provenance {
            GlyphProvenance::Buffer { charpos: source } => source == charpos,
            GlyphProvenance::Str { source, .. } => self
                .string_source(source)
                .is_some_and(|source| source.covers_buffer_charpos(charpos)),
            GlyphProvenance::Redisplay(_) => false,
        }
    }

    pub fn shift_string_source_buffer_positions(&mut self, from: usize, delta: i64) {
        for source in &mut self.string_sources {
            source.shift_buffer_positions(from, delta);
        }
    }

    /// Every face definition required to replay this row faithfully.
    ///
    /// Pointer appearances are render dependencies even when none of the row's
    /// glyphs use their hover/pressed face as their normal paint face.
    pub fn referenced_face_ids(&self) -> impl Iterator<Item = FaceId> + '_ {
        self.glyphs
            .iter()
            .flatten()
            .map(|glyph| glyph.face_id)
            .chain(
                self.pointer_appearances
                    .iter()
                    .map(|appearance| appearance.face_id),
            )
    }

    pub fn pointer_runs(&self) -> &[GlyphPointerRun] {
        &self.pointer_runs
    }

    pub fn rebuild_pointer_runs(&mut self, char_width: f32, row_width: f32) {
        self.pointer_runs.clear();
        let used_width = self.glyphs[GlyphArea::Text.index()]
            .iter()
            .filter(|glyph| !glyph.padding)
            .map(|glyph| glyph.materialized_pixel_advance(char_width))
            .sum::<f32>();
        let mut x = if self.reversed_p {
            (row_width - used_width).max(0.0)
        } else {
            0.0
        };
        let mut col = 0u32;
        for area in &self.glyphs {
            for glyph in area {
                if glyph.padding {
                    continue;
                }
                let width = glyph.materialized_pixel_advance(char_width);
                let col_width = u32::from(glyph.materialized_slot_span());
                if let Some(appearance) = glyph.pointer_appearance
                    && let Some(kind) = glyph.pointer_primitive_kind()
                {
                    if let Some(previous) = self.pointer_runs.last_mut()
                        && previous.appearance == appearance
                        && previous.kind == kind
                        && previous.first_col + previous.col_len == col
                    {
                        previous.col_len = previous.col_len.saturating_add(col_width);
                        previous.glyph_len = previous.glyph_len.saturating_add(1);
                        previous.width += width;
                    } else {
                        self.pointer_runs.push(GlyphPointerRun {
                            appearance,
                            kind,
                            first_col: col,
                            col_len: col_width,
                            glyph_len: 1,
                            x,
                            width,
                        });
                    }
                }
                x += width;
                col = col.saturating_add(col_width);
            }
        }
    }

    /// Adjust the buffer positions carried by this row's pointer appearances
    /// for an insertion of `delta` characters at `from` (0-based charpos, the
    /// same space as glyph `charpos`): endpoints at or past the insertion
    /// point move; endpoints before it stay. Buffer-kind source ranges and
    /// buffer display-replacement anchors are positional; Lisp-string ranges
    /// are string-internal offsets and synthetic sources carry ids, so both
    /// are left untouched. Used by the edit fast path's below-reuse so reused
    /// rows report the same pointer identities a full rebuild produces.
    pub fn shift_pointer_appearance_buffer_positions(&mut self, from: u64, delta: u64) {
        for appearance in &mut self.pointer_appearances {
            let identity = &mut appearance.source;
            if identity.kind == GlyphPointerSourceKind::Buffer {
                if identity.range_start >= from {
                    identity.range_start += delta;
                }
                if identity.range_end >= from {
                    identity.range_end += delta;
                }
            }
            if let GlyphPointerOccurrenceIdentity::BufferDisplayReplacement { anchor, .. } =
                &mut identity.occurrence
                && *anchor >= from
            {
                *anchor += delta;
            }
        }
    }

    pub fn truncate_pointer_appearances(&mut self, len: usize) {
        self.pointer_appearances.truncate(len);
    }

    pub fn truncate_image_margins(&mut self, len: usize) {
        self.image_margins.truncate(len);
    }

    pub fn truncate_string_sources(&mut self, len: usize) {
        self.string_sources.truncate(len);
    }

    pub fn clear(&mut self) {
        for area in &mut self.glyphs {
            area.clear();
        }
        self.hash = 0;
        self.pointer_appearances.clear();
        self.image_margins.clear();
        self.string_sources.clear();
        self.pointer_runs.clear();
        self.cursor_col = None;
        self.cursor_type = None;
        self.truncated_left = false;
        self.continued = false;
        self.reversed_p = false;
        self.displays_text = false;
        self.ends_at_zv = false;
        self.pixel_y = 0.0;
        self.height_px = 0.0;
        self.ascent_px = 0.0;
        self.start_charpos = 0;
        self.end_charpos = 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphPointerRun {
    pub appearance: GlyphPointerAppearanceId,
    pub kind: PresentedPrimitiveKind,
    pub first_col: u32,
    pub col_len: u32,
    pub glyph_len: u32,
    pub x: f32,
    pub width: f32,
}

/// Copy-on-write row handle. Rows under construction are uniquely owned, so
/// [`MatrixRow::make_mut`] mutates in place for free; once a frame is
/// accepted the same rows are SHARED by refcount between the sealed
/// presentation, the retained per-window matrix, and any replay plan built
/// from it — the Rust equivalent of GNU's pointer-swapped current/desired
/// matrices (dispnew.c never copies row contents either). Cloning a matrix or
/// reusing a row costs a refcount bump, not a per-glyph deep copy; the first
/// mutation of a shared row (e.g. re-decorating the cursor on a reused row)
/// clones just that row.
///
/// A NEWTYPE (not an `Arc` alias) so the mutation discipline is
/// compiler-enforced: reads go through `Deref`, and the only write path is
/// `make_mut`, which makes the cheap-verbatim vs copy-on-write distinction
/// explicit at every call site.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct MatrixRow(std::sync::Arc<GlyphRow>);

impl MatrixRow {
    pub fn new(row: GlyphRow) -> Self {
        Self(std::sync::Arc::new(row))
    }

    /// Mutable access: in-place when uniquely owned (rows under
    /// construction), copy-on-write when shared (reused rows).
    pub fn make_mut(this: &mut Self) -> &mut GlyphRow {
        std::sync::Arc::make_mut(&mut this.0)
    }
}

impl std::ops::Deref for MatrixRow {
    type Target = GlyphRow;

    fn deref(&self) -> &GlyphRow {
        &self.0
    }
}

impl AsRef<GlyphRow> for MatrixRow {
    fn as_ref(&self) -> &GlyphRow {
        &self.0
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GlyphMatrix {
    pub rows: Vec<MatrixRow>,
    /// Per-row layout provenance for THIS frame (spec 4.6), parallel to
    /// `rows`. Damage lives beside — not inside — the copy-on-write rows:
    /// it is per-frame transient metadata, and stamping it on a shared
    /// `MatrixRow` would force a deep copy of every reused row each frame.
    /// PRIVATE so alignment with `rows` is owned entirely by the accessors
    /// (`row_damage`/`set_row_damage`) and the resize paths — external code
    /// cannot desynchronize the two vectors.
    #[serde(default)]
    row_damage: Vec<RowDamage>,
    pub nrows: usize,
    pub ncols: usize,
    pub matrix_x: usize,
    pub matrix_y: usize,
    pub header_line: bool,
    pub tab_line: bool,
}

impl GlyphMatrix {
    pub fn new(nrows: usize, ncols: usize) -> Self {
        // Matrix rows start disabled. `begin_row` and
        // `begin_status_line_row` flip `enabled = true` for rows
        // that are actually populated during a frame. Rows the
        // walker skips (below-the-text scratch rows, unused
        // slots) stay disabled so `overwrite_last_window_right_border`
        // and `TtyRif::rasterize` know not to touch them. Matches
        // GNU's `MATRIX_ROW_ENABLED_P` discipline where disabled
        // rows are inert until the walker marks them valid.
        let rows = (0..nrows)
            .map(|_| {
                let mut row = GlyphRow::new(GlyphRowRole::Text);
                row.enabled = false;
                MatrixRow::new(row)
            })
            .collect();
        Self {
            row_damage: vec![RowDamage::New; nrows],
            rows,
            nrows,
            ncols,
            matrix_x: 0,
            matrix_y: 0,
            header_line: false,
            tab_line: false,
        }
    }

    pub fn clear(&mut self) {
        for row in &mut self.rows {
            MatrixRow::make_mut(row).clear();
        }
    }

    pub fn resize(&mut self, nrows: usize, ncols: usize) {
        self.rows.resize_with(nrows, || {
            let mut row = GlyphRow::new(GlyphRowRole::Text);
            row.enabled = false;
            MatrixRow::new(row)
        });
        self.rows.truncate(nrows);
        self.row_damage.resize(nrows, RowDamage::New);
        self.nrows = nrows;
        self.ncols = ncols;
    }

    /// This frame's provenance for row `idx` (`New` when out of range or
    /// never stamped this frame).
    pub fn row_damage(&self, idx: usize) -> RowDamage {
        self.row_damage.get(idx).copied().unwrap_or(RowDamage::New)
    }

    pub fn set_row_damage(&mut self, idx: usize, damage: RowDamage) {
        if self.row_damage.len() < self.rows.len() {
            self.row_damage.resize(self.rows.len(), RowDamage::New);
        }
        if let Some(slot) = self.row_damage.get_mut(idx) {
            *slot = damage;
        }
    }

    pub fn ensure_hashes(&mut self) {
        for row in &mut self.rows {
            if row.hash == 0 && row.total_glyphs() > 0 {
                let row = MatrixRow::make_mut(row);
                row.hash = row.compute_hash();
            }
        }
    }
}

/// Per-row layout provenance for incremental redisplay (spec §4.6).
/// Stored directly on [`GlyphRow`] so provenance and visual identity cannot
/// drift apart before the render-side damage compositor consumes them.
#[derive(Clone, Copy, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum RowDamage {
    /// Row was laid out from scratch this cycle.
    #[default]
    New,
    /// Row was reused verbatim from the retained matrix at the same `pixel_y`.
    Reused,
    /// Row was reused but shifted by a uniform vertical delta (scroll).
    ReusedShifted { dvpos: Px },
}

impl RowDamage {
    /// Whether this row had to be laid out from scratch this cycle.
    pub fn is_relaid(self) -> bool {
        matches!(self, RowDamage::New)
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WindowMatrixEntry {
    pub window_id: DisplayWindowId,
    pub matrix: GlyphMatrix,
    /// Frame-relative bounds of the whole Emacs window area owned by
    /// this matrix, including margins/fringes and chrome rows.
    pub pixel_bounds: Rect,
    /// Frame-relative bounds of the GNU TEXT_AREA inside this window.
    ///
    /// Buffer text glyphs and the physical cursor are laid out in
    /// text-area-local coordinates; materialization applies this
    /// origin when converting them to frame pixels.  Header/mode-line
    /// rows remain window-wide and continue to use `pixel_bounds`.
    pub text_pixel_bounds: Rect,
    /// Canonical frame-relative clip for body-text primitives.  Production
    /// presentations install this from the sealed window partition instead of
    /// reconstructing it from glyph rows.  `None` is retained only for
    /// standalone protocol fixtures built without a layout transaction.
    #[serde(default)]
    pub text_clip_bounds: Option<Rect>,
    /// True when this window is the frame's selected window at the
    /// time the display state was built. The TTY rasterizer uses
    /// this to decide which window owns the physical terminal
    /// cursor: only the selected window contributes a
    /// `cursor_col` to the terminal cursor position, even though
    /// other windows may still draw a hollow cursor glyph via
    /// `cursor-in-non-selected-windows`. Mirrors GNU
    /// `src/xdisp.c::display_and_set_cursor`, which only resolves
    /// the frame cursor from the selected window's row.
    pub selected: bool,
}

// ---------------------------------------------------------------------------
// Non-grid item structs — these mirror FrameGlyph variants for items that
// don't belong on the character grid (backgrounds, borders, cursors, etc.).
// ---------------------------------------------------------------------------

/// A window background rectangle.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackgroundItem {
    pub bounds: Rect,
    pub color: Color,
}

/// A rectangular fill painted with a realized face.
///
/// This represents redisplay-owned blank cells: areas such as the body text
/// region of a window whose background comes from buffer-local face remapping.
/// It is intentionally face-based instead of color-only so TTY backends can
/// preserve terminal-default foreground/background semantics.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FaceFillItem {
    pub window_id: DisplayWindowId,
    pub row_role: GlyphRowRole,
    pub clip_rect: Option<Rect>,
    pub bounds: Rect,
    pub face_id: FaceId,
}

/// A window border/divider rectangle.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BorderItem {
    pub window_id: DisplayWindowId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

/// Why a cursor entry exists in the retained display state.
///
/// Semantic window carets participate in incremental redisplay and therefore
/// carry the buffer position whose accepted presentation must be replayed.
/// Paint-only cursors must never be mistaken for that authoritative caret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CursorItemRole {
    WindowCaret {
        charpos: usize,
    },
    #[default]
    Decorative,
}

impl CursorItemRole {
    pub const fn window_caret_charpos(self) -> Option<usize> {
        match self {
            Self::WindowCaret { charpos } => Some(charpos),
            Self::Decorative => None,
        }
    }
}

/// A cursor entry.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CursorItem {
    pub window_id: DisplayWindowId,
    /// Typed semantic identity prevents paint-only cursors from entering the
    /// retained window-caret path.
    #[serde(default)]
    pub role: CursorItemRole,
    pub slot_id: DisplaySlotId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub style: CursorStyle,
    pub color: Color,
    /// Foreground used to redraw a glyph covered by a filled box cursor.
    #[serde(default)]
    pub cursor_fg: Color,
    /// Pixels above the baseline. Needed so `cursor_draw_rect` places the cursor
    /// top at `glyph_baseline - ascent`; a non-selected window's cursor is drawn
    /// one row too low when this is dropped to 0.
    #[serde(default)]
    pub ascent: f32,
}

/// A scroll bar.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScrollBarItem {
    pub window_id: DisplayWindowId,
    pub row_role: GlyphRowRole,
    pub clip_rect: Option<Rect>,
    pub horizontal: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub position: i64,
    pub portion: i64,
    pub whole: i64,
    pub thumb_start: f32,
    pub thumb_size: f32,
    pub track_color: Color,
    pub thumb_color: Color,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FrameDisplayState {
    /// Evaluator interaction snapshot paired with these exact pixels.
    pub presentation_id: PresentationId,
    /// Why this presentation was produced.
    ///
    /// The compositor reads this to decide whether layout motion applies: a
    /// presentation committed while an interactive resize is running installs
    /// instantly, because during a drag GNU's Lisp owns the divider position
    /// and the compositor must not animate toward a position window.el may
    /// still clamp or round.
    pub origin: crate::presentation_origin::PresentationOrigin,
    /// Canonical parent-relative placement paired with this presentation.
    #[serde(default)]
    pub frame_placement: crate::PresentedFramePlacement,
    /// Pointer semantics and transient paints paired with this exact snapshot.
    #[serde(default)]
    pub presented_pointer_source: crate::PresentedPointerSourceMap,
    /// Semantic hit regions and exact positions paired with this snapshot.
    #[serde(default)]
    pub presented_hit_index: crate::PresentedHitIndex,
    pub window_matrices: Vec<WindowMatrixEntry>,
    /// Authoritative frame-level chrome bands.
    pub frame_chrome: FrameChrome,
    pub frame_cols: usize,
    pub frame_rows: usize,
    pub frame_pixel_width: f32,
    pub frame_pixel_height: f32,
    pub char_width: f32,
    pub char_height: f32,
    pub font_pixel_size: f32,
    pub background: Color,
    pub faces: HashMap<FaceId, Face>,
    /// Native catalog generation used for all font resolution and geometry in
    /// this immutable presentation.
    #[serde(default)]
    pub font_catalog_generation: crate::font::FontCatalogGeneration,
    /// Resolved font table for this frame. `Face::default_resolved_font_id`
    /// and (eventually) shaped glyph runs reference entries here; the render
    /// thread rasterizes these exact fonts instead of re-selecting by
    /// family/weight/slant.
    pub fonts: crate::font::ResolvedFontTable,
    /// Per-character fallback fonts for chars the face primary font may not
    /// cover (CJK/emoji/symbols): `face_id → representative char → font id`.
    #[serde(default)]
    pub char_fonts: crate::font::CharFontTable,
    /// Shaped composed clusters: `face_id → cluster text → resolved glyphs`.
    #[serde(default)]
    pub shaped_clusters: crate::font::ShapedClusterTable,
    pub undecorated: bool,
    pub border_width: f32,
    pub border_color: Color,
    #[serde(default)]
    pub outer_border_width: f32,
    #[serde(default)]
    pub outer_border_color: Color,
    pub background_alpha: f32,
    pub no_accept_focus: bool,
    pub window_infos: Vec<WindowInfo>,
    pub transition_hints: Vec<ContentTransitionHint>,
    /// Window background rectangles.
    pub backgrounds: Vec<BackgroundItem>,
    /// Face-backed rectangular fills for redisplay-owned blank cells.
    pub face_fills: Vec<FaceFillItem>,
    /// Window border/divider rectangles.
    pub borders: Vec<BorderItem>,
    /// Cursor entries.
    pub cursors: Vec<CursorItem>,
    /// Per-window cursor effect profiles.
    pub cursor_effects_by_window: HashMap<DisplayWindowId, EffectsConfig>,
    /// Scroll bars.
    pub scroll_bars: Vec<ScrollBarItem>,
    /// Authoritative active cursor for the frame.
    pub phys_cursor: Option<PhysCursor>,
    /// Effect hints for the renderer.
    pub effect_hints: Vec<WindowEffectHint>,
    /// Resolved fringe bitmaps for this frame, keyed by registry index. Each
    /// `GlyphRow::left_fringe_bitmap` references one of these by `bitmap_index`.
    pub fringe_bitmaps: HashMap<u16, FringeBitmapData>,
}

#[cfg(debug_assertions)]
thread_local! {
    static MATERIALIZE_CALL_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[cfg(debug_assertions)]
pub fn reset_materialize_call_count_for_current_thread() {
    MATERIALIZE_CALL_COUNT.with(|count| count.set(0));
}

#[cfg(debug_assertions)]
pub fn materialize_call_count_for_current_thread() -> u32 {
    MATERIALIZE_CALL_COUNT.with(std::cell::Cell::get)
}

/// Authoritative paint cell shared by every glyph materialized from one row.
///
/// This mirrors GNU `struct glyph_string`: glyph strings keep their own
/// horizontal advance and font/ink metrics, but their `y` and `height` always
/// come from the containing `glyph_row`. Keeping that invariant in one value
/// prevents glyph-specific layout metrics from leaking into face background,
/// pointer, and decoration geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MaterializedRowCell {
    y: f32,
    height: f32,
}

fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = a.right().min(b.right()).max(left);
    let bottom = a.bottom().min(b.bottom()).max(top);
    Rect::new(left, top, right - left, bottom - top)
}

impl MaterializedRowCell {
    fn new(win_y: f32, row_index: u32, row: &GlyphRow, fallback_height: f32) -> Self {
        let y = if row.height_px > 0.0 {
            win_y + row.pixel_y
        } else {
            win_y + row_index as f32 * fallback_height
        };
        let height = if row.height_px > 0.0 {
            row.height_px
        } else {
            fallback_height
        };
        Self { y, height }
    }

    fn rect(self, x: f32, width: f32) -> Rect {
        Rect::new(x, self.y, width, self.height)
    }
}

impl FrameDisplayState {
    /// Image identities whose textures must remain drawable for this immutable
    /// presentation.
    ///
    /// This walks the canonical row model directly. In particular, callers do
    /// not need to materialize a deferred presentation merely to establish its
    /// renderer residency fence.
    #[must_use]
    pub fn referenced_images(&self) -> RetainedImageSet {
        fn collect_row(row: &GlyphRow, retained: &mut RetainedImageSet) {
            retained.extend(GlyphArea::ALL.into_iter().flat_map(|area| {
                row.glyphs[area.index()].iter().filter_map(|glyph| {
                    let GlyphType::Image { image_id, .. } = glyph.glyph_type else {
                        return None;
                    };
                    u32::try_from(image_id).ok().map(ImageId::new)
                })
            }));
        }

        let mut retained = RetainedImageSet::default();
        for band in self.frame_chrome.bands() {
            if let FrameChromeContent::DisplayRow(content) = band.content() {
                collect_row(content.row(), &mut retained);
            }
        }
        for entry in &self.window_matrices {
            for row in &entry.matrix.rows {
                collect_row(row, &mut retained);
            }
        }
        retained
    }

    /// Resolve GNU's three glyph-row areas from the immutable geometry sealed
    /// for this presentation.
    ///
    /// Window rows store glyphs by semantic area, while consumers need backend-
    /// specific pixel/cell coordinates. This is the single boundary that joins
    /// those two models. A configured margin receives its own structural origin;
    /// window-wide chrome reserves only GNU's final LAST_AREA cell.
    pub fn glyph_row_area_layout(
        &self,
        entry: &WindowMatrixEntry,
        role: GlyphRowRole,
    ) -> GlyphRowAreaLayout {
        let row_bounds = entry.row_pixel_bounds(role);
        let row_clip = if role == GlyphRowRole::Text {
            entry.text_area_clip_rect()
        } else {
            row_bounds
        };
        let char_width = if entry.matrix.ncols > 0 {
            row_bounds.width / entry.matrix.ncols as f32
        } else {
            self.char_width
        };
        if role != GlyphRowRole::Text {
            return GlyphRowAreaLayout::window_wide(row_bounds, row_clip, char_width);
        }

        let regions = self.window_infos.iter().find_map(|info| {
            if info.window_id != entry.window_id {
                return None;
            }
            match info.geometry {
                PresentedWindowGeometry::Complete { regions, .. } => Some(regions),
                PresentedWindowGeometry::Skipped { .. } => None,
            }
        });
        let Some(regions) = regions else {
            return GlyphRowAreaLayout::window_text(row_bounds, row_clip, None, None, char_width);
        };

        GlyphRowAreaLayout::window_text(
            row_bounds,
            row_clip,
            regions.left_margin,
            regions.right_margin,
            char_width,
        )
    }

    /// Verify that window metadata and semantic hit regions are projections of
    /// the same immutable presentation geometry.
    ///
    /// The layout producer calls this while sealing a presentation. It lives
    /// at the protocol boundary so another producer cannot publish internally
    /// coherent, but mutually divergent, spatial products.
    pub fn validate_spatial_projections(&self) -> Result<(), crate::PresentedHitError> {
        use crate::frame_glyphs::PresentedWindowGeometry;
        use crate::{PresentedHitError, PresentedRegionKind};

        if self.presented_hit_index.presentation() != self.presentation_id {
            return Err(PresentedHitError::StalePresentation {
                expected: self.presentation_id,
                requested: self.presented_hit_index.presentation(),
            });
        }

        let expected_bounds = |window: DisplayWindowId,
                               kind: PresentedRegionKind|
         -> Result<Option<crate::FrameRect>, PresentedHitError> {
            let Some(info) = self
                .window_infos
                .iter()
                .find(|info| info.window_id == window)
            else {
                return Ok(None);
            };
            let PresentedWindowGeometry::Complete { regions, .. } = info.geometry else {
                return Ok(None);
            };
            let rect = match kind {
                PresentedRegionKind::TextBody => Some(regions.text_body),
                PresentedRegionKind::LeftMargin => regions.left_margin,
                PresentedRegionKind::RightMargin => regions.right_margin,
                PresentedRegionKind::LeftFringe => regions.left_fringe,
                PresentedRegionKind::RightFringe => regions.right_fringe,
                PresentedRegionKind::LeftScrollBar => regions.left_scroll_bar,
                PresentedRegionKind::RightScrollBar => regions.right_scroll_bar,
                PresentedRegionKind::HorizontalScrollBar => regions.horizontal_scroll_bar,
                PresentedRegionKind::TabLine => regions.tab_line,
                PresentedRegionKind::HeaderLine => regions.header_line,
                PresentedRegionKind::ModeLine => regions.mode_line,
                PresentedRegionKind::RightDivider => regions.right_divider,
                PresentedRegionKind::BottomDivider => regions.bottom_divider,
                PresentedRegionKind::MenuBar
                | PresentedRegionKind::ToolBar
                | PresentedRegionKind::CompactBar
                | PresentedRegionKind::TabBar => None,
            };
            let Some(rect) = rect else {
                return Ok(None);
            };
            if rect.width == 0.0 || rect.height == 0.0 {
                return Ok(None);
            }
            crate::FrameRect::new(rect.x, rect.y, rect.width, rect.height)
                .map(Some)
                .map_err(|_| PresentedHitError::InvalidRegionGeometry)
        };

        for info in &self.window_infos {
            let PresentedWindowGeometry::Complete { regions, .. } = info.geometry else {
                continue;
            };
            let valid_rect = |rect: crate::types::Rect| {
                crate::FrameRect::new(rect.x, rect.y, rect.width, rect.height)
                    .map(|_| ())
                    .map_err(|_| PresentedHitError::InvalidRegionGeometry)
            };
            valid_rect(regions.outer)?;
            let is_contained = |rect: crate::types::Rect| {
                const EDGE_EPSILON: f32 = 0.01;
                rect.x + EDGE_EPSILON >= regions.outer.x
                    && rect.y + EDGE_EPSILON >= regions.outer.y
                    && rect.x + rect.width <= regions.outer.x + regions.outer.width + EDGE_EPSILON
                    && rect.y + rect.height <= regions.outer.y + regions.outer.height + EDGE_EPSILON
            };
            for (kind, rect) in [
                (PresentedRegionKind::TextBody, Some(regions.text_body)),
                (PresentedRegionKind::LeftMargin, regions.left_margin),
                (PresentedRegionKind::RightMargin, regions.right_margin),
                (PresentedRegionKind::LeftFringe, regions.left_fringe),
                (PresentedRegionKind::RightFringe, regions.right_fringe),
                (PresentedRegionKind::LeftScrollBar, regions.left_scroll_bar),
                (
                    PresentedRegionKind::RightScrollBar,
                    regions.right_scroll_bar,
                ),
                (
                    PresentedRegionKind::HorizontalScrollBar,
                    regions.horizontal_scroll_bar,
                ),
                (PresentedRegionKind::TabLine, regions.tab_line),
                (PresentedRegionKind::HeaderLine, regions.header_line),
                (PresentedRegionKind::ModeLine, regions.mode_line),
                (PresentedRegionKind::RightDivider, regions.right_divider),
                (PresentedRegionKind::BottomDivider, regions.bottom_divider),
            ] {
                let Some(rect) = rect else {
                    continue;
                };
                valid_rect(rect)?;
                if !is_contained(rect) {
                    return Err(PresentedHitError::WindowGeometryMismatch {
                        window: info.window_id,
                        region: kind,
                    });
                }
            }

            // A complete window geometry is a partition, not merely a set of
            // rectangles that happen to fit inside `outer`. Enforce the same
            // top-to-bottom ordering used by GNU redisplay so an internally
            // consistent producer bug cannot seal overlapping chrome/body or
            // divider geometry.
            let mut previous_bottom = regions.outer.y;
            for (kind, rect) in [
                (PresentedRegionKind::TabLine, regions.tab_line),
                (PresentedRegionKind::HeaderLine, regions.header_line),
                (PresentedRegionKind::TextBody, Some(regions.text_body)),
                (
                    PresentedRegionKind::HorizontalScrollBar,
                    regions.horizontal_scroll_bar,
                ),
                (PresentedRegionKind::ModeLine, regions.mode_line),
                (PresentedRegionKind::BottomDivider, regions.bottom_divider),
            ] {
                let Some(rect) = rect else {
                    continue;
                };
                if rect.y < previous_bottom {
                    return Err(PresentedHitError::WindowGeometryMismatch {
                        window: info.window_id,
                        region: kind,
                    });
                }
                previous_bottom = rect.y + rect.height;
            }

            let overlaps = |left: crate::types::Rect, right: crate::types::Rect| {
                left.x < right.x + right.width
                    && right.x < left.x + left.width
                    && left.y < right.y + right.height
                    && right.y < left.y + left.height
            };
            if let Some(right_divider) = regions.right_divider {
                for (kind, rect) in [
                    (PresentedRegionKind::TabLine, regions.tab_line),
                    (PresentedRegionKind::HeaderLine, regions.header_line),
                    (PresentedRegionKind::TextBody, Some(regions.text_body)),
                    (
                        PresentedRegionKind::HorizontalScrollBar,
                        regions.horizontal_scroll_bar,
                    ),
                    (PresentedRegionKind::ModeLine, regions.mode_line),
                    (PresentedRegionKind::BottomDivider, regions.bottom_divider),
                ] {
                    if rect.is_some_and(|rect| overlaps(rect, right_divider)) {
                        return Err(PresentedHitError::WindowGeometryMismatch {
                            window: info.window_id,
                            region: kind,
                        });
                    }
                }
            }
            let mut horizontal_bands = vec![(PresentedRegionKind::TextBody, regions.text_body)];
            horizontal_bands.extend(
                [
                    (PresentedRegionKind::LeftScrollBar, regions.left_scroll_bar),
                    (PresentedRegionKind::LeftMargin, regions.left_margin),
                    (PresentedRegionKind::LeftFringe, regions.left_fringe),
                    (PresentedRegionKind::RightFringe, regions.right_fringe),
                    (PresentedRegionKind::RightMargin, regions.right_margin),
                    (
                        PresentedRegionKind::RightScrollBar,
                        regions.right_scroll_bar,
                    ),
                    (PresentedRegionKind::RightDivider, regions.right_divider),
                ]
                .into_iter()
                .filter_map(|(kind, rect)| rect.map(|rect| (kind, rect))),
            );
            horizontal_bands.sort_by(|(_, left), (_, right)| left.x.total_cmp(&right.x));
            for pair in horizontal_bands.windows(2) {
                let [(_, left), (right_kind, right)] = pair else {
                    unreachable!("windows(2) always yields two entries");
                };
                if overlaps(*left, *right) {
                    return Err(PresentedHitError::WindowGeometryMismatch {
                        window: info.window_id,
                        region: *right_kind,
                    });
                }
            }
            if let Some(matrix) = self
                .window_matrices
                .iter()
                .find(|matrix| matrix.window_id == info.window_id)
                && (matrix.pixel_bounds != regions.outer
                    || matrix.text_area_clip_rect() != regions.text_body)
            {
                return Err(PresentedHitError::WindowGeometryMismatch {
                    window: info.window_id,
                    region: PresentedRegionKind::TextBody,
                });
            }
            for kind in [
                PresentedRegionKind::TextBody,
                PresentedRegionKind::LeftMargin,
                PresentedRegionKind::RightMargin,
                PresentedRegionKind::LeftFringe,
                PresentedRegionKind::RightFringe,
                PresentedRegionKind::LeftScrollBar,
                PresentedRegionKind::RightScrollBar,
                PresentedRegionKind::HorizontalScrollBar,
                PresentedRegionKind::TabLine,
                PresentedRegionKind::HeaderLine,
                PresentedRegionKind::ModeLine,
                PresentedRegionKind::RightDivider,
                PresentedRegionKind::BottomDivider,
            ] {
                let Some(expected) = expected_bounds(info.window_id, kind)? else {
                    continue;
                };
                let mut matching = self.presented_hit_index.regions().iter().filter(|region| {
                    region.window() == Some(info.window_id) && region.kind() == kind
                });
                if matching.next().map(|region| region.bounds()) != Some(expected)
                    || matching.next().is_some()
                {
                    return Err(PresentedHitError::WindowGeometryMismatch {
                        window: info.window_id,
                        region: kind,
                    });
                }
            }
        }

        for region in self
            .presented_hit_index
            .regions()
            .iter()
            .filter(|region| region.window().is_some())
        {
            let window = region.window().expect("filtered window region");
            if expected_bounds(window, region.kind())? != Some(region.bounds()) {
                return Err(PresentedHitError::WindowGeometryMismatch {
                    window,
                    region: region.kind(),
                });
            }
        }

        for (index, handle) in self
            .presented_hit_index
            .resize_handles()
            .iter()
            .copied()
            .enumerate()
        {
            let kind = handle.axis().region_kind();
            let mismatch = || PresentedHitError::WindowGeometryMismatch {
                window: handle.window(),
                region: kind,
            };
            let Some(info) = self
                .window_infos
                .iter()
                .find(|info| info.window_id == handle.window())
            else {
                return Err(mismatch());
            };
            let PresentedWindowGeometry::Complete { regions, .. } = info.geometry else {
                return Err(mismatch());
            };
            const EDGE_EPSILON: f32 = 0.01;
            let bounds = handle.bounds();
            let outer = regions.outer;
            let contained = bounds.x() + EDGE_EPSILON >= outer.x
                && bounds.y() + EDGE_EPSILON >= outer.y
                && bounds.x() + bounds.width() <= outer.right() + EDGE_EPSILON
                && bounds.y() + bounds.height() <= outer.bottom() + EDGE_EPSILON;
            let attached_to_resized_edge = match (handle.axis(), handle.edge()) {
                (crate::PresentedResizeAxis::Horizontal, crate::PresentedResizeEdge::Leading) => {
                    (bounds.x() - outer.x).abs() <= EDGE_EPSILON
                }
                (crate::PresentedResizeAxis::Horizontal, crate::PresentedResizeEdge::Trailing) => {
                    (bounds.x() + bounds.width() - outer.right()).abs() <= EDGE_EPSILON
                }
                (crate::PresentedResizeAxis::Vertical, crate::PresentedResizeEdge::Leading) => {
                    (bounds.y() - outer.y).abs() <= EDGE_EPSILON
                }
                (crate::PresentedResizeAxis::Vertical, crate::PresentedResizeEdge::Trailing) => {
                    (bounds.y() + bounds.height() - outer.bottom()).abs() <= EDGE_EPSILON
                }
            };
            let non_overlapping = !self
                .presented_hit_index
                .resize_handles()
                .iter()
                .take(index)
                .any(|previous| {
                    previous.window() == handle.window()
                        && previous.axis() == handle.axis()
                        && previous.edge() == handle.edge()
                        && previous.bounds().x() < bounds.x() + bounds.width()
                        && bounds.x() < previous.bounds().x() + previous.bounds().width()
                        && previous.bounds().y() < bounds.y() + bounds.height()
                        && bounds.y() < previous.bounds().y() + previous.bounds().height()
                });
            if !contained || !attached_to_resized_edge || !non_overlapping {
                return Err(mismatch());
            }
        }

        Ok(())
    }

    pub fn new(frame_cols: usize, frame_rows: usize, char_width: f32, char_height: f32) -> Self {
        Self {
            presentation_id: PresentationId::default(),
            origin: crate::presentation_origin::PresentationOrigin::Ordinary,
            frame_placement: crate::PresentedFramePlacement::default(),
            presented_pointer_source: crate::PresentedPointerSourceMap::empty(),
            presented_hit_index: crate::PresentedHitIndex::default(),
            window_matrices: Vec::new(),
            frame_chrome: FrameChrome::default(),
            frame_cols,
            frame_rows,
            frame_pixel_width: frame_cols as f32 * char_width,
            frame_pixel_height: frame_rows as f32 * char_height,
            char_width,
            char_height,
            font_pixel_size: char_height,
            background: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            faces: HashMap::new(),
            font_catalog_generation: crate::font::FontCatalogGeneration::default(),
            fonts: crate::font::ResolvedFontTable::new(),
            char_fonts: crate::font::CharFontTable::new(),
            shaped_clusters: crate::font::ShapedClusterTable::new(),
            undecorated: false,
            border_width: 0.0,
            border_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            outer_border_width: 0.0,
            outer_border_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            background_alpha: 1.0,
            no_accept_focus: false,
            window_infos: Vec::new(),
            transition_hints: Vec::new(),
            backgrounds: Vec::new(),
            face_fills: Vec::new(),
            borders: Vec::new(),
            cursors: Vec::new(),
            cursor_effects_by_window: HashMap::new(),
            scroll_bars: Vec::new(),
            phys_cursor: None,
            effect_hints: Vec::new(),
            fringe_bitmaps: HashMap::new(),
        }
    }

    /// Return one window's exact accepted cursor presentation, independent of
    /// whether that window is active or inactive.
    ///
    /// Active/inactive is a transport choice in this state (`phys_cursor` vs
    /// `cursors`), not a placement choice. Incremental redisplay consumes this
    /// unified view so both roles retain identical semantic and pixel geometry.
    pub fn presented_cursor_for_window(&self, window_id: DisplayWindowId) -> Option<PhysCursor> {
        if let Some(cursor) = self
            .phys_cursor
            .as_ref()
            .filter(|cursor| cursor.window_id == window_id)
        {
            return Some(cursor.clone());
        }
        self.cursors.iter().find_map(|cursor| match cursor.role {
            CursorItemRole::WindowCaret { charpos } if cursor.window_id == window_id => {
                Some(PhysCursor {
                    window_id: cursor.window_id,
                    charpos,
                    row: cursor.slot_id.row as usize,
                    col: cursor.slot_id.col,
                    slot_id: cursor.slot_id,
                    x: cursor.x,
                    y: cursor.y,
                    width: cursor.width,
                    height: cursor.height,
                    ascent: cursor.ascent,
                    style: cursor.style,
                    color: cursor.color,
                    cursor_fg: cursor.cursor_fg,
                })
            }
            CursorItemRole::WindowCaret { .. } | CursorItemRole::Decorative => None,
        })
    }

    /// Create a `FrameDisplayState` from an existing `FrameGlyphBuffer`.
    ///
    /// Copies transport metadata and the remaining non-row primitives
    /// (backgrounds, borders, cursors, and scroll bars). Row-owned glyphs are
    /// intentionally not reconstructed from their lossy flat projection.
    pub fn from_frame_glyph_buffer(buf: &FrameGlyphBuffer) -> Self {
        let frame_cols = (buf.width / buf.char_width.max(1.0)) as usize;
        let frame_rows = (buf.height / buf.char_height.max(1.0)) as usize;
        let mut state = Self::new(frame_cols, frame_rows, buf.char_width, buf.char_height);
        state.presentation_id = buf.presentation_id;
        state.frame_placement = buf.frame_placement;
        state.presented_hit_index = buf.presented_hit_index().clone();
        state.frame_pixel_width = buf.width;
        state.frame_pixel_height = buf.height;
        state.font_pixel_size = buf.font_pixel_size;
        state.background = buf.background;
        state.undecorated = buf.undecorated;
        state.border_width = buf.border_width;
        state.border_color = buf.border_color;
        state.outer_border_width = buf.outer_border_width;
        state.outer_border_color = buf.outer_border_color;
        state.background_alpha = buf.background_alpha;
        state.no_accept_focus = buf.no_accept_focus;
        state.faces = buf.faces.clone();
        state.font_catalog_generation = buf.font_catalog_generation;
        state.fonts = buf.fonts.clone();
        state.char_fonts = buf.char_fonts.clone();
        state.shaped_clusters = buf.shaped_clusters.clone();
        state.window_infos = buf.window_infos.clone();
        state.frame_chrome = buf.frame_chrome.clone();
        // Reconstruct the layout-internal phys_cursor from the unified list's
        // active entry; charpos isn't carried on WindowCursor so default to 0.
        state.phys_cursor = buf.active_cursor().map(|c| PhysCursor {
            window_id: c.window_id,
            charpos: 0,
            row: c.slot_id.row as usize,
            col: c.slot_id.col,
            slot_id: c.slot_id,
            x: c.x,
            y: c.y,
            width: c.width,
            height: c.height,
            ascent: c.ascent,
            style: c.style,
            color: c.color,
            cursor_fg: c.cursor_fg,
        });
        state.cursor_effects_by_window = buf.cursor_effects_by_window.clone();
        state.fringe_bitmaps = buf.fringe_bitmaps.clone();
        state.transition_hints = buf.transition_hints.clone();
        state.effect_hints = buf.effect_hints.clone();
        // Render buffers do not retain buffer-position semantics. Non-active
        // entries therefore round-trip as paint-only cursors; the active entry
        // is reconstructed into `phys_cursor` above.
        state.cursors.extend(
            buf.window_cursors
                .iter()
                .filter(|cursor| !cursor.active)
                .map(|cursor| CursorItem {
                    window_id: cursor.window_id,
                    role: CursorItemRole::Decorative,
                    slot_id: cursor.slot_id,
                    x: cursor.x,
                    y: cursor.y,
                    width: cursor.width,
                    height: cursor.height,
                    style: cursor.style,
                    color: cursor.color,
                    cursor_fg: cursor.cursor_fg,
                    ascent: cursor.ascent,
                }),
        );

        // Decompose only primitives that are not owned by a glyph row.
        for glyph in &buf.glyphs {
            match glyph {
                FrameGlyph::Background { bounds, color } => {
                    state.backgrounds.push(BackgroundItem {
                        bounds: *bounds,
                        color: *color,
                    });
                }
                FrameGlyph::Border {
                    window_id,
                    x,
                    y,
                    width,
                    height,
                    color,
                    ..
                } => {
                    state.borders.push(BorderItem {
                        window_id: *window_id,
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                        color: *color,
                    });
                }
                FrameGlyph::Image { .. }
                | FrameGlyph::Video { .. }
                | FrameGlyph::Xwidget { .. }
                | FrameGlyph::Surface { .. } => {}
                FrameGlyph::ScrollBar {
                    window_id,
                    row_role,
                    clip_rect,
                    horizontal,
                    x,
                    y,
                    width,
                    height,
                    position,
                    portion,
                    whole,
                    thumb_start,
                    thumb_size,
                    track_color,
                    thumb_color,
                } => {
                    state.scroll_bars.push(ScrollBarItem {
                        window_id: *window_id,
                        row_role: *row_role,
                        clip_rect: *clip_rect,
                        horizontal: *horizontal,
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                        position: *position,
                        portion: *portion,
                        whole: *whole,
                        thumb_start: *thumb_start,
                        thumb_size: *thumb_size,
                        track_color: *track_color,
                        thumb_color: *thumb_color,
                    });
                }
                // Char, Stretch, Terminal — grid content, not decomposed here
                _ => {}
            }
        }

        state
    }

    /// Convert this `FrameDisplayState` into a `FrameGlyphBuffer`.
    ///
    /// Materializes the `GlyphMatrix` grid into pixel-positioned
    /// `FrameGlyph` entries and appends all non-grid items (backgrounds,
    /// borders, cursors, etc.).
    pub fn materialize(&self) -> FrameGlyphBuffer {
        #[cfg(debug_assertions)]
        MATERIALIZE_CALL_COUNT.with(|count| count.set(count.get() + 1));
        let mut buf = FrameGlyphBuffer::with_size(self.frame_pixel_width, self.frame_pixel_height);
        buf.presentation_id = self.presentation_id;
        buf.frame_placement = self.frame_placement;
        buf.char_width = self.char_width;
        buf.char_height = self.char_height;
        buf.font_pixel_size = self.font_pixel_size;
        buf.background = self.background;
        buf.undecorated = self.undecorated;
        buf.border_width = self.border_width;
        buf.border_color = self.border_color;
        buf.outer_border_width = self.outer_border_width;
        buf.outer_border_color = self.outer_border_color;
        buf.background_alpha = self.background_alpha;
        buf.no_accept_focus = self.no_accept_focus;

        // Copy faces
        for (id, face) in &self.faces {
            buf.faces.insert(*id, face.clone());
        }

        buf.font_catalog_generation = self.font_catalog_generation;
        // Copy resolved fonts
        for (id, font) in &self.fonts {
            buf.fonts.insert(*id, font.clone());
        }
        buf.char_fonts = self.char_fonts.clone();
        buf.shaped_clusters = self.shaped_clusters.clone();

        // Copy window_infos
        for info in &self.window_infos {
            buf.window_infos.push(info.clone());
        }

        // Copy fringe bitmaps (the bits referenced by each row's fringe info).
        buf.fringe_bitmaps = self.fringe_bitmaps.clone();

        // --- Grid conversion ---

        // Copy effect hints
        buf.effect_hints = self.effect_hints.clone();

        // Copy transition hints
        buf.transition_hints = self.transition_hints.clone();
        buf.frame_chrome = self.frame_chrome.clone();

        // --- Materialize all glyphs in canonical row/overlay order ---
        self.for_each_glyph(|g| buf.glyphs.push(g));

        // --- Materialize cursors ---
        // These are non-active cursor presentations. Their typed role remains
        // available to layout retention, while the selected window's active
        // cursor is pushed by set_phys_cursor below. These write to
        // `buf.window_cursors`, not `buf.glyphs`, so the glyph order produced
        // above is preserved.
        for cursor in &self.cursors {
            buf.window_cursors.push(WindowCursor {
                window_id: cursor.window_id,
                slot_id: cursor.slot_id,
                x: cursor.x,
                y: cursor.y,
                width: cursor.width,
                height: cursor.height,
                style: cursor.style,
                color: cursor.color,
                cursor_fg: cursor.cursor_fg,
                // Carry the real ascent so `cursor_draw_rect` places the cursor
                // top at `baseline - ascent`; a hardcoded 0 dropped a
                // non-selected window's cursor one text row too low.
                ascent: cursor.ascent,
                active: false,
            });
        }
        buf.cursor_effects_by_window = self.cursor_effects_by_window.clone();

        if let Some(cursor) = self.phys_cursor.clone() {
            buf.set_phys_cursor(cursor);
        }

        if !self.presented_pointer_source.is_empty() {
            buf.install_presented_pointer_source_map(&self.presented_pointer_source)
                .expect("FrameDisplayState pointer map must match its materialized primitives");
        }
        if !self.presented_hit_index.is_empty() {
            self.validate_spatial_projections()
                .expect("FrameDisplayState spatial projections must share canonical geometry");
        }
        let hit_index = if self.presented_hit_index.is_empty()
            && self.presented_hit_index.presentation() != self.presentation_id
        {
            crate::PresentedHitIndex::empty(self.presentation_id)
        } else {
            self.presented_hit_index.clone()
        };
        buf.install_presented_hit_index(hit_index)
            .expect("FrameDisplayState hit index must match its presentation");

        buf
    }

    /// Visit every `FrameGlyph` this state materializes, in the canonical
    /// `materialize()` order, calling `push` for each.
    ///
    /// This is the glyph-production half of [`Self::materialize`], factored out
    /// so callers can iterate the matrix directly without building the flat
    /// `Vec<FrameGlyph>`. It emits, in order: backgrounds, frame-chrome grid
    /// rows, window-matrix grid rows, borders, and scroll bars. Images, videos,
    /// and xwidgets are emitted by their owning rows. It does NOT emit cursors
    /// or write any `FrameGlyphBuffer` metadata.
    pub fn for_each_glyph(&self, mut push: impl FnMut(FrameGlyph)) {
        // --- Materialize backgrounds ---
        for bg in &self.backgrounds {
            push(FrameGlyph::Background {
                bounds: bg.bounds,
                color: bg.color,
            });
        }
        for fill in &self.face_fills {
            let face_data = self.resolve_face_for_materialize(fill.face_id);
            // A face fill is non-grid window background, not a displayed
            // stretch glyph.  Giving it a pixel-derived DisplaySlotId places
            // frame-space geometry in the same identity namespace as the
            // window-local glyph matrix and can collide with a real glyph
            // (for example a completion candidate at row 7, column 0).
            // TTY materialization still consumes FaceFillItem directly and
            // therefore retains its face-aware terminal semantics.
            push(FrameGlyph::Background {
                bounds: fill.bounds,
                color: face_data.bg,
            });
        }

        // --- Materialize grid content -> pixel-positioned Char/Stretch glyphs ---
        for band in self.frame_chrome.bands() {
            let FrameChromeContent::DisplayRow(content) = band.content() else {
                continue;
            };
            let bounds = band.bounds().raw();
            let row_index = band.canonical_row(self.char_height);
            self.for_each_grid_row_glyph(
                DisplayWindowId::new(0),
                row_index,
                content.row(),
                bounds,
                GlyphRowAreaLayout::unpartitioned(bounds, bounds),
                self.char_width,
                self.char_height,
                &mut push,
            );
        }
        for entry in &self.window_matrices {
            // Body (`Text`) rows clip to the text-area band so a vscroll's
            // top-clipped first row / exposed bottom row do not bleed over the
            // header/tab-line or mode-line; chrome rows keep the window bounds.
            for (row_idx, glyph_row) in entry.matrix.rows.iter().enumerate() {
                let row_bounds = entry.row_pixel_bounds(glyph_row.role);
                let area_layout = self.glyph_row_area_layout(entry, glyph_row.role);
                let char_w = if entry.matrix.ncols > 0 {
                    row_bounds.width / entry.matrix.ncols as f32
                } else {
                    self.char_width
                };
                self.for_each_grid_row_glyph(
                    entry.window_id,
                    row_idx as u32,
                    glyph_row,
                    row_bounds,
                    area_layout,
                    char_w,
                    self.char_height,
                    &mut push,
                );
            }
        }

        // --- Materialize left/right fringe bitmaps ---
        //
        // Buffer-text rows carry the fringe bitmaps: truncation / continuation
        // arrows (GNU draw_window_fringes), the empty-line `~`, and magit
        // section-heading fold arrows. Both fringes are emitted; each column is
        // clamped to the scroll-bar edge below so a right-side bar can't hide
        // the right-fringe arrows.
        for entry in &self.window_matrices {
            let window_id = entry.window_id;
            let text_area_clip = entry.text_area_clip_rect();

            // Horizontal fringe geometry is constant per window, so resolve it
            // once. GNU (`fringe.c` draw_fringe_bitmap_1) positions fringe
            // bitmaps against `window_box_left/right(TEXT_AREA)` and keeps the
            // vertical scroll bar OUTSIDE the fringe — the fringe is exactly the
            // gap between the text area and the bar (WINDOW_*_FRINGE_WIDTH).
            // If the column instead ran to the window edge it swallowed the
            // scroll-bar column, and centering the arrow inside that wide span
            // dropped it behind the (opaque, later-drawn) bar — so a right-side
            // scroll bar hid EVERY truncation / continuation arrow (the left
            // fringe rendered only because it had no bar over it). Clamp each
            // fringe column to the scroll-bar edge so the arrow stays in the
            // real fringe, adjacent to the text.
            let window_left = entry.pixel_bounds.x;
            let window_right = entry.pixel_bounds.x + entry.pixel_bounds.width;
            let text_left = entry.text_pixel_bounds.x;
            let text_right = entry.text_pixel_bounds.x + entry.text_pixel_bounds.width;
            let mut left_fringe_start = window_left;
            let mut right_fringe_end = window_right;
            for sb in &self.scroll_bars {
                if sb.window_id != window_id || sb.horizontal {
                    continue;
                }
                // A bar left of the text bounds the left fringe on its left; a
                // bar right of the text bounds the right fringe on its right.
                if sb.x + sb.width <= text_left {
                    left_fringe_start = left_fringe_start.max(sb.x + sb.width);
                }
                if sb.x >= text_right {
                    right_fringe_end = right_fringe_end.min(sb.x);
                }
            }
            let left_fringe_x = left_fringe_start;
            let left_fringe_width = (text_left - left_fringe_start).max(0.0);
            let right_fringe_x = text_right;
            let right_fringe_width = (right_fringe_end - text_right).max(0.0);

            for (row_idx, glyph_row) in entry.matrix.rows.iter().enumerate() {
                if !glyph_row.enabled {
                    continue;
                }
                if glyph_row.left_fringe_bitmap.is_none()
                    && glyph_row.right_fringe_bitmap.is_none()
                    && glyph_row.overlay_arrow_bitmap.is_none()
                {
                    continue;
                }
                let row_bounds = entry.row_pixel_bounds(glyph_row.role);
                let y = if glyph_row.height_px > 0.0 {
                    row_bounds.y + glyph_row.pixel_y
                } else {
                    row_bounds.y + row_idx as f32 * self.char_height
                };
                let height = if glyph_row.height_px > 0.0 {
                    glyph_row.height_px
                } else {
                    self.char_height
                };
                // Empty-line / truncation fringe bitmaps ride buffer-text rows,
                // so a vscroll clips them to the same VERTICAL band as the body
                // glyphs — but the fringe lives in the fringe column, so keep the
                // full window HORIZONTAL extent. With no chrome rows this
                // reproduces the historical `Some(pixel_bounds)`.
                let clip_rect = if glyph_row.role == GlyphRowRole::Text {
                    Some(Rect::new(
                        entry.pixel_bounds.x,
                        text_area_clip.y,
                        entry.pixel_bounds.width,
                        text_area_clip.height,
                    ))
                } else {
                    Some(entry.pixel_bounds)
                };

                if let Some(info) = glyph_row.left_fringe_bitmap {
                    push(FrameGlyph::FringeBitmap {
                        window_id,
                        row_role: glyph_row.role,
                        clip_rect,
                        x: left_fringe_x,
                        y,
                        width: left_fringe_width,
                        height,
                        bitmap_index: info.bitmap_index,
                        face_id: info.face_id,
                        side: FringeSide::Left,
                    });
                }
                // GNU draws the overlay arrow over the left fringe after the
                // row's own left bitmap, never instead of it (`src/fringe.c`
                // `draw_fringe_bitmap`).
                if let Some(info) = glyph_row.overlay_arrow_bitmap {
                    push(FrameGlyph::FringeBitmap {
                        window_id,
                        row_role: glyph_row.role,
                        clip_rect,
                        x: left_fringe_x,
                        y,
                        width: left_fringe_width,
                        height,
                        bitmap_index: info.bitmap_index,
                        face_id: info.face_id,
                        side: FringeSide::Left,
                    });
                }
                if let Some(info) = glyph_row.right_fringe_bitmap {
                    push(FrameGlyph::FringeBitmap {
                        window_id,
                        row_role: glyph_row.role,
                        clip_rect,
                        x: right_fringe_x,
                        y,
                        width: right_fringe_width,
                        height,
                        bitmap_index: info.bitmap_index,
                        face_id: info.face_id,
                        side: FringeSide::Right,
                    });
                }
            }
        }

        // --- Materialize borders ---
        for border in &self.borders {
            push(FrameGlyph::Border {
                window_id: border.window_id,
                row_role: GlyphRowRole::Text,
                clip_rect: None,
                x: border.x,
                y: border.y,
                width: border.width,
                height: border.height,
                color: border.color,
            });
        }

        // --- Materialize scroll bars ---
        for sb in &self.scroll_bars {
            push(FrameGlyph::ScrollBar {
                window_id: sb.window_id,
                row_role: sb.row_role,
                clip_rect: sb.clip_rect,
                horizontal: sb.horizontal,
                x: sb.x,
                y: sb.y,
                width: sb.width,
                height: sb.height,
                position: sb.position,
                portion: sb.portion,
                whole: sb.whole,
                thumb_start: sb.thumb_start,
                thumb_size: sb.thumb_size,
                track_color: sb.track_color,
                thumb_color: sb.thumb_color,
            });
        }
    }

    /// Resolve face attributes for grid materialization.
    ///
    /// Returns a helper struct with the resolved colors, font properties, and
    /// decoration flags needed by `FrameGlyph::Char` and `FrameGlyph::Stretch`.
    fn resolve_face_for_materialize(&self, face_id: FaceId) -> MaterializedFaceData {
        if let Some(face) = self.faces.get(&face_id) {
            MaterializedFaceData {
                fg: face.foreground,
                bg: face.background,
                font_ascent: face.font_ascent.max(0) as f32,
                font_weight: face.font_weight,
                italic: face.attributes.contains(FaceAttributes::ITALIC),
                font_size: face.font_size,
                underline: face.underline_style,
                underline_color: face.underline_color,
                strike_through: face.attributes.contains(FaceAttributes::STRIKE_THROUGH),
                strike_through_color: face.strike_through_color,
                overline: face.attributes.contains(FaceAttributes::OVERLINE),
                overline_color: face.overline_color,
                overstrike: false,
            }
        } else {
            MaterializedFaceData {
                fg: Color::new(1.0, 1.0, 1.0, 1.0),
                bg: self.background,
                font_ascent: 0.0,
                font_weight: 400,
                italic: false,
                font_size: self.font_pixel_size,
                underline: UnderlineStyle::None,
                underline_color: None,
                strike_through: false,
                strike_through_color: None,
                overline: false,
                overline_color: None,
                overstrike: false,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn for_each_grid_row_glyph(
        &self,
        window_id: DisplayWindowId,
        row_index: u32,
        glyph_row: &GlyphRow,
        pixel_bounds: Rect,
        area_layout: GlyphRowAreaLayout,
        char_w: f32,
        char_h: f32,
        emit: &mut impl FnMut(FrameGlyph),
    ) {
        if !glyph_row.enabled {
            return;
        }

        // Keep one row affine until its legacy chrome completion is known.
        // GNU transfers the terminal side of a boxed chrome run to the final
        // stretch glyph; emitting directly would leave no way to retract that
        // side from the preceding glyph.
        let mut materialized_row = Vec::new();
        let mut push = |glyph| materialized_row.push(glyph);

        let win_x = pixel_bounds.x;
        let win_y = pixel_bounds.y;
        let win_w = pixel_bounds.width;
        let row_cell = MaterializedRowCell::new(win_y, row_index, glyph_row, char_h);
        let y = row_cell.y;
        let row_height = row_cell.height;
        let row_role = glyph_row.role;
        let mut col = usize::from(glyph_row.start_col);
        let mut x_cursor = win_x + glyph_row.pixel_x.max(0.0);
        let mut current_geometry = GlyphAreaGeometry::new(pixel_bounds, pixel_bounds);
        let chrome_clip_rect = Some(intersect_rects(
            pixel_bounds,
            Rect::new(win_x, y, win_w, row_height),
        ));
        let reversed_text_width = glyph_row.reversed_p.then(|| {
            glyph_row.glyphs[GlyphArea::Text.index()]
                .iter()
                .filter(|glyph| !glyph.padding)
                .map(|glyph| {
                    if glyph.pixel_width > 0.0 {
                        glyph.pixel_width
                    } else {
                        match &glyph.glyph_type {
                            GlyphType::Stretch { width_cols } => *width_cols as f32 * char_w,
                            _ if glyph.wide => char_w * 2.0,
                            _ => char_w,
                        }
                    }
                })
                .sum::<f32>()
        });

        for area in GlyphArea::ALL {
            if let GlyphAreaPlacement::Structural(geometry) = area_layout.placement(area) {
                current_geometry = geometry;
                let bounds = geometry.bounds();
                x_cursor = bounds.x;
                if area == GlyphArea::Text {
                    x_cursor += glyph_row.pixel_x.max(0.0);
                    // GNU's `reversed_p` applies only to TEXT_AREA. Marginal
                    // glyphs keep their own left-to-right structural origins.
                    if let Some(used) = reversed_text_width {
                        x_cursor = bounds.x + (bounds.width - used).max(0.0);
                    }
                }
            }
            let area_bounds = current_geometry.bounds();
            let clip_rect = Some(if row_role.is_chrome() {
                intersect_rects(
                    current_geometry.clip(),
                    Rect::new(area_bounds.x, y, area_bounds.width, row_height),
                )
            } else {
                current_geometry.clip()
            });
            let right_edge = area_bounds.right();
            for glyph in &glyph_row.glyphs[area.index()] {
                if glyph.padding {
                    continue;
                }
                let fallback_width = match &glyph.glyph_type {
                    GlyphType::Stretch { width_cols } => *width_cols as f32 * char_w,
                    GlyphType::Image { .. }
                    | GlyphType::Video { .. }
                    | GlyphType::Xwidget { .. }
                    | GlyphType::Surface { .. }
                    | GlyphType::Glyphless { .. } => char_w,
                    GlyphType::AutomaticComposite { terminal, .. } => {
                        f32::from(terminal.width_cols) * char_w
                    }
                    GlyphType::Char { .. } | GlyphType::Composite { .. } => {
                        if glyph.wide {
                            char_w * 2.0
                        } else {
                            char_w
                        }
                    }
                };
                let glyph_width = if glyph.pixel_width > 0.0 {
                    glyph.pixel_width
                } else {
                    fallback_width
                };
                let x = x_cursor;
                if x >= right_edge {
                    break;
                }
                let materialized_width = glyph_width.min(right_edge - x).max(0.0);
                if materialized_width <= 0.0 {
                    break;
                }
                let slot_id = DisplaySlotId {
                    window_id,
                    row: row_index,
                    col: col as u16,
                };

                match &glyph.glyph_type {
                    GlyphType::Char { ch } => {
                        let font_ascent =
                            self.resolve_face_for_materialize(glyph.face_id).font_ascent;
                        let row_ascent = if glyph_row.ascent_px > 0.0 {
                            glyph_row.ascent_px
                        } else if font_ascent > 0.0 {
                            font_ascent.min(row_height)
                        } else {
                            row_height
                        };
                        let cell = row_cell.rect(x, materialized_width);
                        let baseline = cell.y + row_ascent + glyph.vertical_offset_px;
                        push(FrameGlyph::Char {
                            window_id,
                            row_role,
                            clip_rect,
                            slot_id,
                            bidi_level: glyph.bidi_level,
                            char: *ch,
                            composed: None,
                            x: cell.x,
                            y: cell.y,
                            baseline,
                            width: cell.width,
                            height: cell.height,
                            ascent: if font_ascent > 0.0 {
                                font_ascent.min(row_height)
                            } else {
                                row_ascent
                            },
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Composite { text } | GlyphType::AutomaticComposite { text, .. } => {
                        let font_ascent =
                            self.resolve_face_for_materialize(glyph.face_id).font_ascent;
                        let row_ascent = if glyph_row.ascent_px > 0.0 {
                            glyph_row.ascent_px
                        } else if font_ascent > 0.0 {
                            font_ascent.min(row_height)
                        } else {
                            row_height
                        };
                        let cell = row_cell.rect(x, materialized_width);
                        let baseline = cell.y + row_ascent + glyph.vertical_offset_px;
                        push(FrameGlyph::Char {
                            window_id,
                            row_role,
                            clip_rect,
                            slot_id,
                            bidi_level: glyph.bidi_level,
                            char: text.chars().next().unwrap_or(' '),
                            composed: Some(text.clone()),
                            x: cell.x,
                            y: cell.y,
                            baseline,
                            width: cell.width,
                            height: cell.height,
                            ascent: if font_ascent > 0.0 {
                                font_ascent.min(row_height)
                            } else {
                                row_ascent
                            },
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Stretch { .. } => {
                        let face_data = self.resolve_face_for_materialize(glyph.face_id);
                        let cell = row_cell.rect(x, materialized_width);
                        push(FrameGlyph::Stretch {
                            window_id,
                            row_role,
                            clip_rect,
                            slot_id,
                            bidi_level: glyph.bidi_level,
                            x: cell.x,
                            y: cell.y,
                            width: cell.width,
                            height: cell.height,
                            bg: face_data.bg,
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Image {
                        image_id,
                        source_rect,
                        margins: margins_id,
                        ..
                    } => {
                        let margins = glyph_row
                            .image_margins(*margins_id)
                            .copied()
                            .unwrap_or_default();
                        let left_margin = margins.left();
                        let right_margin = margins.right();
                        let top_margin = margins.top();
                        let bottom_margin = margins.bottom();
                        let baseline = y + if glyph_row.ascent_px > 0.0 {
                            glyph_row.ascent_px
                        } else {
                            row_height
                        };
                        let layout_height = if glyph.pixel_height > 0.0 {
                            glyph.pixel_height
                        } else {
                            row_height
                        };
                        let layout_ascent = if glyph.pixel_ascent > 0.0 {
                            glyph.pixel_ascent
                        } else {
                            layout_height
                        };
                        let slot_y = baseline - layout_ascent + glyph.vertical_offset_px;
                        push(FrameGlyph::Image {
                            window_id,
                            row_role,
                            clip_rect,
                            slot_id: Some(slot_id),
                            image_id: ImageId::new(*image_id as u32),
                            source_rect: *source_rect,
                            slot_rect: Rect::new(x, slot_y, materialized_width, layout_height),
                            box_rect: Rect::new(x, y, materialized_width, row_height),
                            x: x + left_margin,
                            y: slot_y + top_margin,
                            width: (materialized_width - left_margin - right_margin).max(0.0),
                            height: (layout_height - top_margin - bottom_margin).max(0.0),
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Video {
                        video_id, opacity, ..
                    } => {
                        let layout_height = if glyph.pixel_height > 0.0 {
                            glyph.pixel_height
                        } else {
                            row_height
                        };
                        let layout_ascent = if glyph.pixel_ascent > 0.0 {
                            glyph.pixel_ascent
                        } else {
                            layout_height
                        };
                        let baseline = y + if glyph_row.ascent_px > 0.0 {
                            glyph_row.ascent_px
                        } else {
                            row_height
                        };
                        push(FrameGlyph::Video {
                            window_id,
                            row_role,
                            clip_rect,
                            slot_id: Some(slot_id),
                            video_id: *video_id,
                            x,
                            y: baseline - layout_ascent + glyph.vertical_offset_px,
                            width: materialized_width,
                            height: layout_height,
                            opacity: *opacity,
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Xwidget {
                        xwidget_id,
                        webview_id,
                        content,
                        ..
                    } => {
                        let layout_height = if glyph.pixel_height > 0.0 {
                            glyph.pixel_height
                        } else {
                            row_height
                        };
                        let layout_ascent = if glyph.pixel_ascent > 0.0 {
                            glyph.pixel_ascent
                        } else {
                            layout_height
                        };
                        let baseline = y + if glyph_row.ascent_px > 0.0 {
                            glyph_row.ascent_px
                        } else {
                            row_height
                        };
                        let origin = GeometryPoint::<FrameSpace, LogicalPixels>::from_px(
                            x,
                            baseline - layout_ascent + glyph.vertical_offset_px,
                        )
                        .expect("materialized xwidget origin is finite");
                        let clip = clip_rect.map(|clip| {
                            GeometryRect::<FrameSpace, LogicalPixels>::new(
                                clip.x,
                                clip.y,
                                clip.width,
                                clip.height,
                            )
                            .expect("materialized xwidget clip is valid")
                        });
                        // Unlike ordinary painted glyphs, an xwidget carries
                        // its visible text-area clip separately.  GNU keeps a
                        // narrow overflowing xwidget's whole glyph advance in
                        // truncating rows (`display_line`, src/xdisp.c) and
                        // clips only the native presentation
                        // (`x_draw_xwidget_glyph_string`, src/xwidget.c).
                        let layout_advance = XwidgetLayoutAdvance::new(Px(glyph_width))
                            .expect("a materialized xwidget has positive finite width");
                        push(FrameGlyph::Xwidget {
                            window_id,
                            row_role,
                            slot_id: Some(slot_id),
                            xwidget_id: *xwidget_id,
                            webview_id: *webview_id,
                            presentation: XwidgetPresentationGeometry::new(
                                origin,
                                *content,
                                layout_advance,
                                clip,
                            ),
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Surface { surface_id, .. } => {
                        let layout_height = if glyph.pixel_height > 0.0 {
                            glyph.pixel_height
                        } else {
                            row_height
                        };
                        let layout_ascent = if glyph.pixel_ascent > 0.0 {
                            glyph.pixel_ascent
                        } else {
                            layout_height
                        };
                        let baseline = y + if glyph_row.ascent_px > 0.0 {
                            glyph_row.ascent_px
                        } else {
                            row_height
                        };
                        push(FrameGlyph::Surface {
                            window_id,
                            row_role,
                            clip_rect,
                            slot_id: Some(slot_id),
                            surface_id: SurfaceId::new(*surface_id as u32),
                            x,
                            y: baseline - layout_ascent + glyph.vertical_offset_px,
                            width: materialized_width,
                            height: layout_height,
                            face_id: glyph.face_id,
                            box_vertical_edges: glyph.box_vertical_edges,
                        });
                    }
                    GlyphType::Glyphless { ch, presentation } => match presentation {
                        GlyphlessPresentation::ThinSpace => {
                            let face_data = self.resolve_face_for_materialize(glyph.face_id);
                            push(FrameGlyph::Stretch {
                                window_id,
                                row_role,
                                clip_rect,
                                slot_id,
                                bidi_level: glyph.bidi_level,
                                x,
                                y,
                                width: materialized_width,
                                height: row_height,
                                bg: face_data.bg,
                                face_id: glyph.face_id,
                                box_vertical_edges: glyph.box_vertical_edges,
                            });
                        }
                        GlyphlessPresentation::EmptyBox | GlyphlessPresentation::HexCode => {
                            let font_ascent =
                                self.resolve_face_for_materialize(glyph.face_id).font_ascent;
                            let row_ascent = if glyph_row.ascent_px > 0.0 {
                                glyph_row.ascent_px
                            } else if font_ascent > 0.0 {
                                font_ascent.min(row_height)
                            } else {
                                row_height
                            };
                            let baseline = y + row_ascent + glyph.vertical_offset_px;
                            push(FrameGlyph::Char {
                                window_id,
                                row_role,
                                clip_rect,
                                slot_id,
                                bidi_level: glyph.bidi_level,
                                char: *ch,
                                composed: None,
                                x,
                                y,
                                baseline,
                                width: materialized_width,
                                height: row_height,
                                ascent: if font_ascent > 0.0 {
                                    font_ascent.min(row_height)
                                } else {
                                    row_ascent
                                },
                                face_id: glyph.face_id,
                                box_vertical_edges: glyph.box_vertical_edges,
                            });
                        }
                    },
                }
                col += usize::from(glyph.materialized_slot_span());
                x_cursor += glyph_width;
            }
        }

        // Window-owned chrome still relies on the legacy row-completion
        // fallback introduced before typed frame chrome.  A frame tab bar is
        // already a complete `FrameChromeContent::DisplayRow` scene: adding a
        // second, inferred primitive here loses GNU's iterator-selected tail
        // face and can smear the last item's background across the band.
        let final_x = x_cursor.min(win_x + win_w);
        let right_edge = win_x + win_w;
        drop(push);
        if final_x < right_edge
            && col > 0
            && row_role.is_chrome()
            && row_role != GlyphRowRole::TabBar
        {
            let last_face_id = glyph_row
                .glyphs
                .iter()
                .rev()
                .flat_map(|area| area.iter().rev())
                .find(|g| !g.padding)
                .map(|g| g.face_id)
                .unwrap_or(FaceId::new(0));
            let face_data = self.resolve_face_for_materialize(last_face_id);
            let fill_edges = if self
                .faces
                .get(&last_face_id)
                .is_some_and(|face| face.box_type != crate::face::BoxType::None)
            {
                if let Some(previous) = materialized_row
                    .iter_mut()
                    .rev()
                    .find(|glyph| glyph.box_vertical_edges().is_some())
                    && let Some(previous_edges) = previous.box_vertical_edges()
                {
                    previous.set_box_vertical_edges(crate::face::BoxVerticalEdges::from_ownership(
                        previous_edges.owns_left(),
                        false,
                    ));
                }
                crate::face::BoxVerticalEdges::Right
            } else {
                crate::face::BoxVerticalEdges::Both
            };
            materialized_row.push(FrameGlyph::Stretch {
                window_id,
                row_role,
                clip_rect: chrome_clip_rect,
                slot_id: DisplaySlotId {
                    window_id,
                    row: row_index,
                    col: col as u16,
                },
                bidi_level: 0,
                x: final_x,
                y,
                width: right_edge - final_x,
                height: row_height,
                bg: face_data.bg,
                face_id: last_face_id,
                box_vertical_edges: fill_edges,
            });
        }

        for glyph in materialized_row {
            emit(glyph);
        }
    }
}

impl WindowMatrixEntry {
    pub fn row_pixel_bounds(&self, role: GlyphRowRole) -> Rect {
        if role == GlyphRowRole::Text {
            self.text_pixel_bounds
        } else {
            self.pixel_bounds
        }
    }

    /// Vertical clip band for buffer-text (`Text` role) rows: the window's text
    /// area between the tab/header lines and the mode line.
    ///
    /// A `w->vscroll` scrolls a window's contents UP, so the first body row is
    /// laid out above this band (top-clipped) and one extra, partially visible
    /// row is exposed at the bottom (below the last full row).  The renderer
    /// clips every glyph/background vertically to its `clip_rect`; clamping body
    /// rows to this band keeps that vscroll overflow from bleeding over the
    /// header/tab-line chrome above or the mode-line below.
    ///
    /// The band is derived from the chrome rows already present in the matrix
    /// (the header/tab lines' bottoms and the mode line's top), which — unlike
    /// the buffer rows — are NOT shifted by vscroll and so are stable anchors.
    /// The horizontal extent keeps `text_pixel_bounds` (the text columns), so
    /// with no chrome rows this reproduces `text_pixel_bounds` byte-for-byte —
    /// the clip only narrows vertically, and only when chrome rows are present.
    pub fn text_area_clip_rect(&self) -> Rect {
        if let Some(clip) = self.text_clip_bounds {
            return clip;
        }
        let win = self.pixel_bounds;
        let text = self.text_pixel_bounds;
        let mut top = win.y;
        let mut bottom = win.y + win.height;
        for row in &self.matrix.rows {
            if !row.enabled || row.height_px <= 0.0 {
                continue;
            }
            let row_top = win.y + row.pixel_y;
            match row.role {
                GlyphRowRole::TabLine | GlyphRowRole::HeaderLine => {
                    top = top.max(row_top + row.height_px);
                }
                GlyphRowRole::ModeLine => {
                    bottom = bottom.min(row_top);
                }
                _ => {}
            }
        }
        Rect::new(text.x, top, text.width, (bottom - top).max(0.0))
    }
}

#[derive(Clone, Debug)]
pub struct ScrollRun {
    pub window_id: u64,
    pub first_row: usize,
    pub last_row: usize,
    pub distance: i32,
}

pub trait RedisplayInterface {
    fn update_window_begin(&mut self, window_id: u64);
    fn write_glyphs(&mut self, row: &GlyphRow, area: GlyphArea, start: usize, len: usize);
    fn clear_end_of_line(&mut self, row: &GlyphRow, area: GlyphArea);
    fn scroll_run(&mut self, run: &ScrollRun);
    fn update_window_end(&mut self, window_id: u64);
    fn set_cursor(&mut self, row: u16, col: u16, style: CursorStyle);
    fn flush(&mut self);
}

#[cfg(test)]
#[path = "glyph_matrix_test.rs"]
mod tests;
