//! Numeric face-height domains at the Lisp and font-realization boundaries.

use crate::emacs_core::value::{Value, ValueKind};
use crate::face::FaceHeight;
use crate::tagged::value::Fixnum;
use thiserror::Error;

/// A positive absolute Lisp face height in tenths of points.
/// GNU accepts the whole positive fixnum domain, beyond the backend's i32.
/// This immutable scalar contains no heap state and may cross mutators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct AbsoluteHeight(Fixnum);

/// A finite relative Lisp face height whose GNU test merge against 10 is
/// positive. GNU's signed fixnum narrowing admits some large negative scales.
/// This immutable scalar contains no heap state and may cross mutators.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct RelativeHeight(f64);

/// A finite, positive scale suitable for font realization.
/// This immutable scalar contains no heap state and may cross mutators.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct PositiveRelativeHeight(f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NumericFaceHeight {
    Absolute(AbsoluteHeight),
    Relative(RelativeHeight),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub(crate) enum FaceHeightError {
    #[error("face height fails GNU numeric height validation")]
    InvalidNumericHeight,
    #[error("face height exceeds the font backend's positive range")]
    BackendRange,
}

impl TryFrom<Value> for NumericFaceHeight {
    type Error = FaceHeightError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value.kind() {
            ValueKind::Fixnum(height) if height > 0 => Fixnum::try_from(height)
                .map(|height| Self::Absolute(AbsoluteHeight(height)))
                .map_err(|crate::tagged::value::FixnumRangeError::OutOfRange(_)| {
                    FaceHeightError::InvalidNumericHeight
                }),
            ValueKind::Float => {
                let height = value.xfloat();
                if height.is_finite() && i64::from(gnu_height_merge_fixnum(height * 10.0)) > 0 {
                    Ok(Self::Relative(RelativeHeight(height)))
                } else {
                    Err(FaceHeightError::InvalidNumericHeight)
                }
            }
            _ => Err(FaceHeightError::InvalidNumericHeight),
        }
    }
}

impl TryFrom<NumericFaceHeight> for FaceHeight {
    type Error = FaceHeightError;

    fn try_from(height: NumericFaceHeight) -> Result<Self, Self::Error> {
        match height {
            NumericFaceHeight::Absolute(height) => i32::try_from(i64::from(height.0))
                .map(Self::Absolute)
                .map_err(|_| FaceHeightError::BackendRange),
            NumericFaceHeight::Relative(height) => {
                let height = PositiveRelativeHeight::try_from(height)?;
                Ok(Self::Relative(height.0))
            }
        }
    }
}

impl TryFrom<f64> for PositiveRelativeHeight {
    type Error = FaceHeightError;

    fn try_from(height: f64) -> Result<Self, Self::Error> {
        if height.is_finite() && height > 0.0 {
            Ok(Self(height))
        } else {
            Err(FaceHeightError::BackendRange)
        }
    }
}

impl TryFrom<RelativeHeight> for PositiveRelativeHeight {
    type Error = FaceHeightError;

    fn try_from(height: RelativeHeight) -> Result<Self, Self::Error> {
        Self::try_from(height.0)
    }
}

/// A height validated for backend realization. Small positive scales produced
/// by relative merges need not pass the setter's separate test against 10.
/// This immutable scalar contains no heap state and may cross mutators.
#[derive(Clone, Debug, PartialEq)]
#[repr(transparent)]
pub(crate) struct BackendFaceHeight(FaceHeight);

impl TryFrom<Value> for BackendFaceHeight {
    type Error = FaceHeightError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value.kind() {
            ValueKind::Fixnum(height) if height > 0 => i32::try_from(height)
                .map(|height| Self(FaceHeight::Absolute(height)))
                .map_err(|_| FaceHeightError::BackendRange),
            ValueKind::Float => PositiveRelativeHeight::try_from(value.xfloat())
                .map(|height| Self(FaceHeight::Relative(height.0))),
            _ => Err(FaceHeightError::BackendRange),
        }
    }
}

impl From<BackendFaceHeight> for FaceHeight {
    fn from(height: BackendFaceHeight) -> Self {
        height.0
    }
}

/// GNU `xfaces.c::merge_face_heights` truncates a double to EMACS_INT and then
/// applies `make_fixnum`'s signed payload narrowing. Finite products such as
/// 4.7e18 intentionally probe as positive despite exceeding MOST_POSITIVE_FIXNUM.
/// Keep that GNU-specific conversion explicit and return a validated payload;
/// nonfinite/unrepresentable products never reach the Rust numeric cast.
/// This pure conversion carries no Lisp heap state or mutator affinity.
pub(crate) fn gnu_height_merge_fixnum(height: f64) -> Fixnum {
    if !height.is_finite() || height < i64::MIN as f64 || height >= i64::MAX as f64 {
        // GNU's conversion yields an indefinite integer on these inputs;
        // its fixnum payload decodes to zero, as verified with GNU 31.1.
        return Fixnum::saturating(0);
    }
    let integer = height.trunc() as i64;
    Fixnum::from_payload_bits(integer as u64)
}

static_assertions::assert_impl_all!(NumericFaceHeight: Send, Sync);
static_assertions::assert_impl_all!(PositiveRelativeHeight: Send, Sync);
static_assertions::assert_impl_all!(BackendFaceHeight: Send, Sync);

#[cfg(test)]
#[path = "tests/height_boundaries.rs"]
mod tests;
