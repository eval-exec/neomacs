//! Thread communication infrastructure for two-thread architecture.
//!
//! Provides channels between the evaluator and render threads. Input wakeups
//! are owned by the evaluator's cross-platform wait notifier after the input
//! bridge queues a converted event.

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use neomacs_display_protocol::SealedFramePresentation;
use neomacs_display_protocol::{
    ImageColorContext, ImageId, ImageLoadToken, ImageMaskPolicy, ImageRealization, ImageRotation,
    ImageSizeSpec, SelectionOwner, VideoId,
};
pub use neomacs_display_protocol::{
    ImageStateEvent, MenuBarItem, PopupMenuItem, TabBarItem, ToolBarImageSource, ToolBarItem,
    ToolBarItemType, VisualConfig,
};
use neomacs_video_model::{PlaybackAction, VideoDiagnostics, VideoOpenRequest};
use neovm_core::window::GuiFrameGeometryHints;

/// Selection addressed by a clipboard request.
///
/// `Clipboard` is always the system clipboard.  `Primary` is the X11 or
/// Wayland primary selection on Linux and a process-local store elsewhere,
/// matching GNU w32's Lisp property.  GNU NS instead uses a named pasteboard
/// and can observe foreign ownership; the clipboard runtime documents that
/// explicit divergence beside its process-local state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ClipboardSelection {
    Clipboard,
    Primary,
}

/// Monitor information transported from the frontend to the evaluator.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub width_mm: i32,
    pub height_mm: i32,
    pub name: Option<String>,
}

/// Frame-local logical coordinates shared by one pointer action and its
/// presentation-qualified target. Keeping the coordinates here prevents the
/// semantic hit and raw action from disagreeing about where the input occurred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerPosition {
    pub x: f32,
    pub y: f32,
    pub target_frame_id: u64,
}

/// How a pointer position relates to the immutable presentation on screen.
///
/// `Unpresented` is explicit for exposed/native surface area. A producer cannot
/// accidentally omit presentation state from a pointer action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerTarget {
    Presented {
        presentation: u64,
        hit: Option<neomacs_display_protocol::PresentedHit>,
    },
    Unpresented,
}

/// The unit carried by a scroll delta. This replaces the invalid state where a
/// boolean precision flag can disagree with the meaning of the numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

/// Pointer action interpreted at one [`PointerPosition`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerAction {
    Button {
        button: u32,
        pressed: bool,
        modifiers: u32,
    },
    Move {
        modifiers: u32,
    },
    Scroll {
        delta: ScrollDelta,
        modifiers: u32,
    },
}

/// Atomic display-to-evaluator pointer input.
///
/// The transport has one variant for native move, button, and scroll actions,
/// so target qualification cannot be sent, reordered, or forgotten separately.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedPointerInput {
    pub position: PointerPosition,
    pub target: PointerTarget,
    pub action: PointerAction,
}

