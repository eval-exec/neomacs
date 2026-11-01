//! The minibuffer's window: GNU's `minibuf_window` and the active-minibuffer
//! window queries (`src/minibuf.c`, `src/window.c:103`).
//!
//! GNU keeps `minibuf_window` as one C global that `choose_minibuf_frame`,
//! `move_minibuffers_onto_frame` and `set-minibuffer-window` repoint; Neomacs
//! derives the same window from the active minibuffer session and the
//! selected frame.  These are `Context` methods on the minibuffer-window
//! domain, kept out of the `eval` facade like the other domain children
//! (`use super::*` keeps the parent's view of `Context` and its imports).

use super::*;

impl Context {
    pub fn minibuffer_is_active(&self) -> bool {
        self.minibuffers.is_active()
    }

    pub fn active_minibuffer_window_id(&self) -> Option<WindowId> {
        if let Some(wid) = *self.active_minibuffer_window {
            return Some(wid);
        }
        let state = self.minibuffers.current()?;

        for frame_id in self.frames.frame_list() {
            let Some(frame) = self.frames.get(frame_id) else {
                continue;
            };
            if let Some(minibuffer_wid) = frame.minibuffer_window
                && let Some(window) = frame.find_window(minibuffer_wid)
                && window.buffer_id() == Some(state.buffer_id)
            {
                return Some(minibuffer_wid);
            }
        }
        None
    }

    pub fn minibuffer_window_is_active(&self, window_id: WindowId) -> bool {
        self.active_minibuffer_window_id() == Some(window_id)
    }

    /// GNU's `minibuf_window` (`src/window.c:103`): the window that displays
    /// minibuffer contents.
    ///
    /// GNU keeps this as one global slot. `choose_minibuf_frame` points it at
    /// the selected frame's minibuffer window when a minibuffer starts or
    /// exits (`src/minibuf.c:115-131`), `move_minibuffers_onto_frame` follows
    /// frame switches, and `set-minibuffer-window` can point it at any live
    /// minibuffer window (`src/minibuf.c:240-252`). Neomacs has no such slot:
    /// while a minibuffer session is active its display window is the active
    /// minibuffer window, and otherwise the target is the selected frame's
    /// minibuffer window.
    ///
    /// Known gaps, both pre-existing: an explicit `set-minibuffer-window`
    /// target is not remembered (that builtin notes the missing slot), and a
    /// selected frame without a minibuffer window (a child or tooltip frame)
    /// does not inherit the previous target the way GNU's global does.
    pub(crate) fn minibuf_window_id(&self) -> Option<WindowId> {
        self.active_minibuffer_window_id().or_else(|| {
            self.frames
                .selected_frame()
                .and_then(|frame| frame.minibuffer_window)
        })
    }

    /// Window that invoked the currently active minibuffer.
    ///
    /// GNU keeps this window's mode/header line active while the minibuffer
    /// owns input selection (`minibuffer-selected-window`).
    pub fn minibuffer_selected_window_id(&self) -> Option<WindowId> {
        // GNU window.c returns the saved caller only while a minibuffer is
        // selected, at positive minibuffer depth, and the caller is live.
        // Keep the saved identity for nested reads/window-configuration
        // restoration; deletion invalidates the query, not the session.
        if !self.minibuffer_is_active() {
            return None;
        }
        let selected_frame = self.frames.selected_frame()?;
        if selected_frame.minibuffer_window != Some(selected_frame.selected_window) {
            return None;
        }
        let caller = (*self.minibuffer_selected_window)?;
        crate::emacs_core::window_cmds::WindowDomain::Live.frame_of(&self.frames, caller)?;
        Some(caller)
    }

    pub fn activate_minibuffer_window_for_buffer(
        &mut self,
        minibuf_id: BufferId,
        prompt: crate::heap_types::LispString,
        initial_input: Option<crate::heap_types::LispString>,
    ) -> Result<Option<WindowId>, Flow> {
        let entry_level = {
            let state = self.minibuffers.read_from_minibuffer_lisp(
                minibuf_id,
                &prompt,
                initial_input.as_ref(),
                None,
            )?;
            let depth = std::num::NonZeroUsize::new(state.depth)
                .expect("an active minibuffer has nonzero depth");
            crate::emacs_core::minibuffer::MinibufferEntryLevel::from_depth(depth)
        };

        let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
            &mut self.frames,
            &mut self.buffers,
        );
        let Some(frame) = self.frames.get(frame_id) else {
            self.buffers.switch_current(minibuf_id);
            return Ok(None);
        };
        let Some(minibuffer_window_id) = frame.minibuffer_window else {
            self.buffers.switch_current(minibuf_id);
            return Ok(None);
        };
        let previous_selected_window = frame.selected_window;
        if super::gnu_redisplay_hooks_enabled() && frame.find_window(minibuffer_window_id).is_some()
        {
            // GNU minibuf.c:828 publishes set_window_buffer before normal
            // Fselect_window; no Lisp executes until this transition commits.
            self.gnu_mark_window_mode_line(minibuffer_window_id);
            if previous_selected_window != minibuffer_window_id {
                self.gnu_mark_selection(Some(previous_selected_window), minibuffer_window_id, true);
            }
        }

        crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
            &mut self.frames,
            &mut self.buffers,
            frame_id,
        );
        if let Some(frame) = self.frames.get_mut(frame_id) {
            if let Some(window) = frame.find_window_mut(minibuffer_window_id) {
                window.set_buffer(minibuf_id);
                if super::gnu_redisplay_hooks_enabled() {
                    if let crate::window::Window::Leaf { hscroll, .. } = window {
                        *hscroll = 0;
                    }
                    window.set_suspend_auto_hscroll(false);
                }
                crate::window::window_markers::attach_window_position_markers(
                    &mut self.buffers,
                    window,
                );
            }
            let _ = frame.select_window(minibuffer_window_id);
        }
        self.buffers.switch_current(minibuf_id);
        crate::emacs_core::reader::MinibufferSelectedWindowUpdate::for_entry(
            entry_level,
            previous_selected_window,
            minibuffer_window_id,
        )
        .apply(&mut self.minibuffer_selected_window);
        *self.active_minibuffer_window = Some(minibuffer_window_id);
        Ok(Some(minibuffer_window_id))
    }
}
