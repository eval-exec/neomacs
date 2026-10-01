//! One active program batch, one replaceable pending batch, one completed
//! result. Evaluator-owned layout identities never enter this mailbox.

use super::program::{ComputedRow, RowProgram, RowProgramError};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

const MAX_ROWS: usize = 64;
pub(crate) const MAX_PROGRAMS: usize = 64;
const MAX_BYTES: usize = 64 * 1024;
const MAX_GLYPHS: usize = 16 * 1024;
const MAX_ITEMS: usize = 4096;
pub(crate) const MAX_FONT_BYTES: usize = 64 * 1024;

/// An opaque receipt. A caller must still validate its full layout key before
/// admitting the corresponding result; this number says nothing about reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowJobTicket(u64);

pub(crate) struct RowJobResult {
    pub ticket: RowJobTicket,
    pub rows: Result<Vec<ComputedRow>, RowProgramError>,
}

struct Job {
    ticket: RowJobTicket,
    rows: Vec<RowProgram>,
}

#[derive(Default)]
struct Mailbox {
    pending: Option<Job>,
    completed: Option<RowJobResult>,
    stopping: bool,
}

#[derive(Default)]
struct Shared {
    mailbox: Mutex<Mailbox>,
    wake: Condvar,
    revision: AtomicU64,
}

pub(crate) struct RowWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Default for RowWorker {
    fn default() -> Self {
        Self {
            shared: Arc::default(),
            thread: None,
        }
    }
}

impl RowWorker {
    pub(crate) fn submit(
        &mut self,
        rows: Vec<RowProgram>,
    ) -> Result<RowJobTicket, RowProgramError> {
        if rows.is_empty() || rows.len() > MAX_PROGRAMS {
            return Err(RowProgramError::Budget);
        }
        // Reserve worst-case output, not merely the number of glyphs capture
        // happened to see. This bounds active, pending and completed payloads.
        let mut bytes = 0usize;
        let mut glyphs = 0usize;
        let mut items = 0usize;
        let mut font_bytes = 0usize;
        let mut snapshots = rustc_hash::FxHashSet::default();
        for row in &rows {
            let limits = row.limits();
            bytes = bytes.saturating_add(limits.text_bytes);
            glyphs = glyphs.saturating_add(limits.glyphs);
            items = items.saturating_add(limits.items);
            if row
                .font_snapshot_identity()
                .is_some_and(|identity| snapshots.insert(identity))
            {
                font_bytes = font_bytes.saturating_add(row.font_bytes());
            }
        }
        if bytes > MAX_BYTES
            || glyphs > MAX_GLYPHS
            || items > MAX_ITEMS
            || font_bytes > MAX_FONT_BYTES
        {
            return Err(RowProgramError::Budget);
        }
        if self.thread.is_none() {
            let shared = self.shared.clone();
            self.thread = Some(
                std::thread::Builder::new()
                    .name("neomacs-row-layout".into())
                    .spawn(move || run(shared))
                    .map_err(|_| RowProgramError::Unsupported)?,
            );
        }
        let mut mailbox = self.shared.mailbox.lock().unwrap();
        let next = self
            .shared
            .revision
            .load(Ordering::Relaxed)
            .checked_add(1)
            .ok_or(RowProgramError::Budget)?;
        let ticket = RowJobTicket(next);
        self.shared.revision.store(next, Ordering::Release);
        mailbox.pending = Some(Job { ticket, rows });
        mailbox.completed = None;
        tracing::debug!(target: "neomacs_layout_engine::row_worker",
            ticket = ticket.0, "row job submitted");
        self.shared.wake.notify_one();
        Ok(ticket)
    }

    /// Invalidate active work as well as pending and already completed work.
    /// No join or wait is performed on the input/redisplay path.
    pub(crate) fn cancel(&mut self) {
        let mut mailbox = self.shared.mailbox.lock().unwrap();
        self.shared.revision.fetch_add(1, Ordering::Release);
        mailbox.pending = None;
        mailbox.completed = None;
    }

    pub(crate) fn take_completed(&mut self) -> Option<RowJobResult> {
        let result = self.shared.mailbox.lock().unwrap().completed.take();
        if let Some(result) = &result {
            tracing::debug!(target: "neomacs_layout_engine::row_worker",
                ticket = result.ticket.0, "row result taken");
        }
        result
    }
}

fn run(shared: Arc<Shared>) {
    let mut fonts = super::font_measurement::WorkerFontMeasurements::default();
    loop {
        let job = {
            let mut mailbox = shared.mailbox.lock().unwrap();
            while mailbox.pending.is_none() && !mailbox.stopping {
                mailbox = shared.wake.wait(mailbox).unwrap();
            }
            if mailbox.stopping {
                return;
            }
            mailbox.pending.take().unwrap()
        };
        let cancelled = || shared.revision.load(Ordering::Acquire) != job.ticket.0;
        tracing::debug!(target: "neomacs_layout_engine::row_worker",
            ticket = job.ticket.0, programs = job.rows.len(), "row computation started");
        let rows = compute_rows(job.rows, &mut fonts, cancelled);
        let mut mailbox = shared.mailbox.lock().unwrap();
        if !mailbox.stopping && !cancelled() {
            tracing::debug!(target: "neomacs_layout_engine::row_worker",
                ticket = job.ticket.0, rows = rows.as_ref().map_or(0, Vec::len),
                succeeded = rows.is_ok(), "row result ready");
            mailbox.completed = Some(RowJobResult {
                ticket: job.ticket,
                rows,
            });
        } else {
            tracing::debug!(target: "neomacs_layout_engine::row_worker",
                ticket = job.ticket.0, "row result cancelled");
        }
    }
}

// A failed row is never published. Earlier complete rows remain useful for
// a partial scroll; cancellation invalidates the entire batch.
fn compute_rows(
    programs: Vec<RowProgram>,
    fonts: &mut super::font_measurement::WorkerFontMeasurements,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<ComputedRow>, RowProgramError> {
    let mut rows = Vec::with_capacity(programs.len());
    let mut programs = programs.into_iter();
    while let Some(mut program) = programs.next() {
        if rows.len() == MAX_ROWS {
            break;
        }
        program.measure_on_worker(fonts, &cancelled)?;
        while !program.is_complete() {
            let Some(mut fragment) = programs.next() else {
                break;
            };
            fragment.measure_on_worker(fonts, &cancelled)?;
            program.append_measured_fragment(
                fragment,
                super::program::RowProgramLimits {
                    items: MAX_ITEMS,
                    text_bytes: MAX_BYTES,
                    glyphs: MAX_GLYPHS,
                },
            )?;
        }
        match program.compute_visual_rows(MAX_ROWS - rows.len(), &cancelled) {
            Ok(computed) => rows.extend(computed),
            Err(
                RowProgramError::Overflow | RowProgramError::Budget | RowProgramError::Incomplete,
            ) if !rows.is_empty() => break,
            Err(error) => return Err(error),
        }
    }
    if cancelled() {
        Err(RowProgramError::Cancelled)
    } else {
        Ok(rows)
    }
}

impl Drop for RowWorker {
    fn drop(&mut self) {
        {
            let mut mailbox = self.shared.mailbox.lock().unwrap();
            mailbox.stopping = true;
            mailbox.pending = None;
            mailbox.completed = None;
            self.shared.revision.fetch_add(1, Ordering::Release);
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
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
}
