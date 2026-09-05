use crate::display_face_policy::EffectiveWindowDefaultFace;
use crate::display_status_line::{
    ChromeRowRenderServices, FrameChromeOutputTarget, FrameTabBarDisplayRowRender,
    FrameTabBarDisplayRowRenderState, FrameTabBarDisplayRowRequest,
};
use crate::display_text_output_install::install_output_resolved_face;
use crate::display_text_window_row_lifecycle::TextWindowTerminalRightBorderRequest;
use crate::font::metrics::FontMetrics;
use crate::frame_face_arena::FrameFaceAttempt;
use crate::neovm_bridge::ResolvedFace;
use crate::output::builder::DisplayOutputBuilder;
use crate::types::{FrameParams, WindowParams};
use crate::window_output::TextWindowOutputTarget;
use neomacs_display_protocol::frame_chrome::{
    ChromeBandRequest, ChromeLayoutError, FrameChrome, FrameSize, PresentationId,
};
use neomacs_display_protocol::frame_glyphs::{
    BufferTransitionTarget, BufferViewportRegion, ContentTransitionHint, GlyphRowRole,
    PresentedCellOrigin as ProtocolCellOrigin, PresentedWindowGeometry as ProtocolWindowGeometry,
    WindowEffectHint, WindowInfo, derive_buffer_replacement_hint,
};
use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, ScrollBarItem};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, DisplayWindowId, Rect};
use neomacs_display_protocol::{ContentTransitionIntent, TransitionDirection};
use neovm_core::emacs_core::eval::DisplayHost;
use neovm_core::window::PresentedWindowRegions;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowFrameMetadata {
    pub(crate) buffer_name: String,
    pub(crate) buffer_file_name: String,
    pub(crate) modified: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowFrameGeometry {
    pub(crate) right_edge: f32,
    pub(crate) bottom_edge: f32,
    pub(crate) is_rightmost: bool,
    pub(crate) is_bottommost: bool,
    pub(crate) reserve_terminal_right_border_col: bool,
}

pub(crate) struct WindowFrameGeometryRequest<'a> {
    params: &'a WindowParams,
    frame_params: &'a FrameParams,
    main_area_bottom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameOutputIdentity {
    pub(crate) frame_id: u64,
    pub(crate) parent_id: u64,
    pub(crate) parent_x: f32,
    pub(crate) parent_y: f32,
    pub(crate) z_order: i32,
    pub(crate) undecorated: bool,
    pub(crate) border_width: f32,
    pub(crate) border_color: Color,
    pub(crate) outer_border_width: f32,
    pub(crate) outer_border_color: Color,
    pub(crate) background_alpha: f32,
    pub(crate) no_accept_focus: bool,
}

pub(crate) struct FrameOutputOwner {
    builder: DisplayOutputBuilder,
    pending_frame_chrome: Vec<ChromeBandRequest>,
    presentation_id: PresentationId,
}

pub(crate) struct FrameOutputTarget<'a> {
    builder: &'a mut DisplayOutputBuilder,
}

struct FrameOutputSession<'builder, 'chrome> {
    builder: &'builder mut DisplayOutputBuilder,
    pending_frame_chrome: &'chrome mut Vec<ChromeBandRequest>,
}

impl FrameOutputOwner {
    pub(crate) fn new() -> Self {
        Self {
            builder: DisplayOutputBuilder::new(),
            pending_frame_chrome: Vec::new(),
            presentation_id: PresentationId::default(),
        }
    }

    fn session(&mut self) -> FrameOutputSession<'_, '_> {
        FrameOutputSession::new(&mut self.builder, &mut self.pending_frame_chrome)
    }

