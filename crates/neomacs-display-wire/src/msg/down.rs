//! Downlink: evaluator → frontend (DESIGN §6.8).

use neomacs_display_protocol::input_progress::{InputCancelled, InputSerial, InputStreamId};
use neomacs_display_protocol::menu::{MenuBarRequestId, MenuToken, PopupMenuItem};
use neomacs_display_protocol::scroll_coverage::CoveragePublicationId;
use neomacs_display_protocol::{
    DesktopFontRenderOptions, DisplayWindowId, Face, FaceId, GuiFrameGeometryHints,
    GuiResizeRequestId, LiveDisplayFrameId, ModifierPolicy, PopupPlacement, PresentationId, Rect,
    SurfaceChannelSource, SurfaceId, SurfaceShaderLanguage, SurfaceUniformInit, TerminalId,
    WindowFullscreenMode, WindowVisibility,
};
use serde::{Deserialize, Serialize};

use super::{
    ChannelId, ChunkEnd, ConvertId, DisplayName, FrameShaderRequestId, Lane, RequestId,
    ScreenPosition, SelectionGeneration, SelectionName, SelectionTarget, TerminalNumber, Toggle,
    TooltipTicket, WindowSize,
};
use crate::assets::AssetDown;
use crate::payload::Payload;
use crate::present::PresentOp;
use crate::rows::RowRuns;

/// Every downlink message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Downlink {
    /// Open a `neo` terminal on this session; answered `ChannelOpened` or
    /// `ChannelRefused`.
    OpenChannel {
        channel: ChannelId,
        terminal: TerminalNumber,
        name: DisplayName,
    },
    CloseChannel {
        channel: ChannelId,
        reason: ChannelCloseReason,
    },
    /// Keyed latest-wins window state per `(frame, kind)` (D32).
    Window {
        frame: LiveDisplayFrameId,
        op: WindowOp,
    },
    /// One FrameSync operation (DESIGN §6.10).
    Present(PresentOp),
    Asset(AssetDown),
    Ui(UiOp),
    Config(ConfigOp),
    Selection {
        channel: ChannelId,
        op: SelectionOp,
    },
    /// Answers an uplink `Convert`.
    ConvertReply {
        convert: ConvertId,
        outcome: ConvertOutcome,
    },
    /// Converted selection bytes, chunked on the bulk lane.
    SelectionData {
        convert: ConvertId,
        offset: u64,
        bytes: Payload<[u8]>,
        end: ChunkEnd,
    },
    /// The evaluator consumed input through this serial (D33).
    InputProgress {
        stream: InputStreamId,
        consumed_through: u64,
    },
    Request {
        id: RequestId,
        channel: ChannelId,
        body: RequestBody,
    },
    Cancel {
        id: RequestId,
    },
    /// Tracked inputs that will never be consumed (replaces `Drop`).
    InputCancelled {
        cancelled: Vec<InputCancelled>,
    },
    Hint(HintOp),
    Media(MediaOp),
}

impl Downlink {
    /// The lane this message travels on (DESIGN §6.8).
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Present(_) => Lane::Present,
            Self::Asset(asset) => asset.lane(),
            Self::SelectionData { .. } => Lane::Bulk,
            Self::Hint(_) => Lane::Hint,
            Self::Media(media) => media.lane(),
            Self::OpenChannel { .. }
            | Self::CloseChannel { .. }
            | Self::Window { .. }
            | Self::Ui(_)
            | Self::Config(_)
            | Self::Selection { .. }
            | Self::ConvertReply { .. }
            | Self::InputProgress { .. }
            | Self::Request { .. }
            | Self::Cancel { .. }
            | Self::InputCancelled { .. } => Lane::Control,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelCloseReason {
    /// `delete-terminal` or `x-close-connection`.
    Deleted,
    /// The evaluator is shutting down.
    Shutdown,
}

/// Whether a new native window adopts the provisional startup window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WindowAdoption {
    NewWindow,
    AdoptProvisional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decorations {
    Decorated,
    Undecorated,
}

/// How a focus request treats activation (GNU `x-focus-frame` NOACTIVATE).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FocusActivation {
    Activate,
    NoActivate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttentionUrgency {
    Urgent,
    Informational,
}

/// The X cursor-font shape the pointer should take (`x-pointer-shape`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PointerShape(pub i32);

/// Everything a native window needs at creation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowInit {
    pub channel: ChannelId,
    pub parent: Option<LiveDisplayFrameId>,
    pub size: WindowSize,
    pub position: Option<ScreenPosition>,
    pub visibility: WindowVisibility,
    pub decorations: Decorations,
    pub title: String,
    pub hints: GuiFrameGeometryHints,
    pub adoption: WindowAdoption,
}

