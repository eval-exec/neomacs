//! Thread communication infrastructure for two-thread architecture.
//!
//! Provides lock-free channels and wakeup mechanism between Emacs and render threads.

use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::os::unix::io::RawFd;

use crate::core::frame_glyphs::FrameGlyphBuffer;

/// Emacs window ID used by threaded-mode's single winit window.
pub const THREADED_MAIN_WINDOW_ID: u32 = 1;

/// Input event from render thread to Emacs
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key {
        window_id: u32,
        keysym: u32,
        modifiers: u32,
        pressed: bool,
    },
    MouseButton {
        window_id: u32,
        button: u32,
        x: f32,
        y: f32,
        pressed: bool,
        modifiers: u32,
    },
    MouseMove {
        window_id: u32,
        x: f32,
        y: f32,
        modifiers: u32,
    },
    MouseScroll {
        window_id: u32,
        delta_x: f32,
        delta_y: f32,
        x: f32,
        y: f32,
        modifiers: u32,
    },
    WindowResize {
        window_id: u32,
        width: u32,
        height: u32,
    },
    WindowClose {
        window_id: u32,
    },
    WindowFocus {
        window_id: u32,
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
    /// Create video player
    VideoCreate { id: u32, path: String },
    /// Control video playback
    VideoPlay { id: u32 },
    VideoPause { id: u32 },
    VideoDestroy { id: u32 },
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
        match self.input_tx.try_send(event) {
            Ok(()) => {
                self.wakeup.wake();
            }
            Err(TrySendError::Full(event)) => {
                log::warn!("Dropping input event because channel is full: {:?}", event);
            }
            Err(TrySendError::Disconnected(event)) => {
                log::warn!("Dropping input event because channel is disconnected: {:?}", event);
            }
        }
    }
}
