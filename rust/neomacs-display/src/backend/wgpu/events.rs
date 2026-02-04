//! Event types for winit → Emacs communication.

/// Input event kinds matching Emacs event types.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    KeyPress = 1,
    KeyRelease = 2,
    MousePress = 3,
    MouseRelease = 4,
    MouseMove = 5,
    Scroll = 6,
    Resize = 7,
    CloseRequest = 8,
    FocusIn = 9,
    FocusOut = 10,
}

/// Modifier flags matching Emacs.
pub const NEOMACS_SHIFT_MASK: u32 = 1 << 0;
pub const NEOMACS_CTRL_MASK: u32 = 1 << 1;
pub const NEOMACS_META_MASK: u32 = 1 << 2;
pub const NEOMACS_SUPER_MASK: u32 = 1 << 3;

/// Event kind constants for FFI.
pub const NEOMACS_EVENT_KEY_PRESS: u32 = EventKind::KeyPress as u32;
pub const NEOMACS_EVENT_KEY_RELEASE: u32 = EventKind::KeyRelease as u32;
pub const NEOMACS_EVENT_BUTTON_PRESS: u32 = EventKind::MousePress as u32;
pub const NEOMACS_EVENT_BUTTON_RELEASE: u32 = EventKind::MouseRelease as u32;
pub const NEOMACS_EVENT_MOUSE_MOVE: u32 = EventKind::MouseMove as u32;
pub const NEOMACS_EVENT_SCROLL: u32 = EventKind::Scroll as u32;
pub const NEOMACS_EVENT_RESIZE: u32 = EventKind::Resize as u32;
pub const NEOMACS_EVENT_CLOSE: u32 = EventKind::CloseRequest as u32;
pub const NEOMACS_EVENT_FOCUS_IN: u32 = EventKind::FocusIn as u32;
pub const NEOMACS_EVENT_FOCUS_OUT: u32 = EventKind::FocusOut as u32;

/// Input event structure passed to C.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct NeomacsInputEvent {
    pub kind: u32,
    pub window_id: u32,
    pub timestamp: u64,
    pub x: i32,
    pub y: i32,
    pub keycode: u32,
    pub keysym: u32,
    pub modifiers: u32,
    pub button: u32,
    pub scroll_delta_x: f32,
    pub scroll_delta_y: f32,
    pub width: u32,
    pub height: u32,
}

impl Default for NeomacsInputEvent {
    fn default() -> Self {
        Self {
            kind: 0,
            window_id: 0,
            timestamp: 0,
            x: 0,
            y: 0,
            keycode: 0,
            keysym: 0,
            modifiers: 0,
            button: 0,
            scroll_delta_x: 0.0,
            scroll_delta_y: 0.0,
            width: 0,
            height: 0,
        }
    }
}
