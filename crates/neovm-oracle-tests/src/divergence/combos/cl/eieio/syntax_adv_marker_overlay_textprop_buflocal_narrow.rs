//! Combo: cl-eieio syntax-table + parse-partial-sexp + overlays + markers
//! + textprop + buflocal + narrow + undo.
//! Tests syntax table manipulation, sexp parsing, and scan-lists with
//! EIEIO objects, overlays, and complex editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_syntax_parse_with_overlay_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass syn-snap ()
    ((step :initarg :step :accessor ss-step :initform "")
     (depth-at-10 :initarg :depth :accessor ss-depth :initform 0)
     (m-pos :initarg :m-pos :accessor ss-mp :initform 0)
     (buf-string :initarg :bs :accessor ss-bs :initform "")))
  (let* ((buf (generate-new-buffer "syn1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(foo (bar (baz)) quux)")
      (setq-local my-syn-log nil)
      (let* ((ov (make-overlay 5 14))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (syn-snap :step "init"
                       :depth (car (parse-partial-sexp 1 10))
                       :m-pos (marker-position m)
                       :bs (buffer-string)) snaps)
        (goto-char 5)
        (insert "(inner ")
        (setq my-syn-log (cons "ins-inner" my-syn-log))
        (push (syn-snap :step "insert"
                       :depth (car (parse-partial-sexp 1 12))
                       :m-pos (marker-position m)
                       :bs (buffer-string)) snaps)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 5 20)
          (push (syn-snap :step "narrow"
                         :depth (car (parse-partial-sexp (point-min) 10))
                         :m-pos (marker-position m)
                         :bs (buffer-substring-no-properties
                              (point-min) (point-max))) snaps))
        (push (syn-snap :step "widen"
                       :depth (car (parse-partial-sexp 1 15))
                       :m-pos (marker-position m)
                       :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (syn-snap :step "undo"
                       :depth (car (parse-partial-sexp 1 10))
                       :m-pos (marker-position m)
                       :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ss-step s) (ss-depth s)
                                                (ss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S syn-log=%S"
                       results (reverse my-syn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-syn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_syntax_scan_list_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass scan-snap ()
    ((step :initarg :step :accessor scs-step :initform "")
     (scan-fwd :initarg :fwd :accessor scs-fwd :initform nil)
     (scan-bwd :initarg :bwd :accessor scs-bwd :initform nil)
     (m-pos :initarg :m-pos :accessor scs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "syn2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(aaa (bbb (ccc) ddd) eee)")
      (setq-local my-scan-log nil)
      (let* ((ov (make-overlay 6 18))
             (_ (overlay-put ov 'face 'italic))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (scan-snap :step "init"
                        :fwd (scan-lists 10 1 1)
                        :bwd (scan-lists 10 -1 1)
                        :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "((new)) ")
        (setq my-scan-log (cons "ins@6" my-scan-log))
        (push (scan-snap :step "insert"
                        :fwd (condition-case nil (scan-lists 12 1 1) (error nil))
                        :bwd (condition-case nil (scan-lists 12 -1 1) (error nil))
                        :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (put-text-property 6 14 'syntax-table '(4))
        (setq my-scan-log (cons "syntax-tp@6-14" my-scan-log))
        (push (scan-snap :step "syntax-change"
                        :fwd (condition-case nil (scan-lists 12 1 1) (error nil))
                        :bwd (condition-case nil (scan-lists 12 -1 1) (error nil))
                        :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (scan-snap :step "undo-syntax"
                        :fwd (scan-lists 12 1 1)
                        :bwd (scan-lists 12 -1 1)
                        :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (scs-step s) (scs-fwd s)
                                                (scs-bwd s) (scs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S scan-log=%S"
                       results (reverse my-scan-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'scs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-scan-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_syntax_forward_comment_with_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass comment-snap ()
    ((step :initarg :step :accessor cs-step :initform "")
     (comment-end :initarg :ce :accessor cs-ce :initform nil)
     (m-pos :initarg :m-pos :accessor cs-mp :initform 0)
     (buf-string :initarg :bs :accessor cs-bs :initform "")))
  (let* ((buf (generate-new-buffer "syn3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAA ; comment line\nBBB /* block */ CCC\nDDD")
      (setq-local my-comm-log nil)
      (let* ((ov (make-overlay 5 25))
             (_ (overlay-put ov 'face 'font-lock-comment-face))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (comment-snap :step "init"
                           :ce (progn (goto-char 5) (forward-comment 1) (point))
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (goto-char 5)
        (insert "/* new block */ ")
        (setq my-comm-log (cons "ins-block-comment" my-comm-log))
        (push (comment-snap :step "insert"
                           :ce (progn (goto-char 5) (forward-comment 1) (point))
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 3 30)
          (push (comment-snap :step "narrow"
                             :ce (progn (goto-char (point-min))
                                       (forward-comment 1) (point))
                             :m-pos (marker-position m)
                             :bs (buffer-substring-no-properties
                                  (point-min) (point-max))) snaps))
        (push (comment-snap :step "widen"
                           :ce (progn (goto-char 5) (forward-comment 1) (point))
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (comment-snap :step "undo"
                           :ce (progn (goto-char 5) (forward-comment 1) (point))
                           :m-pos (marker-position m)
                           :bs (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cs-step s) (cs-ce s)
                                                (cs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S comm-log=%S"
                       results (reverse my-comm-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-comm-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_syntax_modify_syntax_table_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass modsyn-snap ()
    ((step :initarg :step :accessor ms-step :initform "")
     (char-class-at-5 :initarg :cc :accessor ms-cc :initform nil)
     (depth-at-8 :initarg :depth :accessor ms-depth :initform 0)
     (m-pos :initarg :m-pos :accessor ms-mp :initform 0)))
  (let* ((buf (generate-new-buffer "syn4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(abc|def&ghi)")
      (setq-local my-modsyn-log nil)
      (let* ((ov (make-overlay 5 9))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 7))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (modsyn-snap :step "init"
                          :cc (char-syntax (char-after 5))
                          :depth (car (parse-partial-sexp 1 8))
                          :m-pos (marker-position m)) snaps)
        (modify-syntax-entry ?| "(|")
        (setq my-modsyn-log (cons "pipe-open" my-modsyn-log))
        (push (modsyn-snap :step "pipe-open"
                          :cc (char-syntax (char-after 5))
                          :depth (car (parse-partial-sexp 1 8))
                          :m-pos (marker-position m)) snaps)
        (modify-syntax-entry ?& ")&")
        (setq my-modsyn-log (cons "amp-close" my-modsyn-log))
        (push (modsyn-snap :step "amp-close"
                          :cc (char-syntax (char-after 5))
                          :depth (car (parse-partial-sexp 1 8))
                          :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "XX")
        (setq my-modsyn-log (cons "edit@5" my-modsyn-log))
        (push (modsyn-snap :step "edit"
                          :cc (char-syntax (char-after 7))
                          :depth (car (parse-partial-sexp 1 10))
                          :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ms-step s) (ms-cc s)
                                                (ms-depth s) (ms-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ms-log=%S"
                       results (reverse my-modsyn-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ms-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (char-syntax ?|)
              (char-syntax ?&)
              my-modsyn-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_syntax_ppss_with_textprop_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ppss-snap ()
    ((step :initarg :step :accessor ps-step :initform "")
     (ppss-depth :initarg :depth :accessor ps-depth :initform 0)
     (ppss-in-str :initarg :instr :accessor ps-instr :initform 0)
     (ppss-in-com :initarg :incom :accessor ps-incom :initform 0)
     (m-pos :initarg :m-pos :accessor ps-mp :initform 0)))
  (let* ((buf (generate-new-buffer "syn5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(foo \"string\" (bar) ; comment\n baz)")
      (setq-local my-ppss-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil)
             (ppss-at
              (lambda (pos)
                (let ((ps (parse-partial-sexp 1 pos)))
                  (list (nth 0 ps) (nth 3 ps) (nth 4 ps))))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (let ((p (funcall ppss-at 12)))
          (push (ppss-snap :step "init"
                           :depth (car p)
                           :instr (cadr p)
                           :incom (nth 2 p)
                           :m-pos (marker-position m)) snaps))
        (put-text-property 6 14 'syntax-table '(15))
        (setq my-ppss-log (cons "syntax-tp-string" my-ppss-log))
        (let ((p (funcall ppss-at 12)))
          (push (ppss-snap :step "syntax-change"
                           :depth (car p)
                           :instr (cadr p)
                           :incom (nth 2 p)
                           :m-pos (marker-position m)) snaps))
        (goto-char 6)
        (insert "((extra)) ")
        (setq my-ppss-log (cons "ins@6" my-ppss-log))
        (let ((p (funcall ppss-at 15)))
          (push (ppss-snap :step "insert"
                           :depth (car p)
                           :instr (cadr p)
                           :incom (nth 2 p)
                           :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ps-step s) (ps-depth s)
                                                (ps-instr s) (ps-incom s)
                                                (ps-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ppss-log=%S"
                       results (reverse my-ppss-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ps-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ppss-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
