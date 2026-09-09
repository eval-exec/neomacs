//! Tooltip-local text layout. Native placement is deliberately absent.
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::tooltip::{TooltipRequest, TooltipTextRun};

pub(crate) struct TooltipCharacter {
    pub value: char,
    pub x: f32,
    pub advance: f32,
    pub face: Option<usize>,
}

pub(crate) struct TooltipLine {
    pub start: usize,
    pub characters: Vec<TooltipCharacter>,
    width: f32,
}

pub struct TooltipLayout {
    pub(crate) lines: Vec<TooltipLine>,
    pub(crate) runs: Vec<TooltipTextRun>,
    pub(crate) fg: (f32, f32, f32),
    pub(crate) bg: (f32, f32, f32),
    pub(crate) border: (f32, f32, f32),
    pub(crate) border_width: f32,
    pub(crate) padding: f32,
    pub(crate) line_height: f32,
    pub(crate) bounds: (f32, f32, f32, f32),
}

impl TooltipLayout {
    pub fn measure(request: &TooltipRequest, char_width: f32, line_height: f32) -> Self {
        Self::measure_with(request, char_width, line_height, |_, _| char_width)
    }

    /// The same atlas that paints supplies advances. Limits are expressed in
    /// default-font columns, but wrapping uses the actual styled glyph widths.
    pub fn measure_with(
        request: &TooltipRequest,
        char_width: f32,
        line_height: f32,
        mut advance: impl FnMut(char, Option<&Face>) -> f32,
    ) -> Self {
        let (max_width, max_rows) = request.max_size.map_or((f32::MAX, usize::MAX), |limits| {
            (
                limits.columns.get() as f32 * char_width,
                limits.rows.get() as usize,
            )
        });
        let mut runs = request.runs.clone();
        runs.sort_by_key(|run| run.range.start);
        let line_height = runs
            .iter()
            .filter_map(|run| run.font.as_ref())
            .map(|font| font.ascent_px + font.descent_px)
            .fold(line_height, f32::max);
        let mut lines = Vec::new();
        let mut line = TooltipLine {
            start: 0,
            characters: Vec::new(),
            width: 0.0,
        };
        for (index, ch) in request.text.chars().enumerate() {
            if ch == '\n' {
                lines.push(line);
                line = TooltipLine {
                    start: index + 1,
                    characters: Vec::new(),
                    width: 0.0,
                };
            } else {
                let candidate = runs.partition_point(|run| run.range.end <= index);
                let face = runs
                    .get(candidate)
                    .filter(|run| run.range.contains(&index))
                    .map(|_| candidate);
                let width = advance(ch, face.map(|index| &runs[index].face));
                let width = if width.is_finite() && width >= 0.0 {
                    width
                } else {
                    char_width
                };
                if line.width + width > max_width && !line.characters.is_empty() {
                    lines.push(line);
                    line = TooltipLine {
                        start: index,
                        characters: Vec::new(),
                        width: 0.0,
                    };
                    if lines.len() >= max_rows {
                        break;
                    }
                }
                line.characters.push(TooltipCharacter {
                    value: ch,
                    x: line.width,
                    advance: width,
                    face,
                });
                line.width += width;
            }
            if lines.len() >= max_rows {
                break;
            }
        }
        if lines.len() < max_rows {
            lines.push(line);
        }
        let padding = request.padding as f32 + request.border_width as f32;
        let width = lines.iter().map(|line| line.width).fold(0.0, f32::max) + padding * 2.0;
        let height = lines.len().max(1) as f32 * line_height + padding * 2.0;
        let rgb = |pixel: u32| {
            (
                ((pixel >> 16) & 255) as f32 / 255.0,
                ((pixel >> 8) & 255) as f32 / 255.0,
                (pixel & 255) as f32 / 255.0,
            )
        };
        Self {
            lines,
            runs,
            line_height,
            fg: rgb(request.foreground.unwrap_or(0)),
            bg: rgb(request.background.unwrap_or(0xffffe0)),
            border: rgb(request.border.or(request.foreground).unwrap_or(0)),
            border_width: request.border_width as f32,
            padding,
            bounds: (0.0, 0.0, width.ceil().max(1.0), height.ceil().max(1.0)),
        }
    }
    pub fn extent(&self) -> (f32, f32) {
        (self.bounds.2, self.bounds.3)
    }
    pub fn fit_surface(&mut self, width: f32, height: f32) {
        // A compositor may constrain a native surface after measurement.
        // Reflow using the already measured advances, preserving hard breaks
        // and the user's column limit. Callers remeasure before a later resize.
        let max_width = (width - 2.0 * self.padding).max(1.0);
        let mut lines = Vec::new();
        for original in std::mem::take(&mut self.lines) {
            let mut line = TooltipLine {
                start: original.start,
                characters: Vec::new(),
                width: 0.0,
            };
            for (index, mut character) in original.characters.into_iter().enumerate() {
                if line.width + character.advance > max_width && !line.characters.is_empty() {
                    lines.push(line);
                    line = TooltipLine {
                        start: original.start + index,
                        characters: Vec::new(),
                        width: 0.0,
                    };
                }
                character.x = line.width;
                line.width += character.advance;
                line.characters.push(character);
            }
            lines.push(line);
        }
        self.lines = lines;
        self.bounds.2 = width;
        self.bounds.3 = height;
    }
}

#[cfg(test)]
#[path = "tooltip_layout_test.rs"]
mod tests;
