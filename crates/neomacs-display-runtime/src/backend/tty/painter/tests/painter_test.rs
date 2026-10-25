use super::*;

#[derive(Debug, PartialEq, Eq)]
enum PaintEvent {
    Begin,
    ClearScreen,
    Row,
    Finish,
}

#[derive(Default)]
struct Painter {
    rows: Vec<usize>,
    fail: bool,
    events: Vec<PaintEvent>,
}
impl TtyPainter for Painter {
    fn begin(&mut self, _: usize, _: usize) -> io::Result<()> {
        self.events.push(PaintEvent::Begin);
        Ok(())
    }
    fn clear_screen(&mut self) -> io::Result<()> {
        self.events.push(PaintEvent::ClearScreen);
        Ok(())
    }
    fn row(&mut self, row: usize, _: &[TtyCell]) -> io::Result<()> {
        self.rows.push(row);
        self.events.push(PaintEvent::Row);
        Ok(())
    }
    fn finish(&mut self, _: Option<(u16, u16, TerminalCursorShape)>) -> io::Result<()> {
        self.events.push(PaintEvent::Finish);
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
    assert_eq!(rif.current.cells[0].ch(), ' ');
    assert_eq!(rif.desired.cells[0].ch(), 'A');
    painter.fail = false;
    painter.rows.clear();
    rif.paint(&mut painter).unwrap();
    assert_eq!(painter.rows, [0, 1]);
    assert_eq!(rif.current.cells[0].ch(), 'A');
    rif.desired = rif.current.clone();
    painter.rows.clear();
    rif.paint(&mut painter).unwrap();
    assert!(painter.rows.is_empty());
    rif.desired = rif.current.clone();
    rif.desired.set(1, 0, 'B', CellAttrs::default(), false);
    rif.paint(&mut painter).unwrap();
    assert_eq!(painter.rows, [1]);
}

#[test]
fn resized_native_output_clears_before_rows_and_retries_after_failed_flush() {
    let mut rif = TtyRif::new(3, 2);
    let mut painter = Painter::default();
    rif.paint(&mut painter).unwrap();
    assert!(!painter.events.contains(&PaintEvent::ClearScreen));

    rif.resize(4, 2);
    painter.events.clear();
    painter.fail = true;
    assert!(rif.paint(&mut painter).is_err());
    assert_eq!(
        painter.events,
        [
            PaintEvent::Begin,
            PaintEvent::ClearScreen,
            PaintEvent::Row,
            PaintEvent::Row,
            PaintEvent::Finish
        ]
    );
    assert_eq!(
        rif.physical_clear_requirement(),
        PendingPhysicalClear::AfterResize
    );

    painter.events.clear();
    painter.fail = false;
    rif.paint(&mut painter).unwrap();
    assert_eq!(
        painter.events,
        [
            PaintEvent::Begin,
            PaintEvent::ClearScreen,
            PaintEvent::Row,
            PaintEvent::Row,
            PaintEvent::Finish
        ]
    );
    assert_eq!(rif.physical_clear_requirement(), PendingPhysicalClear::None);

    painter.events.clear();
    rif.force_redraw();
    rif.paint(&mut painter).unwrap();
    assert_eq!(
        painter.events,
        [
            PaintEvent::Begin,
            PaintEvent::Row,
            PaintEvent::Row,
            PaintEvent::Finish
        ]
    );
}
