//! Strict combo oracle probes, batch 236: paragraph / sentence / page
//! navigation. forward/backward-paragraph, forward/backward-sentence,
//! forward-page, and paragraph-start/sentence-end boundary detection.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_forward_backward_paragraph_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n")
  (goto-char 1)
  (let ((p1 (progn (forward-paragraph) (point)))
        (p2 (progn (forward-paragraph) (point)))
        (back (progn (backward-paragraph) (point))))
    (list p1 p2 back)))
"##;
    let expect = expect_test::expect![[r#""OK (18 37 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_forward_backward_sentence_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "First sentence. Second sentence! Third one? End.\n")
  (goto-char 1)
  (let ((s1 (progn (forward-sentence) (point)))
        (s2 (progn (forward-sentence) (point)))
        (s3 (progn (forward-sentence) (point)))
        (back (progn (backward-sentence) (point))))
    (list s1 s2 s3 back)))
"##;
    let expect = expect_test::expect![[r#""OK (49 50 50 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_page_delimiter_forward_page() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "Page one content.\n\nPage two content.\n")
  (goto-char 1)
  (let ((first-page (progn (forward-page) (point)))
        (at-delimiter (char-before)))
    (list first-page at-delimiter)))
"##;
    let expect = expect_test::expect![[r#""OK (20 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
