//! Strict combo oracle probes, batch 248: hi-lock. highlight-regexp /
//! highlight-lines-matching-regexp creating face overlays + hi-lock-interactive-
//! patterns, highlight-symbol, and hi-lock-mode cleanup.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_highlight_regexp_overlays_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'hi-lock)
(with-temp-buffer
  (insert "hello world hello there hello\n")
  (highlight-regexp "hello" 'hi-yellow)
  (let ((count (length (overlays-in (point-min) (point-max))))
        (faces (mapcar (lambda (o) (overlay-get o 'hi-lock-face))
                       (overlays-in (point-min) (point-max)))))
    (unhighlight-regexp "hello")
    (list count
          faces
          (length (overlays-in (point-min) (point-max))))))
"##;
    let expect = expect_test::expect![[r#""OK (3 (nil nil nil) 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_highlight_lines_matching_regexp_full_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'hi-lock)
(with-temp-buffer
  (insert "match line\nother line\nmatch again\n")
  (highlight-lines-matching-regexp "match" 'hi-pink)
  (let ((count (length (overlays-in (point-min) (point-max)))))
    (unhighlight-regexp "match")
    (list count (length (overlays-in (point-min) (point-max))))))
"##;
    let expect = expect_test::expect![[r#""OK (2 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_hi_lock_mode_interactive_patterns_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'hi-lock)
(with-current-buffer (get-buffer-create " *probe-hilock*")
  (hi-lock-mode 1)
  (highlight-regexp "probe-pattern" 'hi-green)
  (let ((patterns hi-lock-interactive-patterns)
        (mode hi-lock-mode))
    (hi-lock-mode -1)
    (let ((result (list mode
                        (> (length patterns) 0)
                        hi-lock-mode)))
      (kill-buffer (current-buffer))
      result)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
