use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

use neomacs_display_protocol::frame_time::EventTime;

use winit::dpi::{LogicalSize, PhysicalSize, Size};

use crate::clipboard::ClipboardService;
use crate::core::face::Face;
pub use crate::thread_comm::MonitorInfo;
use crate::thread_comm::{FrameShaderAvailability, RenderComms};
pub(super) use neomacs_display_protocol::PointerAppearancePhase;
use neomacs_display_protocol::{
    EffectsConfig, FrameGlyphBuffer, FrameRect, ImageCacheUsage, ImageId, ImageLoadToken,
    InteractionId, PointerAppearanceId, PointerAppearanceSelection, PresentationId,
    PresentedResizeAxis, ToolBarImageSource, TransitionPolicy, VisualConfig,
};
use neomacs_renderer_wgpu::WgpuRenderer;
use neovm_core::emacs_core::image_catalog::ResolvedImageMetadata;

use super::cursor::CursorState;
use super::frame_windows::{
    FrameLifecycle, GuiFrameRenderState, GuiFrameWindowManager, GuiFrameWindowState,
};
use super::render_quality::{RenderBackendProfile, RenderQualityPolicy};

/// Decoded image facts shared from the render thread to the evaluator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageDecodeTerminal {
    Ready(ResolvedImageMetadata),
    Failed(String),
}

/// Result of a redisplay-safe, non-blocking terminal-state observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageTerminalProbe {
    Busy,
    Available(Option<ImageDecodeTerminal>),
}

/// Exclusive renderer publication capability for terminal image results.
///
/// The contained map is deliberately private. Holding this value models the
/// only contention redisplay's [`ImageTerminalProbe::Busy`] path must tolerate;
/// dropping it wakes bounded synchronous waiters once for the whole batch.
pub struct ImageTerminalPublication<'a> {
    terminals: MutexGuard<'a, HashMap<ImageLoadToken, ImageDecodeTerminal>>,
    terminal_changed: &'a Condvar,
}

impl ImageTerminalPublication<'_> {
    pub fn publish(&mut self, load: ImageLoadToken, terminal: ImageDecodeTerminal) {
        self.terminals.insert(load, terminal);
    }

    pub fn remove(&mut self, load: ImageLoadToken) {
        self.terminals.remove(&load);
    }

    pub fn clear_image(&mut self, image: ImageId) {
        self.terminals.retain(|load, _| load.image() != image);
    }
}

impl Drop for ImageTerminalPublication<'_> {
    fn drop(&mut self) {
        self.terminal_changed.notify_all();
    }
}

/// Cross-thread image facts published by the renderer and read by Elisp.
///
/// Terminal decode results need a condition variable for bounded synchronous
/// measurement. Cache usage is an independent lock-free snapshot: querying
/// `image-cache-size` never waits for a decode or the render thread.
pub struct ImageRenderState {
    terminals: Mutex<HashMap<ImageLoadToken, ImageDecodeTerminal>>,
    terminal_changed: Condvar,
    cache_bytes: AtomicU64,
}

impl Default for ImageRenderState {
    fn default() -> Self {
        Self {
            terminals: Mutex::new(HashMap::new()),
            terminal_changed: Condvar::new(),
            cache_bytes: AtomicU64::new(0),
        }
    }
}

impl ImageRenderState {
    fn lock_terminals(&self) -> MutexGuard<'_, HashMap<ImageLoadToken, ImageDecodeTerminal>> {
        self.terminals
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn terminal(&self, load: ImageLoadToken) -> Option<ImageDecodeTerminal> {
        self.lock_terminals().get(&load).cloned()
    }

