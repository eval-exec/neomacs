//! Mutable non-row frame output state owned while layout builds a frame snapshot.

use crate::output::install_request::{
    OutputCursorInstallRequest, OutputFrameArtifactInstallRequest, OutputFrameStateInstallRequest,
    OutputWindowMetadataInstallRequest,
};
use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::{
    ContentTransitionHint, PhysCursor, WindowEffectHint, WindowInfo,
};
use neomacs_display_protocol::glyph_matrix::{
    BackgroundItem, BorderItem, CursorItem, FaceFillItem, FrameDisplayState, ScrollBarItem,
};
use neomacs_display_protocol::types::{Color, DisplayFrameId, DisplayWindowId};
use std::collections::{HashMap, HashSet};

pub(crate) struct OutputFrameBuildState {
    backgrounds: Vec<BackgroundItem>,
    face_fills: Vec<FaceFillItem>,
    borders: Vec<BorderItem>,
    cursors: Vec<CursorItem>,
    scroll_bars: Vec<ScrollBarItem>,
    phys_cursor: Option<PhysCursor>,
    cursor_effects_by_window: HashMap<DisplayWindowId, EffectsConfig>,
    window_infos: Vec<WindowInfo>,
    pending_window_geometry: HashSet<DisplayWindowId>,
    transition_hints: Vec<ContentTransitionHint>,
    effect_hints: Vec<WindowEffectHint>,
    background_color: Color,
    font_pixel_size: f32,
    frame_id: DisplayFrameId,
    parent_id: DisplayFrameId,
    parent_x: f32,
    parent_y: f32,
    z_order: i32,
    undecorated: bool,
    border_width: f32,
    border_color: Color,
    outer_border_width: f32,
    outer_border_color: Color,
    background_alpha: f32,
    no_accept_focus: bool,
}

impl OutputFrameBuildState {
    pub(crate) fn new() -> Self {
        Self {
            backgrounds: Vec::new(),
            face_fills: Vec::new(),
            borders: Vec::new(),
            cursors: Vec::new(),
            scroll_bars: Vec::new(),
            phys_cursor: None,
            cursor_effects_by_window: HashMap::new(),
            window_infos: Vec::new(),
            pending_window_geometry: HashSet::new(),
            transition_hints: Vec::new(),
            effect_hints: Vec::new(),
            background_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            font_pixel_size: 0.0,
            frame_id: DisplayFrameId::new(0),
            parent_id: DisplayFrameId::new(0),
            parent_x: 0.0,
            parent_y: 0.0,
            z_order: 0,
            undecorated: false,
            border_width: 0.0,
            border_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            outer_border_width: 0.0,
            outer_border_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            background_alpha: 1.0,
            no_accept_focus: false,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.backgrounds.clear();
        self.face_fills.clear();
        self.borders.clear();
        self.cursors.clear();
        self.scroll_bars.clear();
        self.phys_cursor = None;
        self.cursor_effects_by_window.clear();
        self.window_infos.clear();
        self.pending_window_geometry.clear();
        self.transition_hints.clear();
        self.effect_hints.clear();
        self.background_color = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        self.font_pixel_size = 0.0;
        self.frame_id = DisplayFrameId::new(0);
        self.parent_id = DisplayFrameId::new(0);
        self.parent_x = 0.0;
        self.parent_y = 0.0;
        self.z_order = 0;
        self.undecorated = false;
        self.border_width = 0.0;
        self.border_color = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        self.outer_border_width = 0.0;
        self.outer_border_color = Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        self.background_alpha = 1.0;
        self.no_accept_focus = false;
    }

    pub(crate) fn install_window_metadata(&mut self, request: OutputWindowMetadataInstallRequest) {
        match request {
            OutputWindowMetadataInstallRequest::TextDisplayRange(range) => {
                if let Some(info) = self.window_infos.last_mut()
                    && info.window_id == range.window_id
                {
                    info.window_start = range.window_start;
                    info.window_end = range.window_end;
                }
            }
            OutputWindowMetadataInstallRequest::PresentedGeometry(geometry) => {
                if let Some(info) = self
                    .window_infos
                    .iter_mut()
                    .find(|info| info.window_id == geometry.window_id)
                {
                    info.geometry = geometry.geometry;
                    self.pending_window_geometry.remove(&geometry.window_id);
                    let regions = match geometry.geometry {
                        neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
                            regions,
                            ..
                        } => Some(regions),
                        neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Skipped {
                            ..
                        } => None,
                    };
                    info.tab_line_height = regions
                        .and_then(|regions| regions.tab_line)
                        .map_or(0.0, |rect| rect.height);
                    info.header_line_height = regions
                        .and_then(|regions| regions.header_line)
                        .map_or(0.0, |rect| rect.height);
                    info.mode_line_height = regions
                        .and_then(|regions| regions.mode_line)
                        .map_or(0.0, |rect| rect.height);
                }
            }
            OutputWindowMetadataInstallRequest::RestoreRetryCheckpoint(checkpoint) => {
                self.transition_hints
                    .truncate(checkpoint.transition_hints_len);
                self.effect_hints.truncate(checkpoint.effect_hints_len);
            }
        }
    }

