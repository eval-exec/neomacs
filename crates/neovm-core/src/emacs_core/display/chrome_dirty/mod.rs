//! Which windows must re-generate their chrome (mode / header / tab line).
//!
//! This is the port of GNU's mode-line dirty flags. GNU keeps two, and the
//! distinction matters because they have different reach:
//!
//! * `update_mode_lines` — a global (`xdisp.c:901-907`, set by
//!   `bset_update_mode_line`). Everything buffer-scoped raises it, because
//!   a buffer can be shown in several windows and GNU does not track which.
//! * `w->update_mode_line` — per window (`xdisp.c:909-920`, set by
//!   `wset_update_mode_line`), raised by the window-scoped events:
//!   `set-window-start`, `set-window-buffer`, the scroll commands.
//!
//! Both feed the same two decisions. The one that matters for cost is that
//! they are *preconditions of GNU's one-line optimization*
//! (`xdisp.c:17572-17610`): while they are clear, an edit confined to one
//! line re-displays that glyph row and jumps straight to the update phase
//! (`goto update`, `:17726`), never entering `redisplay_window` and so never
//! reaching `display_mode_lines`. That, not the guard chain inside
//! `redisplay_window`, is why GNU does not re-walk the mode line on every
//! keystroke. The full extraction is in `tmp/p52-gnu-extraction.md`.
//!
//! Neomacs's analogue of that optimization is the cursor-only / edit-replay
//! fast path, and since P5.2(b) that path consults these flags as one of its
//! preconditions (`buffer_source/render_plan.rs`).
//!
//! GNU also sets `prevent_redisplay_optimizations_p` alongside the flag in
//! several places (notably `Fforce_mode_line_update`). We do not model that
//! separately: in GNU it exists to disqualify optimizations that do not
//! consult `update_mode_line` itself, whereas here the chrome flag is a
//! precondition of the fast path directly.
//!
//! # Why acknowledgement is per window
//!
//! P5.2(a) cleared every flag at the end of each accepted layout, which is
//! correct only while every window regenerates its chrome unconditionally.
//! Once a window may *skip*, a blanket clear is a staleness bug with two
//! shapes: a window that skipped would have its own outstanding flag eaten,
//! and — the one that survives even a same-frame fix — laying out frame A
//! would clear a flag raised for frame B's windows, which were never visited.
//!
//! So the global flag is modelled as a GENERATION rather than a boolean, and
//! each window records the generation it last generated chrome at
//! ([`Self::note_chrome_generated`]). A window is dirty when it carries its
//! own mark or when it has not yet acknowledged the current global
//! generation, which makes "never laid out" and "laid out before the flag was
//! raised" the same state — both dirty, both correct.
use crate::window::WindowId;
use std::collections::{HashMap, HashSet};

/// The set of windows whose chrome must be re-generated.
#[derive(Debug, Default, Clone)]
pub struct ChromeDirty {
    /// GNU `update_mode_lines`, as a generation: every window is dirty until
    /// it acknowledges this value. Bumping it is the "all windows" mark.
    all_generation: u64,
    /// GNU `w->update_mode_line`: these windows specifically.
    windows: HashSet<WindowId>,
    /// The `all_generation` each window last generated chrome at. A window
    /// with no entry has never generated chrome and is therefore dirty.
    acknowledged: HashMap<WindowId, u64>,
}

impl ChromeDirty {
    /// GNU `bset_update_mode_line` (`xdisp.c:901-907`): a buffer-scoped event
    /// raises the global flag, because the buffer may be shown in windows
    /// this call cannot enumerate.
    pub fn mark_all(&mut self) {
        self.all_generation = self.all_generation.wrapping_add(1);
    }

    /// GNU `wset_update_mode_line` (`xdisp.c:909-920`): a window-scoped event.
    pub fn mark_window(&mut self, window: WindowId) {
        self.windows.insert(window);
    }

    /// Whether WINDOW must re-generate its chrome this redisplay.
    pub fn is_dirty(&self, window: WindowId) -> bool {
        self.windows.contains(&window)
            || self.acknowledged.get(&window) != Some(&self.all_generation)
    }

    /// Whether any window known to this set is still outstanding.
    ///
    /// "Known" is the honest qualifier: a window that has never generated
    /// chrome has no acknowledgement entry and cannot be enumerated here, so
    /// this answers the question for windows redisplay has already visited.
    /// [`Self::is_dirty`] is the per-window authority and does treat an
    /// unknown window as dirty.
    pub fn is_any_dirty(&self) -> bool {
        !self.windows.is_empty()
            || self
                .acknowledged
                .values()
                .any(|generation| *generation != self.all_generation)
    }

    /// Called by redisplay for each window whose chrome it actually
    /// generated. GNU's analogue is `mark_window_display_accurate_1` clearing
    /// `w->update_mode_line`, plus `update_mode_lines` being reset once
    /// `redisplay_internal` has made a pass.
    ///
    /// A window that SKIPPED its chrome must not be passed here — its flag has
    /// not been honored and has to survive into the next redisplay.
    pub fn note_chrome_generated(&mut self, window: WindowId) {
        self.windows.remove(&window);
        self.acknowledged.insert(window, self.all_generation);
    }

    /// Drop a deleted window's acknowledgement so the table does not grow
    /// without bound across a session's window churn.
    pub fn forget_window(&mut self, window: WindowId) {
        self.windows.remove(&window);
        self.acknowledged.remove(&window);
    }
}