    pub fn begin_terminal_publication(&self) -> ImageTerminalPublication<'_> {
        ImageTerminalPublication {
            terminals: self.lock_terminals(),
            terminal_changed: &self.terminal_changed,
        }
    }

    pub fn try_terminal(&self, load: ImageLoadToken) -> ImageTerminalProbe {
        match self.terminals.try_lock() {
            Ok(terminals) => ImageTerminalProbe::Available(terminals.get(&load).cloned()),
            Err(std::sync::TryLockError::WouldBlock) => ImageTerminalProbe::Busy,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                ImageTerminalProbe::Available(poisoned.into_inner().get(&load).cloned())
            }
        }
    }

    pub fn publish_terminal(&self, load: ImageLoadToken, terminal: ImageDecodeTerminal) {
        self.begin_terminal_publication().publish(load, terminal);
    }

    pub fn remove_terminal(&self, load: ImageLoadToken) {
        self.begin_terminal_publication().remove(load);
    }

    pub fn clear_image_terminals(&self, image: ImageId) {
        self.begin_terminal_publication().clear_image(image);
    }

    pub fn wait_for_terminal(
        &self,
        load: ImageLoadToken,
        timeout: std::time::Duration,
    ) -> Option<ImageDecodeTerminal> {
        let deadline = std::time::Instant::now() + timeout;
        let mut terminals = self.lock_terminals();
        loop {
            if let Some(terminal) = terminals.get(&load).cloned() {
                return Some(terminal);
            }
            let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
            match self.terminal_changed.wait_timeout(terminals, remaining) {
                Ok((guard, result)) => {
                    terminals = guard;
                    if result.timed_out() {
                        return terminals.get(&load).cloned();
                    }
                }
                Err(poisoned) => {
                    let (guard, _) = poisoned.into_inner();
                    terminals = guard;
                }
            }
        }
    }

    pub fn publish_cache_usage(&self, usage: ImageCacheUsage) {
        self.cache_bytes
            .store(usage.total_bytes(), Ordering::Release);
    }

    #[must_use]
    pub fn cached_size_bytes(&self) -> u64 {
        self.cache_bytes.load(Ordering::Acquire)
    }
}

pub type SharedImageRenderState = Arc<ImageRenderState>;

/// Shared storage for monitor info accessible from both threads.
/// The Condvar is notified once monitors have been populated.
pub type SharedMonitorInfo = Arc<(Mutex<Vec<MonitorInfo>>, std::sync::Condvar)>;

pub(super) fn backend_uses_winit_logical_pixels() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::display_scale::active_window_coordinate_system().map_or_else(
            || std::env::var_os("WAYLAND_DISPLAY").is_some(),
            |system| {
                matches!(
                    system,
                    crate::display_scale::WindowCoordinateSystem::WinitLogical
                )
            },
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

pub(super) fn effective_window_scale_factor(raw_scale_factor: f64) -> f64 {
    // On X11 fontconfig already handles DPI — the font metrics returned are
    // already scaled for the display.  Only Wayland needs us to apply the
    // compositor scale factor to rendering.
    if backend_uses_winit_logical_pixels() {
        raw_scale_factor
    } else {
        1.0
    }
}

pub(super) fn window_size_from_emacs_pixels(width: u32, height: u32) -> Size {
    if backend_uses_winit_logical_pixels() {
        Size::Logical(LogicalSize::new(width as f64, height as f64))
    } else {
        // X11: physical pixels as-is, matching GNU Emacs.  fontconfig DPI
        // already scales font sizes, so window dimensions are already at
        // the correct physical size.
        Size::Physical(PhysicalSize::new(width, height))
    }
}

pub(super) fn emacs_pixels_from_window_size(
    width: u32,
    height: u32,
    scale_factor: f64,
) -> (u32, u32) {
    if backend_uses_winit_logical_pixels() {
        (
            (width as f64 / scale_factor).round() as u32,
            (height as f64 / scale_factor).round() as u32,
        )
    } else {
        // X11: fontconfig handles DPI.  Return physical pixels as-is
        // so Emacs computes the correct character grid with the already-
        // scaled font metrics.
        (width, height)
    }
}

#[cfg(feature = "webview")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WebViewPointerCapture {
    pub(super) target: neomacs_webview::WebViewInputTarget,
    /// WebView content origin in the native window's logical coordinates.
    pub(super) surface_origin_x: f32,
    pub(super) surface_origin_y: f32,
}

/// FPS counter and frame time tracking state.
#[derive(Clone)]
pub(super) struct FpsCounter {
    pub(super) enabled: bool,
    pub(super) last_instant: Instant,
    pub(super) frame_count: u32,
    pub(super) display_value: f32,
    pub(super) frame_time_ms: f32,
    pub(super) render_start: Instant,
}