/// A child frame's placement in its parent, in the parent's logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildPlacement {
    pub parent: LiveDisplayFrameId,
    pub bounds: Rect,
    pub z_order: i32,
}

/// Whole-frame opacity control (GNU `alpha`, `alpha-background`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum OpacityOp {
    Accept,
    SetLowerLimit { limit: f32 },
    SetRedirects { enabled: Toggle },
    Retire,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WindowOp {
    Create(Box<WindowInit>),
    Destroy,
    SetTitle {
        title: String,
    },
    SetGeometryHints {
        hints: GuiFrameGeometryHints,
    },
    Resize {
        size: WindowSize,
        request: Option<GuiResizeRequestId>,
    },
    SetPosition {
        position: ScreenPosition,
    },
    SetVisibility {
        visibility: WindowVisibility,
    },
    SetFullscreen {
        mode: WindowFullscreenMode,
    },
    SetDecorations {
        decorations: Decorations,
    },
    Iconify,
    Focus {
        activation: FocusActivation,
    },
    RequestAttention {
        urgency: AttentionUrgency,
    },
    SetPointerShape {
        shape: PointerShape,
    },
    WarpPointer {
        position: ScreenPosition,
    },
    ShowChild {
        placement: ChildPlacement,
    },
    RemoveChild,
    Opacity(OpacityOp),
    ImeSpot {
        rect: Rect,
    },
}

/// Menu colors in sRGB.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MenuColors {
    pub foreground: [f32; 3],
    pub background: [f32; 3],
}

/// A tooltip laid out on the evaluator: its rows and the faces they use.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TooltipLayout {
    pub size: WindowSize,
    pub rows: Vec<RowRuns>,
    pub faces: Vec<(FaceId, Face)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UiOp {
    ShowPopupMenu {
        token: MenuToken,
        frame: LiveDisplayFrameId,
        request: Option<MenuBarRequestId>,
        placement: PopupPlacement,
        items: Vec<PopupMenuItem>,
        title: Option<String>,
        colors: Option<MenuColors>,
    },
    HidePopupMenu {
        token: MenuToken,
    },
    PresentTooltip {
        ticket: TooltipTicket,
        /// The input whose handling showed the tooltip (a causal stamp).
        cause: Option<InputSerial>,
        frame: LiveDisplayFrameId,
        anchor: Rect,
        layout: Payload<TooltipLayout>,
    },
    DismissTooltip {
        ticket: TooltipTicket,
    },
    VisualBell {
        frame: LiveDisplayFrameId,
    },
}

