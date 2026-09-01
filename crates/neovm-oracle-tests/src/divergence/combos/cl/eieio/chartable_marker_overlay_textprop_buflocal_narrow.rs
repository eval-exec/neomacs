//! Combo: cl-eieio char-table / category-table manipulation + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests character category tables, syntax class lookups, and char-table
//! interactions with overlays, markers, and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_chartable_syntax_class_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ct-snap ()
    ((step :initarg :step :accessor cts-step :initform "")
     (syn-a :initarg :syna :accessor cts-syna :initform nil)
     (syn-b :initarg :synb :accessor cts-synb :initform nil)
     (m-pos :initarg :m-pos :accessor cts-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ct1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA.BBBB+CCCC")
      (setq-local my-ct-log nil)
      (let* ((ov (make-overlay 5 10))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 7))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (ct-snap :step "init"
                      :syna (char-syntax ?A)
                      :synb (char-syntax ?.)
                      :m-pos (marker-position m)) snaps)
        (modify-syntax-entry ?. "(.")
        (modify-syntax-entry ?+ ") ")
        (setq my-ct-log (cons "modify-.+" my-ct-log))
        (push (ct-snap :step "modified"
                      :syna (char-syntax ?A)
                      :synb (char-syntax ?.)
                      :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "XX")
        (setq my-ct-log (cons "ins@5" my-ct-log))
        (push (ct-snap :step "edit"
                      :syna (char-syntax ?A)
                      :synb (char-syntax ?.)
                      :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 3 14)
          (push (ct-snap :step "narrow"
                        :syna (char-syntax ?A)
                        :synb (char-syntax ?.)
                        :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cts-step s) (cts-syna s)
                                                (cts-synb s) (cts-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ct-log=%S"
                       results (reverse my-ct-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cts-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (char-syntax ?.) (char-syntax ?+)
              my-ct-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_chartable_category_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Category ‘y’ is already defined\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cat-snap ()
    ((step :initarg :step :accessor cats-step :initform "")
     (cat-a :initarg :cata :accessor cats-cata :initform nil)
     (cat-b :initarg :catb :accessor cats-catb :initform nil)
     (m-pos :initarg :m-pos :accessor cats-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ct2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAABBBBCCCCDDDD")
      (setq-local my-cat-log nil)
      (let* ((ov (make-overlay 5 12))
             (_ (overlay-put ov 'face 'italic))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil)
             (ct (category-table)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (cat-snap :step "init"
                       :cata (aref (category-table) ?A)
                       :catb (aref (category-table) ?B)
                       :m-pos (marker-position m)) snaps)
        (define-category ?x "test-category-x" ct)
        (modify-category-entry ?A ?x ct)
        (setq my-cat-log (cons "cat-x-A" my-cat-log))
        (push (cat-snap :step "cat-A-x"
                       :cata (aref (category-table) ?A)
                       :catb (aref (category-table) ?B)
                       :m-pos (marker-position m)) snaps)
        (define-category ?y "test-category-y" ct)
        (modify-category-entry ?B ?y ct)
        (modify-category-entry ?C ?x ct)
        (setq my-cat-log (cons "cat-y-B+cat-x-C" my-cat-log))
        (push (cat-snap :step "multi-cat"
                       :cata (aref (category-table) ?A)
                       :catb (aref (category-table) ?B)
                       :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "MMM")
        (setq my-cat-log (cons "ins@5" my-cat-log))
        (push (cat-snap :step "edit"
                       :cata (aref (category-table) ?A)
                       :catb (aref (category-table) ?B)
                       :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cats-step s) (cats-cata s)
                                                (cats-catb s) (cats-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S cat-log=%S"
                       results (reverse my-cat-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cats-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (category-docstring ?x ct)
              my-cat-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_chartable_with_syntax_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass syntab-snap ()
    ((step :initarg :step :accessor sts-step :initform "")
     (depth-at-5 :initarg :d5 :accessor sts-d5 :initform 0)
     (depth-at-10 :initarg :d10 :accessor sts-d10 :initform 0)
     (m-pos :initarg :m-pos :accessor sts-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ct3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "((a(b)c)d)e(f)")
      (setq-local my-st-log nil)
      (let* ((ov (make-overlay 4 10))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 7))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (syntab-snap :step "init"
                          :d5 (car (parse-partial-sexp 1 5))
                          :d10 (car (parse-partial-sexp 1 10))
                          :m-pos (marker-position m)) snaps)
        (put-text-property 4 8 'syntax-table (string-to-syntax "_"))
        (setq my-st-log (cons "syntax-tp@4-8" my-st-log))
        (push (syntab-snap :step "syntax-tp"
                          :d5 (car (parse-partial-sexp 1 5))
                          :d10 (car (parse-partial-sexp 1 10))
                          :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (insert "XX")
        (setq my-st-log (cons "ins@3" my-st-log))
        (push (syntab-snap :step "edit"
                          :d5 (car (parse-partial-sexp 1 5))
                          :d10 (car (parse-partial-sexp 1 12))
                          :m-pos (marker-position m)) snaps)
        (put-text-property 6 12 'syntax-table (string-to-syntax "()"))
        (setq my-st-log (cons "syntax-tp@6-12-parens" my-st-log))
        (push (syntab-snap :step "syntax-tp2"
                          :d5 (car (parse-partial-sexp 1 5))
                          :d10 (car (parse-partial-sexp 1 12))
                          :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (sts-step s) (sts-d5 s)
                                                (sts-d10 s) (sts-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S st-log=%S"
                       results (reverse my-st-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sts-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-st-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_chartable_case_table_with_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass case-snap ()
    ((step :initarg :step :accessor css-step :initform "")
     (downcase-a :initarg :dca :accessor css-dca :initform nil)
     (upcase-z :initarg :ucz :accessor css-ucz :initform nil)
     (m-pos :initarg :m-pos :accessor css-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ct4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "ABCDEF-GHIJKL")
      (setq-local my-cs-log nil)
      (let* ((ov (make-overlay 7 12))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (case-snap :step "init"
                        :dca (downcase ?A)
                        :ucz (upcase ?z)
                        :m-pos (marker-position m)) snaps)
        (let ((ct (current-case-table)))
          (set-case-table ct))
        (push (case-snap :step "case-table-set"
                        :dca (downcase ?A)
                        :ucz (upcase ?z)
                        :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (insert "xxx")
        (setq my-cs-log (cons "ins@3" my-cs-log))
        (push (case-snap :step "edit"
                        :dca (downcase ?A)
                        :ucz (upcase ?z)
                        :m-pos (marker-position m)) snaps)
        (put-text-property 3 6 'face 'error)
        (setq my-cs-log (cons "face-tp@3-6" my-cs-log))
        (push (case-snap :step "face-change"
                        :dca (downcase ?A)
                        :ucz (upcase ?z)
                        :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (css-step s) (css-dca s)
                                                (css-ucz s) (css-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S cs-log=%S"
                       results (reverse my-cs-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'css-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cs-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_chartable_translate_region_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments make-string 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass trans-snap ()
    ((step :initarg :step :accessor trs-step :initform "")
     (buf-string :initarg :bs :accessor trs-bs :initform "")
     (m-pos :initarg :m-pos :accessor trs-mp :initform 0)
     (ov-bounds :initarg :ov :accessor trs-ov :initform nil)))
  (let* ((buf (generate-new-buffer "ct5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "abcdefghij-klmnopqrst")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-tr-log nil)
      (let* ((ov (make-overlay 5 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (trans-snap :step "init"
                         :bs (buffer-string)
                         :m-pos (marker-position m)
                         :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (let ((tab (make-string 256)))
          (dotimes (i 256)
            (aset tab i i))
          (aset tab ?a ?A)
          (aset tab ?b ?B)
          (aset tab ?c ?C)
          (aset tab ?d ?D)
          (aset tab ?e ?E)
          (aset tab ?f ?F)
          (aset tab ?g ?G)
          (aset tab ?h ?H)
          (aset tab ?i ?I)
          (aset tab ?j ?J)
          (translate-region 1 11 tab))
        (setq my-tr-log (cons "translate-a-j" my-tr-log))
        (push (trans-snap :step "translate"
                         :bs (buffer-string)
                         :m-pos (marker-position m)
                         :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (undo-boundary)
        (goto-char 5)
        (insert "ZZ")
        (setq my-tr-log (cons "ins@5" my-tr-log))
        (push (trans-snap :step "edit"
                         :bs (buffer-string)
                         :m-pos (marker-position m)
                         :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (trans-snap :step "undo-edit"
                         :bs (buffer-string)
                         :m-pos (marker-position m)
                         :ov (list (overlay-start ov) (overlay-end ov))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (trs-step s) (trs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S tr-log=%S"
                       results (reverse my-tr-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'trs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-tr-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
