use super::*;

#[test]
fn format_time_string_epoch() {
    crate::test_utils::init_test_tracing();
    // Unix epoch: 1970-01-01 00:00:00 UTC (Thursday)
    let result = builtin_format_time_string(vec![
        Value::string("%Y-%m-%d %H:%M:%S"),
        Value::fixnum(0),
        Value::T,
    ]);
    assert_eq!(
        result.unwrap().as_utf8_str().unwrap(),
        "1970-01-01 00:00:00"
    );
}

#[test]
fn format_time_string_day_name() {
    crate::test_utils::init_test_tracing();
    // 1970-01-01 is a Thursday.
    let result = builtin_format_time_string(vec![Value::string("%A"), Value::fixnum(0), Value::T]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "Thursday");
}

#[test]
fn format_time_string_month_name() {
    crate::test_utils::init_test_tracing();
    let result = builtin_format_time_string(vec![Value::string("%B"), Value::fixnum(0), Value::T]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "January");
}

#[test]
fn format_time_string_known_date() {
    crate::test_utils::init_test_tracing();
    // 2000-01-01 00:00:00 UTC = 946684800
    let result = builtin_format_time_string(vec![
        Value::string("%Y-%m-%d %A"),
        Value::fixnum(946684800),
        Value::T,
    ]);
    assert_eq!(
        result.unwrap().as_utf8_str().unwrap(),
        "2000-01-01 Saturday"
    );
}

#[test]
fn format_time_string_literal_percent() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_format_time_string(vec![Value::string("100%%"), Value::fixnum(0), Value::T]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "100%");
}

#[test]
fn format_time_string_epoch_seconds_specifier_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = builtin_format_time_string(vec![
        Value::string("%s"),
        Value::fixnum(1704067200),
        Value::T,
    ]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "1704067200");
}

#[test]
fn format_time_string_iso_format() {
    crate::test_utils::init_test_tracing();
    let result = builtin_format_time_string(vec![
        Value::string("%F %T"),
        Value::fixnum(946684800),
        Value::T,
    ]);
    assert_eq!(
        result.unwrap().as_utf8_str().unwrap(),
        "2000-01-01 00:00:00"
    );
}

#[test]
fn format_time_string_iso_week_directives_match_gnu() {
    crate::test_utils::init_test_tracing();
    let cases = [
        (1609459200, "2020|20|53|5|2021-01-01"),
        (1609372800, "2020|20|53|4|2020-12-31"),
        (1577750400, "2020|20|01|2|2019-12-31"),
        (1483228800, "2016|16|52|7|2017-01-01"),
        (1767225600, "2026|26|01|4|2026-01-01"),
    ];
    for (timestamp, expected) in cases {
        let result = builtin_format_time_string(vec![
            Value::string("%G|%g|%V|%u|%Y-%m-%d"),
            Value::fixnum(timestamp),
            Value::T,
        ]);
        assert_eq!(result.unwrap().as_utf8_str().unwrap(), expected);
    }

    let result = builtin_format_time_string(vec![
        Value::string("%-V|%-G|%-g"),
        Value::fixnum(1767225600),
        Value::T,
    ]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "1|2026|26");
}

#[test]
fn format_time_string_ampm() {
    crate::test_utils::init_test_tracing();
    // 2000-01-01 15:30:00 UTC = 946684800 + 15*3600 + 30*60 = 946740600
    let result = builtin_format_time_string(vec![
        Value::string("%I:%M %p"),
        Value::fixnum(946740600),
        Value::T,
    ]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "03:30 PM");
}

#[test]
fn format_time_string_no_time_uses_current() {
    crate::test_utils::init_test_tracing();
    // Should not error when TIME is nil.
    let result = builtin_format_time_string(vec![Value::string("%Y"), Value::NIL]);
    assert!(result.is_ok());
    // Should return a 4-digit year.
    let year_str = result.unwrap();
    assert_eq!(year_str.as_utf8_str().unwrap().len(), 4);
}

#[test]
fn format_time_string_honors_explicit_zone_argument() {
    crate::test_utils::init_test_tracing();
    let result = builtin_format_time_string(vec![
        Value::string("%Y-%m-%d %H:%M:%S %z %Z"),
        Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
        Value::fixnum(3600),
    ]);
    assert_eq!(
        result.unwrap().as_utf8_str().unwrap(),
        "1970-01-01 01:00:00 +0100 +01"
    );
}

