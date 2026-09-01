//! Window and chrome render commands.

use super::RenderApp;
use crate::thread_comm::{InputEvent, WindowCommand};
use winit::dpi::PhysicalPosition;
use winit::window::{CursorIcon, UserAttentionType};

impl RenderApp {
    fn remove_pending_child_subtree(&mut self, frame_id: u64) {
        let mut subtree = std::collections::HashSet::from([frame_id]);
        loop {
            let before = subtree.len();
            for (&id, state) in &self.pending_child_frames {
                if state
                    .frame_placement
                    .parent()
                    .is_some_and(|parent| subtree.contains(&parent.get()))
                {
                    subtree.insert(id);
                }
            }
            if subtree.len() == before {
                break;
            }
        }
        self.pending_child_frames
            .retain(|id, _| !subtree.contains(id));
    }

    pub(super) fn handle_window(&mut self, cmd: WindowCommand) {
        match cmd {
            WindowCommand::SetWindowDecorations { decorated } => {
                if let Some(window) = self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.window())
                {
                    window.set_decorations(decorated);
                }
            }
            WindowCommand::SetMouseCursor { cursor_type } => {
                if let Some(window) = self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.window())
                {
                    if cursor_type == 0 {
                        window.set_cursor_visible(false);
                    } else {
                        window.set_cursor_visible(true);
                        let icon = match cursor_type {
                            2 => CursorIcon::Text,
                            3 => CursorIcon::Pointer,
                            4 => CursorIcon::Crosshair,
                            5 => CursorIcon::EwResize,
                            6 => CursorIcon::NsResize,
                            7 => CursorIcon::Wait,
                            8 => CursorIcon::NwseResize,
                            9 => CursorIcon::NeswResize,
                            10 => CursorIcon::NeswResize,
                            11 => CursorIcon::NwseResize,
                            _ => CursorIcon::Default,
                        };
                        window.set_cursor(icon);
                    }
                }
            }
            WindowCommand::WarpMouse { x, y } => {
                if let Some(window) = self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.window())
                {
                    let pos = PhysicalPosition::new(x as f64, y as f64);
                    let _ = window.set_cursor_position(pos);
                }
            }
            WindowCommand::SetWindowTitle { title } => {
                self.frame_windows
                    .primary_window_mut()
                    .expect("primary window state")
                    .chrome_mut()
                    .title = title.clone();
                if let Some(primary_state) = self.frame_windows.primary_window_mut() {
                    primary_state.set_title(title);
                    if !primary_state.chrome().decorations_enabled {
                        primary_state.render.mark_dirty();
                    }
                }
            }
            WindowCommand::SetFrameWindowTitle { frame, title } => {
                let emacs_frame_id = frame.raw_id();
                if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
                    window_state.set_title(title);
                } else if self.frame_windows.is_primary_frame_id(emacs_frame_id) {
                    self.frame_windows
                        .primary_window_mut()
                        .expect("primary window state")
                        .chrome_mut()
                        .title = title;
                } else {
                    tracing::warn!(
                        "SetFrameWindowTitle requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                }
            }
            WindowCommand::SetWindowFullscreen { frame, mode } => {
                let emacs_frame_id = frame.raw_id();
                if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
                    window_state.set_fullscreen_mode(mode);
                } else {
                    tracing::warn!(
                        "SetWindowFullscreen requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                }
            }
            WindowCommand::SetWindowMinimized { minimized } => {
                if let Some(window) = self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.window())
                {
                    window.set_minimized(minimized);
                }
            }
            WindowCommand::SetWindowPosition { x, y } => {
                if let Some(window) = self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.window())
                {
                    window.set_outer_position(PhysicalPosition::new(x, y));
                }
            }
            WindowCommand::SetWindowSize { width, height } => {
                tracing::debug!("WindowCommand::SetWindowSize {}x{}", width, height);
                if let Some(primary_state) = self.frame_windows.primary_window_mut() {
                    primary_state.request_inner_size(width, height);
                }
            }
            WindowCommand::ResizeWindow {
                frame,
                width,
                height,
                geometry_hints,
            } => {
                let emacs_frame_id = frame.raw_id();
                tracing::debug!(
                    "WindowCommand::ResizeWindow frame_id=0x{:x} {}x{}",
                    emacs_frame_id,
                    width,
                    height
                );
                if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
                    window_state.apply_geometry_hints(geometry_hints);
                    window_state.request_inner_size(width, height);
                } else {
                    tracing::warn!(
                        "ResizeWindow requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                }
            }
            WindowCommand::SetFrameGeometryHints {
                frame,
                geometry_hints,
            } => {
                let emacs_frame_id = frame.raw_id();
                tracing::debug!(
                    "WindowCommand::SetFrameGeometryHints frame_id=0x{:x} base={}x{} inc={}x{}",
                    emacs_frame_id,
                    geometry_hints.base_width,
                    geometry_hints.base_height,
                    geometry_hints.width_inc,
                    geometry_hints.height_inc
                );
                if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
                    window_state.apply_geometry_hints(geometry_hints);
                } else {
                    tracing::warn!(
                        "SetFrameGeometryHints requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                }
            }
            WindowCommand::SetWindowDecorated { decorated } => {
                self.frame_windows
                    .primary_window_mut()
                    .expect("primary window state")
                    .chrome_mut()
                    .decorations_enabled = decorated;
                self.frame_windows.set_top_level_decorations(decorated);
            }
            WindowCommand::RequestAttention { urgent } => {
                if let Some(window) = self
                    .frame_windows
                    .primary_window()
                    .and_then(|ws| ws.window())
                {
                    let attention = if urgent {
                        Some(UserAttentionType::Critical)
                    } else {
                        Some(UserAttentionType::Informational)
                    };
                    window.request_user_attention(attention);
                }
            }
            WindowCommand::CreateWindow {
                frame,
                width,
                height,
                title,
                geometry_hints,
            } => {
                let emacs_frame_id = frame.raw_id();
                tracing::info!(
                    "CreateWindow request: frame_id=0x{:x} {}x{} \"{}\"",
                    emacs_frame_id,
                    width,
                    height,
                    title
                );
                self.frame_windows.request_create(
                    emacs_frame_id,
                    width,
                    height,
                    title,
                    geometry_hints,
                );
            }
            WindowCommand::DestroyWindow { frame } => {
                let emacs_frame_id = frame.raw_id();
                tracing::info!("DestroyWindow request: frame_id=0x{:x}", emacs_frame_id);
                let mut retirements = Vec::new();
                if let Some(window) = self.frame_windows.get_mut(emacs_frame_id) {
                    // Destruction cancels capture first, flushing any older
                    // pinned generation before retiring buffers still shown.
                    retirements.extend(window.render.cancel_pointer_interaction().1);
                    let mut presentations: Vec<_> = window
                        .render
                        .displayed_presentations()
                        .into_iter()
                        .collect();
                    presentations.sort_unstable();
                    for presentation in presentations {
                        if let Some(presentation) =
                            window.render.route_presentation_retirement(presentation)
                        {
                            retirements.push(presentation);
                        }
                    }
                }
                for presentation in retirements {
                    self.comms
                        .send_input(InputEvent::PresentationRetired { presentation });
                }
                if self.frame_windows.is_primary_frame_id(emacs_frame_id) {
                    self.frame_windows.take_primary_window();
                    self.frame_windows.clear_primary_mapping();
                } else {
                    self.frame_windows.request_destroy(emacs_frame_id);
                }
            }
            WindowCommand::AdoptPrimaryFrame { frame } => {
                let emacs_frame_id = frame.raw_id();
                tracing::info!("AdoptPrimaryFrame request: frame_id=0x{:x}", emacs_frame_id);
                self.frame_windows.adopt_primary_frame_id(emacs_frame_id);
                // The window's scheduling identity moves from the pending id
                // (0) to the adopted Emacs frame id; retire the old entry so
                // its deadlines and request token cannot go stale.
                self.frame_coordinator
                    .remove_window(super::frame_sched::NativeWindowId(0));
            }
            WindowCommand::ShowChildFrame { frame_id } => {
                tracing::info!(
                    frame_id,
                    "child_frame_lifecycle: render_thread_show_command"
                );
                self.frame_windows
                    .show_child_frame_in_top_level_windows(frame_id);
            }
            WindowCommand::RemoveChildFrame { frame_id } => {
                tracing::info!(
                    frame_id,
                    "child_frame_lifecycle: render_thread_remove_command"
                );
                let mut retirements = Vec::new();
                self.frame_windows.for_each_top_level_window_mut(|window| {
                    for presentation in window.render.child_subtree_presentations(frame_id) {
                        if let Some(presentation) =
                            window.render.route_presentation_retirement(presentation)
                        {
                            retirements.push(presentation);
                        }
                    }
                });
                self.frame_windows
                    .remove_child_frame_from_top_level_windows(frame_id);
                self.remove_pending_child_subtree(frame_id);
                for presentation in retirements {
                    self.comms
                        .send_input(InputEvent::PresentationRetired { presentation });
                }
            }
            WindowCommand::ScrollBlit { .. } => {
                // handled above in dispatch, here as exhaustive match
            }
        }
    }
}
