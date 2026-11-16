//! PEER 5786 W1: Lisp-visible startup symbols must be obarray members.
//!
//! GNU interns every `features` entry and every name on the coding-system
//! paths. Two different Lisp views exist:
//! - `(coding-system-list)` is the FUNCTION, which by design omits the
//!   auto-created `-unix`/`-dos`/`-mac` subsidiaries (mule.el:1099-1114);
//! - the `coding-system-list` VARIABLE holds every base, alias and
//!   subsidiary. GNU creates the subsidiaries with `intern` in
//!   `make_subsidiaries` (src/coding.c:10968-10984), so every element is an
//!   obarray member.
//! The form counts non-members in all three views and both editors must
//! answer zero. List SIZES differ because Neomacs covers fewer systems, so
//! this row pins membership only.
#![allow(dead_code)]
use crate::common;

#[test]
fn features_and_coding_systems_are_obarray_members_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  (length (delq t (mapcar (lambda (sym) (eq sym (intern-soft (symbol-name sym))))
                          features)))
  (length (delq t (mapcar (lambda (sym) (eq sym (intern-soft (symbol-name sym))))
                          (coding-system-list))))
  (length (delq t (mapcar (lambda (sym) (eq sym (intern-soft (symbol-name sym))))
                          coding-system-list))))"#;
    let expected = expect_test::expect![[r#""OK (0 0 0)""#]];
    common::assert_oracle_parity_expect(form, expected);
}
