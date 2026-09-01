//! Strict combo oracle probes, batch 82: studlify-region (StUdLyCaPs) and
//! morse-region/unmorse-region (Morse code conversion). Deterministic string
//! transformations.
//!
//! Both libraries live under `lisp/play/` (GNU layout); the probes load them
//! from there so the conversions are actually exercised instead of
//! degenerating into `load` file-missing locks.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p6_studlify_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Hello WoRld foo BAr\"""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "Hello World Foo Bar")
  (studlify-region (point-min) (point-max))
  (buffer-string))
"##,
        &["play/studly.el"],
        expect,
    );
}

#[test]
fn div_p6_morse_and_unmorse_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"...././.-../.-../---\" \"h e l l o\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (with-temp-buffer
        (insert "HELLO")
        (morse-region (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (insert ".... . .-.. .-.. ---")
        (unmorse-region (point-min) (point-max))
        (buffer-string)))
"##,
        &["play/morse.el"],
        expect,
    );
}
