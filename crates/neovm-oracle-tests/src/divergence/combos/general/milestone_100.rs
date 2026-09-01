//! Divergence tests: milestone batch 100 — comprehensive integration stress tests.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_full_text_editing_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-pipe-log-xxx nil)
  (insert "The quick brown fox jumps over the lazy dog")
  (let ((ov-bold (make-overlay 5 10))
        (ov-hide (make-overlay 16 19))
        (m-start (copy-marker 1 t))
        (m-end (copy-marker (point-max))))
    (overlay-put ov-bold 'face 'bold)
    (overlay-put ov-hide 'invisible t)
    (put-text-property 1 10 'category 'sentence-start)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "\\b\\w+\\b" nil t)
      (let ((word (match-string 0)))
        (when (string= word "fox")
          (replace-match "cat"))
        (when (string= word "lazy")
          (replace-match "sleepy"))))
    (let ((result (buffer-string))
          (bold-start (overlay-start ov-bold))
          (bold-end (overlay-end ov-bold))
          (m-start-pos (marker-position m-start))
          (m-end-pos (marker-position m-end)))
      (list result
            (string-match "cat" result)
            (string-match "sleepy" result)
            bold-start bold-end
            m-start-pos m-end-pos
            (> m-end-pos m-start-pos)
            (get-text-property 1 'category)
            (eq (get-text-property 1 'category) 'sentence-start)
            (buffer-size)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_lifecycle_with_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-life-xxx ()
    ((name :initarg :name :accessor test-life-name-xxx)
     (history :initform nil :accessor test-life-history-xxx)))
  (cl-defmethod test-life-record-xxx ((obj test-life-xxx) event)
    (push event (slot-value obj 'history)))
  (advice-add 'test-life-record-xxx :before
               (lambda (obj event)
                 (push (list 'before event) (slot-value obj 'history))))
  (let ((o (test-life-xxx "o" :name "test"))
        (log nil))
    (fset 'test-life-watcher-xxx
          (let ((obj o))
            (lambda (event)
              (test-life-record-xxx obj event)
              (push event log))))
    (test-life-watcher-xxx 'created)
    (test-life-watcher-xxx 'modified)
    (test-life-watcher-xxx 'saved)
    (list (test-life-name-xxx o)
          (nreverse log)
          (equal (nreverse log) '(created modified saved))
          (length (test-life-history-xxx o))
          (>= (length (test-life-history-xxx o)) 3)
          (advice-remove 'test-life-record-xxx
                          (lambda (obj event)
                            (push (list 'before event)
                                  (slot-value obj 'history)))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_churn_with_overlays_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 200 ?X))
  (let ((ovs nil) (mks nil))
    (dotimes (i 20)
      (let ((start (+ 1 (* i 10)))
            (end (+ 5 (* i 10))))
        (push (make-overlay start end) ovs)
        (push (copy-marker start t) mks)))
    (dotimes (_ 5)
      (undo-boundary)
      (goto-char 50)
      (insert "YYY"))
    (let ((s1 (buffer-size))
          (ov-count (length (overlays-in 1 (point-max)))))
      (primitive-undo 3 buffer-undo-list)
      (list s1
            (= s1 215)
            ov-count
            (>= ov-count 10)
            (buffer-size)
            (every (lambda (m) (marker-position m)) mks)
            (= (length mks) 20)
            (= (length ovs) 20))))) "#,
        expect,
    );
}

#[test]
fn divergence_nested_macro_eval_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 t 21 t t t 22 t 12 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-gen-fn-xxx (name base)
    (let ((fn-name (intern (format "test-%s-fn-xxx" name))))
      (list 'defun fn-name (list 'n)
            (list '* 'n (eval base)))))
  (test-gen-fn-xxx double 2)
  (test-gen-fn-xxx triple 3)
  (list (test-double-fn-xxx 5)
        (= (test-double-fn-xxx 5) 10)
        (test-triple-fn-xxx 7)
        (= (test-triple-fn-xxx 7) 21)
        (fboundp 'test-double-fn-xxx)
        (fboundp 'test-triple-fn-xxx)
        (apply 'test-double-fn-xxx '(11))
        (= (apply 'test-double-fn-xxx '(11)) 22)
        (funcall 'test-triple-fn-xxx 4)
        (= (funcall 'test-triple-fn-xxx 4) 12))) "#,
        expect,
    );
}

#[test]
fn divergence_error_recovery_full_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((body1 cleanup body2 cleanup \"error: error\" body3 cleanup \"error: arith-error\") (body1 cleanup body2 cleanup \"error: error\" body3 cleanup \"error: arith-error\") (cleanup body2 cleanup \"error: error\" body3 cleanup \"error: arith-error\") nil (\"error: arith-error\") (body3 cleanup \"error: arith-error\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-err-log-xxx nil)
  (defun test-safe-run-xxx (fn)
    (condition-case e
        (unwind-protect
            (funcall fn)
          (push 'cleanup test-err-log-xxx))
      (error
       (push (format "error: %s" (car e)) test-err-log-xxx))))
  (test-safe-run-xxx (lambda () (push 'body1 test-err-log-xxx) 42))
  (test-safe-run-xxx (lambda () (push 'body2 test-err-log-xxx) (error "boom")))
  (test-safe-run-xxx (lambda () (push 'body3 test-err-log-xxx) (/ 1 0)))
  (let ((log (nreverse test-err-log-xxx)))
    (list log
          (member 'body1 log)
          (member 'cleanup log)
          (member "error: boom" log)
          (member "error: arith-error" log)
          (member 'body3 log)
          (>= (length log) 6)))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_hierarchy_command_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (test-cmd-c-xxx test-cmd-b-xxx test-cmd-c-xxx t t t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-cmd-a-xxx () (interactive) "cmd-a")
  (defun test-cmd-b-xxx () (interactive) "cmd-b")
  (defun test-cmd-c-xxx () (interactive) "cmd-c")
  (let ((global-map (make-sparse-keymap))
        (mode-map (make-sparse-keymap))
        (local-map (make-sparse-keymap)))
    (define-key global-map "a" 'test-cmd-a-xxx)
    (define-key global-map "b" 'test-cmd-b-xxx)
    (define-key mode-map "b" 'test-cmd-b-xxx)
    (define-key mode-map "c" 'test-cmd-c-xxx)
    (define-key local-map "a" 'test-cmd-c-xxx)
    (set-keymap-parent mode-map global-map)
    (set-keymap-parent local-map mode-map)
    (list (lookup-key local-map "a")
          (lookup-key local-map "b")
          (lookup-key local-map "c")
          (eq (lookup-key local-map "a") 'test-cmd-c-xxx)
          (eq (lookup-key local-map "b") 'test-cmd-b-xxx)
          (eq (lookup-key local-map "c") 'test-cmd-c-xxx)
          (lookup-key local-map "d")
          (commandp (lookup-key local-map "a"))))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_overlay_undo_full_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAAAXXX-BBBB-CCCC-DDDD-EEEEYYY\" 0 3 (face italic) 12 16 (face underline)) #(\"AAAAXXX-BBBB-CCCC-DDDD-EEEE\" 0 3 (face italic) 12 16 (face underline)) #(\"AAAAXXX-BBBB-CCCC-DDDD-EEEE\" 0 3 (face italic) 12 16 (face underline)) 5 12 5 5 12 italic t bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'face 'bold)
    (put-text-property 1 4 'face 'italic)
    (put-text-property 10 14 'face 'underline)
    (undo-boundary)
    (goto-char 5)
    (insert "XXX")
    (undo-boundary)
    (goto-char (point-max))
    (insert "YYY")
    (let ((s (buffer-string))
          (ov-s (overlay-start ov))
          (ov-e (overlay-end ov))
          (f1 (get-text-property 1 'face)))
      (primitive-undo 1 buffer-undo-list)
      (let ((s2 (buffer-string))
            (ov-s2 (overlay-start ov)))
        (primitive-undo 1 buffer-undo-list)
        (list s s2 (buffer-string)
              ov-s ov-e ov-s2
              (overlay-start ov) (overlay-end ov)
              f1 (eq f1 'italic)
              (overlay-get ov 'face)))))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_eval_obarray_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-clo-xxx 0)
  (let ((closures nil))
    (dotimes (i 5)
      (let ((test-clo-xxx i))
        (push (lambda () test-clo-xxx) closures)))
    (let ((vals (mapcar 'funcall (nreverse closures))))
      (list vals
            (equal vals '(0 1 2 3 4))
            (every 'integerp vals)
            (apply '+ vals)
            (= (apply '+ vals) 10)
            (intern-soft "test-clo-xxx")
            (symbolp (intern-soft "test-clo-xxx"))
            (= (symbol-value 'test-clo-xxx) 0)
            (let ((test-clo-xxx 99))
              (+ (eval 'test-clo-xxx) 1))
            (= (let ((test-clo-xxx 99))
                 (+ (eval 'test-clo-xxx) 1)) 100))))) "#,
        expect,
    );
}

#[test]
fn divergence_multibyte_regex_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable s1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "caf\xc3\xa9 na\xc3\xafve r\xc3\xa9sum\xc3\xa9")
  (let ((m1 (copy-marker 1))
        (m2 (copy-marker (point-max))))
    (put-text-property 1 5 'group 'start)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "\xc3\xa9" nil t)
      (replace-match "e"))
    (let ((s1 (buffer-string))
          (len1 (length s1))
          (p1 (marker-position m1))
          (p2 (marker-position m2)))
      (primitive-undo 3 buffer-undo-list)
      (list s1 len1 p1 p2
            (buffer-string)
            (marker-position m1)
            (marker-position m2)
            (get-text-property 1 'group)
            (eq (get-text-property 1 'group) 'start))))) "#,
        expect,
    );
}

#[test]
fn divergence_condition_case_with_overlays_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "START-ERROR-MARKER-END")
  (let ((ov (make-overlay 1 25))
        (m (copy-marker 13)))
    (overlay-put ov 'tag 'protected)
    (put-text-property 1 25 'status 'initial)
    (narrow-to-region 7 18)
    (condition-case e
        (progn
          (goto-char (point-min))
          (insert "DANGER")
          (put-text-property 1 30 'status 'modified)
          (error "test error"))
      (error
       (let ((msg (cadr e)))
         (widen)
         (list msg
               (string= msg "test error")
               (overlay-start ov) (overlay-end ov)
               (overlay-get ov 'tag)
               (eq (overlay-get ov 'tag) 'protected)
               (marker-position m)
               (get-text-property 1 'status)))))) "#,
        expect,
    );
}
