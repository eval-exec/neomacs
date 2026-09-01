//! Complex combo batch 453 — 5 proptest fuzz batches generating random
//! Elisp forms to surface new divergence patterns via property testing.

use crate::common::{ORACLE_PROP_CASES, assert_oracle_parity};
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn div_cx453_proptest_arithmetic(
        a in -1000..1000i64,
        b in -1000..1000i64,
        op in 0..4u8,
    ) {
        let form = match op {
            0 => format!("(+ {} {})", a, b),
            1 => format!("(- {} {})", a, b),
            2 => format!("(* {} {})", a, b),
            _ => {
                if b == 0 { format!("(/ {} 1)", a) }
                else { format!("(/ {} {})", a, b) }
            }
        };
        assert_oracle_parity(&form);
    }

    #[test]
    fn div_cx453_proptest_string_ops(
        s in "[a-zA-Z]{0,10}",
        start in 0usize..10,
        len in 0usize..10,
    ) {
        let end = start + len;
        let form = format!(
            "(list (length \"{}\") (string-to-char \"{}\") (substring-no-properties \"{}\" {} {}))",
            s, s, s, start.min(10), end.min(10)
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn div_cx453_proptest_char_ops(c in 32u8..127u8) {
        let form = format!(
            "(list (char-to-string {}) (string-to-char (char-to-string {})))",
            c, c
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn div_cx453_proptest_comparisons(a in -100..100i64, b in -100..100i64) {
        let form = format!(
            "(list (= {} {}) (< {} {}) (> {} {}) (/= {} {}))",
            a, b, a, b, a, b, a, b
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn div_cx453_proptest_buffer_ops(pos in 1usize..20, insert in "[a-z]{1,5}") {
        let form = format!(
            "(with-temp-buffer\n  (insert \"abcdefghijklmnopqrst\")\n  (goto-char {})\n  (insert \"{}\")\n  (buffer-string))",
            pos.min(20), insert
        );
        assert_oracle_parity(&form);
    }
}
