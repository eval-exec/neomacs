//! Strict combo oracle probes, batch 362: buffer-match-p + buffer-local
//! variables deep. buffer-match-p with various predicates,
//! buffer-local-variables listing, and buffer-local-boundp.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_buffer_match_p_modes_visiting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b1 (get-buffer-create " *probe-bmp1*"))
      (b2 (get-buffer-create " *probe-bmp2*")))
  (unwind-protect
      (progn
        (with-current-buffer b1 (fundamental-mode))
        (with-current-buffer b2 (emacs-lisp-mode))
        (list (buffer-match-p b1 t)
              (buffer-match-p b1 '(derived-mode . fundamental-mode))
              (buffer-match-p b2 '(derived-mode . emacs-lisp-mode))
              (buffer-match-p b1 '(mode . fundamental-mode))
              (buffer-match-p b2 '(mode . fundamental-mode))))
    (kill-buffer b1)
    (kill-buffer b2)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_local_variables_listing_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-blv*")))
  (unwind-protect
      (with-current-buffer b
        (make-local-variable 'probe-blv-var)
        (setq probe-blv-var 'local)
        (let ((locals (buffer-local-variables)))
          (list (consp locals)
                (assq 'probe-blv-var locals)
                (local-variable-p 'probe-blv-var)
                (buffer-local-boundp 'probe-blv-var b)
                (buffer-local-boundp 'probe-not-set b)
                (buffer-local-value 'probe-blv-var b))))
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[r#""OK (t (probe-blv-var . local) t t nil local)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_match_p_name_regexp_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b1 (get-buffer-create " *probe-name-a*"))
      (b2 (get-buffer-create "test-file.txt")))
  (unwind-protect
      (list (buffer-match-p b1 '(name . " *probe-name-a*"))
            (buffer-match-p b2 '(name . "test-file.txt"))
            (buffer-match-p b1 '(name-mode . fundamental-mode))
            (buffer-match-p b1 nil))
    (kill-buffer b1)
    (kill-buffer b2)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument stringp (name . \" *probe-name-a*\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
