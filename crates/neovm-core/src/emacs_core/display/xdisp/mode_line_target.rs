//! Compile-time admission for the common native percent/append path.
//!
//! Only stored STRING has an ordinal capability. DISPLAY contains neither the
//! capability nor an enum alternative that could contain one.

use super::{CharLen, DisplayColumn, Flow, ModeLineNativeFieldWidth, ModeLineStringTarget};

mod sealed {
    pub trait Sealed {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PointFieldPolicy {
    Live,
    Suppressed,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum PropertyApplication {
    OverlayThenDisplayWidths,
    MaterializeThenOverlay,
}

pub(super) trait TargetPolicy: sealed::Sealed + Copy {
    const PROPERTY_APPLICATION: PropertyApplication;

    fn point_fields(&self) -> PointFieldPolicy;
    fn dash_string(&self, used: CharLen, width: ModeLineNativeFieldWidth) -> String;
}

#[derive(Clone, Copy, Debug)]
/// Native admission owns only the row width; it can be sent/shared without
/// evaluator state. The enclosing formatter still owns its mutable metadata.
pub(super) struct DisplayTarget {
    columns: DisplayColumn,
}

impl DisplayTarget {
    pub(super) fn new(columns: DisplayColumn) -> Self {
        Self { columns }
    }
}

impl sealed::Sealed for DisplayTarget {}

impl TargetPolicy for DisplayTarget {
    const PROPERTY_APPLICATION: PropertyApplication = PropertyApplication::OverlayThenDisplayWidths;

    fn point_fields(&self) -> PointFieldPolicy {
        PointFieldPolicy::Live
    }

    fn dash_string(&self, used: CharLen, _width: ModeLineNativeFieldWidth) -> String {
        if self.columns.get() > used.get() {
            "-".repeat(self.columns.get() - used.get())
        } else {
            "--".to_owned()
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// Stored output borrows its admitted same-thread formatter extent. No handle
/// survives that extent or crosses mutators; static assertions pin this rule.
pub(super) enum StoredTarget<'scope> {
    String(ModeLineStringTarget<'scope>),
    #[default]
    NoProperties,
    Title {
        frame_columns: DisplayColumn,
    },
}

impl StoredTarget<'_> {
    pub(super) fn enter_element(&self) -> Result<(), Flow> {
        match self {
            Self::String(owner) => owner.advance().map(|_| ()),
            Self::NoProperties | Self::Title { .. } => Ok(()),
        }
    }
}

impl sealed::Sealed for StoredTarget<'_> {}

impl TargetPolicy for StoredTarget<'_> {
    const PROPERTY_APPLICATION: PropertyApplication = PropertyApplication::MaterializeThenOverlay;

    fn point_fields(&self) -> PointFieldPolicy {
        match self {
            Self::Title { .. } => PointFieldPolicy::Suppressed,
            Self::String(_) | Self::NoProperties => PointFieldPolicy::Live,
        }
    }

    fn dash_string(&self, _used: CharLen, width: ModeLineNativeFieldWidth) -> String {
        match self {
            Self::String(_) | Self::NoProperties => "--".to_owned(),
            Self::Title { frame_columns } => {
                // GNU frame.h FRAME_MESSAGE_BUF_SIZE and xdisp.c decode_mode_spec:
                // the fixed dash string is 140 ASCII bytes/characters; zero or
                // >141 minimum width requests the frame message fallback.
                const DASH_LITERAL_LENGTH: CharLen = CharLen::new(140);
                let length = if width.is_empty()
                    || width.exceeds_char_len(CharLen::new(DASH_LITERAL_LENGTH.get() + 1))
                {
                    frame_columns.get().saturating_mul(4).saturating_sub(1)
                } else {
                    DASH_LITERAL_LENGTH.get()
                };
                "-".repeat(length)
            }
        }
    }
}

static_assertions::assert_eq_size!(DisplayTarget, DisplayColumn);
static_assertions::assert_impl_all!(DisplayTarget: Copy, Send, Sync);
static_assertions::assert_impl_all!(StoredTarget<'static>: Copy);
static_assertions::assert_not_impl_any!(StoredTarget<'static>: Send, Sync);
const _: () = assert!(!std::mem::needs_drop::<DisplayTarget>());
const _: () = assert!(!std::mem::needs_drop::<StoredTarget<'static>>());
