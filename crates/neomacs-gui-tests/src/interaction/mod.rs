//! Native desktop input and independent window/pixel observations.
mod geometry;
pub use geometry::{CaptureMapping, DesktopPoint, DesktopRect};

use serde::{Deserialize, Serialize};
use std::{fmt, path::Path};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(unix)]
pub mod macos;

/// Drop ANSI CSI sequences (ESC [ ... final-byte) from one captured trace
/// line, borrowing the original when it carries none.
///
/// The dumps the GUI harness parses are colorized on hosts whose tools emit
/// SGR (newer libwayland versions do), plain on others, and `tracing`'s
/// colorized stdout splits `name=value` into separate spans.  Every substring
/// probe of a captured log therefore goes through this, on every host: it is
/// deliberately outside the Linux-only `linux` adapter so a platform-neutral
/// test target can use it.
pub fn strip_ansi(line: &str) -> std::borrow::Cow<'_, str> {
    if !line.as_bytes().contains(&0x1b) {
        return std::borrow::Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // A CSI sequence ends at the first byte in 0x40..=0x7e.
            for byte in chars.by_ref() {
                if ('@'..='~').contains(&byte) {
                    break;
                }
            }
        } else {
            out.push(character);
        }
    }
    std::borrow::Cow::Owned(out)
}

#[derive(Debug)]
pub enum DriverError {
    Blocked(Vec<String>),
    InvalidGeometry,
    Protocol(String),
    Io(std::io::Error),
}
impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DriverError {}
impl From<std::io::Error> for DriverError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
pub type Result<T> = std::result::Result<T, DriverError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Escape,
    Enter,
    Down,
    Up,
    Left,
    Right,
    Home,
    End,
    X,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputAction {
    Move { point: DesktopPoint },
    Button { button: Button, down: bool },
    Key { key: Key, down: bool },
    Scroll { lines: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowId {
    pub native: u64,
    pub generation: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObservedWindow {
    pub id: WindowId,
    pub bounds: DesktopRect,
    pub title: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopObservation {
    pub display: DesktopRect,
    pub usable: DesktopRect,
    pub windows: Vec<ObservedWindow>,
}

pub struct Capture {
    pub pixels: image::RgbImage,
    pub mapping: CaptureMapping,
}

/// A driver is ready only after its prerequisites have been checked. Input is
/// real OS input. Observations must not reuse the editor's placement requests.
pub trait DesktopDriver {
    fn fit_window(&mut self) -> Result<DesktopRect>;
    fn input(&mut self, action: InputAction) -> Result<()>;
    fn observe(&mut self) -> Result<DesktopObservation>;
    fn capture(&mut self, path: &Path) -> Result<Capture>;

    fn click(&mut self, point: DesktopPoint, button: Button) -> Result<()> {
        self.input(InputAction::Move { point })?;
        self.input(InputAction::Button { button, down: true })?;
        // Model a human click, leaving the down event time to map a popup and
        // establish its grab. Separate tests should exercise rapid-release races.
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.input(InputAction::Button {
            button,
            down: false,
        })
    }
    fn press(&mut self, key: Key) -> Result<()> {
        self.input(InputAction::Key { key, down: true })?;
        self.input(InputAction::Key { key, down: false })
    }
}
