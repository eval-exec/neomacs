use crate::emacs_core::value::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A normalized GNU timer timespec. Fields are private so a timer vector cannot
/// bypass carry/range validation. This immutable scalar is mutator-independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GnuTimerTimestamp {
    seconds: i64,
    nanos: u32,
}

impl GnuTimerTimestamp {
    pub(crate) fn from_components(
        high: Value,
        low: Value,
        usecs: Value,
        psecs: Value,
    ) -> Option<Self> {
        fn integer(value: Value) -> Option<i128> {
            value
                .as_fixnum()
                .map(i128::from)
                .or_else(|| value.as_bignum().and_then(|n| i128::try_from(n).ok()))
        }
        let micros = i128::from(usecs.as_fixnum()?);
        let picos = i128::from(psecs.as_fixnum()?);
        let fraction = micros * 1_000_000 + picos;
        let seconds = integer(high)?
            .checked_mul(65_536)?
            .checked_add(i128::from(low.as_fixnum()?))?
            .checked_add(fraction.div_euclid(1_000_000_000_000))?;
        Some(Self {
            seconds: i64::try_from(seconds).ok()?,
            nanos: (fraction.rem_euclid(1_000_000_000_000) / 1_000) as u32,
        })
    }

    pub(crate) fn now() -> Self {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => Self::from_duration(duration),
            Err(error) => Self::from_nanos_saturating(-Self::duration_nanos(error.duration())),
        }
    }

    fn duration_nanos(duration: Duration) -> i128 {
        i128::from(duration.as_secs()) * 1_000_000_000 + i128::from(duration.subsec_nanos())
    }

    fn nanos(self) -> i128 {
        i128::from(self.seconds) * 1_000_000_000 + i128::from(self.nanos)
    }

    fn from_nanos_saturating(nanos: i128) -> Self {
        let seconds = nanos.div_euclid(1_000_000_000);
        match i64::try_from(seconds) {
            Ok(seconds) => Self {
                seconds,
                nanos: nanos.rem_euclid(1_000_000_000) as u32,
            },
            Err(_) if seconds > 0 => Self {
                seconds: i64::MAX,
                nanos: 999_999_999,
            },
            Err(_) => Self {
                seconds: i64::MIN,
                nanos: 0,
            },
        }
    }

    pub(crate) fn duration_until(self, now: Self) -> Duration {
        if self <= now {
            return Duration::ZERO;
        }
        let delta = self.nanos() - now.nanos();
        // GNU timespec_sub saturates differences outside signed time_t range.
        let max = i128::from(i64::MAX) * 1_000_000_000 + 999_999_999;
        let delta = delta.min(max);
        Duration::new(
            (delta / 1_000_000_000) as u64,
            (delta % 1_000_000_000) as u32,
        )
    }

    pub(crate) fn overdue_duration(self, now: Self) -> Duration {
        now.duration_until(self)
    }

    pub(crate) fn from_duration(duration: Duration) -> Self {
        Self::from_nanos_saturating(Self::duration_nanos(duration))
    }

    pub(crate) fn add_duration(self, duration: Duration) -> Self {
        Self::from_nanos_saturating(self.nanos() + Self::duration_nanos(duration))
    }
}

static_assertions::assert_impl_all!(GnuTimerTimestamp: Send, Sync);

impl From<GnuTimerTimestamp> for Value {
    fn from(timestamp: GnuTimerTimestamp) -> Self {
        Self::vector(vec![
            Self::NIL,
            Self::fixnum(timestamp.seconds >> 16),
            Self::fixnum(timestamp.seconds & 0xffff),
            Self::fixnum(i64::from(timestamp.nanos / 1000)),
            Self::NIL,
            Self::symbol("ignore"),
            Self::NIL,
            Self::NIL,
            Self::fixnum(i64::from(timestamp.nanos % 1000) * 1000),
            Self::NIL,
        ])
    }
}
