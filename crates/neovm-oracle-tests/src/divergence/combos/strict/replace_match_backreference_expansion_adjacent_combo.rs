//! Strict combo oracle probes, batch 192: replace-match backreference
//! expansion variants, isolating the consecutive-backref divergence surfaced
//! in batch 190. Tests \N\N adjacency, backref reuse, literal+backref mixing,
//! \& whole-match, and the match-substitute-replacement elisp wrapper, to pin
//! the exact surface of the replace backreference expansion bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_replace_match_backref_reorder_adjacent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (let ((s "abc")) (string-match "\\(.\\)\\(.\\)\\(.\\)" s)
        (replace-match "\\3\\2\\1" nil nil nil s))
      (let ((s "abcdef")) (string-match "\\(..\\)\\(..\\)\\(..\\)" s)
        (replace-match "\\1\\3\\2" nil nil nil s))
      (let ((s "abc")) (string-match "\\(.\\)\\(.\\)\\(.\\)" s)
        (replace-match "\\1\\2\\3" nil nil nil s))
      (let ((s "abc")) (string-match "\\(.\\)\\(.\\)\\(.\\)" s)
        (replace-match "[\\1\\2\\3]" nil nil nil s))
      (let ((s "xy")) (string-match "\\(.\\)\\(.\\)" s)
        (replace-match "X\\1\\2Y" nil nil nil s))
      (let ((s "xy")) (string-match "\\(.\\)\\(.\\)" s)
        (replace-match "\\1\\2" nil nil nil s))
      (let ((s "e5")) (string-match "\\(.\\)\\(.\\)" s)
        (replace-match "[\\1\\2]" nil nil nil s))
      (let ((s "e5")) (string-match "\\(.\\)\\(.\\)" s)
        (replace-match "[\\1][\\2]" nil nil nil s)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_replace_match_whole_amp_reuse_literal_mix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (let ((s "hello")) (string-match "ell" s)
        (replace-match "[\\&]" nil nil nil s))
      (let ((s "hello")) (string-match "ell" s)
        (replace-match "<\\&>" nil nil nil s))
      (let ((s "abc")) (string-match "\\(.\\)\\(.\\)\\(.\\)" s)
        (replace-match "\\1-\\1-\\3" nil nil nil s))
      (let ((s "ab")) (string-match "\\(.\\)\\(.\\)" s)
        (replace-match "ab" nil nil nil s))
      (let ((s "ab")) (string-match "\\(.\\)\\(.\\)" s)
        (replace-match "\\1\\2\\2\\1" nil nil nil s))
      (let ((s "x1y2")) (string-match "\\(.\\)\\(.\\)\\(.\\)\\(.\\)" s)
        (replace-match "L\\1-\\2-\\3-\\4R" nil nil nil s))
      (let ((s "x1y2")) (string-match "\\(.\\)\\(.\\)\\(.\\)\\(.\\)" s)
        (replace-match "\\4\\3\\2\\1" nil nil nil s)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_match_substitute_replacement_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "a1b2c3")
  (goto-char 1)
  (re-search-forward "\\(.\\)\\(.\\)" nil t)
  (let ((r1 (match-substitute-replacement "[\\1\\2]"))
        (m0 (match-data)))
    (re-search-forward "\\(.\\)\\(.\\)" nil t)
    (let ((r2 (match-substitute-replacement "\\2\\1"))
          (r3 (match-substitute-replacement "[\\1][\\2]")))
      (list r1 r2 r3 m0))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"[a1]\" \"2b\" \"[b][2]\" (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
