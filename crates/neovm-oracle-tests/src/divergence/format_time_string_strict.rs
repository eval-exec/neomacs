//! format-time-string parity strict tests + documented gaps in the
//! padding/flag/specifier handling added by the recent
//! "support _ 0 ^ # flags" change. Fixed UTC times keep output stable.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn fts_a_b_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Mon Apr 22 2024\" \"Monday, April 22\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%a %b %e %Y" '(26150 29968) t) (format-time-string "%A, %B %d" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_misc_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"20\" \"113\" \"02:32 PM\" \"14\" \" 2\" \"\\n\t\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%C" '(26150 29968) t) (format-time-string "%j" '(26150 29968) t) (format-time-string "%I:%M %p" '(26150 29968) t) (format-time-string "%k" '(26150 29968) t) (format-time-string "%l" '(26150 29968) t) (format-time-string "%n%t" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_padding_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"22\" \"22\" \"22\" \"22\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%_d" '(26150 29968) t) (format-time-string "%-d" '(26150 29968) t) (format-time-string "%0e" '(26150 29968) t) (format-time-string "%e" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_percent_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"100%\" \"%Y=2024\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "100%%" '(26150 29968) t) (format-time-string "%%Y=%Y" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_week_iso() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"2024\" \"17\" \"1\" \"1\" \"16\" \"17\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%G" '(26150 29968) t) (format-time-string "%V" '(26150 29968) t) (format-time-string "%u" '(26150 29968) t) (format-time-string "%w" '(26150 29968) t) (format-time-string "%U" '(26150 29968) t) (format-time-string "%W" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_dash_nopad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"48\" \"14\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%-S" '(26150 29968) t) (format-time-string "%-H" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_h_month() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"Apr\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format-time-string "%h" '(26150 29968) t)"##,
        expect,
    );
}

#[test]
fn fts_x_X_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"04/22/24\" \"14:32:48\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%x" '(26150 29968) t) (format-time-string "%X" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn fts_p_lower_ampm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"pm\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format-time-string "%P" '(26150 29968) t)"##,
        expect,
    );
}

#[test]
fn divergence_fts_pct_n_nanoseconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"000000000\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format-time-string "%N" '(26150 29968) t)"##,
        expect,
    );
}

#[test]
fn divergence_fts_pct_r_12hour() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"02:32:48 PM\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format-time-string "%r" '(26150 29968) t)"##,
        expect,
    );
}

#[test]
fn divergence_fts_colon_z_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"+00:00\" \"+00:00:00\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%:z" '(26150 29968) t) (format-time-string "%::z" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn divergence_fts_hash_flag_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"MON\" \"MONDAY\" \"APRIL\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%#a" '(26150 29968) t) (format-time-string "%#A" '(26150 29968) t) (format-time-string "%#B" '(26150 29968) t))"##,
        expect,
    );
}

#[test]
fn divergence_fts_invalid_e_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"%Ed\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(format-time-string "%Ed" '(26150 29968) t)"##,
        expect,
    );
}

#[test]
fn divergence_fts_width_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"002024\" \"014\" \"00048\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%6Y" '(26150 29968) t) (format-time-string "%03H" '(26150 29968) t) (format-time-string "%5S" '(26150 29968) t))"##,
        expect,
    );
}