/// Input event from render thread to Emacs
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Bytes read directly from a Unix TTY.
    ///
    /// This transport fact deliberately carries no terminal-sequence or
    /// modifier interpretation.  The evaluator applies
    /// `keyboard-coding-system` and `input-decode-map` in input order.
    RawTtyBytes {
        bytes: Vec<u8>,
        emacs_frame_id: u64,
    },
    Key {
        keysym: u32,
        modifiers: u32,
        pressed: bool,
        /// Emacs frame_id of the window that produced the key event
        emacs_frame_id: u64,
    },
    PositionedPointer(PositionedPointerInput),
    WindowResize {
        width: u32,
        height: u32,
        /// Physical device pixels per logical Emacs pixel.
        scale_factor: f64,
        /// Emacs frame_id of the window that resized
        emacs_frame_id: u64,
    },
    WindowClose {
        /// Emacs frame_id of the window being closed
        emacs_frame_id: u64,
    },
    WindowFocus {
        focused: bool,
        /// Emacs frame_id of the window that gained/lost focus
        emacs_frame_id: u64,
    },
    /// Monitor configuration changed on the active terminal.
    MonitorsChanged {
        monitors: Vec<MonitorInfo>,
    },
    /// The wgpu device was lost (user shader hang → driver reset) and the
    /// render thread rebuilt its GPU state from scratch. Every renderer-side
    /// media object (decoded images, videos, webkit textures, shader
    /// surfaces, the frame post shader) is gone; the evaluator must
    /// re-resolve them and force a full redisplay.
    DisplayReset,
    /// Backend-neutral embedded-browser lifecycle and page event.
    WebView(neomacs_webview::WebViewEvent),
    /// Image decoding completed or renderer residency was lost.
    ImageStateChanged {
        event: ImageStateEvent,
    },
    /// A shader surface failed to build on the render thread AFTER the Lisp
    /// thread's naga pre-validation accepted it (the naga-accepts /
    /// wgpu-rejects edge, e.g. a device limit). Carries the surface id and the
    /// renderer's error so the evaluator can surface it to Lisp instead of the
    /// failure only living in a log line + a silently-blank quad.
    SurfaceCreateFailed {
        id: u32,
        error: String,
    },
    /// A current full-frame shader request passed evaluator-side validation
    /// but was rejected later by the active device or a concurrently changed
    /// quality policy. The evaluator reports this through
    /// `neomacs-frame-shader-error-functions`.
    FrameShaderFailed {
        error: String,
    },
    /// Terminal creation failed after the evaluator reserved its typed ID.
    #[cfg(feature = "neo-term")]
    TerminalCreateFailed {
        id: crate::terminal::TerminalId,
        error: String,
    },
    /// Terminal child process exited
    #[cfg(feature = "neo-term")]
    TerminalExited {
        id: crate::terminal::TerminalId,
    },
    /// Terminal title changed
    #[cfg(feature = "neo-term")]
    TerminalTitleChanged {
        id: crate::terminal::TerminalId,
        title: String,
    },
    /// Popup menu selection made (index into menu items, -1 = cancelled)
    MenuSelection {
        index: i32,
    },
    /// File(s) dropped onto the window
    FileDrop {
        paths: Vec<String>,
        x: f32,
        y: f32,
    },
    /// Toolbar button clicked (index into toolbar items)
    ToolBarClick {
        index: i32,
        emacs_frame_id: u64,
    },
    /// Pointer observation resolved against an immutable displayed presentation.
    PresentedPointer {
        presentation: u64,
        interaction: u32,
        pressed: bool,
        button: u8,
        x: f32,
        y: f32,
        emacs_frame_id: u64,
    },
    /// Renderer installed this presentation as its drawing and hit-test source.
    PresentationActivated {
        presentation: u64,
        emacs_frame_id: u64,
    },
    /// Renderer rejected or superseded this presentation before activation.
    PresentationDiscarded {
        presentation: u64,
        emacs_frame_id: u64,
    },
    /// Renderer no longer displays or generates hits for this presentation.
    PresentationRetired {
        presentation: u64,
    },
    /// Menu bar item clicked. `menu_x` is the Emacs menu-bar column used by
    /// legacy Lisp paths; `key` is the exact rendered top-level menu key; and
    /// `anchor` is the frame-local logical-pixel rectangle used by the native
    /// popup renderer.
    MenuBarClick {
        index: i32,
        key: String,
        menu_x: f32,
        anchor: PopupAnchorRect,
        emacs_frame_id: u64,
    },
}

pub type PopupAnchorRect = neomacs_display_protocol::Rect;

/// Frame reference in commands flowing from Emacs to the render thread.
///
/// Replaces raw `u64` `emacs_frame_id` — no sentinel values.
/// Matches GNU Emacs convention: 0 is never a valid frame ID
/// (`frame_next_id = 1` in GNU Emacs `frame.c:343`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameRef {
    /// Route to the primary frame (resolved at render-time).
    Primary,
    /// Route to a specific frame by its Emacs-assigned ID.
    Frame(u64),
}

impl FrameRef {
    pub fn raw_id(&self) -> u64 {
        match self {
            Self::Primary => 0,
            Self::Frame(id) => *id,
        }
    }
}

