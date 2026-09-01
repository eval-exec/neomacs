//! Fontification coverage discovered by an immutable display walk.
//!
//! GNU's display iterator invokes `handle_fontified_prop` at the buffer
//! positions it actually visits.  Neomacs snapshots buffer state before its
//! row walk, so Lisp cannot run from inside that immutable walk.  This module
//! preserves GNU's semantic boundary in two phases: the provisional walk
//! records the positions it visibly reached, then a typed sparse plan requests
//! fontification only for uncovered positions and the engine retries with a
//! fresh snapshot.

use crate::neovm_bridge::LayoutBufferView;
use neovm_core::buffer::{CharLen, CharPos0};
use neovm_core::emacs_core::Value;
use neovm_core::window::WindowDisplaySnapshot;

/// One half-open 0-based buffer span reached by the display walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VisibleFontificationSpan {
    start: CharPos0,
    end: CharPos0,
}
impl VisibleFontificationSpan {
    const fn single_char(start: CharPos0) -> Self {
        Self {
            start,
            end: start.add_len(CharLen::new(1)),
        }
    }

    pub(crate) const fn start(self) -> i64 {
        self.start.get() as i64
    }

    pub(crate) const fn end(self) -> i64 {
        self.end.get() as i64
    }

    fn include(&mut self, charpos: CharPos0) -> bool {
        if charpos > self.end {
            return false;
        }
        self.end = self.end.max(charpos.add_len(CharLen::new(1)));
        true
    }
}

/// Sparse visible positions that the speculative contiguous pre-pass did not
/// cover.  Hidden buffer gaps remain gaps instead of becoming one potentially
/// enormous fontification request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VisibleFontificationPlan {
    spans: Vec<VisibleFontificationSpan>,
}

impl VisibleFontificationPlan {
    pub(crate) fn spans(&self) -> impl Iterator<Item = VisibleFontificationSpan> + '_ {
        self.spans.iter().copied()
    }

    fn push(&mut self, charpos: CharPos0) {
        if self
            .spans
            .last_mut()
            .is_some_and(|span| span.include(charpos))
        {
            return;
        }
        self.spans
            .push(VisibleFontificationSpan::single_char(charpos));
    }
}

/// Whether the provisional row walk found unfontified visible positions.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VisibleFontificationCoverage {
    Complete,
    Requires(VisibleFontificationPlan),
}

impl VisibleFontificationCoverage {
    pub(crate) fn inspect<B: LayoutBufferView + ?Sized>(
        buffer: &B,
        snapshot: &WindowDisplaySnapshot,
        contiguous_prepass_end: CharPos0,
    ) -> Self {
        let accessible_end = buffer.layout_point_max_char_pos();
        let fontified = Value::symbol("fontified");
        let mut plan = VisibleFontificationPlan { spans: Vec::new() };

        for point in &snapshot.points {
            let charpos = CharPos0::new(point.buffer_pos.as_i64().saturating_sub(1) as usize);
            if charpos < contiguous_prepass_end || charpos >= accessible_end {
                continue;
            }
            let bytepos = buffer.layout_char_pos_to_emacs_byte_pos(charpos);
            let already_fontified = buffer
                .layout_text_prop_at_emacs_byte_pos(bytepos, fontified)
                .is_some_and(|value| !value.is_nil());
            if !already_fontified {
                plan.push(charpos);
            }
        }

        if plan.spans.is_empty() {
            Self::Complete
        } else {
            Self::Requires(plan)
        }
    }
}