    pub(crate) fn text_window_output_target(&mut self) -> TextWindowOutputTarget<'_> {
        TextWindowOutputTarget::from_builder(&mut self.builder)
    }

    fn frame_output_target(&mut self) -> FrameOutputTarget<'_> {
        FrameOutputTarget::from_builder(&mut self.builder)
    }

    pub(crate) fn reset(&mut self) {
        self.session().reset();
    }

    pub(crate) fn set_face_attempt(&mut self, face_attempt: FrameFaceAttempt) {
        self.builder.set_face_attempt(face_attempt);
    }

    pub(crate) fn finish(
        &mut self,
        frame_params: &FrameParams,
    ) -> Result<FrameDisplayState, ChromeLayoutError> {
        let presentation_id = self.presentation_id;
        let mut state = self.session().finish(frame_params)?;
        state.presentation_id = presentation_id;
        let placement = state.frame_placement;
        state.frame_placement = neomacs_display_protocol::PresentedFramePlacement::new(
            placement.frame(),
            presentation_id,
            placement.parent(),
            placement.outer_in_parent(),
            placement.z_order(),
        );
        Ok(state)
    }

    pub(crate) fn add_frame_chrome_band(&mut self, request: ChromeBandRequest) {
        self.pending_frame_chrome.push(request);
    }

    pub(crate) fn set_presentation_id(&mut self, presentation: u64) {
        self.presentation_id = PresentationId::new(presentation);
    }

    pub(crate) fn install_pointer_face(
        &mut self,
        face_id: neomacs_display_protocol::types::FaceId,
        face: neomacs_display_protocol::face::Face,
    ) {
        self.builder.publish_output_face(face_id, face);
    }

    pub(crate) fn render_frame_tab_bar_row(
        &mut self,
        request: FrameTabBarDisplayRowRequest<'_>,
        render_services: ChromeRowRenderServices<'_, '_>,
        display_host: Option<&dyn DisplayHost>,
    ) -> Option<FrameTabBarDisplayRowRender> {
        request.render(&mut FrameTabBarDisplayRowRenderState::new(
            FrameChromeOutputTarget::from_builder(&mut self.builder),
            render_services,
            display_host,
        ))
    }

    pub(crate) fn latest_window_info(&self, window_id: i64) -> Option<WindowInfo> {
        let window_id = DisplayWindowId::new(window_id);
        self.builder
            .window_infos()
            .iter()
            .rev()
            .find(|info| info.window_id == window_id)
            .cloned()
    }

    pub(crate) fn window_content_height_px(
        &self,
        window_id: i64,
        fallback_row_height: f32,
    ) -> Option<f32> {
        self.builder
            .window_content_height_px(window_id, fallback_row_height)
    }

    pub(crate) fn render_frame_state(&mut self, request: FrameOutputStateRenderRequest<'_>) {
        request.render_and_apply(self.frame_output_target());
    }

    pub(crate) fn render_window_info(&mut self, request: WindowFrameInfoRenderRequest<'_>) {
        request.render_and_apply(self.frame_output_target());
    }

    pub(crate) fn publish_window_geometry(
        &mut self,
        window_id: i64,
        left_col: i64,
        top_line: i64,
        regions: &PresentedWindowRegions,
        materialized: bool,
    ) {
        let cell_origin = ProtocolCellOrigin {
            column: left_col,
            line: top_line,
        };
        let geometry = if materialized {
            ProtocolWindowGeometry::Complete {
                cell_origin,
                regions: *regions,
            }
        } else {
            ProtocolWindowGeometry::Skipped {
                cell_origin,
                outer: regions.outer,
            }
        };
        self.builder.install_window_metadata(
            crate::output::install_request::OutputPresentedWindowGeometryInstallRequest {
                window_id: DisplayWindowId::new(window_id),
                geometry,
            },
        );
    }

    pub(crate) fn render_latest_window_info_effects(
        &mut self,
        request: WindowFrameInfoEffectsRenderRequest<'_>,
        curr_window_infos: &mut HashMap<DisplayWindowId, WindowInfo>,
    ) -> NavigationIntentObservation {
        request.render_latest_and_apply(self.frame_output_target(), curr_window_infos)
    }

    pub(crate) fn render_window_decorations(
        &mut self,
        request: WindowFrameDecorationsRenderRequest<'_>,
        render_services: ChromeRowRenderServices<'_, '_>,
    ) {
        request.render_and_apply(self.frame_output_target(), render_services);
    }

    pub(crate) fn render_line_animation_hints(
        &mut self,
        request: FrameLineAnimationHintsRenderRequest<'_>,
    ) {
        request.render_and_apply(self.frame_output_target());
    }

    pub(crate) fn render_frame_content_transition_hint(
        &mut self,
        request: FrameContentTransitionHintRenderRequest<'_>,
    ) -> NavigationIntentObservation {
        request.render_and_apply(self.frame_output_target())
    }
}

impl<'a> FrameOutputTarget<'a> {
    pub(crate) fn from_builder(builder: &'a mut DisplayOutputBuilder) -> Self {
        Self { builder }
    }

