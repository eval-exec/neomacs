//! GNU's local-time reverse conversion, ported from glibc/gnulib
//! `lib/mktime.c`.
//!
//! GNU Emacs does not call the platform `mktime` directly on every host: when
//! configure finds the C library's `mktime` broken (macOS's version rejects
//! every time before 1900), GNU compiles gnulib's replacement, which is
//! glibc's `__mktime_internal`.  `(encode-time '(0 0 0 0 0 0 nil -1 nil))`
//! proves the difference: GNU answers year -001 on both Linux and macOS, while
//! Darwin's `mktime` returns `(time_t) -1` with `tm_wday` untouched.
//!
//! Delegating to the platform call therefore makes the answer a property of
//! the C library instead of GNU.  This module implements the algorithm both
//! C libraries share: guess an offset, convert the candidate instant with
//! `localtime_r`, and correct the guess by the difference between the
//! requested wall clock and the converted one, with GNU's gap/fold and
//! `tm_isdst` rules.  `localtime_r` itself is wide (Darwin converts year
//! -1199 and year 318857), which is what makes the probe method sufficient.
//!
//! The only C-library-dependent input left is the probe budget: glibc allows
//! 6 probes (`lib/mktime.c:__mktime_internal`), while the gnulib replacement
//! GNU links on Apple doubles it (`:391-394`).  The budget only decides when a
//! pathological oscillation is reported as unrepresentable.

/// `long_int`/`time_t` bounds: both are signed 64-bit on the Unix hosts this
/// engine runs on, so `mktime_min`/`mktime_max` clamp to the full range.
const TIME_MIN: i64 = i64::MIN;
const TIME_MAX: i64 = i64::MAX;

/// Which `mktime` body GNU's reference build for this host contains.
///
/// GNU uses the C library's `mktime` where configure finds it working, and
/// gnulib's `rpl_mktime` where it does not.  The two bodies share the probe
/// loop but differ after a `tm_isdst` mismatch: glibc probes an interval wider
/// than a year and, failing to find the requested flag, shifts the result by
/// one hour of DST; gnulib probes one year and then ignores the request.  The
/// macOS GNU build has `NEED_MKTIME_WORKING` (its libc `mktime` rejects every
/// time before 1900), so Apple takes the gnulib body and glibc platforms the
/// libc body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MktimeVariant {
    /// glibc `time/mktime.c`.
    Glibc,
    /// gnulib `lib/mktime.c` replacement.
    Gnulib,
}

#[cfg(target_vendor = "apple")]
const MKTIME_VARIANT: MktimeVariant = MktimeVariant::Gnulib;
#[cfg(not(target_vendor = "apple"))]
const MKTIME_VARIANT: MktimeVariant = MktimeVariant::Glibc;

/// Probe budget: glibc's six, or the gnulib replacement's doubled twelve
/// (lib/mktime.c:391-394).
const REMAINING_PROBES: u32 = match MKTIME_VARIANT {
    MktimeVariant::Glibc => 6,
    MktimeVariant::Gnulib => 12,
};

const EPOCH_YEAR: i64 = 1970;
const YEAR_BASE: i64 = 1900;

/// Days before the first of each month, for a normal and a leap year
/// (`__mon_yday`, lib/mktime.c:196-203).
const MON_YDAY: [[i64; 13]; 2] = [
    [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365],
    [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366],
];

/// The striding distance GNU probes when the requested `tm_isdst` differs
/// from the converted one; the shortest DST period in tzdata is 601200s
/// (lib/mktime.c:463-467).
const DST_PROBE_STRIDE: i64 = 601_200;

/// glibc's search bound: half the longest relevant interval plus one stride
/// (`duration_max / 2 + stride`, time/mktime.c).
const GLIBC_DST_DELTA_BOUND: i64 = 457_243_209 / 2 + DST_PROBE_STRIDE;

/// gnulib's search bound: one year plus one stride (lib/mktime.c:470-474).
const GNULIB_DST_DELTA_BOUND: i64 = 366 * 24 * 60 * 60 + 1 + DST_PROBE_STRIDE;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum MktimeError {
    /// The probe budget ran out: GNU's `EOVERFLOW`.
    #[error("the requested local time is not representable")]
    Overflow,
    /// The platform conversion failed for a reason other than range: GNU's
    /// `time_spec_invalid` route in `Fencode_time`.
    #[error("the platform local-time conversion failed")]
    Invalid,
}

