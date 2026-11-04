//! Display vocabulary shared below the evaluator (stage-4 W1 E2).
//!
//! These are pure ids/constants that both the display layer and the evaluator
//! name. They live in the lowest crate that can name them so that
//! `neovm-eval`'s Context fields and signatures need no display-crate type.

/// Opaque frame identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameId(pub u64);

/// Keep frame and window numeric domains disjoint while both are represented
/// as Lisp integers.
pub const FRAME_ID_BASE: u64 = 1 << 32;

use crate::value::Value;
use strum::IntoStaticStr;

/// GNU's two frame-argument domains, as a closed set.
///
/// The frame decoders differ only in whether a dead frame is admitted, and --
/// like the window ones -- each fixes the predicate its rejection reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
pub enum FrameDomain {
    /// `decode_live_frame` / `CHECK_LIVE_FRAME`.
    #[strum(serialize = "frame-live-p")]
    Live,
    /// `decode_any_frame` / `CHECK_FRAME`: any frame object.
    #[strum(serialize = "framep")]
    Any,
}

impl FrameDomain {
    /// The symbol a rejection reports, derived from the variant.
    pub fn predicate(self) -> &'static str {
        self.into()
    }
}

/// GNU-compatible lifecycle state for a live frame.
///
/// GNU stores `visible` and `iconified` as separate bits, but only these three
/// public states are valid.  Keeping the sum type here prevents callers from
/// manufacturing the impossible `visible && iconified` combination and makes
/// every projection choose its intended semantics explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrameVisibility {
    #[default]
    Visible,
    Iconified,
    Invisible,
}

impl FrameVisibility {
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }

    pub const fn is_visible_or_iconified(self) -> bool {
        matches!(self, Self::Visible | Self::Iconified)
    }

    pub const fn is_invisible(self) -> bool {
        matches!(self, Self::Invisible)
    }

    pub fn from_lisp_value(value: Value) -> Self {
        if value.is_nil() {
            Self::Invisible
        } else if value == Value::symbol("icon") {
            Self::Iconified
        } else {
            Self::Visible
        }
    }

    pub fn lisp_value(self) -> Value {
        match self {
            Self::Visible => Value::T,
            Self::Iconified => Value::symbol("icon"),
            Self::Invisible => Value::NIL,
        }
    }
}

/// Whether hidden frames participate in a frame-tree query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFrameVisibility {
    VisibleOnly,
    IncludeInvisible,
}

impl RenderFrameVisibility {
    pub const fn includes_invisible(self) -> bool {
        matches!(self, Self::IncludeInvisible)
    }
}

/// A redisplay epoch stamp.
///
/// `NonZeroU32` makes the reserved value unrepresentable, so
/// `Option<ChangeStamp>` IS GNU's "0 or a stamp" in one word and the guard
/// cannot be dropped by accident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChangeStamp(std::num::NonZeroU32);

impl ChangeStamp {
    /// The first epoch.
    pub const FIRST: Self = Self(std::num::NonZeroU32::new(1).unwrap());

    /// The next epoch, wrapping past zero exactly as GNU's guard does.
    #[must_use]
    pub fn next(self) -> Self {
        Self(std::num::NonZeroU32::new(self.0.get().wrapping_add(1)).unwrap_or(Self::FIRST.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // side by side
    Vertical,   // stacked
}

/// Host-local reference to an exact selected font entity. Handles are never
/// reused during a host's lifetime; the host validates every lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontEntityHandle(std::num::NonZeroU32);

impl FontEntityHandle {
    pub fn new(value: u32) -> Option<Self> {
        std::num::NonZeroU32::new(value).map(Self)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// The cache key for shaped gstrings: the header's font family, pixel size
/// (bit-exact), and the run's characters. Two gstrings with the same key
/// shape identically, so one cache entry serves both.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GstringShapeCacheKey {
    pub family: String,
    pub pixel_size_bits: u64,
    pub chars: Vec<i64>,
}
