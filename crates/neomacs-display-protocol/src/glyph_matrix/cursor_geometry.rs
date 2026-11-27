//! Cursor placement telemetry for display bug reports.
//!
//! A "the cursor is drawn on the wrong row" report needs three facts that no
//! log carried before (#855): where layout put the cursor, where the renderer
//! draws it, and the row and font metrics that connect the two. Layout keeps
//! its rectangle in [`FrameDisplayState::phys_cursor`]. The renderer draws the
//! rectangle that [`FrameGlyphBuffer::cursor_draw_rect`] derives from the
//! glyph occupying the cursor slot (`baseline - ascent` for a character). A
//! slot that resolves to a glyph on another row moves the drawn cursor while
//! layout's rectangle stays right, so the record carries both.
//!
//! The row top uses the materializer's own rule ([`MaterializedRowCell`]), so
//! `drawn_in_row` compares the drawn rectangle with the band the row is
//! painted in, not with a second reconstruction of it.
//!
//! The record is emitted on [`TARGET`] at DEBUG level, so the bug-report
//! command line (`RUST_LOG=debug`) captures it, and
//! `RUST_LOG=warn,neomacs::cursor_geometry=debug` captures only it. The
//! layout engine logs the same target with `stage=layout` (window start,
//! hscroll, vscroll and the window's layout class); both lines name the
//! presentation they describe.

use std::fmt;

use super::{FrameDisplayState, MaterializedRowCell, RowDamage};
use crate::frame_glyphs::{CursorStyle, DisplaySlotId, FrameGlyph, FrameGlyphBuffer, GlyphRowRole};
use crate::types::{DisplayWindowId, FaceId, Rect};

/// The tracing target shared by the layout and draw stages.
pub const TARGET: &str = "neomacs::cursor_geometry";

/// Layout's cursor rectangle, before the draw-time slot resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutCursorRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
}

/// The matrix row layout assigned the cursor to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorRowGeometry {
    pub role: GlyphRowRole,
    /// Frame-absolute top of the painted row band.
    pub top: f32,
    /// Height of the painted row band.
    pub height: f32,
    pub pixel_y: f32,
    pub ascent: f32,
    pub line_spacing: f32,
    pub start_charpos: usize,
    pub end_charpos: usize,
    pub cursor_col: Option<u16>,
    pub damage: RowDamage,
}

/// The glyph the renderer resolves the cursor slot to.
#[derive(Clone, Debug, PartialEq)]
pub enum CursorSlotGlyph {
    Char {
        ch: char,
        x: f32,
        y: f32,
        baseline: f32,
        width: f32,
        height: f32,
        ascent: f32,
        face_id: FaceId,
    },
    Stretch {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        face_id: FaceId,
    },
    /// Images, video, surfaces and widgets draw the cursor over their slot.
    Media { kind: &'static str },
    /// Any other slot-owning glyph.
    Other,
    /// No glyph occupies the slot; layout's rectangle stands in.
    Missing,
}

impl CursorSlotGlyph {
    fn of(glyph: Option<&FrameGlyph>) -> Self {
        match glyph {
            None => Self::Missing,
            Some(FrameGlyph::Char {
                char: ch,
                x,
                y,
                baseline,
                width,
                height,
                ascent,
                face_id,
                ..
            }) => Self::Char {
                ch: *ch,
                x: *x,
                y: *y,
                baseline: *baseline,
                width: *width,
                height: *height,
                ascent: *ascent,
                face_id: *face_id,
            },
            Some(FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                face_id,
                ..
            }) => Self::Stretch {
                x: *x,
                y: *y,
                width: *width,
                height: *height,
                face_id: *face_id,
            },
            Some(FrameGlyph::Image { .. }) => Self::Media { kind: "image" },
            Some(FrameGlyph::Video { .. }) => Self::Media { kind: "video" },
            Some(FrameGlyph::Surface { .. }) => Self::Media { kind: "surface" },
            Some(FrameGlyph::Xwidget { .. }) => Self::Media { kind: "xwidget" },
            Some(_) => Self::Other,
        }
    }

