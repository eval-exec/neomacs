//! Thread communication infrastructure for two-thread architecture.
//!
//! Provides lock-free channels and wakeup mechanism between Emacs and render threads.

use crossbeam_channel::{bounded, Receiver, Sender};
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
}

/// Command from Emacs to render thread
#[derive(Debug)]
pub enum RenderCommand {
    /// Shutdown the render thread
    Shutdown,
    /// Create a WebKit view
    WebKitCreate { id: u32, width: u32, height: u32 },
    /// Load URL in WebKit view
    WebKitLoadUri { id: u32, url: String },
    /// Resize WebKit view
    WebKitResize { id: u32, width: u32, height: u32 },
    /// Destroy WebKit view
    WebKitDestroy { id: u32 },
    /// Create video player
    VideoCreate { id: u32, path: String },
    /// Control video playback
    VideoPlay { id: u32 },
    VideoPause { id: u32 },
    VideoDestroy { id: u32 },
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
const FRAME_CHANNEL_CAPACITY: usize = 2;  // Double buffer
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
        let (frame_tx, frame_rx) = bounded(FRAME_CHANNEL_CAPACITY);
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
