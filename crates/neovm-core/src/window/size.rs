//! Validated window geometry at the Lisp and tree boundaries.

use thiserror::Error;

/// A nonnegative GNU window pixel size that fits the window.c int domain.
///
/// This immutable value carries no runtime state and can be shared by mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct WindowPixels(i32);

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum WindowSizeError {
    #[error("window pixel size is outside 0..=INT_MAX")]
    OutOfRange,
}

impl TryFrom<i64> for WindowPixels {
    type Error = WindowSizeError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        let value = i32::try_from(value).map_err(|_| WindowSizeError::OutOfRange)?;
        if value < 0 {
            return Err(WindowSizeError::OutOfRange);
        }
        Ok(Self(value))
    }
}

impl From<WindowPixels> for i64 {
    fn from(value: WindowPixels) -> Self {
        i64::from(value.0)
    }
}

impl From<WindowPixels> for crate::tagged::value::Fixnum {
    fn from(value: WindowPixels) -> Self {
        crate::tagged::value::Fixnum::saturating(i64::from(value))
    }
}

const _: () = assert!(i32::MAX as i64 <= crate::emacs_core::value::Value::MOST_POSITIVE_FIXNUM);
const _: () = assert!(size_of::<WindowPixels>() == size_of::<i32>());
static_assertions::assert_impl_all!(WindowPixels: Send, Sync);

/// A GNU `new_total` staging value, measured in character lines or columns.
/// GNU permits signed fixnums here; geometry checks run when resizing applies.
/// This immutable scalar contains no heap state and may cross mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct WindowTotal(crate::tagged::value::Fixnum);

impl TryFrom<i64> for WindowTotal {
    type Error = crate::tagged::value::FixnumRangeError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        crate::tagged::value::Fixnum::try_from(value).map(Self)
    }
}

impl WindowTotal {
    /// GNU `Fset_window_new_total` applies `make_fixnum` after each ADD.
    /// Both inputs fit signed 62-bit payloads, so their sum fits i64. Explicit
    /// signed payload narrowing keeps the stored slot canonical as in GNU.
    pub(crate) fn gnu_added(self, other: Self) -> Self {
        let sum = i64::from(self.0) + i64::from(other.0);
        Self(crate::tagged::value::Fixnum::from_payload_bits(sum as u64))
    }
}

impl From<WindowTotal> for i64 {
    fn from(value: WindowTotal) -> Self {
        i64::from(value.0)
    }
}

impl From<WindowTotal> for crate::tagged::value::Fixnum {
    fn from(value: WindowTotal) -> Self {
        value.0
    }
}

const _: () = assert!(crate::emacs_core::value::Value::MOST_POSITIVE_FIXNUM <= i64::MAX / 2);
const _: () = assert!(crate::emacs_core::value::Value::MOST_NEGATIVE_FIXNUM >= i64::MIN / 2);
const _: () = assert!(size_of::<WindowTotal>() == size_of::<i64>());
static_assertions::assert_impl_all!(WindowTotal: Send, Sync);

/// Two positive, representable extents computed before any tree mutation.
///
/// It is immutable and has no mutator affinity. Signed legacy requests are
/// interpreted in i128 so even the most-negative i64 cannot overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SplitSizes {
    old: WindowPixels,
    new: WindowPixels,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(crate) enum SplitSizeError {
    #[error("Size of new window too small (after split)")]
    NewTooSmall,
    #[error("Resizing old window failed")]
    OldTooSmall,
}

impl SplitSizes {
    /// Rejects a request when either resulting half is empty or exceeds INT_MAX.
    pub(crate) fn new(total: f32, requested: Option<i64>) -> Result<Self, SplitSizeError> {
        let total = i128::from(total.round().max(0.0) as i64);
        let new = match requested {
            Some(n) if n > 0 => i128::from(n),
            Some(n) if n < 0 => total + i128::from(n),
            Some(_) | None => total / 2,
        };
        if new < 1 {
            return Err(SplitSizeError::NewTooSmall);
        }
        let old = total - new;
        if old < 1 {
            return Err(SplitSizeError::OldTooSmall);
        }
        let old = i64::try_from(old)
            .ok()
            .and_then(|old| WindowPixels::try_from(old).ok())
            .ok_or(SplitSizeError::OldTooSmall)?;
        let new = i64::try_from(new)
            .ok()
            .and_then(|new| WindowPixels::try_from(new).ok())
            .ok_or(SplitSizeError::OldTooSmall)?;
        Ok(Self { old, new })
    }

