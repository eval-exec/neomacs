//! Combo: cl-eieio syntax-table / scan-lists + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests syntax-aware navigation with EIEIO objects tracking parse state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_scan_lists_paren_balancing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 62 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass paren-pos ()
    ((depth :initarg :depth :accessor pp-depth :initform 0)
     (pos :initarg :pos :accessor pp-pos :initform 1)
     (char :initarg :char :accessor pp-char :initform nil)))
  (let* ((buf (generate-new-buffer "syn1"))
         (positions nil))
    (with-current-buffer buf
      (insert "(a (b (c)) (d (e)))")
      (put-text-property 1 1 'level 0)
      (put-text-property 3 3 'level 1)
      (put-text-property 5 5 'level 2)
      (put-text-property 9 9 'level 2)
      (put-text-property 11 11 'level 1)
      (put-text-property 13 13 'level 2)
      (setq-local my-positions positions)
      (let* ((ov (make-overlay 1 18))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (scan-results nil))
        (undo-boundary)
        (goto-char 1)
        (let ((depth 0)
              (pos 1))
          (while (< pos (point-max))
            (let ((ch (char-after pos))
                  (syn (char-syntax (char-after pos))))
              (when (or (eq syn ?\() (eq syn ?\)))
                (if (eq syn ?\()
                    (setq depth (1+ depth))
                  (setq depth (1- depth)))
                (push (paren-pos :depth depth :pos pos :char ch) positions))
            (setq pos (1+ pos)))))
        (setq positions (reverse positions))
        (goto-char 1)
        (let ((forward-scan (scan-lists (point) 1 0)))
          (push (list 'forward-0 forward-scan) scan-results))
        (goto-char 3)
        (let ((forward-scan (scan-lists (point) 1 0)))
          (push (list 'forward-1 forward-scan) scan-results))
        (goto-char 5)
        (let ((forward-scan (scan-lists (point) 1 0)))
          (push (list 'forward-2 forward-scan) scan-results))
        (setq scan-results (reverse scan-results))
        (let ((pos-data (mapcar (lambda (p) (list (pp-depth p) (pp-pos p) (pp-char p))) positions)))
          (goto-char (point-max))
          (insert (format " | pos=%s scans=%s m=%d"
                         pos-data scan-results (marker-position m)))
          (set-marker m 5)
          (put-text-property (1- (point-max)) (point-max) 'paren-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-positions))))
    (kill-buffer buf)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_forward_backward_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass syntax-jump ()
    ((from :initarg :from :accessor sj-from :initform 1)
     (to :initarg :to :accessor sj-to :initform 1)
     (dir :initarg :dir :accessor sj-dir :initform "")))
  (let* ((buf (generate-new-buffer "syn2"))
         (jumps nil))
    (with-current-buffer buf
      (insert "(foo (bar baz) (qux))")
      (put-text-property 1 19 'type 'expr)
      (setq-local my-jumps jumps)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (forward-list 1)
        (push (syntax-jump :from 1 :to (point) :dir "forward-list") jumps)
        (push (list 'fwd-list (point)) results)
        (backward-list 1)
        (push (syntax-jump :from (point) :to 1 :dir "backward-list") jumps)
        (push (list 'bwd-list (point)) results)
        (goto-char 6)
        (forward-sexp 1)
        (push (syntax-jump :from 6 :to (point) :dir "forward-sexp") jumps)
        (push (list 'fwd-sexp-from-6 (point)) results)
        (backward-sexp 1)
        (push (syntax-jump :from (point) :to 6 :dir "backward-sexp") jumps)
        (push (list 'bwd-sexp (point)) results)
        (setq jumps (reverse jumps))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s jumps=%s m=%d"
                       results
                       (mapcar (lambda (j) (list (sj-dir j) (sj-from j) (sj-to j))) jumps)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'syntax-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-jumps))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_syntax_narrow_scan_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-scan ()
    ((region :initarg :region :accessor ns-region :initform "")
     (result :initarg :result :accessor ns-result :initform nil)))
    (let* ((buf (generate-new-buffer "syn3"))
           (ns1 (narrow-scan :region "full"))
           (ns2 (narrow-scan :region "inner")))
      (with-current-buffer buf
        (insert "(a (b (c (d) e) f) g)")
        (put-text-property 1 1 'depth 0)
        (put-text-property 3 3 'depth 1)
        (put-text-property 5 5 'depth 2)
        (put-text-property 7 7 'depth 3)
        (setq-local my-scans (list ns1 ns2))
        (let* ((ov (make-overlay 3 15))
               (_ (overlay-put ov 'priority 1))
               (m (make-marker))
               (_ (set-marker m 3))
               (results nil))
          (undo-boundary)
          (goto-char 1)
          (setf (ns-result ns1) (scan-lists (point) 1 0))
          (push (list 'full-scan (ns-result ns1)) results)
          (save-restriction
            (narrow-to-region 5 15)
            (goto-char (point-min))
            (condition-case err
                (progn
                  (setf (ns-result ns2) (scan-lists (point) 1 0))
                  (push (list 'narrow-scan (ns-result ns2)) results))
              (error
               (push (list 'narrow-error (cdr err)) results))))
          (setq results (reverse results))
          (goto-char (point-max))
          (insert (format " | results=%s ns1=%s ns2=%s m=%d"
                         results (ns-result ns1) (ns-result ns2)
                         (marker-position m)))
          (set-marker m 5)
          (put-text-property (1- (point-max)) (point-max) 'nscan-log t)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs
                  (marker-position m)
                  (buffer-string)
                  my-scans)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_parse_partial_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass parse-state ()
    ((pos :initarg :pos :accessor ps-pos :initform 1)
     (depth :initarg :depth :accessor ps-depth :initform 0)
     (contains :initarg :contains :accessor ps-contains :initform nil)))
  (let* ((buf (generate-new-buffer "syn4"))
         (states nil))
    (with-current-buffer buf
      (insert "(defun foo (x y)\n  (list x y))")
      (put-text-property 1 7 'syntax 'keyword)
      (put-text-property 8 11 'syntax 'name)
      (put-text-property 12 12 'syntax 'open)
      (setq-local my-states states)
      (let* ((ov (make-overlay 1 12))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (results nil))
        (undo-boundary)
        (dolist (pos '(1 7 12 14 20 26))
          (when (<= pos (point-max))
            (goto-char pos)
            (let ((pps (parse-partial-sexp (point-min) (point))))
              (push (parse-state :pos pos
                                :depth (car pps)
                                :contains (nth 2 pps))
                    states)
              (push (list pos (car pps) (nth 2 pps)) results))))
        (setq states (reverse states))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s states=%d m=%d"
                       results (length states) (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'pps-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-states))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_syntax_table_modify_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable words)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass syntax-change ()
    ((char :initarg :char :accessor sc-char :initform nil)
     (old-syntax :initarg :old-syntax :accessor sc-old :initform nil)
     (new-syntax :initarg :new-syntax :accessor sc-new :initform nil)))
  (let* ((buf (generate-new-buffer "syn5"))
         (changes nil)
         (c1 (syntax-change :char ?/ :old-syntax nil :new-syntax nil))
         (c2 (syntax-change :char ?$ :old-syntax nil :new-syntax nil)))
    (with-current-buffer buf
      (insert "a/b$c d")
      (put-text-property 1 2 'word 'a)
      (put-text-property 3 4 'word 'b)
      (put-text-property 5 6 'word 'c)
      (setq-local my-changes (list c1 c2))
      (let* ((ov (make-overlay 1 5))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 3))
             (results nil))
        (undo-boundary)
        (setf (sc-old c1) (char-syntax ?/)
              (sc-old c2) (char-syntax ?$))
        (push (list 'old-/ (sc-old c1) 'old-$ (sc-old c2)) results)
        (modify-syntax-entry ?/ "\"" (syntax-table))
        (modify-syntax-entry ?$ "\"" (syntax-table))
        (setf (sc-new c1) (char-syntax ?/)
              (sc-new c2) (char-syntax ?$))
        (push (list 'new-/ (sc-new c1) 'new-$ (sc-new c2)) results)
        (let ((words nil) (pos 1))
          (while (< pos (point-max))
            (let ((end (save-excursion (goto-char pos) (forward-word 1) (point))))
              (when (> end pos)
                (push (buffer-substring pos end) words))
              (setq pos (max (1+ pos) end)))))
        (setq words (reverse words))
        (push (list 'words words) results)
        (modify-syntax-entry ?/ "/" (syntax-table))
        (modify-syntax-entry ?/ "." (syntax-table))
        (modify-syntax-entry ?$ "_" (syntax-table))
        (let ((words2 nil) (pos 1))
          (while (< pos (point-max))
            (let ((end (save-excursion (goto-char pos) (forward-word 1) (point))))
              (when (> end pos)
                (push (buffer-substring pos end) words2))
              (setq pos (max (1+ pos) end)))))
        (setq words2 (reverse words2))
        (push (list 'words2 words2) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d" results (marker-position m)))
        (set-marker m 2)
        (put-text-property (1- (point-max)) (point-max) 'syntax-mod-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-changes))))
    (kill-buffer buf)))"#,
        expect,
    );
}
