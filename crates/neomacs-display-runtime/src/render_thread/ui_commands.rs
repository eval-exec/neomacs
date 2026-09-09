//! UI overlay, animation, and effect render commands.

use super::{RenderApp, TooltipState};
use crate::thread_comm::{ConfigCommand, ToolBarItem, UiCommand};
use neomacs_display_protocol::ToolBarImageSource;
use neomacs_display_protocol::{AxisSize, ImageColorContext, ImageRotation, ImageSizeSpec};

impl RenderApp {
    pub(super) fn ensure_toolbar_icon_textures(&mut self, items: &[ToolBarItem], icon_size: u32) {
        for item in items {
            if item.is_separator() {
                continue;
            }
            let Some(image) = item.image.as_ref() else {
                continue;
            };
            let key = (image.clone(), icon_size);
            if self.toolbar.icon_textures.contains_key(&key) {
                continue;
            }
            let Some(renderer) = self.renderer.as_mut() else {
                continue;
            };

            let id = match image {
                ToolBarImageSource::File { path } => renderer.load_image_file(
                    path,
                    ImageSizeSpec::new(AxisSize::AtMost(icon_size), AxisSize::AtMost(icon_size)),
                    ImageRotation::None,
                    ImageColorContext::default(),
                ),
            };
            self.toolbar.icon_textures.insert(key, id);
            tracing::debug!(
                "Loaded toolbar image '{}' as image_id={}",
                image.cache_key(),
                id
            );
        }
    }

    pub(super) fn handle_ui(&mut self, cmd: UiCommand) {
        match cmd {
            UiCommand::ShowPopupMenu {
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
            UiCommand::ShowTooltip {
                frame,
                x,
                y,
                text,
                fg_r,
                fg_g,
                fg_b,
                bg_r,
                bg_g,
                bg_b,
            } => {
                let emacs_frame_id = frame.raw_id();
                tracing::debug!("ShowTooltip frame=0x{:x} at ({}, {})", emacs_frame_id, x, y);
                let (fs, lh, cw, screen_w, screen_h) = self
                    .frame_windows
                    .get(emacs_frame_id)
                    .map(|window_state| {
                        let (fs, lh, cw) = window_state.render.font_metrics();
                        let (screen_w, screen_h) = window_state.native_size();
                        let scale = window_state.scale_factor() as f32;
                        (fs, lh, cw, screen_w as f32 / scale, screen_h as f32 / scale)
                    })
                    .or_else(|| {
                        if !self.frame_windows.is_primary_frame_id(emacs_frame_id) {
                            return None;
                        }
                        let (fs, lh, cw) = self
                            .frame_windows
                            .primary_window()
                            .map(|ws| &ws.render)
                            .map(|primary_frame| primary_frame.font_metrics())
                            .unwrap_or((13.0, 17.0, 13.0 * 0.6));
                        let (screen_w, screen_h) =
                            self.frame_windows
                                .primary_window()
                                .map_or((0.0, 0.0), |ws| {
                                    let (w, h) = ws.native_size();
                                    let s = ws.scale_factor() as f32;
                                    (w as f32 / s, h as f32 / s)
                                });
                        Some((fs, lh, cw, screen_w, screen_h))
                    })
                    .unwrap_or_else(|| {
                        let (screen_w, screen_h) =
                            self.frame_windows
                                .primary_window()
                                .map_or((0.0, 0.0), |ws| {
                                    let (w, h) = ws.native_size();
                                    let s = ws.scale_factor() as f32;
                                    (w as f32 / s, h as f32 / s)
                                });
                        (13.0, 17.0, 13.0 * 0.6, screen_w, screen_h)
                    });
                let tooltip = TooltipState::new(
                    x,
                    y,
                    &text,
                    (fg_r, fg_g, fg_b),
                    (bg_r, bg_g, bg_b),
                    screen_w,
                    screen_h,
                    fs,
                    lh,
                    cw,
                );
                if let Some(window_state) = self.frame_windows.get_mut(emacs_frame_id) {
                    window_state.render.set_tooltip(Some(tooltip));
                } else if self.frame_windows.is_primary_frame_id(emacs_frame_id) {
                    if let Some(ws) = self.frame_windows.primary_window_mut() {
                        ws.render.set_tooltip(Some(tooltip))
                    };
                } else {
                    tracing::warn!(
                        "ShowTooltip requested for unknown frame_id=0x{:x}",
                        emacs_frame_id
                    );
                }
            }
            UiCommand::HideTooltip => {
                tracing::debug!("HideTooltip");
                self.frame_windows.hide_top_level_tooltips();
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