impl From<FrameRef> for u64 {
    fn from(f: FrameRef) -> u64 {
        f.raw_id()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFullscreenMode {
    None,
    Fullboth,
    Fullscreen,
    Fullwidth,
    Fullheight,
    Maximized,
}

/// Native-window projection of GNU's visible/iconified/invisible frame state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowVisibility {
    Visible,
    Iconified,
    Invisible,
}

/// Lifecycle commands for the render thread.
#[derive(Debug)]
pub enum LifecycleCommand {
    /// Shutdown the render thread
    Shutdown,
    /// Suspend the active TTY frontend.
    SuspendTty,
    /// Resume the active TTY frontend.
    ResumeTty,
}

/// Window and chrome management commands.
#[derive(Debug)]
pub enum WindowCommand {
    /// Scroll blit pixels within pixel buffer
    ScrollBlit {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        from_y: i32,
        to_y: i32,
        bg_r: f32,
        bg_g: f32,
        bg_b: f32,
    },
    /// Change the mouse pointer cursor shape (arrow, hand, ibeam, etc.)
    SetMouseCursor { cursor_type: i32 },
    /// Warp (move) the mouse pointer to given pixel position
    WarpMouse { x: i32, y: i32 },
    /// Set the window title
    SetWindowTitle { title: String },
    /// Set the title for a specific GUI frame window.
    /// `frame.raw_id() == 0` also targets the adopted primary window.
    SetFrameWindowTitle { frame: FrameRef, title: String },
    /// Set fullscreen/maximized state for a GUI frame window.
    /// `frame.raw_id() == 0` also targets the adopted primary window.
    SetWindowFullscreen {
        frame: FrameRef,
        mode: WindowFullscreenMode,
    },
    /// Show or hide the window manager's title bar and border -- the
    /// `undecorated` frame parameter.
    SetWindowDecorations { decorated: bool },
    /// Apply the complete visibility state to a specific GUI frame window.
    SetWindowVisibility {
        frame: FrameRef,
        visibility: WindowVisibility,
    },
    /// Set window position
    SetWindowPosition { x: i32, y: i32 },
    /// Request window inner size change
    SetWindowSize { width: u32, height: u32 },
    /// Request resizing a specific GUI frame window.
    /// `frame.raw_id() == 0` also targets the adopted primary window.
    ResizeWindow {
        frame: FrameRef,
        width: u32,
        height: u32,
        geometry_hints: GuiFrameGeometryHints,
    },
    /// Update geometry hints for a specific GUI frame window.
    /// `frame.raw_id() == 0` also targets the adopted primary window.
    SetFrameGeometryHints {
        frame: FrameRef,
        geometry_hints: GuiFrameGeometryHints,
    },
    /// Set window decorations (title bar, borders)
    SetWindowDecorated { decorated: bool },
    /// Create a new OS window for a top-level Emacs frame
    CreateWindow {
        frame: FrameRef,
        width: u32,
        height: u32,
        title: String,
        geometry_hints: GuiFrameGeometryHints,
    },
    /// Associate the already-created primary OS window with its real Emacs frame ID.
    AdoptPrimaryFrame { frame: FrameRef },
    /// Destroy an OS window for a top-level Emacs frame
    DestroyWindow { frame: FrameRef },
    /// Mark a child frame visible again.
    ShowChildFrame { frame_id: u64 },
    /// Remove a child frame (sent when frame is deleted, unparented, or hidden)
    RemoveChildFrame { frame_id: u64 },
    /// Request window attention (urgency hint / taskbar flash)
    RequestAttention { urgent: bool },
}

/// Typed commands for one stable compositor-owned video session.
///
/// This is intentionally a single state-machine command instead of separate
/// loosely-related asset variants. Adding a playback operation now makes the
/// render-thread match exhaustive at compile time.
#[derive(Debug)]
pub enum VideoSessionCommand {
    Open {
        id: VideoId,
        request: VideoOpenRequest,
    },
    Control {
        id: VideoId,
        action: PlaybackAction,
    },
    Close {
        id: VideoId,
    },
    /// Request one coherent renderer-owned path and pool snapshot.
    Diagnostics {
        reply: Sender<Result<VideoDiagnostics, String>>,
    },
    /// Start a fresh observation epoch and acknowledge after every
    /// renderer/native counter has crossed the same boundary. The returned
    /// zero-point snapshot is captured in that same render-thread command, so
    /// callers cannot accidentally subtract post-boundary work.
    BeginMeasurementEpoch {
        reply: Sender<Result<VideoDiagnostics, String>>,
    },
}

/// Content source for a shader surface
/// (`docs/display-engine/SHADER_SURFACES.md`).
#[derive(Debug)]
pub enum SurfaceSource {
    /// User shader source (WGSL or Shadertoy-dialect GLSL) defining
    /// `mainImage`; the render thread composes it with the generated prelude.
    /// `uniforms` carries the named user uniforms in slot order with initial
    /// values; `channel0` optionally names another surface sampled as
    /// `iChannel0`.
    Shader {
        language: neomacs_renderer_wgpu::shader_surface::SurfaceShaderLanguage,
        source: String,
        uniforms: Vec<neomacs_renderer_wgpu::SurfaceUniformInit>,
        channel0: Option<neomacs_renderer_wgpu::shader_surface::SurfaceChannelSource>,
    },
    /// Raw RGBA8 pixels, row-major, tightly packed.
    Pixels { data: Vec<u8> },
}

/// Asset and embedded-content commands.
#[derive(Debug)]
pub enum AssetCommand {
    /// Load image from file (async, ID pre-allocated)
    ImageLoadFile {
        load: ImageLoadToken,
        path: String,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        /// Immutable logical/device geometry captured for this load.
        realization: ImageRealization,
        /// Colors used by face-sensitive image formats and image-cache identity.
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: neomacs_display_protocol::ImageFrameIndex,
        sequence: neomacs_display_protocol::ImageSequenceId,
    },
    /// Load image from encoded data bytes (PNG, JPEG, SVG, etc.)
    ImageLoadData {
        load: ImageLoadToken,
        data: neovm_core::emacs_core::image_catalog::ImageDataSource,
        size: ImageSizeSpec,
        rotation: ImageRotation,
        /// Immutable logical/device geometry captured for this load.
        realization: ImageRealization,
        /// Colors used by face-sensitive image formats and image-cache identity.
        colors: ImageColorContext,
        mask: ImageMaskPolicy,
        frame: neomacs_display_protocol::ImageFrameIndex,
        sequence: neomacs_display_protocol::ImageSequenceId,
    },
    /// Load image from raw ARGB32 pixel data
    ImageLoadArgb32 {
        load: ImageLoadToken,
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
    },
    /// Load image from raw RGB24 pixel data
    ImageLoadRgb24 {
        load: ImageLoadToken,
        data: Vec<u8>,
        width: u32,
        height: u32,
        stride: u32,
    },
    /// Retire an image after its last render presentation releases it.
    ImageRetire { image: ImageId },
    /// Retire CPU-side animation decoder/compositor state independently of
    /// frame textures.
    ImageSequenceRetire {
        retirement: neomacs_display_protocol::ImageSequenceRetirement,
    },
    /// Debug-only: latch the device-lost flag so the full device-loss
    /// recovery path (GPU rebuild + `InputEvent::DisplayReset`) runs against
    /// a healthy device. Sent by the hidden `neomacs--debug-lose-device`
    /// builtin; never used in production paths.
    DebugSimulateDeviceLoss,
    /// Backend-neutral embedded-browser operation.
    WebView(neomacs_webview::WebViewCommand),
    /// Create a shader surface (docs/display-engine/SHADER_SURFACES.md)
    SurfaceCreate {
        id: u32,
        source: SurfaceSource,
        width: u32,
        height: u32,
        animate: bool,
        /// Per-surface animation frame-rate cap (`:fps`); `None` = display
        /// refresh. Throttles both the surface's own re-render and the
        /// compositor demand cadence when it is the only active demand.
        fps: Option<u32>,
        /// Whether the media-budget eviction driver may free this surface
        /// under memory pressure. Only the declarative display-spec resolver
        /// sends true: it re-runs on every redisplay walk of a visible spec
        /// and recreates the surface, so eviction is invisible. Imperative
        /// `neomacs-surface-create` ids are held bare by Lisp — evicting one
        /// would blank it permanently — so that path sends false.
        recreatable: bool,
    },
    /// Update one named uniform on a shader surface
    SurfaceSetUniform {
        id: u32,
        name: String,
        value: [f32; 4],
    },
    /// Free a shader surface
    SurfaceFree { id: u32 },
    /// Install (Some, already composed+validated source in the given
    /// language, plus the user uniforms in slot order) or remove (None) the
    /// full-frame post shader
    FrameShaderSet {
        request: FrameShaderRequestId,
        composed: Option<(
            String,
            neomacs_renderer_wgpu::shader_surface::SurfaceShaderLanguage,
            Vec<neomacs_renderer_wgpu::shader_surface::SurfaceUniformInit>,
        )>,
    },
    /// Update one named uniform on the installed full-frame post shader
    /// (cheap; no recompile)
    FrameShaderSetUniform {
        request: FrameShaderRequestId,
        name: String,
        value: [f32; 4],
    },
    /// Open, control, or close a stable video session.
    Video(VideoSessionCommand),
}

/// Terminal commands.
#[cfg(feature = "neo-term")]
#[derive(Debug)]
pub enum TerminalCommand {
    /// Create a terminal
    TerminalCreate {
        id: crate::terminal::TerminalId,
        size: crate::terminal::TerminalGridSize,
        target: crate::terminal::TerminalDisplayTarget,
        shell: Option<String>,
    },
    /// Write input to a terminal
    TerminalWrite {
        id: crate::terminal::TerminalId,
        data: Vec<u8>,
    },
    /// Resize a terminal
    TerminalResize {
        id: crate::terminal::TerminalId,
        size: crate::terminal::TerminalGridSize,
    },
    /// Destroy a terminal
    TerminalDestroy { id: crate::terminal::TerminalId },
    /// Set floating terminal position and opacity
    TerminalSetFloat {
        id: crate::terminal::TerminalId,
        placement: crate::terminal::TerminalFloatPlacement,
    },
}

/// UI overlay commands.
#[derive(Debug)]
pub enum UiCommand {
    /// Show a popup menu anchored in the owning frame's logical-pixel space.
    ShowPopupMenu {
        /// Emacs frame_id of the owning top-level frame
        frame: FrameRef,
        placement: neomacs_display_protocol::PopupPlacement,
        items: Vec<PopupMenuItem>,
        title: Option<String>,
        /// Menu face colors (sRGB 0.0-1.0). None = use defaults.
        fg: Option<(f32, f32, f32)>,
        bg: Option<(f32, f32, f32)>,
    },
    /// Hide the active popup menu
    HidePopupMenu,
    /// Show a tooltip at position (x, y)
    ShowTooltip {
        /// Emacs frame_id of the owning top-level frame
        frame: FrameRef,
        x: f32,
        y: f32,
        text: String,
        fg_r: f32,
        fg_g: f32,
        fg_b: f32,
        bg_r: f32,
        bg_g: f32,
        bg_b: f32,
    },
    /// Hide the active tooltip
    HideTooltip,
    /// Trigger visual bell flash
    VisualBell {
        /// Emacs frame_id of the flashing top-level frame
        frame: FrameRef,
    },
}

/// Config and styling commands.
#[derive(Debug)]
// This public command enum preserves its established by-value payload API.
// Boxing `VisualConfig` only to narrow the enum would break downstream
// constructors and destructuring code.
#[allow(clippy::large_enum_variant)]
pub enum ConfigCommand {
    /// Enable or disable font ligatures
    SetLigaturesEnabled { enabled: bool },
    /// Replace the complete, already validated visual configuration snapshot.
    SetVisualConfig(VisualConfig),
    /// Toggle scroll indicators and focus ring
    SetScrollIndicators { enabled: bool },
    /// Set custom title bar height (0 = hidden, >0 = show with given height)
    SetTitlebarHeight { height: f32 },
    /// Toggle FPS counter overlay
    SetShowFps { enabled: bool },
    /// Set window corner radius for borderless mode (0 = no rounding)
    SetCornerRadius { radius: f32 },
    /// Set extra spacing (line spacing in pixels, letter spacing in pixels)
    SetExtraSpacing {
        line_spacing: f32,
        letter_spacing: f32,
    },
    /// Configure child frame visual style (drop shadow, rounded corners)
    SetChildFrameStyle {
        corner_radius: f32,
        shadow_enabled: bool,
        shadow_layers: u32,
        shadow_offset: f32,
        shadow_opacity: f32,
    },
}

/// Clipboard requests routed through the display owner to its serialized worker.
///
/// The evaluator may await `reply`, but the Winit event loop only forwards the
/// request and never performs native clipboard I/O itself.
#[derive(Debug)]
pub enum ClipboardCommand {
    SetText {
        selection: ClipboardSelection,
        text: Option<String>,
        expires_at: Instant,
        reply: Sender<Result<(), String>>,
    },
    GetText {
        selection: ClipboardSelection,
        expires_at: Instant,
        reply: Sender<Result<Option<String>, String>>,
    },
    GetOwnership {
        selection: ClipboardSelection,
        expires_at: Instant,
        reply: Sender<Result<SelectionOwner, String>>,
    },
}

impl ClipboardCommand {
    pub(crate) fn is_expired(&self) -> bool {
        let expires_at = match self {
            Self::SetText { expires_at, .. }
            | Self::GetText { expires_at, .. }
            | Self::GetOwnership { expires_at, .. } => expires_at,
        };
        Instant::now() >= *expires_at
    }
}

/// Command from Emacs to render thread
#[derive(Debug)]
// This public transport enum preserves its established by-value command API.
// Boxing `Config` would move the same downstream break one level outward.
#[allow(clippy::large_enum_variant)]
pub enum RenderCommand {
    Lifecycle(LifecycleCommand),
    Window(WindowCommand),
    Asset(AssetCommand),
    #[cfg(feature = "neo-term")]
    Terminal(TerminalCommand),
    Ui(UiCommand),
    Config(ConfigCommand),
    Clipboard(ClipboardCommand),
}

/// Channel capacities
// Frame channel: unbounded so try_send never drops frames.
// The render thread drains all queued frames and keeps only the latest
// (see poll_frame()), so memory stays bounded in practice.
//
// GNU Emacs' `kbd_buffer` holds 4096 input events and `tty_read_avail_input`
// stops reading terminal bytes when the buffer is under pressure rather than
// silently dropping command input.  Keep Neomacs' render-to-evaluator input
// queue at the same scale and use backpressure for durable user input below.
const INPUT_CHANNEL_CAPACITY: usize = 4096;
const COMMAND_CHANNEL_CAPACITY: usize = 64;

/// Effective full-frame shader availability published by the render thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameShaderAvailability {
    /// No adapter has been selected. Callers may queue a request; bootstrap
    /// will negotiate the final policy before processing normal commands.
    Pending = 0,
    Available = 1,
    SuppressedByQualityPolicy = 2,
}

