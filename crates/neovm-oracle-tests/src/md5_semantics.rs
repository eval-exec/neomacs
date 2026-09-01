//! Oracle parity tests for GNU `md5` primitive semantics.
//!
//! GNU implements `Fmd5` in `src/fns.c` by forwarding to `secure_hash` with
//! the optional CODING-SYSTEM and NOERROR arguments.  The digest is computed
//! after encoding text with the requested coding system, not from the internal
//! string bytes alone.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_md5_string_honors_coding_system_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (md5 "é")
 (md5 "é" nil nil 'utf-8)
 (md5 "é" nil nil 'utf-16le)
 (condition-case err
     (md5 "é" nil nil 'unknown-coding-system)
   (error (list (car err) (cdr err))))
 (md5 "é" nil nil 'unknown-coding-system t))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"66ddcd97cfdeabb2f6fb8a999b4bc76f\" \"66ddcd97cfdeabb2f6fb8a999b4bc76f\" \"ed71e8ffd3d8c47c1a2e22c53cd384aa\" (coding-system-error (unknown-coding-system)) \"66ddcd97cfdeabb2f6fb8a999b4bc76f\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
