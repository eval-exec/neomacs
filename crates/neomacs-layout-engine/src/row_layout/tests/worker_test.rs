use super::super::program::{RowProgramGeometry, RowProgramLimits};
use super::*;
use crate::display_item::*;
use crate::display_row::builder::DisplayTabPolicy;
use crate::display_row::face_state::{DisplayRowFace, DisplayRowGlyphMeasurer};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::types::{Color, FaceId};

fn row(text: &str) -> RowProgram {
    row_with_wrap(text, 1000.0, false)
}

fn row_with_wrap(text: &str, width: f32, character_wrap: bool) -> RowProgram {
    let face = FaceId::new(1);
    let faces = vec![DisplayRowFace::from_resolved(
        face,
        &ResolvedFace::default(),
    )];
    let mut measurer = DisplayRowGlyphMeasurer::new(&faces, None, 8.0);
    RowProgram::capture(
        RowProgramGeometry {
            inherited_line_spacing: 0.0,
            character_wrap,
            word_wrap: false,
            fringe: None,
            width,
            metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
            tabs: DisplayTabPolicy::every(4),
            base_face: face,
            background: Color::BLACK,
        },
        [
            DisplayItem::new(
                SourceSpan::synthetic(1, 0, text.len()),
                RenderFaceRef::FaceId(face),
                DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
            ),
            DisplayItem::new(
                SourceSpan::synthetic(1, text.len(), text.len() + 1),
                RenderFaceRef::FaceId(face),
                DisplayItemKind::RowBreak(DisplayRowBreak {
                    line_height: DisplayLineHeightPolicy::Default,
                    line_spacing: DisplayLineSpacingPolicy::Inherit,
                }),
            ),
        ],
        faces.clone(),
        &mut measurer,
        RowProgramLimits {
            items: 4,
            text_bytes: 128,
            glyphs: 128,
        },
    )
    .unwrap()
}

#[test]
fn worker_bounds_visual_rows_instead_of_physical_program_count() {
    let mut worker = RowWorker::default();
    let ticket = worker
        .submit(
            (0..64)
                .map(|_| row_with_wrap("abcdefgh", 16.0, true))
                .collect(),
        )
        .unwrap();
    let result = finish(&mut worker);
    assert_eq!(result.ticket, ticket);
    let rows = result.rows.unwrap();
    assert_eq!(rows.len(), MAX_ROWS);
    assert!(
        rows.iter()
            .any(|row| row.end_kind == super::super::program::ComputedRowEnd::Continuation)
    );
    assert_eq!(
        rows.last().unwrap().end_kind,
        super::super::program::ComputedRowEnd::Newline
    );
}

fn finish(worker: &mut RowWorker) -> RowJobResult {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(result) = worker.take_completed() {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not complete"
        );
        std::thread::yield_now();
    }
}

#[test]
fn newest_request_replaces_pending_and_completed_results() {
    let mut worker = RowWorker::default();
    let first = worker.submit(vec![row("first")]).unwrap();
    assert_eq!(finish(&mut worker).ticket, first);
    let mut newest = first;
    for _ in 0..100 {
        newest = worker.submit(vec![row("newest")]).unwrap();
    }
    let result = finish(&mut worker);
    assert_eq!(result.ticket, newest);
    assert_eq!(result.rows.unwrap().len(), 1);
    assert!(worker.take_completed().is_none());
}

#[test]
fn cancellation_clears_mailboxes_and_does_not_block_new_work() {
    let mut worker = RowWorker::default();
    worker.submit(vec![row("cancelled")]).unwrap();
    worker.cancel();
    assert!(worker.take_completed().is_none());
    let ticket = worker.submit(vec![row("accepted")]).unwrap();
    assert_eq!(finish(&mut worker).ticket, ticket);
}

#[test]
fn batches_reserve_all_output_and_do_not_start_unbounded_work() {
    let mut worker = RowWorker::default();
    assert_eq!(worker.submit(vec![]), Err(RowProgramError::Budget));
    assert_eq!(
        worker.submit((0..=MAX_ROWS).map(|_| row("a")).collect()),
        Err(RowProgramError::Budget)
    );
    assert!(worker.thread.is_none());
    let ticket = worker.submit(vec![row("a")]).unwrap();
    assert_eq!(finish(&mut worker).ticket, ticket);
}
