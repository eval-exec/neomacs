//! GNU's long-line redisplay restriction (`long_line_optimizations_p`).
//!
//! GNU `handle_fontified_prop` (`src/xdisp.c:4702-4719`) and
//! `safe_run_hooks_maybe_narrowed` (`src/keyboard.c:1983-2001`) run Lisp
//! hooks on a labeled narrowing around point in a buffer whose redisplay found
//! a long line. GNU records `restore_point_unwind` beside the restriction
//! (`src/editfns.c:2890-2895`), so point is exactly restored even though
//! `narrow-to-region` clamped it.
//!
//! The narrowing is transient: it exists only around the hook call. It must
//! not leave the durable `clip_changed` witness of a user restriction change
//! behind, because Neo's layout treats that witness as a new frame input and
//! would re-lay-out the whole frame after every hook call.

use super::Context;
use neovm_edit::buffer::{BufferId, CharPos0, EmacsBytePos, EmacsByteRange, SavedRestrictionState};
use neovm_value::value::Value;

/// One entered long-line restriction. Restore it with
/// [`Context::exit_long_line_narrowing`] on every path out of the hook.
pub struct LongLineNarrowing {
    buffer_id: BufferId,
    saved: SavedRestrictionState,
    clip_before: bool,
    point_before: EmacsBytePos,
}

impl Context {
    /// Refresh the current buffer's `long_line_optimizations_p` flag, as GNU
    /// `redisplay_window` does (`src/xdisp.c:20646-20663`). The buffer method
    /// owns GNU's eight-modification-tick hysteresis.
    #[doc(hidden)]
    pub fn refresh_long_line_optimizations(&mut self, buffer_id: BufferId) {
        if let Some(threshold) = self
            .builtin_var_value_by_name("long-line-threshold")
            .and_then(|value| value.as_int())
            && let Some(buffer) = self.edit.buffers.get_mut(buffer_id)
        {
            buffer.refresh_long_line_optimizations(threshold);
        }
    }

    /// Enter GNU's large-narrowing region around `pos` (a 1-based Lisp
    /// position) when the current buffer has a long line and
    /// `long-line-optimizations-region-size` is positive.
    ///
    /// `get_large_narrowing_begv/zv` (`src/xdisp.c:3793-3822`) are in 1-based
    /// buffer positions; `CharPos0` is 0-based, so the conversions subtract
    /// one. The walk back to the beginning of the line is bounded by
    /// `long-line-optimizations-bol-search-limit`.
    #[doc(hidden)]
    pub fn enter_long_line_narrowing(
        &mut self,
        pos: i64,
        label: &'static str,
    ) -> Option<LongLineNarrowing> {
        let region_size = self
            .builtin_var_value_by_name("long-line-optimizations-region-size")
            .and_then(|value| value.as_int())?;
        if region_size <= 0 {
            return None;
        }
        let bol_limit = self
            .builtin_var_value_by_name("long-line-optimizations-bol-search-limit")
            .and_then(|value| value.as_int())
            .unwrap_or(0);
        let buffer_id = self.edit.buffers.current_buffer_id()?;
        let buffer = self.edit.buffers.get(buffer_id)?;
        if !buffer.long_line_optimizations_p() {
            return None;
        }
        let point_min = buffer.point_min_lisp_char_pos().as_i64();
        let point_max = buffer.point_max_lisp_char_pos().as_i64();
        let len = region_size / 2;
        let mut beg = pos.saturating_sub(len).max(point_min);
        let zv = pos.saturating_add(len).min(point_max);
        let mut limit = bol_limit.max(0);
        while beg > point_min && limit > 0 {
            // The character before 1-based `beg` sits at CharPos0(beg - 2).
            let before =
                buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new((beg - 2).max(0) as usize));
            if buffer.emacs_byte_at_pos(before) == Some(b'\n') {
                break;
            }
            beg -= 1;
            limit -= 1;
        }
        if beg >= zv {
            return None;
        }
        let range = EmacsByteRange::new(
            buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new((beg - 1) as usize)),
            buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new((zv - 1) as usize)),
        );
        let clip_before = buffer.mode_line_clip_changed();
        let point_before = buffer.point_emacs_byte_pos();
        let saved = self.edit.buffers.save_current_restriction_state()?;
        self.edit
            .buffers
            .internal_labeled_narrow_to_emacs_byte_range(buffer_id, range, Value::symbol(label))?;
        Some(LongLineNarrowing {
            buffer_id,
            saved,
            clip_before,
            point_before,
        })
    }

    /// Undo [`Self::enter_long_line_narrowing`]: restore the restriction,
    /// window point, and the pre-existing clip witness.
    #[doc(hidden)]
    pub fn exit_long_line_narrowing(&mut self, narrowing: LongLineNarrowing) {
        self.edit
            .buffers
            .restore_saved_restriction_state(narrowing.saved);
        let _ = self
            .edit
            .buffers
            .goto_buffer_emacs_byte_pos(narrowing.buffer_id, narrowing.point_before);
        if !narrowing.clip_before
            && let Some(buffer) = self.edit.buffers.get(narrowing.buffer_id)
        {
            buffer.acknowledge_mode_line_clip();
        }
    }
}