/// The JSON text of `VisualConfig`'s serde reflection: the one embedded
/// self-describing document (DESIGN §6.9).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualConfigDoc(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChildFrameShadow {
    Off,
    On {
        layers: u32,
        offset: f32,
        opacity: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CursorBlink {
    Off,
    On { interval_ms: u32 },
}

/// Keyed latest-wins configuration, replayed on attach and resume.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConfigOp {
    VisualConfig(Payload<VisualConfigDoc>),
    ModifierPolicy(ModifierPolicy),
    FontRenderOptions(DesktopFontRenderOptions),
    ScrollIndicators(Toggle),
    TitlebarHeight {
        height: f32,
    },
    ShowFps(Toggle),
    CornerRadius {
        radius: f32,
    },
    ExtraSpacing {
        line: f32,
        letter: f32,
    },
    ChildFrameStyle {
        corner_radius: f32,
        shadow: ChildFrameShadow,
    },
    /// `ns-confirm-quit`.
    ConfirmQuit(Toggle),
    /// The selected top-level frame.
    SelectedRoot(Option<LiveDisplayFrameId>),
    CursorBlink(CursorBlink),
    /// Lets the frontend exempt quit edges from its input bound (D33).
    QuitKeys {
        quit_char: char,
        single_kboard_current: Option<ChannelId>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SelectionContent {
    /// Up to 64 KiB inline.
    Inline(Payload<[u8]>),
    /// Larger values are converted on request (`Convert`), as GNU converts at
    /// request time.
    Deferred { len_hint: u64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SelectionOp {
    Own {
        selection: SelectionName,
        generation: SelectionGeneration,
        content: SelectionContent,
    },
    Disown {
        selection: SelectionName,
        generation: SelectionGeneration,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvertOutcome {
    /// `SelectionData` chunks follow on the bulk lane.
    Converted { target: SelectionTarget, len: u64 },
    /// No converter produced data for the target.
    NoConversion,
    /// The selection changed owner since the frontend asked.
    NotOwner,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestBody {
    SelectionGet {
        selection: SelectionName,
        target: SelectionTarget,
    },
    SelectionExists {
        selection: SelectionName,
    },
    OpenUri {
        uri: String,
    },
    /// The text of a neo-term terminal, until neo-term is re-homed (M15).
    TerminalText {
        id: TerminalId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HintOp {
    /// Paint an evaluator-resolved viewport while canonical layout is pending.
    ScrollPreview {
        frame: LiveDisplayFrameId,
        window: DisplayWindowId,
        presentation: PresentationId,
        epoch: u64,
        publication: CoveragePublicationId,
        offset: f32,
        inputs: Vec<InputSerial>,
    },
    /// Ask for more scroll coverage of a window.
    CoverageRefill {
        frame: LiveDisplayFrameId,
        window: DisplayWindowId,
        epoch: u64,
    },
}

/// A shader surface's content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SurfaceSourceSpec {
    Shader {
        language: SurfaceShaderLanguage,
        source: String,
        uniforms: Vec<SurfaceUniformInit>,
        channel0: Option<SurfaceChannelSource>,
    },
    /// Raw RGBA8 pixels, row-major, tightly packed.
    Pixels { data: Payload<[u8]> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceAnimation {
    Static,
    Animated { fps: Option<u32> },
}

/// Whether the media budget may evict a surface (it is recreated on the
/// next redisplay) or must keep it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceEviction {
    Recreatable,
    Pinned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameShader {
    pub source: String,
    pub language: SurfaceShaderLanguage,
    pub uniforms: Vec<SurfaceUniformInit>,
}

/// Capability-gated media (DESIGN §9). Video and web views are not in
/// revision 1.0: their vocabulary lives outside the display protocol.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MediaOp {
    SurfaceCreate {
        id: SurfaceId,
        source: SurfaceSourceSpec,
        size: WindowSize,
        animation: SurfaceAnimation,
        eviction: SurfaceEviction,
    },
    SurfaceSetUniform {
        id: SurfaceId,
        name: String,
        value: [f32; 4],
    },
    SurfaceFree {
        id: SurfaceId,
    },
    FrameShaderSet {
        request: FrameShaderRequestId,
        shader: Option<FrameShader>,
    },
    FrameShaderSetUniform {
        request: FrameShaderRequestId,
        name: String,
        value: [f32; 4],
    },
}

impl MediaOp {
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::SurfaceCreate {
                source: SurfaceSourceSpec::Pixels { .. },
                ..
            } => Lane::Bulk,
            Self::SurfaceCreate { .. }
            | Self::SurfaceSetUniform { .. }
            | Self::SurfaceFree { .. }
            | Self::FrameShaderSet { .. }
            | Self::FrameShaderSetUniform { .. } => Lane::Control,
        }
    }
}
