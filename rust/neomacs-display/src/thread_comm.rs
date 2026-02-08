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