/// Typing-speed overlay state for one native GUI frame window.
#[derive(Default)]
pub(super) struct TypingSpeedState {
    /// Key press timestamps for WPM calculation.
    pub(super) key_press_times: Vec<EventTime>,
    /// Smoothed WPM value for display.
    pub(super) displayed_wpm: f32,
}

/// Idle dim overlay state for one native GUI frame window.
pub(super) struct IdleDimState {
    pub(super) last_activity_time: EventTime,
    pub(super) current_alpha: f32,
    pub(super) active: bool,
}

impl IdleDimState {
    /// Idle state for a window that has just been created.
    ///
    /// There is no `Default`: "no activity recorded" has no honest zero value.
    /// Seeding with `None` and treating it as "idle forever" would dim a
    /// freshly-opened window immediately, so creation counts as activity and
    /// the caller must say when it happened.
    pub(super) fn new(at: EventTime) -> Self {
        Self {
            last_activity_time: at,
            current_alpha: 0.0,
            active: false,
        }
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            enabled: false,
            last_instant: Instant::now(),
            frame_count: 0,
            display_value: 0.0,
            frame_time_ms: 0.0,
            render_start: Instant::now(),
        }
    }
}

/// One resolved native cursor policy, ordered from strongest to weakest by
/// [`Self::resolve`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PointerCursorIntent {
    Default,
    ChromeAction,
    PresentedWindowResize(PresentedResizeAxis),
    NativeFrameResize(winit::window::ResizeDirection),
}

impl PointerCursorIntent {
    #[must_use]
    pub(super) const fn resolve(
        native_resize: Option<winit::window::ResizeDirection>,
        presented_resize: Option<PresentedResizeAxis>,
        chrome_action: bool,
    ) -> Self {
        if let Some(direction) = native_resize {
            Self::NativeFrameResize(direction)
        } else if let Some(axis) = presented_resize {
            Self::PresentedWindowResize(axis)
        } else if chrome_action {
            Self::ChromeAction
        } else {
            Self::Default
        }
    }

    #[must_use]
    pub(super) fn icon(self) -> winit::window::CursorIcon {
        match self {
            Self::Default => winit::window::CursorIcon::Default,
            Self::ChromeAction => winit::window::CursorIcon::Pointer,
            Self::PresentedWindowResize(axis) => match axis {
                PresentedResizeAxis::Horizontal => winit::window::CursorIcon::EwResize,
                PresentedResizeAxis::Vertical => winit::window::CursorIcon::NsResize,
            },
            Self::NativeFrameResize(direction) => winit::window::CursorIcon::from(direction),
        }
    }
}

/// Borderless native-window chrome state (title bar, resize edges, decorations).
#[derive(Clone)]
pub(super) struct WindowChrome {
    pub(super) decorations_enabled: bool,
    pub(super) resize_edge: Option<winit::window::ResizeDirection>,
    pub(super) cursor_intent: PointerCursorIntent,
    pub(super) title: String,
    pub(super) titlebar_height: f32,
    pub(super) titlebar_hover: u32,
    /// When the titlebar was last clicked, for double-click detection.
    /// `None` until the first click: a window has not been clicked at creation,
    /// and pretending otherwise made the first click within the double-click
    /// interval of opening a window maximize it instead of dragging it.
    pub(super) last_titlebar_click: Option<EventTime>,
    pub(super) is_fullscreen: bool,
    pub(super) corner_radius: f32,
}

