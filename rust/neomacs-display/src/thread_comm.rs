//! Thread communication infrastructure for two-thread architecture.
//!
//! Provides lock-free channels and wakeup mechanism between Emacs and render threads.

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::os::unix::io::RawFd;

use crate::core::frame_glyphs::FrameGlyphBuffer;

/// Input event from render thread to Emacs
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key {
        keysym: u32,
        modifiers: u32,
        pressed: bool,
    },
    MouseButton {
        button: u32,
        x: f32,
        y: f32,
        pressed: bool,
        modifiers: u32,
    },
    MouseMove {
        x: f32,
        y: f32,
        modifiers: u32,
    },
    MouseScroll {
        delta_x: f32,
        delta_y: f32,
        x: f32,
        y: f32,
        modifiers: u32,
        /// True if deltas are in pixels (touchpad), false if in lines (mouse wheel)
        pixel_precise: bool,
    },
    WindowResize {
        width: u32,
        height: u32,
    },
    WindowClose,
    WindowFocus {
        focused: bool,
    },
    /// WebKit view title changed
    #[cfg(feature = "wpe-webkit")]
    WebKitTitleChanged {
        id: u32,
        title: String,
    },
    /// WebKit view URL changed
    #[cfg(feature = "wpe-webkit")]
    WebKitUrlChanged {
        id: u32,
        url: String,
    },
    /// WebKit view load progress changed
    #[cfg(feature = "wpe-webkit")]
    WebKitProgressChanged {
        id: u32,
        progress: f64,
    },
    /// WebKit view finished loading
    #[cfg(feature = "wpe-webkit")]
    WebKitLoadFinished {
        id: u32,
    },
    /// Image dimensions ready (sent after async image load)
    ImageDimensionsReady {
        id: u32,
        width: u32,
        height: u32,
    },
    /// Terminal child process exited
    #[cfg(feature = "neo-term")]
    TerminalExited { id: u32 },
    /// Terminal title changed
    #[cfg(feature = "neo-term")]
    TerminalTitleChanged { id: u32, title: String },
    /// Popup menu selection made (index into menu items, -1 = cancelled)
    MenuSelection { index: i32 },
    /// File(s) dropped onto the window
    FileDrop {
        paths: Vec<String>,
        x: f32,
        y: f32,
    },
}

/// A single item in a popup menu
#[derive(Debug, Clone)]
pub struct PopupMenuItem {
    /// Display label for the item
    pub label: String,
    /// Keyboard shortcut text (e.g., "C-x C-s"), or empty
    pub shortcut: String,
    /// Whether the item is enabled (selectable)
    pub enabled: bool,
    /// Whether this is a separator line
    pub separator: bool,
    /// Whether this is a submenu header (has children)
    pub submenu: bool,
    /// Nesting depth (0 = top-level, 1 = first submenu, etc.)
    pub depth: u32,
}

