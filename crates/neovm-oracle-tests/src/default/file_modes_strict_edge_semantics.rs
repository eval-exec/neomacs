//! Oracle parity tests for GNU default file mode semantics.
//!
//! GNU implements `default-file-modes` and `set-default-file-modes` in
//! `src/fileio.c`.  `set-default-file-modes` stores `~MODE & 0777`, so only
//! the low 9 permission bits are observable through `default-file-modes`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_default_file_modes_low_bits_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((original (default-file-modes)))
  (unwind-protect
      (let (results)
        (push (condition-case err
                  (default-file-modes 'extra)
                (error (list (car err) (cdr err))))
              results)
        (push (condition-case err
                  (set-default-file-modes)
                (error (list (car err) (cdr err))))
              results)
        (push (condition-case err
                  (set-default-file-modes nil)
                (error (list (car err) (cdr err))))
              results)
        (push (set-default-file-modes #o700) results)
        (push (default-file-modes) results)
        (push (set-default-file-modes #o1777) results)
        (push (default-file-modes) results)
        (push (set-default-file-modes -1) results)
        (push (default-file-modes) results)
        (nreverse results))
    (set-default-file-modes original)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments (default-file-modes 1)) (wrong-number-of-arguments (set-default-file-modes 0)) (wrong-type-argument (fixnump nil)) nil 448 nil 511 nil 511)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
