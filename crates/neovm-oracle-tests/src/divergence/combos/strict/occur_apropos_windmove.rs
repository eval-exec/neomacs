//! Strict combo oracle probes, batch 65: genuinely-untested deterministic
//! areas — occur (regex line matching into *Occur* buffer),
//! apropos-internal (symbol regex search), and windmove-find-other-window.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_n5_occur_line_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t #(\"3 matches for \\\"foo\\\" in buffer:  *probe-occur-src*\\n      1:foo\\n      3:foo\\n      5:foobar\\n\" 0 50 (face underline read-only t) 50 58 (occur-prefix t front-sticky t rear-nonsticky t read-only t occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" mouse-face highlight) 58 61 (occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" face match occur-match t mouse-face highlight) 61 62 (occur-target ((#<marker in no buffer> . #<marker in no buffer>))) 62 70 (occur-prefix t front-sticky t rear-nonsticky t read-only t occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" mouse-face highlight) 70 73 (occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" face match occur-match t mouse-face highlight) 73 74 (occur-target ((#<marker in no buffer> . #<marker in no buffer>))) 74 82 (occur-prefix t front-sticky t rear-nonsticky t read-only t occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" mouse-face highlight) 82 85 (occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" face match occur-match t mouse-face highlight) 85 88 (occur-target ((#<marker in no buffer> . #<marker in no buffer>)) follow-link t help-echo \"mouse-2: go to this occurrence\" mouse-face highlight) 88 89 (occur-target ((#<marker in no buffer> . #<marker in no buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((source-buf (generate-new-buffer " *probe-occur-src*")))
  (unwind-protect
      (progn
        (with-current-buffer source-buf
          (insert "foo\nbar\nfoo\nbaz\nfoobar\n"))
        (with-current-buffer source-buf
          (occur "foo"))
        (let ((occur-buf (get-buffer "*Occur*")))
          (list (bufferp occur-buf)
                (and occur-buf
                     (with-current-buffer occur-buf
                       (buffer-string))))))
    (when (buffer-live-p source-buf) (kill-buffer source-buf))
    (when (get-buffer "*Occur*") (kill-buffer "*Occur*"))))
"##,
        expect,
    );
}

#[test]
fn div_n5_apropos_internal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((defun) (car) (cons) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (member 'defun (apropos-internal "^defun\\'"))
      (member 'car (apropos-internal "^car\\'"))
      (member 'cons (apropos-internal "^cons\\'"))
      (> (length (apropos-internal "string")) 10)
      (= (length (apropos-internal "^zzz-nonexistent-xyz\\'")) 0))
"##,
        expect,
    );
}

#[test]
fn div_n5_windmove_find_other_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function windmove-find-other-window)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *probe-wm-a*"))
      (b2 (get-buffer-create " *probe-wm-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (let ((w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b2)
          (select-window w2)
          (list (eq (windmove-find-other-window 'left) (window-parent w2))
                (window-live-p (windmove-find-other-window 'left))
                (count-windows))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_n5_occur_count_nlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((source-buf (generate-new-buffer " *probe-occur-count*")))
  (unwind-protect
      (progn
        (with-current-buffer source-buf
          (insert "match1\nnomatch\nmatch2\nmatch3\nnomatch2\n"))
        (with-current-buffer source-buf
          (occur "match"))
        (let ((occur-buf (get-buffer "*Occur*")))
          (and occur-buf
               (with-current-buffer occur-buf
                 (count-lines (point-min) (point-max))))))
    (when (buffer-live-p source-buf) (kill-buffer source-buf))
    (when (get-buffer "*Occur*") (kill-buffer "*Occur*"))))
"##,
        expect,
    );
}
