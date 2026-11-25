//! Frame vocabulary the value layer embeds and the display protocol carries.

/// One evaluator-owned GUI resize request, unique within its frame's lifetime.
/// Native completions carry the frame identity separately; equal dimensions
/// do not make two resize requests interchangeable.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GuiResizeRequestId(core::num::NonZeroU64);

impl GuiResizeRequestId {
    pub const FIRST: Self = Self(core::num::NonZeroU64::MIN);

    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        match core::num::NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Never wrap a frame's serial and reuse the identity of an older request.
    #[must_use]
    pub fn next(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("GUI resize request IDs exhausted"),
        )
    }
}

/// GNU-compatible lifecycle state for a live frame.
///
/// GNU stores `visible` and `iconified` as separate bits, but only these three
/// public states are valid.  Keeping the sum type here prevents callers from
/// manufacturing the impossible `visible && iconified` combination and makes
/// every projection choose its intended semantics explicitly. The Lisp
/// projection lives with the value layer (`neovm_value::display_vocab`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FrameVisibility {
    #[default]
    Visible,
    Iconified,
    Invisible,
}

impl FrameVisibility {
    #[must_use]
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }

    #[must_use]
    pub const fn is_visible_or_iconified(self) -> bool {
        matches!(self, Self::Visible | Self::Iconified)
    }

    #[must_use]
    pub const fn is_invisible(self) -> bool {
        matches!(self, Self::Invisible)
    }
}
