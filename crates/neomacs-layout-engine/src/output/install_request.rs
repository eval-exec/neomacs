//! Typed output install requests consumed by `DisplayOutputBuilder`.

use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::{
    ContentTransitionHint, CursorStyle, DisplaySlotId, PhysCursor, PresentedWindowGeometry,
    WindowInfo,
};
use neomacs_display_protocol::glyph_matrix::{
    CursorItem, CursorItemRole, FaceFillItem, ScrollBarItem,
};
use neomacs_display_protocol::types::{Color, DisplayFrameId, DisplayWindowId, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutputCursorInstallRequest {
    window_id: DisplayWindowId,
    role: CursorItemRole,
    slot_id: DisplaySlotId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    ascent: f32,
    style: CursorStyle,
    color: Color,
    cursor_fg: Color,
}

impl OutputCursorInstallRequest {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        window_id: DisplayWindowId,
        role: CursorItemRole,
        slot_id: DisplaySlotId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        ascent: f32,
        style: CursorStyle,
        color: Color,
        cursor_fg: Color,
    ) -> Self {
        Self {
            window_id,
            role,
            slot_id,
            x,
            y,
            width,
            height,
            ascent,
            style,
            color,
            cursor_fg,
        }
    }

    pub(crate) fn cursor_item(self) -> CursorItem {
        CursorItem {
            window_id: self.window_id,
            role: self.role,
            slot_id: self.slot_id,
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            style: self.style,
            color: self.color,
            cursor_fg: self.cursor_fg,
            ascent: self.ascent,
        }
    }
}

#[derive(Clone, Debug)]
// Installation requests are consumed immediately and keep artifacts by value
// to avoid allocating per scene item.
#[allow(clippy::large_enum_variant)]
pub(crate) enum OutputFrameArtifactInstallRequest {
    Background {
        bounds: Rect,
        color: Color,
    },
    FaceFill(FaceFillItem),
    Border {
        window_id: DisplayWindowId,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },
    ScrollBar(ScrollBarItem),
    WindowInfo(WindowInfo),
    TransitionHint(ContentTransitionHint),
    PhysCursor(PhysCursor),
}

impl OutputFrameArtifactInstallRequest {
    pub(crate) fn phys_cursor(cursor: PhysCursor) -> Self {
        Self::PhysCursor(cursor)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OutputFrameIdentityInstallRequest {
    pub(crate) frame_id: DisplayFrameId,
    pub(crate) parent_id: DisplayFrameId,
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

#[derive(Clone, Debug)]
// Large payload variant; boxing is a perf hint deferred out of the lint gate.
#[allow(clippy::large_enum_variant)]
pub(crate) enum OutputFrameStateInstallRequest {
    Identity(OutputFrameIdentityInstallRequest),
    BackgroundColor(Color),
    FontPixelSize(f32),
    CursorEffects {
        window_id: DisplayWindowId,
        effects: EffectsConfig,
    },
}

impl OutputFrameStateInstallRequest {
    pub(crate) fn cursor_effects(window_id: DisplayWindowId, effects: EffectsConfig) -> Self {
        Self::CursorEffects { window_id, effects }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputTextWindowDisplayRangeInstallRequest {
    pub(crate) window_id: DisplayWindowId,
    pub(crate) window_start: i64,
    pub(crate) window_end: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputRetryCheckpointRestoreRequest {
    pub(crate) transition_hints_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutputPresentedWindowGeometryInstallRequest {
    pub(crate) window_id: DisplayWindowId,
    pub(crate) geometry: PresentedWindowGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq)]
// Presentation geometry is a `Copy` protocol value; boxing it would change the
// request semantics and add allocation to window publication.
#[allow(clippy::large_enum_variant)]
pub(crate) enum OutputWindowMetadataInstallRequest {
    TextDisplayRange(OutputTextWindowDisplayRangeInstallRequest),
    PresentedGeometry(OutputPresentedWindowGeometryInstallRequest),
    RestoreRetryCheckpoint(OutputRetryCheckpointRestoreRequest),
}

impl From<OutputPresentedWindowGeometryInstallRequest> for OutputWindowMetadataInstallRequest {
    fn from(request: OutputPresentedWindowGeometryInstallRequest) -> Self {
        Self::PresentedGeometry(request)
    }
}

impl OutputTextWindowDisplayRangeInstallRequest {
    pub(crate) fn new(window_id: DisplayWindowId, window_start: i64, window_end: i64) -> Self {
        Self {
            window_id,
            window_start,
            window_end,
        }
    }
}

impl OutputRetryCheckpointRestoreRequest {
    pub(crate) fn new(transition_hints_len: usize) -> Self {
        Self {
            transition_hints_len,
        }
    }
}

impl From<OutputTextWindowDisplayRangeInstallRequest> for OutputWindowMetadataInstallRequest {
    fn from(request: OutputTextWindowDisplayRangeInstallRequest) -> Self {
        Self::TextDisplayRange(request)
    }
}

impl From<OutputRetryCheckpointRestoreRequest> for OutputWindowMetadataInstallRequest {
    fn from(request: OutputRetryCheckpointRestoreRequest) -> Self {
        Self::RestoreRetryCheckpoint(request)
    }
}
