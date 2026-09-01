//! Divergence tests: buffer-local + closure + advice + keymap + command combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buflocal_closure_advice_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function advice--cdar)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-bcav-xxx nil)
  (make-local-variable 'test-bcav-xxx)
  (setq test-bcav-xxx 'initial)
  (let ((captured-value 'from-let))
    (defun test-bcaf-xxx ()
      (list test-bcav-xxx captured-value)))
  (advice-add 'test-bcaf-xxx :around
    (lambda (oldfn &rest args)
      (let ((orig (apply oldfn args)))
        (list 'advised orig test-bcav-xxx))))
  (setq test-bcav-xxx 'modified)
  (let ((r1 (test-bcaf-xxx)))
    (setq test-bcav-xxx 'again)
    (let ((r2 (test-bcaf-xxx)))
      (advice-remove 'test-bcaf-xxx
        (advice--cdar (advice--symbol-function 'test-bcaf-xxx)))
      (list r1 r2
            (test-bcaf-xxx)
            (equal r1 '(advised (modified from-let) modified))
            (equal r2 '(advised (again from-let) again))
            (equal (test-bcaf-xxx) '(again from-let)))))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_marker_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"oriINSERTEDginal\" 0 3 (part first) 12 15 (part second)) 12 14 #(\"original\" 0 3 (part first) 4 7 (part second)) t 4 t 6 t first t second t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (make-local-variable 'test-bmu-xxx)
  (setq test-bmu-xxx "original")
  (insert test-bmu-xxx)
  (let ((m1 (copy-marker 4 t))
        (m2 (copy-marker 6 nil)))
    (put-text-property 1 4 'part 'first)
    (put-text-property 5 8 'part 'second)
    (undo-boundary)
    (goto-char 4)
    (insert "INSERTED")
    (let ((s (buffer-string))
          (p1 (marker-position m1))
          (p2 (marker-position m2)))
      (primitive-undo 1 buffer-undo-list)
      (list s p1 p2
            (buffer-string)
            (string= (buffer-string) "original")
            (marker-position m1)
            (= (marker-position m1) 4)
            (marker-position m2)
            (= (marker-position m2) 6)
            (get-text-property 1 'part)
            (eq (get-text-property 1 'part) 'first)
            (get-text-property 5 'part)
            (eq (get-text-property 5 'part) 'second))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_closure_advice_interact() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1 1 nil 2 2 nil ([f5]))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-kca-result-xxx nil)
  (let ((counter 0))
    (defun test-kcaf-xxx ()
      (setq counter (+ counter 1))
      (setq test-kca-result-xxx counter)))
  (advice-add 'test-kcaf-xxx :before
    (lambda (&rest args)
      (setq test-kca-result-xxx 'before-called)))
  (let ((map (make-sparse-keymap)))
    (define-key map [f5] 'test-kcaf-xxx)
    (let ((binding (lookup-key map [f5])))
      (list (eq binding 'test-kcaf-xxx)
            (test-kcaf-xxx)
            test-kca-result-xxx
            (eq test-kca-result-xxx 'before-called)
            (test-kcaf-xxx)
            test-kca-result-xxx
            (eq test-kca-result-xxx 'before-called)
            (where-is-internal 'test-kcaf-xxx map))))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_overlay_textprop_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 26 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (make-local-variable 'test-boat-state-xxx)
  (setq test-boat-state-xxx 'ready)
  (insert "STATUS-HERE")
  (let ((ov (make-overlay 1 11)))
    (overlay-put ov 'state 'monitor)
    (put-text-property 1 7 'type 'label)
    (put-text-property 8 11 'type 'value)
    (defun test-boatf-xxx ()
      (overlay-put ov 'state test-boat-state-xxx)
      (list (overlay-get ov 'state) test-boat-state-xxx))
    (advice-add 'test-boatf-xxx :after
      (lambda (&rest args)
        (overlay-put ov 'advised t)))
    (setq test-boat-state-xxx 'active)
    (let ((r1 (test-boatf-xxx)))
      (setq test-boat-state-xxx 'done)
      (let ((r2 (test-boatf-xxx)))
        (list r1 r2
              (overlay-get ov 'state)
              (overlay-get ov 'advised)
              (get-text-property 1 'type)
              (eq (get-text-property 1 'type) 'label)
              (get-text-property 8 'type)
              (eq (get-text-property 8 'type) 'value)
              (buffer-string))))))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_overwrite_with_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure ((val . alpha)) nil val) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((val 'alpha))
    (defun test-cowf-xxx () val)
    (advice-add 'test-cowf-xxx :filter-args
      (lambda (args)
        (list 'filtered))))
  (let ((r1 (test-cowf-xxx)))
    (let ((val 'beta))
      (defun test-cowf-xxx () val))
    (let ((r2 (test-cowf-xxx)))
      (advice-remove 'test-cowf-xxx
        (advice--cdar (advice--symbol-function 'test-cowf-xxx)))
      (list r1 r2
            (test-cowf-xxx)
            (eq r1 'alpha)
            (eq r2 'beta)
            (eq (test-cowf-xxx) 'beta))))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_regex_match_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((5 11 \"KEY123\" nil) (16 22 \"KEY456\" nil)) t t t nil nil search t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (make-local-variable 'test-brmo-pattern-xxx)
  (setq test-brmo-pattern-xxx "KEY[0-9]+")
  (insert "abc KEY123 def KEY456 ghi")
  (let ((ov (make-overlay 1 24))
        (matches nil))
    (overlay-put ov 'zone 'search)
    (goto-char 1)
    (while (re-search-forward test-brmo-pattern-xxx nil t)
      (push (list (match-beginning 0)
                  (match-end 0)
                  (buffer-substring (match-beginning 0) (match-end 0))
                  (get-text-property (match-beginning 0) 'zone))
            matches))
    (let ((result (nreverse matches)))
      (list result
            (= (length result) 2)
            (string= (nth 2 (car result)) "KEY123")
            (string= (nth 2 (cadr result)) "KEY456")
            (eq (nth 3 (car result)) 'search)
            (eq (nth 3 (cadr result)) 'search)
            (overlay-get ov 'zone)
            (eq (overlay-get ov 'zone) 'search)
            (= (overlay-start ov) 1)
            (= (overlay-end ov) 24))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_buflocal_marker_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 39 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (make-local-variable 'test-ubmc-data-xxx)
  (setq test-ubmc-data-xxx "PREFIX-MIDDLE-SUFFIX")
  (insert test-ubmc-data-xxx)
  (let ((m1 (copy-marker 1 t))
        (m2 (copy-marker 7 t))
        (m3 (copy-marker 13 t))
        (m4 (copy-marker 19)))
    (put-text-property 1 6 'section 'head)
    (put-text-property 7 12 'section 'mid)
    (put-text-property 13 19 'section 'tail)
    (let ((ov (make-overlay 7 12)))
      (overlay-put ov 'region 'middle)
      (undo-boundary)
      (narrow-to-region 7 12)
      (goto-char (point-min))
      (insert "ZZ")
      (let ((narrowed-s (buffer-string))
            (m2-pos (marker-position m2))
            (m3-pos (marker-position m3)))
        (primitive-undo 1 buffer-undo-list)
        (widen)
        (list narrowed-s m2-pos m3-pos
              (buffer-string)
              (string= (buffer-string) "PREFIX-MIDDLE-SUFFIX")
              (marker-position m1)
              (= (marker-position m1) 1)
              (marker-position m2)
              (= (marker-position m2) 7)
              (marker-position m3)
              (= (marker-position m3) 13)
              (marker-position m4)
              (= (marker-position m4) 19)
              (get-text-property 1 'section)
              (eq (get-text-property 1 'section) 'head)
              (get-text-property 7 'section)
              (eq (get-text-property 7 'section) 'mid)
              (overlay-get ov 'region)
              (eq (overlay-get ov 'region) 'middle))))))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_let_binding_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((around local-override local-override) (around local-override local-override) nil (around local-override local-override) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-albs-xxx 'global)
  (defun test-albsf-xxx () test-albs-xxx)
  (advice-add 'test-albsf-xxx :around
    (lambda (oldfn &rest args)
      (let ((test-albs-xxx 'local-override))
        (list 'around (funcall oldfn) test-albs-xxx))))
  (let ((r1 (test-albsf-xxx)))
    (let ((test-albs-xxx 'outer-let))
      (let ((r2 (test-albsf-xxx)))
        (list r1 r2
              (eq test-albs-xxx 'global)
              (test-albsf-xxx)
              (equal r1 '(around global local-override))
              (equal r2 '(around outer-let local-override))))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_inheritance_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-kia-result-xxx nil)
  (defun test-kiaf-xxx () (setq test-kia-result-xxx 'parent-called))
  (advice-add 'test-kiaf-xxx :after
    (lambda (&rest args) (setq test-kia-result-xxx 'advised)))
  (let ((parent-map (make-sparse-keymap))
        (child-map (make-sparse-keymap)))
    (define-key parent-map [f1] 'test-kiaf-xxx)
    (set-keymap-parent child-map parent-map)
    (define-key child-map [f2] 'test-kiaf-xxx)
    (let ((b1 (lookup-key parent-map [f1]))
          (b2 (lookup-key child-map [f1]))
          (b3 (lookup-key child-map [f2]))
          (b4 (lookup-key parent-map [f2])))
      (list (eq b1 'test-kiaf-xxx)
            (eq b2 'test-kiaf-xxx)
            (eq b3 'test-kiaf-xxx)
            (null b4)
            (command-remapping 'test-kiaf-xxx)
            test-kia-result-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_kill_buffer_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 28 57)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer " test-bkbp1-xxx"))
        (buf2 (generate-new-buffer " test-bkbp2-xxx")))
    (with-current-buffer buf1
      (make-local-variable 'test-bkbp-xxx)
      (setq test-bkbp-xxx 'buf1-value)
      (insert "CONTENT1")
      (put-text-property 1 8 'src 'buf1))
    (with-current-buffer buf2
      (make-local-variable 'test-bkbp-xxx)
      (setq test-bkbp-xxx 'buf2-value)
      (insert "CONTENT2")
      (put-text-property 1 8 'src 'buf2))
    (let ((v1 (buffer-local-value 'test-bkbp-xxx buf1))
          (v2 (buffer-local-value 'test-bkbp-xxx buf2)))
      (kill-buffer buf1)
      (list (eq v1 'buf1-value)
            (eq v2 'buf2-value)
            (not (buffer-live-p buf1))
            (buffer-live-p buf2)
            (with-current-buffer buf2
              (buffer-string))
            (with-current-buffer buf2
              (get-text-property 1 'src))
            (eq (with-current-buffer buf2
                  (get-text-property 1 'src))
                'buf2)
            (buffer-local-value 'test-bkbp-xxx buf2)))))) "#,
        expect,
    );
}
