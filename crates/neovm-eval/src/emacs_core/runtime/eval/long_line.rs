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

/// GNU `get_large_narrowing_begv/zv` (`src/xdisp.c:3793-3822`) in 1-based
/// Lisp positions, together with the whole buffer's BEG/Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongLineNarrowingBounds {
    /// First position of the narrowed region (1-based, inclusive).
    pub beg: i64,
    /// End of the narrowed region (1-based, exclusive).
    pub zv: i64,
    /// Whole-buffer start (GNU `BEG`, 1-based).
    pub buffer_start: i64,
    /// Whole-buffer end (GNU `Z`, 1-based, exclusive).
    pub buffer_end: i64,
}

impl LongLineNarrowingBounds {
    /// GNU installs no restriction when the region already covers the whole
    /// buffer (`begv != BEG || zv != Z`, src/xdisp.c:4713 and
    /// src/keyboard.c:1994). The comparison is against BEG/Z, not BEGV/ZV, so
    /// a narrowed buffer still gets the labeled restriction.
    pub fn covers_whole_buffer(self) -> bool {
        self.beg <= self.buffer_start && self.zv >= self.buffer_end
    }
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

    /// GNU's `get_large_narrowing_begv/zv` (`src/xdisp.c:3793-3822`) around
    /// `pos` (a 1-based Lisp position), when the current buffer has a long
    /// line and `long-line-optimizations-region-size` is positive.
    ///
    /// The region is `pos +/- region-size/2`, clamped to the accessible
    /// region, with the beginning moved back to the line start for at most
    /// `long-line-optimizations-bol-search-limit` characters. `CharPos0` is
    /// 0-based, so byte conversions subtract one.
    #[doc(hidden)]
    pub fn long_line_narrowing_bounds(&self, pos: i64) -> Option<LongLineNarrowingBounds> {
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
        (beg < zv).then_some(LongLineNarrowingBounds {
            beg,
            zv,
            // GNU BEG is the buffer's first position; Z is one past its last
            // character. Neo stores the whole-buffer length independently of
            // the accessible restriction.
            buffer_start: 1,
            buffer_end: buffer.total_char_len().get() as i64 + 1,
        })
    }

    /// Enter GNU's large-narrowing region for a fontification call.
    ///
    /// GNU `handle_fontified_prop` (`src/xdisp.c:4702-4715`) does not centre
    /// on the fontified position: its iterator caches `large_narrowing_begv/zv`
    /// computed from `window_point` (`reseat`, src/xdisp.c:8008-8011), and
    /// recomputes at `IT_CHARPOS` only when the fontified position falls
    /// outside the cache — and that fallback is per call; it does not replace
    /// the iterator's window-point-centred cache. `window_bounds` is that
    /// cache (computed once per display walk from the window's point).
    ///
    /// Like GNU (`src/xdisp.c:4713`), no restriction is installed when the
    /// computed region covers the whole buffer.
    #[doc(hidden)]
    pub fn enter_long_line_narrowing_for_fontification(
        &mut self,
        window_bounds: Option<LongLineNarrowingBounds>,
        pos: i64,
        label: &'static str,
    ) -> Option<LongLineNarrowing> {
        let bounds = match window_bounds {
            Some(bounds) if pos >= bounds.beg && pos <= bounds.zv => bounds,
            _ => self.long_line_narrowing_bounds(pos)?,
        };
        self.enter_long_line_narrowing_bounds(bounds, label)
    }

    /// Enter GNU's large-narrowing region around `pos`, recomputing the
    /// bounds for every call. This is the command-hook shape
    /// (`src/keyboard.c:1983-2001`), where GNU narrows around point anew.
    #[doc(hidden)]
    pub fn enter_long_line_narrowing(
        &mut self,
        pos: i64,
        label: &'static str,
    ) -> Option<LongLineNarrowing> {
        let bounds = self.long_line_narrowing_bounds(pos)?;
        self.enter_long_line_narrowing_bounds(bounds, label)
    }

    /// Install the restriction for already-computed bounds. GNU skips the
    /// restriction entirely when it would cover the whole accessible region
    /// (`if (begv != BEGV || zv != ZV)`, src/xdisp.c:4713 and
    /// src/keyboard.c:1994), so the label is absent and no save/restore work
    /// happens.
    #[doc(hidden)]
    pub fn enter_long_line_narrowing_bounds(
        &mut self,
        bounds: LongLineNarrowingBounds,
        label: &'static str,
    ) -> Option<LongLineNarrowing> {
        if bounds.covers_whole_buffer() {
            return None;
        }
        let buffer_id = self.edit.buffers.current_buffer_id()?;
        let buffer = self.edit.buffers.get(buffer_id)?;
        let range = EmacsByteRange::new(
            buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new((bounds.beg - 1) as usize)),
            buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new((bounds.zv - 1) as usize)),
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