/// Identity of one requested full-frame shader state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameShaderRequestId(u64);

/// Renderer-acknowledged state for one frame-shader request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameShaderExecution {
    Absent,
    Pending,
    Installed,
    Rejected,
    SuppressedByQualityPolicy,
}

#[derive(Debug, Clone, Copy)]
struct FrameShaderRuntimeState {
    next_request: u64,
    current_request: FrameShaderRequestId,
    requested: bool,
    execution: FrameShaderExecution,
}

impl Default for FrameShaderRuntimeState {
    fn default() -> Self {
        Self {
            next_request: 1,
            current_request: FrameShaderRequestId(0),
            requested: false,
            execution: FrameShaderExecution::Absent,
        }
    }
}

/// Display capability and renderer-acknowledged shader state shared by both
/// thread handles.
///
/// The availability field remains lock-free for the synchronous Lisp policy
/// check. Request generations and acknowledgements use one small mutex so a
/// late renderer reply cannot describe a newer request.
#[derive(Debug)]
pub struct SharedRenderCapabilities {
    frame_shader: AtomicU8,
    frame_shader_runtime: Mutex<FrameShaderRuntimeState>,
}

impl Default for SharedRenderCapabilities {
    fn default() -> Self {
        Self::new(FrameShaderAvailability::Pending)
    }
}

