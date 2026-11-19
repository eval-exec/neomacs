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

/// PEER 5799 W2: GNU's `define-coding-system-alias` also creates the
/// alias's `-unix`/`-dos`/`-mac` subsidiaries and aliases each to the
/// base's subsidiary (`Fdefine_coding_system_alias`, src/coding.c:
/// 11581-11606). They are ordinary Lisp-visible coding systems, so both
/// coding-system views and the derived names must match GNU. The function
/// view counts non-subsidiary systems only; the variable holds every name.
#[test]
fn coding_system_alias_subsidiaries_match_gnu_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list
  (length (coding-system-list))
  (length coding-system-list)
  (mapcar (lambda (name)
            (let ((s (intern-soft name)))
              (list (and s t)
                    (and s (coding-system-base s))
                    (and s (coding-system-eol-type s)))))
          '("ascii-unix" "cn-big5-dos" "big5-hkscs-mac")))"#;
    let expected = expect_test::expect![[
        r#""OK (269 1071 ((t us-ascii 0) (t chinese-big5 1) (t chinese-big5-hkscs 2)))""#
    ]];
    common::assert_oracle_parity_expect(form, expected);
}

/// GNU `make-symbol` returns a fresh uninterned object that is not the
/// obarray's symbol for the name: `intern-soft` answers nil for it. The
/// member funnels (`intern`, `intern_member`) never produce this state.
#[test]
fn make_symbol_is_not_the_obarray_symbol_oracle() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (make-symbol "peer-5786-uninterned")))
  (list (eq s (intern-soft "peer-5786-uninterned"))
        (intern-soft "peer-5786-uninterned")))"#;
    let expected = expect_test::expect![[r#""OK (nil nil)""#]];
    common::assert_oracle_parity_expect(form, expected);
}
