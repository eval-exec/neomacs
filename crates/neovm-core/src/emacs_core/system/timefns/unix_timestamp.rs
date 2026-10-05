use super::{time_error_overflow, time_value_seconds_and_nanos};
use crate::emacs_core::error::Flow;
use crate::emacs_core::value::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A representable signed Unix timestamp with normalized nanoseconds.
///
/// `nanoseconds` is less than one second. Negative timestamps use floor
/// seconds, so half a second before the epoch is (-1, 500_000_000), matching
/// GNU timespecs. This immutable value contains no Lisp or mutator state and
/// is safe to move between concurrently running mutators.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UnixTimestamp {
    seconds: i64,
    nanoseconds: u32,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(crate) enum TimestampError {
    #[error("Specified time is not representable")]
    OutOfRange,
}

static_assertions::assert_impl_all!(UnixTimestamp: Send, Sync);

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "File-time callers migrate in the next change")
)]
impl UnixTimestamp {
    pub(crate) const fn seconds(self) -> i64 {
        self.seconds
    }

    pub(crate) const fn nanoseconds(self) -> u32 {
        self.nanoseconds
    }
}

impl TryFrom<&Value> for UnixTimestamp {
    type Error = Flow;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        if let Some(seconds) = value.as_fixnum() {
            return Ok(Self {
                seconds,
                nanoseconds: 0,
            });
        }
        let (seconds, nanoseconds) = time_value_seconds_and_nanos(value)?;
        let nanoseconds = u32::try_from(nanoseconds)
            .ok()
            .filter(|&n| n < 1_000_000_000)
            .ok_or_else(time_error_overflow)?;
        Ok(Self {
            seconds,
            nanoseconds,
        })
    }
}

impl TryFrom<SystemTime> for UnixTimestamp {
    type Error = TimestampError;

    fn try_from(value: SystemTime) -> Result<Self, Self::Error> {
        let (seconds, nanoseconds) = match value.duration_since(UNIX_EPOCH) {
            Ok(duration) => (i128::from(duration.as_secs()), duration.subsec_nanos()),
            Err(error) => {
                let duration = error.duration();
                let nanos = duration.subsec_nanos();
                if nanos == 0 {
                    (-i128::from(duration.as_secs()), 0)
                } else {
                    (-i128::from(duration.as_secs()) - 1, 1_000_000_000 - nanos)
                }
            }
        };
        Ok(Self {
            seconds: i64::try_from(seconds).map_err(|_| TimestampError::OutOfRange)?,
            nanoseconds,
        })
    }
}

impl TryFrom<UnixTimestamp> for SystemTime {
    type Error = TimestampError;

    fn try_from(value: UnixTimestamp) -> Result<Self, Self::Error> {
        let time = if value.seconds >= 0 {
            UNIX_EPOCH.checked_add(Duration::new(
                value.seconds.unsigned_abs(),
                value.nanoseconds,
            ))
        } else {
            let (seconds, nanos) = if value.nanoseconds == 0 {
                (value.seconds.unsigned_abs(), 0)
            } else {
                (
                    value.seconds.unsigned_abs() - 1,
                    1_000_000_000 - value.nanoseconds,
                )
            };
            UNIX_EPOCH.checked_sub(Duration::new(seconds, nanos))
        };
        time.ok_or(TimestampError::OutOfRange)
    }
}
