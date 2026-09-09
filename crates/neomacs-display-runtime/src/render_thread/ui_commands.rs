//! UI overlay, animation, and effect render commands.

use super::RenderApp;
use crate::thread_comm::{ConfigCommand, UiCommand};

impl RenderApp {
    pub(super) fn handle_ui(&mut self, cmd: UiCommand) {
        match cmd {
            UiCommand::ShowPopupMenu {
                tooltips,
                request_id,
                token,
                frame,
                placement,
                items,
                title,
                fg,
                bg,
            } => {
                let emacs_frame_id = frame.raw_id();
                let anchor = placement.anchor();
                tracing::info!(
                    "ShowPopupMenu frame=0x{:x} anchor=({}, {}, {}, {}) side={:?} with {} items",
                    emacs_frame_id,
                    anchor.x,
                    anchor.y,
                    anchor.width,
                    anchor.height,
                    placement.preferred_side(),
                    items.len()
                );
                let owner = self.frame_windows.get(emacs_frame_id).or_else(|| {
                    self.frame_windows
                        .primary_window()
                        .filter(|_| self.frame_windows.is_primary_frame_id(emacs_frame_id))
                });
                if let Some(owner) = owner {
                    if let Some(parent) = owner.window() {
                        let anchor = placement.anchor();
                        let Some((x, y)) =
                            owner.render.surface_point_from_frame(anchor.x, anchor.y)
                        else {
                            self.comms
                                .send_input(crate::thread_comm::InputEvent::MenuSelection {
                                    index: -1,
                                    token: Some(token),
                                });
                            return;
                        };
                        let placement = neomacs_display_protocol::PopupPlacement::new(
                            neomacs_display_protocol::Rect::new(x, y, anchor.width, anchor.height),
                            placement.preferred_side(),
                            placement.offset(),
                            placement.constraint(),
                        );
                        let (fs, lh, cw) = owner.render.font_metrics();
                        let mut session =
                            crate::menus::MenuSession::new(0.0, 0.0, items, title, fs, lh, cw);
                        session.face_fg = fg;
                        session.face_bg = bg;
                        let mut fonts =
                            neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer::with_size(
                                0.0, 0.0,
                            );
                        if let Some(frame) = owner.render.compositor.current_frame.as_ref() {
                            fonts.clone_font_bindings_from(frame);
                        }
                        let accepted = self.menus.open(crate::menus::MenuRequest {
                            tooltips,
                            request_id,
                            token,
                            frame_id: emacs_frame_id,
                            parent: parent.clone(),
                            placement,
                            session,
                            fonts,
                        });
                        if accepted && let Some(owner) = self.frame_windows.get_mut(emacs_frame_id)
                        {
                            self.comms.tooltip_context.invalidate();
                            self.tooltips.hide();
                            owner.render.menu_opened();
                        }
                    } else {
                        self.comms
                            .send_input(crate::thread_comm::InputEvent::MenuSelection {
                                index: -1,
                                token: Some(token),
                            });
                    }
                } else {
                    tracing::warn!(
                        "ShowPopupMenu requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                    self.comms
                        .send_input(crate::thread_comm::InputEvent::MenuSelection {
                            index: -1,
                            token: Some(token),
                        });
                }
                while let Some(result) = self.menus.take_result() {
                    self.comms
                        .send_input(crate::thread_comm::InputEvent::MenuSelection {
                            index: result.index(),
                            token: Some(result.token),
                        });
                }
                self.sync_menu_heading();
            }
            UiCommand::HidePopupMenu { token } => {
                self.menus.hide(token);
                self.sync_menu_heading();
            }
            UiCommand::PresentTooltip {
                frame,
                request,
                ticket,
            } => {
                if !ticket.is_current() || self.menus.owner().is_some() {
                    ticket.cancel();
                    return;
                }
                let frame = frame.raw_id();
                let owner = self.frame_windows.get(frame).or_else(|| {
                    self.frame_windows
                        .primary_window()
                        .filter(|_| self.frame_windows.is_primary_frame_id(frame))
                });
                if let Some(owner) = owner {
                    if let Some(parent) = owner.window() {
                        let mut fonts =
                            neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer::with_size(
                                0.0, 0.0,
                            );
                        if let Some(frame) = owner.render.compositor.current_frame.as_ref() {
                            fonts.clone_font_bindings_from(frame);
                        }
                        let (x, y) = owner.render.mouse_pos;
                        self.tooltips.show(
                            crate::tooltips::TooltipOwner {
                                frame,
                                parent: parent.clone(),
                                anchor: neomacs_display_protocol::Rect::new(x, y, 1.0, 1.0),
                                metrics: owner.render.font_metrics(),
                                fonts,
                            },
                            request,
                            crate::tooltips::TooltipSource::Lisp(ticket),
                            neomacs_display_protocol::frame_time::observe_platform_now()
                                .into_instant(),
                        );
                    }
                }
            }
            UiCommand::DismissTooltip { ticket } => {
                self.tooltips.dismiss(&ticket);
            }
            UiCommand::VisualBell { frame } => {
                let emacs_frame_id = frame.raw_id();
                let now = neomacs_display_protocol::frame_time::observe_platform_now();
                let cursor_error_pulse_enabled = self.effects.cursor_error_pulse.enabled;
                let edge_snap_enabled = self.effects.edge_snap.enabled;
                let edge_snap_duration_ms = self.effects.edge_snap.duration_ms;
                if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
                    window_state.render.trigger_visual_bell(
                        cursor_error_pulse_enabled,
                        edge_snap_enabled,
                        edge_snap_duration_ms,
                        now,
                    );
                } else if self.frame_windows.is_primary_frame_id(emacs_frame_id) {
                    if let Some(ws) = self.frame_windows.primary_window_mut() {
                        ws.render.set_visual_bell_start(Some(now))
                    };
                } else {
                    tracing::warn!(
                        "VisualBell requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                }
            }
        }
    }

    pub(super) fn handle_config(&mut self, cmd: ConfigCommand) {
        match cmd {
            ConfigCommand::SetLigaturesEnabled { enabled } => {
                tracing::info!("Ligatures enabled: {}", enabled);
            }
            ConfigCommand::SetVisualConfig(config) => {
                self.requested_visual_config = config;
                self.apply_requested_visual_config();
                self.frame_windows.mark_top_level_dirty();
            }
            ConfigCommand::SetScrollIndicators { enabled } => {
                self.scroll_indicators_enabled = enabled;
                self.frame_windows.mark_top_level_dirty();
            }
            ConfigCommand::SetTitlebarHeight { height } => {
                self.frame_windows.set_top_level_titlebar_height(height);
            }
            ConfigCommand::SetShowFps { enabled } => {
                self.frame_windows.set_top_level_fps_enabled(enabled);
            }
            ConfigCommand::SetCornerRadius { radius } => {
                self.frame_windows.set_top_level_corner_radius(radius);
            }
            ConfigCommand::SetExtraSpacing {
                line_spacing,
                letter_spacing,
            } => {
                self.extra_line_spacing = line_spacing;
                self.extra_letter_spacing = letter_spacing;
                self.frame_windows.mark_top_level_dirty();
            }
            ConfigCommand::SetChildFrameStyle {
                corner_radius,
                shadow_enabled,
                shadow_layers,
                shadow_offset,
                shadow_opacity,
            } => {
                self.child_frame_style.corner_radius = corner_radius;
                self.child_frame_style.shadow_enabled = shadow_enabled;
                self.child_frame_style.shadow_layers = shadow_layers;
                self.child_frame_style.shadow_offset = shadow_offset;
                self.child_frame_style.shadow_opacity = shadow_opacity;
                self.frame_windows.mark_top_level_dirty();
            }
        }
    }
}