    fn face_id(&self) -> Option<FaceId> {
        match self {
            Self::Char { face_id, .. } | Self::Stretch { face_id, .. } => Some(*face_id),
            Self::Media { .. } | Self::Other | Self::Missing => None,
        }
    }
}

/// Realized font metrics of the face on the cursor slot.
#[derive(Clone, Debug, PartialEq)]
pub struct CursorFaceMetrics {
    pub face_id: FaceId,
    pub name: Option<std::sync::Arc<str>>,
    pub family: std::sync::Arc<str>,
    pub size: f32,
    pub ascent: i32,
    pub descent: i32,
}

/// One presented frame's cursor, from layout's rectangle to the drawn one.
#[derive(Clone, Debug, PartialEq)]
pub struct CursorDrawGeometry {
    pub presentation: u64,
    pub window: DisplayWindowId,
    pub charpos: usize,
    pub matrix_row: usize,
    pub col: u16,
    pub slot: DisplaySlotId,
    pub style: CursorStyle,
    pub layout: LayoutCursorRect,
    /// What the renderer draws; `None` when the materialized frame has no
    /// active cursor.
    pub drawn: Option<Rect>,
    pub text_area: Option<Rect>,
    pub row: Option<CursorRowGeometry>,
    pub slot_glyph: CursorSlotGlyph,
    pub face: Option<CursorFaceMetrics>,
    pub frame_char_width: f32,
    pub frame_char_height: f32,
}

impl CursorDrawGeometry {
    /// Capture the active cursor of STATE as FRAME (its materialization)
    /// draws it. `None` when layout published no active cursor.
    pub fn capture(state: &FrameDisplayState, frame: &FrameGlyphBuffer) -> Option<Self> {
        let cursor = state.phys_cursor.as_ref()?;
        let entry = state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id == cursor.window_id);
        let row = entry.and_then(|entry| {
            let glyph_row = entry.matrix.rows.get(cursor.row)?;
            let (row_bounds, _, _) = state.window_row_materialization_inputs(entry, glyph_row);
            let cell = MaterializedRowCell::new(
                row_bounds.y,
                cursor.row as u32,
                glyph_row,
                state.char_height,
            );
            Some(CursorRowGeometry {
                role: glyph_row.role,
                top: cell.y,
                height: cell.height,
                pixel_y: glyph_row.pixel_y,
                ascent: glyph_row.ascent_px,
                line_spacing: glyph_row.line_spacing_px,
                start_charpos: glyph_row.start_charpos,
                end_charpos: glyph_row.end_charpos,
                cursor_col: glyph_row.cursor_col,
                damage: entry.matrix.row_damage(cursor.row),
            })
        });
        let drawn = frame.active_cursor().map(|active| {
            let (x, y, width, height) = frame.cursor_draw_rect(
                active.slot_id,
                active.style,
                active.ascent,
                (active.x, active.y, active.width, active.height),
            );
            Rect::new(x, y, width, height)
        });
        let slot_glyph = CursorSlotGlyph::of(frame.slot_glyph(cursor.slot_id));
        let face = slot_glyph
            .face_id()
            .and_then(|face_id| state.faces.get(&face_id))
            .map(|face| CursorFaceMetrics {
                face_id: face.id,
                name: face.lisp_name.clone(),
                family: face.font_family.clone(),
                size: face.font_size,
                ascent: face.font_ascent,
                descent: face.font_descent,
            });
        Some(Self {
            presentation: state.presentation_id.get(),
            window: cursor.window_id,
            charpos: cursor.charpos,
            matrix_row: cursor.row,
            col: cursor.col,
            slot: cursor.slot_id,
            style: cursor.style,
            layout: LayoutCursorRect {
                x: cursor.x,
                y: cursor.y,
                width: cursor.width,
                height: cursor.height,
                ascent: cursor.ascent,
            },
            drawn,
            text_area: entry.map(|entry| entry.text_pixel_bounds),
            row,
            slot_glyph,
            face,
            frame_char_width: state.char_width,
            frame_char_height: state.char_height,
        })
    }

    /// Whether the drawn cursor's top lies inside its row's painted band.
    /// `None` when either side is unknown.
    pub fn drawn_in_row(&self) -> Option<bool> {
        let drawn = self.drawn?;
        let row = self.row?;
        Some(drawn.y + 0.5 >= row.top && drawn.y < row.top + row.height - 0.5)
    }
}

