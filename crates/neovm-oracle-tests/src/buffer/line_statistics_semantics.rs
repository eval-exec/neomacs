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