    pub(crate) fn install_artifact(&mut self, request: OutputFrameArtifactInstallRequest) {
        match request {
            OutputFrameArtifactInstallRequest::Background { bounds, color } => {
                self.backgrounds.push(BackgroundItem { bounds, color });
            }
            OutputFrameArtifactInstallRequest::FaceFill(item) => {
                self.face_fills.push(item);
            }
            OutputFrameArtifactInstallRequest::Border {
                window_id,
                x,
                y,
                width,
                height,
                color,
            } => {
                self.borders.push(BorderItem {
                    window_id,
                    x,
                    y,
                    width,
                    height,
                    color,
                });
            }
            OutputFrameArtifactInstallRequest::ScrollBar(item) => self.scroll_bars.push(item),
            OutputFrameArtifactInstallRequest::WindowInfo(info) => {
                assert!(
                    self.window_infos
                        .iter()
                        .all(|existing| existing.window_id != info.window_id),
                    "duplicate output window identity"
                );
                self.pending_window_geometry.insert(info.window_id);
                self.window_infos.push(info);
            }
            OutputFrameArtifactInstallRequest::TransitionHint(hint) => {
                self.transition_hints.push(hint);
            }
            OutputFrameArtifactInstallRequest::EffectHint(hint) => self.effect_hints.push(hint),
            OutputFrameArtifactInstallRequest::PhysCursor(cursor) => {
                self.phys_cursor = Some(cursor)
            }
        }
    }

    pub(crate) fn install_cursor(&mut self, request: OutputCursorInstallRequest) {
        self.cursors.push(request.cursor_item());
    }

    pub(crate) fn install_frame_state(&mut self, request: OutputFrameStateInstallRequest) {
        match request {
            OutputFrameStateInstallRequest::Identity(identity) => {
                self.frame_id = identity.frame_id;
                self.parent_id = identity.parent_id;
                self.parent_x = identity.parent_x;
                self.parent_y = identity.parent_y;
                self.z_order = identity.z_order;
                self.undecorated = identity.undecorated;
                self.border_width = identity.border_width;
                self.border_color = identity.border_color;
                self.outer_border_width = identity.outer_border_width;
                self.outer_border_color = identity.outer_border_color;
                self.background_alpha = identity.background_alpha;
                self.no_accept_focus = identity.no_accept_focus;
            }
            OutputFrameStateInstallRequest::BackgroundColor(color) => self.background_color = color,
            OutputFrameStateInstallRequest::FontPixelSize(size) => self.font_pixel_size = size,
            OutputFrameStateInstallRequest::CursorEffects { window_id, effects } => {
                self.cursor_effects_by_window.insert(window_id, effects);
            }
        }
    }

    pub(crate) fn phys_cursor_mut(&mut self) -> Option<&mut PhysCursor> {
        self.phys_cursor.as_mut()
    }

    pub(crate) fn window_infos(&self) -> &[WindowInfo] {
        &self.window_infos
    }

    pub(crate) fn transition_hints(&self) -> &[ContentTransitionHint] {
        &self.transition_hints
    }

    pub(crate) fn effect_hints(&self) -> &[WindowEffectHint] {
        &self.effect_hints
    }

    pub(crate) fn background_color(&self) -> &Color {
        &self.background_color
    }

    pub(crate) fn cursors(&self) -> &[CursorItem] {
        &self.cursors
    }

    pub(crate) fn phys_cursor(&self) -> Option<&PhysCursor> {
        self.phys_cursor.as_ref()
    }

    pub(crate) fn install_into(self, state: &mut FrameDisplayState) {
        assert!(
            self.pending_window_geometry.is_empty(),
            "every output window must install complete or skipped presented geometry"
        );
        state.backgrounds = self.backgrounds;
        state.face_fills = self.face_fills;
        state.borders = self.borders;
        state.cursors = self.cursors;
        state.scroll_bars = self.scroll_bars;
        state.phys_cursor = self.phys_cursor;
        state.cursor_effects_by_window = self.cursor_effects_by_window;
        state.window_infos = self.window_infos;
        state.transition_hints = self.transition_hints;
        state.effect_hints = self.effect_hints;
        state.background = self.background_color;
        state.font_pixel_size = self.font_pixel_size;
        let parent = (self.parent_id.get() != 0).then_some(self.parent_id);
        let (x, y) = if parent.is_some() {
            (self.parent_x, self.parent_y)
        } else {
            (0.0, 0.0)
        };
        state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
            self.frame_id,
            state.presentation_id,
            parent,
            neomacs_display_protocol::ParentFrameRect::new(
                x,
                y,
                state.frame_pixel_width,
                state.frame_pixel_height,
            )
            .expect("output frame placement is valid"),
            self.z_order,
        );
        state.undecorated = self.undecorated;
        state.border_width = self.border_width;
        state.border_color = self.border_color;
        state.outer_border_width = self.outer_border_width;
        state.outer_border_color = self.outer_border_color;
        state.background_alpha = self.background_alpha;
        state.no_accept_focus = self.no_accept_focus;
    }
}