    pub(crate) fn old(self) -> f32 {
        i64::from(self.old) as f32
    }

    pub(crate) fn new_size(self) -> f32 {
        i64::from(self.new) as f32
    }
}

/// Horizontal scroll clipped to GNU's nonnegative fixnum/ptrdiff_t domain.
/// This immutable scalar may be shared between mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct HorizontalScroll(crate::tagged::value::Fixnum);

impl HorizontalScroll {
    pub(crate) fn saturating(value: i128) -> Self {
        let value = value.clamp(
            0,
            i128::from(crate::emacs_core::value::Value::MOST_POSITIVE_FIXNUM),
        ) as i64;
        Self(crate::tagged::value::Fixnum::saturating(value))
    }
}

impl From<HorizontalScroll> for i64 {
    fn from(value: HorizontalScroll) -> Self {
        i64::from(value.0)
    }
}

impl From<HorizontalScroll> for crate::tagged::value::Fixnum {
    fn from(value: HorizontalScroll) -> Self {
        value.0
    }
}

const _: () = assert!(size_of::<HorizontalScroll>() == size_of::<i64>());
static_assertions::assert_impl_all!(HorizontalScroll: Send, Sync);
static_assertions::assert_impl_all!(SplitSizes: Send, Sync);

#[cfg(test)]
#[path = "tests/size_boundaries.rs"]
mod tests;

/// Distinguishes valid geometry from GNU's signed, temporary staging slots.
/// Immutable Fixnum/WindowPixels witnesses carry no heap or mutator affinity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowPixelStage(WindowPixelStageKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowPixelStageKind {
    ValidPixels(WindowPixels),
    LegacyStaged(crate::tagged::value::Fixnum),
}

/// The operation determines its admissible integer range; ADD also carries a
/// canonical signed staging slot rather than assuming committed pixel geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowPixelOperation {
    Set,
    Add(crate::tagged::value::Fixnum),
}

impl WindowPixelOperation {
    pub(crate) fn bounds(self) -> (i64, i64) {
        let minimum = match self {
            Self::Set => 0,
            Self::Add(base) => -i64::from(base),
        };
        (minimum, minimum + i64::from(i32::MAX))
    }
}

impl TryFrom<(i64, WindowPixelOperation)> for WindowPixelStage {
    type Error = WindowSizeError;

    fn try_from((request, operation): (i64, WindowPixelOperation)) -> Result<Self, Self::Error> {
        let (minimum, maximum) = operation.bounds();
        if request < minimum || request > maximum {
            return Err(WindowSizeError::OutOfRange);
        }
        let stored = match operation {
            WindowPixelOperation::Set => crate::tagged::value::Fixnum::try_from(request)
                .map_err(|_| WindowSizeError::OutOfRange)?,
            WindowPixelOperation::Add(base) => {
                // GNU window.c Fset_window_new_pixel stores check_integer_range
                // in a C int, AFTER checking the requested integer range.
                // A failed sibling resize may leave a negative staging base,
                // so the accepted delta need not fit i32. Preserve GNU's
                // explicit signed-int narrowing and then make_fixnum payload.
                let bytes = request.to_le_bytes();
                let gnu_int_delta =
                    i64::from(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
                crate::tagged::value::Fixnum::from_payload_bits(
                    (i64::from(base) + gnu_int_delta) as u64,
                )
            }
        };
        let kind = match WindowPixels::try_from(i64::from(stored)) {
            Ok(pixels) => WindowPixelStageKind::ValidPixels(pixels),
            Err(WindowSizeError::OutOfRange) => WindowPixelStageKind::LegacyStaged(stored),
        };
        Ok(Self(kind))
    }
}

impl From<WindowPixelStage> for crate::tagged::value::Fixnum {
    fn from(value: WindowPixelStage) -> Self {
        match value.0 {
            WindowPixelStageKind::ValidPixels(pixels) => pixels.into(),
            WindowPixelStageKind::LegacyStaged(staged) => staged,
        }
    }
}

impl From<WindowPixelStage> for i64 {
    fn from(value: WindowPixelStage) -> Self {
        i64::from(crate::tagged::value::Fixnum::from(value))
    }
}

static_assertions::assert_impl_all!(WindowPixelStage: Send, Sync);
static_assertions::assert_impl_all!(WindowPixelOperation: Send, Sync);
