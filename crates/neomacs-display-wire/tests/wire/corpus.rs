//! The golden corpus: one fixture per message variant, nested families
//! included (DESIGN §6.6 items 4-5).
//!
//! `down_tag`/`up_tag` match every variant without a wildcard, and every tag
//! has a sample (`down_sample`/`up_sample` match every tag), so a new
//! variant does not compile until it has a fixture.

use std::num::NonZeroU64;

use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::input_progress::{
    InputCancelled, InputReceipt, InputSerial, InputStreamId,
};
use neomacs_display_protocol::menu::{MenuBarRequestId, MenuToken, PopupMenuItem};
use neomacs_display_protocol::{
    DesktopFontRenderOptions, DisplayTime, DisplayWindowId, FaceId, FrontendKey,
    GuiFrameGeometryHints, GuiResizeRequestId, ImageId, LiveDisplayFrameId, ModifierBits,
    ModifierPolicy, MonitorInfo, Point, PointerTarget, PopupConstraintPolicy, PopupPlacement,
    PopupPreferredSide, PresentationId, Rect, SurfaceChannelSource, SurfaceId,
    SurfaceShaderLanguage, SurfaceUniformInit, SystemFonts, TerminalId, WindowFullscreenMode,
    WindowVisibility,
};
use neomacs_display_wire::assets::{
    AssetDown, AssetId, AssetKind, AssetUp, ContentId, FontContainer, WantPriority,
};
use neomacs_display_wire::codec::WireCodec;
use neomacs_display_wire::msg::down::{
    AttentionUrgency, ChannelCloseReason, ChildFrameShadow, ChildPlacement, ConfigOp,
    ConvertOutcome, CursorBlink, Decorations, FocusActivation, FrameShader, HintOp, MediaOp,
    MenuColors, OpacityOp, PointerShape, RequestBody, SelectionContent, SelectionOp,
    SurfaceAnimation, SurfaceEviction, SurfaceSourceSpec, TooltipLayout, UiOp, VisualConfigDoc,
    WindowAdoption, WindowInit, WindowOp,
};
use neomacs_display_wire::msg::up::{
    ButtonState, Capability, CapabilitySet, ChannelRefusal, DisplayFacts, DisplayResetReason,
    FocusState, FrameShaderExecution, InputEdge, LevelFact, MediaEvent, MenuChoice, PointerAction,
    PolicyRefusal, PresentationAck, ReplyOutcome, ReplyValue, Resource, ScrollDelta,
    SelectionOwner, SelectionPresence, TooltipDismissal,
};
use neomacs_display_wire::msg::{
    ChannelId, ChunkEnd, ConvertId, DisplayName, Downlink, FactVersion, FrameShaderRequestId,
    LayoutPosition, RequestId, ScreenPosition, SelectionGeneration, SelectionName, SelectionTarget,
    TerminalNumber, Toggle, TooltipTicket, Uplink, WindowSize,
};
use neomacs_display_wire::payload::Payload;
use neomacs_display_wire::present::{
    FrameSyncEncoder, PresentOp, RejectReason, StoreLimits, UpdateSeq,
};
use neomacs_display_wire::rows::RowRuns;
use neomacs_display_wire::schema::GoldenFixture;
use neomacs_display_wire::testing::TestFrame;
use strum::IntoEnumIterator;

fn nz(raw: u64) -> NonZeroU64 {
    NonZeroU64::new(raw).expect("non-zero")
}

pub fn frame() -> LiveDisplayFrameId {
    LiveDisplayFrameId::new(nz(3))
}

fn channel() -> ChannelId {
    ChannelId::new(nz(1))
}

fn serial(raw: u64) -> InputSerial {
    InputSerial::new(nz(raw))
}

fn rect() -> Rect {
    Rect::new(8.0, 16.0, 120.0, 32.5)
}

fn content_id() -> AssetId {
    AssetId::Content(ContentId([0xab; 32]))
}

/// Decode a vocabulary value from bytes, for types whose constructors
/// allocate process-wide ids.
fn decoded<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    WireCodec::default()
        .decode(bytes)
        .expect("fixture bytes decode")
}

/// One presentation of a small frame, as FrameSync encodes it.
pub fn sample_present_ops() -> Vec<PresentOp> {
    let mut frame_source = TestFrame::new(24, 4, "Hello, World\nsecond line\nTHIRD");
    let presentation = frame_source.present();
    FrameSyncEncoder::new(StoreLimits::default())
        .encode(frame(), &presentation)
        .expect("a test frame encodes")
}

fn present_op(pick: fn(&PresentOp) -> bool) -> PresentOp {
    sample_present_ops()
        .into_iter()
        .find(pick)
        .expect("the sample presentation has this operation")
}

fn sample_row() -> RowRuns {
    let ops = sample_present_ops();
    ops.into_iter()
        .find_map(|op| match op {
            PresentOp::DefineRow { row, .. } => Some((*row.value().expect("row")).clone()),
            _ => None,
        })
        .expect("a defined row")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIter)]