    fn reborrow(&mut self) -> FrameOutputTarget<'_> {
        FrameOutputTarget {
            builder: self.builder,
        }
    }

    fn builder(&mut self) -> &mut DisplayOutputBuilder {
        self.builder
    }

    fn text_window_output_target(&mut self) -> TextWindowOutputTarget<'_> {
        TextWindowOutputTarget::from_builder(self.builder())
    }

    fn set_frame_identity(&mut self, identity: FrameOutputIdentity) {
        self.builder.set_output_frame_identity(
            identity.frame_id,
            identity.parent_id,
            identity.parent_x,
            identity.parent_y,
            identity.z_order,
            identity.undecorated,
            identity.border_width,
            identity.border_color,
            identity.outer_border_width,
            identity.outer_border_color,
            identity.background_alpha,
            identity.no_accept_focus,
        );
    }

    fn set_background_color(&mut self, color: Color) {
        self.builder.set_output_background_color(color);
    }

    fn background_color(&self) -> Color {
        *self.builder.background_color()
    }

    fn set_font_pixel_size(&mut self, font_pixel_size: f32) {
        self.builder.set_output_font_pixel_size(font_pixel_size);
    }

    fn install_resolved_face(
        &mut self,
        face_id: FaceId,
        face: &ResolvedFace,
        metrics: Option<FontMetrics>,
    ) {
        install_output_resolved_face(self.builder(), face_id, face, metrics);
    }

    fn add_background(&mut self, bounds: Rect, color: Color) {
        self.builder.add_output_background(bounds, color);
    }

    fn add_window_info(&mut self, info: WindowInfo) {
        self.builder.add_output_window_info(info);
    }

    fn latest_window_info(&self) -> Option<WindowInfo> {
        self.builder.window_infos().last().cloned()
    }

    fn window_infos(&self) -> &[WindowInfo] {
        self.builder.window_infos()
    }

    fn add_transition_hint(&mut self, hint: ContentTransitionHint) {
        self.builder.add_output_transition_hint(hint);
    }

    fn transition_hints(&self) -> &[ContentTransitionHint] {
        self.builder.transition_hints()
    }

    fn add_effect_hint(&mut self, hint: WindowEffectHint) {
        self.builder.add_output_effect_hint(hint);
    }

    fn add_border(
        &mut self,
        window_id: i64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    ) {
        self.builder
            .add_output_border(window_id, x, y, width, height, color);
    }

    fn add_scroll_bar(&mut self, item: ScrollBarItem) {
        self.builder.add_output_scroll_bar(item);
    }

    fn window_cursor_y(&self, info: &WindowInfo) -> Option<f32> {
        let in_window = |x: f32, y: f32, hollow: bool| -> bool {
            !hollow
                && x >= info.bounds.x
                && x < info.bounds.x + info.bounds.width
                && y >= info.bounds.y
                && y < info.bounds.y + info.bounds.height
        };
        if let Some(phys) = self.builder.phys_cursor()
            && in_window(phys.x, phys.y, phys.style.is_hollow())
        {
            return Some(phys.y);
        }
        for cursor in self.builder.cursors() {
            if in_window(cursor.x, cursor.y, cursor.style.is_hollow()) {
                return Some(cursor.y);
            }
        }
        None
    }
}

impl<'builder, 'chrome> FrameOutputSession<'builder, 'chrome> {
    fn new(
        builder: &'builder mut DisplayOutputBuilder,
        pending_frame_chrome: &'chrome mut Vec<ChromeBandRequest>,
    ) -> Self {
        Self {
            builder,
            pending_frame_chrome,
        }
    }

    fn reset(&mut self) {
        self.builder.reset();
        self.pending_frame_chrome.clear();
    }

    fn finish(self, frame_params: &FrameParams) -> Result<FrameDisplayState, ChromeLayoutError> {
        let frame_cols = (frame_params.width / frame_params.char_width.max(1.0)) as usize;
        let frame_rows = (frame_params.height / frame_params.char_height.max(1.0)) as usize;
        let display_output_builder = std::mem::replace(self.builder, DisplayOutputBuilder::new());
        let mut frame_display_state = display_output_builder.finish_with_pixel_size(
            frame_cols,
            frame_rows,
            frame_params.char_width,
            frame_params.char_height,
            frame_params.width,
            frame_params.height,
        );
        frame_display_state.frame_chrome = FrameChrome::layout(
            FrameSize::new(frame_params.width, frame_params.height)?,
            std::mem::take(self.pending_frame_chrome),
        )?;

        Ok(frame_display_state)
    }
}

pub(crate) struct FrameOutputStateRenderRequest<'a> {
    identity: Option<FrameOutputIdentity>,
    background_color: Color,
    font_pixel_size: f32,
    default_face: &'a ResolvedFace,
    default_metrics: Option<FontMetrics>,
}

impl<'a> FrameOutputStateRenderRequest<'a> {
    pub(crate) fn new(
        identity: Option<FrameOutputIdentity>,
        background_color: Color,
        font_pixel_size: f32,
        default_face: &'a ResolvedFace,
        default_metrics: Option<FontMetrics>,
    ) -> Self {
        Self {
            identity,
            background_color,
            font_pixel_size,
            default_face,
            default_metrics,
        }
    }

