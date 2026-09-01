//! Combo: cl-eieio syntax-ppss/parse-partial-sexp + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests sexp parsing with EIEIO objects tracking parse state, overlays, and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_parse_partial_sexp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass parse-state ()
    ((pos :initarg :pos :accessor ps-pos :initform 0)
     (depth :initarg :depth :accessor ps-depth :initform 0)
     (in-string :initarg :in-string :accessor ps-instr :initform nil)
     (in-comment :initarg :in-comment :accessor ps-incmt :initform nil)))
  (let* ((buf (generate-new-buffer "sy1"))
         (states nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(defun foo ()\n  (let ((x \"hello\"))\n    (list x (+ 1 2) x)))")
      (put-text-property 1 15 'zone 'defn)
      (put-text-property 16 30 'zone 'body)
      (put-text-property 31 50 'zone 'tail)
      (setq-local my-states states)
      (let* ((ov (make-overlay 16 35))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 20))
             (results nil))
        (undo-boundary)
        (dolist (pos '(1 5 10 15 20 25 30 35 40 45))
          (let ((ppss (parse-partial-sexp 1 pos)))
            (push (parse-state :pos pos
                              :depth (nth 0 ppss)
                              :in-string (nth 3 ppss)
                              :in-comment (nth 4 ppss))
                  states)))
        (setq states (reverse states))
        (setq results (mapcar (lambda (s) (list (ps-pos s) (ps-depth s))) states))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 10)
        (put-text-property (1- (point-max)) (point-max) 'ps-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length states)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_parse_sexp_edit_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 19 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass reparse-snap ()
    ((step :initarg :step :accessor rs-step :initform "")
     (depth-at-15 :initarg :depth :accessor rs-depth :initform 0)
     (in-string-at-15 :initarg :in-str :accessor rs-instr :initform nil)
     (buf-string :initarg :buf-string :accessor rs-bs :initform "")))
  (let* ((buf (generate-new-buffer "sy2"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(foo (bar \"abc\") (baz 123))")
      (put-text-property 1 6 'zone 'fn)
      (put-text-property 7 18 'zone 'arg1)
      (put-text-property 19 29 'zone 'arg2)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 18))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 12))
             (results nil))
        (undo-boundary)
        (let ((ppss (parse-partial-sexp 1 15)))
          (push (reparse-snap :step "init"
                             :depth (nth 0 ppss)
                             :in-str (nth 3 ppss)
                             :buf-string (buffer-string)) snaps))
        (goto-char 13)
        (insert "XY")
        (let ((ppss (parse-partial-sexp 1 17)))
          (push (reparse-snap :step "insert-in-string"
                             :depth (nth 0 ppss)
                             :in-str (nth 3 ppss)
                             :buf-string (buffer-string)) snaps))
        (goto-char 10)
        (insert "(")
        (let ((ppss (parse-partial-sexp 1 18)))
          (push (reparse-snap :step "insert-paren"
                             :depth (nth 0 ppss)
                             :in-str (nth 3 ppss)
                             :buf-string (buffer-string)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rs-step s) (rs-depth s) (rs-instr s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'rs-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_parse_narrow_restricted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-parse ()
    ((narrow-bounds :initarg :narrow :accessor np-bounds :initform nil)
     (depth-at-mid :initarg :depth :accessor np-depth :initform 0)
     (last-sexp :initarg :last-sexp :accessor np-sexp :initform nil)))
  (let* ((buf (generate-new-buffer "sy3"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(progn\n  (foo (bar 1))\n  (baz (qux 2))\n  (end))")
      (put-text-property 1 7 'zone 'prgn)
      (put-text-property 8 22 'zone 'f1)
      (put-text-property 23 38 'zone 'f2)
      (put-text-property 39 48 'zone 'end)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 8 38))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (undo-boundary)
        (let ((ppss (parse-partial-sexp 1 15)))
          (push (narrow-parse :narrow (list (point-min) (point-max))
                             :depth (nth 0 ppss)
                             :last-sexp (nth 1 ppss)) snaps))
        (save-restriction
          (narrow-to-region 8 22)
          (let ((ppss (parse-partial-sexp (point-min) (point-max))))
            (push (narrow-parse :narrow (list (point-min) (point-max))
                               :depth (nth 0 ppss)
                               :last-sexp (nth 1 ppss)) snaps)))
        (let ((ppss (parse-partial-sexp 1 15)))
          (push (narrow-parse :narrow (list (point-min) (point-max))
                             :depth (nth 0 ppss)
                             :last-sexp (nth 1 ppss)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (np-bounds s) (np-depth s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'np-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_parse_syntax_table_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass syntax-ov-snap ()
    ((pos :initarg :pos :accessor sos-pos :initform 0)
     (depth :initarg :depth :accessor sos-depth :initform 0)
     (char-syntax :initarg :char-syntax :accessor sos-syn :initform 0)))
  (let* ((buf (generate-new-buffer "sy4"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(aaa (bbb) ccc (ddd) eee)")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 26 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 13))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (dolist (pos '(1 5 7 10 13 15 20 25))
          (let ((ppss (parse-partial-sexp 1 pos))
                (syn (char-syntax (char-after pos))))
            (push (syntax-ov-snap :pos pos
                                 :depth (nth 0 ppss)
                                 :char-syntax syn)
                  snaps)))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sos-pos s) (sos-depth s) (sos-syn s))) snaps))
        (goto-char 10)
        (insert "(")
        (let ((ppss (parse-partial-sexp 1 11)))
          (push (list 'after-insert (nth 0 ppss)) results))
        (delete-region 10 11)
        (let ((ppss (parse-partial-sexp 1 10)))
          (push (list 'after-delete (nth 0 ppss)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'sos-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_parse_forward_sexps_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass sexp-nav-snap ()
    ((step :initarg :step :accessor sns-step :initform "")
     (point-before :initarg :before :accessor sns-before :initform 0)
     (point-after :initarg :after :accessor sns-after :initform 0)
     (depth :initarg :depth :accessor sns-depth :initform 0)))
  (let* ((buf (generate-new-buffer "sy5"))
         (snaps nil))
    (with-current-buffer buf
      (emacs-lisp-mode)
      (insert "(a (b (c d) e) f (g h) i)")
      (put-text-property 1 3 'zone 'a)
      (put-text-property 4 16 'zone 'b)
      (put-text-property 17 24 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 4 16))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (let ((before (point)))
          (forward-sexp 1)
          (let ((ppss (parse-partial-sexp 1 (point))))
            (push (sexp-nav-snap :step "fwd-1"
                                :before before
                                :after (point)
                                :depth (nth 0 ppss)) snaps)))
        (let ((before (point)))
          (forward-sexp 1)
          (let ((ppss (parse-partial-sexp 1 (point))))
            (push (sexp-nav-snap :step "fwd-2"
                                :before before
                                :after (point)
                                :depth (nth 0 ppss)) snaps)))
        (let ((before (point)))
          (forward-sexp -1)
          (let ((ppss (parse-partial-sexp 1 (point))))
            (push (sexp-nav-snap :step "back-1"
                                :before before
                                :after (point)
                                :depth (nth 0 ppss)) snaps)))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sns-step s) (sns-before s) (sns-after s) (sns-depth s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sns-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
