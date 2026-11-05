use crate::emacs_core::autoload::is_autoload_value;

use crate::test_utils::{eval_with_ldefs_boot_autoloads, runtime_startup_eval_all};

use neovm_text::emacs_core::format::*;

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

#[test]
fn format_time_string_timezone() {
    crate::test_utils::init_test_tracing();
    let result = builtin_format_time_string(vec![Value::string("%Z"), Value::fixnum(0), Value::T]);
    assert_eq!(
        result.unwrap().as_utf8_str().unwrap(),
        crate::test_utils::host_utc_zone_abbreviation()
    );
}

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

#[test]
fn string_pixel_width_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["string-pixel-width"]);
    let function = eval
        .obarray
        .symbol_function("string-pixel-width")
        .expect("missing string-pixel-width startup function cell");
    assert!(is_autoload_value(
        &function,
        neovm_value::tagged::value::SymbolMode::Bare
    ));
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
    // X-LIMIT replaces the row edge but NOT the window's `line_wrap`
    // (`it.last_visible_x = max_x`, src/xdisp.c:11924), so a 4-pixel limit in a
    // 1x1-cell batch window wraps "abcdef" into four rows of one character
    // rather than truncating it.  GNU 31.1 in batch answers `(4 . 3)` for this
    // exact form and `(4 . 2)` only with `truncate-lines` non-nil; on an X
    // frame with a 9x20 cell it answers `(4 . 200)` and `(4 . 40)`.
    assert_eq!(results[8], "OK (4 . 3)");
    assert_eq!(results[9], "OK nil");
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
    // `%#Z` lowercases the zone abbreviation; the UTC zone's abbreviation is
    // the platform gmtime's ("GMT" on glibc, "UTC" on Darwin).
    assert_eq!(
        fts("%#Z"),
        crate::test_utils::host_utc_zone_abbreviation().to_lowercase()
    );
    // `^` uppercases.
    assert_eq!(fts("%^a"), "SUN");
    assert_eq!(fts("%^p"), "PM");
    // %P is always lowercase even with `^`.
    assert_eq!(fts("%^P"), "pm");
}

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

fn bootstrap_eval(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
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
