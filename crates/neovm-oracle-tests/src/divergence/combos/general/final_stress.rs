//! Divergence tests: final integration stress batch — mega combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_mega_combo_edit_undo_overlays_markers_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 39 60)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "fn foo(x) {\n  return x + 1;\n}\n\nfn bar(y) {\n  return y * 2;\n}\n")
  (let ((ov-fn1 (make-overlay 1 7))
        (ov-fn2 (make-overlay 31 37))
        (m-return1 (copy-marker 19 t))
        (m-return2 (copy-marker 57 t)))
    (overlay-put ov-fn1 'face 'font-lock-keyword-face)
    (overlay-put ov-fn2 'face 'font-lock-keyword-face)
    (put-text-property 1 7 'syntax 'keyword)
    (put-text-property 19 25 'syntax 'return)
    (put-text-property 31 37 'syntax 'keyword)
    (put-text-property 57 63 'syntax 'return)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "foo" nil t)
    (replace-match "calculate")
    (undo-boundary)
    (goto-char 31)
    (re-search-forward "bar" nil t)
    (replace-match "compute")
    (let ((s1 (buffer-string))
          (kw1 (get-text-property 1 'syntax))
          (ret1-pos (marker-position m-return1))
          (fn1-start (overlay-start ov-fn1))
          (fn2-start (overlay-start ov-fn2)))
      (primitive-undo 2 buffer-undo-list)
      (list s1
            (buffer-string)
            (string= (buffer-string)
                     "fn foo(x) {\n  return x + 1;\n}\n\nfn bar(y) {\n  return y * 2;\n}\n")
            kw1 (eq kw1 'keyword)
            ret1-pos
            fn1-start fn2-start
            (overlay-start ov-fn1) (overlay-end ov-fn1)
            (overlay-start ov-fn2) (overlay-end ov-fn2)
            (marker-position m-return1)
            (marker-position m-return2)
            (get-text-property 1 'syntax)
            (eq (get-text-property 1 'syntax) 'keyword)))) #"#,
        expect,
    );
}

#[test]
fn divergence_mega_combo_eieio_closure_eval_advice_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 36 73)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-mega-obj-xxx ()
    ((name :initarg :name :accessor test-mega-name-xxx)
     (history :initform nil :accessor test-mega-history-xxx)))
  (cl-defmethod test-mega-log-xxx ((obj test-mega-obj-xxx) event)
    (push event (slot-value obj 'history)))
  (advice-add 'test-mega-log-xxx :before
               (lambda (obj event)
                 (push (list 'advised event) (slot-value obj 'history))))
  (let ((o (make-instance 'test-mega-obj-xxx :name "test"))
        (m (copy-marker 1 t)))
    (insert "TRACKING")
    (put-text-property 1 9 'tracked t)
    (test-mega-log-xxx o 'created)
    (test-mega-log-xxx o 'modified)
    (let ((hist (test-mega-history-xxx o))
          (name (test-mega-name-xxx o))
          (m-pos (marker-position m)))
      (undo-boundary)
      (goto-char 5)
      (insert "XX")
      (let ((m-pos2 (marker-position m)))
        (primitive-undo 1 buffer-undo-list)
        (list hist name m-pos m-pos2
              (string= name "test")
              (>= (length hist) 4)
              (member '(advised created) hist)
              (member 'created hist)
              (marker-position m)
              (= (marker-position m) m-pos)
              (get-text-property 1 'tracked)
              (eq (get-text-property 1 'tracked) t)
              (advice-remove 'test-mega-log-xxx
                              (lambda (obj event)
                                (push (list 'advised event)
                                      (slot-value obj 'history))))))))) #"#,
        expect,
    );
}

#[test]
fn divergence_mega_combo_buffer_narrow_overlay_undo_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 40 38)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "SECTION1-AAAA SECTION2-BBBB SECTION3-CCCC SECTION4-DDDD")
  (let ((ov1 (make-overlay 1 13))
        (ov2 (make-overlay 14 26))
        (ov3 (make-overlay 27 39))
        (ov4 (make-overlay 40 52)))
    (overlay-put ov1 'section 1)
    (overlay-put ov2 'section 2)
    (overlay-put ov3 'section 3)
    (overlay-put ov4 'section 4)
    (put-text-property 1 13 'group 'a)
    (put-text-property 14 26 'group 'b)
    (put-text-property 27 39 'group 'c)
    (put-text-property 40 52 'group 'd)
    (narrow-to-region 14 39)
    (undo-boundary)
    (goto-char (point-min))
    (re-search-forward "BBBB" nil t)
    (replace-match "XXXX")
    (undo-boundary)
    (re-search-forward "CCCC" nil t)
    (replace-match "YYYY")
    (let ((s-narrow (buffer-string))
          (ov-sec2 (overlay-get ov2 'section))
          (ov-sec3 (overlay-get ov3 'section)))
      (primitive-undo 2 buffer-undo-list)
      (widen)
      (list s-narrow ov-sec2 ov-sec3
            (buffer-string)
            (string= (buffer-string)
                     "SECTION1-AAAA SECTION2-BBBB SECTION3-CCCC SECTION4-DDDD")
            (overlay-start ov1) (overlay-end ov1)
            (overlay-start ov2) (overlay-end ov2)
            (overlay-get ov1 'section)
            (= (overlay-get ov1 'section) 1)
            (get-text-property 1 'group)
            (eq (get-text-property 1 'group) 'a)
            (get-text-property 14 'group)
            (eq (get-text-property 14 'group) 'b)
            (= (buffer-size) 52))))) #"#,
        expect,
    );
}

#[test]
fn divergence_mega_combo_keymap_closure_eieio_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 28 55)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-mkce-xxx ()
    ((val :initarg :val :initform 0)
     (map :initform nil)))
  (cl-defmethod test-mkce-setup-xxx ((obj test-mkce-xxx))
    (let ((my-map (make-sparse-keymap))
          (v (slot-value obj 'val)))
      (define-key my-map "i"
        (lambda () (interactive) (cl-incf v)))
      (define-key my-map "d"
        (lambda () (interactive) (cl-decf v)))
      (define-key my-map "g"
        (lambda () (interactive) v))
      (oset obj map my-map)))
  (let ((o (make-instance 'test-mkce-xxx :val 10)))
    (test-mkce-setup-xxx o)
    (let ((map (slot-value o 'map)))
      (list (lookup-key map "i")
            (commandp (lookup-key map "i"))
            (lookup-key map "g")
            (commandp (lookup-key map "g"))
            (funcall (lookup-key map "i"))
            (funcall (lookup-key map "i"))
            (funcall (lookup-key map "g"))
            (= (funcall (lookup-key map "g")) 12)
            (funcall (lookup-key map "d"))
            (funcall (lookup-key map "g"))
            (= (funcall (lookup-key map "g")) 11))))) #"#,
        expect,
    );
}

#[test]
fn divergence_mega_combo_multibyte_overlay_undo_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 30 56)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "start-\xc3\xa9\xc3\xa0-\xc3\xb9-end")
  (let ((ov (make-overlay 7 9))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 13)))
    (overlay-put ov 'face 'bold)
    (put-text-property 1 6 'part 'pre)
    (put-text-property 7 9 'part 'accent)
    (put-text-property 10 14 'part 'post)
    (undo-boundary)
    (goto-char 7)
    (insert "\xc3\xa9\xc3\xa0")
    (let ((s1 (buffer-string))
          (ov-s (overlay-start ov))
          (ov-e (overlay-end ov))
          (m1p (marker-position m1))
          (m2p (marker-position m2)))
      (primitive-undo 1 buffer-undo-list)
      (list s1 ov-s ov-e m1p m2p
            (buffer-string)
            (marker-position m1)
            (marker-position m2)
            (overlay-start ov) (overlay-end ov)
            (overlay-get ov 'face)
            (get-text-property 1 'part)
            (eq (get-text-property 1 'part) 'pre)
            (get-text-property 7 'part)
            (eq (get-text-property 7 'part) 'accent)
            (get-text-property 10 'part)
            (eq (get-text-property 10 'part) 'post)))) #"#,
        expect,
    );
}