/// The epoch seconds of a canonicalized local `struct tm`, matching
/// `__mktime_internal (tp, true, ...)`.
///
/// On success `broken_down` is replaced with the converted value, exactly as
/// `mktime` does; `Err` is GNU's `(time_t) -1` with `tm_wday` left negative.
pub(super) fn mktime_local(broken_down: &mut libc::tm) -> Result<i64, MktimeError> {
    // GNU's `mktime_z` runs `set_tz`, which calls `tzset`; `localtime_r` below
    // reads the resulting rules.  The `libc` crate does not bind `tzset`.
    unsafe extern "C" {
        fn tzset();
    }
    // SAFETY: tzset has no pointer arguments.  Its callers serialize timezone
    // mutations through the same process-wide lock as `ScopedTzEnv`.
    unsafe { tzset() };
    let sec = i64::from(broken_down.tm_sec);
    let min = i64::from(broken_down.tm_min);
    let hour = i64::from(broken_down.tm_hour);
    let isdst = i64::from(broken_down.tm_isdst);

    // GNU normalizes MON into YEAR first, keeping the truncated remainder.
    let mon = i64::from(broken_down.tm_mon);
    let mon_remainder = mon % 12;
    let negative_mon_remainder = mon_remainder < 0;
    let mon_years = mon / 12 - i64::from(negative_mon_remainder);
    let year = i64::from(broken_down.tm_year) + mon_years;

    let month_index = mon_remainder + if negative_mon_remainder { 12 } else { 0 };
    let month_index = usize::try_from(month_index)
        .expect("the month remainder is normalized to 0..=11 before indexing");
    let mon_yday = MON_YDAY[usize::from(leapyear(year))][month_index] - 1;
    let yday = mon_yday + i64::from(broken_down.tm_mday);

    // LEAP_SECONDS_POSSIBLE: `ydhms_diff` assumes every minute has 60
    // seconds, so out-of-range seconds are clamped for the probe and restored
    // by `finish` afterwards.
    let sec_requested = sec;
    let sec = sec.clamp(0, 59);

    // GNU carries a static `localtime_offset` guess between calls; it only
    // changes how quickly the probe converges, never the answer.
    let negative_offset_guess = 0i64;
    let t0 = ydhms_diff(
        year,
        yday,
        hour,
        min,
        sec,
        EPOCH_YEAR - YEAR_BASE,
        0,
        0,
        0,
        negative_offset_guess,
    );
    let mut t = t0;
    let (mut t1, mut t2) = (t0, t0);
    let mut dst2 = false;
    let mut remaining_probes = REMAINING_PROBES;
    let mut converted = unsafe { std::mem::zeroed::<libc::tm>() };

    loop {
        ranged_convert(&mut t, &mut converted)?;
        let dt = tm_diff(year, yday, hour, min, sec, &converted);
        if dt == 0 {
            break;
        }

        if t == t1
            && t != t2
            && (converted.tm_isdst < 0
                || (if isdst < 0 {
                    dst2 <= (converted.tm_isdst != 0)
                } else {
                    (isdst != 0) != (converted.tm_isdst != 0)
                }))
        {
            // A spring-forward gap: return the nearest side, preferring the
            // one whose `tm_isdst` differs from the request.
            let (canonical_time, canonical) = finish(t, converted, sec, sec_requested)?;
            *broken_down = canonical;
            return Ok(canonical_time);
        }

        remaining_probes -= 1;
        if remaining_probes == 0 {
            return Err(MktimeError::Overflow);
        }

        t1 = t2;
        t2 = t;
        t = t.checked_add(dt).ok_or(MktimeError::Overflow)?;
        dst2 = converted.tm_isdst != 0;
    }

    // A match without the requested DST flag: probe nearby instants for one
    // that has it and extrapolate back.
    if isdst_differ(isdst, i64::from(converted.tm_isdst)) {
        let delta_bound = match MKTIME_VARIANT {
            MktimeVariant::Glibc => GLIBC_DST_DELTA_BOUND,
            MktimeVariant::Gnulib => GNULIB_DST_DELTA_BOUND,
        };
        let found = search_neighboring_isdst(
            &mut t,
            &mut converted,
            year,
            yday,
            hour,
            min,
            sec,
            isdst,
            delta_bound,
        )?;
        if !found && MKTIME_VARIANT == MktimeVariant::Glibc {
            // glibc assumes the caller's DST flag was off by one hour of DST
            // (`dst_difference = (isdst == 0) - (tm_isdst == 0)`), shifts the
            // instant and accepts the shifted conversion, or reports overflow.
            let dst_difference = i64::from(isdst == 0) - i64::from(converted.tm_isdst == 0);
            t = t
                .checked_add(60 * 60 * dst_difference)
                .ok_or(MktimeError::Overflow)?;
            let mut shifted = t;
            let mut shifted_tm = unsafe { std::mem::zeroed::<libc::tm>() };
            if !(TIME_MIN..=TIME_MAX).contains(&shifted)
                || !convert_exact(&mut shifted, &mut shifted_tm)
            {
                return Err(MktimeError::Overflow);
            }
            t = shifted;
            converted = shifted_tm;
        }
    }

    let (canonical_time, canonical) = finish(t, converted, sec, sec_requested)?;
    *broken_down = canonical;
    Ok(canonical_time)
}

