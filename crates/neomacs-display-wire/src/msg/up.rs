//! Uplink: frontend → evaluator (DESIGN §6.8).

use neomacs_display_protocol::input_progress::InputSerial;
use neomacs_display_protocol::menu::{MenuBarRequestId, MenuToken};
use neomacs_display_protocol::{
    DesktopFontRenderOptions, DisplayTime, FrontendKey, GuiResizeRequestId, LiveDisplayFrameId,
    ModifierBits, MonitorInfo, PointerTarget, PresentationId, Rect, SurfaceId, SystemFonts,
};
use serde::{Deserialize, Serialize};

use super::{
    ChannelId, ConvertId, FactVersion, FrameShaderRequestId, Lane, LayoutPosition, RequestId,
    ScreenPosition, SelectionGeneration, SelectionName, SelectionTarget, TooltipTicket, WindowSize,
};
use crate::assets::{AssetId, AssetUp};
use crate::present::RejectReason;

/// Every uplink message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Uplink {
    ChannelOpened {
        channel: ChannelId,
        facts: Box<DisplayFacts>,
    },
    ChannelRefused {
        channel: ChannelId,
        reason: ChannelRefusal,
    },
    /// An input edge: FIFO, lossless, exactly once by serial.
    Input {
        serial: InputSerial,
        time: DisplayTime,
        event: InputEdge,
    },
    /// Input levels, coalesced in place (latest per frame).
    PointerMotion {
        frame: LiveDisplayFrameId,
        target: PointerTarget,
        at: LayoutPosition,
        mods: ModifierBits,
    },
    WindowGeometry {
        frame: LiveDisplayFrameId,
        size: WindowSize,
        scale: f64,
    },
    /// Presentation lifecycle, causal with pointer input (O2).
    Presentation(PresentationAck),
    /// Ownership ordered relative to input (O3).
    SelectionState {
        channel: ChannelId,
        selection: SelectionName,
        owner: SelectionOwner,
        sequence: Option<u64>,
    },
    /// A local application pasted Emacs-owned data.
    Convert {
        convert: ConvertId,
        channel: ChannelId,
        selection: SelectionName,
        target: SelectionTarget,
        generation: SelectionGeneration,
    },
    Fact(LevelFact),
    Asset(AssetUp),
    Reply {
        id: RequestId,
        outcome: ReplyOutcome,
    },
    Media(MediaEvent),
    /// The frontend dropped edges at its bound and says so.
    InputOverflow {
        from: InputSerial,
        to: InputSerial,
    },
}

impl Uplink {
    /// The lane this message travels on (DESIGN §6.8).
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Input { .. }
            | Self::PointerMotion { .. }
            | Self::WindowGeometry { .. }
            | Self::Presentation(_)
            | Self::SelectionState { .. } => Lane::Input,
            Self::Fact(_) => Lane::Facts,
            Self::ChannelOpened { .. }
            | Self::ChannelRefused { .. }
            | Self::Convert { .. }
            | Self::Asset(_)
            | Self::Reply { .. }
            | Self::Media(_)
            | Self::InputOverflow { .. } => Lane::Control,
        }
    }
}

/// A capability with its version and parameters (DESIGN §6.17).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: u16,
    pub params: Vec<(String, String)>,
}

/// Capabilities, sorted by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilitySet(pub Vec<Capability>);

/// One entry of the display's resource database (`x-get-resource`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    pub value: String,
}

