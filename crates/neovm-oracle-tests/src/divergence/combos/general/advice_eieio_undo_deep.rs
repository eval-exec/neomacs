//! Divergence tests: advice + EIEIO + undo + buffer + keymap deep combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_advice_on_buffer_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function advice--cdar)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-aobf-log-xxx nil)
  (advice-add 'insert :before
    (lambda (&rest args)
      (push (cons 'before-insert (length args)) test-aobf-log-xxx)))
  (advice-add 'delete-region :before
    (lambda (start end)
      (push (list 'before-delete start end) test-aobf-log-xxx)))
  (insert "HELLO")
  (delete-region 1 3)
  (let ((log (nreverse test-aobf-log-xxx)))
    (advice-remove 'insert (advice--cdar (advice--symbol-function 'insert)))
    (advice-remove 'delete-region (advice--cdar (advice--symbol-function 'delete-region)))
    (list log
          (= (length log) 2)
          (eq (car (nth 0 log)) 'before-insert)
          (= (cdr (nth 0 log)) 1)
          (eq (car (nth 1 log)) 'before-delete)
          (buffer-string)
          (string= (buffer-string) "LLO")))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_with_eieio_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t counter t 3 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-kwec-xxx ()
    ((name :initarg :name :accessor test-kwec-name)
     (count :initform 0 :accessor test-kwec-count)))
  (defvar test-kwec-instance-xxx nil)
  (defun test-kwec-cmd-xxx ()
    (interactive)
    (when test-kwec-instance-xxx
      (setf (test-kwec-count test-kwec-instance-xxx)
            (+ (test-kwec-count test-kwec-instance-xxx) 1))))
  (let ((obj (test-kwec-xxx :name 'counter))
        (map (make-sparse-keymap)))
    (setq test-kwec-instance-xxx obj)
    (define-key map [f5] 'test-kwec-cmd-xxx)
    (let ((binding (lookup-key map [f5])))
      (funcall 'test-kwec-cmd-xxx)
      (funcall 'test-kwec-cmd-xxx)
      (funcall 'test-kwec-cmd-xxx)
      (list (eq binding 'test-kwec-cmd-xxx)
            (test-kwec-name obj)
            (eq (test-kwec-name obj) 'counter)
            (test-kwec-count obj)
            (= (test-kwec-count obj) 3)
            (commandp 'test-kwec-cmd-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_advised_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function advice--cdar)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-uwai-count-xxx 0)
  (advice-add 'insert :filter-args
    (lambda (args)
      (setq test-uwai-count-xxx (+ test-uwai-count-xxx 1))
      args))
  (insert "ORIGINAL")
  (put-text-property 1 8 'tag 'original)
  (let ((ov (make-overlay 1 8)))
    (overlay-put ov 'type 'wrapper)
    (undo-boundary)
    (goto-char 1)
    (insert "PREFIX-")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "ORIGINAL" nil t)
    (replace-match "MODIFIED")
    (let ((s (buffer-string))
          (cnt test-uwai-count-xxx))
      (advice-remove 'insert (advice--cdar (advice--symbol-function 'insert)))
      (primitive-undo 2 buffer-undo-list)
      (list s cnt
            (> cnt 0)
            (buffer-string)
            (string= (buffer-string) "ORIGINAL")
            (get-text-property 1 'tag)
            (eq (get-text-property 1 'tag) 'original)
            (overlay-get ov 'type)
            (eq (overlay-get ov 'type) 'wrapper))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_with_advised_methods() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ewam-xxx ()
    ((value :initarg :value :initform 0 :accessor test-ewam-value)))
  (cl-defgeneric test-ewam-process-xxx (obj)
    "Process the object.")
  (cl-defmethod test-ewam-process-xxx ((obj test-ewam-xxx))
    (setf (test-ewam-value obj) (* (test-ewam-value obj) 2))
    (test-ewam-value obj))
  (advice-add 'test-ewam-process-xxx :around
    (lambda (oldfn obj &rest args)
      (let ((before (test-ewam-value obj)))
        (let ((result (apply oldfn obj args)))
          (list 'result result 'before before)))))
  (let ((obj (test-ewam-xxx :value 5)))
    (let ((r1 (test-ewam-process-xxx obj)))
      (list r1
            (equal r1 '(result 10 before 5))
            (test-ewam-value obj)
            (= (test-ewam-value obj) 10)
            (let ((r2 (test-ewam-process-xxx obj)))
              (equal r2 '(result 20 before 10)))))) "#,
        expect,
    );
}

#[test]
fn divergence_closure_with_buffer_local_and_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (modified t 17 t #(\"CONTENTINSERTED--HERE\" 0 6 (section header) 16 20 (section body)) #(\"CONTENT-HERE\" 0 6 (section header) 7 11 (section body)) t 8 t modified t local t header t body t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (make-local-variable 'test-cwblu-xxx)
  (setq test-cwblu-xxx 'initial)
  (insert "CONTENT-HERE")
  (put-text-property 1 7 'section 'header)
  (put-text-property 8 12 'section 'body)
  (let ((ov (make-overlay 1 12))
        (m (copy-marker 8 t))
        (get-fn (lambda () test-cwblu-xxx))
        (set-fn (lambda (v) (setq test-cwblu-xxx v))))
    (overlay-put ov 'scope 'local)
    (funcall set-fn 'modified)
    (undo-boundary)
    (goto-char 8)
    (insert "INSERTED-")
    (let ((val1 (funcall get-fn))
          (m-pos (marker-position m))
          (s (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list val1 (eq val1 'modified)
            m-pos (> m-pos 8)
            s
            (buffer-string)
            (string= (buffer-string) "CONTENT-HERE")
            (marker-position m) (= (marker-position m) 8)
            (funcall get-fn) (eq (funcall get-fn) 'modified)
            (overlay-get ov 'scope) (eq (overlay-get ov 'scope) 'local)
            (get-text-property 1 'section) (eq (get-text-property 1 'section) 'header)
            (get-text-property 8 'section) (eq (get-text-property 8 'section) 'body))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_advice_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function advice--cdar)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "EDITABLE-CONTENT-HERE")
  (put-text-property 1 8 'zone 'editable)
  (put-text-property 9 15 'zone 'content)
  (put-text-property 16 19 'zone 'tail)
  (let ((ov (make-overlay 1 19))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 9 t))
        (m3 (copy-marker 16)))
    (overlay-put ov 'edit 'active)
    (defvar test-kauc-count-xxx 0)
    (advice-add 'insert :before
      (lambda (&rest args)
        (setq test-kauc-count-xxx (+ test-kauc-count-xxx 1))))
    (undo-boundary)
    (goto-char 9)
    (insert "XX")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "CONTENT" nil t)
    (replace-match "MODIFIED")
    (let ((s (buffer-string))
          (cnt test-kauc-count-xxx)
          (m1-pos (marker-position m1))
          (m2-pos (marker-position m2))
          (m3-pos (marker-position m3)))
      (advice-remove 'insert (advice--cdar (advice--symbol-function 'insert)))
      (primitive-undo 2 buffer-undo-list)
      (list s cnt (> cnt 0) m1-pos m2-pos m3-pos
            (buffer-string)
            (string= (buffer-string) "EDITABLE-CONTENT-HERE")
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 9)
            (marker-position m3) (= (marker-position m3) 16)
            (overlay-get ov 'edit) (eq (overlay-get ov 'edit) 'active)
            (get-text-property 1 'zone) (eq (get-text-property 1 'zone) 'editable)
            (get-text-property 9 'zone) (eq (get-text-property 9 'zone) 'content)
            (get-text-property 16 'zone) (eq (get-text-property 16 'zone) 'tail))))) "#,
        expect,
    );
}

#[test]
fn divergence_recursive_edit_with_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (13 nil 6 t #(\"STATE-BEFORE\" 0 4 (part state) 5 10 (part before)) t state t #(\"STATE-BEFORE-APPENDED\" 0 4 (part state) 5 10 (part before) 11 12 (part appended) 12 19 (part appended)) t state t appended t modified t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "STATE-BEFORE")
  (put-text-property 1 5 'part 'state)
  (put-text-property 6 11 'part 'before)
  (let ((ov (make-overlay 1 11))
        (m (copy-marker 6 t)))
    (overlay-put ov 'phase 'initial)
    (let ((saved-point (point))
          (saved-marker (marker-position m))
          (saved-buffer (buffer-string))
          (saved-prop (get-text-property 1 'part)))
      (goto-char (point-max))
      (insert "-APPENDED")
      (overlay-put ov 'phase 'modified)
      (put-text-property 12 20 'part 'appended)
      (list saved-point (= saved-point 1)
            saved-marker (= saved-marker 6)
            saved-buffer (string= saved-buffer "STATE-BEFORE")
            saved-prop (eq saved-prop 'state)
            (buffer-string)
            (> (buffer-size) 11)
            (get-text-property 1 'part) (eq (get-text-property 1 'part) 'state)
            (get-text-property 12 'part) (eq (get-text-property 12 'part) 'appended)
            (overlay-get ov 'phase) (eq (overlay-get ov 'phase) 'modified))))) "#,
        expect,
    );
}

#[test]
fn divergence_multibyte_undo_with_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 4 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABC\x03B1\x03B2\x03B3DEF")
  (let ((ov (make-overlay 1 12))
        (m1 (copy-marker 1 t))
        (m2 (copy-marker 4 t))
        (m3 (copy-marker 10 t)))
    (overlay-put ov 'charset 'mixed)
    (put-text-property 1 3 'script 'latin)
    (put-text-property 4 9 'script 'greek)
    (put-text-property 10 12 'script 'latin)
    (undo-boundary)
    (goto-char 4)
    (insert "\x0391\x0392")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "ABC" nil t)
    (replace-match "XYZ")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (= (length (buffer-string)) 9)
            (= (buffer-size) 9)
            (marker-position m1) (= (marker-position m1) 1)
            (marker-position m2) (= (marker-position m2) 4)
            (marker-position m3) (= (marker-position m3) 10)
            (overlay-get ov 'charset) (eq (overlay-get ov 'charset) 'mixed)
            (get-text-property 1 'script) (eq (get-text-property 1 'script) 'latin)
            (get-text-property 4 'script) (eq (get-text-property 4 'script) 'greek)
            (get-text-property 10 'script) (eq (get-text-property 10 'script) 'latin))))) "#,
        expect,
    );
}

#[test]
fn divergence_edit_session_full_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"NEDIT1--MIDDLE\" 7 8 (region body)) 1 15 1 15 #(\"SESSION-START\" 0 6 (region header) 7 8 (region body) 8 12 (region body)) t 1 t 9 nil 1 t active t header t body t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "SESSION-START")
  (let ((m-start (copy-marker 1 t))
        (m-end (copy-marker (1+ (buffer-size))))
        (ov-session (make-overlay 1 (1+ (buffer-size)))))
    (overlay-put ov-session 'session 'active)
    (put-text-property 1 7 'region 'header)
    (put-text-property 8 13 'region 'body)
    (undo-boundary)
    (goto-char 8)
    (insert "EDIT1-")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "START" nil t)
    (replace-match "MIDDLE")
    (undo-boundary)
    (delete-region 1 7)
    (let ((s (buffer-string))
          (m-start-pos (marker-position m-start))
          (m-end-pos (marker-position m-end))
          (ov-s (overlay-start ov-session))
          (ov-e (overlay-end ov-session)))
      (primitive-undo 3 buffer-undo-list)
      (list s m-start-pos m-end-pos ov-s ov-e
            (buffer-string)
            (string= (buffer-string) "SESSION-START")
            (marker-position m-start) (= (marker-position m-start) 1)
            (marker-position m-end) (> (marker-position m-end) 12)
            (overlay-start ov-session) (= (overlay-start ov-session) 1)
            (overlay-get ov-session 'session) (eq (overlay-get ov-session 'session) 'active)
            (get-text-property 1 'region) (eq (get-text-property 1 'region) 'header)
            (get-text-property 8 'region) (eq (get-text-property 8 'region) 'body))))) "#,
        expect,
    );
}

#[test]
fn divergence_propagate_props_through_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"aaa-REPLACED-ccc-REPLACED-eee-REPLACED-ggg\" 0 2 (idx 1) 13 15 (idx 3) 26 28 (idx 5) 39 41 (idx 7)) 1 nil nil nil #(\"aaa-bbb-ccc-ddd-eee-fff-ggg\" 0 2 (idx 1) 4 6 (idx 2) 8 10 (idx 3) 12 14 (idx 4) 16 18 (idx 5) 20 22 (idx 6) 24 26 (idx 7)) t 1 t 2 t 3 t 4 t 5 t 6 t 7 t all t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa-bbb-ccc-ddd-eee-fff-ggg")
  (put-text-property 1 3 'idx 1)
  (put-text-property 5 7 'idx 2)
  (put-text-property 9 11 'idx 3)
  (put-text-property 13 15 'idx 4)
  (put-text-property 17 19 'idx 5)
  (put-text-property 21 23 'idx 6)
  (put-text-property 25 27 'idx 7)
  (let ((ov (make-overlay 1 27)))
    (overlay-put ov 'scope 'all)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "bbb\\|ddd\\|fff" nil t)
      (replace-match "REPLACED"))
    (let ((s (buffer-string))
          (i1 (get-text-property 1 'idx))
          (i3 (get-text-property 9 'idx))
          (i5 (get-text-property 17 'idx))
          (i7 (get-text-property 25 'idx)))
      (primitive-undo 1 buffer-undo-list)
      (list s i1 i3 i5 i7
            (buffer-string)
            (string= (buffer-string) "aaa-bbb-ccc-ddd-eee-fff-ggg")
            (get-text-property 1 'idx) (= (get-text-property 1 'idx) 1)
            (get-text-property 5 'idx) (= (get-text-property 5 'idx) 2)
            (get-text-property 9 'idx) (= (get-text-property 9 'idx) 3)
            (get-text-property 13 'idx) (= (get-text-property 13 'idx) 4)
            (get-text-property 17 'idx) (= (get-text-property 17 'idx) 5)
            (get-text-property 21 'idx) (= (get-text-property 21 'idx) 6)
            (get-text-property 25 'idx) (= (get-text-property 25 'idx) 7)
            (overlay-get ov 'scope) (eq (overlay-get ov 'scope) 'all))))) "#,
        expect,
    );
}
