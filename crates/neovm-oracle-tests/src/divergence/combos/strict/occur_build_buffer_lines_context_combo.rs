//! Strict combo oracle probes, batch 233: occur buffer building. occur over a
//! source buffer producing the *Occur* edit buffer with line numbers + context,
//! and the resulting line count + content shape.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_occur_build_buffer_line_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((src (get-buffer-create " *probe-occur-src*")))
  (with-current-buffer src
    (erase-buffer)
    (insert "alpha one\nbeta two\nalpha two\ngamma\nalpha three\n"))
  (when (get-buffer "*Occur*") (kill-buffer "*Occur*"))
  (with-current-buffer src
    (goto-char (point-min))
    (occur "alpha" 0))
  (let ((occur-buf (get-buffer "*Occur*")))
    (prog1
        (list (bufferp occur-buf)
              (and occur-buf (with-current-buffer occur-buf (count-lines (point-min) (point-max))))
              (and occur-buf (with-current-buffer occur-buf
                                (goto-char (point-min))
                                (string-match "alpha one" (buffer-string)))))
      (when occur-buf (kill-buffer occur-buf))
      (kill-buffer src))))
"##;
    let expect = expect_test::expect![[r#""OK (t 4 60)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_occur_context_lines_surrounding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((src (get-buffer-create " *probe-occur-ctx*")))
  (with-current-buffer src
    (erase-buffer)
    (insert "l1\nmatch\nl3\nl4\nmatch\nl6\n"))
  (when (get-buffer "*Occur*") (kill-buffer "*Occur*"))
  (with-current-buffer src
    (goto-char (point-min))
    (occur "match" 1))
  (let ((occur-buf (get-buffer "*Occur*")))
    (prog1
        (list (bufferp occur-buf)
              (and occur-buf (with-current-buffer occur-buf (buffer-string))))
      (when occur-buf (kill-buffer occur-buf))
      (kill-buffer src))))
"##;
    let expect = expect_test::expect![[
        r#""OK (t #(\"2 matches for \\\"match\\\" in buffer:  *probe-occur-ctx*\\n       :l1\\n      2:match\\n       :l3\\n-------\\n       :l4\\n      5:match\\n       :l6\\n\" 0 52 (face underline read-only t) 63 71 (mouse-face highlight help-echo \"mouse-2: go to this occurrence\" follow-link t occur-target ((#<marker in no buffer> . #<marker in no buffer>)) read-only t rear-nonsticky t front-sticky t occur-prefix t) 71 76 (mouse-face highlight occur-match t face match help-echo \"mouse-2: go to this occurrence\" follow-link t occur-target ((#<marker in no buffer> . #<marker in no buffer>))) 76 77 (occur-target ((#<marker in no buffer> . #<marker in no buffer>))) 107 115 (mouse-face highlight help-echo \"mouse-2: go to this occurrence\" follow-link t occur-target ((#<marker in no buffer> . #<marker in no buffer>)) read-only t rear-nonsticky t front-sticky t occur-prefix t) 115 120 (mouse-face highlight occur-match t face match help-echo \"mouse-2: go to this occurrence\" follow-link t occur-target ((#<marker in no buffer> . #<marker in no buffer>))) 120 121 (occur-target ((#<marker in no buffer> . #<marker in no buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_occur_no_matches_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((src (get-buffer-create " *probe-occur-nm*")))
  (with-current-buffer src
    (erase-buffer)
    (insert "foo\nbar\nbaz\n"))
  (when (get-buffer "*Occur*") (kill-buffer "*Occur*"))
  (condition-case err
      (with-current-buffer src
        (goto-char (point-min))
        (occur "zzznomatch" 0)
        'ran)
    (error (cons 'caught (car err))))
  (let ((occur-buf (get-buffer "*Occur*")))
    (prog1
        (list (bufferp occur-buf)
              (and occur-buf (with-current-buffer occur-buf (buffer-string))))
      (when occur-buf (kill-buffer occur-buf))
      (kill-buffer src))))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