/// GNU's `offset_found` tail: restore out-of-range seconds, then hand back the
/// canonical broken-down time.
fn finish(
    mut t: i64,
    mut converted: libc::tm,
    sec: i64,
    sec_requested: i64,
) -> Result<(i64, libc::tm), MktimeError> {
    if sec_requested != i64::from(converted.tm_sec) {
        let mut sec_adjustment = i64::from(sec == 0 && converted.tm_sec == 60);
        sec_adjustment -= sec;
        sec_adjustment += sec_requested;
        t = t.checked_add(sec_adjustment).ok_or(MktimeError::Overflow)?;
        if !(TIME_MIN..=TIME_MAX).contains(&t) {
            return Err(MktimeError::Overflow);
        }
        let mut adjusted = t;
        let mut adjusted_tm = unsafe { std::mem::zeroed::<libc::tm>() };
        ranged_convert(&mut adjusted, &mut adjusted_tm)?;
        t = adjusted;
        converted = adjusted_tm;
    }
    Ok((t, converted))
}

/// Probe instants around `t` for one whose DST flag matches the request, then
/// extrapolate back to the requested wall clock.  `delta_bound` is the variant
/// constant: glibc searches a wider interval than gnulib, but the loop body is
/// the same in both (`time/mktime.c`, `lib/mktime.c:476-503`).
#[allow(clippy::too_many_arguments)]
fn search_neighboring_isdst(
    t: &mut i64,
    converted: &mut libc::tm,
    year: i64,
    yday: i64,
    hour: i64,
    min: i64,
    sec: i64,
    requested_isdst: i64,
    delta_bound: i64,
) -> Result<bool, MktimeError> {
    let mut delta = DST_PROBE_STRIDE;
    while delta < delta_bound {
        for direction in [-1i64, 1] {
            let Some(candidate) = t.checked_add(delta * direction) else {
                continue;
            };
            let mut candidate_time = candidate;
            let mut candidate_tm = unsafe { std::mem::zeroed::<libc::tm>() };
            ranged_convert(&mut candidate_time, &mut candidate_tm)?;
            if isdst_differ(requested_isdst, i64::from(candidate_tm.tm_isdst)) {
                continue;
            }
            let Some(guess) =
                candidate_time.checked_add(tm_diff(year, yday, hour, min, sec, &candidate_tm))
            else {
                continue;
            };
            if !(TIME_MIN..=TIME_MAX).contains(&guess) {
                continue;
            }
            let mut guess_time = guess;
            let mut guess_tm = unsafe { std::mem::zeroed::<libc::tm>() };
            match ranged_convert(&mut guess_time, &mut guess_tm) {
                Ok(()) => {
                    *t = guess_time;
                    *converted = guess_tm;
                    return Ok(true);
                }
                // `errno != EOVERFLOW` aborts; a range miss keeps probing
                // (lib/mktime.c:489-500).
                Err(MktimeError::Overflow) => {}
                Err(error) => return Err(error),
            }
        }
        delta += DST_PROBE_STRIDE;
    }
    Ok(false)
}

/// The raw `convert_time` conversion glibc's isdst fallback uses: unlike
/// [`ranged_convert`], failure is not range-searched and always becomes
/// `EOVERFLOW` at the caller (glibc time/mktime.c, "overwrites any earlier
/// conversion error").
fn convert_exact(t: &mut i64, tm: &mut libc::tm) -> bool {
    match localtime_r(*t) {
        Ok(converted) => {
            *tm = converted;
            true
        }
        Err(_) => false,
    }
}

