//! GNU's `lib/mktime.c` reverse conversion, checked against the reference
//! implementations it replaces.
//!
//! The port has two jobs the platform call cannot do alone: it must answer the
//! same on every host GNU runs on (Darwin's `mktime` rejects pre-1900 times:
//! it returns `(time_t) -1` and leaves `tm_wday` untouched), and on glibc it
//! must agree with the very implementation GNU uses there.  The first test
//! pins the case that exposed the divergence; the second compares the port
//! against the C library over representable dates, gaps and folds.

use super::super::mktime::{MktimeError, mktime_local};
use super::super::with_tz_env;

fn broken_down(
    year: i32,
    month_zero: i32,
    day: i32,
    hour: i32,
    min: i32,
    sec: i32,
    isdst: i32,
) -> libc::tm {
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month_zero;
    tm.tm_mday = day;
    tm.tm_hour = hour;
    tm.tm_min = min;
    tm.tm_sec = sec;
    tm.tm_isdst = isdst;
    tm.tm_wday = -1;
    tm
}

/// `(encode-time '(0 0 0 0 0 0 nil -1 nil))` in America/New_York:
/// `Fencode_time` subtracts the 1900 base and month base, so the request is
/// year 0, December of year -1, day 0 -- GNU normalizes it to -001-11-30 and
/// answers this exact instant on both Linux and macOS.  Darwin's `mktime`
/// alone fails here, which is why the engine no longer delegates.
#[test]
fn pre_1900_local_time_matches_gnu_on_every_host() {
    with_tz_env(Some("America/New_York"), || {
        let mut tm = broken_down(0, -1, 0, 0, 0, 0, -1);
        let seconds = mktime_local(&mut tm).expect("GNU represents -001-11-30");
        assert_eq!(seconds, -62_169_966_238);
        // mktime canonicalizes the request the way decode-time reports it:
        // year -1 (tm_year -1901), November (tm_mon 10), day 30, Tuesday.
        assert_eq!(
            (tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_wday),
            (-1901, 10, 30, 2)
        );
        assert_eq!((tm.tm_hour, tm.tm_min, tm.tm_sec), (0, 0, 0));
    });
}

/// The port's result is the same instant `localtime_r` maps back to the
/// canonical wall clock, in a zone whose pre-1900 offset is LMT.
#[test]
fn pre_1900_result_round_trips_through_localtime() {
    with_tz_env(Some("Europe/London"), || {
        let mut tm = broken_down(1850, 0, 1, 12, 0, 0, -1);
        let seconds = mktime_local(&mut tm).expect("1850 is representable");
        let mut converted = unsafe { std::mem::zeroed::<libc::tm>() };
        let time: libc::time_t = seconds;
        // SAFETY: `converted` is a valid out-parameter.
        let result = unsafe { libc::localtime_r(&time, &mut converted) };
        assert!(!result.is_null(), "localtime_r reads the port's answer");
        assert_eq!(
            (
                converted.tm_year,
                converted.tm_mon,
                converted.tm_mday,
                converted.tm_hour
            ),
            (1850 - 1900, 0, 1, 12)
        );
    });
}

/// GNU's own two mktime bodies disagree when the requested DST flag cannot be
/// satisfied (New York observed no DST in 1902): glibc shifts the instant back
/// one hour, gnulib ignores the impossible request.  Each is the reference on
/// its own platform (verified against the live GNU on both).
#[cfg(target_vendor = "apple")]
#[test]
fn gnulib_variant_ignores_an_impossible_dst_request() {
    with_tz_env(Some("America/New_York"), || {
        let mut tm = broken_down(1902, 0, 1, 0, 0, 0, 1);
        let seconds = mktime_local(&mut tm).expect("1902 is representable");
        assert_eq!(seconds, -2_145_898_800);
        assert_eq!(
            (tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_isdst),
            (2, 0, 1, 0, 0)
        );
    });
}

#[cfg(not(target_vendor = "apple"))]
#[test]
fn glibc_variant_shifts_an_impossible_dst_request_by_one_hour() {
    with_tz_env(Some("America/New_York"), || {
        let mut tm = broken_down(1902, 0, 1, 0, 0, 0, 1);
        let seconds = mktime_local(&mut tm).expect("1902 is representable");
        assert_eq!(seconds, -2_145_902_400);
        assert_eq!(
            (tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_isdst),
            (1, 11, 31, 23, 0)
        );
    });
}

/// On glibc the port must agree with the C library GNU uses there, including
/// the `tm_isdst` search and the spring-forward/fold rules.
#[cfg(target_os = "linux")]
#[test]
fn port_agrees_with_glibc_mktime() {
    let dates = [
        // Ordinary dates spanning DST and standard time.
        (1902, 0, 1, 0),
        (1950, 1, 28, 12),
        (1970, 0, 1, 0),
        (2023, 2, 13, 3),
        (2023, 5, 30, 23),
        (2023, 9, 5, 12),
        (2037, 11, 31, 23),
        (2100, 0, 1, 0),
        // Spring-forward gap (2023-03-12 02:30 does not exist in New York)
        // and the autumn fold (2023-11-05 01:30 happens twice).
        (2023, 2, 12, 2),
        (2023, 10, 5, 1),
    ];
    for zone in [
        "America/New_York",
        "Europe/London",
        "Asia/Tokyo",
        "UTC",
        "Australia/Lord_Howe",
    ] {
        with_tz_env(Some(zone), || {
            for (year, month, day, hour) in dates {
                for isdst in [-1, 0, 1] {
                    // `hour` 2:30 for the gap probe and 1:30 for the fold.
                    let minute = if hour == 2 || hour == 1 { 30 } else { 0 };
                    let mut port = broken_down(year, month, day, hour, minute, 0, isdst);
                    let mut libc_tm = port;
                    let port_result = mktime_local(&mut port);
                    // SAFETY: `libc_tm` is an initialized struct tm.
                    let libc_result = unsafe { libc::mktime(&mut libc_tm) };
                    let libc_result = if libc_result == -1 && libc_tm.tm_wday < 0 {
                        Err(MktimeError::Overflow)
                    } else {
                        Ok(libc_result)
                    };
                    assert_eq!(
                        port_result, libc_result,
                        "{zone} {year}-{month}-{day} {hour}:{minute} isdst={isdst}"
                    );
                    if port_result.is_ok() {
                        assert_eq!(
                            (
                                port.tm_year,
                                port.tm_mon,
                                port.tm_mday,
                                port.tm_hour,
                                port.tm_min,
                                port.tm_sec,
                                port.tm_wday,
                                port.tm_yday,
                                port.tm_isdst,
                            ),
                            (
                                libc_tm.tm_year,
                                libc_tm.tm_mon,
                                libc_tm.tm_mday,
                                libc_tm.tm_hour,
                                libc_tm.tm_min,
                                libc_tm.tm_sec,
                                libc_tm.tm_wday,
                                libc_tm.tm_yday,
                                libc_tm.tm_isdst,
                            ),
                            "canonical struct tm differs for {zone} \
                             {year}-{month}-{day} {hour}:{minute} isdst={isdst}"
                        );
                    }
                }
            }
        });
    }
}