/// Prepared evaluator-side request. Dropping it before [`commit`](Self::commit)
/// restores the previous logical state, so a disconnected command channel
/// cannot publish an unqueued request.
pub struct PreparedFrameShaderRequest<'a> {
    capabilities: &'a SharedRenderCapabilities,
    request: FrameShaderRequestId,
    previous: FrameShaderRuntimeState,
    committed: bool,
}

impl PreparedFrameShaderRequest<'_> {
    pub fn id(&self) -> FrameShaderRequestId {
        self.request
    }

    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for PreparedFrameShaderRequest<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut state = self.capabilities.frame_shader_runtime_state();
        if state.current_request == self.request {
            let next_request = state.next_request;
            *state = self.previous;
            state.next_request = next_request;
        }
    }
}

impl SharedRenderCapabilities {
    /// Construct a fixed initial snapshot. Embedders normally use
    /// [`Default`] (pending); an explicit value is useful when constructing a
    /// host around an already-negotiated renderer.
    pub fn new(frame_shader: FrameShaderAvailability) -> Self {
        Self {
            frame_shader: AtomicU8::new(frame_shader as u8),
            frame_shader_runtime: Mutex::new(FrameShaderRuntimeState::default()),
        }
    }

    fn frame_shader_runtime_state(&self) -> std::sync::MutexGuard<'_, FrameShaderRuntimeState> {
        match self.frame_shader_runtime.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Reserve and publish a pending request before it is queued. The
    /// returned transaction rolls back unless the caller commits it.
    pub fn prepare_frame_shader_request(&self, requested: bool) -> PreparedFrameShaderRequest<'_> {
        let mut state = self.frame_shader_runtime_state();
        let previous = *state;
        let request = FrameShaderRequestId(state.next_request);
        state.next_request = state
            .next_request
            .checked_add(1)
            .expect("frame shader request id exhausted");
        state.current_request = request;
        state.requested = requested;
        state.execution = if requested {
            match self.frame_shader_availability() {
                FrameShaderAvailability::SuppressedByQualityPolicy => {
                    FrameShaderExecution::SuppressedByQualityPolicy
                }
                FrameShaderAvailability::Pending | FrameShaderAvailability::Available => {
                    FrameShaderExecution::Pending
                }
            }
        } else {
            FrameShaderExecution::Pending
        };
        drop(state);
        PreparedFrameShaderRequest {
            capabilities: self,
            request,
            previous,
            committed: false,
        }
    }

    /// Observe effective state only if REQUEST still names the latest request.
    pub fn frame_shader_execution(&self, request: FrameShaderRequestId) -> FrameShaderExecution {
        let state = self.frame_shader_runtime_state();
        if state.current_request == request {
            state.execution
        } else {
            FrameShaderExecution::Rejected
        }
    }

    pub(crate) fn acknowledge_frame_shader(
        &self,
        request: FrameShaderRequestId,
        execution: FrameShaderExecution,
    ) -> bool {
        let mut state = self.frame_shader_runtime_state();
        if state.current_request == request {
            state.execution = execution;
            true
        } else {
            false
        }
    }

    pub fn frame_shader_availability(&self) -> FrameShaderAvailability {
        match self.frame_shader.load(Ordering::Acquire) {
            1 => FrameShaderAvailability::Available,
            2 => FrameShaderAvailability::SuppressedByQualityPolicy,
            _ => FrameShaderAvailability::Pending,
        }
    }

    pub(crate) fn publish_frame_shader_availability(&self, availability: FrameShaderAvailability) {
        self.frame_shader
            .store(availability as u8, Ordering::Release);
        let mut state = self.frame_shader_runtime_state();
        if state.requested {
            state.execution = match availability {
                FrameShaderAvailability::SuppressedByQualityPolicy => {
                    FrameShaderExecution::SuppressedByQualityPolicy
                }
                FrameShaderAvailability::Pending | FrameShaderAvailability::Available => {
                    if state.execution == FrameShaderExecution::SuppressedByQualityPolicy {
                        FrameShaderExecution::Pending
                    } else {
                        state.execution
                    }
                }
            };
        }
    }

    /// Invalidate renderer-owned acknowledgement before device teardown while
    /// preserving the evaluator's declarative request for recovery replay.
    pub(crate) fn begin_renderer_reset(&self) {
        let mut state = self.frame_shader_runtime_state();
        state.execution = if state.requested {
            FrameShaderExecution::Pending
        } else {
            FrameShaderExecution::Absent
        };
    }
}