struct RectText(Rect);

impl fmt::Display for RectText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = self.0;
        write!(f, "({:.1},{:.1} {:.1}x{:.1})", r.x, r.y, r.width, r.height)
    }
}

impl fmt::Display for CursorDrawGeometry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let l = self.layout;
        write!(
            f,
            "stage=draw presentation={} window={} charpos={} style={:?} \
             layout=(row={} col={} slot={},{} x={:.1} y={:.1} w={:.1} h={:.1} ascent={:.1})",
            self.presentation,
            self.window.get(),
            self.charpos,
            self.style,
            self.matrix_row,
            self.col,
            self.slot.row,
            self.slot.col,
            l.x,
            l.y,
            l.width,
            l.height,
            l.ascent,
        )?;
        match self.drawn {
            Some(rect) => write!(f, " drawn={}", RectText(rect))?,
            None => f.write_str(" drawn=none")?,
        }
        match self.drawn_in_row() {
            Some(inside) => write!(f, " drawn_in_row={inside}")?,
            None => f.write_str(" drawn_in_row=unknown")?,
        }
        match self.text_area {
            Some(rect) => write!(f, " text_area={}", RectText(rect))?,
            None => f.write_str(" text_area=none")?,
        }
        match self.row {
            Some(row) => write!(
                f,
                " row=(role={:?} top={:.1} h={:.1} pixel_y={:.1} ascent={:.1} spacing={:.1} \
                 chars={}..{} cursor_col={} damage={:?})",
                row.role,
                row.top,
                row.height,
                row.pixel_y,
                row.ascent,
                row.line_spacing,
                row.start_charpos,
                row.end_charpos,
                row.cursor_col
                    .map_or_else(|| "none".to_owned(), |col| col.to_string()),
                row.damage,
            )?,
            None => f.write_str(" row=missing")?,
        }
        match &self.slot_glyph {
            CursorSlotGlyph::Char {
                ch,
                x,
                y,
                baseline,
                width,
                height,
                ascent,
                face_id,
            } => write!(
                f,
                " glyph=char({:?} x={:.1} y={:.1} baseline={:.1} w={:.1} h={:.1} ascent={:.1} face={})",
                ch,
                x,
                y,
                baseline,
                width,
                height,
                ascent,
                face_id.get(),
            )?,
            CursorSlotGlyph::Stretch {
                x,
                y,
                width,
                height,
                face_id,
            } => write!(
                f,
                " glyph=stretch(x={:.1} y={:.1} w={:.1} h={:.1} face={})",
                x,
                y,
                width,
                height,
                face_id.get(),
            )?,
            CursorSlotGlyph::Media { kind } => write!(f, " glyph={kind}")?,
            CursorSlotGlyph::Other => f.write_str(" glyph=other")?,
            CursorSlotGlyph::Missing => f.write_str(" glyph=missing")?,
        }
        match &self.face {
            Some(face) => write!(
                f,
                " face=(id={} name={} family={:?} size={:.1} ascent={} descent={})",
                face.face_id.get(),
                face.name.as_deref().unwrap_or("none"),
                &*face.family,
                face.size,
                face.ascent,
                face.descent,
            )?,
            None => f.write_str(" face=none")?,
        }
        write!(
            f,
            " frame_char={:.1}x{:.1}",
            self.frame_char_width, self.frame_char_height
        )
    }
}

#[cfg(test)]
#[path = "tests/cursor_geometry_test.rs"]
mod tests;