pub enum DownTag {
    OpenChannel,
    CloseChannel,
    WindowCreate,
    WindowDestroy,
    WindowSetTitle,
    WindowSetGeometryHints,
    WindowResize,
    WindowSetPosition,
    WindowSetVisibility,
    WindowSetFullscreen,
    WindowSetDecorations,
    WindowIconify,
    WindowFocus,
    WindowRequestAttention,
    WindowSetPointerShape,
    WindowWarpPointer,
    WindowShowChild,
    WindowRemoveChild,
    WindowOpacityAccept,
    WindowOpacitySetLowerLimit,
    WindowOpacitySetRedirects,
    WindowOpacityRetire,
    WindowImeSpot,
    PresentBegin,
    PresentDefineRow,
    PresentTables,
    PresentWindows,
    PresentFacts,
    PresentCommit,
    PresentStoreReset,
    AssetOffer,
    AssetForget,
    AssetImageRealize,
    AssetImageDrop,
    AssetPut,
    UiShowPopupMenu,
    UiHidePopupMenu,
    UiPresentTooltip,
    UiDismissTooltip,
    UiVisualBell,
    ConfigVisualConfig,
    ConfigModifierPolicy,
    ConfigFontRenderOptions,
    ConfigScrollIndicators,
    ConfigTitlebarHeight,
    ConfigShowFps,
    ConfigCornerRadius,
    ConfigExtraSpacing,
    ConfigChildFrameStyle,
    ConfigConfirmQuit,
    ConfigSelectedRoot,
    ConfigCursorBlink,
    ConfigQuitKeys,
    SelectionOwn,
    SelectionDisown,
    ConvertReply,
    SelectionData,
    InputProgress,
    RequestSelectionGet,
    RequestSelectionExists,
    RequestOpenUri,
    RequestTerminalText,
    Cancel,
    InputCancelled,
    HintScrollPreview,
    HintCoverageRefill,
    MediaSurfaceCreate,
    MediaSurfaceSetUniform,
    MediaSurfaceFree,
    MediaFrameShaderSet,
    MediaFrameShaderSetUniform,
}

pub fn down_tag(message: &Downlink) -> DownTag {
    match message {
        Downlink::OpenChannel { .. } => DownTag::OpenChannel,
        Downlink::CloseChannel { .. } => DownTag::CloseChannel,
        Downlink::Window { op, .. } => match op {
            WindowOp::Create(_) => DownTag::WindowCreate,
            WindowOp::Destroy => DownTag::WindowDestroy,
            WindowOp::SetTitle { .. } => DownTag::WindowSetTitle,
            WindowOp::SetGeometryHints { .. } => DownTag::WindowSetGeometryHints,
            WindowOp::Resize { .. } => DownTag::WindowResize,
            WindowOp::SetPosition { .. } => DownTag::WindowSetPosition,
            WindowOp::SetVisibility { .. } => DownTag::WindowSetVisibility,
            WindowOp::SetFullscreen { .. } => DownTag::WindowSetFullscreen,
            WindowOp::SetDecorations { .. } => DownTag::WindowSetDecorations,
            WindowOp::Iconify => DownTag::WindowIconify,
            WindowOp::Focus { .. } => DownTag::WindowFocus,
            WindowOp::RequestAttention { .. } => DownTag::WindowRequestAttention,
            WindowOp::SetPointerShape { .. } => DownTag::WindowSetPointerShape,
            WindowOp::WarpPointer { .. } => DownTag::WindowWarpPointer,
            WindowOp::ShowChild { .. } => DownTag::WindowShowChild,
            WindowOp::RemoveChild => DownTag::WindowRemoveChild,
            WindowOp::Opacity(OpacityOp::Accept) => DownTag::WindowOpacityAccept,
            WindowOp::Opacity(OpacityOp::SetLowerLimit { .. }) => {
                DownTag::WindowOpacitySetLowerLimit
            }
            WindowOp::Opacity(OpacityOp::SetRedirects { .. }) => DownTag::WindowOpacitySetRedirects,
            WindowOp::Opacity(OpacityOp::Retire) => DownTag::WindowOpacityRetire,
            WindowOp::ImeSpot { .. } => DownTag::WindowImeSpot,
        },
        Downlink::Present(op) => match op {
            PresentOp::Begin(_) => DownTag::PresentBegin,
            PresentOp::DefineRow { .. } => DownTag::PresentDefineRow,
            PresentOp::Tables(_) => DownTag::PresentTables,
            PresentOp::Windows(_) => DownTag::PresentWindows,
            PresentOp::Facts(_) => DownTag::PresentFacts,
            PresentOp::Commit { .. } => DownTag::PresentCommit,
            PresentOp::StoreReset => DownTag::PresentStoreReset,
        },
        Downlink::Asset(asset) => match asset {
            AssetDown::Offer { .. } => DownTag::AssetOffer,
            AssetDown::Forget { .. } => DownTag::AssetForget,
            AssetDown::ImageRealize { .. } => DownTag::AssetImageRealize,
            AssetDown::ImageDrop { .. } => DownTag::AssetImageDrop,
            AssetDown::Put { .. } => DownTag::AssetPut,
        },
        Downlink::Ui(ui) => match ui {
            UiOp::ShowPopupMenu { .. } => DownTag::UiShowPopupMenu,
            UiOp::HidePopupMenu { .. } => DownTag::UiHidePopupMenu,
            UiOp::PresentTooltip { .. } => DownTag::UiPresentTooltip,
            UiOp::DismissTooltip { .. } => DownTag::UiDismissTooltip,
            UiOp::VisualBell { .. } => DownTag::UiVisualBell,
        },
        Downlink::Config(config) => match config {
            ConfigOp::VisualConfig(_) => DownTag::ConfigVisualConfig,
            ConfigOp::ModifierPolicy(_) => DownTag::ConfigModifierPolicy,
            ConfigOp::FontRenderOptions(_) => DownTag::ConfigFontRenderOptions,
            ConfigOp::ScrollIndicators(_) => DownTag::ConfigScrollIndicators,
            ConfigOp::TitlebarHeight { .. } => DownTag::ConfigTitlebarHeight,
            ConfigOp::ShowFps(_) => DownTag::ConfigShowFps,
            ConfigOp::CornerRadius { .. } => DownTag::ConfigCornerRadius,
            ConfigOp::ExtraSpacing { .. } => DownTag::ConfigExtraSpacing,
            ConfigOp::ChildFrameStyle { .. } => DownTag::ConfigChildFrameStyle,
            ConfigOp::ConfirmQuit(_) => DownTag::ConfigConfirmQuit,
            ConfigOp::SelectedRoot(_) => DownTag::ConfigSelectedRoot,
            ConfigOp::CursorBlink(_) => DownTag::ConfigCursorBlink,
            ConfigOp::QuitKeys { .. } => DownTag::ConfigQuitKeys,
        },
        Downlink::Selection { op, .. } => match op {
            SelectionOp::Own { .. } => DownTag::SelectionOwn,
            SelectionOp::Disown { .. } => DownTag::SelectionDisown,
        },
        Downlink::ConvertReply { .. } => DownTag::ConvertReply,
        Downlink::SelectionData { .. } => DownTag::SelectionData,
        Downlink::InputProgress { .. } => DownTag::InputProgress,
        Downlink::Request { body, .. } => match body {
            RequestBody::SelectionGet { .. } => DownTag::RequestSelectionGet,
            RequestBody::SelectionExists { .. } => DownTag::RequestSelectionExists,
            RequestBody::OpenUri { .. } => DownTag::RequestOpenUri,
            RequestBody::TerminalText { .. } => DownTag::RequestTerminalText,
        },
        Downlink::Cancel { .. } => DownTag::Cancel,
        Downlink::InputCancelled { .. } => DownTag::InputCancelled,
        Downlink::Hint(hint) => match hint {
            HintOp::ScrollPreview { .. } => DownTag::HintScrollPreview,
            HintOp::CoverageRefill { .. } => DownTag::HintCoverageRefill,
        },
        Downlink::Media(media) => match media {
            MediaOp::SurfaceCreate { .. } => DownTag::MediaSurfaceCreate,
            MediaOp::SurfaceSetUniform { .. } => DownTag::MediaSurfaceSetUniform,
            MediaOp::SurfaceFree { .. } => DownTag::MediaSurfaceFree,
            MediaOp::FrameShaderSet { .. } => DownTag::MediaFrameShaderSet,
            MediaOp::FrameShaderSetUniform { .. } => DownTag::MediaFrameShaderSetUniform,
        },
    }
}

