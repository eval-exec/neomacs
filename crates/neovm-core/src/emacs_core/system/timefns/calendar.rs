use super::{Flow, TM_YEAR_BASE, decode_epoch_secs, time_error_overflow};

/// Broken-down calendar fields representable by GNU's `struct tm` members.
///
/// Fields are checked before normalization, including the month/year offsets.
/// This plain immutable value contains no Lisp pointers or mutator state and
/// is safe to move between concurrently running mutators.
#[derive(Clone, Copy, Debug)]
pub(super) struct CalendarTime {
    sec: i32,
    min: i32,
    hour: i32,
    day: i32,
    month_zero: i32,
    year_from_1900: i32,
}

static_assertions::assert_impl_all!(CalendarTime: Send, Sync);

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(super) enum CalendarFieldOverflow {
    #[error("Specified time is not representable")]
    OutOfRange,
}

impl TryFrom<[i64; 6]> for CalendarTime {
    type Error = CalendarFieldOverflow;

    fn try_from([sec, min, hour, day, month, year]: [i64; 6]) -> Result<Self, Self::Error> {
        let checked =
            |value: i64| i32::try_from(value).map_err(|_| CalendarFieldOverflow::OutOfRange);
        Ok(Self {
            sec: checked(sec)?,
            min: checked(min)?,
            hour: checked(hour)?,
            day: checked(day)?,
            month_zero: checked(
                month
                    .checked_sub(1)
                    .ok_or(CalendarFieldOverflow::OutOfRange)?,
            )?,
            year_from_1900: checked(
                year.checked_sub(TM_YEAR_BASE)
                    .ok_or(CalendarFieldOverflow::OutOfRange)?,
            )?,
        })
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "encode-time callers migrate in the next change")
)]
impl CalendarTime {
    /// Closed-form Gregorian conversion, bounded by the checked C-int fields.
    /// Intermediate values fit in i64 even when month/day fields normalize far
    /// outside their usual ranges. GNU rejects normalized years beyond tm_year.
    pub(super) fn epoch_seconds(self) -> Result<i64, Flow> {
        let year = i64::from(self.year_from_1900) + TM_YEAR_BASE;
        let m0 = i64::from(self.month_zero);
        let year = year + m0.div_euclid(12);
        let month = m0.rem_euclid(12) + 1;
        let y = year - i64::from(month <= 2);
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let mp = if month > 2 { month - 3 } else { month + 9 };
        let doy = (153 * mp + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        let seconds = days * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.min) * 60
            + i64::from(self.sec);
        decode_epoch_secs(seconds)?;
        Ok(seconds)
    }
}

impl CalendarTime {
    #[cfg(unix)]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "mktime callers migrate in the next change")
    )]
    pub(super) fn into_tm(self) -> libc::tm {
        // SAFETY: zero initializes every field of libc::tm validly; the
        // checked calendar fields replace its input members below.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_sec = self.sec;
        tm.tm_min = self.min;
        tm.tm_hour = self.hour;
        tm.tm_mday = self.day;
        tm.tm_mon = self.month_zero;
        tm.tm_year = self.year_from_1900;
        tm
    }
}
