//! Terminal content extraction — snapshot of terminal state for rendering.
//!
//! Each frame, the render thread extracts a `TerminalContent` from the
//! `rio_vt::crosswords::Crosswords` and converts cells to rendering
//! primitives.

use super::colors::ansi_to_color;
use crate::core::types::Color;
use rio_vt::config::colors::{AnsiColor, ColorRgb, NamedColor};
use rio_vt::crosswords::pos::{Column, Line};
use rio_vt::crosswords::square::{ContentTag, Square, Wide};
use rio_vt::crosswords::style::{Style, StyleFlags as CellFlags};
use rio_vt::crosswords::{Crosswords, Mode};
use rio_vt::event::EventListener;

/// A single cell ready for GPU rendering.
#[derive(Debug, Clone)]
pub struct RenderCell {
    /// Grid column (0-based).
    pub col: usize,
    /// Grid row (0-based, 0 = top of visible area).
    pub row: usize,
    /// Character to display (space for empty cells).
    pub c: char,
    /// Foreground color.
    pub fg: Color,
    /// Background color.
    pub bg: Color,
    /// Cell flags (bold, italic, underline, etc.).
    pub flags: CellFlags,
}

/// Cursor state for rendering.
#[derive(Debug, Clone)]
pub struct RenderCursor {
    pub col: usize,
    pub row: usize,
    pub visible: bool,
}

/// Snapshot of terminal state for one frame.
#[derive(Debug, Clone)]
pub struct TerminalContent {
    /// All visible cells.
    pub cells: Vec<RenderCell>,
    /// Grid dimensions (columns x rows).
    pub cols: usize,
    pub rows: usize,
    /// Cursor info.
    pub cursor: RenderCursor,
    /// Default background color.
    pub default_bg: Color,
    /// Default foreground color.
    pub default_fg: Color,
}

/// Resolve a square's fg color, bg color, and style flags. Squares store a
/// packed style id (or a bg-only fast path) rather than inline colors, so
/// the per-grid style table is consulted for full styling.
fn square_style(square: &Square, styles: &[Style]) -> (AnsiColor, AnsiColor, CellFlags) {
    match square.content_tag() {
        ContentTag::Codepoint => {
            let style = styles
                .get(square.style_id() as usize)
                .copied()
                .unwrap_or_default();
            (style.fg, style.bg, style.flags)
        }
        ContentTag::BgPalette => (
            AnsiColor::Named(NamedColor::Foreground),
            AnsiColor::Indexed(square.bg_palette_index()),
            CellFlags::empty(),
        ),
        ContentTag::BgRgb => {
            let (r, g, b) = square.bg_rgb();
            (
                AnsiColor::Named(NamedColor::Foreground),
                AnsiColor::Spec(ColorRgb { r, g, b }),
                CellFlags::empty(),
            )
        }
    }
}

impl TerminalContent {
    /// Extract renderable content from a rio-vt terminal.
    pub fn from_term<T: EventListener>(term: &Crosswords<T>) -> Self {
        let num_cols = term.columns();
        let num_lines = term.screen_lines();
        let styles = term.grid.style_set.styles();

        let default_fg = Color::WHITE;
        let default_bg = Color::BLACK;

        let mut cells = Vec::with_capacity(num_cols * num_lines);

        for row_idx in 0..num_lines {
            let row = &term.grid[Line(row_idx as i32)];
            for col_idx in 0..num_cols {
                let square = &row[Column(col_idx)];

                // Skip wide char spacers (second cell of double-width character)
                if matches!(square.wide(), Wide::Spacer | Wide::LeadingSpacer) {
                    continue;
                }

                let c = match square.c() {
                    '\0' => ' ',
                    c => c,
                };

                let (sq_fg, sq_bg, flags) = square_style(square, styles);
                let fg = ansi_to_color(&sq_fg, &default_fg, &default_bg);
                let bg = ansi_to_color(&sq_bg, &default_fg, &default_bg);

                cells.push(RenderCell {
                    col: col_idx,
                    row: row_idx,
                    c,
                    fg,
                    bg,
                    flags,
                });
            }
        }

        let cursor_pos = term.cursor().pos;
        let cursor = RenderCursor {
            col: cursor_pos.col.0,
            row: cursor_pos.row.0.max(0) as usize,
            visible: term.mode().contains(Mode::SHOW_CURSOR),
        };

        TerminalContent {
            cells,
            cols: num_cols,
            rows: num_lines,
            cursor,
            default_bg,
            default_fg,
        }
    }
}

/// Extract text from a terminal grid region as a String.
pub fn extract_text<T: EventListener>(
    term: &Crosswords<T>,
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
) -> String {
    let num_cols = term.columns();
    let num_lines = term.screen_lines();
    let mut text = String::new();

    for row in start_row..=end_row {
        let col_start = if row == start_row { start_col } else { 0 };
        let col_end = if row == end_row {
            end_col
        } else {
            num_cols.saturating_sub(1)
        };

        if row < num_lines {
            let line = &term.grid[Line(row as i32)];
            for col in col_start..=col_end {
                if col < num_cols {
                    let square = &line[Column(col)];
                    if !matches!(square.wide(), Wide::Spacer | Wide::LeadingSpacer) {
                        text.push(match square.c() {
                            '\0' => ' ',
                            c => c,
                        });
                    }
                }
            }
        }
        if row < end_row {
            text.push('\n');
        }
    }

    // Trim trailing whitespace per line
    text.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "content_test.rs"]
mod tests;
