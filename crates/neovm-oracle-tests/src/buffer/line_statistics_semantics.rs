//! Oracle parity tests for GNU `buffer-line-statistics` semantics.
//!
//! GNU implements `Fbuffer_line_statistics` in `src/fns.c` by scanning raw
//! buffer bytes for `\n`.  Line lengths are byte counts, so a preceding `\r`
//! in CRLF text is counted as part of the line.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_buffer_line_statistics_counts_raw_crlf_bytes_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((b (get-buffer-create " *bls-oracle*")))
  (with-current-buffer b
    (erase-buffer)
    (insert "a\r\nbb\nccc")
    (list
     (buffer-line-statistics)
     (progn
       (erase-buffer)
       (insert "a\nb\n")
       (buffer-line-statistics))
     (progn
       (erase-buffer)
       (insert "é\nxx")
       (buffer-line-statistics)))))
"#;

    let expect = expect_test::expect![r#""OK ((3 3 2.3333333333333335) (2 1 1.0) (2 2 2.0))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// GNU 31.1 `src/fns.c:Fbuffer_line_statistics` scans from BUF_BEG to BUF_Z,
/// including text outside the accessible region, and measures byte widths.
#[test]
fn oracle_buffer_line_statistics_ignores_narrowing_and_counts_multibyte_bytes() {
    let form = r#"
(mapcar
 (lambda (fixture)
   (with-temp-buffer
     (insert (nth 0 fixture))
     (let ((whole (buffer-line-statistics)))
       (narrow-to-region (nth 1 fixture) (nth 2 fixture))
       (let ((narrowed
              (list (buffer-line-statistics)
                    (buffer-line-statistics nil)
                    (buffer-line-statistics (current-buffer))
                    (buffer-line-statistics (buffer-name))
                    (list (point-min) (point-max)))))
         (widen)
         (list whole narrowed (buffer-line-statistics))))))
 '(("abcdefghij\nb\ncccc\nd" 12 18)
   ("ééé\nz\n界界界\nqq" 5 10)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((4 10 4.0) ((4 10 4.0) (4 10 4.0) (4 10 4.0) (4 10 4.0) (12 18)) (4 10 4.0)) ((4 9 4.5) ((4 9 4.5) (4 9 4.5) (4 9 4.5) (4 9 4.5) (5 10)) (4 9 4.5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
