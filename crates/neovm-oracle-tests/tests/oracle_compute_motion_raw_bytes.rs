//! `compute-motion` over raw bytes: one before the span, one inside it, and
//! a byte >= 0x80 of a unibyte buffer.  GNU displays each such byte as
//! `\NNN`, four columns (indent.c `compute_motion`).
use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn compute_motion_counts_raw_bytes_by_length_and_display_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(list
             (with-temp-buffer
               (insert (string-to-multibyte "x\200abc\tdef"))
               (compute-motion 3 '(0 . 0) (point-max) '(80 . 0) 80 nil nil))
             (with-temp-buffer
               (insert (string-to-multibyte "abc\200d\tef"))
               (compute-motion 1 '(0 . 0) (point-max) '(80 . 0) 80 nil nil))
             (with-temp-buffer
               (set-buffer-multibyte nil)
               (insert "a\t\351b")
               (compute-motion 1 '(0 . 0) (point-max) '(80 . 0) 80 nil nil)))"#,
        expect_test::expect![[r#""OK ((10 11 0 10 nil) (9 18 0 17 nil) (5 13 0 12 nil))""#]],
    );
}

/// A multibyte character wider than one column that runs past WIDTH moves
/// whole to the next line, so the continuation subtracts the column it
/// started at; a unibyte byte >= 0x80 subtracts WIDTH (indent.c
/// `compute_motion`, `wide_column_end_hpos`).
#[test]
fn compute_motion_continues_wide_multibyte_characters_whole() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((truncate-partial-width-windows nil))
             (list
              (with-temp-buffer
                (insert (string-to-multibyte "abc\200"))
                (compute-motion 1 '(0 . 0) (point-max) '(100 . 100) 4 nil nil))
              (with-temp-buffer
                (insert "abc漢")
                (compute-motion 1 '(0 . 0) (point-max) '(100 . 100) 4 nil nil))
              (with-temp-buffer
                (set-buffer-multibyte nil)
                (insert "abc\200")
                (compute-motion 1 '(0 . 0) (point-max) '(100 . 100) 4 nil nil))))"#,
        expect_test::expect![[r#""OK ((5 4 1 3 t) (5 2 1 3 t) (5 3 1 3 t))""#]],
    );
}