impl Default for WindowChrome {
    fn default() -> Self {
        Self {
            decorations_enabled: true,
            resize_edge: None,
            cursor_intent: PointerCursorIntent::Default,
            title: String::from("neomacs"),
            titlebar_height: 30.0,
            titlebar_hover: 0,
            last_titlebar_click: None,
            is_fullscreen: false,
            corner_radius: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ImeCursorArea {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PresentedAppearanceKey {
    frame_id: u64,
    presentation: PresentationId,
    appearance: PointerAppearanceId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PendingPointerDamage {
    key: PresentedAppearanceKey,
    rect: FrameRect,
}

impl PendingPointerDamage {
    pub(super) const fn new(key: PresentedAppearanceKey, rect: FrameRect) -> Self {
        Self { key, rect }
    }

    pub(super) const fn key(self) -> PresentedAppearanceKey {
        self.key
    }

    #[cfg(test)]
    pub(super) const fn rect(self) -> FrameRect {
        self.rect
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PresentedPointerHit {
    frame_id: u64,
    presentation: PresentationId,
    interaction: Option<InteractionId>,
    appearance: Option<PointerAppearanceId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PresentedInteractionKey {
    frame_id: u64,
    presentation: PresentationId,
    interaction: InteractionId,
}

impl PresentedInteractionKey {
    pub(super) const fn new(presentation: PresentationId, interaction: InteractionId) -> Self {
        Self {
            frame_id: 0,
            presentation,
            interaction,
        }
    }

    pub(super) const fn for_frame(
        frame_id: u64,
        presentation: PresentationId,
        interaction: InteractionId,
    ) -> Self {
        Self {
            frame_id,
            presentation,
            interaction,
        }
    }

    pub(super) const fn frame_id(self) -> u64 {
        self.frame_id
    }

    pub(super) const fn presentation(self) -> PresentationId {
        self.presentation
    }

    pub(super) const fn interaction(self) -> InteractionId {
        self.interaction
    }
}

/// A left-button press owned by a presented interaction. A `None` target is
/// reserved for blank chrome space whose release must not leak to the buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PresentedPressCapture {
    target: Option<PresentedInteractionKey>,
    surface_origin: (f32, f32),
}

impl PresentedPressCapture {
    pub(super) const fn new(target: Option<PresentedInteractionKey>) -> Self {
        Self {
            target,
            surface_origin: (0.0, 0.0),
        }
    }

    pub(super) const fn with_surface_origin(
        target: PresentedInteractionKey,
        surface_origin: (f32, f32),
    ) -> Self {
        Self {
            target: Some(target),
            surface_origin,
        }
    }

    pub(super) const fn target(self) -> Option<PresentedInteractionKey> {
        self.target
    }

    pub(super) fn local_coordinates(self, surface_x: f32, surface_y: f32) -> (f32, f32) {
        (
            surface_x - self.surface_origin.0,
            surface_y - self.surface_origin.1,
        )
    }
}

impl PresentedPointerHit {
    pub(super) const fn new(
        frame_id: u64,
        presentation: PresentationId,
        interaction: Option<InteractionId>,
        appearance: Option<PointerAppearanceId>,
    ) -> Self {
        Self {
            frame_id,
            presentation,
            interaction,
            appearance,
        }
    }

    pub(super) const fn presentation(self) -> PresentationId {
        self.presentation
    }

    pub(super) const fn interaction(self) -> Option<InteractionId> {
        self.interaction
    }

    pub(super) fn appearance_key(self) -> Option<PresentedAppearanceKey> {
        self.appearance.map(|appearance| {
            PresentedAppearanceKey::for_frame(self.frame_id, self.presentation, appearance)
        })
    }
}

impl PresentedAppearanceKey {
    #[cfg(test)]
    pub(super) const fn new(presentation: PresentationId, appearance: PointerAppearanceId) -> Self {
        Self {
            frame_id: 0,
            presentation,
            appearance,
        }
    }

    pub(super) const fn for_frame(
        frame_id: u64,
        presentation: PresentationId,
        appearance: PointerAppearanceId,
    ) -> Self {
        Self {
            frame_id,
            presentation,
            appearance,
        }
    }

    pub(super) const fn frame_id(self) -> u64 {
        self.frame_id
    }

    pub(super) const fn presentation(self) -> PresentationId {
        self.presentation
    }

    pub(super) const fn appearance(self) -> PointerAppearanceId {
        self.appearance
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActivePointerAppearance {
    key: PresentedAppearanceKey,
    phase: PointerAppearancePhase,
}

impl ActivePointerAppearance {
    pub(super) const fn new(key: PresentedAppearanceKey, phase: PointerAppearancePhase) -> Self {
        Self { key, phase }
    }

    pub(super) const fn key(self) -> PresentedAppearanceKey {
        self.key
    }

    pub(super) const fn presentation(self) -> PresentationId {
        self.key.presentation()
    }

    pub(super) const fn appearance(self) -> PointerAppearanceId {
        self.key.appearance()
    }

    #[cfg(test)]
    pub(super) const fn phase(self) -> PointerAppearancePhase {
        self.phase
    }

    /// Produce renderer state only for the exact immutable presentation that
    /// owns both the appearance id and its primitive spans.
    pub(super) fn selection_for(
        self,
        frame: &FrameGlyphBuffer,
    ) -> Option<PointerAppearanceSelection> {
        if self.presentation() != frame.presentation_id
            || frame
                .presented_pointer()
                .appearance(self.appearance())
                .is_none()
        {
            return None;
        }
        Some(PointerAppearanceSelection::new(
            self.appearance(),
            self.phase,
        ))
    }
}

/// Snapshot-qualified pointer visual selection.
///
/// The pressed key is intentionally independent from the currently hovered
/// key: capture may remain on one visual range while the pointer hovers
/// another, and returning to the captured range restores its pressed phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PointerAppearanceState {
    active: Option<ActivePointerAppearance>,
    pressed: Option<PresentedAppearanceKey>,
}

impl PointerAppearanceState {
    pub(super) const fn active(self) -> Option<ActivePointerAppearance> {
        self.active
    }

    pub(super) fn selection_for(
        self,
        frame: &FrameGlyphBuffer,
    ) -> Option<PointerAppearanceSelection> {
        self.active.and_then(|active| active.selection_for(frame))
    }

    #[cfg(test)]
    pub(super) const fn pressed(self) -> Option<PresentedAppearanceKey> {
        self.pressed
    }

    pub(super) fn hover(&mut self, key: Option<PresentedAppearanceKey>) -> bool {
        let previous = *self;
        let next = key.map(|key| {
            let phase = if self.pressed == Some(key) {
                PointerAppearancePhase::Pressed
            } else {
                PointerAppearancePhase::Hover
            };
            ActivePointerAppearance::new(key, phase)
        });
        self.active = next;
        *self != previous
    }

    pub(super) fn hover_would_change(self, key: Option<PresentedAppearanceKey>) -> bool {
        let mut next = self;
        next.hover(key)
    }

    pub(super) fn button_would_change(
        self,
        key: Option<PresentedAppearanceKey>,
        pressed: bool,
    ) -> bool {
        let mut next = self;
        let mut changed = next.hover(key);
        changed |= if pressed {
            next.press()
        } else {
            next.release()
        };
        changed
    }

    pub(super) fn press(&mut self) -> bool {
        let previous = *self;
        self.pressed = self.active.map(ActivePointerAppearance::key);
        if let Some(active) = self.active.as_mut() {
            active.phase = PointerAppearancePhase::Pressed;
        }
        *self != previous
    }

    pub(super) fn release(&mut self) -> bool {
        let previous = *self;
        self.pressed = None;
        if let Some(active) = self.active.as_mut() {
            active.phase = PointerAppearancePhase::Hover;
        }
        *self != previous
    }

    pub(super) fn retire(&mut self, presentation: PresentationId) -> bool {
        let previous = *self;
        if self
            .active
            .is_some_and(|active| active.presentation() == presentation)
        {
            self.active = None;
        }
        if self
            .pressed
            .is_some_and(|pressed| pressed.presentation() == presentation)
        {
            self.pressed = None;
        }
        *self != previous
    }

    pub(super) fn cancel(&mut self) -> bool {
        let changed = self.active.is_some() || self.pressed.is_some();
        *self = Self::default();
        changed
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GuiChromeInteractionState {
    pub(super) menu_bar_hovered: Option<u32>,
    pub(super) menu_bar_active: Option<u32>,
    pub(super) toolbar_hovered: Option<u32>,
    pub(super) toolbar_pressed: Option<u32>,
    pub(super) toolbar_press_captured: bool,
    pub(super) compact_bar_menu_hovered: Option<u32>,
    pub(super) compact_bar_menu_active: Option<u32>,
    pub(super) compact_bar_tool_hovered: Option<u32>,
    pub(super) compact_bar_tool_pressed: Option<u32>,
}

impl GuiChromeInteractionState {
    pub(super) fn clear_menu_bar(&mut self) {
        self.menu_bar_hovered = None;
        self.menu_bar_active = None;
    }

    pub(super) fn clear_tab_bar(&mut self) {
        // Preserve press capture across chrome removal so a chrome press does
        // not leak a buffer release if the tab bar disappears mid-click.
    }

    pub(super) fn clear_toolbar(&mut self) {
        self.toolbar_hovered = None;
        self.toolbar_pressed = None;
        // Preserve press capture across chrome removal so a chrome press does
        // not leak a buffer release if the toolbar disappears mid-click.
    }

    pub(super) fn clear_compact_bar(&mut self) {
        self.compact_bar_menu_hovered = None;
        self.compact_bar_menu_active = None;
        self.compact_bar_tool_hovered = None;
        self.compact_bar_tool_pressed = None;
    }
}

pub(super) struct ChildFrameStyle {
    pub(super) corner_radius: f32,
    pub(super) shadow_enabled: bool,
    pub(super) shadow_layers: u32,
    pub(super) shadow_offset: f32,
    pub(super) shadow_opacity: f32,
}

impl Default for ChildFrameStyle {
    fn default() -> Self {
        Self {
            corner_radius: 0.0,
            shadow_enabled: true,
            shadow_layers: 4,
            shadow_offset: 2.0,
            shadow_opacity: 0.3,
        }
    }
}

#[derive(Default)]
pub(super) struct ToolbarResources {
    pub(super) icon_textures: HashMap<(ToolBarImageSource, u32), ImageId>,
}

pub(super) struct RenderGpuContext {
    pub(super) instance: wgpu::Instance,
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
}

pub(super) struct RenderApp {
    pub(super) comms: RenderComms,

    /// Display-lifetime owner for decoded application icon data and native
    /// Wayland toplevel-icon protocol state.
    pub(super) window_icon: crate::window_icon::WindowIconService,

    /// Non-blocking handle to the clipboard worker that owns its display.
    pub(super) clipboard: Result<ClipboardService, String>,

    pub(super) gpu: Option<RenderGpuContext>,
    pub(super) renderer: Option<WgpuRenderer>,
    /// Native decoder workers signal this callback after replacing a latest
    /// frame or publishing control state. Production installs a winit proxy;
    /// tests deliberately retain the no-op callback.
    #[cfg(feature = "video")]
    pub(super) video_wake: neomacs_video::VideoWake,
    /// Native browser callbacks wake winit after publishing lifecycle, script,
    /// or frame work. Production replaces the no-op test value before run.
    #[cfg(feature = "webview")]
    pub(super) webview_wake: neomacs_webview::WebViewWake,
    /// Device generation attached to every imported native video surface.
    #[cfg(feature = "video")]
    pub(super) video_gpu_generation: neomacs_video::GpuGeneration,
    /// GPU-independent playback intent parked while a lost device and its
    /// renderer-owned VideoSystem are rebuilt.
    #[cfg(feature = "video")]
    pub(super) pending_video_recovery: Vec<neomacs_renderer_wgpu::VideoRecoveryManifest>,
    pub(super) backend_profile: RenderBackendProfile,
    pub(super) render_policy: RenderQualityPolicy,

    /// Shared device-lost latch (`Arc<AtomicBool>` inside) plus the
    /// consecutive surface-Lost streak. Fed by the wgpu device-lost callback
    /// and the surface-acquire path; drained at the top of
    /// `handle_about_to_wait`, which runs `recover_from_device_loss`.
    pub(super) device_lost: super::device_loss::DeviceLossDetector,

    /// Shared media memory accounting. Fed from the asset-command choke
    /// point (`asset_commands.rs`); shader surfaces only so far — see the
    pub(super) faces: HashMap<neomacs_display_protocol::types::FaceId, Face>,
    /// Sorted (frame_id, ingest_seq) fingerprint of the frames the current
    /// `faces` map was aggregated from; unchanged fingerprint skips the
    /// per-render rebuild entirely.
    pub(super) faces_signature: Vec<(u64, u64)>,
    pub(super) modifiers: u32,

    pub(super) image_metadata: SharedImageRenderState,

    pub(super) cursor_defaults: CursorState,

    pub(super) requested_visual_config: VisualConfig,
    pub(super) effects: EffectsConfig,

    pub(super) transition_policy: TransitionPolicy,
    #[cfg(feature = "webview")]
    pub(super) webview_system: Option<neomacs_webview::WebViewSystem>,
    #[cfg(feature = "webview")]
    pub(super) webview_scene_clocks:
        HashMap<neomacs_webview::HostWindowId, super::frame_ingest::WebViewSceneClock>,
    #[cfg(feature = "webview")]
    pub(super) focused_webview: Option<neomacs_webview::WebViewInputTarget>,
    #[cfg(feature = "webview")]
    pub(super) webview_pointer_capture: Option<WebViewPointerCapture>,

    #[cfg(feature = "neo-term")]
    pub(super) terminal_manager: crate::terminal::TerminalManager,
    #[cfg(feature = "neo-term")]
    pub(super) shared_terminals: crate::terminal::SharedTerminals,

    pub(super) frame_windows: GuiFrameWindowManager,
    /// Latest child snapshots whose immediate ancestry has not been presented
    /// yet. They are retried transactionally when an ancestor arrives.
    pub(super) pending_child_frames:
        HashMap<u64, neomacs_display_protocol::SealedFramePresentation>,

    pub(super) child_frame_style: ChildFrameStyle,
    pub(super) toolbar: ToolbarResources,

    pub(super) scroll_indicators_enabled: bool,

    pub(super) extra_line_spacing: f32,
    pub(super) extra_letter_spacing: f32,

    pub(super) shared_monitors: Option<SharedMonitorInfo>,
    pub(super) monitors_populated: bool,
    pub(super) last_monitor_snapshot: Vec<MonitorInfo>,
    pub(super) debug_first_frame_readback_pending: bool,
    pub(super) debug_surface_readback_frames_remaining: u32,
    pub(super) lifecycle_flags: RenderLifecycle,
    /// Demand-driven frame scheduler: owns per-window redraw coalescing and
    /// wake deadlines (frame scheduling plan, Stage 2).
    pub(super) frame_coordinator: super::frame_sched::FrameCoordinator,
}

pub(super) struct RenderLifecycle {
    pub resumed_seen: bool,
    pub about_to_wait_seen: bool,
    pub poll_when_idle: bool,
    pub shutdown_requested: bool,
}

impl RenderLifecycle {
    pub fn new(poll_when_idle: bool) -> Self {
        Self {
            resumed_seen: false,
            about_to_wait_seen: false,
            poll_when_idle,
            shutdown_requested: false,
        }
    }
}

/// The `NEOMACS_MEDIA_BUDGET_MB` override (a decimal megabyte count) lets
/// the eviction driver be exercised interactively without allocating
/// hundreds of megabytes of surfaces. Unset or unparseable values fall back
/// to the renderer's 256MB default. Applied to the renderer (which owns the
/// budget, beside the caches) at bootstrap.
pub(super) fn media_budget_env_limit() -> Option<usize> {
    std::env::var("NEOMACS_MEDIA_BUDGET_MB")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map(|mb| mb.saturating_mul(1024 * 1024))
}

impl RenderApp {
    pub(super) fn install_backend_profile(&mut self, profile: RenderBackendProfile) {
        self.backend_profile = profile;
        self.apply_requested_visual_config();
    }

    pub(super) fn apply_requested_visual_config(&mut self) {
        let next_policy =
            RenderQualityPolicy::negotiate(self.backend_profile, &self.requested_visual_config);
        tracing::info!(
            adapter_class = ?next_policy.profile().adapter_class(),
            quality_mode = ?next_policy.mode(),
            "applying render-quality policy"
        );
        let frame_shader_availability = if next_policy.frame_post_disposition().is_enabled() {
            FrameShaderAvailability::Available
        } else {
            FrameShaderAvailability::SuppressedByQualityPolicy
        };
        self.comms
            .capabilities
            .publish_frame_shader_availability(frame_shader_availability);
        if !next_policy.allows_dynamic_effects() {
            self.frame_windows.discard_top_level_renderer_effects();
        }
        let effective = next_policy.effective_visual_config();
        self.cursor_defaults.apply_visual_config(effective);
        self.transition_policy = next_policy.transition_policy();
        self.frame_windows
            .apply_top_level_transition_policy(self.transition_policy);
        self.effects = effective.effects.clone();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.effects = self.effects.clone();
        }
        self.frame_windows
            .sync_top_level_cursor_config(&self.cursor_defaults, true);
        if !self.cursor_defaults.blink_enabled {
            self.frame_windows.force_top_level_cursor_blink_on();
        }
        self.render_policy = next_policy;
    }

    pub(super) fn new(
        comms: RenderComms,
        width: u32,
        height: u32,
        title: String,
        image_metadata: SharedImageRenderState,
        shared_monitors: SharedMonitorInfo,
        poll_when_idle: bool,
        #[cfg(feature = "neo-term")] shared_terminals: crate::terminal::SharedTerminals,
    ) -> Self {
        let mut frame_windows = GuiFrameWindowManager::new();
        frame_windows.set_primary_pending(GuiFrameWindowState {
            lifecycle: FrameLifecycle::Pending {
                width,
                height,
                scale_factor: 1.0,
                mouse_hidden_for_typing: false,
                ime_enabled: false,
                last_ime_cursor_area: None,
                chrome: WindowChrome {
                    title,
                    ..WindowChrome::default()
                },
                geometry_hints: None,
            },
            render: GuiFrameRenderState::new_without_device(
                0,
                false,
                neomacs_display_protocol::frame_time::observe_platform_now(),
            ),
        });

        let requested_visual_config = VisualConfig::default();
        let backend_profile = RenderBackendProfile::pending();
        let render_policy =
            RenderQualityPolicy::negotiate(backend_profile, &requested_visual_config);

        Self {
            comms,
            window_icon: crate::window_icon::WindowIconService::new(),
            clipboard: Err("clipboard is unavailable before display initialization".to_owned()),
            gpu: None,
            renderer: None,
            #[cfg(feature = "video")]
            video_wake: neomacs_video::VideoWake::noop(),
            #[cfg(feature = "webview")]
            webview_wake: neomacs_webview::WebViewWake::noop(),
            #[cfg(feature = "video")]
            video_gpu_generation: neomacs_video::GpuGeneration::INITIAL,
            #[cfg(feature = "video")]
            pending_video_recovery: Vec::new(),
            backend_profile,
            render_policy,
            device_lost: super::device_loss::DeviceLossDetector::new(),
            faces: HashMap::new(),
            faces_signature: Vec::new(),
            modifiers: 0,
            image_metadata,
            cursor_defaults: CursorState::default(),
            requested_visual_config,
            effects: EffectsConfig::default(),
            transition_policy: TransitionPolicy::default(),
            #[cfg(feature = "webview")]
            webview_system: None,
            #[cfg(feature = "webview")]
            webview_scene_clocks: HashMap::new(),
            #[cfg(feature = "webview")]
            focused_webview: None,
            #[cfg(feature = "webview")]
            webview_pointer_capture: None,
            #[cfg(feature = "neo-term")]
            terminal_manager: crate::terminal::TerminalManager::new(),
            #[cfg(feature = "neo-term")]
            shared_terminals,
            frame_windows,
            pending_child_frames: HashMap::new(),
            child_frame_style: ChildFrameStyle::default(),
            toolbar: ToolbarResources::default(),
            scroll_indicators_enabled: false,
            extra_line_spacing: 0.0,
            extra_letter_spacing: 0.0,
            shared_monitors: Some(shared_monitors),
            monitors_populated: false,
            last_monitor_snapshot: Vec::new(),
            debug_first_frame_readback_pending: std::env::var_os(
                "NEOMACS_DEBUG_FIRST_FRAME_READBACK",
            )
            .is_some(),
            debug_surface_readback_frames_remaining: std::env::var(
                "NEOMACS_DEBUG_SURFACE_READBACK",
            )
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|count| *count > 0)
            .unwrap_or_else(|| {
                if std::env::var_os("NEOMACS_DEBUG_SURFACE_READBACK").is_some() {
                    32
                } else {
                    0
                }
            }),
            lifecycle_flags: RenderLifecycle::new(poll_when_idle),
            frame_coordinator: super::frame_sched::FrameCoordinator::new(),
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod tests;