fn window(op: WindowOp) -> Downlink {
    Downlink::Window { frame: frame(), op }
}

pub fn down_sample(tag: DownTag) -> Downlink {
    let position = ScreenPosition { x: -4, y: 300 };
    match tag {
        DownTag::OpenChannel => Downlink::OpenChannel {
            channel: channel(),
            terminal: TerminalNumber::new(2),
            name: DisplayName("neo:laptop".into()),
        },
        DownTag::CloseChannel => Downlink::CloseChannel {
            channel: channel(),
            reason: ChannelCloseReason::Deleted,
        },
        DownTag::WindowCreate => window(WindowOp::Create(Box::new(WindowInit {
            channel: channel(),
            parent: None,
            size: WindowSize {
                width: 800,
                height: 600,
            },
            position: Some(position),
            visibility: WindowVisibility::Visible,
            decorations: Decorations::Decorated,
            title: "*scratch*".into(),
            hints: GuiFrameGeometryHints {
                base_width: 16,
                base_height: 8,
                min_width: 80,
                min_height: 40,
                width_inc: 8,
                height_inc: 16,
            },
            adoption: WindowAdoption::NewWindow,
        }))),
        DownTag::WindowDestroy => window(WindowOp::Destroy),
        DownTag::WindowSetTitle => window(WindowOp::SetTitle {
            title: "émacs".into(),
        }),
        DownTag::WindowSetGeometryHints => window(WindowOp::SetGeometryHints {
            hints: GuiFrameGeometryHints {
                base_width: 0,
                base_height: 0,
                min_width: 1,
                min_height: 1,
                width_inc: 7,
                height_inc: 15,
            },
        }),
        DownTag::WindowResize => window(WindowOp::Resize {
            size: WindowSize {
                width: 1024,
                height: 768,
            },
            request: GuiResizeRequestId::new(5),
        }),
        DownTag::WindowSetPosition => window(WindowOp::SetPosition { position }),
        DownTag::WindowSetVisibility => window(WindowOp::SetVisibility {
            visibility: WindowVisibility::Iconified,
        }),
        DownTag::WindowSetFullscreen => window(WindowOp::SetFullscreen {
            mode: WindowFullscreenMode::Maximized,
        }),
        DownTag::WindowSetDecorations => window(WindowOp::SetDecorations {
            decorations: Decorations::Undecorated,
        }),
        DownTag::WindowIconify => window(WindowOp::Iconify),
        DownTag::WindowFocus => window(WindowOp::Focus {
            activation: FocusActivation::NoActivate,
        }),
        DownTag::WindowRequestAttention => window(WindowOp::RequestAttention {
            urgency: AttentionUrgency::Urgent,
        }),
        DownTag::WindowSetPointerShape => window(WindowOp::SetPointerShape {
            shape: PointerShape(152),
        }),
        DownTag::WindowWarpPointer => window(WindowOp::WarpPointer { position }),
        DownTag::WindowShowChild => window(WindowOp::ShowChild {
            placement: ChildPlacement {
                parent: LiveDisplayFrameId::new(nz(1)),
                bounds: rect(),
                z_order: 2,
            },
        }),
        DownTag::WindowRemoveChild => window(WindowOp::RemoveChild),
        DownTag::WindowOpacityAccept => window(WindowOp::Opacity(OpacityOp::Accept)),
        DownTag::WindowOpacitySetLowerLimit => {
            window(WindowOp::Opacity(OpacityOp::SetLowerLimit { limit: 0.2 }))
        }
        DownTag::WindowOpacitySetRedirects => window(WindowOp::Opacity(OpacityOp::SetRedirects {
            enabled: Toggle::On,
        })),
        DownTag::WindowOpacityRetire => window(WindowOp::Opacity(OpacityOp::Retire)),
        DownTag::WindowImeSpot => window(WindowOp::ImeSpot { rect: rect() }),
        DownTag::PresentBegin => {
            Downlink::Present(present_op(|op| matches!(op, PresentOp::Begin(_))))
        }
        DownTag::PresentDefineRow => {
            Downlink::Present(present_op(|op| matches!(op, PresentOp::DefineRow { .. })))
        }
        DownTag::PresentTables => {
            Downlink::Present(present_op(|op| matches!(op, PresentOp::Tables(_))))
        }
        DownTag::PresentWindows => {
            Downlink::Present(present_op(|op| matches!(op, PresentOp::Windows(_))))
        }
        DownTag::PresentFacts => {
            Downlink::Present(present_op(|op| matches!(op, PresentOp::Facts(_))))
        }
        DownTag::PresentCommit => {
            Downlink::Present(present_op(|op| matches!(op, PresentOp::Commit { .. })))
        }
        DownTag::PresentStoreReset => Downlink::Present(PresentOp::StoreReset),
        DownTag::AssetOffer => Downlink::Asset(AssetDown::Offer {
            id: content_id(),
            kind: AssetKind::FontFile {
                container: FontContainer::Sfnt,
                faces: 1,
            },
            len: 123_456,
        }),
        DownTag::AssetForget => Downlink::Asset(AssetDown::Forget {
            id: AssetId::Session(nz(9)),
        }),
        DownTag::AssetImageRealize => Downlink::Asset(AssetDown::ImageRealize {
            image: ImageId::new(4),
            pixels: AssetId::Session(nz(9)),
            width: 64,
            height: 32,
        }),
        DownTag::AssetImageDrop => Downlink::Asset(AssetDown::ImageDrop {
            image: ImageId::new(4),
        }),
        DownTag::AssetPut => Downlink::Asset(AssetDown::Put {
            id: content_id(),
            offset: 65_536,
            bytes: Payload::from_bytes(b"glyf"),
            end: ChunkEnd::More,
        }),
        DownTag::UiShowPopupMenu => Downlink::Ui(UiOp::ShowPopupMenu {
            token: MenuToken {
                session: 1,
                revision: 2,
            },
            frame: frame(),
            request: Some(MenuBarRequestId(7)),
            placement: PopupPlacement::new(
                rect(),
                PopupPreferredSide::Below,
                Point { x: 1.0, y: 2.0 },
                PopupConstraintPolicy::None,
            ),
            items: vec![PopupMenuItem {
                kind: neomacs_display_protocol::menu::MenuItemKind::Label,
                help: Some("Cut the region".into()),
                label: "Cut".into(),
                shortcut: "C-w".into(),
                depth: 0,
            }],
            title: Some("Edit".into()),
            colors: Some(MenuColors {
                foreground: [0.0, 0.0, 0.0],
                background: [1.0, 1.0, 1.0],
            }),
        }),
        DownTag::UiHidePopupMenu => Downlink::Ui(UiOp::HidePopupMenu {
            token: MenuToken {
                session: 1,
                revision: 2,
            },
        }),
        DownTag::UiPresentTooltip => Downlink::Ui(UiOp::PresentTooltip {
            ticket: TooltipTicket::new(nz(3)),
            cause: Some(serial(11)),
            frame: frame(),
            anchor: rect(),
            layout: Payload::shared(TooltipLayout {
                size: WindowSize {
                    width: 96,
                    height: 16,
                },
                rows: vec![sample_row()],
                faces: vec![(FaceId::new(0), Face::default())],
            }),
        }),
        DownTag::UiDismissTooltip => Downlink::Ui(UiOp::DismissTooltip {
            ticket: TooltipTicket::new(nz(3)),
        }),
        DownTag::UiVisualBell => Downlink::Ui(UiOp::VisualBell { frame: frame() }),
        DownTag::ConfigVisualConfig => Downlink::Config(ConfigOp::VisualConfig(Payload::shared(
            VisualConfigDoc("{\"cursor\":{}}".into()),
        ))),
        DownTag::ConfigModifierPolicy => {
            Downlink::Config(ConfigOp::ModifierPolicy(ModifierPolicy::default()))
        }
        DownTag::ConfigFontRenderOptions => Downlink::Config(ConfigOp::FontRenderOptions(
            DesktopFontRenderOptions::default(),
        )),
        DownTag::ConfigScrollIndicators => Downlink::Config(ConfigOp::ScrollIndicators(Toggle::On)),
        DownTag::ConfigTitlebarHeight => {
            Downlink::Config(ConfigOp::TitlebarHeight { height: 28.0 })
        }
        DownTag::ConfigShowFps => Downlink::Config(ConfigOp::ShowFps(Toggle::Off)),
        DownTag::ConfigCornerRadius => Downlink::Config(ConfigOp::CornerRadius { radius: 6.5 }),
        DownTag::ConfigExtraSpacing => Downlink::Config(ConfigOp::ExtraSpacing {
            line: 2.0,
            letter: 0.5,
        }),
        DownTag::ConfigChildFrameStyle => Downlink::Config(ConfigOp::ChildFrameStyle {
            corner_radius: 4.0,
            shadow: ChildFrameShadow::On {
                layers: 3,
                offset: 2.0,
                opacity: 0.25,
            },
        }),
        DownTag::ConfigConfirmQuit => Downlink::Config(ConfigOp::ConfirmQuit(Toggle::On)),
        DownTag::ConfigSelectedRoot => Downlink::Config(ConfigOp::SelectedRoot(Some(frame()))),
        DownTag::ConfigCursorBlink => {
            Downlink::Config(ConfigOp::CursorBlink(CursorBlink::On { interval_ms: 500 }))
        }
        DownTag::ConfigQuitKeys => Downlink::Config(ConfigOp::QuitKeys {
            quit_char: '\u{7}',
            single_kboard_current: Some(channel()),
        }),
        DownTag::SelectionOwn => Downlink::Selection {
            channel: channel(),
            op: SelectionOp::Own {
                selection: SelectionName::Clipboard,
                generation: SelectionGeneration(4),
                content: SelectionContent::Inline(Payload::from_bytes(b"yank")),
            },
        },
        DownTag::SelectionDisown => Downlink::Selection {
            channel: channel(),
            op: SelectionOp::Disown {
                selection: SelectionName::Primary,
                generation: SelectionGeneration(5),
            },
        },
        DownTag::ConvertReply => Downlink::ConvertReply {
            convert: ConvertId::new(nz(2)),
            outcome: ConvertOutcome::Converted {
                target: SelectionTarget("UTF8_STRING".into()),
                len: 70_000,
            },
        },
        DownTag::SelectionData => Downlink::SelectionData {
            convert: ConvertId::new(nz(2)),
            offset: 0,
            bytes: Payload::from_bytes(b"region"),
            end: ChunkEnd::Last,
        },
        DownTag::InputProgress => Downlink::InputProgress {
            stream: InputStreamId::FIRST,
            consumed_through: 42,
        },
        DownTag::RequestSelectionGet => Downlink::Request {
            id: RequestId::new(nz(1)),
            channel: channel(),
            body: RequestBody::SelectionGet {
                selection: SelectionName::Clipboard,
                target: SelectionTarget("TARGETS".into()),
            },
        },
        DownTag::RequestSelectionExists => Downlink::Request {
            id: RequestId::new(nz(2)),
            channel: channel(),
            body: RequestBody::SelectionExists {
                selection: SelectionName::Secondary,
            },
        },
        DownTag::RequestOpenUri => Downlink::Request {
            id: RequestId::new(nz(3)),
            channel: channel(),
            body: RequestBody::OpenUri {
                uri: "https://www.gnu.org/software/emacs/".into(),
            },
        },
        DownTag::RequestTerminalText => Downlink::Request {
            id: RequestId::new(nz(4)),
            channel: channel(),
            body: RequestBody::TerminalText {
                id: TerminalId::new(6).expect("non-zero"),
            },
        },
        DownTag::Cancel => Downlink::Cancel {
            id: RequestId::new(nz(1)),
        },
        DownTag::InputCancelled => Downlink::InputCancelled {
            cancelled: vec![InputCancelled(InputReceipt::new(
                InputStreamId::FIRST,
                serial(12),
            ))],
        },
        DownTag::HintScrollPreview => Downlink::Hint(HintOp::ScrollPreview {
            frame: frame(),
            window: DisplayWindowId::new(1),
            presentation: PresentationId::new(9),
            epoch: 2,
            publication: decoded(&[7]),
            offset: -48.0,
            inputs: vec![serial(13), serial(14)],
        }),
        DownTag::HintCoverageRefill => Downlink::Hint(HintOp::CoverageRefill {
            frame: frame(),
            window: DisplayWindowId::new(1),
            epoch: 2,
        }),
        DownTag::MediaSurfaceCreate => Downlink::Media(MediaOp::SurfaceCreate {
            id: SurfaceId::new(5),
            source: SurfaceSourceSpec::Shader {
                language: SurfaceShaderLanguage::Wgsl,
                source: "fn mainImage() {}".into(),
                uniforms: vec![SurfaceUniformInit {
                    name: "uSpeed".into(),
                    value: [1.0, 0.0, 0.0, 0.0],
                    components: 1,
                }],
                channel0: Some(SurfaceChannelSource::Surface(4)),
            },
            size: WindowSize {
                width: 256,
                height: 128,
            },
            animation: SurfaceAnimation::Animated { fps: Some(30) },
            eviction: SurfaceEviction::Recreatable,
        }),
        DownTag::MediaSurfaceSetUniform => Downlink::Media(MediaOp::SurfaceSetUniform {
            id: SurfaceId::new(5),
            name: "uSpeed".into(),
            value: [2.0, 0.0, 0.0, 0.0],
        }),
        DownTag::MediaSurfaceFree => Downlink::Media(MediaOp::SurfaceFree {
            id: SurfaceId::new(5),
        }),
        DownTag::MediaFrameShaderSet => Downlink::Media(MediaOp::FrameShaderSet {
            request: FrameShaderRequestId::new(nz(1)),
            shader: Some(FrameShader {
                source: "@fragment fn main() {}".into(),
                language: SurfaceShaderLanguage::Wgsl,
                uniforms: Vec::new(),
            }),
        }),
        DownTag::MediaFrameShaderSetUniform => Downlink::Media(MediaOp::FrameShaderSetUniform {
            request: FrameShaderRequestId::new(nz(1)),
            name: "uTint".into(),
            value: [0.5, 0.5, 0.5, 1.0],
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumIter)]
pub enum UpTag {
    ChannelOpened,
    ChannelRefused,
    InputKey,
    InputImeCommit,
    InputPointer,
    InputPresentedPointer,
    InputToolBarClick,
    InputMenuBarClick,
    InputMenuSelection,
    InputTooltipDismissed,
    InputFileDrop,
    InputCloseRequested,
    InputFocus,
    InputMoved,
    InputIconified,
    InputDeiconified,
    InputResizeCompleted,
    InputApplicationQuit,
    PointerMotion,
    WindowGeometry,
    PresentationReceived,
    PresentationActivated,
    PresentationDiscarded,
    PresentationRetired,
    PresentationRejected,
    PresentationStoreAck,
    SelectionState,
    Convert,
    FactMonitors,
    FactDesktop,
    FactResources,
    FactCapabilities,
    FactDisplayReset,
    AssetHave,
    AssetWant,
    AssetStored,
    AssetEvicted,
    AssetResidencyLost,
    ReplySelectionData,
    ReplySelectionExists,
    ReplyUriOpened,
    ReplyTerminalText,
    ReplyTimedOut,
    ReplyRefused,
    ReplyFailed,
    MediaSurfaceCreateFailed,
    MediaFrameShaderAck,
    MediaFrameShaderFailed,
    InputOverflow,
}

pub fn up_tag(message: &Uplink) -> UpTag {
    match message {
        Uplink::ChannelOpened { .. } => UpTag::ChannelOpened,
        Uplink::ChannelRefused { .. } => UpTag::ChannelRefused,
        Uplink::Input { event, .. } => match event {
            InputEdge::Key { .. } => UpTag::InputKey,
            InputEdge::ImeCommit { .. } => UpTag::InputImeCommit,
            InputEdge::Pointer { .. } => UpTag::InputPointer,
            InputEdge::PresentedPointer { .. } => UpTag::InputPresentedPointer,
            InputEdge::ToolBarClick { .. } => UpTag::InputToolBarClick,
            InputEdge::MenuBarClick { .. } => UpTag::InputMenuBarClick,
            InputEdge::MenuSelection { .. } => UpTag::InputMenuSelection,
            InputEdge::TooltipDismissed { .. } => UpTag::InputTooltipDismissed,
            InputEdge::FileDrop { .. } => UpTag::InputFileDrop,
            InputEdge::CloseRequested { .. } => UpTag::InputCloseRequested,
            InputEdge::Focus { .. } => UpTag::InputFocus,
            InputEdge::Moved { .. } => UpTag::InputMoved,
            InputEdge::Iconified { .. } => UpTag::InputIconified,
            InputEdge::Deiconified { .. } => UpTag::InputDeiconified,
            InputEdge::ResizeCompleted { .. } => UpTag::InputResizeCompleted,
            InputEdge::ApplicationQuit { .. } => UpTag::InputApplicationQuit,
        },
        Uplink::PointerMotion { .. } => UpTag::PointerMotion,
        Uplink::WindowGeometry { .. } => UpTag::WindowGeometry,
        Uplink::Presentation(ack) => match ack {
            PresentationAck::Received { .. } => UpTag::PresentationReceived,
            PresentationAck::Activated { .. } => UpTag::PresentationActivated,
            PresentationAck::Discarded { .. } => UpTag::PresentationDiscarded,
            PresentationAck::Retired { .. } => UpTag::PresentationRetired,
            PresentationAck::Rejected { .. } => UpTag::PresentationRejected,
            PresentationAck::StoreAck { .. } => UpTag::PresentationStoreAck,
        },
        Uplink::SelectionState { .. } => UpTag::SelectionState,
        Uplink::Convert { .. } => UpTag::Convert,
        Uplink::Fact(fact) => match fact {
            LevelFact::Monitors { .. } => UpTag::FactMonitors,
            LevelFact::Desktop { .. } => UpTag::FactDesktop,
            LevelFact::Resources { .. } => UpTag::FactResources,
            LevelFact::Capabilities { .. } => UpTag::FactCapabilities,
            LevelFact::DisplayReset { .. } => UpTag::FactDisplayReset,
        },
        Uplink::Asset(asset) => match asset {
            AssetUp::Have { .. } => UpTag::AssetHave,
            AssetUp::Want { .. } => UpTag::AssetWant,
            AssetUp::Stored { .. } => UpTag::AssetStored,
            AssetUp::Evicted { .. } => UpTag::AssetEvicted,
            AssetUp::ResidencyLost { .. } => UpTag::AssetResidencyLost,
        },
        Uplink::Reply { outcome, .. } => match outcome {
            ReplyOutcome::Value(ReplyValue::SelectionData { .. }) => UpTag::ReplySelectionData,
            ReplyOutcome::Value(ReplyValue::SelectionExists(_)) => UpTag::ReplySelectionExists,
            ReplyOutcome::Value(ReplyValue::UriOpened) => UpTag::ReplyUriOpened,
            ReplyOutcome::Value(ReplyValue::TerminalText(_)) => UpTag::ReplyTerminalText,
            ReplyOutcome::TimedOut => UpTag::ReplyTimedOut,
            ReplyOutcome::Refused(_) => UpTag::ReplyRefused,
            ReplyOutcome::Failed { .. } => UpTag::ReplyFailed,
        },
        Uplink::Media(event) => match event {
            MediaEvent::SurfaceCreateFailed { .. } => UpTag::MediaSurfaceCreateFailed,
            MediaEvent::FrameShaderAck { .. } => UpTag::MediaFrameShaderAck,
            MediaEvent::FrameShaderFailed { .. } => UpTag::MediaFrameShaderFailed,
        },
        Uplink::InputOverflow { .. } => UpTag::InputOverflow,
    }
}

fn input(event: InputEdge) -> Uplink {
    Uplink::Input {
        serial: serial(21),
        time: DisplayTime::from_millis(123_456),
        event,
    }
}

fn monitor() -> MonitorInfo {
    MonitorInfo {
        x: 0,
        y: 0,
        width: 2560,
        height: 1440,
        scale: 1.5,
        width_mm: 600,
        height_mm: 340,
        name: Some("DP-1".into()),
    }
}

fn reply(outcome: ReplyOutcome) -> Uplink {
    Uplink::Reply {
        id: RequestId::new(nz(1)),
        outcome,
    }
}

pub fn up_sample(tag: UpTag) -> Uplink {
    let at = LayoutPosition { x: 12.5, y: 40.0 };
    let presentation = PresentationId::new(9);
    match tag {
        UpTag::ChannelOpened => Uplink::ChannelOpened {
            channel: channel(),
            facts: Box::new(DisplayFacts {
                monitors: vec![monitor()],
                system_fonts: SystemFonts::default(),
                text_dpi: 96.0,
                render_options: None,
                capabilities: CapabilitySet(vec![Capability {
                    name: "frames".into(),
                    version: 1,
                    params: Vec::new(),
                }]),
                resources: vec![Resource {
                    name: "Emacs.font".into(),
                    value: "Monospace-12".into(),
                }],
            }),
        },
        UpTag::ChannelRefused => Uplink::ChannelRefused {
            channel: channel(),
            reason: ChannelRefusal::Policy,
        },
        UpTag::InputKey => input(InputEdge::Key {
            frame: frame(),
            key: FrontendKey::Character('x'),
            mods: ModifierBits::SHIFT,
            text: Some("X".into()),
        }),
        UpTag::InputImeCommit => input(InputEdge::ImeCommit {
            frame: frame(),
            text: "日本".into(),
        }),
        UpTag::InputPointer => input(InputEdge::Pointer {
            frame: frame(),
            target: PointerTarget::Unpresented,
            at,
            action: PointerAction::Scroll {
                delta: ScrollDelta::Pixels { x: 0.0, y: -3.5 },
            },
            mods: ModifierBits::NONE,
        }),
        UpTag::InputPresentedPointer => input(InputEdge::PresentedPointer {
            frame: frame(),
            presentation,
            interaction: 4,
            state: ButtonState::Pressed,
            button: 1,
            at,
        }),
        UpTag::InputToolBarClick => input(InputEdge::ToolBarClick {
            frame: frame(),
            presentation,
            item: 2,
        }),
        UpTag::InputMenuBarClick => input(InputEdge::MenuBarClick {
            frame: frame(),
            presentation,
            request: Some(MenuBarRequestId(3)),
            index: 1,
            key: "edit".into(),
            anchor: rect(),
        }),
        UpTag::InputMenuSelection => input(InputEdge::MenuSelection {
            token: MenuToken {
                session: 1,
                revision: 2,
            },
            choice: MenuChoice::Item(3),
        }),
        UpTag::InputTooltipDismissed => input(InputEdge::TooltipDismissed {
            ticket: TooltipTicket::new(nz(3)),
            reason: TooltipDismissal::PointerLeft,
        }),
        UpTag::InputFileDrop => input(InputEdge::FileDrop {
            frame: frame(),
            target: PointerTarget::Unpresented,
            uris: vec!["file://laptop/home/me/notes.org".into()],
        }),
        UpTag::InputCloseRequested => input(InputEdge::CloseRequested { frame: frame() }),
        UpTag::InputFocus => input(InputEdge::Focus {
            frame: frame(),
            state: FocusState::Focused,
            tooltip_stamp: Some(8),
        }),
        UpTag::InputMoved => input(InputEdge::Moved {
            frame: frame(),
            position: ScreenPosition { x: 10, y: 20 },
        }),
        UpTag::InputIconified => input(InputEdge::Iconified { frame: frame() }),
        UpTag::InputDeiconified => input(InputEdge::Deiconified { frame: frame() }),
        UpTag::InputResizeCompleted => input(InputEdge::ResizeCompleted {
            request: GuiResizeRequestId::new(5).expect("non-zero"),
        }),
        UpTag::InputApplicationQuit => input(InputEdge::ApplicationQuit { channel: channel() }),
        UpTag::PointerMotion => Uplink::PointerMotion {
            frame: frame(),
            target: PointerTarget::Unpresented,
            at,
            mods: ModifierBits::NONE,
        },
        UpTag::WindowGeometry => Uplink::WindowGeometry {
            frame: frame(),
            size: WindowSize {
                width: 1280,
                height: 720,
            },
            scale: 2.0,
        },
        UpTag::PresentationReceived => Uplink::Presentation(PresentationAck::Received {
            frame: frame(),
            presentation,
        }),
        UpTag::PresentationActivated => Uplink::Presentation(PresentationAck::Activated {
            frame: frame(),
            presentation,
            pending_assets: vec![content_id()],
        }),
        UpTag::PresentationDiscarded => Uplink::Presentation(PresentationAck::Discarded {
            frame: frame(),
            presentation,
        }),
        UpTag::PresentationRetired => {
            Uplink::Presentation(PresentationAck::Retired { presentation })
        }
        UpTag::PresentationRejected => Uplink::Presentation(PresentationAck::Rejected {
            frame: frame(),
            presentation,
            reason: RejectReason::UnknownSlot(17),
        }),
        UpTag::PresentationStoreAck => Uplink::Presentation(PresentationAck::StoreAck {
            through: UpdateSeq(12),
        }),
        UpTag::SelectionState => Uplink::SelectionState {
            channel: channel(),
            selection: SelectionName::Clipboard,
            owner: SelectionOwner::Ours {
                generation: SelectionGeneration(4),
            },
            sequence: Some(99),
        },
        UpTag::Convert => Uplink::Convert {
            convert: ConvertId::new(nz(2)),
            channel: channel(),
            selection: SelectionName::Clipboard,
            target: SelectionTarget("text/plain;charset=utf-8".into()),
            generation: SelectionGeneration(4),
        },
        UpTag::FactMonitors => Uplink::Fact(LevelFact::Monitors {
            version: FactVersion(2),
            list: vec![monitor()],
        }),
        UpTag::FactDesktop => Uplink::Fact(LevelFact::Desktop {
            version: FactVersion(1),
            system_fonts: SystemFonts::default(),
            text_dpi: 109.0,
            render_options: Some(DesktopFontRenderOptions::default()),
        }),
        UpTag::FactResources => Uplink::Fact(LevelFact::Resources {
            version: FactVersion(1),
            db: vec![Resource {
                name: "Emacs.background".into(),
                value: "black".into(),
            }],
        }),
        UpTag::FactCapabilities => Uplink::Fact(LevelFact::Capabilities {
            version: FactVersion(3),
            set: CapabilitySet(vec![Capability {
                name: "selections".into(),
                version: 1,
                params: vec![("clipboard".into(), "exact".into())],
            }]),
        }),
        UpTag::FactDisplayReset => Uplink::Fact(LevelFact::DisplayReset {
            reason: DisplayResetReason::DeviceLost,
        }),
        UpTag::AssetHave => Uplink::Asset(AssetUp::Have {
            ids: vec![content_id()],
        }),
        UpTag::AssetWant => Uplink::Asset(AssetUp::Want {
            ids: vec![content_id()],
            priority: WantPriority::Visible,
        }),
        UpTag::AssetStored => Uplink::Asset(AssetUp::Stored { id: content_id() }),
        UpTag::AssetEvicted => Uplink::Asset(AssetUp::Evicted { id: content_id() }),
        UpTag::AssetResidencyLost => Uplink::Asset(AssetUp::ResidencyLost {
            image: ImageId::new(4),
        }),
        UpTag::ReplySelectionData => reply(ReplyOutcome::Value(ReplyValue::SelectionData {
            target: SelectionTarget("UTF8_STRING".into()),
            bytes: Some(b"pasted".to_vec()),
        })),
        UpTag::ReplySelectionExists => reply(ReplyOutcome::Value(ReplyValue::SelectionExists(
            SelectionPresence::Present,
        ))),
        UpTag::ReplyUriOpened => reply(ReplyOutcome::Value(ReplyValue::UriOpened)),
        UpTag::ReplyTerminalText => {
            reply(ReplyOutcome::Value(ReplyValue::TerminalText("$ ".into())))
        }
        UpTag::ReplyTimedOut => reply(ReplyOutcome::TimedOut),
        UpTag::ReplyRefused => reply(ReplyOutcome::Refused(PolicyRefusal::SchemeNotAllowed {
            scheme: "javascript".into(),
        })),
        UpTag::ReplyFailed => reply(ReplyOutcome::Failed {
            detail: "no clipboard".into(),
        }),
        UpTag::MediaSurfaceCreateFailed => Uplink::Media(MediaEvent::SurfaceCreateFailed {
            id: SurfaceId::new(5),
            error: "device limit".into(),
        }),
        UpTag::MediaFrameShaderAck => Uplink::Media(MediaEvent::FrameShaderAck {
            request: FrameShaderRequestId::new(nz(1)),
            execution: FrameShaderExecution::Running,
        }),
        UpTag::MediaFrameShaderFailed => Uplink::Media(MediaEvent::FrameShaderFailed {
            request: FrameShaderRequestId::new(nz(1)),
            error: "naga: unknown identifier".into(),
        }),
        UpTag::InputOverflow => Uplink::InputOverflow {
            from: serial(30),
            to: serial(40),
        },
    }
}

/// The family a fixture belongs to: the prefix of its tag's name.
pub fn family(name: &str) -> &'static str {
    const FAMILIES: [&str; 16] = [
        "down.Window",
        "down.Present",
        "down.Asset",
        "down.Ui",
        "down.Config",
        "down.Request",
        "down.Hint",
        "down.Media",
        "up.Input",
        "up.Presentation",
        "up.Fact",
        "up.Asset",
        "up.Reply",
        "up.Media",
        "down.",
        "up.",
    ];
    FAMILIES
        .into_iter()
        .find(|prefix| name.starts_with(prefix))
        .expect("every fixture is downlink or uplink")
}

/// Every fixture, in the canonical corpus order (built once per process).
pub fn golden_corpus() -> Vec<GoldenFixture> {
    static CORPUS: std::sync::OnceLock<Vec<GoldenFixture>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(build_corpus).clone()
}

fn build_corpus() -> Vec<GoldenFixture> {
    let codec = WireCodec::default();
    let down = DownTag::iter().map(|tag| GoldenFixture {
        name: format!("down.{tag:?}"),
        bytes: codec.encode(&down_sample(tag)).expect("a fixture encodes"),
    });
    let up = UpTag::iter().map(|tag| GoldenFixture {
        name: format!("up.{tag:?}"),
        bytes: codec.encode(&up_sample(tag)).expect("a fixture encodes"),
    });
    down.chain(up).collect()
}

/// The corpus lines of one family.
pub fn family_text(fixtures: &[GoldenFixture], wanted: &str) -> String {
    let selected: Vec<GoldenFixture> = fixtures
        .iter()
        .filter(|fixture| family(&fixture.name) == wanted)
        .cloned()
        .collect();
    neomacs_display_wire::schema::corpus_text(&selected)
}
