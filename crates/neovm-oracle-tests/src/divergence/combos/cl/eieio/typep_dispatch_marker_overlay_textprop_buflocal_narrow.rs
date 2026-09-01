//! Combo: cl-typep + EIEIO class hierarchy type dispatch + overlays +
//! markers + textprop + buflocal + narrow + undo.
//! Tests type-checking interplay with editing state mutations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_typep_hierarchy_dispatch_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tp-base ()
    ((val :initarg :val :accessor tv :initform 0)
     (log :initarg :log :accessor tl :initform nil)))
  (defclass tp-mid-a (tp-base)
    ((tag-a :initarg :ta :accessor ta :initform "A")))
  (defclass tp-mid-b (tp-base)
    ((tag-b :initarg :tb :accessor tb :initform "B")))
  (defclass tp-leaf-ab (tp-mid-a tp-mid-b)
    ((leaf-tag :initarg :lt :accessor lt :initform "AB")))
  (let* ((buf (generate-new-buffer "tp1"))
         (snaps nil)
         (base (tp-base :val 0 :log nil))
         (mid-a (tp-mid-a :val 10 :log nil :ta "mid-a"))
         (mid-b (tp-mid-b :val 20 :log nil :tb "mid-b"))
         (leaf (tp-leaf-ab :val 30 :log nil :ta "la" :tb "lb" :lt "leaf")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) (point-max))
                           'zone (aref "abcdefghij" i)))
      (setq-local my-tp-log nil)
      (let* ((ov1 (make-overlay 6 20))
             (ov2 (make-overlay 26 40))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 5))
             (m1 (set-marker (make-marker) 10))
             (m2 (set-marker (make-marker) 30))
             (results nil)
             (type-check
              (lambda (obj label)
                (list label
                      (cl-typep obj 'tp-base)
                      (cl-typep obj 'tp-mid-a)
                      (cl-typep obj 'tp-mid-b)
                      (cl-typep obj 'tp-leaf-ab)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (funcall type-check base "base") results)
        (push (funcall type-check mid-a "mid-a") results)
        (push (funcall type-check mid-b "mid-b") results)
        (push (funcall type-check leaf "leaf") results)
        (goto-char 8)
        (insert "XXX")
        (push (cl-typep leaf 'tp-base) (tl leaf))
        (setq my-tp-log (cons "ins@8" my-tp-log))
        (push (list "edit1" (marker-position m1) (marker-position m2)
                    (cl-typep leaf 'tp-mid-a) (cl-typep leaf 'tp-mid-b)
                    (tv leaf)) results)
        (setf (tv leaf) (+ (tv leaf) (marker-position m1)))
        (save-restriction
          (narrow-to-region 5 35)
          (goto-char 10)
          (insert "YYY")
          (setf (tv mid-a) (+ (tv mid-a) (marker-position m1)))
          (push (cl-typep mid-a 'tp-mid-b) (tl mid-a))
          (setq my-tp-log (cons "narrow-ins" my-tp-log))
          (push (list "narrow" (marker-position m1) (marker-position m2)
                      (tv leaf) (tv mid-a)) results))
        (setf (tv base) (+ (tv base) (tv leaf) (tv mid-a) (tv mid-b)))
        (push (list "final-sum" (tv base) (tv mid-a) (tv mid-b) (tv leaf)
                    (marker-position m1) (marker-position m2)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S tp-log=%S tl-base=%S tl-leaf=%S"
                       results (reverse my-tp-log) (tl base) (tl leaf)))
        (set-marker m1 3)
        (set-marker m2 3)
        (put-text-property (1- (point-max)) (point-max) 'tp-log t)
        (list (buffer-string)
              (tv base) (tv mid-a) (tv mid-b) (tv leaf)
              (cl-typep leaf 'tp-base)
              (cl-typep leaf 'tp-mid-a)
              (cl-typep leaf 'tp-mid-b)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              my-tp-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_typep_dispatch_method_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass shape ()
    ((name :initarg :name :accessor sh-name :initform "")
     (area :initarg :area :accessor sh-area :initform 0)
     (log :initarg :log :accessor sh-log :initform nil)))
  (defclass circle (shape)
    ((radius :initarg :radius :accessor ci-radius :initform 0)))
  (defclass rectangle (shape)
    ((width :initarg :width :accessor re-width :initform 0)
     (height :initarg :height :accessor re-height :initform 0)))
  (defclass triangle (shape)
    ((base-len :initarg :base :accessor tri-base :initform 0)
     (tri-height :initarg :theight :accessor tri-height :initform 0)))
  (defmethod sh-describe ((obj shape))
    (format "shape:%S area:%d" (sh-name obj) (sh-area obj)))
  (defmethod sh-describe ((obj circle))
    (format "circle:%S r=%d area=%d" (sh-name obj) (ci-radius obj) (sh-area obj)))
  (defmethod sh-describe ((obj rectangle))
    (format "rect:%S w=%d h=%d area=%d" (sh-name obj) (re-width obj) (re-height obj) (sh-area obj)))
  (defmethod sh-describe ((obj triangle))
    (format "tri:%S b=%d h=%d area=%d" (sh-name obj) (tri-base obj) (tri-height obj) (sh-area obj)))
  (let* ((buf (generate-new-buffer "tp2"))
         (snaps nil)
         (shapes (list (circle :name "c1" :radius 5 :area 78 :log nil)
                       (rectangle :name "r1" :width 4 :height 6 :area 24 :log nil)
                       (triangle :name "t1" :base 3 :tri-height 8 :area 12 :log nil)
                       (shape :name "s1" :area 99 :log nil)))
         (results nil))
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
      (setq-local my-sh-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (dolist (s shapes)
          (push (list (sh-describe s)
                      (cl-typep s 'circle)
                      (cl-typep s 'rectangle)
                      (cl-typep s 'triangle)
                      (cl-typep s 'shape)) results))
        (goto-char 8)
        (insert "XXX")
        (dolist (s shapes)
          (when (cl-typep s 'circle)
            (setf (ci-radius s) (* (ci-radius s) 2))
            (setf (sh-area s) (* (sh-area s) 4)))
          (when (cl-typep s 'rectangle)
            (setf (re-width s) (* (re-width s) 2))
            (setf (sh-area s) (* (sh-area s) 2)))
          (when (cl-typep s 'triangle)
            (setf (tri-base s) (* (tri-base s) 3))
            (setf (sh-area s) (* (sh-area s) 3))))
        (setq my-sh-log (cons "ins@8" my-sh-log))
        (push (list "edit" (marker-position m)) results)
        (dolist (s shapes)
          (push (list (sh-describe s)) results))
        (save-restriction
          (narrow-to-region 5 35)
          (dolist (s shapes)
            (when (cl-typep s 'circle)
              (push (list "narrow-circle" (sh-describe s)) results))))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S sh-log=%S"
                       results (reverse my-sh-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sh-log t)
        (list (buffer-string)
              (mapcar (lambda (s) (list (sh-name s) (sh-area s))) shapes)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sh-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_typep_change_class_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass animal ()
    ((name :initarg :name :accessor an-name :initform "")
     (sound :initarg :sound :accessor an-sound :initform "")
     (log :initarg :log :accessor an-log :initform nil)))
  (defclass mammal (animal)
    ((fur-color :initarg :fur :accessor mm-fur :initform "")))
  (defclass bird (animal)
    ((wingspan :initarg :wings :accessor bd-wings :initform 0)))
  (defmethod an-describe ((obj animal))
    (format "%s says %s" (an-name obj) (an-sound obj)))
  (defmethod an-describe ((obj mammal))
    (format "%s(mammal,%s) says %s" (an-name obj) (mm-fur obj) (an-sound obj)))
  (defmethod an-describe ((obj bird))
    (format "%s(bird,wings=%d) says %s" (an-name obj) (bd-wings obj) (an-sound obj)))
  (let* ((buf (generate-new-buffer "tp3"))
         (snaps nil)
         (cat (mammal :name "cat" :sound "meow" :fur "orange" :log nil))
         (eagle (bird :name "eagle" :sound "screech" :wings 200 :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-an-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init"
                    (an-describe cat) (cl-typep cat 'mammal) (cl-typep cat 'bird)
                    (an-describe eagle) (cl-typep eagle 'mammal) (cl-typep eagle 'bird))
              results)
        (goto-char 8)
        (insert "XXX")
        (push (list "pre-change" (an-describe cat) (an-describe eagle)
                    (marker-position m)) results)
        (change-class cat 'bird :wings 30)
        (setf (an-sound cat) "chirp")
        (push (list "cat->bird"
                    (an-describe cat) (cl-typep cat 'mammal) (cl-typep cat 'bird)
                    (marker-position m)) results)
        (change-class eagle 'mammal :fur "brown")
        (setf (an-sound eagle) "roar")
        (push (list "eagle->mammal"
                    (an-describe eagle) (cl-typep eagle 'mammal) (cl-typep eagle 'bird)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 35)
          (push (list "narrow"
                      (cl-typep cat 'bird) (cl-typep eagle 'mammal)
                      (cl-typep cat 'animal) (cl-typep eagle 'animal)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S an-log=%S"
                       results (reverse my-an-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'an-log t)
        (list (buffer-string)
              (cl-typep cat 'mammal) (cl-typep cat 'bird)
              (cl-typep eagle 'mammal) (cl-typep eagle 'bird)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-an-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_typep_overlay_marker_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass node ()
    ((id :initarg :id :accessor nd-id :initform 0)
     (marker :initarg :marker :accessor nd-mk :initform nil)
     (overlay :initarg :overlay :accessor nd-ov :initform nil)
     (log :initarg :log :accessor nd-log :initform nil)))
  (defclass typed-node (node)
    ((ntype :initarg :ntype :accessor tn-type :initform "plain")))
  (let* ((buf (generate-new-buffer "tp4"))
         (snaps nil)
         (nodes nil)
         (types (list "bold" "italic" "underline" "shadow" "highlight")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (setq-local my-nd-log nil)
      (let* ((results nil)
             (m (set-marker (make-marker) 25)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (dotimes (i 5)
          (let* ((pos (+ 1 (* i 9)))
                 (end (min (+ pos 9) (point-max)))
                 (nd (typed-node :id (1+ i) :ntype (nth i types) :log nil))
                 (mk (set-marker (make-marker) pos))
                 (ov (make-overlay pos end)))
            (overlay-put ov 'face (intern (nth i types)))
            (overlay-put ov 'node-id (1+ i))
            (put-text-property pos end 'node-id (1+ i))
            (setf (nd-mk nd) mk)
            (setf (nd-ov nd) ov)
            (push nd nodes)))
        (setq nodes (reverse nodes))
        (push (list "init"
                    (mapcar (lambda (n)
                             (list (nd-id n) (tn-type n)
                                   (cl-typep n 'node) (cl-typep n 'typed-node)
                                   (marker-position (nd-mk n))
                                   (overlay-start (nd-ov n))))
                            nodes)
                    (marker-position m)) results)
        (goto-char 12)
        (insert "XXXXXX")
        (setq my-nd-log (cons "ins@12" my-nd-log))
        (push (list "edit"
                    (mapcar (lambda (n)
                             (list (nd-id n)
                                   (marker-position (nd-mk n))
                                   (overlay-start (nd-ov n))))
                            nodes)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 8 45)
          (dolist (n nodes)
            (when (cl-typep n 'typed-node)
              (push (list "narrow" (nd-id n) (tn-type n)
                          (marker-position (nd-mk n))) (nd-log n))))
          (push (list "narrow"
                      (mapcar (lambda (n)
                               (list (nd-id n) (marker-position (nd-mk n))))
                              nodes)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S nd-log=%S mpos=%d"
                       results my-nd-log (marker-position m)))
        (set-marker m 3)
        (list (buffer-string)
              (mapcar (lambda (n)
                       (list (nd-id n) (tn-type n)
                             (marker-position (nd-mk n))
                             (overlay-start (nd-ov n))
                             (cl-typep n 'typed-node)))
                      nodes)
              (marker-position m)
              my-nd-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_typep_predicate_with_slots_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass scored-item ()
    ((label :initarg :label :accessor si-label :initform "")
     (score :initarg :score :accessor si-score :initform 0)
     (active :initarg :active :accessor si-active :initform t)
     (log :initarg :log :accessor si-log :initform nil)))
  (defclass bonus-item (scored-item)
    ((bonus :initarg :bonus :accessor bi-bonus :initform 0)))
  (defmethod si-effective-score ((obj scored-item))
    (if (si-active obj) (si-score obj) 0))
  (defmethod si-effective-score ((obj bonus-item))
    (if (si-active obj) (+ (si-score obj) (bi-bonus obj)) 0))
  (let* ((buf (generate-new-buffer "tp5"))
         (snaps nil)
         (items (list (scored-item :label "a" :score 10 :active t :log nil)
                      (bonus-item :label "b" :score 20 :bonus 5 :active t :log nil)
                      (scored-item :label "c" :score 30 :active nil :log nil)
                      (bonus-item :label "d" :score 40 :bonus 15 :active t :log nil)))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ-KKKK-LLLL")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (put-text-property 41 45 'face 'error)
      (put-text-property 46 50 'face 'match)
      (setq-local my-si-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (total-fn
              (lambda ()
                (apply #'+ (mapcar (lambda (it)
                                    (if (cl-typep it 'bonus-item)
                                        (+ (si-score it) (bi-bonus it))
                                      (si-score it)))
                                   items)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init"
                    (mapcar (lambda (it)
                             (list (si-label it)
                                   (cl-typep it 'bonus-item)
                                   (si-effective-score it)))
                            items)
                    (funcall total-fn)
                    (marker-position m)) results)
        (goto-char 12)
        (insert "XXX")
        (dolist (it items)
          (when (and (cl-typep it 'bonus-item) (si-active it))
            (setf (bi-bonus it) (* (bi-bonus it) 2))))
        (setq my-si-log (cons "ins@12" my-si-log))
        (push (list "edit"
                    (mapcar (lambda (it)
                             (list (si-label it) (si-effective-score it)))
                            items)
                    (funcall total-fn)
                    (marker-position m)) results)
        (setf (si-active (nth 2 items)) t)
        (setf (si-active (nth 3 items)) nil)
        (push (list "toggle"
                    (mapcar (lambda (it)
                             (list (si-label it)
                                   (si-active it)
                                   (si-effective-score it)))
                            items)
                    (funcall total-fn)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (dolist (it items)
            (when (cl-typep it 'scored-item)
              (setf (si-score it) (+ (si-score it) 100))
              (push (si-label it) (si-log it))))
          (push (list "narrow"
                      (mapcar (lambda (it)
                               (list (si-label it) (si-score it)
                                     (si-effective-score it)))
                              items)
                      (funcall total-fn)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S si-log=%S"
                       results
                       (mapcar (lambda (it) (list (si-label it) (si-log it))) items)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'si-log t)
        (list (buffer-string)
              (mapcar (lambda (it)
                       (list (si-label it) (si-score it)
                             (si-active it)
                             (cl-typep it 'bonus-item)
                             (si-effective-score it)))
                      items)
              (funcall total-fn)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-si-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