/// `ranged_convert` (lib/mktime.c:283-324): convert T with `localtime_r`,
/// binary-searching toward zero if it is outside `time_t`'s range.
fn ranged_convert(t: &mut i64, tm: &mut libc::tm) -> Result<(), MktimeError> {
    let t1 = (*t).clamp(TIME_MIN, TIME_MAX);
    match localtime_r(t1) {
        Ok(converted) => {
            *t = t1;
            *tm = converted;
            return Ok(());
        }
        Err(MktimeError::Invalid) => return Err(MktimeError::Invalid),
        Err(MktimeError::Overflow) => {}
    }

    let mut bad = t1;
    let mut ok = 0i64;
    let mut ok_tm: Option<libc::tm> = None;
    loop {
        let mid = long_int_avg(ok, bad);
        if mid == ok || mid == bad {
            break;
        }
        match localtime_r(mid) {
            Ok(converted) => {
                ok = mid;
                ok_tm = Some(converted);
            }
            Err(MktimeError::Invalid) => return Err(MktimeError::Invalid),
            Err(MktimeError::Overflow) => bad = mid,
        }
    }
    match ok_tm {
        Some(converted) => {
            *t = ok;
            *tm = converted;
            Ok(())
        }
        None => Err(MktimeError::Overflow),
    }
}

fn localtime_r(t: i64) -> Result<libc::tm, MktimeError> {
    let value: libc::time_t = t as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    // SAFETY: both pointers are valid for the duration of the call.
    let converted = unsafe { libc::localtime_r(&value, &mut tm) };
    if !converted.is_null() {
        return Ok(tm);
    }
    let errno = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EINVAL);
    if errno == libc::EOVERFLOW {
        Err(MktimeError::Overflow)
    } else {
        Err(MktimeError::Invalid)
    }
}

/// Is YEAR (numbered as in `tm_year`, i.e. years since 1900) a leap year?
/// Written without adding `YEAR_BASE`, so negative years work
/// (lib/mktime.c:187-194).
fn leapyear(year: i64) -> bool {
    (year & 3) == 0 && (year % 100 != 0 || ((year / 100) & 3) == ((-(YEAR_BASE / 100)) & 3))
}

/// `ydhms_diff` (lib/mktime.c:216-245): the second difference between two
/// broken-down times, counting leap days in between.
#[allow(clippy::too_many_arguments)]
fn ydhms_diff(
    year1: i64,
    yday1: i64,
    hour1: i64,
    min1: i64,
    sec1: i64,
    year0: i64,
    yday0: i64,
    hour0: i64,
    min0: i64,
    sec0: i64,
) -> i64 {
    let a4 = (year1 >> 2) + (YEAR_BASE >> 2) - i64::from((year1 & 3) == 0);
    let b4 = (year0 >> 2) + (YEAR_BASE >> 2) - i64::from((year0 & 3) == 0);
    let a100 = (a4 + i64::from(a4 < 0)) / 25 - i64::from(a4 < 0);
    let b100 = (b4 + i64::from(b4 < 0)) / 25 - i64::from(b4 < 0);
    let a400 = a100 >> 2;
    let b400 = b100 >> 2;
    let intervening_leap_days = (a4 - b4) - (a100 - b100) + (a400 - b400);

    let years = year1 - year0;
    let days = 365 * years + yday1 - yday0 + intervening_leap_days;
    let hours = 24 * days + hour1 - hour0;
    let minutes = 60 * hours + min1 - min0;
    60 * minutes + sec1 - sec0
}

/// `tm_diff` (lib/mktime.c:252-263).
fn tm_diff(year: i64, yday: i64, hour: i64, min: i64, sec: i64, tm: &libc::tm) -> i64 {
    ydhms_diff(
        year,
        yday,
        hour,
        min,
        sec,
        i64::from(tm.tm_year),
        i64::from(tm.tm_yday),
        i64::from(tm.tm_hour),
        i64::from(tm.tm_min),
        i64::from(tm.tm_sec),
    )
}

/// `isdst_differ` (lib/mktime.c:209-213): one is zero, the other positive,
/// and both are known (non-negative).
fn isdst_differ(a: i64, b: i64) -> bool {
    ((a == 0) != (b == 0)) && a >= 0 && b >= 0
}

/// `long_int_avg` (lib/mktime.c:251): the average rounded toward positive
/// infinity without overflowing.
fn long_int_avg(a: i64, b: i64) -> i64 {
    (a >> 1) + (b >> 1) + ((a | b) & 1)
}
