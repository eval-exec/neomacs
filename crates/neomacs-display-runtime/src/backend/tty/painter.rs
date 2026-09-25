//! Structured output for terminals that cannot consume the ANSI encoder.
use super::{CellAttrs, CellMaterialization, TerminalCursorShape, TtyCell, TtyRif};
use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use std::io;

pub trait TtyPainter {
    fn begin(&mut self, width: usize, height: usize) -> io::Result<()>;
    fn row(&mut self, row: usize, cells: &[TtyCell]) -> io::Result<()>;
    fn finish(&mut self, cursor: Option<(u16, u16, TerminalCursorShape)>) -> io::Result<()>;
}

impl TtyRif {
    /// Paint dirty rows using native terminal operations. A failed write keeps
    /// the desired frame and forces a full retry; only a successful flush commits
    /// the screen model. No ANSI scroll/insert/erase assumptions cross this seam.
    pub fn paint(&mut self, painter: &mut impl TtyPainter) -> io::Result<()> {
        self.frame_stats = super::TtyFrameStats::default();
        let result = (|| {
            painter.begin(self.desired.width, self.desired.height)?;
            for row in 0..self.desired.height {
                let start = row * self.desired.width;
                let range = start..start + self.desired.width;
                let next = &self.desired.cells[range.clone()];
                let previous = &self.current.cells[range];
                let unchanged = next.iter().zip(previous).all(|(a, b)| {
                    a.ch == b.ch
                        && a.attrs == b.attrs
                        && a.padding == b.padding
                        && a.extenders == b.extenders
                });
                if self.force_full_render || !unchanged {
                    painter.row(row, next)?;
                    self.frame_stats.write_runs += 1;
                    self.frame_stats.cells_written +=
                        next.iter().filter(|cell| !cell.padding).count() as u32;
                }
            }
            painter.finish(self.cursor_visible.then_some((
                self.cursor_row,
                self.cursor_col,
                self.cursor_shape,
            )))
        })();
        if result.is_ok() {
            for cell in &mut self.desired.cells {
                cell.materialization = CellMaterialization::Written;
            }
            std::mem::swap(&mut self.current, &mut self.desired);
            self.force_full_render = false;
            self.scroll_seed = None;
        } else {
            self.force_full_render = true;
        }
        // The painter writes its own cursor; B1's record of the terminal
        // cursor applies only to the ANSI encoder.
        self.damage.forget_terminal();
        result
    }
}

/// Encode a row using this terminal's own attribute sequences and glyph text.
/// Both byte renderers share GNU's face rules; native Windows consumes cells.
pub fn encode_cells(
    output: &mut Vec<u8>,
    cells: &[TtyCell],
    capabilities: &TtyAttributeCapabilities,
) {
    encode_cells_with(cells, capabilities, |part| match part {
        CellOutput::Control(bytes) | CellOutput::Text(bytes) => output.extend_from_slice(bytes),
    });
}

/// Control sequences may contain tputs padding; text is always literal.
pub enum CellOutput<'a> {
    Control(&'a [u8]),
    Text(&'a [u8]),
}

pub fn encode_cells_with(
    cells: &[TtyCell],
    capabilities: &TtyAttributeCapabilities,
    mut emit: impl FnMut(CellOutput<'_>),
) {
    let mut previous: Option<CellAttrs> = None;
    let mut bytes = Vec::new();
    for cell in cells.iter().filter(|cell| !cell.padding) {
        if previous.as_ref() != Some(&cell.attrs) {
            if let Some(attrs) = previous {
                super::write_turn_off_face(&mut bytes, &attrs, capabilities);
            }
            super::write_turn_on_face(&mut bytes, &cell.attrs, capabilities);
            emit(CellOutput::Control(&bytes));
            bytes.clear();
            previous = Some(cell.attrs);
        }
        super::write_cell_contents(&mut bytes, cell);
        emit(CellOutput::Text(&bytes));
        bytes.clear();
    }
    if let Some(attrs) = previous {
        super::write_turn_off_face(&mut bytes, &attrs, capabilities);
        emit(CellOutput::Control(&bytes));
    }
}

#[cfg(test)]
#[path = "painter/tests/painter_test.rs"]
mod tests;
