use super::*;
use crate::emacs_core::autoload::is_autoload_value;
use crate::test_utils::{eval_with_ldefs_boot_autoloads, runtime_startup_eval_all};

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

// ===================================================================
// format-spec tests
// ===================================================================

#[test]
fn format_spec_bootstrap_matches_gnu_elisp() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (format-spec "%n is %a" '((?n . "Bob") (?a . "21")))
        (format-spec "100%% done" nil)
        (format-spec "[%10n]" '((?n . "hi")))
        (format-spec "[%-10n]" '((?n . "hi")))
        (format-spec "[%05n]" '((?n . "42")))
        (condition-case err (format-spec "hello %x world" nil) (error (car err)))
        (format-spec "hello %x world" nil 'ignore)
        (condition-case err (format-spec "hi") (error (car err)))
        "#,
    );
    assert_eq!(results[0], r#"OK "Bob is 21""#);
    assert_eq!(results[1], r#"OK "100% done""#);
    assert_eq!(results[2], r#"OK "[        hi]""#);
    assert_eq!(results[3], r#"OK "[hi        ]""#);
    assert_eq!(results[4], r#"OK "[00042]""#);
    assert_eq!(results[5], "OK error");
    assert_eq!(results[6], r#"OK "hello %x world""#);
    assert_eq!(results[7], "OK wrong-number-of-arguments");
}

#[test]
fn format_percent_s_uses_recursive_princ_semantics_for_lists() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (format "%s" '("development" "testing" "production"))
        "#,
    );
    assert_eq!(results[0], r#"OK "(development testing production)""#);
}

// ===================================================================
// format-time-string tests
// ===================================================================

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
fn format_time_string_timezone() {
    crate::test_utils::init_test_tracing();
    let result = builtin_format_time_string(vec![Value::string("%Z"), Value::fixnum(0), Value::T]);
    assert_eq!(result.unwrap().as_utf8_str().unwrap(), "GMT");
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

// ===================================================================
// format-seconds tests
// ===================================================================

#[test]
fn format_seconds_bootstrap_matches_gnu_elisp() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (format-seconds "%h:%m:%s" 3661)
        (format-seconds "%d days, %h:%m:%s" 90061)
        (format-seconds "%h:%m:%s" 0)
        (format-seconds "100%%" 0)
        "#,
    );
    assert_eq!(results[0], r#"OK "1:1:1""#);
    assert_eq!(results[1], r#"OK "1 days, 1:1:1""#);
    assert_eq!(results[2], r#"OK "0:0:0""#);
    assert_eq!(results[3], r#"OK "100%""#);
}

// ===================================================================
// subr-x string helper tests
// ===================================================================

#[test]
fn subr_x_string_helpers_bootstrap_match_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (load "subr-x")
        (special-variable-p 'fill-column)
        (let ((pad (symbol-function 'string-pad))
              (limit (symbol-function 'string-limit))
              (glyph (symbol-function 'string-glyph-split)))
          (list (subrp pad)
                (subrp limit)
                (subrp glyph)
                (funcall pad "x" 4 ?0 t)
                (funcall limit "abcd" 3 t)
                (funcall glyph "abc")))
        (string-fill "x" 2)
        (string-fill "aa bb ccc d" 5)
        (string-fill "a b\n\nc d" 10)
        (condition-case err (string-fill 1 2) (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], r#"OK (nil nil nil "000x" "bcd" ("a" "b" "c"))"#);
    assert_eq!(results[3], r#"OK "x""#);
    assert_eq!(results[4], "OK \"aa bb\nccc d\"");
    assert_eq!(results[5], "OK \"a b\n\nc d\"");
    assert_eq!(results[6], "OK \"\u{1}\"");
}

#[test]
fn subr_x_string_helpers_autoload() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (let ((before-pad (symbol-function 'string-pad))
              (before-limit (symbol-function 'string-limit))
              (before-glyph (symbol-function 'string-glyph-split)))
          (list (autoloadp before-pad)
                (autoloadp before-limit)
                (autoloadp before-glyph)
                (string-pad "x" 2)
                (string-limit "abcd" 2)
                (string-glyph-split "abc")
                (autoloadp (symbol-function 'string-pad))
                (autoloadp (symbol-function 'string-limit))
                (autoloadp (symbol-function 'string-glyph-split))
                (subrp (symbol-function 'string-pad))
                (subrp (symbol-function 'string-limit))
                (subrp (symbol-function 'string-glyph-split))))
        "#,
    );
    // GNU `.elc` loading folds eval-when-compile to a constant, so
    // string-pad / string-limit / string-glyph-split stay autoloaded
    // until first use. Now that NeoMacs prefers `.elc` (since .elc
    // loading was enabled), autoloadp returns t for the initial
    // before-pad/limit/glyph values, then nil after the first call
    // resolves the autoload.
    assert_eq!(
        results[0],
        r#"OK (t t t "x " "ab" ("a" "b" "c") nil nil nil nil nil nil)"#
    );
}

// ===================================================================
// string-lines tests
// ===================================================================

#[test]
fn string_lines_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'string-lines))
        (string-lines "a\nb\nc")
        (string-lines "a\nb\n")
        (string-lines "a\n\nb\n" t)
        (string-lines "")
        (string-lines "" t)
        (string-lines "a\n\nb\n" nil t)
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK ("a" "b" "c")"#);
    assert_eq!(results[2], r#"OK ("a" "b")"#);
    assert_eq!(results[3], r#"OK ("a" "b")"#);
    assert_eq!(results[4], r#"OK ("")"#);
    assert_eq!(results[5], "OK nil");
    assert_eq!(results[6], "OK (\"a\n\" \"\n\" \"b\n\")");
}

// ===================================================================
// string-clean-whitespace tests
// ===================================================================

#[test]
fn string_clean_whitespace_bootstrap_matches_gnu_elisp() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (string-clean-whitespace "  hello   world  ")
        (string-clean-whitespace "a\t\tb\n\nc")
        (string-clean-whitespace "hello world")
        (string-clean-whitespace "")
        (string-clean-whitespace "   ")
        (condition-case err (string-clean-whitespace 1) (error (car err)))
        "#,
    );
    assert_eq!(results[0], r#"OK "hello world""#);
    assert_eq!(results[1], r#"OK "a b c""#);
    assert_eq!(results[2], r#"OK "hello world""#);
    assert_eq!(results[3], "OK \"\"");
    assert_eq!(results[4], "OK \"\"");
    assert_eq!(results[5], "OK wrong-type-argument");
}

// ===================================================================
// string-pixel-width tests
// ===================================================================

#[test]
fn string_pixel_width_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["string-pixel-width"]);
    let function = eval
        .obarray
        .symbol_function("string-pixel-width")
        .expect("missing string-pixel-width startup function cell");
    assert!(is_autoload_value(&function));
}

#[test]
fn string_pixel_width_bootstrap_matches_gnu_subr_x() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (string-pixel-width "hello")
        (string-pixel-width "")
        (string-pixel-width "\t")
        (string-pixel-width "a\t")
        (string-pixel-width "a\tb")
        (string-pixel-width "漢字")
        (string-pixel-width "é")
        (with-temp-buffer
          (insert "abc\ndef")
          (buffer-text-pixel-size nil nil t))
        (with-temp-buffer
          (insert "abcdef\n123")
          (buffer-text-pixel-size nil nil 4))
        (subrp (symbol-function 'string-pixel-width))
        "#,
    );
    assert_eq!(results[0], "OK 5");
    assert_eq!(results[1], "OK 0");
    assert_eq!(results[2], "OK 8");
    assert_eq!(results[3], "OK 8");
    assert_eq!(results[4], "OK 9");
    assert_eq!(results[5], "OK 4");
    assert_eq!(results[6], "OK 1");
    assert_eq!(results[7], "OK (3 . 2)");
    assert_eq!(results[8], "OK (4 . 2)");
    assert_eq!(results[9], "OK nil");
}

// unix_to_broken_down internal tests
// ===================================================================

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

// ===================================================================
// format-time-string directive parser parity with GNU nstrftime
// (lib/strftime.c). Fixed time 1625402096 = 2021-07-04 12:34:56 UTC,
// a Sunday. Each expected value was captured from GNU Emacs --batch.
// ===================================================================

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
fn format_time_string_hash_case_flag_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // `#` uppercases text directives (NOT a per-char case swap), and
    // lowercases %p / %Z.
    assert_eq!(fts("%#a"), "SUN");
    assert_eq!(fts("%#A"), "SUNDAY");
    assert_eq!(fts("%#b"), "JUL");
    assert_eq!(fts("%#B"), "JULY");
    assert_eq!(fts("%#p"), "pm");
    assert_eq!(fts("%#Z"), "gmt");
    // `^` uppercases.
    assert_eq!(fts("%^a"), "SUN");
    assert_eq!(fts("%^p"), "PM");
    // %P is always lowercase even with `^`.
    assert_eq!(fts("%^P"), "pm");
}

#[test]
fn format_time_string_subsecond_flags_match_gnu() {
    crate::test_utils::init_test_tracing();
    // `(1 . 4)` = 1/4 s = 0.25 s, i.e. 250_000_000 ns.
    let quarter = Value::cons(Value::fixnum(1), Value::fixnum(4));
    let fts_n = |fmt: &str| -> String {
        builtin_format_time_string(vec![Value::string(fmt), quarter.clone(), Value::T])
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

// ===================================================================
// `E` / `O` locale modifiers
//
// GNU `lib/strftime.c` does not ignore these: each conversion decides
// whether it accepts one, and a conversion that does not takes
// `goto bad_format`, which copies the WHOLE directive out literally
// (`cpy (f - percent + 1, percent)`).  So `%Ed` yields the four-character
// string "%Ed", not the day of month.
//
// In the C locale every ACCEPTED combination falls back to the plain
// conversion (the `_NL_CURRENT` era strings are empty), so only the
// rejections are observable.  Verified against GNU Emacs for all
// conversion characters x {E, O}.
// ===================================================================

/// Format `spec` against a fixed UTC instant (2024-04-22 14:32:48).
fn fts_eo(spec: &str) -> String {
    let result = builtin_format_time_string(vec![
        Value::string(spec),
        Value::fixnum(1_713_796_368),
        Value::T,
    ]);
    result.unwrap().as_utf8_str().unwrap().to_string()
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

// ===================================================================
// `define-coding-system-alias`
//
// GNU registers the alias as a KEY in the same table as its target,
// pointing at the SAME spec (`Fputhash (alias, spec,
// Vcoding_system_hash_table)`, src/coding.c), so every lookup resolves
// it -- encoders and decoders included.  neomacs selects a codec by
// matching the NAME against a table of built-in systems, which a
// user-defined alias falls through, leaving no codec and encoding
// non-ASCII as `?`.
// ===================================================================

#[test]
fn coding_system_alias_encodes_through_its_target() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let _ = ev.eval_str("(define-coding-system-alias 'neo-alias-utf8 'utf-8)");

    // Was `(116 63 115 116)` -- `63` is `?`, i.e. silent data loss.
    let encoded = ev.eval_str("(append (encode-coding-string \"t\\u00ebst\" 'neo-alias-utf8) nil)");
    assert_eq!(
        crate::emacs_core::format_eval_result(&encoded),
        "OK (116 195 171 115 116)"
    );

    let roundtrip = ev.eval_str(
        "(let ((s \"t\\u00ebst\")) \
           (string= s (decode-coding-string (encode-coding-string s 'neo-alias-utf8) \
                                            'neo-alias-utf8)))",
    );
    assert_eq!(crate::emacs_core::format_eval_result(&roundtrip), "OK t");
}

#[test]
fn coding_system_alias_of_a_charset_system_encodes_through_its_target() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let _ = ev.eval_str("(define-coding-system-alias 'neo-alias-8859-15 'iso-8859-15)");
    let encoded = ev.eval_str("(append (encode-coding-string \"\\u00e9\" 'neo-alias-8859-15) nil)");
    assert_eq!(crate::emacs_core::format_eval_result(&encoded), "OK (233)");
}

#[test]
fn coding_system_alias_is_reported_verbatim_in_last_coding_system_used() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let _ = ev.eval_str("(define-coding-system-alias 'neo-alias-reported 'utf-8)");
    // GNU stores `CODING_ID_NAME (coding.id)` -- the name the caller passed,
    // NOT the resolved base -- so resolving for codec selection must not leak
    // into this variable.
    let last = ev.eval_str(
        "(progn (encode-coding-string \"t\\u00ebst\" 'neo-alias-reported) \
           last-coding-system-used)",
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&last),
        "OK neo-alias-reported"
    );
}
