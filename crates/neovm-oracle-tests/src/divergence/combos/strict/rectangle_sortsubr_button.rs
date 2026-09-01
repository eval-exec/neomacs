//! Strict combo oracle probes, batch 59: genuinely-untested deterministic
//! areas — rectangle operations (extract/string/delete/apply-on-rectangle),
//! sort-subr (low-level sort), and button.el text-button creation.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_m0_extract_and_string_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"aa\" \"bb\") \"aXYa\\nbXYb\\ncccc\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert "aaaa\nbbbb\ncccc\n")
        (extract-rectangle 2 9))
      (with-temp-buffer
        (insert "aaaa\nbbbb\ncccc\n")
        (string-rectangle 2 9 "XY")
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_m0_delete_and_apply_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aa\\nbb\\ncccc\\n\" ((1 3) (1 3)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((del (with-temp-buffer
             (insert "aaaa\nbbbb\ncccc\n")
             (delete-rectangle 2 9)
             (buffer-string))))
  (let ((log nil))
    (with-temp-buffer
      (insert "aaaa\nbbbb\ncccc\n")
      (apply-on-rectangle
        (lambda (beg end) (push (list beg end) log))
        2 9))
    (list del (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_m0_rectangle_corners_and_clear() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a   efg\\nabcdefg\\nabcdefg\\n\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert "abcdefg\nabcdefg\nabcdefg\n")
        (clear-rectangle 2 5)
        (buffer-string))
      (with-temp-buffer
        (insert "abcdefg\nabcdefg\nabcdefg\n")
        (copy-rectangle-as-kill 2 8)
        (car-safe kill-ring)))
"##,
        expect,
    );
}

#[test]
fn div_m0_sort_subr_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"3\\n1\\n2\\n5\\n4\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "3\n1\n2\n5\n4\n")
  (sort-subr nil
             (lambda (pos) (and pos (not (eobp))))
             #'forward-line
             (lambda () (string-to-number
                         (buffer-substring (point) (line-end-position)))))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_m0_button_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Unknown button type ‘help-variable’\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "text here")
  (let ((b (make-text-button 1 4 'type 'help-variable 'help-args '(probe-var))))
    (list (buttonp b)
          (button-has-type-p b 'help-variable)
          (button-start b)
          (button-end b)
          (button-get b 'help-args))))
"##,
        &["button.el"],
        expect,
    );
}

#[test]
fn div_m0_sort_subr_reverse_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"banana\\napple\\ncherry\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "banana\napple\ncherry\n")
  (sort-subr t
             (lambda (pos) (and pos (not (eobp))))
             #'forward-line
             (lambda () (buffer-substring-no-properties
                         (point) (line-end-position))))
  (buffer-string))
"##,
        expect,
    );
}