/// Command from Emacs to render thread
#[derive(Debug)]
pub enum RenderCommand {
    /// Shutdown the render thread
    Shutdown,
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
    /// Load image from file (async, ID pre-allocated)
    ImageLoadFile {
        id: u32,
        path: String,
        max_width: u32,
        max_height: u32,
    },
    /// Free an image from cache
    ImageFree { id: u32 },
    /// Create a WebKit view
    WebKitCreate { id: u32, width: u32, height: u32 },
    /// Load URL in WebKit view
    WebKitLoadUri { id: u32, url: String },
    /// Resize WebKit view
    WebKitResize { id: u32, width: u32, height: u32 },
    /// Destroy WebKit view
    WebKitDestroy { id: u32 },
    /// Click in WebKit view
    WebKitClick { id: u32, x: i32, y: i32, button: u32 },
    /// Pointer event in WebKit view (raw API)
    WebKitPointerEvent { id: u32, event_type: u32, x: i32, y: i32, button: u32, state: u32, modifiers: u32 },
    /// Scroll in WebKit view
    WebKitScroll { id: u32, x: i32, y: i32, delta_x: i32, delta_y: i32 },
    /// Keyboard event in WebKit view
    WebKitKeyEvent { id: u32, keyval: u32, keycode: u32, pressed: bool, modifiers: u32 },
    /// Navigate back in WebKit view
    WebKitGoBack { id: u32 },
    /// Navigate forward in WebKit view
    WebKitGoForward { id: u32 },
    /// Reload WebKit view
    WebKitReload { id: u32 },
    /// Execute JavaScript in WebKit view
    WebKitExecuteJavaScript { id: u32, script: String },
    /// Set floating WebKit overlay position and size
    WebKitSetFloating { id: u32, x: f32, y: f32, width: f32, height: f32 },
    /// Remove floating WebKit overlay
    WebKitRemoveFloating { id: u32 },
    /// Create video player
    VideoCreate { id: u32, path: String },
    /// Control video playback
    VideoPlay { id: u32 },
    VideoPause { id: u32 },
    VideoDestroy { id: u32 },
    /// Change the mouse pointer cursor shape (arrow, hand, ibeam, etc.)
    SetMouseCursor { cursor_type: i32 },
    /// Warp (move) the mouse pointer to given pixel position
    WarpMouse { x: i32, y: i32 },
    /// Set the window title
    SetWindowTitle { title: String },
    /// Set fullscreen mode (0=none, 1=fullscreen, 4=maximized)
    SetWindowFullscreen { mode: u32 },
    /// Minimize/iconify the window
    SetWindowMinimized { minimized: bool },
    /// Set window position
    SetWindowPosition { x: i32, y: i32 },
    /// Request window inner size change
    SetWindowSize { width: u32, height: u32 },
    /// Set window decorations (title bar, borders)
    SetWindowDecorated { decorated: bool },
    /// Configure cursor blinking
    SetCursorBlink { enabled: bool, interval_ms: u32 },
    /// Configure cursor animation (smooth motion)
    SetCursorAnimation { enabled: bool, speed: f32 },
    /// Configure all animations
    SetAnimationConfig {
        cursor_enabled: bool,
        cursor_speed: f32,
        cursor_style: crate::core::types::CursorAnimStyle,
        cursor_duration_ms: u32,
        crossfade_enabled: bool,
        crossfade_duration_ms: u32,
        scroll_enabled: bool,
        scroll_duration_ms: u32,
        scroll_effect: u32,
        scroll_easing: u32,
        trail_size: f32,
        crossfade_effect: u32,
        crossfade_easing: u32,
    },
    /// Create a terminal
    #[cfg(feature = "neo-term")]
    TerminalCreate {
        id: u32,
        cols: u16,
        rows: u16,
        mode: u8, // 0=Window, 1=Inline, 2=Floating
        shell: Option<String>,
    },
    /// Write input to a terminal
    #[cfg(feature = "neo-term")]
    TerminalWrite { id: u32, data: Vec<u8> },
    /// Resize a terminal
    #[cfg(feature = "neo-term")]
    TerminalResize { id: u32, cols: u16, rows: u16 },
    /// Destroy a terminal
    #[cfg(feature = "neo-term")]
    TerminalDestroy { id: u32 },
    /// Set floating terminal position and opacity
    #[cfg(feature = "neo-term")]
    TerminalSetFloat { id: u32, x: f32, y: f32, opacity: f32 },
    /// Show a popup menu at position (x, y)
    ShowPopupMenu {
        x: f32,
        y: f32,
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
        x: f32,
        y: f32,
        text: String,
        fg_r: f32, fg_g: f32, fg_b: f32,
        bg_r: f32, bg_g: f32, bg_b: f32,
    },
    /// Hide the active tooltip
    HideTooltip,
    /// Trigger visual bell flash
    VisualBell,
    /// Request window attention (urgency hint / taskbar flash)
    RequestAttention { urgent: bool },
    /// Toggle scroll indicators and focus ring
    SetScrollIndicators { enabled: bool },
    /// Set custom title bar height (0 = hidden, >0 = show with given height)
    SetTitlebarHeight { height: f32 },
    /// Toggle FPS counter overlay
    SetShowFps { enabled: bool },
    /// Set window corner radius for borderless mode (0 = no rounding)
    SetCornerRadius { radius: f32 },
    /// Set extra spacing (line spacing in pixels, letter spacing in pixels)
    SetExtraSpacing { line_spacing: f32, letter_spacing: f32 },
    /// Set background gradient (top and bottom colors in sRGB 0.0-1.0, angle 0=vertical)
    SetBackgroundGradient {
        enabled: bool,
        top_r: f32, top_g: f32, top_b: f32,
        bottom_r: f32, bottom_g: f32, bottom_b: f32,
    },
    /// Configure scroll bar appearance
    SetScrollBarConfig {
        /// Width in pixels (0 = use default 12px)
        width: i32,
        /// Thumb corner radius ratio (0.0-1.0, default 0.4)
        thumb_radius: f32,
        /// Track background opacity (0.0-1.0, default 0.6)
        track_opacity: f32,
        /// Hover brightness multiplier (default 1.4)
        hover_brightness: f32,
    },
    /// Configure indent guide rendering
    SetIndentGuideConfig {
        enabled: bool,
        /// Color as sRGB 0.0-1.0
        r: f32, g: f32, b: f32,
        /// Opacity 0.0-1.0
        opacity: f32,
    },
    /// Configure rainbow indent guide colors (up to 6 cycling colors by depth)
    SetIndentGuideRainbow {
        enabled: bool,
        /// Colors as sRGB 0.0-1.0 tuples with opacity
        colors: Vec<(f32, f32, f32, f32)>,
    },
    /// Configure current line highlight
    SetLineHighlight {
        enabled: bool,
        /// Color as sRGB 0.0-1.0
        r: f32, g: f32, b: f32,
        /// Opacity 0.0-1.0
        opacity: f32,
    },
    /// Configure visible whitespace rendering
    SetShowWhitespace {
        enabled: bool,
        /// Color as sRGB 0.0-1.0
        r: f32, g: f32, b: f32,
        /// Opacity 0.0-1.0
        opacity: f32,
    },
    /// Configure inactive window dimming
    SetInactiveDim {
        enabled: bool,
        /// Dimming opacity 0.0-1.0 (how much to darken)
        opacity: f32,
    },
    /// Configure mode-line separator style
    SetModeLineSeparator {
        /// 0=none, 1=line, 2=shadow, 3=gradient
        style: u32,
        /// Separator color as sRGB 0.0-1.0
        r: f32, g: f32, b: f32,
        /// Height in pixels (for shadow/gradient)
        height: f32,
    },
    /// Configure cursor glow effect
    SetCursorGlow {
        enabled: bool,
        /// Glow color as sRGB 0.0-1.0
        r: f32, g: f32, b: f32,
        /// Glow radius in pixels
        radius: f32,
        /// Peak opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor pulse animation (sinusoidal glow modulation)
    SetCursorPulse {
        enabled: bool,
        /// Pulse speed in Hz (cycles per second, e.g. 1.0 = 1 cycle/sec)
        speed: f32,
        /// Minimum opacity multiplier (0.0-1.0)
        min_opacity: f32,
    },
    /// Configure focus mode (dim everything outside focused region)
    SetFocusMode {
        enabled: bool,
        /// Dimming opacity for unfocused lines (0.0-1.0)
        opacity: f32,
    },
    /// Configure minimap (code overview column)
    SetMinimap {
        enabled: bool,
        /// Width of the minimap column in pixels
        width: f32,
    },
    /// Configure typing ripple effect
    SetTypingRipple {
        enabled: bool,
        /// Maximum radius in pixels
        max_radius: f32,
        /// Duration in milliseconds
        duration_ms: u32,
    },
    /// Configure search highlight pulse (glow on isearch match)
    SetSearchPulse {
        enabled: bool,
        /// Face ID of the isearch face (used to identify matching glyphs)
        face_id: u32,
    },
    /// Configure line insertion/deletion animation
    SetLineAnimation {
        enabled: bool,
        duration_ms: u32,
    },
    /// Configure vignette effect (edge darkening)
    SetVignette {
        enabled: bool,
        /// Intensity of darkening (0.0-1.0)
        intensity: f32,
        /// Radius as percentage of frame diagonal (0-100)
        radius: f32,
    },
    /// Configure window switch highlight fade
    SetWindowSwitchFade {
        enabled: bool,
        duration_ms: u32,
        intensity: f32,
    },
    /// Configure inactive window color tint
    SetInactiveTint {
        enabled: bool,
        /// Tint color (sRGB 0.0-1.0)
        color: (f32, f32, f32),
        /// Tint opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure scroll progress indicator bar
    SetScrollProgress {
        enabled: bool,
        /// Bar height in pixels
        height: f32,
        /// Bar color (sRGB 0.0-1.0)
        color: (f32, f32, f32),
        /// Bar opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure active window border glow
    SetWindowGlow {
        enabled: bool,
        /// Glow color (sRGB 0.0-1.0)
        color: (f32, f32, f32),
        /// Glow radius in pixels
        radius: f32,
        /// Peak glow opacity (0.0-1.0)
        intensity: f32,
    },
    /// Configure breadcrumb/path bar overlay
    SetBreadcrumb {
        enabled: bool,
        /// Background opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure breadcrumb title fade animation
    SetTitleFade {
        enabled: bool,
        /// Fade duration in milliseconds
        duration_ms: u32,
    },
    /// Configure typing speed indicator overlay
    SetTypingSpeed {
        enabled: bool,
    },
    /// Configure smooth border color transition on focus change
    SetBorderTransition {
        enabled: bool,
        /// Active border color (sRGB 0.0-1.0)
        active_color: (f32, f32, f32),
        /// Duration in milliseconds
        duration_ms: u32,
    },
    /// Configure buffer-local accent color strip
    SetAccentStrip {
        enabled: bool,
        /// Strip width in pixels
        width: f32,
    },
    /// Configure frosted glass effect on mode-lines
    SetFrostedGlass {
        enabled: bool,
        /// Frost opacity (0.0-1.0)
        opacity: f32,
        /// Blur radius (noise spread in pixels)
        blur: f32,
    },
    /// Configure cursor trail fade (afterimage ghost trail)
    SetCursorTrailFade {
        enabled: bool,
        /// Number of trail positions to keep
        length: u32,
        /// Fade duration in milliseconds
        fade_ms: u32,
    },
    /// Configure selection region glow highlight
    SetRegionGlow {
        enabled: bool,
        /// Face ID of the region face
        face_id: u32,
        /// Glow radius in pixels
        radius: f32,
        /// Glow opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure idle screen dimming after inactivity
    SetIdleDim {
        enabled: bool,
        /// Seconds of inactivity before dimming starts
        delay_secs: f32,
        /// Dim opacity (0.0-1.0, how dark to make it)
        opacity: f32,
        /// Fade duration in milliseconds
        fade_ms: u32,
    },
    /// Configure noise/film grain texture overlay
    SetNoiseGrain {
        enabled: bool,
        /// Grain intensity (0.0-1.0)
        intensity: f32,
        /// Grain size in pixels
        size: f32,
    },
    /// Configure window padding gradient (inner edge shading for depth)
    SetPaddingGradient {
        enabled: bool,
        /// Edge color (sRGB 0.0-1.0)
        color: (f32, f32, f32),
        /// Peak opacity at edge (0.0-1.0)
        opacity: f32,
        /// Width of gradient in pixels
        width: f32,
    },
    /// Configure cursor drop shadow
    SetCursorShadow {
        enabled: bool,
        /// Horizontal shadow offset in pixels
        offset_x: f32,
        /// Vertical shadow offset in pixels
        offset_y: f32,
        /// Shadow opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure animated focus ring around selected window
    SetFocusRing {
        enabled: bool,
        /// Ring color (sRGB 0.0-1.0)
        color: (f32, f32, f32),
        /// Ring opacity (0.0-1.0)
        opacity: f32,
        /// Dash length in pixels
        dash_length: f32,
        /// Animation speed (pixels per second)
        speed: f32,
    },
    /// Configure window background tint based on file type
    SetWindowModeTint {
        enabled: bool,
        /// Tint opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure window watermark for empty/small buffers
    SetWindowWatermark {
        enabled: bool,
        /// Opacity of watermark text (0.0-1.0)
        opacity: f32,
        /// Max buffer size (chars) to show watermark
        threshold: u32,
    },
    /// Configure smooth cursor size transition on text-scale-adjust
    SetCursorSizeTransition {
        enabled: bool,
        /// Transition duration in milliseconds
        duration_ms: u32,
    },
    /// Configure cursor color cycling (rainbow hue rotation)
    SetCursorColorCycle {
        enabled: bool,
        /// Cycles per second
        speed: f32,
        /// Saturation (0.0-1.0)
        saturation: f32,
        /// Lightness (0.0-1.0)
        lightness: f32,
    },
    /// Configure header/mode-line shadow depth effect
    SetHeaderShadow {
        enabled: bool,
        /// Shadow intensity (0.0-1.0)
        intensity: f32,
        /// Shadow size in pixels
        size: f32,
    },
    /// Configure zen mode (distraction-free centered content)
    SetZenMode {
        enabled: bool,
        /// Width of the content area in percentage of window width (0-100)
        content_width_pct: f32,
        /// Opacity of the margin overlay (0.0-1.0)
        margin_opacity: f32,
    },
    /// Configure smooth mode-line content transition
    SetModeLineTransition {
        enabled: bool,
        /// Transition duration in milliseconds
        duration_ms: u32,
    },
    /// Configure cursor wake animation (pop/scale effect on blink-on)
    SetCursorWake {
        enabled: bool,
        /// Duration of wake animation in milliseconds
        duration_ms: u32,
        /// Scale factor at start of animation (e.g. 1.5 = 150% size)
        scale: f32,
    },
    /// Configure text fade-in animation for new content
    SetTextFadeIn {
        enabled: bool,
        /// Fade-in duration in milliseconds
        duration_ms: u32,
    },
    /// Configure line spacing animation on scroll (accordion effect)
    SetScrollLineSpacing {
        enabled: bool,
        /// Extra spacing in pixels applied at leading edge during scroll
        max_spacing: f32,
        /// Duration of the spacing animation in milliseconds
        duration_ms: u32,
    },
    /// Configure window content shadow/depth between split panes
    SetWindowContentShadow {
        enabled: bool,
        /// Shadow size in pixels
        size: f32,
        /// Shadow opacity 0.0-1.0
        opacity: f32,
    },
    /// Configure window edge snap indicator
    SetEdgeSnap {
        enabled: bool,
        /// Flash color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Flash duration in milliseconds
        duration_ms: u32,
    },
    /// Configure cursor crosshair guide lines
    SetCursorCrosshair {
        enabled: bool,
        /// Line color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Line opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure buffer modified border indicator
    SetModifiedIndicator {
        enabled: bool,
        /// Border color for modified buffers (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Border width in pixels
        width: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure inactive window stained glass effect
    SetStainedGlass {
        enabled: bool,
        /// Tint opacity (0.0-1.0)
        opacity: f32,
        /// Saturation of the generated color (0.0-1.0)
        saturation: f32,
    },
    /// Configure focused window gradient border
    SetFocusGradientBorder {
        enabled: bool,
        /// Top color (sRGB floats)
        top_r: f32, top_g: f32, top_b: f32,
        /// Bottom color (sRGB floats)
        bot_r: f32, bot_g: f32, bot_b: f32,
        /// Border width in pixels
        width: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor magnetism effect on click
    SetCursorMagnetism {
        enabled: bool,
        /// Ring color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of concentric rings
        ring_count: u32,
        /// Animation duration in ms
        duration_ms: u32,
        /// Max opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure window depth shadow layers
    SetDepthShadow {
        enabled: bool,
        /// Number of shadow layers
        layers: u32,
        /// Offset per layer in pixels
        offset: f32,
        /// Shadow color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Base opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure mode-line gradient background
    SetModeLineGradient {
        enabled: bool,
        /// Left color (sRGB floats)
        left_r: f32, left_g: f32, left_b: f32,
        /// Right color (sRGB floats)
        right_r: f32, right_g: f32, right_b: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure window corner fold effect
    SetCornerFold {
        enabled: bool,
        /// Fold size in pixels
        size: f32,
        /// Accent color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Fold opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure frosted window border effect
    SetFrostedBorder {
        enabled: bool,
        /// Border width in pixels
        width: f32,
        /// Border opacity (0.0-1.0)
        opacity: f32,
        /// Border color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
    },
    /// Configure line number pulse on cursor line
    SetLineNumberPulse {
        enabled: bool,
        /// Pulse color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Intensity (0.0-1.0)
        intensity: f32,
        /// Pulse cycle speed in milliseconds
        cycle_ms: u32,
    },
    /// Configure window breathing border animation
    SetBreathingBorder {
        enabled: bool,
        /// Border color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Minimum opacity (0.0-1.0)
        min_opacity: f32,
        /// Maximum opacity (0.0-1.0)
        max_opacity: f32,
        /// Full cycle duration in milliseconds
        cycle_ms: u32,
    },
    /// Configure window scanline (CRT) effect
    SetScanlines {
        enabled: bool,
        /// Spacing between scanlines in pixels
        spacing: u32,
        /// Scanline opacity (0.0-1.0)
        opacity: f32,
        /// Scanline color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
    },
    /// Configure cursor comet tail effect
    SetCursorComet {
        enabled: bool,
        /// Trail length (number of ghost copies)
        trail_length: u32,
        /// Fade speed in milliseconds
        fade_ms: u32,
        /// Comet color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Max opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor spotlight/radial gradient effect
    SetCursorSpotlight {
        enabled: bool,
        /// Spotlight radius in pixels
        radius: f32,
        /// Intensity (0.0-1.0)
        intensity: f32,
        /// Spotlight color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
    },
    /// Configure cursor particle trail effect
    SetCursorParticles {
        enabled: bool,
        /// Particle color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Number of particles emitted per cursor move
        count: u32,
        /// Particle lifetime in milliseconds
        lifetime_ms: u32,
        /// Gravity strength (pixels/sec^2, positive = downward)
        gravity: f32,
    },
    /// Configure per-window rounded border
    SetWindowBorderRadius {
        enabled: bool,
        /// Corner radius in pixels
        radius: f32,
        /// Border line width in pixels
        border_width: f32,
        /// Border color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Border opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure typing heat map overlay
    SetTypingHeatMap {
        enabled: bool,
        /// Hot color (sRGB floats) for recent edits
        hot_r: f32,
        hot_g: f32,
        hot_b: f32,
        /// Fade-out duration in milliseconds
        fade_ms: u32,
        /// Maximum opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure smooth theme transition (crossfade on background color change)
    SetThemeTransition {
        enabled: bool,
        /// Crossfade duration in milliseconds
        duration_ms: u32,
    },
    /// Configure cursor click halo effect
    SetClickHalo {
        enabled: bool,
        /// Halo color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Duration of expanding animation in milliseconds
        duration_ms: u32,
        /// Maximum radius of the halo in pixels
        max_radius: f32,
    },
    /// Configure scroll velocity fade overlay
    SetScrollVelocityFade {
        enabled: bool,
        /// Maximum overlay opacity at peak velocity (0.0-1.0)
        max_opacity: f32,
        /// Fade-out duration in milliseconds after scroll stops
        fade_ms: u32,
    },
    /// Configure mini-buffer completion highlight glow
    SetMinibufferHighlight {
        enabled: bool,
        /// Highlight color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Glow opacity 0.0-1.0
        opacity: f32,
    },
    /// Configure smooth window padding transition on resize
    SetResizePadding {
        enabled: bool,
        /// Transition duration in milliseconds
        duration_ms: u32,
        /// Maximum extra padding in pixels at start of transition
        max_padding: f32,
    },
    /// Configure cursor error pulse (brief color flash on bell)
    SetCursorErrorPulse {
        enabled: bool,
        /// Pulse color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Pulse duration in milliseconds
        duration_ms: u32,
    },
    /// Configure line wrap indicator overlay
    SetWrapIndicator {
        enabled: bool,
        /// Color (sRGB floats)
        r: f32,
        g: f32,
        b: f32,
        /// Opacity 0.0-1.0
        opacity: f32,
    },
    /// Configure per-window scroll momentum indicator
    SetScrollMomentum {
        enabled: bool,
        /// Fade-out duration in milliseconds
        fade_ms: u32,
        /// Bar width in pixels
        width: f32,
    },
    /// Configure matrix/digital rain effect
    SetMatrixRain {
        enabled: bool,
        /// Drop color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of columns
        column_count: u32,
        /// Fall speed in pixels/sec
        speed: f32,
        /// Drop opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor elastic snap animation
    SetCursorElasticSnap {
        enabled: bool,
        /// Overshoot amount (0.0-1.0, fraction of distance to overshoot)
        overshoot: f32,
        /// Duration in milliseconds
        duration_ms: u32,
    },
    /// Configure window frost/ice border effect
    SetFrostBorder {
        enabled: bool,
        /// Ice color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Border width in pixels
        width: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor afterimage ghost effect
    SetCursorGhost {
        enabled: bool,
        /// Ghost color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of ghost copies
        count: u32,
        /// Ghost fade duration in ms
        fade_ms: u32,
        /// Drift speed in pixels/sec
        drift: f32,
        /// Max opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure window edge glow on scroll boundaries
    SetEdgeGlow {
        enabled: bool,
        /// Glow color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Glow height in pixels
        height: f32,
        /// Peak opacity (0.0-1.0)
        opacity: f32,
        /// Fade duration in milliseconds
        fade_ms: u32,
    },
    /// Configure window rain/drip ambient effect
    SetRainEffect {
        enabled: bool,
        /// Drop color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of simultaneous drops
        drop_count: u32,
        /// Drop fall speed in pixels/sec
        speed: f32,
        /// Drop opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor ripple wave effect on keystroke
    SetCursorRippleWave {
        enabled: bool,
        /// Ripple color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of concentric rings
        ring_count: u32,
        /// Max radius in pixels
        max_radius: f32,
        /// Duration in milliseconds
        duration_ms: u32,
        /// Peak ring opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure window aurora/northern lights effect
    SetAurora {
        enabled: bool,
        /// Primary color (sRGB floats)
        r1: f32, g1: f32, b1: f32,
        /// Secondary color (sRGB floats)
        r2: f32, g2: f32, b2: f32,
        /// Band height in pixels
        height: f32,
        /// Animation speed multiplier (1.0 = default)
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure heat distortion effect
    SetHeatDistortion {
        enabled: bool,
        /// Intensity of distortion (0.0-1.0)
        intensity: f32,
        /// Speed of shimmer animation
        speed: f32,
        /// Width of affected edge region in pixels
        edge_width: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor lighthouse beam effect
    SetCursorLighthouse {
        enabled: bool,
        /// Beam color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Beam angular width in degrees
        beam_width: f32,
        /// Rotation speed (revolutions per second)
        rotation_speed: f32,
        /// Beam length in pixels
        beam_length: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure neon border effect
    SetNeonBorder {
        enabled: bool,
        /// Neon tube color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Glow intensity (0.0-1.0)
        intensity: f32,
        /// Flicker amount (0.0-1.0, 0=no flicker)
        flicker: f32,
        /// Border thickness in pixels
        thickness: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor sonar ping effect
    SetCursorSonarPing {
        enabled: bool,
        /// Ring color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of concentric rings per ping
        ring_count: u32,
        /// Max expansion radius in pixels
        max_radius: f32,
        /// Duration of each ping in ms
        duration_ms: u32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure lightning bolt effect
    SetLightningBolt {
        enabled: bool,
        /// Bolt color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Frequency of bolts per second (0.5-5.0)
        frequency: f32,
        /// Bolt intensity/brightness (0.0-1.0)
        intensity: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor orbit particles effect
    SetCursorOrbitParticles {
        enabled: bool,
        /// Particle color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of orbiting particles
        particle_count: u32,
        /// Orbit radius in pixels
        orbit_radius: f32,
        /// Orbit speed (revolutions per second)
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure plasma border effect
    SetPlasmaBorder {
        enabled: bool,
        /// Primary color (sRGB floats)
        r1: f32, g1: f32, b1: f32,
        /// Secondary color (sRGB floats)
        r2: f32, g2: f32, b2: f32,
        /// Border width in pixels
        width: f32,
        /// Animation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor heartbeat pulse effect
    SetCursorHeartbeat {
        enabled: bool,
        /// Pulse color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Pulse rate (beats per minute)
        bpm: f32,
        /// Max pulse radius in pixels
        max_radius: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure warp/distortion grid effect
    SetWarpGrid {
        enabled: bool,
        /// Grid color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Grid cell density (cells across width)
        density: u32,
        /// Distortion amplitude in pixels
        amplitude: f32,
        /// Animation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor DNA helix trail effect
    SetCursorDnaHelix {
        enabled: bool,
        /// Strand 1 color (sRGB floats)
        r1: f32, g1: f32, b1: f32,
        /// Strand 2 color (sRGB floats)
        r2: f32, g2: f32, b2: f32,
        /// Helix radius in pixels
        radius: f32,
        /// Rotation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure prism/rainbow edge effect
    SetPrismEdge {
        enabled: bool,
        /// Spectrum band width in pixels
        width: f32,
        /// Animation speed multiplier
        speed: f32,
        /// Saturation (0.0-1.0)
        saturation: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor pendulum swing effect
    SetCursorPendulum {
        enabled: bool,
        /// Swing arc color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Arc length in pixels
        arc_length: f32,
        /// Damping factor (0.0-1.0, higher = faster decay)
        damping: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure hex grid overlay effect
    SetHexGrid {
        enabled: bool,
        /// Grid line color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Hex cell size in pixels
        cell_size: f32,
        /// Pulse animation speed multiplier
        pulse_speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor sparkle burst effect
    SetCursorSparkleBurst {
        enabled: bool,
        /// Sparkle color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of sparkle particles per burst
        particle_count: u32,
        /// Burst radius in pixels
        burst_radius: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure circuit board trace effect
    SetCircuitTrace {
        enabled: bool,
        /// Trace color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Trace width in pixels
        trace_width: f32,
        /// Animation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor compass rose effect
    SetCursorCompass {
        enabled: bool,
        /// Compass color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Compass size in pixels
        size: f32,
        /// Rotation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure topographic contour effect
    SetTopoContour {
        enabled: bool,
        /// Contour line color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Line spacing in pixels
        spacing: f32,
        /// Animation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor metronome tick effect
    SetCursorMetronome {
        enabled: bool,
        /// Tick color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Tick height in pixels
        tick_height: f32,
        /// Fade duration in ms
        fade_ms: u32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure constellation overlay effect
    SetConstellation {
        enabled: bool,
        /// Star color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of stars
        star_count: u32,
        /// Connection distance in pixels
        connect_dist: f32,
        /// Twinkle speed multiplier
        twinkle_speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor radar sweep effect
    SetCursorRadar {
        enabled: bool,
        /// Sweep color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Sweep radius in pixels
        radius: f32,
        /// Rotation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure kaleidoscope overlay effect
    SetKaleidoscope {
        enabled: bool,
        /// Kaleidoscope color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of mirror segments
        segments: u32,
        /// Rotation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor ripple ring effect
    SetCursorRippleRing {
        enabled: bool,
        /// Ring color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Max ring radius in pixels
        max_radius: f32,
        /// Number of concentric rings
        ring_count: u32,
        /// Expansion speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure noise field overlay effect
    SetNoiseField {
        enabled: bool,
        /// Noise color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Noise scale (higher = coarser)
        scale: f32,
        /// Animation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor scope effect
    SetCursorScope {
        enabled: bool,
        /// Scope color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Line thickness in pixels
        thickness: f32,
        /// Gap around cursor in pixels
        gap: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure spiral vortex overlay effect
    SetSpiralVortex {
        enabled: bool,
        /// Spiral color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Number of spiral arms
        arms: u32,
        /// Rotation speed multiplier
        speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor shockwave effect
    SetCursorShockwave {
        enabled: bool,
        /// Shockwave color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Max expansion radius in pixels
        radius: f32,
        /// Decay speed multiplier
        decay: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure diamond lattice overlay effect
    SetDiamondLattice {
        enabled: bool,
        /// Lattice color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Diamond cell size in pixels
        cell_size: f32,
        /// Shimmer animation speed
        shimmer_speed: f32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure cursor gravity well effect
    SetCursorGravityWell {
        enabled: bool,
        /// Field color (sRGB floats)
        r: f32, g: f32, b: f32,
        /// Field radius in pixels
        field_radius: f32,
        /// Number of field lines
        line_count: u32,
        /// Opacity (0.0-1.0)
        opacity: f32,
    },
    /// Configure wave interference overlay effect
    SetWaveInterference {
        enabled: bool,
        r: f32, g: f32, b: f32,
        wavelength: f32,
        source_count: u32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor portal effect
    SetCursorPortal {
        enabled: bool,
        r: f32, g: f32, b: f32,
        radius: f32,
        speed: f32,
        opacity: f32,
    },
    /// Configure chevron pattern overlay effect
    SetChevronPattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        spacing: f32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor bubble effect
    SetCursorBubble {
        enabled: bool,
        r: f32, g: f32, b: f32,
        count: u32,
        rise_speed: f32,
        opacity: f32,
    },
    /// Configure sunburst pattern overlay effect
    SetSunburstPattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        ray_count: u32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor firework effect
    SetCursorFirework {
        enabled: bool,
        r: f32, g: f32, b: f32,
        particle_count: u32,
        burst_radius: f32,
        opacity: f32,
    },
    /// Configure honeycomb dissolve overlay effect
    SetHoneycombDissolve {
        enabled: bool,
        r: f32, g: f32, b: f32,
        cell_size: f32,
        dissolve_speed: f32,
        opacity: f32,
    },
    /// Configure cursor tornado effect
    SetCursorTornado {
        enabled: bool,
        r: f32, g: f32, b: f32,
        radius: f32,
        particle_count: u32,
        opacity: f32,
    },
    /// Configure moiré pattern overlay effect
    SetMoirePattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        line_spacing: f32,
        angle_offset: f32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor lightning effect
    SetCursorLightning {
        enabled: bool,
        r: f32, g: f32, b: f32,
        bolt_count: u32,
        max_length: f32,
        opacity: f32,
    },
    /// Configure dot matrix overlay effect
    SetDotMatrix {
        enabled: bool,
        r: f32, g: f32, b: f32,
        dot_spacing: f32,
        pulse_speed: f32,
        opacity: f32,
    },
    /// Configure cursor snowflake effect
    SetCursorSnowflake {
        enabled: bool,
        r: f32, g: f32, b: f32,
        count: u32,
        fall_speed: f32,
        opacity: f32,
    },
    /// Configure concentric rings overlay effect
    SetConcentricRings {
        enabled: bool,
        r: f32, g: f32, b: f32,
        ring_spacing: f32,
        expansion_speed: f32,
        opacity: f32,
    },
    /// Configure cursor flame effect
    SetCursorFlame {
        enabled: bool,
        r: f32, g: f32, b: f32,
        particle_count: u32,
        height: f32,
        opacity: f32,
    },
    /// Configure zigzag pattern overlay effect
    SetZigzagPattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        amplitude: f32,
        frequency: f32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor crystal effect
    SetCursorCrystal {
        enabled: bool,
        r: f32, g: f32, b: f32,
        facet_count: u32,
        radius: f32,
        opacity: f32,
    },
    /// Configure basket weave overlay effect
    SetBasketWeave {
        enabled: bool,
        r: f32, g: f32, b: f32,
        strip_width: f32,
        strip_spacing: f32,
        opacity: f32,
    },
    /// Configure cursor sparkler effect
    SetCursorSparkler {
        enabled: bool,
        r: f32, g: f32, b: f32,
        spark_count: u32,
        burn_speed: f32,
        opacity: f32,
    },
    /// Configure fish scale overlay effect
    SetFishScale {
        enabled: bool,
        r: f32, g: f32, b: f32,
        scale_size: f32,
        row_offset: f32,
        opacity: f32,
    },
    /// Configure cursor plasma ball effect
    SetCursorPlasmaBall {
        enabled: bool,
        r: f32, g: f32, b: f32,
        tendril_count: u32,
        arc_speed: f32,
        opacity: f32,
    },
    /// Configure trefoil knot overlay effect
    SetTrefoilKnot {
        enabled: bool,
        r: f32, g: f32, b: f32,
        knot_size: f32,
        rotation_speed: f32,
        opacity: f32,
    },
    /// Configure cursor quill pen effect
    SetCursorQuillPen {
        enabled: bool,
        r: f32, g: f32, b: f32,
        trail_length: u32,
        ink_speed: f32,
        opacity: f32,
    },
    /// Configure herringbone pattern overlay effect
    SetHerringbonePattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        tile_width: f32,
        tile_height: f32,
        opacity: f32,
    },
    /// Configure cursor aurora borealis effect
    SetCursorAuroraBorealis {
        enabled: bool,
        r: f32, g: f32, b: f32,
        band_count: u32,
        shimmer_speed: f32,
        opacity: f32,
    },
    /// Configure target reticle overlay effect
    SetTargetReticle {
        enabled: bool,
        r: f32, g: f32, b: f32,
        ring_count: u32,
        pulse_speed: f32,
        opacity: f32,
    },
    /// Configure cursor feather effect
    SetCursorFeather {
        enabled: bool,
        r: f32, g: f32, b: f32,
        feather_count: u32,
        drift_speed: f32,
        opacity: f32,
    },
    /// Configure plaid pattern overlay effect
    SetPlaidPattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        band_width: f32,
        band_spacing: f32,
        opacity: f32,
    },
    /// Configure cursor stardust effect
    SetCursorStardust {
        enabled: bool,
        r: f32, g: f32, b: f32,
        particle_count: u32,
        fall_speed: f32,
        opacity: f32,
    },
    /// Configure brick wall overlay effect
    SetBrickWall {
        enabled: bool,
        r: f32, g: f32, b: f32,
        brick_width: f32,
        brick_height: f32,
        opacity: f32,
    },
    /// Configure cursor compass needle effect
    SetCursorCompassNeedle {
        enabled: bool,
        r: f32, g: f32, b: f32,
        needle_length: f32,
        spin_speed: f32,
        opacity: f32,
    },
    /// Configure sine wave overlay effect
    SetSineWave {
        enabled: bool,
        r: f32, g: f32, b: f32,
        amplitude: f32,
        wavelength: f32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor galaxy effect
    SetCursorGalaxy {
        enabled: bool,
        r: f32, g: f32, b: f32,
        star_count: u32,
        radius: f32,
        opacity: f32,
    },
    /// Configure rotating gear overlay effect
    SetRotatingGear {
        enabled: bool,
        r: f32, g: f32, b: f32,
        gear_size: f32,
        rotation_speed: f32,
        opacity: f32,
    },
    /// Configure cursor prism effect
    SetCursorPrism {
        enabled: bool,
        r: f32, g: f32, b: f32,
        ray_count: u32,
        spread: f32,
        opacity: f32,
    },
    /// Configure crosshatch pattern overlay effect
    SetCrosshatchPattern {
        enabled: bool,
        r: f32, g: f32, b: f32,
        line_spacing: f32,
        angle: f32,
        speed: f32,
        opacity: f32,
    },
    /// Configure cursor moth effect
    SetCursorMoth {
        enabled: bool,
        r: f32, g: f32, b: f32,
        moth_count: u32,
        wing_size: f32,
        opacity: f32,
    },
    /// Configure background pattern
    SetBackgroundPattern {
        /// 0=none, 1=dots, 2=grid, 3=crosshatch
        style: u32,
        /// Spacing between pattern elements in pixels
        spacing: f32,
        /// Pattern color as sRGB 0.0-1.0
        r: f32, g: f32, b: f32,
        /// Pattern opacity 0.0-1.0
        opacity: f32,
    },
}

/// Wakeup pipe for signaling Emacs from render thread
pub struct WakeupPipe {
    read_fd: RawFd,
    write_fd: RawFd,
}

impl WakeupPipe {
    /// Create a new wakeup pipe
    pub fn new() -> std::io::Result<Self> {
        let (read, write) = os_pipe::pipe()?;
        use std::os::unix::io::IntoRawFd;
        Ok(Self {
            read_fd: read.into_raw_fd(),
            write_fd: write.into_raw_fd(),
        })
    }

    /// Get the read fd for Emacs to select() on
    pub fn read_fd(&self) -> RawFd {
        self.read_fd
    }

    /// Signal Emacs to wake up (called from render thread)
    pub fn wake(&self) {
        unsafe {
            libc::write(self.write_fd, [1u8].as_ptr() as *const _, 1);
        }
    }

    /// Clear the wakeup signal (called from Emacs thread)
    pub fn clear(&self) {
        let mut buf = [0u8; 64];
        unsafe {
            // Non-blocking read to drain the pipe
            let flags = libc::fcntl(self.read_fd, libc::F_GETFL);
            libc::fcntl(self.read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            while libc::read(self.read_fd, buf.as_mut_ptr() as *mut _, buf.len()) > 0 {}
            libc::fcntl(self.read_fd, libc::F_SETFL, flags);
        }
    }
}

impl Drop for WakeupPipe {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.read_fd);
            libc::close(self.write_fd);
        }
    }
}

/// Channel capacities
// Frame channel: unbounded so try_send never drops frames.
// The render thread drains all queued frames and keeps only the latest
// (see poll_frame()), so memory stays bounded in practice.
const INPUT_CHANNEL_CAPACITY: usize = 256;
const COMMAND_CHANNEL_CAPACITY: usize = 64;

/// Communication channels between threads
pub struct ThreadComms {
    /// Frame glyphs: Emacs → Render
    pub frame_tx: Sender<FrameGlyphBuffer>,
    pub frame_rx: Receiver<FrameGlyphBuffer>,

    /// Commands: Emacs → Render
    pub cmd_tx: Sender<RenderCommand>,
    pub cmd_rx: Receiver<RenderCommand>,

    /// Input events: Render → Emacs
    pub input_tx: Sender<InputEvent>,
    pub input_rx: Receiver<InputEvent>,

    /// Wakeup pipe: Render → Emacs
    pub wakeup: WakeupPipe,
}

impl ThreadComms {
    /// Create new thread communication channels
    pub fn new() -> std::io::Result<Self> {
        let (frame_tx, frame_rx) = unbounded();
        let (cmd_tx, cmd_rx) = bounded(COMMAND_CHANNEL_CAPACITY);
        let (input_tx, input_rx) = bounded(INPUT_CHANNEL_CAPACITY);
        let wakeup = WakeupPipe::new()?;

        Ok(Self {
            frame_tx,
            frame_rx,
            cmd_tx,
            cmd_rx,
            input_tx,
            input_rx,
            wakeup,
        })
    }

    /// Split into Emacs-side and Render-side handles
    pub fn split(self) -> (EmacsComms, RenderComms) {
        let emacs = EmacsComms {
            frame_tx: self.frame_tx,
            cmd_tx: self.cmd_tx,
            input_rx: self.input_rx,
            wakeup_read_fd: self.wakeup.read_fd(),
            wakeup_clear: WakeupClear { fd: self.wakeup.read_fd },
        };

        let render = RenderComms {
            frame_rx: self.frame_rx,
            cmd_rx: self.cmd_rx,
            input_tx: self.input_tx,
            wakeup: self.wakeup,
        };

        (emacs, render)
    }
}

/// Emacs thread communication handle
pub struct EmacsComms {
    pub frame_tx: Sender<FrameGlyphBuffer>,
    pub cmd_tx: Sender<RenderCommand>,
    pub input_rx: Receiver<InputEvent>,
    pub wakeup_read_fd: RawFd,
    pub wakeup_clear: WakeupClear,
}

/// Handle for clearing wakeup pipe
pub struct WakeupClear {
    fd: RawFd,
}

impl WakeupClear {
    pub fn clear(&self) {
        let mut buf = [0u8; 64];
        unsafe {
            let flags = libc::fcntl(self.fd, libc::F_GETFL);
            libc::fcntl(self.fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            while libc::read(self.fd, buf.as_mut_ptr() as *mut _, buf.len()) > 0 {}
            libc::fcntl(self.fd, libc::F_SETFL, flags);
        }
    }
}

/// Render thread communication handle
pub struct RenderComms {
    pub frame_rx: Receiver<FrameGlyphBuffer>,
    pub cmd_rx: Receiver<RenderCommand>,
    pub input_tx: Sender<InputEvent>,
    pub wakeup: WakeupPipe,
}

impl RenderComms {
    /// Send input event to Emacs and wake it up
    pub fn send_input(&self, event: InputEvent) {
        if self.input_tx.try_send(event).is_ok() {
            self.wakeup.wake();
        }
    }
}
