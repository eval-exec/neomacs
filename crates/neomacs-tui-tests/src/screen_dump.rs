//! A canonical text encoding of what a terminal shows, for oracles that record
//! a screen in one run and compare it in another (`tests/seam_baseline.rs`,
//! MILESTONES M0).
//!
//! Two dumps are equal exactly when the screens are visibly equal: the same
//! size, the same characters with the same paint in every cell, the same
//! cursor (its position only while it is visible) and the same terminal modes.
//! Cells compare by [`VisibleCellStyle`], this harness's one definition of the
//! attributes that can paint a cell, so terminal write history is not part of a
//! dump: a foreground an erase left on blank cells, or a written space against
//! a never-written cell. Neither are vt100's soft-wrap flags nor the pending SGR
//! state; both record how the bytes arrived, not what is shown, and damage
//! timing may legitimately differ between two implementations of one display.

use crate::{DisplayAttributes, DisplayColor, VisibleBlankStyle, VisibleCellStyle};
use std::fmt::Write as _;

/// The first line of every dump. Change the version with the encoding, so a
/// baseline recorded in one encoding never equals a dump in another.
const ENCODING: &str = "neomacs-tui-tests canonical-screen v1";

/// The canonical text encoding of one terminal screen's visible state.
///
/// The text is line-oriented for review and diffing: a header (size, cursor,
/// modes), one `text` line per row, and run-length `style` lines giving each
/// cell's visible class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalScreenDump(String);

impl CanonicalScreenDump {
    /// Encode what `screen` shows.
    #[must_use]
    pub fn capture(screen: &vt100::Screen) -> Self {
        let (rows, columns) = screen.size();
        let label_width = rows.saturating_sub(1).to_string().len();
        let mut text = String::new();
        let _ = writeln!(text, "{ENCODING}");
        let _ = writeln!(text, "size {rows}x{columns}");
        if screen.hide_cursor() {
            text.push_str("cursor hidden\n");
        } else {
            let (row, column) = screen.cursor_position();
            let _ = writeln!(text, "cursor visible {row},{column}");
        }
        let _ = writeln!(
            text,
            "modes alternate-screen={} application-keypad={} application-cursor={} \
             bracketed-paste={} mouse={:?}/{:?}",
            on_off(screen.alternate_screen()),
            on_off(screen.application_keypad()),
            on_off(screen.application_cursor()),
            on_off(screen.bracketed_paste()),
            screen.mouse_protocol_mode(),
            screen.mouse_protocol_encoding(),
        );

        text.push_str("text\n");
        for row in 0..rows {
            let _ = write!(text, "{row:0label_width$}|");
            for column in 0..columns {
                let cell = cell_at(screen, row, column);
                match CellClass::of(cell) {
                    CellClass::Glyph {
                        width: CellWidth::Continuation,
                        ..
                    } => {}
                    CellClass::Glyph { .. } => text.push_str(cell.contents()),
                    CellClass::Blank(_) => text.push(' '),
                }
            }
            text.push_str("|\n");
        }

        text.push_str("style\n");
        for row in 0..rows {
            let mut run: Option<(u16, CellClass)> = None;
            for column in 0..columns {
                let class = CellClass::of(cell_at(screen, row, column));
                match run {
                    Some((_, current)) if current == class => {}
                    Some((start, current)) => {
                        write_run(&mut text, row, label_width, start..=column - 1, current);
                        run = Some((column, class));
                    }
                    None => run = Some((column, class)),
                }
            }
            if let Some((start, current)) = run {
                write_run(&mut text, row, label_width, start..=columns - 1, current);
            }
        }
        Self(text)
    }

    /// The encoded text, as recorded in a baseline file.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn cell_at(screen: &vt100::Screen, row: u16, column: u16) -> &vt100::Cell {
    screen
        .cell(row, column)
        .unwrap_or_else(|| panic!("terminal cell ({row}, {column}) is absent"))
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

/// How a glyph cell occupies the grid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CellWidth {
    Single,
    /// The first cell of a double-width character.
    Wide,
    /// The cell a double-width character covers after its first.
    Continuation,
}

/// The visible class of one cell: equal classes paint identically.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CellClass {
    Glyph {
        width: CellWidth,
        attributes: DisplayAttributes,
    },
    Blank(VisibleBlankStyle),
}

impl CellClass {
    fn of(cell: &vt100::Cell) -> Self {
        match VisibleCellStyle::from(cell) {
            VisibleCellStyle::Glyph(attributes) => {
                let width = if cell.is_wide_continuation() {
                    CellWidth::Continuation
                } else if cell.is_wide() {
                    CellWidth::Wide
                } else {
                    CellWidth::Single
                };
                Self::Glyph { width, attributes }
            }
            VisibleCellStyle::Blank(style) => Self::Blank(style),
        }
    }

    fn write(self, text: &mut String) {
        match self {
            Self::Glyph { width, attributes } => {
                text.push_str(match width {
                    CellWidth::Single => "glyph",
                    CellWidth::Wide => "wide",
                    CellWidth::Continuation => "cont",
                });
                write_attributes(text, attributes);
            }
            Self::Blank(VisibleBlankStyle::Background(background)) => {
                let _ = write!(text, "blank bg={}", color_token(background));
            }
            Self::Blank(VisibleBlankStyle::Underlined {
                foreground,
                background,
                bold,
                dim,
            }) => {
                let _ = write!(
                    text,
                    "blank-underline fg={} bg={}",
                    color_token(foreground),
                    color_token(background)
                );
                write_flags(text, [("bold", bold), ("dim", dim)]);
            }
            Self::Blank(VisibleBlankStyle::Inverted(attributes)) => {
                text.push_str("blank-inverse");
                write_attributes(text, attributes);
            }
        }
    }
}

fn write_run(
    text: &mut String,
    row: u16,
    label_width: usize,
    columns: std::ops::RangeInclusive<u16>,
    class: CellClass,
) {
    let _ = write!(
        text,
        "{row:0label_width$} {}-{} ",
        columns.start(),
        columns.end()
    );
    class.write(text);
    text.push('\n');
}

fn write_attributes(text: &mut String, attributes: DisplayAttributes) {
    let _ = write!(
        text,
        " fg={} bg={}",
        color_token(attributes.foreground),
        color_token(attributes.background)
    );
    write_flags(
        text,
        [
            ("bold", attributes.bold),
            ("dim", attributes.dim),
            ("italic", attributes.italic),
            ("underline", attributes.underline),
            ("inverse", attributes.inverse),
        ],
    );
}

fn write_flags<const N: usize>(text: &mut String, flags: [(&str, bool); N]) {
    for (name, set) in flags {
        if set {
            text.push(' ');
            text.push_str(name);
        }
    }
}

fn color_token(color: DisplayColor) -> String {
    match color {
        DisplayColor::Default => "default".to_string(),
        DisplayColor::Indexed(index) => format!("idx:{index}"),
        DisplayColor::Rgb(red, green, blue) => format!("rgb:{red:02x}{green:02x}{blue:02x}"),
    }
}