/// Communication channels between threads
pub struct ThreadComms {
    /// Frame display state: Emacs → Render
    pub frame_tx: Sender<SealedFramePresentation>,
    pub frame_rx: Receiver<SealedFramePresentation>,

    /// Commands: Emacs → Render
    pub cmd_tx: Sender<RenderCommand>,
    pub cmd_rx: Receiver<RenderCommand>,

    /// Input events: Render → Emacs
    pub input_tx: Sender<InputEvent>,
    pub input_rx: Receiver<InputEvent>,

    pub capabilities: Arc<SharedRenderCapabilities>,
}

impl ThreadComms {
    /// Create new thread communication channels
    pub fn new() -> Self {
        let (frame_tx, frame_rx) = unbounded();
        let (cmd_tx, cmd_rx) = bounded(COMMAND_CHANNEL_CAPACITY);
        let (input_tx, input_rx) = bounded(INPUT_CHANNEL_CAPACITY);
        let capabilities = Arc::new(SharedRenderCapabilities::default());
        Self {
            frame_tx,
            frame_rx,
            cmd_tx,
            cmd_rx,
            input_tx,
            input_rx,
            capabilities,
        }
    }

    /// Split into Emacs-side and Render-side handles
    pub fn split(self) -> (EmacsComms, RenderComms) {
        let emacs = EmacsComms {
            frame_tx: self.frame_tx,
            cmd_tx: self.cmd_tx,
            input_rx: self.input_rx,
            capabilities: Arc::clone(&self.capabilities),
        };

        let render = RenderComms {
            frame_rx: self.frame_rx,
            cmd_rx: self.cmd_rx,
            input_tx: self.input_tx,
            capabilities: self.capabilities,
        };

        (emacs, render)
    }
}