    pub(crate) fn render_and_apply(self, mut state: FrameOutputTarget<'_>) {
        if let Some(identity) = self.identity {
            state.set_frame_identity(identity);
        }
        state.set_background_color(self.background_color);
        state.set_font_pixel_size(self.font_pixel_size);
        state.install_resolved_face(FaceId::new(0), self.default_face, self.default_metrics);
    }
}

impl<'a> WindowFrameGeometryRequest<'a> {
    pub(crate) fn new(
        params: &'a WindowParams,
        frame_params: &'a FrameParams,
        main_area_bottom: f32,
    ) -> Self {
        Self {
            params,
            frame_params,
            main_area_bottom,
        }
    }

    pub(crate) fn resolve(self) -> WindowFrameGeometry {
        let right_edge = self.params.bounds.x + self.params.bounds.width;
        let bottom_edge = self.params.bounds.y + self.params.bounds.height;
        let is_rightmost = right_edge >= self.frame_params.width - 1.0;
        let is_bottommost =
            self.params.is_minibuffer() || bottom_edge >= self.main_area_bottom - 1.0;
        let reserve_terminal_right_border_col = !self.frame_params.window_system
            && self.frame_params.right_divider_width == 0
            && !is_rightmost
            && !self.params.is_minibuffer();

        WindowFrameGeometry {
            right_edge,
            bottom_edge,
            is_rightmost,
            is_bottommost,
            reserve_terminal_right_border_col,
        }
    }
}

pub(crate) struct WindowFrameInfoRenderRequest<'a> {
    params: &'a WindowParams,
    metadata: WindowFrameMetadata,
}

impl<'a> WindowFrameInfoRenderRequest<'a> {
    pub(crate) fn new(params: &'a WindowParams, metadata: WindowFrameMetadata) -> Self {
        Self { params, metadata }
    }

