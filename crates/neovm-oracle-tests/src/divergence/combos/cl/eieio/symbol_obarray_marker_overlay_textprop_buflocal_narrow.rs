//! Combo: cl-eieio symbol/obarray interaction + marker + overlay + textprop + buflocal + undo.
//! Tests defclass symbol creation, intern-soft, symbol-value, obarray interaction with EIEIO.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_defclass_creates_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable cls)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass sym-test-foo ()
    ((val :initarg :val :accessor stf-val :initform 0)))
  (let* ((buf (generate-new-buffer "ob1"))
         (obj (sym-test-foo :val 42)))
    (with-current-buffer buf
      (insert "CLASS:sym-test-foo:val=42")
      (put-text-property 1 6 'field 'type)
      (put-text-property 7 20 'field 'class)
      (put-text-property 21 26 'field 'val)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10))
             (class-sym (intern-soft "sym-test-foo"))
             (accessor-sym (intern-soft "stf-val"))
             (constructor-sym (intern-soft "sym-test-foo"))
             (class-bound (and class-sym (fboundp class-sym)))
             (acc-bound (and accessor-sym (fboundp accessor-sym)))
             (class-type (and class-sym (symbol-function class-sym))))
        (undo-boundary)
        (setf (stf-val obj) 100)
        (let ((new-val (stf-val obj))
              (sym-val-list nil))
          (mapatoms (lambda (s)
                      (when (and (string-prefix-p "stf-" (symbol-name s))
                                 (fboundp s))
                        (push (symbol-name s) sym-val-list)))
                    obarray)
          (goto-char 21)
          (insert (format "v=%d:syms=%s" new-val (mapconcat #'identity (sort sym-val-list #'string<) ",")))
          (setf (marker-position m) 23)
          (put-text-property 21 (+ 21 (length (format "v=%d:syms=%s"
                                                        new-val (mapconcat #'identity (sort sym-val-list #'string<) ","))))
                            'sym-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-val (stf-val my-obj))
              (cls (intern-soft "sym-test-foo"))
              (cls-bound (and cls (fboundp cls))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-val cls-bound class-bound acc-bound
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_symbol_plist_class_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass annotated-class ()
    ((data :initarg :data :accessor ac-data :initform nil)
     (label :initarg :label :accessor ac-label :initform "")))
  (let* ((buf (generate-new-buffer "ob2"))
         (obj (annotated-class :data '(1 2 3) :label "test"))
         (class-symbol (intern-soft "annotated-class")))
    (with-current-buffer buf
      (insert "ANNO:test:data=(1,2,3)")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 10 'field 'label)
      (put-text-property 11 22 'field 'data)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'italic))
             (m (make-marker))
             (_ (set-marker m 8))
             (class-plist (and class-symbol (symbol-plist class-symbol)))
             (has-eieio-meta (and class-plist (plist-member class-plist 'eieio-class-definition))))
        (undo-boundary)
        (setf (ac-label obj) "updated")
        (setf (ac-data obj) (append (ac-data obj) '(4 5)))
        (let* ((new-data (ac-data obj))
               (data-str (mapconcat (lambda (x) (format "%s" x)) new-data ","))
               (new-plist (and class-symbol (symbol-plist class-symbol)))
               (still-meta (and new-plist (plist-member new-plist 'eieio-class-definition))))
          (goto-char 6)
          (insert (format "%s:%s:meta=%s" (ac-label obj) data-str still-meta))
          (setf (marker-position m) 12)
          (put-text-property 6 (+ 6 (length (format "%s:%s:meta=%s"
                                                      (ac-label obj) data-str still-meta)))
                            'anno-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (label (ac-label my-obj))
              (data (ac-data my-obj))
              (final-plist (and class-symbol (symbol-plist class-symbol))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs label data has-eieio-meta final-plist
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mapatoms_find_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function eieio--class)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mapat-alpha ()
    ((x :initarg :x :accessor ma-x :initform 0)))
  (defclass mapat-beta ()
    ((y :initarg :y :accessor mb-y :initform 0)))
  (defclass mapat-gamma (mapat-alpha mapat-beta)
    ((z :initarg :z :accessor mg-z :initform 0)))
  (let* ((buf (generate-new-buffer "ob3"))
         (g (mapat-gamma :x 1 :y 2 :z 3))
         (found-classes nil))
    (mapatoms (lambda (s)
                (when (string-prefix-p "mapat-" (symbol-name s))
                  (when (and (fboundp s) (eieio--class (symbol-function s)))
                    (push (symbol-name s) found-classes))))
              obarray)
    (setq found-classes (sort found-classes #'string<))
    (with-current-buffer buf
      (insert (format "CLASSES:%s" (mapconcat #'identity found-classes ",")))
      (put-text-property 1 8 'field 'header)
      (put-text-property 9 (+ 8 (length (mapconcat #'identity found-classes ","))) 'field 'names)
      (setq-local my-obj g)
      (setq-local class-names found-classes)
      (let* ((ov (make-overlay 9 (+ 8 (length (mapconcat #'identity found-classes ",")))))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 12)))
        (undo-boundary)
        (setf (ma-x g) 10
              (mb-y g) 20
              (mg-z g) 30)
        (let ((x (ma-x g))
              (y (mb-y g))
              (z (mg-z g))
              (cpl (eieio-class-precedence-list (eieio-object-class g)))
              (cpl-names (mapcar (lambda (c) (symbol-name (eieio-class-name c))) cpl)))
          (goto-char (point-max))
          (insert (format "|x=%d:y=%d:z=%d:cpl=%s" x y z cpl-names))
          (setf (marker-position m) 15)
          (put-text-property (1+ (length "CLASSES:")) (+ (length "CLASSES:") 10) 'updated t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (x (ma-x my-obj))
              (y (mb-y my-obj))
              (z (mg-z my-obj))
              (cn class-names))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs x y z cn
                (marker-position m)
                (buffer-string)
                my-obj class-names)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_intern_construct_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dyn-item ()
    ((tag :initarg :tag :accessor di-tag :initform "")
     (order :initarg :order :accessor di-order :initform 0)))
  (let* ((buf (generate-new-buffer "ob4"))
         (items nil))
    (dolist (spec '(("alpha" . 1) ("beta" . 2) ("gamma" . 3)))
      (let ((item (dyn-item :tag (car spec) :order (cdr spec))))
        (push item items)))
    (setq items (nreverse items))
    (with-current-buffer buf
      (insert "DYN:alpha:1-beta:2-gamma:3")
      (put-text-property 1 4 'field 'header)
      (put-text-property 5 10 'field 't1)
      (put-text-property 11 12 'field 'o1)
      (put-text-property 13 17 'field 't2)
      (put-text-property 18 19 'field 'o2)
      (put-text-property 20 25 'field 't3)
      (put-text-property 26 27 'field 'o3)
      (setq-local my-items items)
      (let* ((ov (make-overlay 5 19))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8))
             (tags-before (mapcar (lambda (i) (cons (di-tag i) (di-order i))) items)))
        (undo-boundary)
        (dolist (item items)
          (setf (di-order item) (* (di-order item) 10)))
        (let* ((tags-after (mapcar (lambda (i) (cons (di-tag i) (di-order i))) items))
               (sym-names nil))
          (dolist (item items)
            (let ((sym (intern (format "dyn-item-%s" (di-tag item)))))
              (set sym (di-order item))
              (push (cons (symbol-name sym) (symbol-value sym)) sym-names)))
          (setq sym-names (sort sym-names (lambda (a b) (string< (car a) (car b)))))
          (goto-char 5)
          (insert (format "%s->%s|%s"
                         tags-before tags-after
                         (mapconcat (lambda (p) (format "%s=%s" (car p) (cdr p))) sym-names ",")))
          (setf (marker-position m) 15)
          (put-text-property 5 (+ 5 (length (format "%s->%s|%s"
                                                      tags-before tags-after
                                                      (mapconcat (lambda (p) (format "%s=%s" (car p) (cdr p))) sym-names ","))))
                            'dyn-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (orders (mapcar (lambda (i) (di-order i)) my-items))
              (tags (mapcar (lambda (i) (di-tag i)) my-items)))
          (primitive-undo 1 buffer-undo-list)
          (dolist (item my-items)
            (let ((sym (intern-soft (format "dyn-item-%s" (di-tag item)))))
              (when (and sym (boundp sym)) (makunbound sym))))
          (list mp os oe bs orders tags
                (marker-position m)
                (buffer-string)
                my-items)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_redefinition_preserve_instances() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable has-c)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass redef-test ()
    ((a :initarg :a :accessor rt-a :initform 0)
     (b :initarg :b :accessor rt-b :initform 0)))
  (let* ((buf (generate-new-buffer "ob5"))
         (obj (redef-test :a 10 :b 20)))
    (with-current-buffer buf
      (insert "REDEF:a=10:b=20")
      (put-text-property 1 6 'field 'header)
      (put-text-property 7 11 'field 'a)
      (put-text-property 12 16 'field 'b)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 7 16))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 9))
             (pre-class (eieio-class-name (eieio-object-class obj)))
             (pre-slots (mapcar (lambda (s) (slot-value obj s)) '(a b))))
        (undo-boundary)
        (defclass redef-test ()
          ((a :initarg :a :accessor rt-a :initform 0)
           (b :initarg :b :accessor rt-b :initform 0)
           (c :initarg :c :accessor rt-c :initform 99)))
        (let ((post-class (eieio-class-name (eieio-object-class obj)))
              (post-a (rt-a obj))
              (post-b (rt-b obj))
              (has-c (slot-exists-p obj 'c))
              (c-default (and has-c (slot-boundp obj 'c) (rt-c obj))))
          (setf (rt-a obj) 100
                (rt-c obj) 42)
          (let ((new-a (rt-a obj))
                (new-c (rt-c obj)))
            (goto-char 7)
            (insert (format "%s[%s:%s]->[%s:%s:%s]"
                           pre-class pre-a pre-b
                           post-class new-a post-b new-c))
            (setf (marker-position m) 12)
            (put-text-property 7 (+ 7 (length (format "%s[%s:%s]->[%s:%s:%s]"
                                                        pre-class pre-a pre-b
                                                        post-class new-a post-b new-c)))
                              'redef-result t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a (rt-a my-obj))
              (b (rt-b my-obj))
              (has-c (slot-exists-p my-obj 'c)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a b has-c
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}