#[test]
fn broken_down_epoch() {
    crate::test_utils::init_test_tracing();
    let tm = unix_to_broken_down(0);
    assert_eq!(tm.year, 1970);
    assert_eq!(tm.month, 1);
    assert_eq!(tm.day, 1);
    assert_eq!(tm.hour, 0);
    assert_eq!(tm.minute, 0);
    assert_eq!(tm.second, 0);
    assert_eq!(tm.weekday, 4); // Thursday
}

#[test]
fn broken_down_y2k() {
    crate::test_utils::init_test_tracing();
    // 2000-01-01 00:00:00 UTC = 946684800
    let tm = unix_to_broken_down(946684800);
    assert_eq!(tm.year, 2000);
    assert_eq!(tm.month, 1);
    assert_eq!(tm.day, 1);
    assert_eq!(tm.weekday, 6); // Saturday
}

#[test]
fn broken_down_leap_year() {
    crate::test_utils::init_test_tracing();
    // 2000-02-29 00:00:00 UTC = 946684800 + 59*86400 = 946684800 + 5097600 = 951782400
    let tm = unix_to_broken_down(951782400);
    assert_eq!(tm.year, 2000);
    assert_eq!(tm.month, 2);
    assert_eq!(tm.day, 29);
}

#[test]
fn broken_down_end_of_day() {
    crate::test_utils::init_test_tracing();
    // 1970-01-01 23:59:59 = 86399
    let tm = unix_to_broken_down(86399);
    assert_eq!(tm.year, 1970);
    assert_eq!(tm.month, 1);
    assert_eq!(tm.day, 1);
    assert_eq!(tm.hour, 23);
    assert_eq!(tm.minute, 59);
    assert_eq!(tm.second, 59);
}

#[test]
fn broken_down_2024() {
    crate::test_utils::init_test_tracing();
    // 2024-03-15 12:30:45 UTC
    // Compute: days from 1970 to 2024-03-15
    // Using known: 2024-01-01 = 1704067200
    // Jan has 31 days, Feb has 29 (2024 is leap), so Mar 15 = 31 + 29 + 14 = 74 days after Jan 1
    // 1704067200 + 74 * 86400 = 1704067200 + 6393600 = 1710460800
    // + 12*3600 + 30*60 + 45 = 43200 + 1800 + 45 = 45045
    // Total: 1710505845
    let tm = unix_to_broken_down(1710505845);
    assert_eq!(tm.year, 2024);
    assert_eq!(tm.month, 3);
    assert_eq!(tm.day, 15);
    assert_eq!(tm.hour, 12);
    assert_eq!(tm.minute, 30);
    assert_eq!(tm.second, 45);
}

#[test]
fn format_time_string_r_directive_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // %r expands to the 12-hour clock with AM/PM (C-locale "%I:%M:%S %p").
    assert_eq!(fts("%r"), "12:34:56 PM");
}

#[test]
fn format_time_string_colon_z_family_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // UTC offset rendered with colon separators.
    assert_eq!(fts("%z"), "+0000");
    assert_eq!(fts("%:z"), "+00:00");
    assert_eq!(fts("%::z"), "+00:00:00");
    assert_eq!(fts("%:::z"), "+00");
}

#[test]
fn format_time_string_width_and_pad_flags_match_gnu() {
    crate::test_utils::init_test_tracing();
    // Field width with zero/space padding on numeric directives.
    assert_eq!(fts("%010Y"), "0000002021");
    assert_eq!(fts("%6m"), "000007");
    assert_eq!(fts("%08H"), "00000012");
    assert_eq!(fts("%03e"), "004");
    assert_eq!(fts("%_3d"), "  4");
    // `-` flag suppresses both padding and the field width.
    assert_eq!(fts("%-3S"), "56");
    assert_eq!(fts("%5S"), "00056");
    assert_eq!(fts("%_5S"), "   56");
    // `-` removes the default zero-padding.
    assert_eq!(fts("%-m"), "7");
    assert_eq!(fts("%-d"), "4");
    // Width applies to text directives too (space-padded on the left).
    assert_eq!(fts("%3p"), " PM");
}