    pub(crate) fn render_and_apply(self, mut state: FrameOutputTarget<'_>) {
        state.add_background(
            self.params.bounds,
            Color::from_pixel(self.params.default_bg),
        );
        state.add_window_info(WindowInfo {
            window_id: DisplayWindowId::new(self.params.window_id),
            buffer_id: self.params.buffer_id,
            window_start: self.params.window_start,
            window_end: 0,
            buffer_size: self.params.buffer_size,
            buffer_modiff: neomacs_display_protocol::presentation_origin::BufferModiff::new(
                self.params.buffer_modiff as u64,
            ),
            bounds: Rect::new(
                self.params.bounds.x,
                self.params.bounds.y,
                self.params.bounds.width,
                self.params.bounds.height,
            ),
            geometry: ProtocolWindowGeometry::default(),
            mode_line_height: self.params.mode_line_height,
            header_line_height: self.params.header_line_height,
            tab_line_height: self.params.tab_line_height,
            selected: self.params.selected,
            is_minibuffer: self.params.is_minibuffer(),
            char_height: self.params.char_height,
            buffer_name: self.metadata.buffer_name,
            buffer_file_name: self.metadata.buffer_file_name,
            modified: self.metadata.modified,
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowContentTransitionMode {
    PerWindow {
        navigation: Option<TransitionDirection>,
    },
    SuppressedByFrameNavigation {
        superseded_navigation: Option<TransitionDirection>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum NavigationIntentObservation {
    None,
    TransitionEmitted(TransitionDirection),
    RetiredWithoutTransition(TransitionDirection),
    SupersededByFrameNavigation(TransitionDirection),
}

impl NavigationIntentObservation {
    const fn retired(navigation: Option<TransitionDirection>) -> Self {
        match navigation {
            Some(direction) => Self::RetiredWithoutTransition(direction),
            None => Self::None,
        }
    }

    pub(crate) const fn direction_to_acknowledge(self) -> Option<TransitionDirection> {
        match self {
            Self::None => None,
            Self::TransitionEmitted(direction)
            | Self::RetiredWithoutTransition(direction)
            | Self::SupersededByFrameNavigation(direction) => Some(direction),
        }
    }
}

pub(crate) struct WindowFrameInfoEffectsRenderRequest<'a> {
    prev_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
    content_transition_mode: WindowContentTransitionMode,
}

impl<'a> WindowFrameInfoEffectsRenderRequest<'a> {
    pub(crate) fn new(
        prev_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
        content_transition_mode: WindowContentTransitionMode,
    ) -> Self {
        Self {
            prev_window_infos,
            content_transition_mode,
        }
    }

    pub(crate) fn render_latest_and_apply(
        self,
        mut state: FrameOutputTarget<'_>,
        curr_window_infos: &mut HashMap<DisplayWindowId, WindowInfo>,
    ) -> NavigationIntentObservation {
        let Some(curr) = state.latest_window_info() else {
            return NavigationIntentObservation::None;
        };
        let used_navigation = self.record_transition_hint(state.reborrow(), &curr);
        self.record_effect_hints(state, &curr);
        curr_window_infos.insert(curr.window_id, curr);
        used_navigation
    }

    fn record_transition_hint(
        &self,
        mut state: FrameOutputTarget<'_>,
        curr: &WindowInfo,
    ) -> NavigationIntentObservation {
        let navigation = match self.content_transition_mode {
            WindowContentTransitionMode::PerWindow { navigation } => navigation,
            WindowContentTransitionMode::SuppressedByFrameNavigation {
                superseded_navigation,
            } => {
                return superseded_navigation
                    .map_or(NavigationIntentObservation::None, |direction| {
                        NavigationIntentObservation::SupersededByFrameNavigation(direction)
                    });
            }
        };
        let Some(prev) = self.prev_window_infos.get(&curr.window_id) else {
            return NavigationIntentObservation::retired(navigation);
        };
        let Some(mut hint) = derive_buffer_replacement_hint(prev, curr) else {
            return NavigationIntentObservation::retired(navigation);
        };
        let observation = match (&mut hint, self.content_transition_mode) {
            (
                ContentTransitionHint::BufferReplaced { intent, .. },
                WindowContentTransitionMode::PerWindow { navigation },
            ) => {
                *intent = navigation.map_or(
                    ContentTransitionIntent::Replace,
                    ContentTransitionIntent::Navigate,
                );
                navigation.map_or(NavigationIntentObservation::None, |direction| {
                    NavigationIntentObservation::TransitionEmitted(direction)
                })
            }
            _ => NavigationIntentObservation::retired(navigation),
        };
        state.add_transition_hint(hint);
        observation
    }

    fn record_effect_hints(&self, mut state: FrameOutputTarget<'_>, curr: &WindowInfo) {
        if curr.is_minibuffer {
            return;
        }

        let Some(prev) = self.prev_window_infos.get(&curr.window_id) else {
            return;
        };
        if prev.buffer_id == 0 || curr.buffer_id == 0 {
            return;
        }

        if prev.buffer_id != curr.buffer_id {
            return;
        }

        if prev.window_start == curr.window_start {
            return;
        }

        let direction = if curr.window_start > prev.window_start {
            1
        } else {
            -1
        };
    }
}

pub(crate) struct FrameLineAnimationHintsRenderRequest<'a> {
    prev_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
    curr_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
}

impl<'a> FrameLineAnimationHintsRenderRequest<'a> {
    pub(crate) fn new(
        prev_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
        curr_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
    ) -> Self {
        Self {
            prev_window_infos,
            curr_window_infos,
        }
    }

    pub(crate) fn render_and_apply(self, mut state: FrameOutputTarget<'_>) {
        for (window_id, curr) in self.curr_window_infos {
            if curr.is_minibuffer {
                continue;
            }
            let Some(prev) = self.prev_window_infos.get(window_id) else {
                continue;
            };
            if prev.buffer_id == 0 || curr.buffer_id == 0 {
                continue;
            }
            if prev.buffer_id != curr.buffer_id
                || prev.window_start != curr.window_start
                || prev.buffer_size == curr.buffer_size
            {
                continue;
            }

            if let Some(edit_y) = state.window_cursor_y(curr) {
                let offset = if curr.buffer_size > prev.buffer_size {
                    -curr.char_height
                } else {
                    curr.char_height
                };
                state.add_effect_hint(WindowEffectHint::LineAnimation {
                    window_id: curr.window_id,
                    bounds: curr.bounds,
                    edit_y: edit_y + curr.char_height,
                    offset,
                });
            }
        }
    }
}

pub(crate) struct FrameContentTransitionHintRenderRequest<'a> {
    prev_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
    curr_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
    navigation: Option<TransitionDirection>,
}

impl<'a> FrameContentTransitionHintRenderRequest<'a> {
    pub(crate) fn new(
        prev_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
        curr_window_infos: &'a HashMap<DisplayWindowId, WindowInfo>,
        navigation: Option<TransitionDirection>,
    ) -> Self {
        Self {
            prev_window_infos,
            curr_window_infos,
            navigation,
        }
    }

    pub(crate) fn render_and_apply(
        self,
        mut state: FrameOutputTarget<'_>,
    ) -> NavigationIntentObservation {
        if self.prev_window_infos.is_empty() {
            return NavigationIntentObservation::retired(self.navigation);
        }

        let prev_non_mini = non_minibuffer_window_ids(self.prev_window_infos);
        let curr_non_mini = non_minibuffer_window_ids(self.curr_window_infos);

        if prev_non_mini.is_empty() || curr_non_mini.is_empty() {
            return NavigationIntentObservation::retired(self.navigation);
        }

        let should_transition = self.navigation.is_some() || prev_non_mini != curr_non_mini;
        if !should_transition
            || state.transition_hints().iter().any(|hint| {
                matches!(
                    hint,
                    ContentTransitionHint::BufferReplaced {
                        target: BufferTransitionTarget::Frame { .. },
                        ..
                    }
                )
            })
        {
            return NavigationIntentObservation::retired(self.navigation);
        }

        let Some(regions) = compatible_non_minibuffer_content_regions(
            self.prev_window_infos,
            self.curr_window_infos,
        ) else {
            return NavigationIntentObservation::retired(self.navigation);
        };
        let intent = self.navigation.map_or(
            ContentTransitionIntent::Replace,
            ContentTransitionIntent::Navigate,
        );
        state.add_transition_hint(ContentTransitionHint::BufferReplaced {
            target: BufferTransitionTarget::Frame { regions },
            intent,
        });
        self.navigation
            .map_or(NavigationIntentObservation::None, |direction| {
                NavigationIntentObservation::TransitionEmitted(direction)
            })
    }
}

fn compatible_non_minibuffer_content_regions(
    previous: &HashMap<DisplayWindowId, WindowInfo>,
    current: &HashMap<DisplayWindowId, WindowInfo>,
) -> Option<Vec<BufferViewportRegion>> {
    let previous_regions = non_minibuffer_content_regions(previous)?;
    let current_regions = non_minibuffer_content_regions(current)?;
    (previous_regions == current_regions).then_some(current_regions)
}

fn non_minibuffer_content_regions(
    window_infos: &HashMap<DisplayWindowId, WindowInfo>,
) -> Option<Vec<BufferViewportRegion>> {
    let mut regions: Vec<_> = window_infos
        .values()
        .filter(|info| !info.is_minibuffer)
        .map(|info| Some((info.window_id, info.geometry.buffer_viewport()?)))
        .collect::<Option<_>>()?;
    regions.sort_by(|(left_id, left), (right_id, right)| {
        left.bounds()
            .y
            .total_cmp(&right.bounds().y)
            .then_with(|| left.bounds().x.total_cmp(&right.bounds().x))
            .then_with(|| left_id.cmp(right_id))
    });
    (!regions.is_empty()).then(|| regions.into_iter().map(|(_, region)| region).collect())
}

fn non_minibuffer_window_ids(
    window_infos: &HashMap<DisplayWindowId, WindowInfo>,
) -> HashSet<DisplayWindowId> {
    window_infos
        .iter()
        .filter(|(_, info)| !info.is_minibuffer)
        .map(|(window_id, _)| *window_id)
        .collect()
}

pub(crate) struct WindowFrameDecorationsRenderRequest<'a> {
    params: &'a WindowParams,
    frame_params: &'a FrameParams,
    geometry: WindowFrameGeometry,
    info: &'a WindowInfo,
    effective_default_face: Option<&'a EffectiveWindowDefaultFace>,
}

impl<'a> WindowFrameDecorationsRenderRequest<'a> {
    pub(crate) fn new(
        params: &'a WindowParams,
        frame_params: &'a FrameParams,
        geometry: WindowFrameGeometry,
        info: &'a WindowInfo,
        effective_default_face: Option<&'a EffectiveWindowDefaultFace>,
    ) -> Self {
        Self {
            params,
            frame_params,
            geometry,
            info,
            effective_default_face,
        }
    }

    pub(crate) fn render_and_apply(
        self,
        mut state: FrameOutputTarget<'_>,
        mut render_services: ChromeRowRenderServices<'_, '_>,
    ) {
        WindowScrollBarsRenderRequest::new(self.params, self.info)
            .render_and_apply(state.reborrow());
        self.render_right_divider(state.reborrow(), render_services.reborrow());
        self.render_bottom_divider(state);
    }

    fn render_right_divider(
        &self,
        mut state: FrameOutputTarget<'_>,
        render_services: ChromeRowRenderServices<'_, '_>,
    ) {
        if self.params.is_minibuffer() || self.geometry.is_rightmost {
            return;
        }

        if self.frame_params.right_divider_width > 0 {
            let width = self.frame_params.right_divider_width as f32;
            let height = self.params.bounds.height
                - if self.frame_params.bottom_divider_width > 0 && !self.geometry.is_bottommost {
                    self.frame_params.bottom_divider_width as f32
                } else {
                    0.0
                };
            WindowDividerRectsRenderRequest::new(
                self.params.window_id,
                self.geometry.right_edge - width,
                self.params.bounds.y,
                width,
                height.max(0.0),
                WindowDividerOrientation::Vertical,
                self.frame_params,
            )
            .render_and_apply(state);
            return;
        }

        if self.frame_params.window_system {
            state.add_border(
                self.params.window_id,
                self.geometry.right_edge - 1.0,
                self.params.bounds.y,
                1.0,
                self.params.bounds.height.max(0.0),
                Color::from_pixel(self.frame_params.vertical_border_fg),
            );
        } else {
            if let Some(effective_default_face) = self.effective_default_face {
                TextWindowTerminalRightBorderRequest::new(self.frame_params.char_width)
                    .install_and_apply(
                        state.text_window_output_target(),
                        render_services,
                        effective_default_face,
                    );
            }
        }
    }

    fn render_bottom_divider(&self, state: FrameOutputTarget<'_>) {
        if self.params.is_minibuffer()
            || self.geometry.is_bottommost
            || self.frame_params.bottom_divider_width <= 0
        {
            return;
        }

        let height = self.frame_params.bottom_divider_width as f32;
        let width = self.params.bounds.width
            - if self.frame_params.right_divider_width > 0 && !self.geometry.is_rightmost {
                self.frame_params.right_divider_width as f32
            } else {
                0.0
            };
        WindowDividerRectsRenderRequest::new(
            self.params.window_id,
            self.params.bounds.x,
            self.geometry.bottom_edge - height,
            width.max(0.0),
            height,
            WindowDividerOrientation::Horizontal,
            self.frame_params,
        )
        .render_and_apply(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowDividerOrientation {
    Horizontal,
    Vertical,
}

struct WindowDividerRectsRenderRequest<'a> {
    window_id: i64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    orientation: WindowDividerOrientation,
    frame_params: &'a FrameParams,
}

impl<'a> WindowDividerRectsRenderRequest<'a> {
    fn new(
        window_id: i64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        orientation: WindowDividerOrientation,
        frame_params: &'a FrameParams,
    ) -> Self {
        Self {
            window_id,
            x,
            y,
            width,
            height,
            orientation,
            frame_params,
        }
    }

    fn render_and_apply(self, mut state: FrameOutputTarget<'_>) {
        if self.width <= 0.0 || self.height <= 0.0 {
            return;
        }

        let inner = Color::from_pixel(self.frame_params.divider_fg);
        if self.primary_size() < 3.0 {
            state.add_border(
                self.window_id,
                self.x,
                self.y,
                self.width,
                self.height,
                inner,
            );
            return;
        }

        let first = Color::from_pixel(self.frame_params.divider_first_fg);
        let last = Color::from_pixel(self.frame_params.divider_last_fg);
        match self.orientation {
            WindowDividerOrientation::Vertical => {
                state.add_border(self.window_id, self.x, self.y, 1.0, self.height, first);
                state.add_border(
                    self.window_id,
                    self.x + 1.0,
                    self.y,
                    (self.width - 2.0).max(0.0),
                    self.height,
                    inner,
                );
                state.add_border(
                    self.window_id,
                    self.x + self.width - 1.0,
                    self.y,
                    1.0,
                    self.height,
                    last,
                );
            }
            WindowDividerOrientation::Horizontal => {
                state.add_border(self.window_id, self.x, self.y, self.width, 1.0, first);
                state.add_border(
                    self.window_id,
                    self.x,
                    self.y + 1.0,
                    self.width,
                    (self.height - 2.0).max(0.0),
                    inner,
                );
                state.add_border(
                    self.window_id,
                    self.x,
                    self.y + self.height - 1.0,
                    self.width,
                    1.0,
                    last,
                );
            }
        }
    }

    fn primary_size(&self) -> f32 {
        match self.orientation {
            WindowDividerOrientation::Horizontal => self.height,
            WindowDividerOrientation::Vertical => self.width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowScrollBarMetrics {
    pub(crate) position: i64,
    pub(crate) portion: i64,
    pub(crate) whole: i64,
    pub(crate) thumb_start: f32,
    pub(crate) thumb_size: f32,
}

pub(crate) struct WindowScrollBarsRenderRequest<'a> {
    params: &'a WindowParams,
    info: &'a WindowInfo,
}

impl<'a> WindowScrollBarsRenderRequest<'a> {
    pub(crate) fn new(params: &'a WindowParams, info: &'a WindowInfo) -> Self {
        Self { params, info }
    }

    pub(crate) fn render_and_apply(self, mut state: FrameOutputTarget<'_>) {
        // Subtle light track (drawn at scroll_bar.track_opacity) with a clearly
        // darker thumb on top, so the thumb stands out and reads as a position
        // indicator. GNU/GTK inverts this (light thumb in a dark trough); we keep
        // the light track to match neomacs's subtle chrome but give the thumb
        // enough contrast to be plainly visible.
        let track_color = Color::new(0.72, 0.72, 0.72, 1.0);
        let thumb_color = Color::new(0.32, 0.32, 0.32, 1.0);
        let ProtocolWindowGeometry::Complete { regions, .. } = &self.info.geometry else {
            return;
        };

        if let Some(ref side) = self.params.vertical_scroll_bar_side {
            let track = if side == "left" {
                regions.left_scroll_bar
            } else {
                regions.right_scroll_bar
            };
            if let Some(track) = track {
                let accessible_start = self.params.accessible_start_charpos().get();
                let accessible_end = self.params.accessible_end_charpos().get();
                let metrics = WindowScrollBarMetrics::vertical(
                    self.info.window_start,
                    self.info.window_end,
                    accessible_start,
                    accessible_end,
                    track.height,
                );

                state.add_scroll_bar(ScrollBarItem {
                    window_id: DisplayWindowId::new(self.params.window_id),
                    row_role: GlyphRowRole::Text,
                    clip_rect: Some(self.params.bounds),
                    horizontal: false,
                    x: track.x,
                    y: track.y,
                    width: track.width,
                    height: track.height,
                    position: metrics.position,
                    portion: metrics.portion,
                    whole: metrics.whole,
                    thumb_start: metrics.thumb_start,
                    thumb_size: metrics.thumb_size,
                    track_color,
                    thumb_color,
                });
            }
        }

        if self.params.horizontal_scroll_bar {
            let Some(track) = regions.horizontal_scroll_bar else {
                return;
            };
            let track_width = track.width;

            let hscroll_px = self.params.hscroll as f32 * self.params.char_width;
            let visible_px = self.params.text_bounds.width.max(1.0);
            let thumb_size = if track_width > 0.0 {
                (visible_px / (visible_px + hscroll_px + track_width)) * track_width
            } else {
                track_width
            }
            .clamp(8.0, track_width);
            let thumb_start = if track_width > 0.0 && hscroll_px + visible_px > 0.0 {
                (hscroll_px / (hscroll_px + visible_px)) * (track_width - thumb_size)
            } else {
                0.0
            };

            state.add_scroll_bar(ScrollBarItem {
                window_id: DisplayWindowId::new(self.params.window_id),
                row_role: GlyphRowRole::Text,
                clip_rect: Some(self.params.bounds),
                horizontal: true,
                x: track.x,
                y: track.y,
                width: track_width,
                height: track.height,
                position: self.params.hscroll as i64,
                portion: visible_px.round().max(1.0) as i64,
                whole: (visible_px + hscroll_px).round().max(1.0) as i64,
                thumb_start,
                thumb_size,
                track_color,
                thumb_color,
            });
        }
    }
}

impl WindowScrollBarMetrics {
    /// Mirrors GNU `set_vertical_scroll_bar` (xdisp.c): whole = ZV - BEGV,
    /// start = window_start - BEGV, end = Z - window_end_pos - BEGV.
    pub(crate) fn vertical(
        window_start: i64,
        window_end: i64,
        buffer_begv: i64,
        buffer_size: i64,
        track_height: f32,
    ) -> Self {
        let whole = (buffer_size - buffer_begv).max(1);
        let position = (window_start - 1 - buffer_begv).max(0);
        let end = if window_end > 0 {
            (window_end - 1 - buffer_begv).max(position)
        } else {
            position
        };
        let portion = (end - position).max(1);
        let effective_whole = whole.max(portion);

        let thumb_start = (position as f32 / effective_whole as f32) * track_height;
        let thumb_size = (portion as f32 / effective_whole as f32) * track_height;
        let min_thumb = 20.0f32.min(track_height * 0.2);
        let thumb_size = thumb_size.max(min_thumb).min(track_height);
        let thumb_start = thumb_start
            .max(0.0)
            .min((track_height - thumb_size).max(0.0));

        Self {
            position,
            portion,
            whole: effective_whole,
            thumb_start,
            thumb_size,
        }
    }
}

#[cfg(test)]
#[path = "display_frame_output_test.rs"]
mod tests;
