//! Native point decoding. Live counters belong to Window, including nested
//! format-mode-line; read-only split entry points use an isolated snapshot.
use std::cell::Cell;

use crate::buffer::{Buffer, BufferId, BufferManager, CharPos0, EmacsBytePos};
use crate::window::{FrameManager, ModeLineBase, Window, WindowId};

#[derive(Clone, Copy, Debug)]
pub(super) enum Field {
    Line,
    Column,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum PointSelection {
    CurrentBuffer,
    WindowSnapshot,
}

#[derive(Clone, Copy, Debug)]
enum BufferRelation {
    Displayed,
    Foreign,
}

impl BufferRelation {
    fn between(displayed: Option<BufferId>, current: BufferId) -> Self {
        match displayed {
            Some(id) if id == current => Self::Displayed,
            _ => Self::Foreign,
        }
    }
}

pub(super) enum Access<'a> {
    Snapshot,
    /// Constructed only for one native expansion. No Lisp may run with this
    /// borrow alive; eval_form remains the walker's sole Lisp boundary.
    Live(&'a mut FrameManager),
}

impl std::fmt::Debug for Access<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot => f.write_str("Snapshot"),
            Self::Live(_) => f.write_str("Live(FrameManager)"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum Source {
    #[default]
    Live,
    Snapshot {
        buffer: BufferId,
        point: EmacsBytePos,
    },
}

#[derive(Debug, Default)]
pub(super) struct PointContext {
    source: Source,
    window: Option<WindowId>,
    snapshot_buffer: Option<BufferId>,
    snapshot_start: CharPos0,
    snapshot_height: usize,
    // Snapshot entry points cannot mutate FrameManager. Never authoritative on
    // a Live path, and never copied back over the live window after eval.
    snapshot_base: Cell<Option<ModeLineBase>>,
}

fn fresh(buffer: &Buffer, base: Option<ModeLineBase>) -> bool {
    !buffer.mode_line_clip_changed()
        && base.is_none_or(|base| {
            buffer.gnu_beg_unchanged().is_none_or(|unchanged| {
                // GNU compares a char count with its 1-based base_line_pos.
                i128::from(unchanged) >= base.position().get() as i128 + 1
            })
        })
}

impl PointContext {
    pub(super) fn new(
        buffer: &Buffer,
        window: WindowId,
        start: CharPos0,
        point: EmacsBytePos,
        height: usize,
        base: Option<ModeLineBase>,
    ) -> Self {
        Self {
            source: Source::Snapshot {
                buffer: buffer.id(),
                point,
            },
            window: Some(window),
            snapshot_buffer: Some(buffer.id()),
            snapshot_start: start,
            snapshot_height: height,
            snapshot_base: Cell::new(base.filter(|_| fresh(buffer, base))),
        }
    }

    /// Called once at whole Context-format entry, never once per string.
    /// A foreign explicit BUFFER must not flush the original window's state.
    /// Existing Context entry/unwind code owns initial point and restoration.
    /// The disabled legacy-flow entry retains its old explicit point snapshot.
    pub(super) fn enter_context(
        &mut self,
        frames: &mut FrameManager,
        buffers: &BufferManager,
        point_selection: PointSelection,
    ) {
        match point_selection {
            PointSelection::CurrentBuffer => self.source = Source::Live,
            PointSelection::WindowSnapshot => {}
        }
        if let Some(id) = self.window
            && let Some(frame_id) = frames.find_window_frame_id(id)
            && let Some(window) = frames
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(id))
            && let Some(buffer) = buffers.current_buffer()
            && window.buffer_id() == Some(buffer.id())
            && !fresh(buffer, window.mode_line_base())
        {
            window.set_mode_line_base(None);
        }
    }

    fn point(&self, buffer: &Buffer) -> EmacsBytePos {
        let point = match self.source {
            Source::Snapshot { buffer: id, point } if id == buffer.id() => point,
            Source::Live | Source::Snapshot { .. } => buffer.point_emacs_byte_pos(),
        };
        buffer.accessible_emacs_byte_region().clamp(point)
    }

    /// Only %l calls this. A native expansion cannot run Lisp; the live window
    /// borrow ends before the recursive walker can evaluate another form.
    pub(super) fn line_and_column(
        &self,
        buffer: &Buffer,
        access: &mut Access<'_>,
    ) -> super::LineColumn {
        match access {
            Access::Live(frames) => {
                if let Some(id) = self.window
                    && let Some(frame_id) = frames.find_window_frame_id(id)
                    && let Some(frame) = frames.get_mut(frame_id)
                {
                    let char_height = frame.char_height;
                    if let Some(window) = frame.find_window_mut(id) {
                        let relation = BufferRelation::between(window.buffer_id(), buffer.id());
                        let height = usize::try_from(window.total_lines(char_height)).unwrap_or(0);
                        let start = match window {
                            Window::Leaf { window_start, .. } => window_start.to_char_pos(),
                            Window::Internal { .. } => buffer.point_min_char_pos(),
                        };
                        let (value, base) =
                            self.decode(buffer, relation, start, height, window.mode_line_base());
                        if let BufferRelation::Displayed = relation {
                            window.set_mode_line_base(base);
                        }
                        return value;
                    }
                }
                // A missing original window cannot be replaced by selection.
                self.decode(
                    buffer,
                    BufferRelation::Foreign,
                    buffer.point_min_char_pos(),
                    0,
                    None,
                )
                .0
            }
            Access::Snapshot => {
                let relation = BufferRelation::between(self.snapshot_buffer, buffer.id());
                let (value, base) = self.decode(
                    buffer,
                    relation,
                    self.snapshot_start,
                    self.snapshot_height,
                    self.snapshot_base.get(),
                );
                if let BufferRelation::Displayed = relation {
                    self.snapshot_base.set(base);
                }
                value
            }
        }
    }

    fn decode(
        &self,
        buffer: &Buffer,
        relation: BufferRelation,
        start: CharPos0,
        height: usize,
        previous: Option<ModeLineBase>,
    ) -> (super::LineColumn, Option<ModeLineBase>) {
        let begv = buffer.point_min_char_pos();
        let zv = buffer.point_max_char_pos();
        let start = match relation {
            BufferRelation::Displayed if start >= begv && start <= zv => start,
            BufferRelation::Displayed | BufferRelation::Foreign => begv,
        };
        // Numeric character positions deliberately survive an intervening eval.
        // Freshness was checked at whole-format entry, never mid-walk.
        let base = match relation {
            BufferRelation::Displayed => previous
                .filter(|base| base.position() <= start)
                .unwrap_or_else(|| ModeLineBase::first(begv)),
            BufferRelation::Foreign => ModeLineBase::first(begv),
        };
        let start_byte = buffer.char_pos_to_emacs_byte_pos_clamped(start);
        let base_byte = buffer.char_pos_to_emacs_byte_pos_clamped(base.position());
        let point = self.point(buffer);
        let nlines = buffer.count_newlines_emacs_byte(base_byte, start_byte);
        let top_line = base.line_number().saturating_add(nlines);
        let mut next = previous;
        if let BufferRelation::Displayed = relation {
            next = if start == begv {
                ModeLineBase::new(begv, top_line)
            } else if nlines < height.saturating_add(25)
                || nlines > height.saturating_mul(3).saturating_add(50)
                || base.position() == begv
            {
                // GNU's ordinary-LF backward reseat. The preexisting width
                // rejection/?? and selective-display gaps are not added here.
                let wanted = height.saturating_mul(2).saturating_add(30);
                let floor = buffer.point_min_emacs_byte_pos();
                // GNU stops AFTER its wanted-th preceding newline and returns
                // wanted-1. Existing backward-line rank/select with n=wanted-1
                // has exactly that position/count, including a short floor hit.
                // It retains its own existing-index/build/query policy.
                let (position, found) = buffer
                    .lines_backward_emacs_byte(start_byte, floor, wanted - 1)
                    .unwrap_or_else(|| {
                        let mut cursor = start_byte;
                        let mut found = 0_usize;
                        let mut position = floor;
                        while found < wanted {
                            let Some(newline) = buffer.prev_newline_emacs_byte(cursor, floor)
                            else {
                                break;
                            };
                            found += 1;
                            if found == wanted {
                                position = newline.add_len(crate::buffer::EmacsByteLen::new(1));
                                found -= 1;
                                break;
                            }
                            cursor = newline;
                        }
                        (position, found)
                    });
                top_line.checked_sub(found).and_then(|line| {
                    ModeLineBase::new(buffer.emacs_byte_pos_to_char_pos_clamped(position), line)
                })
            } else {
                Some(base)
            };
        }
        // This proof concerns the CURRENT shared base, not an outer entry
        // snapshot: only line-1 at today's BEGV matches the byte prefix origin.
        let value = if base.position() == begv && base.line_number() == 1 && point >= start_byte {
            super::prefix_line_and_column(buffer, buffer.accessible_emacs_byte_region(), point)
        } else {
            super::LineColumn {
                // Literal GNU call uses positive PT as count; PT before start
                // therefore contributes zero (not a signed backward delta).
                line: top_line.saturating_add(buffer.count_newlines_emacs_byte(start_byte, point)),
                column: self.column(buffer),
            }
        };
        (value, next)
    }

    pub(super) fn column(&self, buffer: &Buffer) -> usize {
        let point = self.point(buffer);
        let origin = buffer.point_min_emacs_byte_pos();
        let bol = buffer
            .prev_newline_emacs_byte(point, origin)
            .map(|nl| nl.add_len(crate::buffer::EmacsByteLen::new(1)))
            .unwrap_or(origin);
        let mut tail = Vec::new();
        buffer.copy_emacs_byte_range_to(crate::buffer::EmacsByteRange::new(bol, point), &mut tail);
        super::emacs_char_count(&tail, buffer.get_multibyte())
    }
}
