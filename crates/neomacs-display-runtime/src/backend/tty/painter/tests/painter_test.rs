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
