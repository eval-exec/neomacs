use std::fs::File;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::Duration;
use std::{collections::BTreeMap, fmt};

use neomacs_display_protocol::WebViewId;

macro_rules! webview_id_type {
    ($name:ident, $raw:ty) => {
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($raw);

        impl $name {
            #[must_use]
            pub const fn new(raw: $raw) -> Self {
                Self(raw)
            }

            #[must_use]
            pub const fn get(self) -> $raw {
                self.0
            }
        }
    };
}

webview_id_type!(HostWindowId, u64);
webview_id_type!(WebProfileId, u64);
webview_id_type!(WebViewGeneration, u64);
webview_id_type!(WebViewOccurrenceId, u64);
webview_id_type!(WebViewSceneRevision, u64);
webview_id_type!(ScriptRequestId, u64);
webview_id_type!(NavigationId, u64);
webview_id_type!(PolicyDecisionId, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebContentSize {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl WebContentSize {
    pub fn new(width: u32, height: u32) -> Result<Self, WebContentSizeError> {
        Ok(Self {
            width: NonZeroU32::new(width).ok_or(WebContentSizeError::ZeroWidth)?,
            height: NonZeroU32::new(height).ok_or(WebContentSizeError::ZeroHeight)?,
        })
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebContentSizeError {
    #[error("webview width must be non-zero")]
    ZeroWidth,
    #[error("webview height must be non-zero")]
    ZeroHeight,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoragePartition {
    Persistent(WebProfileId),
    Ephemeral(WebProfileId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowsingRelationship {
    Independent,
    Related(WebViewId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationTarget {
    Uri(String),
    Html {
        contents: String,
        base_uri: Option<String>,
    },
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryAction {
    Back,
    Forward,
    Reload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptWorld {
    Page,
    Isolated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRequest {
    pub request: ScriptRequestId,
    pub view: WebViewId,
    pub source: String,
    pub world: ScriptWorld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusIntent {
    Focus,
    Blur,
}

bitflags::bitflags! {
    /// Backend-neutral keyboard modifier state.
    ///
    /// These values deliberately do not reuse Emacs, winit, WPE, AppKit, or
    /// Win32 bit assignments. Each frontend/backend boundary must translate
    /// its native representation explicitly.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct WebViewModifiers: u8 {
        const SHIFT = 1 << 0;
        const CONTROL = 1 << 1;
        const ALT = 1 << 2;
        const META = 1 << 3;
        const SUPER = 1 << 4;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WebContentPoint {
    x: f32,
    y: f32,
}

impl WebContentPoint {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Middle,
    Secondary,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WebViewScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

/// One backend-neutral input operation whose coordinates are relative to the
/// WebView's full content rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WebViewInput {
    Keyboard {
        key_value: u32,
        hardware_key_code: u32,
        state: ButtonState,
        modifiers: WebViewModifiers,
    },
    PointerMove {
        position: WebContentPoint,
        modifiers: WebViewModifiers,
    },
    PointerButton {
        position: WebContentPoint,
        button: PointerButton,
        state: ButtonState,
        modifiers: WebViewModifiers,
    },
    Scroll {
        position: WebContentPoint,
        delta: WebViewScrollDelta,
        modifiers: WebViewModifiers,
    },
}

/// Identity of one WebView occurrence in one committed host scene.
///
/// Callers obtain this value from `WebViewSystem::presented_target`; private
/// fields prevent synthesizing an unqualified view ID and accidentally
/// delivering input to a replacement presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WebViewInputTarget {
    host: HostWindowId,
    revision: WebViewSceneRevision,
    view: WebViewId,
    occurrence: WebViewOccurrenceId,
}

impl WebViewInputTarget {
    pub(crate) const fn new(
        host: HostWindowId,
        revision: WebViewSceneRevision,
        view: WebViewId,
        occurrence: WebViewOccurrenceId,
    ) -> Self {
        Self {
            host,
            revision,
            view,
            occurrence,
        }
    }

    #[must_use]
    pub const fn host(self) -> HostWindowId {
        self.host
    }

    #[must_use]
    pub const fn revision(self) -> WebViewSceneRevision {
        self.revision
    }

    #[must_use]
    pub const fn view(self) -> WebViewId {
        self.view
    }

    #[must_use]
    pub const fn occurrence(self) -> WebViewOccurrenceId {
        self.occurrence
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JavaScriptPolicy {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeveloperToolsPolicy {
    Enabled,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebViewPolicy {
    javascript: JavaScriptPolicy,
    developer_tools: DeveloperToolsPolicy,
}

impl Default for WebViewPolicy {
    fn default() -> Self {
        Self {
            javascript: JavaScriptPolicy::Enabled,
            developer_tools: DeveloperToolsPolicy::Disabled,
        }
    }
}

impl WebViewPolicy {
    #[must_use]
    pub const fn new(javascript: JavaScriptPolicy, developer_tools: DeveloperToolsPolicy) -> Self {
        Self {
            javascript,
            developer_tools,
        }
    }

    #[must_use]
    pub const fn javascript_policy(&self) -> JavaScriptPolicy {
        self.javascript
    }

    #[must_use]
    pub const fn developer_tools_policy(&self) -> DeveloperToolsPolicy {
        self.developer_tools
    }

    #[must_use]
    pub const fn javascript(&self) -> bool {
        matches!(self.javascript, JavaScriptPolicy::Enabled)
    }

    #[must_use]
    pub const fn developer_tools(&self) -> bool {
        matches!(self.developer_tools, DeveloperToolsPolicy::Enabled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebViewCreate {
    pub id: WebViewId,
    pub storage: StoragePartition,
    pub relationship: BrowsingRelationship,
    pub initial_size: WebContentSize,
    pub policy: WebViewPolicy,
    pub initial_navigation: Option<NavigationTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebViewCommand {
    Create(WebViewCreate),
    Close {
        id: WebViewId,
    },
    SetModelSize {
        id: WebViewId,
        size: WebContentSize,
    },
    Navigate {
        id: WebViewId,
        target: NavigationTarget,
    },
    History {
        id: WebViewId,
        action: HistoryAction,
    },
    EvaluateScript(ScriptRequest),
    Focus {
        id: WebViewId,
        intent: FocusIntent,
    },
}

impl WebViewCommand {
    #[must_use]
    pub const fn id(&self) -> WebViewId {
        match self {
            Self::Create(create) => create.id,
            Self::Close { id }
            | Self::SetModelSize { id, .. }
            | Self::Navigate { id, .. }
            | Self::History { id, .. }
            | Self::Focus { id, .. } => *id,
            Self::EvaluateScript(request) => request.view,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebViewState {
    Waiting,
    Creating,
    Ready,
    Failed,
    Closing,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WebViewEvent {
    Ready {
        id: WebViewId,
        generation: WebViewGeneration,
    },
    Failed {
        id: WebViewId,
        generation: WebViewGeneration,
        error: String,
    },
    Closed {
        id: WebViewId,
        generation: WebViewGeneration,
    },
    TitleChanged {
        id: WebViewId,
        generation: WebViewGeneration,
        title: String,
    },
    UriChanged {
        id: WebViewId,
        generation: WebViewGeneration,
        uri: String,
    },
    LoadProgressChanged {
        id: WebViewId,
        generation: WebViewGeneration,
        progress: f64,
    },
    LoadFinished {
        id: WebViewId,
        generation: WebViewGeneration,
        navigation: Option<NavigationId>,
    },
    ScriptFinished {
        view: WebViewId,
        generation: WebViewGeneration,
        request: ScriptRequestId,
        result: Result<WebValue, ScriptError>,
    },
    ProcessFailed {
        id: WebViewId,
        generation: WebViewGeneration,
        failure: WebProcessFailure,
    },
    FocusChanged {
        id: WebViewId,
        generation: WebViewGeneration,
        focused: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum WebValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<WebValue>),
    Object(BTreeMap<String, WebValue>),
}

impl WebValue {
    pub(crate) fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Number(value) => Self::Number(value.as_f64().unwrap_or(f64::NAN)),
            serde_json::Value::String(value) => Self::String(value),
            serde_json::Value::Array(values) => {
                Self::Array(values.into_iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, Self::from_json(value)))
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptError {
    Rejected(String),
    Cancelled,
    ProcessFailed,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(message) => formatter.write_str(message),
            Self::Cancelled => formatter.write_str("script evaluation was cancelled"),
            Self::ProcessFailed => {
                formatter.write_str("web process failed during script evaluation")
            }
        }
    }
}

/// Backend-neutral reason that a native web content process stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebProcessFailure {
    Crashed,
    ExceededMemoryLimit,
    Terminated,
    Unresponsive,
    LaunchFailed,
    Other(i32),
}

impl fmt::Display for WebProcessFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crashed => formatter.write_str("web content process crashed"),
            Self::ExceededMemoryLimit => {
                formatter.write_str("web content process exceeded its memory limit")
            }
            Self::Terminated => formatter.write_str("web content process was terminated"),
            Self::Unresponsive => formatter.write_str("web content process became unresponsive"),
            Self::LaunchFailed => formatter.write_str("web content process failed to launch"),
            Self::Other(code) => write!(formatter, "web content process failed ({code})"),
        }
    }
}

/// One owned plane of a Linux DMA-BUF browser frame.
///
/// `File` gives the descriptor single-owner RAII semantics without exposing a
/// raw fd that callers could accidentally leak or close twice.
#[derive(Debug)]
pub struct DmaBufPlane {
    file: File,
    stride: u32,
    offset: u32,
}

impl DmaBufPlane {
    #[cfg(any(test, target_os = "linux"))]
    pub(crate) const fn new(file: File, stride: u32, offset: u32) -> Self {
        Self {
            file,
            stride,
            offset,
        }
    }

    #[must_use]
    pub const fn stride(&self) -> u32 {
        self.stride
    }

    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }

    #[must_use]
    pub const fn file(&self) -> &File {
        &self.file
    }
}

/// Opaque ownership token for the native browser buffer behind a DMA-BUF.
///
/// The concrete lease is intentionally private to the platform backend. Its
/// destructor keeps the WPE `GObject` reference paired with the exported
/// descriptors, while this type prevents callers from constructing an
/// unleased browser frame.
pub(crate) trait DmaBufLease: Send + 'static {}

pub(crate) struct DmaBufFrameLease {
    _lease: Box<dyn DmaBufLease>,
}

impl DmaBufFrameLease {
    #[cfg(any(test, target_os = "linux"))]
    pub(crate) fn new(lease: impl DmaBufLease) -> Self {
        Self {
            _lease: Box::new(lease),
        }
    }
}

impl fmt::Debug for DmaBufFrameLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DmaBufFrameLease(..)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaBufReadiness {
    Ready,
    TimedOut,
}

#[derive(Debug)]
pub struct DmaBufFrame {
    planes: Vec<DmaBufPlane>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    rendering_fence: Option<File>,
    fourcc: u32,
    modifier: u64,
    width: u32,
    height: u32,
    _lease: DmaBufFrameLease,
}

impl DmaBufFrame {
    #[cfg(any(test, target_os = "linux"))]
    pub(crate) fn new(
        planes: Vec<DmaBufPlane>,
        rendering_fence: Option<File>,
        fourcc: u32,
        modifier: u64,
        width: u32,
        height: u32,
        lease: DmaBufFrameLease,
    ) -> Self {
        Self {
            planes,
            rendering_fence,
            fourcc,
            modifier,
            width,
            height,
            _lease: lease,
        }
    }

    #[must_use]
    pub fn planes(&self) -> &[DmaBufPlane] {
        &self.planes
    }

    #[must_use]
    pub const fn fourcc(&self) -> u32 {
        self.fourcc
    }

    #[must_use]
    pub const fn modifier(&self) -> u64 {
        self.modifier
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Wait for WPE's producer fence before the GPU imports this frame.
    ///
    /// A missing fence means the producer declared the frame immediately
    /// readable. `TimedOut` is distinct from an I/O error so import policy can
    /// fall back to the already-captured pixel frame without guessing.
    #[cfg(target_os = "linux")]
    pub fn wait_until_ready(&self, timeout: Duration) -> std::io::Result<DmaBufReadiness> {
        use std::os::fd::AsRawFd;

        let Some(fence) = &self.rendering_fence else {
            return Ok(DmaBufReadiness::Ready);
        };
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd: fence.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            // SAFETY: `descriptor` points to one live `pollfd`, and `fence`
            // owns its descriptor for the entire call.
            let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
            if result > 0 {
                return Ok(DmaBufReadiness::Ready);
            }
            if result == 0 {
                return Ok(DmaBufReadiness::TimedOut);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelFrame {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl PixelFrame {
    #[cfg(any(test, target_os = "linux"))]
    pub(crate) fn new(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
        }
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
}

/// A frame produced by a composited backend. Native-overlay backends never
/// produce this value.
#[derive(Debug)]
pub enum WebViewFrame {
    DmaBuf(DmaBufFrame),
    Pixels(PixelFrame),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebViewCommandError {
    #[error("webview {0} already exists")]
    AlreadyExists(WebViewId),
    #[error("unknown webview {0}")]
    UnknownView(WebViewId),
    #[error("webview {0} is not ready")]
    NotReady(WebViewId),
    #[error("webview {0} is not present in an active host scene")]
    NotPresented(WebViewId),
    #[error("webview {view} input targets stale scene {received:?}; current scene is {current:?}")]
    StaleInputScene {
        view: WebViewId,
        current: WebViewSceneRevision,
        received: WebViewSceneRevision,
    },
    #[error("webview {view} occurrence {occurrence:?} is no longer presented")]
    StaleInputOccurrence {
        view: WebViewId,
        occurrence: WebViewOccurrenceId,
    },
    #[error("webview {view} refers to missing related webview {related}")]
    MissingRelatedView { view: WebViewId, related: WebViewId },
    #[error("webview {view} and related webview {related} use incompatible storage partitions")]
    IncompatibleRelatedStorage { view: WebViewId, related: WebViewId },
    #[error("webview {id} backend command failed: {error}")]
    Backend { id: WebViewId, error: String },
}

/// Requested transport for frames produced by a native WebView.
///
/// `Auto` lets the platform resolve one concrete transport when the WebView
/// system starts. A running view never produces both representations of the
/// same native frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WebViewFrameTransport {
    #[default]
    Auto,
    SoftwarePixels,
    DmaBuf,
}

fn frame_transport_from_source(value: Option<&str>) -> WebViewFrameTransport {
    match value {
        Some("pixels") | Some("software") | Some("software-pixels") | Some("pixels-first") => {
            WebViewFrameTransport::SoftwarePixels
        }
        Some("dmabuf") | Some("dma-buf") | Some("dmabuf-first") | Some("dma-buf-first") => {
            WebViewFrameTransport::DmaBuf
        }
        Some("auto") | None => WebViewFrameTransport::Auto,
        Some(value) => {
            tracing::warn!(
                value,
                "unrecognized NEOMACS_WEBVIEW_FRAME_TRANSPORT; using automatic selection"
            );
            WebViewFrameTransport::Auto
        }
    }
}

fn configured_frame_transport() -> WebViewFrameTransport {
    let value = std::env::var("NEOMACS_WEBVIEW_FRAME_TRANSPORT")
        .ok()
        // Preserve the pre-neomacs-webview spelling as a compatibility alias.
        .or_else(|| std::env::var("NEOMACS_WEBVIEW_IMPORT").ok())
        .or_else(|| std::env::var("NEOMACS_WEBKIT_IMPORT").ok());
    frame_transport_from_source(value.as_deref())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebViewSystemConfig {
    pub profile_root: Option<PathBuf>,
    pub frame_transport: WebViewFrameTransport,
}

impl Default for WebViewSystemConfig {
    fn default() -> Self {
        Self {
            profile_root: current_user_profile_root(),
            frame_transport: configured_frame_transport(),
        }
    }
}

fn profile_root_from_sources(
    explicit: Option<PathBuf>,
    platform_data_home: Option<PathBuf>,
    home: Option<PathBuf>,
    home_relative: &std::path::Path,
) -> Option<PathBuf> {
    explicit
        .filter(|path| path.is_absolute())
        .or_else(|| {
            platform_data_home
                .filter(|path| path.is_absolute())
                .map(|path| path.join("neomacs").join("webview"))
        })
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|path| path.join(home_relative))
        })
}

fn current_user_profile_root() -> Option<PathBuf> {
    let explicit = std::env::var_os("NEOMACS_WEBVIEW_PROFILE_ROOT").map(PathBuf::from);

    #[cfg(target_os = "windows")]
    let sources = (
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        std::path::Path::new("AppData/Local/neomacs/webview"),
    );
    #[cfg(target_os = "macos")]
    let sources = (
        None,
        std::env::var_os("HOME").map(PathBuf::from),
        std::path::Path::new("Library/Application Support/Neomacs/WebView"),
    );
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let sources = (
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
        std::path::Path::new(".local/share/neomacs/webview"),
    );

    profile_root_from_sources(explicit, sources.0, sources.1, sources.2)
}

#[cfg(test)]
mod config_tests {
    use std::path::{Path, PathBuf};

    use super::{WebViewFrameTransport, frame_transport_from_source, profile_root_from_sources};

    #[cfg(target_os = "linux")]
    struct TestDmaBufLease;

    #[cfg(target_os = "linux")]
    impl super::DmaBufLease for TestDmaBufLease {}

    #[test]
    fn explicit_profile_root_wins_over_platform_directories() {
        assert_eq!(
            profile_root_from_sources(
                Some(PathBuf::from("/explicit/webview")),
                Some(PathBuf::from("/data")),
                Some(PathBuf::from("/home/user")),
                Path::new(".local/share/neomacs/webview"),
            ),
            Some(PathBuf::from("/explicit/webview"))
        );
    }

    #[test]
    fn relative_environment_directories_cannot_make_profiles_cwd_dependent() {
        assert_eq!(
            profile_root_from_sources(
                None,
                Some(PathBuf::from("relative-data")),
                Some(PathBuf::from("/home/user")),
                Path::new(".local/share/neomacs/webview"),
            ),
            Some(PathBuf::from("/home/user/.local/share/neomacs/webview"))
        );
    }

    #[test]
    fn frame_transport_configuration_is_parsed_into_a_closed_enum() {
        assert_eq!(
            frame_transport_from_source(Some("dmabuf")),
            WebViewFrameTransport::DmaBuf
        );
        assert_eq!(
            frame_transport_from_source(Some("pixels")),
            WebViewFrameTransport::SoftwarePixels
        );
        assert_eq!(
            frame_transport_from_source(Some("auto")),
            WebViewFrameTransport::Auto
        );
        assert_eq!(
            frame_transport_from_source(Some("future-unknown-value")),
            WebViewFrameTransport::Auto
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dma_buf_readiness_is_gated_by_the_producer_fence() {
        use std::fs::File;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::time::Duration;

        let mut descriptors = [-1; 2];
        // SAFETY: `descriptors` has the two slots required by `pipe`.
        assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
        // SAFETY: `pipe` initialized both descriptors, each of which moves
        // into exactly one RAII owner.
        let read_end = unsafe { File::from_raw_fd(descriptors[0]) };
        let write_end = unsafe { OwnedFd::from_raw_fd(descriptors[1]) };
        let frame = super::DmaBufFrame::new(
            Vec::new(),
            Some(read_end),
            0,
            0,
            1,
            1,
            super::DmaBufFrameLease::new(TestDmaBufLease),
        );

        assert_eq!(
            frame.wait_until_ready(Duration::ZERO).unwrap(),
            super::DmaBufReadiness::TimedOut
        );
        let byte = [1u8];
        // SAFETY: the write descriptor is live and `byte` is a one-byte buffer.
        assert_eq!(
            unsafe { libc::write(write_end.as_raw_fd(), byte.as_ptr().cast(), byte.len()) },
            1
        );
        assert_eq!(
            frame.wait_until_ready(Duration::ZERO).unwrap(),
            super::DmaBufReadiness::Ready
        );
    }
}

#[derive(Clone)]
pub struct WebViewWake(Arc<dyn Fn() + Send + Sync>);

impl WebViewWake {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    #[must_use]
    pub fn noop() -> Self {
        Self::new(|| {})
    }

    pub(crate) fn notify(&self) {
        (self.0)();
    }
}

/// Owned capability for attaching native WebView presentation to one winit
/// window. Platform handles remain private inside the crate.
#[derive(Clone)]
pub struct WebViewHost {
    window: Arc<winit::window::Window>,
}

impl WebViewHost {
    #[must_use]
    pub fn new(window: Arc<winit::window::Window>) -> Self {
        Self { window }
    }

    pub(crate) fn window(&self) -> &Arc<winit::window::Window> {
        &self.window
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WebViewInitError {
    #[error("webview support was not built")]
    NotBuilt,
    #[error("webview initialization failed: {0}")]
    Backend(String),
}
