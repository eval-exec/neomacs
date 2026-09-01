//! Plain-text renderings of [`FrameDisplayState`] for the frame snapshot
//! (`neomacs--frame-snapshot`).
//!
//! Agents read these the way they read a tmux capture: a greppable logical
//! grid (`render_text`) plus a face-annotated variant (`render_text_faces`)
//! that lists each row's face runs with names and resolved hex colors. The
//! JSON rendering of the snapshot is plain serde on the struct itself; these
//! are the human/agent convenience views of the SAME state, so they cannot
//! drift from what the renderer draws.
//!
//! Format (frozen — golden tests in `snapshot_text_test.rs` encode it):
//!
//! ```text
//! === frame 1: 80x24 cols 640x384 px ===
//! [menu-bar]|File Edit Options
//! [chrome 0]|tab1 tab2
//! -- window 1 "*scratch*" bounds=(0,0 640x368)px text=(8,0 632x352)px start=1 end=12 selected --
//!    0|;; hello
//!     : run 0-8 default fg=#FFFFFF bg=#000000
//! [mode-line]|-UUU:----F1  *scratch*
//! [cursor] window=1 row=0 col=3 charpos=4 style=FilledBox
//! ```
//!
//! Row text is logical (one line per glyph row, trailing spaces trimmed);
//! with proportional fonts it is row/column oriented, not pixel-aligned —
//! pixel geometry is authoritative in the JSON. A double-width character
//! counts as ONE char in face-run ranges (its padding glyph is skipped).
//! Face-run ranges are half-open `[start, end)` over the printed chars.

use std::fmt::Write as _;

use crate::face::BasicFaceId;
use crate::frame_chrome::FrameChromeContent;
use crate::frame_glyphs::GlyphRowRole;
use crate::glyph_matrix::{FrameDisplayState, GlyphRow, GlyphType};
use crate::types::{Color, FaceId};

/// One run of consecutive printed chars sharing a face id.
struct FaceRun {
    start: usize,
    end: usize,
    face_id: FaceId,
}

impl FrameDisplayState {
    /// Render the frame as a greppable logical text grid.
    pub fn render_text(&self) -> String {
        self.render_snapshot_text(false)
    }

    /// Like [`render_text`](Self::render_text), plus one `: run ...` line per
    /// face run under each row, carrying the face name and resolved colors.
    pub fn render_text_faces(&self) -> String {
        self.render_snapshot_text(true)
    }

    fn render_snapshot_text(&self, with_faces: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "=== frame {}: {}x{} cols {}x{} px ===",
            self.frame_placement.frame(),
            self.frame_cols,
            self.frame_rows,
            self.frame_pixel_width,
            self.frame_pixel_height
        );

        for band in self.frame_chrome.bands() {
            match band.content() {
                FrameChromeContent::MenuBar(menu) => {
                    let labels: Vec<&str> = menu
                        .items()
                        .iter()
                        .map(|item| item.item().label.as_str())
                        .collect();
                    let _ = writeln!(out, "[menu-bar]|{}", labels.join(" "));
                }
                FrameChromeContent::DisplayRow(content) => {
                    let (text, runs) = row_text(content.row());
                    let row_index = (band.bounds().y() / self.char_height.max(1.0)).round() as u32;
                    let _ = writeln!(out, "[chrome {}]|{}", row_index, text);
                    if with_faces {
                        self.write_face_runs(&mut out, &runs);
                    }
                }
                FrameChromeContent::ToolBar(_) | FrameChromeContent::CompactBar(_) => {}
            }
        }

        for entry in &self.window_matrices {
            let info = self
                .window_infos
                .iter()
                .find(|info| info.window_id == entry.window_id);
            match info {
                Some(info) => {
                    let _ = write!(
                        out,
                        "-- window {} {:?} bounds=({},{} {}x{})px text=({},{} {}x{})px start={} end={}",
                        entry.window_id,
                        info.buffer_name,
                        entry.pixel_bounds.x,
                        entry.pixel_bounds.y,
                        entry.pixel_bounds.width,
                        entry.pixel_bounds.height,
                        entry.text_pixel_bounds.x,
                        entry.text_pixel_bounds.y,
                        entry.text_pixel_bounds.width,
                        entry.text_pixel_bounds.height,
                        info.window_start,
                        info.window_end,
                    );
                    if entry.selected {
                        let _ = write!(out, " selected");
                    }
                    if info.is_minibuffer {
                        let _ = write!(out, " minibuffer");
                    }
                    let _ = writeln!(out, " --");
                }
                None => {
                    let _ = writeln!(
                        out,
                        "-- window {}{} --",
                        entry.window_id,
                        if entry.selected { " selected" } else { "" }
                    );
                }
            }

            for (row_index, row) in entry.matrix.rows.iter().enumerate() {
                if !row.enabled {
                    continue;
                }
                let (text, runs) = row_text(row);
                match row.role {
                    GlyphRowRole::Text => {
                        let _ = writeln!(out, "{row_index:4}|{text}");
                    }
                    role => {
                        let _ = writeln!(out, "[{}]|{}", role_name(role), text);
                    }
                }
                if with_faces {
                    self.write_face_runs(&mut out, &runs);
                }
            }
        }