/// What a channel learns about its display when it opens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayFacts {
    pub monitors: Vec<MonitorInfo>,
    pub system_fonts: SystemFonts,
    pub text_dpi: f32,
    pub render_options: Option<DesktopFontRenderOptions>,
    pub capabilities: CapabilitySet,
    pub resources: Vec<Resource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelRefusal {
    /// The frontend's policy does not allow another channel.
    Policy,
    /// The display behind the session is gone.
    DisplayUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PointerAction {
    Button { button: u32, state: ButtonState },
    Move,
    Scroll { delta: ScrollDelta },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FocusState {
    Focused,
    Unfocused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuChoice {
    Item(u32),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TooltipDismissal {
    PointerLeft,
    Replaced,
    Timeout,
}

/// One input edge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InputEdge {
    Key {
        frame: LiveDisplayFrameId,
        key: FrontendKey,
        mods: ModifierBits,
        text: Option<String>,
    },
    ImeCommit {
        frame: LiveDisplayFrameId,
        text: String,
    },
    Pointer {
        frame: LiveDisplayFrameId,
        target: PointerTarget,
        at: LayoutPosition,
        action: PointerAction,
        mods: ModifierBits,
    },
    PresentedPointer {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
        interaction: u32,
        state: ButtonState,
        button: u8,
        at: LayoutPosition,
    },
    ToolBarClick {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
        item: u32,
    },
    MenuBarClick {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
        request: Option<MenuBarRequestId>,
        index: u32,
        key: String,
        anchor: Rect,
    },
    MenuSelection {
        token: MenuToken,
        choice: MenuChoice,
    },
    TooltipDismissed {
        ticket: TooltipTicket,
        reason: TooltipDismissal,
    },
    FileDrop {
        frame: LiveDisplayFrameId,
        target: PointerTarget,
        uris: Vec<String>,
    },
    CloseRequested {
        frame: LiveDisplayFrameId,
    },
    Focus {
        frame: LiveDisplayFrameId,
        state: FocusState,
        tooltip_stamp: Option<u64>,
    },
    Moved {
        frame: LiveDisplayFrameId,
        position: ScreenPosition,
    },
    Iconified {
        frame: LiveDisplayFrameId,
    },
    Deiconified {
        frame: LiveDisplayFrameId,
    },
    ResizeCompleted {
        request: GuiResizeRequestId,
    },
    ApplicationQuit {
        channel: ChannelId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PresentationAck {
    Received {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
    },
    Activated {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
        pending_assets: Vec<AssetId>,
    },
    Discarded {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
    },
    Retired {
        presentation: PresentationId,
    },
    Rejected {
        frame: LiveDisplayFrameId,
        presentation: PresentationId,
        reason: RejectReason,
    },
    StoreAck {
        through: crate::present::UpdateSeq,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SelectionOwner {
    Ours {
        generation: SelectionGeneration,
    },
    Foreign,
    Empty,
    /// The platform has no ownership signal.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DisplayResetReason {
    DeviceLost,
    Reconfigured,
}

/// Versioned level facts, latest-wins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LevelFact {
    Monitors {
        version: FactVersion,
        list: Vec<MonitorInfo>,
    },
    Desktop {
        version: FactVersion,
        system_fonts: SystemFonts,
        text_dpi: f32,
        render_options: Option<DesktopFontRenderOptions>,
    },
    Resources {
        version: FactVersion,
        db: Vec<Resource>,
    },
    Capabilities {
        version: FactVersion,
        set: CapabilitySet,
    },
    DisplayReset {
        reason: DisplayResetReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SelectionPresence {
    Present,
    Absent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplyValue {
    SelectionData {
        target: SelectionTarget,
        bytes: Option<Vec<u8>>,
    },
    SelectionExists(SelectionPresence),
    UriOpened,
    TerminalText(String),
}

/// Why the frontend's policy refused a request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyRefusal {
    SchemeNotAllowed { scheme: String },
    SelectionPrivate,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplyOutcome {
    Value(ReplyValue),
    /// The frontend's own bound (5 s for selections) elapsed.
    TimedOut,
    Refused(PolicyRefusal),
    Failed {
        detail: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameShaderExecution {
    Running,
    SuppressedByQualityPolicy,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaEvent {
    SurfaceCreateFailed {
        id: SurfaceId,
        error: String,
    },
    FrameShaderAck {
        request: FrameShaderRequestId,
        execution: FrameShaderExecution,
    },
    FrameShaderFailed {
        request: FrameShaderRequestId,
        error: String,
    },
}