impl Default for ThreadComms {
    fn default() -> Self {
        Self::new()
    }
}

/// Emacs thread communication handle
pub struct EmacsComms {
    pub frame_tx: Sender<SealedFramePresentation>,
    pub cmd_tx: Sender<RenderCommand>,
    pub input_rx: Receiver<InputEvent>,
    pub capabilities: Arc<SharedRenderCapabilities>,
}

/// Render thread communication handle
pub struct RenderComms {
    pub frame_rx: Receiver<SealedFramePresentation>,
    pub cmd_rx: Receiver<RenderCommand>,
    pub input_tx: Sender<InputEvent>,
    pub capabilities: Arc<SharedRenderCapabilities>,
}

impl RenderComms {
    fn is_lossy_input_event(event: &InputEvent) -> bool {
        matches!(
            event,
            InputEvent::PositionedPointer(PositionedPointerInput {
                action: PointerAction::Move { .. },
                ..
            }) | InputEvent::MenuSelection { index: -1 }
        ) || matches!(
            event,
            InputEvent::WebView(neomacs_webview::WebViewEvent::LoadProgressChanged { .. })
        )
    }

    fn should_log_delivery(event: &InputEvent) -> bool {
        matches!(
            event,
            InputEvent::WindowResize { .. }
                | InputEvent::WindowClose { .. }
                | InputEvent::WindowFocus { .. }
                | InputEvent::MonitorsChanged { .. }
                | InputEvent::DisplayReset
        )
    }

