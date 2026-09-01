//! Divergence tests: buffer + overlay + text-property + EIEIO + advice mega combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_buffer_undo_with_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ebut-xxx ()
    ((buffer :initarg :buffer :accessor test-ebut-buffer)
     (marker :initarg :marker :accessor test-ebut-marker)
     (overlay :initarg :overlay :accessor test-ebut-overlay)
     (last-content :initform nil :accessor test-ebut-last-content)))
  (cl-defmethod test-ebut-insert-xxx ((obj test-ebut-xxx) text)
    (with-current-buffer (test-ebut-buffer obj)
      (goto-char (marker-position (test-ebut-marker obj)))
      (insert text)
      (setf (test-ebut-last-content obj) (buffer-string))))
  (cl-defmethod test-ebut-replace-xxx ((obj test-ebut-xxx) from to)
    (with-current-buffer (test-ebut-buffer obj)
      (goto-char 1)
      (when (re-search-forward from nil t)
        (replace-match to))))
  (cl-defmethod test-ebut-state-xxx ((obj test-ebut-xxx))
    (with-current-buffer (test-ebut-buffer obj)
      (list (buffer-string)
            (marker-position (test-ebut-marker obj))
            (overlay-start (test-ebut-overlay obj))
            (overlay-end (test-ebut-overlay obj))
            (overlay-get (test-ebut-overlay obj) 'owner)
            (eq (overlay-get (test-ebut-overlay obj) 'owner) obj))))
  (let* ((buf (generate-new-buffer " test-ebut-xxx"))
         (widget (test-ebut-xxx)))
    (setf (test-ebut-buffer widget) buf)
    (with-current-buffer buf
      (insert "ORIGINAL")
      (setf (test-ebut-marker widget) (copy-marker 1 t))
      (setf (test-ebut-overlay widget) (make-overlay 1 9))
      (overlay-put (test-ebut-overlay widget) 'owner widget)
      (put-text-property 1 9 'managed widget)
      (undo-boundary))
    (test-ebut-insert-xxx widget "INSERTED-")
    (with-current-buffer buf (undo-boundary))
    (test-ebut-replace-xxx widget "ORIGINAL" "MODIFIED")
    (with-current-buffer buf (undo-boundary))
    (let ((state-before (test-ebut-state-xxx widget)))
      (with-current-buffer buf
        (primitive-undo 3 buffer-undo-list))
      (let ((state-after (test-ebut-state-xxx widget)))
        (kill-buffer buf)
        (list state-before state-after
              (string= (car state-after) "ORIGINAL")
              (eq (nth 4 state-after) widget)
              (eq (nth 5 state-after) t)
              (= (nth 1 state-after) 1)))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_textprop_advice_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"AAA-XXYYY-CCC-DDD-EEE\" 0 2 (group a) 10 12 (group c) 14 16 (group d) 18 20 (group e)) (nil t nil nil nil) #(\"AAA-BBB-CCC-DDD-EEE\" 0 2 (group a) 4 6 (group b) 8 10 (group c) 12 14 (group d) 16 18 (group e)) t 1 t 2 t 3 t 4 t 5 t a t b t c t d t e t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE")
  (let ((ov1 (make-overlay 1 3))
        (ov2 (make-overlay 5 7))
        (ov3 (make-overlay 9 11))
        (ov4 (make-overlay 13 15))
        (ov5 (make-overlay 17 19)))
    (overlay-put ov1 'priority 1)
    (overlay-put ov2 'priority 2)
    (overlay-put ov3 'priority 3)
    (overlay-put ov4 'priority 4)
    (overlay-put ov5 'priority 5)
    (put-text-property 1 3 'group 'a)
    (put-text-property 5 7 'group 'b)
    (put-text-property 9 11 'group 'c)
    (put-text-property 13 15 'group 'd)
    (put-text-property 17 19 'group 'e)
    (defun test-otac-xxx (ov after-change beg end &optional len)
      (overlay-put ov 'touched t))
    (dolist (ov (list ov1 ov2 ov3 ov4 ov5))
      (overlay-put ov 'modification-hooks (list 'test-otac-xxx)))
    (undo-boundary)
    (goto-char 5)
    (insert "XX")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "BBB" nil t)
    (replace-match "YYY")
    (let ((s (buffer-string))
          (touched (mapcar (lambda (ov) (overlay-get ov 'touched))
                           (list ov1 ov2 ov3 ov4 ov5))))
      (primitive-undo 2 buffer-undo-list)
      (list s touched
            (buffer-string)
            (string= (buffer-string) "AAA-BBB-CCC-DDD-EEE")
            (overlay-get ov1 'priority) (= (overlay-get ov1 'priority) 1)
            (overlay-get ov2 'priority) (= (overlay-get ov2 'priority) 2)
            (overlay-get ov3 'priority) (= (overlay-get ov3 'priority) 3)
            (overlay-get ov4 'priority) (= (overlay-get ov4 'priority) 4)
            (overlay-get ov5 'priority) (= (overlay-get ov5 'priority) 5)
            (get-text-property 1 'group) (eq (get-text-property 1 'group) 'a)
            (get-text-property 5 'group) (eq (get-text-property 5 'group) 'b)
            (get-text-property 9 'group) (eq (get-text-property 9 'group) 'c)
            (get-text-property 13 'group) (eq (get-text-property 13 'group) 'd)
            (get-text-property 17 'group) (eq (get-text-property 17 'group) 'e))))) "#,
        expect,
    );
}

#[test]
fn divergence_multi_buffer_overlay_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((buf1 (generate-new-buffer " test-mbos1-xxx"))
         (buf2 (generate-new-buffer " test-mbos2-xxx"))
         (m1 nil) (m2 nil) (ov1 nil) (ov2 nil))
    (with-current-buffer buf1
      (insert "BUFFER-ONE-CONTENT")
      (setq m1 (copy-marker 1 t))
      (setq ov1 (make-overlay 1 17))
      (overlay-put ov1 'source 'buf1)
      (put-text-property 1 6 'part 'header)
      (put-text-property 7 11 'part 'body)
      (put-text-property 12 17 'part 'footer)
      (undo-boundary))
    (with-current-buffer buf2
      (insert "BUFFER-TWO-CONTENT")
      (setq m2 (copy-marker 1 t))
      (setq ov2 (make-overlay 1 17))
      (overlay-put ov2 'source 'buf2)
      (put-text-property 1 6 'part 'header)
      (put-text-property 7 11 'part 'body)
      (put-text-property 12 17 'part 'footer)
      (undo-boundary))
    (with-current-buffer buf1
      (goto-char 7)
      (insert "INSERTED-")
      (undo-boundary))
    (with-current-buffer buf2
      (goto-char 1)
      (re-search-forward "TWO" nil t)
      (replace-match "MODIFIED")
      (undo-boundary))
    (let ((s1 (with-current-buffer buf1 (buffer-string)))
          (s2 (with-current-buffer buf2 (buffer-string)))
          (m1-pos (marker-position m1))
          (m2-pos (marker-position m2)))
      (with-current-buffer buf1 (primitive-undo 1 buffer-undo-list))
      (with-current-buffer buf2 (primitive-undo 1 buffer-undo-list))
      (let ((s1-after (with-current-buffer buf1 (buffer-string)))
            (s2-after (with-current-buffer buf2 (buffer-string)))
            (m1-after (marker-position m1))
            (m2-after (marker-position m2)))
        (kill-buffer buf1)
        (kill-buffer buf2)
        (list s1 s2 m1-pos m2-pos
              s1-after s2-after m1-after m2-after
              (string= s1-after "BUFFER-ONE-CONTENT")
              (string= s2-after "BUFFER-TWO-CONTENT")
              (= m1-after 1)
              (= m2-after 1)
              (overlay-get ov1 'source) (eq (overlay-get ov1 'source) 'buf1)
              (overlay-get ov2 'source) (eq (overlay-get ov2 'source) 'buf2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_prop_change_undo_with_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (warning error t t #(\"AAAA-BBBB-CCCC-DDDD\" 0 3 (face bold) 3 4 (face nil) 4 7 (face italic) 7 8 (face nil) 8 11 (face underline) 11 12 (face nil) 12 15 (face default) 15 16 (face nil)) nil bold t italic t underline t default t error nil highlighted t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (let ((ov (make-overlay 1 17)))
    (overlay-put ov 'face 'highlight)
    (overlay-put ov 'tag 'highlighted)
    (put-text-property 1 4 'face 'bold)
    (put-text-property 5 8 'face 'italic)
    (put-text-property 9 12 'face 'underline)
    (put-text-property 13 16 'face 'default)
    (undo-boundary)
    (put-text-property 1 17 'face 'warning)
    (undo-boundary)
    (overlay-put ov 'face 'error)
    (let ((buf-face (get-text-property 1 'face))
          (ov-face (overlay-get ov 'face)))
      (primitive-undo 2 buffer-undo-list)
      (list buf-face ov-face
            (eq buf-face 'warning)
            (eq ov-face 'error)
            (buffer-string)
            (= (buffer-size) 17)
            (get-text-property 1 'face) (eq (get-text-property 1 'face) 'bold)
            (get-text-property 5 'face) (eq (get-text-property 5 'face) 'italic)
            (get-text-property 9 'face) (eq (get-text-property 9 'face) 'underline)
            (get-text-property 13 'face) (eq (get-text-property 13 'face) 'default)
            (overlay-get ov 'face) (eq (overlay-get ov 'face) 'highlight)
            (overlay-get ov 'tag) (eq (overlay-get ov 'tag) 'highlighted))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_polymorphic_buffer_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-pbe-editor-xxx ()
    ((buf :accessor test-pbe-buf)
     (marker :accessor test-pbe-marker)))
  (defclass test-pbe-inserter-xxx (test-pbe-editor-xxx) ())
  (defclass test-pbe-replacer-xxx (test-pbe-editor-xxx) ())
  (cl-defgeneric test-pbe-execute-xxx (editor text)
    "Execute edit operation.")
  (cl-defmethod test-pbe-execute-xxx ((editor test-pbe-inserter-xxx) text)
    (with-current-buffer (test-pbe-buf editor)
      (goto-char (marker-position (test-pbe-marker editor)))
      (insert text)))
  (cl-defmethod test-pbe-execute-xxx ((editor test-pbe-replacer-xxx) text)
    (with-current-buffer (test-pbe-buf editor)
      (goto-char 1)
      (when (re-search-forward "TARGET" nil t)
        (replace-match text))))
  (let* ((buf (generate-new-buffer " test-pbe-xxx"))
         (ins (test-pbe-inserter-xxx))
         (rep (test-pbe-replacer-xxx)))
    (setf (test-pbe-buf ins) buf)
    (setf (test-pbe-buf rep) buf)
    (with-current-buffer buf
      (insert "TARGET-CONTENT")
      (setf (test-pbe-marker ins) (copy-marker 1 t))
      (setf (test-pbe-marker rep) (copy-marker 1 t))
      (put-text-property 1 6 'type 'target)
      (put-text-property 7 14 'type 'content)
      (undo-boundary))
    (test-pbe-execute-xxx ins "PREFIX-")
    (with-current-buffer buf (undo-boundary))
    (test-pbe-execute-xxx rep "REPLACED")
    (let ((s (with-current-buffer buf (buffer-string))))
      (with-current-buffer buf (primitive-undo 2 buffer-undo-list))
      (let ((s-after (with-current-buffer buf (buffer-string))))
        (kill-buffer buf)
        (list s s-after
              (string= s-after "TARGET-CONTENT")
              (get-text-property 1 'type)
              (eq (get-text-property 1 'type) 'target)))))) "#,
        expect,
    );
}
