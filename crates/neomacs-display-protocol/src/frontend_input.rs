//! Input vocabulary that crosses from a frontend to the evaluator.
//!
//! Every frontend (GUI, TTY, a display link) produces these values and the
//! evaluator consumes them; they are the input half of the display protocol.

/// Toolkit input identity. Text and keysyms overlap numerically, so their
/// provenance must survive transport (U+FF0D is text; XK_Return is a key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FrontendKey {
    /// A Unicode scalar supplied as text by the toolkit or input method.
    Character(char),
    /// A key identity in the X11/native keysym domain, not a code point.
    Keysym(u32),
}

/// A frontend's monotonic clock in milliseconds since its session started
/// (DESIGN §6.15). One domain per session incarnation; the evaluator only
/// takes differences of times from one session and never compares clocks
/// across hosts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DisplayTime(u64);

// A display time is stamped by a platform adapter, never fabricated, and two
// times are related only through `since` (DESIGN §6.15; the "time is a
// parameter" rule of docs/design/display-crate-layout.md).
static_assertions::assert_not_impl_any!(DisplayTime: Default, PartialOrd, Ord);

impl DisplayTime {
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Milliseconds from `earlier` to `self`; `None` when `earlier` is later.
    #[must_use]
    pub const fn since(self, earlier: Self) -> Option<u64> {
        self.0.checked_sub(earlier.0)
    }
}

/// How a pointer position relates to the immutable presentation on screen.
///
/// `Unpresented` is explicit for exposed/native surface area. A producer cannot
/// accidentally omit presentation state from a pointer action.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PointerTarget {
    Presented {
        presentation: u64,
        hit: Option<crate::PresentedHit>,
    },
    Unpresented,
}

/// Identities for a native key that has no keysym.
///
/// macOS, Windows, Android and OpenHarmony hand the frontend a scancode or a
/// virtual-key code, not a keysym. X11's keysym space stops below 0x20000000
/// (the Unicode range tops out at 0x0110ffff and the vendor bit at bit 28),
/// so these bands cannot collide with a real keysym, and a key from one of
/// those platforms keeps a stable identity instead of being dropped. The
/// frontend encodes with the functions below; the evaluator names the key
/// from the same bands.
pub mod native_key {
    pub const BAND_MASK: u32 = 0x0000_ffff;
    pub const MACOS_BASE: u32 = 0x2000_0000;
    pub const WINDOWS_BASE: u32 = 0x2100_0000;
    pub const ANDROID_BASE: u32 = 0x2200_0000;
    pub const OHOS_BASE: u32 = 0x2300_0000;

    /// Identity for a macOS key that winit could not name (`NativeKey::MacOS`,
    /// an Apple scancode).
    #[must_use]
    pub fn macos(scancode: u16) -> u32 {
        MACOS_BASE | (u32::from(scancode) & BAND_MASK)
    }

    /// Identity for a Windows key that winit could not name
    /// (`NativeKey::Windows`, a virtual-key code).
    #[must_use]
    pub fn windows(virtual_key: u16) -> u32 {
        WINDOWS_BASE | (u32::from(virtual_key) & BAND_MASK)
    }

    /// Identity for an Android key that winit could not name
    /// (`NativeKey::Android`, a keycode).
    #[must_use]
    pub fn android(keycode: u32) -> u32 {
        ANDROID_BASE | (keycode & BAND_MASK)
    }

    /// Identity for an OpenHarmony key that winit could not name.
    #[must_use]
    pub fn ohos(keycode: u32) -> u32 {
        OHOS_BASE | (keycode & BAND_MASK)
    }
}
