//! Frame glyph buffer for matrix-based full-frame rendering.
//!
//! Each frame, the C-side matrix walker extracts ALL visible glyphs from
//! Emacs's current_matrix and rebuilds this buffer from scratch. No
//! incremental overlap tracking is needed.

use crate::effect_config::EffectsConfig;
use crate::face::{
    BasicFaceId, BoxBorderStyle, BoxType, BoxVerticalEdges, Face, FaceAttributes, UnderlineStyle,
};
use crate::types::{
    Color, DisplayFrameId, DisplayWindowId, FaceId, ImageId, Px, Rect, SurfaceId, VideoId,
    WebViewId, XwidgetId,
};
use crate::xwidget_extent::XwidgetPresentationGeometry;
use crate::{ContentTransitionIntent, FrameSpace, GeometryRect, LogicalPixels};
use std::collections::HashMap;

pub use crate::cursor::{CursorBarWidth, CursorKind, CursorSpec, CursorStyle};

/// Semantic role of a glyph row emitted by layout.
///
/// This is authoritative layout metadata used by renderer ordering/clipping.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum GlyphRowRole {
    /// Regular buffer text rows.
    #[default]
    Text,
    /// Tab-line row.
    TabLine,
    /// Header-line row.
    HeaderLine,
    /// Mode-line row.
    ModeLine,
    /// Minibuffer/echo text row.
    Minibuffer,
    /// Frame-level tab-bar row.
    TabBar,
}

impl GlyphRowRole {
    /// True for UI chrome rows that should render above regular text rows.
    pub fn is_chrome(self) -> bool {
        matches!(
            self,
            Self::TabLine | Self::HeaderLine | Self::ModeLine | Self::TabBar
        )
    }
}

/// Stable identity for one materialized display slot within a frame.
///
/// This is the shared contract between layout and rendering for
/// "the thing under point": the cursor points at a slot id, and the
/// renderer can target that exact slot instead of re-discovering it
/// from geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DisplaySlotId {
    /// Window that owns the slot.
    pub window_id: DisplayWindowId,
    /// Visual row within the owning window.
    pub row: u32,
    /// Visual column within that row.
    pub col: u16,
}

impl DisplaySlotId {
    pub const ZERO: Self = Self {
        window_id: DisplayWindowId::new(0),
        row: 0,
        col: 0,
    };

    /// Best-effort slot identity for direct pixel-emission paths.
    ///
    /// Matrix-backed layout should populate slot ids from explicit row/column
    /// indices. This helper exists for manual glyph construction in tests and
    /// direct frame-space emission paths that have not been matrix-ified yet.
    pub fn from_pixels(
        window_id: DisplayWindowId,
        x: Px,
        y: Px,
        char_width: Px,
        char_height: Px,
    ) -> Self {
        let row = y.cells_rounded(char_height).get();
        let col = x.cells_rounded(char_width).get().min(u16::MAX as u32) as u16;
        Self {
            window_id,
            row,
            col,
        }
    }
}

impl Default for DisplaySlotId {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Resolved face attributes for one `face_id`, used at draw time by every
/// consumer of [`FrameGlyph::Char`].
///
/// These fields were previously inlined on every character glyph as pure
/// denormalization of the glyph's `face_id`. They are now resolved on demand
/// from the frame face table by [`FrameGlyphBuffer::resolved_face`] (and the
/// identical layout-side [`crate::glyph_matrix::FrameDisplayState`] resolver),
/// keeping the per-glyph payload small.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterializedFaceData {
    pub fg: Color,
    pub bg: Color,
    pub font_ascent: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub font_size: f32,
    pub underline: UnderlineStyle,
    pub underline_color: Option<Color>,
    pub strike_through: bool,
    pub strike_through_color: Option<Color>,
    pub overline: bool,
    pub overline_color: Option<Color>,
    pub overstrike: bool,
}

/// A single glyph to render
#[derive(Debug, Clone, PartialEq)]
pub enum FrameGlyph {
    /// Character glyph with text
    Char {
        /// Window identifier this glyph belongs to.
        window_id: DisplayWindowId,
        /// Layout row role for ordering.
        row_role: GlyphRowRole,
        /// Authoritative clip rect in frame coordinates.
        clip_rect: Option<Rect>,
        /// Stable identity of the covered display slot.
        slot_id: DisplaySlotId,
        /// Bidirectional resolved level for this displayed glyph.
        ///
        /// 0 is the default LTR level; odd values indicate RTL runs.
        bidi_level: u8,
        /// Character to render (base character for single-codepoint glyphs)
        char: char,
        /// Composed text for multi-codepoint grapheme clusters (emoji ZWJ, combining marks).
        /// When Some, the renderer uses this instead of `char` for glyph lookup.
        composed: Option<Box<str>>,
        /// Frame-absolute X position
        x: f32,
        /// Frame-absolute Y position
        y: f32,
        /// Frame-absolute baseline Y position (authoritative from layout)
        baseline: f32,
        /// Glyph width
        width: f32,
        /// Row height
        height: f32,
        /// Font ascent
        ascent: f32,
        /// Face ID for font lookup and all visual face attributes.
        ///
        /// Foreground/background colors, font weight/size, italic, and the
        /// underline/strike-through/overline/overstrike decorations are NOT
        /// stored per glyph; they are resolved from the frame face table by
        /// this id at draw time via [`FrameGlyphBuffer::resolved_face`]. Both
        /// build paths (`materialize` and `set_face`/`set_face_with_font`)
        /// populate `FrameGlyphBuffer::faces` for every emitted `face_id`, so
        /// the lookup is always valid.
        face_id: FaceId,
        /// Vertical sides of this glyph's box run that GNU layout owns.
        ///
        /// Top and bottom rails are properties of every boxed glyph.  GNU's
        /// `left_box_line_p` / `right_box_line_p` flags independently control
        /// only the two terminal sides.
        box_vertical_edges: BoxVerticalEdges,
    },

    /// Stretch (whitespace) glyph
    Stretch {
        /// Window identifier this glyph belongs to.
        window_id: DisplayWindowId,
        /// Layout row role for ordering.
        row_role: GlyphRowRole,
        /// Authoritative clip rect in frame coordinates.
        clip_rect: Option<Rect>,
        /// Stable identity of the covered display slot.
        slot_id: DisplaySlotId,
        /// Bidirectional resolved level for this displayed slot.
        ///
        /// 0 is the default LTR level; odd values indicate RTL runs.
        bidi_level: u8,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        bg: Color,
        face_id: FaceId,
        /// Vertical sides of this glyph's box run that GNU layout owns.
        box_vertical_edges: BoxVerticalEdges,
    },

    /// Image glyph
    Image {
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        clip_rect: Option<Rect>,
        slot_id: Option<DisplaySlotId>,
        image_id: ImageId,
        source_rect: crate::image::ImageSourceRect,
        /// Full image slot, including image margins. Texture sampling uses the
        /// margin-inset `x/y/width/height`; cursor and pointer geometry use this
        /// authoritative slot.
        slot_rect: Rect,
        /// GNU glyph-string box/background extent.  An image may be shorter
        /// than its display row, but its face box spans the complete row.
        box_rect: Rect,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        face_id: FaceId,
        box_vertical_edges: BoxVerticalEdges,
    },

    /// Video glyph (inline in buffer)
    Video {
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        clip_rect: Option<Rect>,
        slot_id: Option<DisplaySlotId>,
        video_id: VideoId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// Compositor alpha in the closed interval 0..=1.
        opacity: f32,
        face_id: FaceId,
        box_vertical_edges: BoxVerticalEdges,
    },

    /// Xwidget glyph (inline in buffer).
    ///
    /// [`XwidgetPresentationGeometry`] keeps GNU's three distinct geometries
    /// together: intrinsic widget content, possibly cropped glyph advance,
    /// and the window clip.  Native placement, Linux composition, cursor
    /// geometry, and hit-testing all consume that one typed contract.
    Xwidget {
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        slot_id: Option<DisplaySlotId>,
        xwidget_id: XwidgetId,
        webview_id: WebViewId,
        presentation: XwidgetPresentationGeometry<FrameSpace>,
        face_id: FaceId,
        box_vertical_edges: BoxVerticalEdges,
    },

    /// Shader-surface glyph (inline in buffer): a texture the compositor
    /// renders from a user-supplied WGSL shader or uploaded pixels
    /// (`docs/display-engine/SHADER_SURFACES.md`).
    Surface {
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        clip_rect: Option<Rect>,
        slot_id: Option<DisplaySlotId>,
        surface_id: SurfaceId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        face_id: FaceId,
        box_vertical_edges: BoxVerticalEdges,
    },

    /// Fringe bitmap drawn in a window's left or right fringe column. The
    /// monochrome bits live once per frame in
    /// [`FrameGlyphBuffer::fringe_bitmaps`], keyed by `bitmap_index`; the
    /// renderer expands them to foreground quads in the fringe column. GNU draws
    /// these via `rif->draw_fringe_bitmap`.
    FringeBitmap {
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        clip_rect: Option<Rect>,
        /// Frame-absolute X of the fringe column's left edge.
        x: f32,
        /// Frame-absolute Y of the row top.
        y: f32,
        /// Fringe column width in pixels.
        width: f32,
        /// Row height in pixels (the bitmap is aligned within this).
        height: f32,
        /// Resolved registry index into `FrameGlyphBuffer::fringe_bitmaps`.
        bitmap_index: u16,
        /// Face id for fg/bg colors (resolved via `resolved_face`).
        face_id: FaceId,
        /// Which fringe this bitmap belongs to.
        side: FringeSide,
    },

