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
mod tests {
    use super::*;
    #[derive(Default)]
    struct Painter {
        rows: Vec<usize>,
        fail: bool,
    }
    impl TtyPainter for Painter {
        fn begin(&mut self, _: usize, _: usize) -> io::Result<()> {
            Ok(())
        }
        fn row(&mut self, row: usize, _: &[TtyCell]) -> io::Result<()> {
            self.rows.push(row);
            Ok(())
        }
        fn finish(&mut self, _: Option<(u16, u16, TerminalCursorShape)>) -> io::Result<()> {
            if self.fail {
                Err(io::Error::other("flush failed"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn native_output_commits_only_after_success_and_retries_full_frame() {
        let mut rif = TtyRif::new(3, 2);
        rif.desired.set(0, 0, 'A', CellAttrs::default(), false);
        let mut painter = Painter {
            fail: true,
            ..Painter::default()
        };
        assert!(rif.paint(&mut painter).is_err());
        assert_eq!(rif.current.cells[0].ch, ' ');
        assert_eq!(rif.desired.cells[0].ch, 'A');
        painter.fail = false;
        painter.rows.clear();
        rif.paint(&mut painter).unwrap();
        assert_eq!(painter.rows, [0, 1]);
        assert_eq!(rif.current.cells[0].ch, 'A');
        rif.desired = rif.current.clone();
        painter.rows.clear();
        rif.paint(&mut painter).unwrap();
        assert!(painter.rows.is_empty());
        rif.desired = rif.current.clone();
        rif.desired.set(1, 0, 'B', CellAttrs::default(), false);
        rif.paint(&mut painter).unwrap();
        assert_eq!(painter.rows, [1]);
    }
}
