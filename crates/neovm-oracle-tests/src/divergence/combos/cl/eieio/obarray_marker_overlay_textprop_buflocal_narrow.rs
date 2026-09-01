//! Combo: cl-eieio obarray / intern / unintern / mapatoms + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests symbol table manipulation with EIEIO objects, intern-soft,
//! obarray operations, and mapatoms with editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_obarray_intern_with_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass sym-entry ()
    ((name :initarg :name :accessor se-name :initform "")
     (count :initarg :count :accessor se-count :initform 0)))
  (let* ((buf (generate-new-buffer "ob1"))
         (my-ob (make-vector 7 0))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-ob-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil)
             (count-ob
              (lambda ()
                (let ((n 0))
                  (mapatoms (lambda (s) (setq n (1+ n))) my-ob)
                  n))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (intern "test-sym-1" my-ob)
        (intern "test-sym-2" my-ob)
        (intern "test-sym-3" my-ob)
        (push (list "init" (funcall count-ob)
                    (intern-soft "test-sym-1" my-ob)) results)
        (set (intern "test-sym-1" my-ob)
             (sym-entry :name "s1" :count 1))
        (push (list "set-sym" (se-count (symbol-value (intern-soft "test-sym-1" my-ob)))
                    (funcall count-ob)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-ob-log (cons "ins@8" my-ob-log))
        (let ((entry (symbol-value (intern-soft "test-sym-1" my-ob))))
          (setf (se-count entry) (1+ (se-count entry))))
        (push (list "edit" (se-count (symbol-value (intern-soft "test-sym-1" my-ob)))
                    (marker-position m)) results)
        (intern "test-sym-4" my-ob)
        (setq my-ob-log (cons "intern-4" my-ob-log))
        (push (list "intern4" (funcall count-ob)
                    (intern-soft "test-sym-4" my-ob)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ob-log=%S"
                       results (reverse my-ob-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ob-log t)
        (list (buffer-string)
              (funcall count-ob)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ob-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_obarray_unintern_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ob-track ()
    ((label :initarg :label :accessor ot-label :initform "")
     (val :initarg :val :accessor ot-val :initform 0)))
  (let* ((buf (generate-new-buffer "ob2"))
         (my-ob (make-vector 5 0))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-ob2-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil)
             (count-ob
              (lambda ()
                (let ((n 0))
                  (mapatoms (lambda (s) (setq n (1+ n))) my-ob)
                  n))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (intern "alpha" my-ob)
        (intern "beta" my-ob)
        (intern "gamma" my-ob)
        (set (intern "alpha" my-ob) (ob-track :label "a" :val 10))
        (set (intern "beta" my-ob) (ob-track :label "b" :val 20))
        (push (list "init" (funcall count-ob)
                    (ot-val (symbol-value (intern-soft "alpha" my-ob)))
                    (intern-soft "alpha" my-ob)) results)
        (unintern "beta" my-ob)
        (setq my-ob2-log (cons "unintern-beta" my-ob2-log))
        (push (list "unintern" (funcall count-ob)
                    (intern-soft "beta" my-ob)
                    (intern-soft "alpha" my-ob)) results)
        (goto-char 8)
        (insert "QQQ")
        (setq my-ob2-log (cons "ins@8" my-ob2-log))
        (push (list "edit" (funcall count-ob)
                    (ot-val (symbol-value (intern-soft "alpha" my-ob)))
                    (marker-position m)) results)
        (intern "delta" my-ob)
        (set (intern "delta" my-ob) (ob-track :label "d" :val 40))
        (setq my-ob2-log (cons "intern-delta" my-ob2-log))
        (push (list "intern-d" (funcall count-ob)
                    (ot-val (symbol-value (intern-soft "delta" my-ob)))) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ob2-log=%S"
                       results (reverse my-ob2-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ob2-log t)
        (list (buffer-string)
              (funcall count-ob)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ob2-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_obarray_mapatoms_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass map-sym ()
    ((key :initarg :key :accessor ms-key :initform "")
     (data :initarg :data :accessor ms-data :initform nil)))
  (let* ((buf (generate-new-buffer "ob3"))
         (my-ob (make-vector 3 0))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-map-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil)
             (collect-syms
              (lambda ()
                (let ((syms nil))
                  (mapatoms (lambda (s)
                             (push (list (symbol-name s)
                                        (when (boundp s)
                                          (if (cl-typep (symbol-value s) 'map-sym)
                                              (ms-key (symbol-value s))
                                            'not-map-sym))))
                               syms))
                  my-ob)
                  (sort syms (lambda (a b) (string< (car a) (car b))))))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (dolist (name (list "sym-a" "sym-b" "sym-c"))
          (set (intern name my-ob)
               (map-sym :key name :data (list name))))
        (push (list "init" (funcall collect-syms)) results)
        (goto-char 8)
        (insert "MMM")
        (setq my-map-log (cons "ins@8" my-map-log))
        (let ((entry (symbol-value (intern-soft "sym-b" my-ob))))
          (setf (ms-data entry) (cons "edited" (ms-data entry))))
        (push (list "edit" (funcall collect-syms)
                    (marker-position m)) results)
        (unintern "sym-a" my-ob)
        (setq my-map-log (cons "unintern-a" my-map-log))
        (push (list "unintern" (funcall collect-syms)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S map-log=%S"
                       results (reverse my-map-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ms-log t)
        (list (buffer-string)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-map-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_obarray_narrow_edit_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-sym ()
    ((tag :initarg :tag :accessor ns-tag :initform "")
     (pos :initarg :pos :accessor ns-pos :initform 0)))
  (let* ((buf (generate-new-buffer "ob4"))
         (my-ob (make-vector 5 0))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-ns-log nil)
      (let* ((ov (make-overlay 10 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil)
             (count-ob
              (lambda ()
                (let ((n 0))
                  (mapatoms (lambda (_) (setq n (1+ n))) my-ob)
                  n))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (intern "narrow-a" my-ob)
        (intern "narrow-b" my-ob)
        (set (intern "narrow-a" my-ob)
             (narrow-sym :tag "a" :pos (marker-position m)))
        (push (list "init" (funcall count-ob)
                    (ns-pos (symbol-value (intern-soft "narrow-a" my-ob)))
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 8 22)
          (push (list "narrow" (funcall count-ob)
                      (ns-pos (symbol-value (intern-soft "narrow-a" my-ob)))
                      (marker-position m)) results)
          (goto-char 10)
          (insert "XXX")
          (setq my-ns-log (cons "ins-narrow@10" my-ns-log))
          (let ((entry (symbol-value (intern-soft "narrow-a" my-ob))))
            (setf (ns-pos entry) (marker-position m)))
          (push (list "edit-narrow" (funcall count-ob)
                      (ns-pos (symbol-value (intern-soft "narrow-a" my-ob)))
                      (marker-position m)) results))
        (push (list "widen" (funcall count-ob)
                    (ns-pos (symbol-value (intern-soft "narrow-a" my-ob)))
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ns-log=%S"
                       results (reverse my-ns-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ns-log t)
        (list (buffer-string)
              (funcall count-ob)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ns-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_obarray_symbol_function_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass sym-meta ()
    ((name :initarg :name :accessor sm-name :initform "")
     (plist-data :initarg :plist :accessor sm-plist :initform nil)))
  (let* ((buf (generate-new-buffer "ob5"))
         (my-ob (make-vector 7 0))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-sm-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (let ((sym (intern "meta-sym" my-ob)))
          (set sym (sym-meta :name "meta" :plist (list :a 1 :b 2)))
          (put sym :custom-prop "hello")
          (push (list "init" (sm-name (symbol-value sym))
                      (get sym :custom-prop)
                      (symbol-plist sym)) results)
          (goto-char 8)
          (insert "ZZZ")
          (setq my-sm-log (cons "ins@8" my-sm-log))
          (put sym :custom-prop "world")
          (push (list "edit" (get sym :custom-prop)
                      (marker-position m)) results)
          (let ((entry (symbol-value sym)))
            (setf (sm-plist entry) (plist-put (sm-plist entry) :c 3)))
          (push (list "plist-update" (sm-plist (symbol-value sym))
                      (get sym :custom-prop)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S sm-log=%S"
                       results (reverse my-sm-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sm-log t)
        (list (buffer-string)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sm-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
