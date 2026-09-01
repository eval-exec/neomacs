//! Combo: cl-eieio with-temp-buffer + cross-buffer EIEIO operations
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests temp buffer lifecycle with EIEIO objects, overlay propagation
//! across buffers, and marker validity through temp buffer operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_temp_buffer_insert_from_main() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass temp-ctx ()
    ((source-buf :initarg :src :accessor tc-src :initform "")
     (snippets :initarg :snips :accessor tc-snips :initform nil)))
  (let* ((buf (generate-new-buffer "tb1"))
         (snaps nil)
         (ctx (temp-ctx :src (buffer-name buf) :snips nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-tb-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (let ((sub (buffer-substring 6 15)))
          (with-temp-buffer
            (insert sub)
            (push (buffer-string) (tc-snips ctx))
            (setq my-tb-log (cons "temp-ins-sub" my-tb-log))))
        (push (list "init" (tc-snips ctx) (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-tb-log (cons "ins@8" my-tb-log))
        (let ((sub (buffer-substring 6 18)))
          (with-temp-buffer
            (insert sub)
            (put-text-property 1 5 'face 'error)
            (push (buffer-string) (tc-snips ctx))
            (setq my-tb-log (cons "temp-ins-sub2" my-tb-log))))
        (push (list "edit" (tc-snips ctx) (marker-position m)) results)
        (with-temp-buffer
          (insert "TEMP-CONTENT")
          (let ((temp-sub (buffer-string))))
        (push (list "temp-scope" (tc-snips ctx) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S tb-log=%S"
                       results (reverse my-tb-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tc-log t)
        (list (buffer-string)
              (length (tc-snips ctx))
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-tb-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_temp_buffer_with_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable my-toc-log)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass temp-ov-ctx ()
    ((label :initarg :label :accessor toc-label :initform "")
     (captured-faces :initarg :faces :accessor toc-faces :initform nil)))
  (let* ((buf (generate-new-buffer "tb2"))
         (snaps nil)
         (ctx (temp-ov-ctx :label "main" :faces nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-toc-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (with-temp-buffer
          (insert "TEMP-AAAA-TEMP")
          (let ((tov (make-overlay 1 5)))
            (overlay-put tov 'face 'error)
            (overlay-put tov 'priority 10)
            (push (overlay-get tov 'face) (toc-faces ctx))
            (setq my-toc-log (cons "temp-ov-created" my-toc-log))))
        (push (list "init" (toc-faces ctx) (marker-position m)) results)
        (push (overlay-get ov 'face) (toc-faces ctx))
        (goto-char 8)
        (insert "MMM")
        (setq my-toc-log (cons "ins@8" my-toc-log))
        (push (list "edit" (toc-faces ctx) (marker-position m)) results)
        (with-temp-buffer
          (insert (with-current-buffer buf (buffer-substring 6 18)))
          (setq my-toc-log (cons "temp-copy" my-toc-log))
          (let ((len (point-max)))
            (push (format "temp-len=%d" len) my-toc-log)))
        (push (list "temp-copy" (toc-faces ctx) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S toc-log=%S"
                       results (reverse my-toc-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'toc-log t)
        (list (buffer-string)
              (length (toc-faces ctx))
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-toc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_temp_buffer_marker_across_bufs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable my-cm-log)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cross-mk ()
    ((m-pos :initarg :mpos :accessor cm-mpos :initform 0)
     (buf-name :initarg :buf :accessor cm-buf :initform "")
     (temp-results :initarg :temp :accessor cm-temp :initform nil)))
  (let* ((buf (generate-new-buffer "tb3"))
         (snaps nil)
         (m (set-marker (make-marker) 10))
         (ctx (cross-mk :mpos 10 :buf (buffer-name buf) :temp nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-cm-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (marker-position m)) results)
        (with-temp-buffer
          (insert "TEMP-CONTENT")
          (push (format "temp-pos=%d" (marker-position m)) my-cm-log)
          (push (marker-position m) (cm-temp ctx)))
        (push (list "after-temp" (marker-position m)) results)
        (setf (cm-mpos ctx) (marker-position m))
        (goto-char 8)
        (insert "QQQ")
        (setq my-cm-log (cons "ins@8" my-cm-log))
        (push (list "edit" (marker-position m) (cm-mpos ctx)) results)
        (with-temp-buffer
          (insert "MORE-TEMP")
          (push (marker-position m) (cm-temp ctx))
          (setq my-cm-log (cons "temp2" my-cm-log)))
        (setf (cm-mpos ctx) (marker-position m))
        (push (list "after-temp2" (marker-position m) (cm-mpos ctx)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S cm-temp=%S cm-log=%S"
                       results (reverse (cm-temp ctx))
                       (reverse my-cm-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cm-log t)
        (list (buffer-string)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (cm-temp ctx)
              my-cm-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_temp_buffer_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable my-tnc-log)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass temp-narrow-ctx ()
    ((edit-count :initarg :edits :accessor tnc-edits :initform 0)
     (temp-strings :initarg :temps :accessor tnc-temps :initform nil)))
  (let* ((buf (generate-new-buffer "tb4"))
         (snaps nil)
         (ctx (temp-narrow-ctx :edits 0 :temps nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-tnc-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (tnc-edits ctx) (marker-position m)) results)
        (with-temp-buffer
          (insert "SNIPPET")
          (push (buffer-string) (tnc-temps ctx))
          (setq my-tnc-log (cons "temp-snippet" my-tnc-log)))
        (save-restriction
          (narrow-to-region 8 28)
          (goto-char 10)
          (insert "XXX")
          (setf (tnc-edits ctx) (1+ (tnc-edits ctx)))
          (setq my-tnc-log (cons "ins-narrow@10" my-tnc-log))
          (push (list "narrow-edit" (tnc-edits ctx)
                      (marker-position m)) results)
          (with-temp-buffer
            (insert "NESTED-TEMP")
            (push (buffer-string) (tnc-temps ctx))
            (setq my-tnc-log (cons "nested-temp" my-tnc-log))))
        (push (list "widen" (tnc-edits ctx) (marker-position m)) results)
        (with-temp-buffer
          (insert "AFTER-WIDEN-TEMP")
          (push (buffer-string) (tnc-temps ctx))
          (setq my-tnc-log (cons "after-widen-temp" my-tnc-log)))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S tnc-temps=%S tnc-log=%S"
                       results (reverse (tnc-temps ctx))
                       (reverse my-tnc-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tnc-log t)
        (list (buffer-string)
              (tnc-edits ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (tnc-temps ctx)
              my-tnc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_temp_buffer_process_and_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass proc-ctx ()
    ((processed :initarg :proc :accessor pc-proc :initform nil)
     (total-chars :initarg :chars :accessor pc-chars :initform 0)))
  (defmethod pc-process-region ((ctx proc-ctx) beg end)
    (let ((sub (buffer-substring beg end))
          (result nil))
      (with-temp-buffer
        (insert sub)
        (goto-char 1)
        (while (search-forward "B" nil t)
          (replace-match "X" t t))
        (setq result (buffer-string)))
      (push result (pc-proc ctx))
      (setf (pc-chars ctx) (+ (length result) (pc-chars ctx)))))
  (let* ((buf (generate-new-buffer "tb5"))
         (snaps nil)
         (ctx (proc-ctx :proc nil :chars 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-BBBB-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (put-text-property 26 30 'face 'error)
      (setq-local my-pc-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (pc-chars ctx) (pc-proc ctx)) results)
        (pc-process-region ctx 1 15)
        (setq my-pc-log (cons "proc-1-15" my-pc-log))
        (push (list "proc1" (pc-chars ctx) (pc-proc ctx)
                    (marker-position m)) results)
        (goto-char 8)
        (insert "BBB")
        (setq my-pc-log (cons "ins@8" my-pc-log))
        (pc-process-region ctx 5 25)
        (push (list "proc2" (pc-chars ctx) (pc-proc ctx)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S pc-log=%S"
                       results (reverse my-pc-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pc-log t)
        (list (buffer-string)
              (pc-chars ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (pc-proc ctx)
              my-pc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