    fn event_name(event: &InputEvent) -> &'static str {
        match event {
            InputEvent::RawTtyBytes { .. } => "raw-tty-bytes",
            InputEvent::Key { .. } => "key",
            InputEvent::PositionedPointer(PositionedPointerInput { action, .. }) => match action {
                PointerAction::Button { .. } => "positioned-pointer-button",
                PointerAction::Move { .. } => "positioned-pointer-move",
                PointerAction::Scroll { .. } => "positioned-pointer-scroll",
            },
            InputEvent::WindowResize { .. } => "window-resize",
            InputEvent::WindowClose { .. } => "window-close",
            InputEvent::WindowFocus { .. } => "window-focus",
            InputEvent::MonitorsChanged { .. } => "monitors-changed",
            InputEvent::DisplayReset => "display-reset",
            InputEvent::WebView(event) => match event {
                neomacs_webview::WebViewEvent::Ready { .. } => "webview-ready",
                neomacs_webview::WebViewEvent::Failed { .. } => "webview-failed",
                neomacs_webview::WebViewEvent::Closed { .. } => "webview-closed",
                neomacs_webview::WebViewEvent::TitleChanged { .. } => "webview-title-changed",
                neomacs_webview::WebViewEvent::UriChanged { .. } => "webview-uri-changed",
                neomacs_webview::WebViewEvent::LoadProgressChanged { .. } => {
                    "webview-load-progress-changed"
                }
                neomacs_webview::WebViewEvent::LoadChanged { .. } => "webview-load-changed",
                neomacs_webview::WebViewEvent::LoadFinished { .. } => "webview-load-finished",
                neomacs_webview::WebViewEvent::ScriptFinished { .. } => "webview-script-finished",
                neomacs_webview::WebViewEvent::ProcessFailed { .. } => "webview-process-failed",
                neomacs_webview::WebViewEvent::FocusChanged { .. } => "webview-focus-changed",
            },
            InputEvent::ImageStateChanged { .. } => "image-state-changed",
            InputEvent::SurfaceCreateFailed { .. } => "surface-create-failed",
            InputEvent::FrameShaderFailed { .. } => "frame-shader-failed",
            InputEvent::MenuSelection { .. } => "menu-selection",
            InputEvent::FileDrop { .. } => "file-drop",
            InputEvent::ToolBarClick { .. } => "toolbar-click",
            InputEvent::PresentedPointer { .. } => "presented-pointer",
            InputEvent::PresentationActivated { .. } => "presentation-activated",
            InputEvent::PresentationDiscarded { .. } => "presentation-discarded",
            InputEvent::PresentationRetired { .. } => "presentation-retired",
            InputEvent::MenuBarClick { .. } => "menubar-click",
            #[cfg(feature = "neo-term")]
            InputEvent::TerminalCreateFailed { .. } => "terminal-create-failed",
            #[cfg(feature = "neo-term")]
            InputEvent::TerminalExited { .. } => "terminal-exited",
            #[cfg(feature = "neo-term")]
            InputEvent::TerminalTitleChanged { .. } => "terminal-title-changed",
        }
    }

    /// Queue a display input event for the input bridge.
    ///
    /// After converting the display event, the bridge owns notifying the
    /// evaluator's wait backend.
    pub fn send_input(&self, event: InputEvent) {
        let log_delivery = Self::should_log_delivery(&event);
        let event_name = Self::event_name(&event);
        if Self::is_lossy_input_event(&event) {
            match self.input_tx.try_send(event) {
                Ok(()) => {
                    if log_delivery {
                        tracing::debug!("send_input: queued {}", event_name);
                    }
                }
                Err(TrySendError::Full(event)) => {
                    tracing::debug!(
                        "send_input: dropped lossy {} because the input queue is full",
                        Self::event_name(&event)
                    );
                }
                Err(TrySendError::Disconnected(event)) => {
                    tracing::warn!(
                        "send_input: dropped {} because the input queue is disconnected",
                        Self::event_name(&event)
                    );
                }
            }
            return;
        }

        match self.input_tx.send(event) {
            Ok(()) => {
                if log_delivery {
                    tracing::debug!("send_input: queued {}", event_name);
                }
            }
            Err(err) => {
                tracing::warn!(
                    "send_input: dropped {} because the input queue is disconnected",
                    Self::event_name(&err.0)
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "thread_comm_test.rs"]
mod tests;