#[test]
fn format_time_string_subsecond_flags_match_gnu() {
    crate::test_utils::init_test_tracing();
    // `(1 . 4)` = 1/4 s = 0.25 s, i.e. 250_000_000 ns.
    let quarter = Value::cons(Value::fixnum(1), Value::fixnum(4));
    let fts_n = |fmt: &str| -> String {
        builtin_format_time_string(vec![Value::string(fmt), quarter, Value::T])
            .unwrap()
            .as_utf8_str()
            .unwrap()
            .to_string()
    };

    // Plain %N: 9 digits, zero-padded on the right (GNU default).
    assert_eq!(fts_n("%N"), "250000000");
    // `-` (NoPad): strip trailing zeros, no padding.
    assert_eq!(fts_n("%-N"), "25");
    // `_` (SpacePad): strip trailing zeros, then space-pad to width 9.
    assert_eq!(fts_n("%_N"), "25       ");
    // `-` with explicit width caps the digits but still strips zeros, no pad.
    assert_eq!(fts_n("%-3N"), "25");
    // Width 3, default zero-pad: "25" + one trailing zero.
    assert_eq!(fts_n("%3N"), "250");
    // `0` flag behaves like the default (zero-pad).
    assert_eq!(fts_n("%0N"), "250000000");
    // Explicit width 9 == default.
    assert_eq!(fts_n("%9N"), "250000000");
    // Width beyond 9 zero-pads past the nanosecond resolution.
    assert_eq!(fts_n("%12N"), "250000000000");
    // `_` with explicit width 3: "25" + one trailing space.
    assert_eq!(fts_n("%_3N"), "25 ");
    // Later flag wins: `0-` ends as NoPad.
    assert_eq!(fts_n("%0-N"), "25");
}

#[test]
fn format_time_string_rejects_e_modifier_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `if (modifier == L_('E')) goto bad_format;`
    for conversion in [
        'B', 'G', 'H', 'I', 'M', 'N', 'S', 'U', 'V', 'W', 'b', 'd', 'e', 'g', 'h', 'j', 'k', 'l',
        'm', 'w',
    ] {
        let spec = format!("%E{conversion}");
        assert_eq!(
            fts_eo(&spec),
            spec,
            "%E{conversion} must be copied out literally, as GNU's bad_format does"
        );
    }
}

#[test]
fn format_time_string_rejects_o_modifier_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `if (modifier == L_('O')) goto bad_format;`
    for conversion in ['X', 'c', 'x'] {
        let spec = format!("%O{conversion}");
        assert_eq!(fts_eo(&spec), spec, "%O{conversion} must be literal");
    }
}

#[test]
fn format_time_string_rejects_both_modifiers_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `if (modifier != 0) goto bad_format;`
    //
    // `A` is spelled `case 'A':` in GNU's source while its neighbours use
    // `case L_('A'):`, which makes it easy to miss when reading the table off.
    for conversion in ['A', 'D', 'F', 'a'] {
        for modifier in ['E', 'O'] {
            let spec = format!("%{modifier}{conversion}");
            assert_eq!(
                fts_eo(&spec),
                spec,
                "%{modifier}{conversion} must be literal"
            );
        }
    }
}

#[test]
fn format_time_string_accepts_modifiers_that_gnu_allows() {
    crate::test_utils::init_test_tracing();
    // Accepted combinations format as the plain conversion in the C locale.
    assert_eq!(fts_eo("%Od"), "22");
    assert_eq!(fts_eo("%OH"), "14");
    assert_eq!(fts_eo("%OB"), "April");
    assert_eq!(fts_eo("%Oj"), "113");
    assert_eq!(fts_eo("%EY"), "2024");
    assert_eq!(fts_eo("%OY"), "2024");
    assert_eq!(fts_eo("%Ey"), "24");
    // `x` and `c` reject only `O`, so `E` goes through.
    assert_eq!(fts_eo("%Ex"), fts_eo("%x"));
    assert_eq!(fts_eo("%Ec"), fts_eo("%c"));
}

#[test]
fn format_time_string_modifier_rejection_keeps_surrounding_text() {
    crate::test_utils::init_test_tracing();
    // `bad_format` emits only the offending directive; the rest of the
    // format string is still processed.
    assert_eq!(fts_eo("[%Ed] %Y"), "[%Ed] 2024");
    assert_eq!(fts_eo("%Y-%Em-%Od"), "2024-%Em-22");
}

fn fts(fmt: &str) -> String {
    builtin_format_time_string(vec![
        Value::string(fmt),
        Value::fixnum(1625402096),
        Value::T,
    ])
    .unwrap()
    .as_utf8_str()
    .unwrap()
    .to_string()
}

/// Format `spec` against a fixed UTC instant (2024-04-22 14:32:48).
fn fts_eo(spec: &str) -> String {
    let result = builtin_format_time_string(vec![
        Value::string(spec),
        Value::fixnum(1_713_796_368),
        Value::T,
    ]);
    result.unwrap().as_utf8_str().unwrap().to_string()
}
