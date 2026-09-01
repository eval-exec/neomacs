//! Strict combo oracle probes, batch 139: font matching, charset priority
//! ordering, process stderr routing with different buffer targets, and
//! cl-loop with conditional sum/count/finally-return hash.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v3_font_matching_and_charset_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (font-family-list)
      (sort (charset-priority-list) #'string<)
      (char-charset ?a)
      (char-charset 128578)
      (char-charset 945)
      (font-spec-p (font-spec :family "Monospace"))
      (fontp (font-spec :family "Monospace") 'font-spec)
      (font-get (font-spec :family "Monospace" :weight 'bold) :weight)
      (> (length (font-family-list)) 0))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function font-spec-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v3_process_stderr_routing_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((out-buf (generate-new-buffer " *probe-psr-out*"))
      (err-buf (generate-new-buffer " *probe-psr-err*")))
  (let ((proc (make-process :name "probe-psr"
                            :command (list shell-file-name shell-command-switch
                                           "echo stdout-line; echo stderr-line 1>&2")
                            :buffer out-buf
                            :stderr err-buf
                            :sentinel (lambda (&rest _) nil))))
    (set-process-query-on-exit-flag proc nil)
    (accept-process-output proc 1)
    (accept-process-output proc 1)
    (let ((out (with-current-buffer out-buf (buffer-string)))
          (err (with-current-buffer err-buf (buffer-string))))
      (kill-buffer out-buf)
      (kill-buffer err-buf)
      (list (string-trim out)
            (string-trim err)
            (string-match-p "stdout" out)
            (string-match-p "stderr" err)
            (not (string-match-p "stderr" out))
            (not (string-match-p "stdout" err)))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v3_cl_loop_conditional_sum_count_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash "a" 5 h)
  (puthash "b" 15 h)
  (puthash "c" 25 h)
  (puthash "d" 3 h)
  (cl-loop for k being the hash-keys of h using (hash-values v)
           if (> v 10)
             sum v into big-sum
             count t into big-count
           else
             sum v into small-sum
             count t into small-count
           end
           finally (return (list big-sum big-count small-sum small-count
                                  (+ big-sum small-sum)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v3_window_margins_fringes_body_width_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wmfb*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((bw0 (window-body-width))
              (tw0 (window-total-width)))
          (set-window-margins nil 3 2)
          (let ((bw1 (window-body-width))
                (tw1 (window-total-width))
                (m1 (window-margins)))
            (set-window-fringes nil 5 6 nil)
            (let ((bw2 (window-body-width))
                  (f1 (window-fringes)))
              (set-window-margins nil 0 0)
              (set-window-fringes nil 8 8 nil)
              (let ((bw3 (window-body-width))
                    (m2 (window-margins))
                    (f2 (window-fringes)))
                (list bw0 tw0 bw1 tw1 m1 bw2 f1 bw3 m2 f2))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[
        r#""OK (80 80 75 80 (3 . 2) 75 (0 0 nil nil) 80 (nil) (0 0 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v3_eieio_slot_default_and_initform_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defclass probe-slot-eval ()
    ((a :initarg :a :initform (+ 1 2))
     (b :initarg :b :initform (list 1 2 3))
     (c :initarg :c :initform nil :type (or null integer))))
  (let ((o1 (probe-slot-eval))
        (o2 (probe-slot-eval :a 99 :b 'custom :c 42)))
    (list (oref o1 a)
          (oref o1 b)
          (oref o1 c)
          (oref o2 a)
          (oref o2 b)
          (oref o2 c)
          (slot-boundp o1 'a)
          (slot-boundp o1 'c)
          (eq (oref o1 b) (oref o1 b)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-defclass)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