        if let Some(cursor) = &self.phys_cursor {
            let _ = writeln!(
                out,
                "[cursor] window={} row={} col={} charpos={} style={:?}",
                cursor.window_id, cursor.row, cursor.col, cursor.charpos, cursor.style
            );
        }

        out
    }

    fn write_face_runs(&self, out: &mut String, runs: &[FaceRun]) {
        for run in runs {
            let (name, fg, bg) = match self.faces.get(&run.face_id) {
                Some(face) => (
                    face.lisp_name.clone().unwrap_or_else(|| {
                        BasicFaceId::from_gnu_code(face.id.get())
                            .map(|basic| basic.name().to_string())
                            .unwrap_or_else(|| format!("face:{}", face.id))
                    }),
                    hex(face.foreground),
                    hex(face.background),
                ),
                None => (
                    format!("face:{}", run.face_id),
                    "??????".into(),
                    "??????".into(),
                ),
            };
            let _ = writeln!(
                out,
                "    : run {}-{} {} fg=#{} bg=#{}",
                run.start, run.end, name, fg, bg
            );
        }
    }
}

/// Render one glyph row as text and collect its face runs.
///
/// Areas are concatenated left-margin + text + right-margin, matching the
/// on-screen left-to-right order. Padding glyphs (second cell of a wide
/// character) are skipped; a `Stretch` prints as its width in spaces; an
/// `Image` prints as `[img:ID]`; `Composite` prints its grapheme text.
/// Trailing spaces are trimmed from the printed line but face runs keep the
/// untrimmed extents.
fn row_text(row: &GlyphRow) -> (String, Vec<FaceRun>) {
    let mut text = String::new();
    let mut runs: Vec<FaceRun> = Vec::new();
    let mut col = 0usize;

    for area in &row.glyphs {
        for glyph in area {
            if glyph.padding {
                continue;
            }
            let start = col;
            match &glyph.glyph_type {
                GlyphType::Char { ch } => {
                    text.push(*ch);
                    col += 1;
                }
                GlyphType::Composite { text: cluster } => {
                    text.push_str(cluster);
                    col += cluster.chars().count();
                }
                GlyphType::Stretch { width_cols } => {
                    let width = *width_cols as usize;
                    text.extend(std::iter::repeat_n(' ', width));
                    col += width;
                }
                GlyphType::Image { image_id, .. } => {
                    let marker = format!("[img:{image_id}]");
                    col += marker.chars().count();
                    text.push_str(&marker);
                }
                GlyphType::Video { video_id, .. } => {
                    let marker = format!("[video:{video_id}]");
                    col += marker.chars().count();
                    text.push_str(&marker);
                }
                GlyphType::Surface { surface_id, .. } => {
                    let marker = format!("[surface:{surface_id}]");
                    col += marker.chars().count();
                    text.push_str(&marker);
                }
                GlyphType::Xwidget { xwidget_id, .. } => {
                    let marker = format!("[xwidget:{xwidget_id}]");
                    col += marker.chars().count();
                    text.push_str(&marker);
                }
                GlyphType::Glyphless { ch } => {
                    text.push(*ch);
                    col += 1;
                }
            }
            if col == start {
                continue; // zero-width glyph: no run contribution
            }
            match runs.last_mut() {
                Some(last) if last.face_id == glyph.face_id && last.end == start => {
                    last.end = col;
                }
                _ => runs.push(FaceRun {
                    start,
                    end: col,
                    face_id: glyph.face_id,
                }),
            }
        }
    }

    while text.ends_with(' ') {
        text.pop();
    }
    (text, runs)
}

fn role_name(role: GlyphRowRole) -> &'static str {
    match role {
        GlyphRowRole::Text => "text",
        GlyphRowRole::TabLine => "tab-line",
        GlyphRowRole::HeaderLine => "header-line",
        GlyphRowRole::ModeLine => "mode-line",
        GlyphRowRole::Minibuffer => "minibuffer",
        GlyphRowRole::TabBar => "tab-bar",
    }
}

/// `#RRGGBB` hex of a color (alpha ignored: snapshot rows are opaque).
fn hex(color: Color) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "{:02X}{:02X}{:02X}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    )
}

#[cfg(test)]
#[path = "snapshot_text_test.rs"]
mod tests;