    /// Window background
    Background { bounds: Rect, color: Color },

    /// Window border (vertical/horizontal divider)
    Border {
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        clip_rect: Option<Rect>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },

    /// Scroll bar (GPU-rendered)
    ScrollBar {
        /// Window identifier this scroll bar belongs to.
        window_id: DisplayWindowId,
        /// Layout row role for ordering.
        row_role: GlyphRowRole,
        /// Authoritative clip rect in frame coordinates.
        clip_rect: Option<Rect>,
        /// True for horizontal, false for vertical
        horizontal: bool,
        /// Frame-absolute position and dimensions of the scroll bar track
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// GNU-compatible scroll-bar semantic position.
        position: i64,
        /// GNU-compatible visible portion size.
        portion: i64,
        /// GNU-compatible whole buffer/content size.
        whole: i64,
        /// Thumb start position (pixels from track start)
        thumb_start: f32,
        /// Thumb size (pixels)
        thumb_size: f32,
        /// Track background color
        track_color: Color,
        /// Thumb color
        thumb_color: Color,
    },

    /// Terminal glyph (inline in buffer or window-mode)
    #[cfg(feature = "neo-term")]
    Terminal {
        terminal_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl FrameGlyph {
    /// Returns true if this glyph belongs to a chrome row
    /// that should be rendered above regular text rows.
    pub fn is_chrome_row(&self) -> bool {
        match self {
            FrameGlyph::Char { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::Stretch { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::Image { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::Video { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::Xwidget { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::Surface { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::FringeBitmap { row_role, .. } => row_role.is_chrome(),
            FrameGlyph::Border { row_role, .. } => row_role.is_chrome(),
            _ => false,
        }
    }

    /// Backward-compatible alias for callers not yet renamed.
    pub fn is_overlay(&self) -> bool {
        self.is_chrome_row()
    }

    /// Slot identity for displayed content that occupies a character cell.
    pub fn slot_id(&self) -> Option<DisplaySlotId> {
        match self {
            FrameGlyph::Char { slot_id, .. } | FrameGlyph::Stretch { slot_id, .. } => {
                Some(*slot_id)
            }
            FrameGlyph::Image { slot_id, .. }
            | FrameGlyph::Video { slot_id, .. }
            | FrameGlyph::Xwidget { slot_id, .. }
            | FrameGlyph::Surface { slot_id, .. } => *slot_id,
            _ => None,
        }
    }

    /// Bidirectional resolved level for displayed character/stretch slots.
    pub fn bidi_level(&self) -> Option<u8> {
        match self {
            FrameGlyph::Char { bidi_level, .. } | FrameGlyph::Stretch { bidi_level, .. } => {
                Some(*bidi_level)
            }
            _ => None,
        }
    }

    /// Vertical box-edge ownership for face-bearing character cells.
    pub fn box_vertical_edges(&self) -> Option<BoxVerticalEdges> {
        match self {
            FrameGlyph::Char {
                box_vertical_edges, ..
            }
            | FrameGlyph::Stretch {
                box_vertical_edges, ..
            }
            | FrameGlyph::Image {
                box_vertical_edges, ..
            }
            | FrameGlyph::Video {
                box_vertical_edges, ..
            }
            | FrameGlyph::Xwidget {
                box_vertical_edges, ..
            }
            | FrameGlyph::Surface {
                box_vertical_edges, ..
            } => Some(*box_vertical_edges),
            _ => None,
        }
    }

    pub(crate) fn set_box_vertical_edges(&mut self, edges: BoxVerticalEdges) {
        match self {
            FrameGlyph::Char {
                box_vertical_edges, ..
            }
            | FrameGlyph::Stretch {
                box_vertical_edges, ..
            }
            | FrameGlyph::Image {
                box_vertical_edges, ..
            }
            | FrameGlyph::Video {
                box_vertical_edges, ..
            }
            | FrameGlyph::Xwidget {
                box_vertical_edges, ..
            }
            | FrameGlyph::Surface {
                box_vertical_edges, ..
            } => *box_vertical_edges = edges,
            _ => {}
        }
    }

    /// Pixel rect `(x, y, width, height)` of the cell this glyph occupies. This
    /// is the authoritative position at which a cursor sitting on the glyph is
    /// drawn; the cursor's grid-derived `PhysCursor` geometry is only an
    /// approximation that diverges from the glyph under scaled fonts. Returns
    /// `None` for glyph kinds that do not occupy a cursor cell (backgrounds,
    /// borders, scroll bars).
    pub fn cell_rect(&self) -> Option<(f32, f32, f32, f32)> {
        match self {
            FrameGlyph::Char {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Video {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Surface {
                x,
                y,
                width,
                height,
                ..
            } => Some((*x, *y, *width, *height)),
            FrameGlyph::Xwidget { presentation, .. } => {
                let slot = presentation.layout_slot_rect();
                Some((slot.x(), slot.y(), slot.width(), slot.height()))
            }
            FrameGlyph::Image { slot_rect, .. } => {
                Some((slot_rect.x, slot_rect.y, slot_rect.width, slot_rect.height))
            }
            _ => None,
        }
    }

    /// Frame-absolute rectangle used for GNU face box painting.
    pub fn box_rect(&self) -> Option<Rect> {
        match self {
            FrameGlyph::Image { box_rect, .. } => Some(*box_rect),
            _ => self
                .cell_rect()
                .map(|(x, y, width, height)| Rect::new(x, y, width, height)),
        }
    }

    /// Whether box rendering also owns this glyph's face-background fill.
    /// Character and stretch backgrounds have dedicated passes; an image
    /// texture covers only its margin-inset content rectangle.
    pub fn box_requires_background_fill(&self) -> bool {
        matches!(self, FrameGlyph::Image { .. })
    }

    /// Left edge of this glyph's cell; see [`FrameGlyph::cell_rect`].
    pub fn cell_x(&self) -> Option<f32> {
        self.cell_rect().map(|(x, ..)| x)
    }

    /// Owning window id for any window-attached glyph. `None` for the
    /// frame-level background and detached terminal glyphs.
    pub fn window_id(&self) -> Option<DisplayWindowId> {
        match self {
            FrameGlyph::Char { window_id, .. }
            | FrameGlyph::Stretch { window_id, .. }
            | FrameGlyph::Image { window_id, .. }
            | FrameGlyph::Video { window_id, .. }
            | FrameGlyph::Xwidget { window_id, .. }
            | FrameGlyph::Surface { window_id, .. }
            | FrameGlyph::FringeBitmap { window_id, .. }
            | FrameGlyph::Border { window_id, .. }
            | FrameGlyph::ScrollBar { window_id, .. } => Some(*window_id),
            _ => None,
        }
    }

    /// Layout row role used for z-ordering. `None` for the frame background
    /// and detached terminal glyphs.
    pub fn row_role(&self) -> Option<GlyphRowRole> {
        match self {
            FrameGlyph::Char { row_role, .. }
            | FrameGlyph::Stretch { row_role, .. }
            | FrameGlyph::Image { row_role, .. }
            | FrameGlyph::Video { row_role, .. }
            | FrameGlyph::Xwidget { row_role, .. }
            | FrameGlyph::Surface { row_role, .. }
            | FrameGlyph::FringeBitmap { row_role, .. }
            | FrameGlyph::Border { row_role, .. }
            | FrameGlyph::ScrollBar { row_role, .. } => Some(*row_role),
            _ => None,
        }
    }

    /// Authoritative clip rect in frame coordinates, if the glyph carries one.
    pub fn clip_rect(&self) -> Option<Rect> {
        match self {
            FrameGlyph::Char { clip_rect, .. }
            | FrameGlyph::Stretch { clip_rect, .. }
            | FrameGlyph::Image { clip_rect, .. }
            | FrameGlyph::Video { clip_rect, .. }
            | FrameGlyph::Surface { clip_rect, .. }
            | FrameGlyph::FringeBitmap { clip_rect, .. }
            | FrameGlyph::Border { clip_rect, .. }
            | FrameGlyph::ScrollBar { clip_rect, .. } => *clip_rect,
            FrameGlyph::Xwidget { presentation, .. } => presentation
                .clip_rect()
                .map(|clip| Rect::new(clip.x(), clip.y(), clip.width(), clip.height())),
            _ => None,
        }
    }

    /// Frame-absolute visual rect of this glyph. Unlike [`FrameGlyph::cell_rect`]
    /// (which is only the cursor-cell kinds) this covers every drawn kind,
    /// including borders, scroll bars, and the frame background (whose `bounds`
    /// are its rect). Exhaustive on purpose: a new variant must declare its rect.
    pub fn geometry(&self) -> Option<Rect> {
        match self {
            FrameGlyph::Char {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Image {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Video {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Surface {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::Border {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::ScrollBar {
                x,
                y,
                width,
                height,
                ..
            }
            | FrameGlyph::FringeBitmap {
                x,
                y,
                width,
                height,
                ..
            } => Some(Rect::new(*x, *y, *width, *height)),
            FrameGlyph::Xwidget { presentation, .. } => {
                let slot = presentation.layout_slot_rect();
                Some(Rect::new(slot.x(), slot.y(), slot.width(), slot.height()))
            }
            FrameGlyph::Background { bounds, .. } => Some(*bounds),
            #[cfg(feature = "neo-term")]
            FrameGlyph::Terminal {
                x,
                y,
                width,
                height,
                ..
            } => Some(Rect::new(*x, *y, *width, *height)),
        }
    }

    /// Face id for every inline face-bearing glyph.
    pub fn face_id(&self) -> Option<FaceId> {
        match self {
            FrameGlyph::Char { face_id, .. }
            | FrameGlyph::Stretch { face_id, .. }
            | FrameGlyph::Image { face_id, .. }
            | FrameGlyph::Video { face_id, .. }
            | FrameGlyph::Xwidget { face_id, .. }
            | FrameGlyph::Surface { face_id, .. } => Some(*face_id),
            _ => None,
        }
    }

    /// Mutable counterpart to [`Self::face_id`] for scene-layer identity
    /// remapping.
    ///
    /// Keeping the exhaustive set of inline face-bearing variants beside the
    /// protocol enum avoids renderer-owned layers each carrying a partial,
    /// silently divergent copy of this knowledge.
    pub fn face_id_mut(&mut self) -> Option<&mut FaceId> {
        match self {
            FrameGlyph::Char { face_id, .. }
            | FrameGlyph::Stretch { face_id, .. }
            | FrameGlyph::Image { face_id, .. }
            | FrameGlyph::Video { face_id, .. }
            | FrameGlyph::Xwidget { face_id, .. }
            | FrameGlyph::Surface { face_id, .. } => Some(face_id),
            _ => None,
        }
    }
}

/// Authoritative physical cursor snapshot for a frame.
///
/// This mirrors GNU's `phys_cursor` / `phys_cursor_*` split at the
/// display-protocol level: layout owns the cursor slot and geometry,
/// the renderer only consumes it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PhysCursor {
    /// Window that owns the cursor.
    pub window_id: DisplayWindowId,
    /// Buffer position covered by the cursor slot.
    pub charpos: usize,
    /// Matrix row that owns the cursor.
    pub row: usize,
    /// Column within the owning row.
    pub col: u16,
    /// Stable identity of the covered display slot.
    pub slot_id: DisplaySlotId,
    /// Frame-absolute cursor origin.
    pub x: f32,
    pub y: f32,
    /// Cursor rectangle dimensions in pixels.
    pub width: f32,
    pub height: f32,
    /// Pixels above the baseline.
    pub ascent: f32,
    /// Visual cursor style.
    pub style: CursorStyle,
    /// Cursor color.
    pub color: Color,
    /// Foreground color to use when redrawing the covered slot.
    pub cursor_fg: Color,
}

/// Unified per-window cursor emitted by layout.
///
/// Exactly one entry per window exists in `FrameGlyphBuffer::window_cursors`.
/// The selected window's entry has `active: true` (the former
/// `FrameGlyphBuffer::phys_cursor`); non-selected windows are decorative.
/// Geometry (x/y/width/height) lives here so animation/spacing/bidi code can
/// adjust every cursor uniformly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowCursor {
    pub window_id: DisplayWindowId,
    pub slot_id: DisplaySlotId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub style: CursorStyle,
    pub color: Color,
    /// Foreground for the glyph under a box cursor. Default Color::BLACK for
    /// non-selected/decorative cursors that never invert text.
    pub cursor_fg: Color,
    /// Pixels above the baseline. 0.0 for decorative cursors.
    pub ascent: f32,
    /// True for the selected window's cursor (the former phys_cursor). Effects
    /// and the slide animation target this entry.
    pub active: bool,
}

/// Which fringe a [`FrameGlyph::FringeBitmap`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FringeSide {
    Left,
    Right,
}

/// Resolved fringe-bitmap data embedded once per frame in
/// [`FrameGlyphBuffer::fringe_bitmaps`]. Mirrors the user-bitmap registry on
/// the evaluator; `bits` rows are MSB-aligned `u16` (leftmost column is bit 15).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FringeBitmapData {
    /// One MSB-aligned row per `height`; column `b` of row `r` is set when
    /// `(bits[r] >> (15 - b)) & 1 == 1`.
    pub bits: Vec<u16>,
    /// Pixel columns used (1..=16).
    pub width: u8,
    /// Number of rows.
    pub height: u8,
    /// Repeat period (0 = not periodic).
    pub period: u8,
    /// Vertical alignment within the row: 0 = center, 1 = top, 2 = bottom.
    pub align: u8,
}

/// Stipple pattern: XBM bitmap data for tiled background patterns
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StipplePattern {
    /// Pattern width in pixels
    pub width: u32,
    /// Pattern height in pixels
    pub height: u32,
    /// Raw XBM bits: row-by-row, each row is (width+7)/8 bytes, LSB-first
    pub bits: Vec<u8>,
}

impl StipplePattern {
    /// The standard built-in mono bitmaps `:stipple` accepts by name, matching
    /// the X11 `bitmaps` GNU loads (and compiles in). Portable — no file lookup
    /// — so `:stipple "gray3"` (also `face-default-stipple`'s default) works on
    /// every platform. Bits are XBM: row-major, `(width+7)/8` bytes/row,
    /// LSB-first.
    pub fn builtin(name: &str) -> Option<Self> {
        let (width, height, bits): (u32, u32, &[u8]) = match name {
            "gray" | "gray1" => (2, 2, &[0x01, 0x02]),
            "gray3" => (4, 4, &[0x01, 0x00, 0x04, 0x00]),
            "light_gray" => (4, 2, &[0x08, 0x02]),
            _ => return None,
        };
        Some(Self {
            width,
            height,
            bits: bits.to_vec(),
        })
    }

    /// Parse an X BitMap (XBM) source file into a stipple pattern, mirroring
    /// GNU's `image_create_bitmap_from_file`. Handles the standard form:
    ///
    /// ```text
    /// #define name_width  W
    /// #define name_height H
    /// static char name_bits[] = { 0x01, 0x00, ... };
    /// ```
    ///
    /// Byte tokens may be hex (`0x01`), C char escapes (`'\x01'`, as GNU's own
    /// sources emit), or decimal. Returns `None` on a malformed file or when
    /// fewer than `(width+7)/8 * height` bytes are present.
    pub fn from_xbm_source(text: &str) -> Option<Self> {
        fn dimension(text: &str, suffix: &str) -> Option<u32> {
            text.lines().find_map(|line| {
                let rest = line[line.find(suffix)? + suffix.len()..].trim_start();
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
        }
        let width = dimension(text, "_width")?;
        let height = dimension(text, "_height")?;
        if width == 0 || height == 0 {
            return None;
        }
        let open = text.find('{')?;
        let close = text[open..].find('}')? + open;
        let mut bits = Vec::new();
        for tok in text[open + 1..close].split(',') {
            let t = tok.trim().trim_matches('\'').trim();
            if t.is_empty() {
                continue;
            }
            let byte = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                u8::from_str_radix(hex, 16).ok()?
            } else if let Some(hex) = t.strip_prefix("\\x") {
                u8::from_str_radix(hex, 16).ok()?
            } else {
                t.parse::<u8>().ok()?
            };
            bits.push(byte);
        }
        let expected = width.div_ceil(8) as usize * height as usize;
        if bits.len() < expected {
            return None;
        }
        bits.truncate(expected);
        Some(Self {
            width,
            height,
            bits,
        })
    }
}

/// Per-window metadata for animation transition detection
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PresentedCellOrigin {
    pub column: i64,
    pub line: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PresentedWindowRegions {
    pub outer: Rect,
    pub text_body: Rect,
    pub left_margin_columns: i64,
    pub right_margin_columns: i64,
    pub left_margin: Option<Rect>,
    pub right_margin: Option<Rect>,
    pub left_fringe: Option<Rect>,
    pub right_fringe: Option<Rect>,
    pub left_scroll_bar: Option<Rect>,
    pub right_scroll_bar: Option<Rect>,
    pub horizontal_scroll_bar: Option<Rect>,
    pub tab_line: Option<Rect>,
    pub header_line: Option<Rect>,
    pub mode_line: Option<Rect>,
    pub right_divider: Option<Rect>,
    pub bottom_divider: Option<Rect>,
}

/// Pixel width of the line-number field one window reserved at the left of its
/// text area — GNU's `it->lnum_pixel_width`.
///
/// GNU measures it by producing the number's glyphs with the window-resolved
/// line-number face (`xdisp.c` `maybe_produce_line_number`), so it is not
/// derivable from the frame's default cell width and cannot be recomputed by a
/// consumer.
///
/// It is a width and not a rect because the origin is already published:
/// the field starts at `PresentedWindowRegions::text_body.x`. A second copy of
/// that origin could disagree with the first.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LineNumberFieldWidth(f32);

impl LineNumberFieldWidth {
    /// `None` when the window reserved no line-number field, so a consumer
    /// cannot paint a zero-width band or read a NaN out of an unmeasured
    /// window.
    #[must_use]
    pub fn measured(px: f32) -> Option<Self> {
        (px.is_finite() && px > 0.0).then_some(Self(px))
    }

    #[must_use]
    pub const fn px(self) -> f32 {
        self.0
    }
}

/// Canonical frame-space clip for paint owned by a buffer viewport.
///
/// Construction stays private so transition producers cannot accidentally
/// substitute a window's outer bounds and animate window chrome.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BufferViewportRegion(Rect);

impl BufferViewportRegion {
    #[must_use]
    pub const fn bounds(self) -> Rect {
        self.0
    }
}

impl PresentedWindowRegions {
    /// Resolve the contiguous body band owned by the displayed buffer.
    ///
    /// Margins and fringes participate because their paint follows buffer
    /// rows. Scroll bars, lines, and dividers remain stable window chrome.
    #[must_use]
    pub fn buffer_viewport(self) -> Option<BufferViewportRegion> {
        let body = self.text_body;
        if !rect_is_positive_and_finite(body) {
            return None;
        }

        let mut left = body.x;
        let mut right = body.right();
        for band in [
            self.left_margin,
            self.right_margin,
            self.left_fringe,
            self.right_fringe,
        ]
        .into_iter()
        .flatten()
        {
            if !rect_is_positive_and_finite(band) || band.y != body.y || band.height != body.height
            {
                return None;
            }
            left = left.min(band.x);
            right = right.max(band.right());
        }

        Some(BufferViewportRegion(Rect::new(
            left,
            body.y,
            right - left,
            body.height,
        )))
    }
}

fn rect_is_positive_and_finite(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width > 0.0
        && rect.height > 0.0
}

/// Atomic geometry state for one window in a presentation.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
// This is a by-value protocol snapshot. Boxing `Complete` would add allocation
// and pointer chasing to every presented window and would remove `Copy`.
#[allow(clippy::large_enum_variant)]
pub enum PresentedWindowGeometry {
    Complete {
        cell_origin: PresentedCellOrigin,
        regions: PresentedWindowRegions,
    },
    Skipped {
        cell_origin: PresentedCellOrigin,
        outer: Rect,
    },
}

impl PresentedWindowGeometry {
    #[must_use]
    pub fn buffer_viewport(self) -> Option<BufferViewportRegion> {
        match self {
            Self::Complete { regions, .. } => regions.buffer_viewport(),
            Self::Skipped { .. } => None,
        }
    }
}

impl Default for PresentedWindowGeometry {
    fn default() -> Self {
        Self::Skipped {
            cell_origin: PresentedCellOrigin::default(),
            outer: Rect::default(),
        }
    }
}

/// Per-window metadata for animation transition detection
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowInfo {
    /// Window pointer as i64 (unique window identifier)
    pub window_id: DisplayWindowId,
    /// Buffer pointer as u64 (unique buffer identifier)
    pub buffer_id: u64,
    /// First visible character position (marker_position(w->start))
    pub window_start: i64,
    /// Last visible character position
    pub window_end: i64,
    /// Total buffer size in characters (BUF_Z)
    pub buffer_size: i64,
    /// The displayed buffer's modification tick, GNU's `MODIFF`.
    ///
    /// Presentation continuity uses this to decide whether scroll displacement
    /// between two presentations can be measured exactly. Two presentations of
    /// the same buffer at the same tick show the same text, so a row whose
    /// character range appears in both is genuinely the same row and the pixel
    /// difference is the scroll distance. If the tick moved, an equal-length
    /// edit could leave every character range plausible while the text beneath
    /// changed, which makes a row match wrong rather than merely ambiguous.
    /// `buffer_size` is not a substitute for the same reason.
    pub buffer_modiff: crate::presentation_origin::BufferModiff,
    /// Frame-absolute window bounds (includes mode-line)
    pub bounds: Rect,
    /// Atomically installed geometry from the same presentation.
    pub geometry: PresentedWindowGeometry,
    /// Line-number field this window's accepted body walk reserved, `None` when
    /// it reserved none. Its origin is `geometry`'s `regions.text_body.x`.
    pub line_number_field: Option<LineNumberFieldWidth>,
    /// Height of the mode-line in pixels (0 if no mode-line)
    pub mode_line_height: f32,
    /// Height of the header-line in pixels (0 if no header-line)
    pub header_line_height: f32,
    /// Height of the tab-line in pixels (0 if no tab-line)
    pub tab_line_height: f32,
    /// Whether this is the selected (active) window
    pub selected: bool,
    /// Whether this is the minibuffer window
    pub is_minibuffer: bool,
    /// Character cell height for this window (tracks text-scale-adjust)
    pub char_height: f32,
    /// Buffer name, e.g. "*scratch*" (empty string if unavailable)
    pub buffer_name: String,
    /// Buffer file name (empty string if no file)
    pub buffer_file_name: String,
    /// Whether the buffer has unsaved modifications
    pub modified: bool,
}

/// Buffer-owned paint participating in one replacement transition.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BufferTransitionTarget {
    /// Replace the content of one stable window viewport.
    Window {
        window_id: DisplayWindowId,
        region: BufferViewportRegion,
    },
    /// Replace all non-minibuffer viewports as one synchronized operation.
    Frame { regions: Vec<BufferViewportRegion> },
}

impl BufferTransitionTarget {
    #[must_use]
    pub fn regions(&self) -> &[BufferViewportRegion] {
        match self {
            Self::Window { region, .. } => std::slice::from_ref(region),
            Self::Frame { regions } => regions,
        }
    }
}

/// Semantic transition request emitted by authoritative layout producers.
///
/// The enum prevents invalid combinations such as applying a viewport scroll
/// to an aggregate frame target.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ContentTransitionHint {
    /// The presented buffer identity changed within stable viewport geometry.
    BufferReplaced {
        target: BufferTransitionTarget,
        intent: ContentTransitionIntent,
    },
}

/// Buffer collecting glyphs for current frame.
///
/// With matrix-based rendering, this buffer is cleared and rebuilt from scratch
/// each frame by the C-side matrix walker. No incremental state management needed.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FrameGlyphBuffer {
    /// Evaluator interaction snapshot paired with these exact pixels.
    pub presentation_id: crate::frame_chrome::PresentationId,
    /// Canonical frame ancestry/placement for this presentation.
    pub frame_placement: crate::PresentedFramePlacement,
    /// Why this presentation was produced.
    ///
    /// Carried here and not only on the `FrameDisplayState` it was
    /// materialized from, because the compositor holds a `FrameGlyphBuffer`:
    /// provenance that stops at materialization never reaches the only code
    /// that reads it.
    pub origin: crate::presentation_origin::PresentationOrigin,
    /// Frame dimensions
    pub width: f32,
    pub height: f32,

    /// Default character cell dimensions (from FRAME_COLUMN_WIDTH / FRAME_LINE_HEIGHT)
    pub char_width: f32,
    pub char_height: f32,
    /// Default font pixel size (from FRAME_FONT(f)->pixel_size)
    pub font_pixel_size: f32,

    /// Frame background color
    pub background: Color,

    /// Whether child-frame decorations are suppressed.
    pub undecorated: bool,
    /// Child frame border width (pixels)
    pub border_width: f32,
    /// Child frame border color
    pub border_color: Color,
    /// Child frame outer border width (pixels)
    pub outer_border_width: f32,
    /// Child frame outer border color
    pub outer_border_color: Color,
    /// Background opacity (1.0 = opaque, 0.0 = transparent)
    pub background_alpha: f32,
    /// Whether this frame should not accept keyboard focus
    pub no_accept_focus: bool,

    /// All glyphs to render this frame
    pub glyphs: Vec<FrameGlyph>,

    /// Authoritative frame-level chrome bands and interaction geometry.
    pub frame_chrome: crate::frame_chrome::FrameChrome,

    /// Validated pointer hit regions and transient paint overrides.
    presented_pointer: crate::presented_pointer::PresentedPointerMap,

    /// Presentation-qualified semantic regions and exact text positions.
    presented_hit_index: crate::presented_pointer::PresentedHitIndex,

    /// Per-window metadata for animation detection
    pub window_infos: Vec<WindowInfo>,

    /// Explicit transition requests emitted by layout producers.
    pub transition_hints: Vec<ContentTransitionHint>,

    /// Unified per-window cursors emitted by layout. Exactly one entry per
    /// window; the selected window's entry has `active: true`.
    pub window_cursors: Vec<WindowCursor>,

    /// Per-window cursor effect profiles emitted by layout.
    ///
    /// Fancy Neomacs cursor effects are an extension layered on top of GNU's
    /// `cursor-type` semantics. The key is the owning window id; renderers use
    /// this profile for that window's cursor and fall back to their global
    /// `EffectsConfig` when the window has no profile.
    pub cursor_effects_by_window: HashMap<DisplayWindowId, EffectsConfig>,

    /// Current face context (set before adding char glyphs).
    ///
    /// Only the fields still read after face-by-reference remain: the face id
    /// stamped onto each glyph, the fg/bg returned by the public getters, and
    /// the font family/size plus overstrike flag consumed when synthesizing the
    /// baseline `Face` for `face_id`. The other face attributes live only in the
    /// synthesized `Face` (resolved later via `resolved_face`).
    current_face_id: FaceId,
    current_fg: Color,
    current_bg: Option<Color>,
    current_font_family: String,
    current_font_size: f32,
    current_overstrike: bool,
    current_window_id: DisplayWindowId,
    current_row_role: GlyphRowRole,
    current_clip_rect: Option<Rect>,

    /// Full face data: face_id -> Face (includes box, underline, etc.)
    /// Rebuilt from scratch each frame by apply_face() in the layout engine.
    pub faces: HashMap<FaceId, Face>,

    /// Native catalog generation paired with `faces` and `fonts`.
    pub font_catalog_generation: crate::font::FontCatalogGeneration,

    /// Resolved font table referenced by `Face::default_resolved_font_id`
    /// (and eventually shaped glyph runs). Carried alongside `faces` so the
    /// renderer rasterizes the exact fonts layout resolved.
    pub fonts: crate::font::ResolvedFontTable,

    /// Per-character fallback fonts (`face_id → repr char → font id`); see
    /// [`crate::font::CharFontTable`].
    pub char_fonts: crate::font::CharFontTable,

    /// Shaped composed clusters (`face_id → cluster text → resolved
    /// glyphs`); see [`crate::font::ShapedClusterTable`].
    pub shaped_clusters: crate::font::ShapedClusterTable,

    /// Resolved fringe bitmaps for this frame, keyed by registry index. Embedded
    /// once per frame from the evaluator's user-bitmap registry so the renderer
    /// can expand a [`FrameGlyph::FringeBitmap`]'s `bitmap_index` to its bits.
    pub fringe_bitmaps: HashMap<u16, FringeBitmapData>,
}

/// A window's geometry is treated as unchanged within this many pixels.
///
/// Sub-pixel jitter between presentations is not a layout change; animating on
/// it would fire a transition on every rounding difference.
const GEOMETRY_STABILITY_EPSILON_PX: f32 = 2.0;

/// Text metrics are treated as unchanged within this many pixels.
const CHAR_HEIGHT_STABILITY_EPSILON_PX: f32 = 1.0;

/// Whether this window's *content identity* changed, warranting a replacement
/// transition.
///
/// This is deliberately narrow. It answers only the question the pixels cannot:
/// that the thing being shown became a different thing, so the outgoing image
/// should be replaced rather than moved. How far a viewport scrolled is a
/// measurement the compositor makes by comparing presentations, not something a
/// producer declares.
///
/// Returns `None` when the window's geometry moved, because retained pixels from
/// a different presentation extent are not compatible transition inputs.
#[must_use]
pub fn derive_buffer_replacement_hint(
    prev: &WindowInfo,
    curr: &WindowInfo,
) -> Option<ContentTransitionHint> {
    if curr.is_minibuffer {
        return None;
    }

    // The retained snapshot and new presentation must describe the same
    // buffer-owned pixels. A current-only clip can otherwise sample old chrome
    // after tab/header/mode-line or split geometry changes.
    let previous_region = prev.geometry.buffer_viewport()?;
    let current_region = curr.geometry.buffer_viewport()?;
    if previous_region != current_region {
        return None;
    }

    let bounds_moved = (prev.bounds.x - curr.bounds.x).abs() > GEOMETRY_STABILITY_EPSILON_PX
        || (prev.bounds.y - curr.bounds.y).abs() > GEOMETRY_STABILITY_EPSILON_PX
        || (prev.bounds.width - curr.bounds.width).abs() > GEOMETRY_STABILITY_EPSILON_PX
        || (prev.bounds.height - curr.bounds.height).abs() > GEOMETRY_STABILITY_EPSILON_PX;
    if bounds_moved {
        return None;
    }

    let buffer_changed =
        prev.buffer_id != 0 && curr.buffer_id != 0 && prev.buffer_id != curr.buffer_id;
    let metrics_changed =
        (prev.char_height - curr.char_height).abs() > CHAR_HEIGHT_STABILITY_EPSILON_PX;
    if !buffer_changed && !metrics_changed {
        return None;
    }

    Some(ContentTransitionHint::BufferReplaced {
        target: BufferTransitionTarget::Window {
            window_id: curr.window_id,
            region: current_region,
        },
        intent: ContentTransitionIntent::Replace,
    })
}

impl FrameGlyphBuffer {
    /// Resolve the default face's concrete font from this frame's matching
    /// font table.
    ///
    /// Keeping this lookup on the snapshot prevents consumers from repeating
    /// the face-id/table join and accidentally treating a stale font id as a
    /// usable partial binding.
    #[must_use]
    pub fn default_resolved_font(&self) -> Option<&crate::font::ResolvedFont> {
        self.faces
            .get(&BasicFaceId::Default.into())
            .and_then(|face| face.default_resolved_font_id)
            .and_then(|font_id| self.fonts.get(&font_id))
    }

    /// Borrow every font binding for this exact frame as one coherent value.
    #[must_use]
    pub fn font_bindings(&self) -> crate::font::FrameFontBindings<'_> {
        crate::font::FrameFontBindings {
            catalog_generation: self.font_catalog_generation,
            faces: &self.faces,
            fonts: &self.fonts,
            char_fonts: &self.char_fonts,
            shaped_clusters: &self.shaped_clusters,
        }
    }

    /// Copy the complete font projection from another frame snapshot.
    ///
    /// Mini-frames used for retained cursor rendering must never copy only a
    /// subset of these mutually-dependent fields.
    pub fn clone_font_bindings_from(&mut self, source: &Self) {
        self.font_catalog_generation = source.font_catalog_generation;
        self.faces.clone_from(&source.faces);
        self.fonts.clone_from(&source.fonts);
        self.char_fonts.clone_from(&source.char_fonts);
        self.shaped_clusters.clone_from(&source.shaped_clusters);
    }

    /// Resolve semantic ownership and interaction/appearance from one point.
    /// Interactive paint without semantic ownership is rejected as a producer
    /// coherence violation instead of allowing independent maps to disagree.
    pub fn resolve_presented_hit(
        &self,
        query: crate::presented_pointer::PresentedHitQuery,
    ) -> Result<
        Option<crate::presented_pointer::PresentedUnifiedHit>,
        crate::presented_pointer::PresentedHitError,
    > {
        self.presented_hit_index.resolve_unified(query)
    }

    /// Semantic hit index installed for this exact rendered presentation.
    pub fn presented_hit_index(&self) -> &crate::presented_pointer::PresentedHitIndex {
        &self.presented_hit_index
    }

    pub fn install_presented_hit_index(
        &mut self,
        mut index: crate::presented_pointer::PresentedHitIndex,
    ) -> Result<(), crate::presented_pointer::PresentedHitError> {
        if index.presentation() != self.presentation_id {
            return Err(
                crate::presented_pointer::PresentedHitError::StalePresentation {
                    expected: self.presentation_id,
                    requested: index.presentation(),
                },
            );
        }
        if !self.presented_pointer.is_empty() {
            index.bind_pointer_regions(self.presented_pointer.regions())?;
        }
        self.presented_hit_index = index;
        Ok(())
    }

    /// Validated pointer metadata installed for this exact frame snapshot.
    pub fn presented_pointer(&self) -> &crate::presented_pointer::PresentedPointerMap {
        &self.presented_pointer
    }

    /// Validate and install pointer metadata after final glyph materialization.
    pub fn install_presented_pointer(
        &mut self,
        regions: Vec<crate::presented_pointer::PresentedPointerRegion>,
        appearances: Vec<crate::presented_pointer::PresentedPointerAppearance>,
    ) -> Result<(), crate::presented_pointer::PresentedPointerMapError> {
        let map = crate::presented_pointer::PresentedPointerMap::from_parts(regions, appearances)?;
        self.install_presented_pointer_map(map)
    }

    /// Attach a transported pointer map after validating it against this frame.
    ///
    /// Deserialization establishes intrinsic map validity only; this operation
    /// atomically establishes renderer-safe contextual validity before storing.
    pub fn install_presented_pointer_map(
        &mut self,
        mut map: crate::presented_pointer::PresentedPointerMap,
    ) -> Result<(), crate::presented_pointer::PresentedPointerMapError> {
        let context =
            crate::presented_pointer::PointerMapValidationContext::from_frame_buffer(self)?;
        map.validate_against(context)?;
        map.rebuild_damage_index(self);
        let mut hit_index = self.presented_hit_index.clone();
        if !map.is_empty() && !hit_index.is_empty() {
            hit_index.bind_pointer_regions(map.regions())?;
        }
        self.presented_hit_index = hit_index;
        self.presented_pointer = map;
        Ok(())
    }

    /// Resolve source slots against this canonical primitive table and install.
    pub fn install_presented_pointer_source_map(
        &mut self,
        source: &crate::presented_pointer::PresentedPointerSourceMap,
    ) -> Result<(), crate::presented_pointer::PresentedPointerMapError> {
        let (regions, appearances) = source.resolve_against(self)?;
        self.install_presented_pointer(regions, appearances)
    }

    fn synthesize_face(
        &self,
        face_id: FaceId,
        fg: Color,
        bg: Option<Color>,
        font_family: &str,
        font_weight: u16,
        italic: bool,
        font_size: f32,
        underline: u8,
        underline_color: Option<Color>,
        strike_through: u8,
        strike_through_color: Option<Color>,
        overline: u8,
        overline_color: Option<Color>,
        _overstrike: bool,
    ) -> Face {
        let mut attrs = FaceAttributes::empty();
        if font_weight >= 700 {
            attrs |= FaceAttributes::BOLD;
        }
        if italic {
            attrs |= FaceAttributes::ITALIC;
        }
        let underline_style = UnderlineStyle::from_gnu_code(underline).unwrap_or_default();
        let has_underline = underline_style != UnderlineStyle::None;
        if has_underline {
            attrs |= FaceAttributes::UNDERLINE;
        }
        if strike_through > 0 {
            attrs |= FaceAttributes::STRIKE_THROUGH;
        }
        if overline > 0 {
            attrs |= FaceAttributes::OVERLINE;
        }

        Face {
            id: face_id,
            foreground: fg,
            background: bg.unwrap_or(Color::TRANSPARENT),
            // Built from raw colours rather than from a realized face, so there
            // is no `tty-color-desc` answer to carry.
            terminal_foreground: None,
            terminal_background: None,
            use_default_foreground: false,
            use_default_background: false,
            underline_color: has_underline.then_some(underline_color).flatten(),
            // Built from raw colours, so there is no realized terminal
            // underline colour either.
            terminal_underline_color: None,
            overline_color,
            strike_through_color,
            box_color: None,
            font_family: font_family.to_string(),
            fontset_base_family: None,
            font_size,
            font_weight,
            attributes: attrs,
            underline_style,
            box_type: BoxType::None,
            box_line_width: Default::default(),
            box_corner_radius: 0,
            box_border_style: BoxBorderStyle::Solid,
            box_border_speed: 1.0,
            box_color2: None,
            font_file_path: None,
            font_ascent: 0,
            font_descent: 0,
            underline_position: 1,
            underline_thickness: 1,
            background_gradient: None,
            lisp_name: None,
            default_resolved_font_id: None,
            stipple: None,
            underline_placement: crate::face::UnderlinePosition::default(),
        }
    }

    pub fn new() -> Self {
        Self {
            presentation_id: crate::frame_chrome::PresentationId::default(),
            frame_placement: crate::PresentedFramePlacement::default(),
            origin: crate::presentation_origin::PresentationOrigin::Ordinary,
            width: 0.0,
            height: 0.0,
            char_width: 8.0,
            char_height: 16.0,
            font_pixel_size: 14.0,
            background: Color::BLACK,
            undecorated: false,
            border_width: 0.0,
            border_color: Color::BLACK,
            outer_border_width: 0.0,
            outer_border_color: Color::BLACK,
            background_alpha: 1.0,
            no_accept_focus: false,
            glyphs: Vec::with_capacity(10000),
            frame_chrome: crate::frame_chrome::FrameChrome::default(),
            presented_pointer: crate::presented_pointer::PresentedPointerMap::empty(),
            presented_hit_index: crate::presented_pointer::PresentedHitIndex::default(),
            window_infos: Vec::with_capacity(16),
            transition_hints: Vec::with_capacity(16),
            window_cursors: Vec::with_capacity(8),
            cursor_effects_by_window: HashMap::new(),
            current_face_id: FaceId::new(0),
            current_fg: Color::WHITE,
            current_bg: None,
            current_font_family: "monospace".to_string(),
            current_font_size: 14.0,
            current_overstrike: false,
            current_window_id: DisplayWindowId::new(0),
            current_row_role: GlyphRowRole::Text,
            current_clip_rect: None,
            faces: HashMap::new(),
            font_catalog_generation: crate::font::FontCatalogGeneration::default(),
            fonts: crate::font::ResolvedFontTable::new(),
            char_fonts: crate::font::CharFontTable::new(),
            shaped_clusters: crate::font::ShapedClusterTable::new(),
            fringe_bitmaps: HashMap::new(),
        }
    }

    /// Create a new buffer with specified dimensions
    pub fn with_size(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ..Self::new()
        }
    }

    /// Clear all glyphs for a fresh full-frame rebuild.
    /// Called at the start of each frame by the matrix walker.
    pub fn clear_all(&mut self) {
        self.glyphs.clear();
        self.frame_chrome = crate::frame_chrome::FrameChrome::default();
        self.presented_pointer = crate::presented_pointer::PresentedPointerMap::empty();
        self.presented_hit_index =
            crate::presented_pointer::PresentedHitIndex::empty(self.presentation_id);
        self.window_infos.clear();
        self.transition_hints.clear();
        self.window_cursors.clear();
        self.fringe_bitmaps.clear();
        self.faces.clear();
        self.fonts.clear();
        self.char_fonts.clear();
        self.shaped_clusters.clear();
        self.current_window_id = DisplayWindowId::new(0);
        self.current_row_role = GlyphRowRole::Text;
        self.current_clip_rect = None;
    }

    fn current_slot_id(&self, x: f32, y: f32) -> DisplaySlotId {
        DisplaySlotId::from_pixels(
            self.current_window_id,
            Px(x),
            Px(y),
            Px(self.char_width),
            Px(self.char_height),
        )
    }

    /// Drain producer-emitted transition hints exactly once.
    pub fn take_transition_hints(&mut self) -> Vec<ContentTransitionHint> {
        std::mem::take(&mut self.transition_hints)
    }

    /// Set frame identity for child frame support.
    /// Called after begin_frame, before glyphs are added.
    pub fn set_frame_identity(
        &mut self,
        frame_id: DisplayFrameId,
        parent_id: DisplayFrameId,
        parent_x: f32,
        parent_y: f32,
        z_order: i32,
        undecorated: bool,
        border_width: f32,
        border_color: Color,
        no_accept_focus: bool,
        background_alpha: f32,
    ) {
        self.frame_placement = crate::PresentedFramePlacement::new(
            frame_id,
            self.presentation_id,
            (parent_id.get() != 0).then_some(parent_id),
            crate::ParentFrameRect::new(parent_x, parent_y, self.width, self.height)
                .expect("frame identity placement is valid"),
            z_order,
        );
        self.undecorated = undecorated;
        self.border_width = border_width;
        self.border_color = border_color;
        self.no_accept_focus = no_accept_focus;
        self.background_alpha = background_alpha;
    }

    /// Set current face attributes for subsequent char glyphs (with font family).
    ///
    /// This also synthesizes a baseline `Face` entry for `face_id`, so the
    /// display IR stays self-consistent even when a caller only switches the
    /// current face state and does not separately populate `faces`.
    pub fn set_face_with_font(
        &mut self,
        face_id: FaceId,
        fg: Color,
        bg: Option<Color>,
        font_family: &str,
        font_weight: u16,
        italic: bool,
        font_size: f32,
        underline: u8,
        underline_color: Option<Color>,
        strike_through: u8,
        strike_through_color: Option<Color>,
        overline: u8,
        overline_color: Option<Color>,
        overstrike: bool,
    ) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_font_family = font_family.to_string();
        self.current_font_size = font_size;
        self.current_overstrike = overstrike;
        self.faces.insert(
            face_id,
            self.synthesize_face(
                face_id,
                fg,
                bg,
                font_family,
                font_weight,
                italic,
                font_size,
                underline,
                underline_color,
                strike_through,
                strike_through_color,
                overline,
                overline_color,
                overstrike,
            ),
        );
    }

    /// Set current face attributes for subsequent char glyphs.
    ///
    /// Uses the current font family and size when synthesizing the baseline
    /// `Face` entry for `face_id`.
    pub fn set_face(
        &mut self,
        face_id: FaceId,
        fg: Color,
        bg: Option<Color>,
        font_weight: u16,
        italic: bool,
        underline: u8,
        underline_color: Option<Color>,
        strike_through: u8,
        strike_through_color: Option<Color>,
        overline: u8,
        overline_color: Option<Color>,
    ) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.faces.insert(
            face_id,
            self.synthesize_face(
                face_id,
                fg,
                bg,
                &self.current_font_family,
                font_weight,
                italic,
                self.current_font_size,
                underline,
                underline_color,
                strike_through,
                strike_through_color,
                overline,
                overline_color,
                self.current_overstrike,
            ),
        );
    }

    /// Set authoritative layout draw context for subsequent glyph emissions.
    pub fn set_draw_context(
        &mut self,
        window_id: DisplayWindowId,
        row_role: GlyphRowRole,
        clip_rect: Option<Rect>,
    ) {
        self.current_window_id = window_id;
        self.current_row_role = row_role;
        self.current_clip_rect = clip_rect;
    }

    /// Get font family for a face_id
    pub fn get_face_font(&self, face_id: FaceId) -> &str {
        self.faces
            .get(&face_id)
            .map(|f| f.font_family.as_str())
            .unwrap_or("monospace")
    }

    /// Return the frame-local face entry used to render `face_id`.
    ///
    /// Face ids are scoped to the owning frame buffer. Parent and child frames
    /// may legally reuse the same numeric id for different face data.
    pub fn render_face(&self, face_id: FaceId) -> Option<&Face> {
        self.faces.get(&face_id)
    }

    /// Resolve the face-derived visual attributes for `face_id`.
    ///
    /// Returns exactly what `materialize` previously inlined onto every
    /// [`FrameGlyph::Char`]: foreground/background colors, font weight/size,
    /// italic, and the decoration codes/colors. The body is identical to the
    /// layout-side `FrameDisplayState::resolve_face_for_materialize`, but reads
    /// from this buffer's own `faces`, `background`, and `font_pixel_size`. The
    /// not-found fallback matches materialization: white fg, frame background,
    /// font weight 400, the frame font pixel size, and no decorations.
    pub fn resolved_face(&self, face_id: FaceId) -> MaterializedFaceData {
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

    /// Get current font family
    pub fn get_current_font_family(&self) -> &str {
        &self.current_font_family
    }

    /// Get current foreground color
    pub fn get_current_fg(&self) -> Color {
        self.current_fg
    }

    /// Get current face background color (for stretch glyphs)
    pub fn get_current_bg(&self) -> Option<Color> {
        self.current_bg
    }

    /// Temporarily set fg/bg colors for margin rendering.
    pub fn set_colors(&mut self, fg: Color, bg: Option<Color>) {
        self.current_fg = fg;
        self.current_bg = bg;
    }

    /// Add a window background rectangle.
    /// With full-frame rebuild, no stale-background removal is needed.
    pub fn add_background(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.glyphs.push(FrameGlyph::Background {
            bounds: Rect::new(x, y, width, height),
            color,
        });
    }

    /// Add a character glyph. No overlap removal needed with full-frame rebuild.
    pub fn add_char(
        &mut self,
        char: char,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        ascent: f32,
        _overlay_hint: bool,
    ) {
        self.glyphs.push(FrameGlyph::Char {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            slot_id: self.current_slot_id(x, y),
            bidi_level: 0,
            char,
            composed: None,
            x,
            y,
            baseline: y + ascent,
            width,
            height,
            ascent,
            face_id: self.current_face_id,
            box_vertical_edges: BoxVerticalEdges::Both,
        });
    }

    /// Add a composed (multi-codepoint) character glyph.
    /// Used for grapheme clusters like emoji ZWJ sequences, combining diacritics.
    pub fn add_composed_char(
        &mut self,
        text: &str,
        base_char: char,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        ascent: f32,
        _overlay_hint: bool,
    ) {
        self.glyphs.push(FrameGlyph::Char {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            slot_id: self.current_slot_id(x, y),
            bidi_level: 0,
            char: base_char,
            composed: Some(text.into()),
            x,
            y,
            baseline: y + ascent,
            width,
            height,
            ascent,
            face_id: self.current_face_id,
            box_vertical_edges: BoxVerticalEdges::Both,
        });
    }

    /// Get current font size
    pub fn font_size(&self) -> f32 {
        self.current_font_size
    }

    /// Set current font size (for display property height scaling)
    pub fn set_font_size(&mut self, size: f32) {
        self.current_font_size = size;
    }

    /// Add a stretch (whitespace) glyph. No overlap removal needed.
    pub fn add_stretch(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        bg: Color,
        face_id: FaceId,
        _overlay_hint: bool,
    ) {
        self.add_stretch_with_box_vertical_edges(
            x,
            y,
            width,
            height,
            bg,
            face_id,
            BoxVerticalEdges::Both,
            _overlay_hint,
        );
    }

    /// Add a stretch with explicit GNU box-run terminal-side ownership.
    #[allow(clippy::too_many_arguments)]
    pub fn add_stretch_with_box_vertical_edges(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        bg: Color,
        face_id: FaceId,
        box_vertical_edges: BoxVerticalEdges,
        _overlay_hint: bool,
    ) {
        self.glyphs.push(FrameGlyph::Stretch {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            slot_id: self.current_slot_id(x, y),
            bidi_level: 0,
            x,
            y,
            width,
            height,
            bg,
            face_id,
            box_vertical_edges,
        });
    }

    /// Add an image glyph
    pub fn add_image(&mut self, image_id: ImageId, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Image {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            slot_id: Some(self.current_slot_id(x, y)),
            image_id,
            source_rect: crate::image::ImageSourceRect::FULL,
            slot_rect: Rect::new(x, y, width, height),
            box_rect: Rect::new(x, y, width, height),
            x,
            y,
            width,
            height,
            face_id: self.current_face_id,
            box_vertical_edges: BoxVerticalEdges::Both,
        });
    }

    /// Add a video glyph
    pub fn add_video(&mut self, video_id: VideoId, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Video {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            slot_id: Some(self.current_slot_id(x, y)),
            video_id,
            x,
            y,
            width,
            height,
            opacity: 1.0,
            face_id: self.current_face_id,
            box_vertical_edges: BoxVerticalEdges::Both,
        });
    }

    /// Add a shader-surface glyph.
    pub fn add_surface(&mut self, surface_id: SurfaceId, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Surface {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            slot_id: Some(self.current_slot_id(x, y)),
            surface_id,
            x,
            y,
            width,
            height,
            face_id: self.current_face_id,
            box_vertical_edges: BoxVerticalEdges::Both,
        });
    }

    /// Add an xwidget glyph whose slot is `slot_width` wide: the widget's
    /// `content` width when nothing cropped it, less when the right edge did.
    pub fn add_xwidget(
        &mut self,
        xwidget_id: XwidgetId,
        webview_id: WebViewId,
        presentation: XwidgetPresentationGeometry<FrameSpace>,
    ) {
        let clip = self.current_clip_rect.map(|clip| {
            GeometryRect::<FrameSpace, LogicalPixels>::new(clip.x, clip.y, clip.width, clip.height)
                .expect("a frame draw-context clip has valid geometry")
        });
        let presentation = presentation.with_clip(clip);
        let origin = presentation.origin();
        self.glyphs.push(FrameGlyph::Xwidget {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            slot_id: Some(self.current_slot_id(origin.x(), origin.y())),
            xwidget_id,
            webview_id,
            presentation,
            face_id: self.current_face_id,
            box_vertical_edges: BoxVerticalEdges::Both,
        });
    }

    /// Register a resolved fringe bitmap for this frame, keyed by registry
    /// index. Idempotent: the same index may be registered once per row that
    /// references it; we keep a single copy.
    pub fn register_fringe_bitmap(&mut self, bitmap_index: u16, data: FringeBitmapData) {
        self.fringe_bitmaps.entry(bitmap_index).or_insert(data);
    }

    /// Emit a fringe-bitmap glyph for the current window/row context. The bits
    /// must already be registered via [`Self::register_fringe_bitmap`].
    #[allow(clippy::too_many_arguments)]
    pub fn push_fringe_bitmap(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        bitmap_index: u16,
        face_id: FaceId,
        side: FringeSide,
    ) {
        self.glyphs.push(FrameGlyph::FringeBitmap {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            x,
            y,
            width,
            height,
            bitmap_index,
            face_id,
            side,
        });
    }

    /// Add cursor
    pub fn add_cursor(
        &mut self,
        window_id: DisplayWindowId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        style: CursorStyle,
        color: Color,
    ) {
        self.window_cursors.push(WindowCursor {
            window_id,
            slot_id: DisplaySlotId::from_pixels(
                window_id,
                Px(x),
                Px(y),
                Px(self.char_width),
                Px(self.char_height),
            ),
            x,
            y,
            width,
            height,
            style,
            color,
            cursor_fg: Color::BLACK,
            ascent: 0.0,
            active: false,
        });
    }

    /// Return the active (selected window's) cursor, if any.
    pub fn active_cursor(&self) -> Option<&WindowCursor> {
        self.window_cursors.iter().find(|c| c.active)
    }

    /// Image assets whose lifetime is extended by this immutable frame.
    pub fn referenced_images(&self) -> impl Iterator<Item = ImageId> + '_ {
        self.glyphs.iter().filter_map(|glyph| match glyph {
            FrameGlyph::Image { image_id, .. } => Some(*image_id),
            _ => None,
        })
    }

    /// Return a mutable reference to the active (selected window's) cursor.
    pub fn active_cursor_mut(&mut self) -> Option<&mut WindowCursor> {
        self.window_cursors.iter_mut().find(|c| c.active)
    }

    /// Set the cursor effect profile for one window.
    pub fn set_window_cursor_effects(
        &mut self,
        window_id: DisplayWindowId,
        effects: EffectsConfig,
    ) {
        self.cursor_effects_by_window.insert(window_id, effects);
    }

    /// Return the cursor effect profile for one window, if layout supplied one.
    pub fn window_cursor_effects(&self, window_id: DisplayWindowId) -> Option<&EffectsConfig> {
        self.cursor_effects_by_window.get(&window_id)
    }

    /// Resolve one window cursor's effect profile using the renderer-wide
    /// profile when layout did not supply a local override.
    pub fn effective_window_cursor_effects<'a>(
        &'a self,
        window_id: DisplayWindowId,
        fallback: &'a EffectsConfig,
    ) -> &'a EffectsConfig {
        self.window_cursor_effects(window_id).unwrap_or(fallback)
    }

    /// Return the active physical cursor's effect profile, if any.
    pub fn phys_cursor_effects(&self) -> Option<&EffectsConfig> {
        self.active_cursor()
            .and_then(|cursor| self.window_cursor_effects(cursor.window_id))
    }

    /// Resolve the effect profile used to render and schedule the active
    /// physical cursor. A selected window's profile overrides the global
    /// renderer profile; frames without one inherit the supplied fallback.
    pub fn effective_phys_cursor_effects<'a>(
        &'a self,
        fallback: &'a EffectsConfig,
    ) -> &'a EffectsConfig {
        self.active_cursor().map_or(fallback, |cursor| {
            self.effective_window_cursor_effects(cursor.window_id, fallback)
        })
    }

    /// Add per-window metadata for animation detection
    pub fn add_window_info(
        &mut self,
        window_id: DisplayWindowId,
        buffer_id: u64,
        window_start: i64,
        window_end: i64,
        buffer_size: i64,
        buffer_modiff: crate::presentation_origin::BufferModiff,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        mode_line_height: f32,
        header_line_height: f32,
        tab_line_height: f32,
        selected: bool,
        is_minibuffer: bool,
        char_height: f32,
        buffer_name: String,
        buffer_file_name: String,
        modified: bool,
    ) {
        self.window_infos.push(WindowInfo {
            window_id,
            buffer_id,
            window_start,
            window_end,
            buffer_size,
            buffer_modiff,
            bounds: Rect::new(x, y, width, height),
            geometry: PresentedWindowGeometry::Skipped {
                cell_origin: PresentedCellOrigin::default(),
                outer: Rect::new(x, y, width, height),
            },
            // This constructor assembles a window by hand and measures no rows,
            // so it has no line-number field to report.
            line_number_field: None,
            mode_line_height,
            header_line_height,
            tab_line_height,
            selected,
            is_minibuffer,
            char_height,
            buffer_name,
            buffer_file_name,
            modified,
        });
    }

    /// Add an explicit transition hint.
    pub fn add_transition_hint(&mut self, hint: ContentTransitionHint) {
        self.transition_hints.push(hint);
    }

    /// Set the authoritative physical cursor for the frame.
    pub fn set_phys_cursor(&mut self, mut cursor: PhysCursor) {
        if let Some(
            FrameGlyph::Image { .. }
            | FrameGlyph::Video { .. }
            | FrameGlyph::Xwidget { .. }
            | FrameGlyph::Surface { .. },
        ) = self.slot_glyph(cursor.slot_id)
        {
            cursor.style = CursorStyle::Hollow;
        }
        let (x, y, width, height) = self.cursor_draw_rect(
            cursor.slot_id,
            cursor.style,
            cursor.ascent,
            (cursor.x, cursor.y, cursor.width, cursor.height),
        );
        cursor.x = x;
        cursor.y = y;
        cursor.width = width;
        cursor.height = height;
        cursor.ascent = cursor.ascent.min(height).max(1.0);
        let entry = WindowCursor {
            window_id: cursor.window_id,
            slot_id: cursor.slot_id,
            x: cursor.x,
            y: cursor.y,
            width: cursor.width,
            height: cursor.height,
            style: cursor.style,
            color: cursor.color,
            cursor_fg: cursor.cursor_fg,
            ascent: cursor.ascent,
            active: true,
        };
        // Exactly one active entry per window: replace an existing active entry
        // for this window rather than duplicating it.
        if let Some(existing) = self
            .window_cursors
            .iter_mut()
            .find(|c| c.active && c.window_id == entry.window_id)
        {
            *existing = entry;
        } else {
            self.window_cursors.push(entry);
        }
    }

    /// Look up the text or stretch glyph occupying a given display slot.
    pub fn slot_glyph(&self, slot_id: DisplaySlotId) -> Option<&FrameGlyph> {
        self.glyphs
            .iter()
            .find(|glyph| glyph.slot_id().is_some_and(|slot| slot == slot_id))
    }

    /// The pixel x where a glyph in `slot_id`'s column would begin: the right
    /// edge of the nearest glyph in the same window+row with a smaller column.
    /// Used to place a cursor that occupies an empty slot (a blank line or the
    /// end of a line) flush with the text column -- where the next glyph would
    /// start -- instead of at a grid-approximate `col * average_width` x. This
    /// mirrors GNU's end-of-line cursor, which sits at `row->x` plus the row's
    /// used pixel widths. Returns None when nothing precedes the slot (e.g. an
    /// empty line with no line-number gutter), leaving the caller's fallback x.
    fn row_pen_x_before(&self, slot_id: DisplaySlotId) -> Option<f32> {
        self.glyphs
            .iter()
            .filter_map(|glyph| {
                let slot = glyph.slot_id()?;
                if slot.window_id == slot_id.window_id
                    && slot.row == slot_id.row
                    && slot.col < slot_id.col
                {
                    let (gx, _gy, gw, _gh) = glyph.cell_rect()?;
                    Some((slot.col, gx + gw))
                } else {
                    None
                }
            })
            .max_by_key(|(col, _)| *col)
            .map(|(_, right_edge)| right_edge)
    }

    /// THE single source of truth for where a cursor is drawn: the pixel cell
    /// rect `(x, y, width, height)` of the glyph occupying `slot_id`. When no
    /// glyph occupies the slot (e.g. an empty line) the layout-supplied
    /// `fallback` rect is returned. All cursor consumers -- the static draw, the
    /// window-cursor draw, the slide-animation target, and effects -- resolve
    /// their position through here so they cannot drift apart (the cursor's
    /// grid-derived `PhysCursor`/`CursorItem` geometry is only an approximation
    /// that diverges from the glyph under scaled fonts). Style- and bidi-aware
    /// shaping (bar width, RTL alignment) is layered on top by the renderer.
    pub fn cursor_cell_rect(
        &self,
        slot_id: DisplaySlotId,
        fallback: (f32, f32, f32, f32),
    ) -> (f32, f32, f32, f32) {
        self.slot_glyph(slot_id)
            .and_then(FrameGlyph::cell_rect)
            .unwrap_or(fallback)
    }

    /// The full pixel rect `(x, y, width, height)` at which a cursor on
    /// `slot_id` with the given `style` is drawn. Resolves the glyph occupying
    /// the slot (its cell `x`, and full rect for media glyphs), keeping the
    /// layout's row `y`/`height`, applying the box/bar/stretch width policy, and
    /// shifting bar/hollow cursors to the right edge on RTL glyphs. `fallback`
    /// (the layout's grid-derived geometry) is used when no glyph occupies the
    /// slot, e.g. an empty line.
    ///
    /// This is THE single cursor-placement computation: the static draw, the
    /// per-window cursors, and the slide-animation target all call it, so they
    /// cannot drift apart (each independent reconstruction previously produced a
    /// distinct cursor bug under line numbers, hidden text, or scaled fonts).
    pub fn cursor_draw_rect(
        &self,
        slot_id: DisplaySlotId,
        style: CursorStyle,
        ascent: f32,
        fallback: (f32, f32, f32, f32),
    ) -> (f32, f32, f32, f32) {
        let mut x = fallback.0;
        // Position is derived from the matrix glyph at draw, like GNU draws the
        // cursor over the glyph at (vpos, hpos): x = the glyph cell's x, and the
        // cursor top y = the glyph baseline minus the cursor ascent (which
        // already encodes the tall-glyph baseline shift). For a slot with no
        // Char glyph (stretch, image, empty line) the layout fallback stands in.
        let mut y = fallback.1;
        let mut width = fallback.2.max(1.0);
        let height = fallback.3.max(1.0);

        if let Some(slot) = self.slot_glyph(slot_id) {
            match slot {
                FrameGlyph::Char {
                    x: slot_x,
                    baseline,
                    width: slot_width,
                    ..
                } => {
                    x = *slot_x;
                    y = *baseline - ascent;
                    if !matches!(style, CursorStyle::Bar(_)) {
                        width = slot_width.max(1.0);
                    }
                }
                FrameGlyph::Stretch { .. } => {
                    // Layout owns the cursor geometry for stretch slots.  A
                    // stretch can be a row-wide face extension whose origin is
                    // the window edge, not the text position represented by the
                    // slot, so it must not replace the layout-supplied x.  The
                    // stretch-cursor width policy is likewise already resolved
                    // into the fallback width by layout.
                }
                FrameGlyph::Image { slot_rect, .. } => {
                    return (
                        slot_rect.x,
                        slot_rect.y,
                        slot_rect.width.max(1.0),
                        slot_rect.height.max(1.0),
                    );
                }
                FrameGlyph::Video {
                    x: slot_x,
                    y: slot_y,
                    width: slot_width,
                    height: slot_height,
                    ..
                }
                | FrameGlyph::Surface {
                    x: slot_x,
                    y: slot_y,
                    width: slot_width,
                    height: slot_height,
                    ..
                } => {
                    return (*slot_x, *slot_y, slot_width.max(1.0), slot_height.max(1.0));
                }
                FrameGlyph::Xwidget { presentation, .. } => {
                    let slot = presentation.layout_slot_rect();
                    return (
                        slot.x(),
                        slot.y(),
                        slot.width().max(1.0),
                        slot.height().max(1.0),
                    );
                }
                _ => {}
            }
        } else if let Some(end_x) = self.row_pen_x_before(slot_id) {
            // No glyph occupies the slot (a blank line or the end of a line).
            // Snap to the right edge of the nearest preceding glyph in the row
            // -- where the next glyph would start -- rather than the layout's
            // grid-approximate fallback x, so the cursor is flush with the text
            // column instead of landing a few pixels into the line-number
            // gutter. GNU's end-of-line cursor likewise sits at the row's used
            // pixel width.
            x = end_x;
        }

        if matches!(
            style,
            CursorStyle::Bar(_) | CursorStyle::Hbar(_) | CursorStyle::Hollow
        ) && let Some(slot) = self.slot_glyph(slot_id)
            && slot.bidi_level().is_some_and(|level| level & 1 != 0)
        {
            let slot_width = match slot {
                FrameGlyph::Char { width, .. } | FrameGlyph::Stretch { width, .. } => *width,
                _ => width,
            };
            if slot_width > width {
                x += slot_width - width;
            }
        }

        (x, y, width, height)
    }

    /// Add border
    pub fn add_border(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.glyphs.push(FrameGlyph::Border {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
            x,
            y,
            width,
            height,
            color,
        });
    }

    /// Add a scroll bar glyph (GPU-rendered)
    pub fn add_scroll_bar(
        &mut self,
        horizontal: bool,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        position: i64,
        portion: i64,
        whole: i64,
        thumb_start: f32,
        thumb_size: f32,
        track_color: Color,
        thumb_color: Color,
    ) {
        self.glyphs.push(FrameGlyph::ScrollBar {
            window_id: self.current_window_id,
            row_role: self.current_row_role,
            clip_rect: self.current_clip_rect,
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
        });
    }

    /// Add terminal glyph (inline or window mode)
    #[cfg(feature = "neo-term")]
    pub fn add_terminal(&mut self, terminal_id: u32, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Terminal {
            terminal_id,
            x,
            y,
            width,
            height,
        });
    }

    /// Get glyph count
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

#[cfg(test)]
#[path = "frame_glyphs_test.rs"]
mod tests;
