//! Combo: cl-eieio multiple inheritance diamond + method resolution + marker + overlay + textprop + buflocal.
//! Tests diamond inheritance, C3 linearization, method dispatch order with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_diamond_inheritance_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass base-a ()
    ((a-val :initarg :a-val :accessor a-val :initform "A")))
  (defclass base-b ()
    ((b-val :initarg :b-val :accessor b-val :initform "B")))
  (defclass middle-c (base-a base-b)
    ((c-val :initarg :c-val :accessor c-val :initform "C")))
  (defclass middle-d (base-b base-a)
    ((d-val :initarg :d-val :accessor d-val :initform "D")))
  (defclass diamond (middle-c middle-d)
    ((x-val :initarg :x-val :accessor x-val :initform "X")))
  (defgeneric describe-hierarchy (obj)
    (:documentation "Describe the object hierarchy."))
  (defmethod describe-hierarchy ((obj base-a))
    (format "A[a=%s]" (a-val obj)))
  (defmethod describe-hierarchy :before ((obj base-a))
    (put-text-property 1 5 'hierarchy 'a))
  (defmethod describe-hierarchy ((obj base-b))
    (format "B[b=%s]" (b-val obj)))
  (defmethod describe-hierarchy ((obj middle-c))
    (format "C[c=%s,%s]" (c-val obj) (cl-call-next-method)))
  (defmethod describe-hierarchy ((obj middle-d))
    (format "D[d=%s,%s]" (d-val obj) (cl-call-next-method)))
  (defmethod describe-hierarchy ((obj diamond))
    (format "Diamond[x=%s,%s]" (x-val obj) (cl-call-next-method)))
  (let* ((buf (generate-new-buffer "di1"))
         (d (diamond :a-val "alpha" :b-val "beta" :c-val "gamma" :d-val "delta" :x-val "xray")))
    (with-current-buffer buf
      (insert "DIAMOND:alpha-beta-gamma-delta-xray")
      (put-text-property 1 8 'layer 'diamond)
      (put-text-property 9 14 'layer 'a)
      (put-text-property 15 19 'layer 'b)
      (put-text-property 20 25 'layer 'c)
      (put-text-property 26 31 'layer 'd)
      (put-text-property 32 36 'layer 'x)
      (setq-local my-obj d)
      (let* ((ov (make-overlay 1 14))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (let ((cpl (eieio-class-precedence-list (eieio-object-class d)))
              (cpl-names (mapcar (lambda (c) (eieio-class-name c)) cpl))
              (desc (describe-hierarchy d)))
          (setf (a-val d) "A2"
                (b-val d) "B2")
          (let ((new-desc (describe-hierarchy d)))
            (goto-char 1)
            (insert (format "[%s|%s]" desc new-desc))
            (setf (marker-position m) 10)
            (put-text-property 1 (+ 1 (length (format "[%s|%s]" desc new-desc)))
                              'diamond-desc t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a (a-val my-obj))
              (b (b-val my-obj))
              (x (x-val my-obj))
              (desc2 (describe-hierarchy my-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a b x desc2
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_inherit_slot_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass readable ()
    ((content :initarg :content :accessor readable-content :initform "")
     (encoding :initarg :encoding :accessor readable-encoding :initform "utf-8")))
  (defclass writable ()
    ((modified :initarg :modified :accessor writable-modified :initform nil)
     (path :initarg :path :accessor writable-path :initform "/tmp")))
  (defclass readwritable (readable writable)
    ((locked :initarg :locked :accessor rw-locked :initform nil)))
  (defgeneric sync-object (obj))
  (defmethod sync-object ((obj readable))
    (format "read[enc=%s,len=%d]" (readable-encoding obj) (length (readable-content obj))))
  (defmethod sync-object ((obj writable))
    (format "write[mod=%s,path=%s]" (writable-modified obj) (writable-path obj)))
  (defmethod sync-object ((obj readwritable))
    (format "rw[%s,%s,lock=%s]" (cl-call-next-method)
            (writable-path obj) (rw-locked obj)))
  (let* ((buf (generate-new-buffer "mi1"))
         (rw (readwritable :content "hello world" :encoding "utf-8"
                           :modified t :path "/data/file.txt" :locked nil)))
    (with-current-buffer buf
      (insert "RW:/data/file.txt:hello-world:utf8")
      (put-text-property 1 3 'type 'rw)
      (put-text-property 4 18 'field 'path)
      (put-text-property 19 30 'field 'content)
      (put-text-property 31 34 'field 'encoding)
      (setq-local rwobj rw)
      (let* ((ov1 (make-overlay 4 18))
             (ov2 (make-overlay 19 34))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let ((sync1 (sync-object rw))
              (path1 (writable-path rw))
              (content1 (readable-content rw)))
          (setf (readable-content rw) "updated content"
                (writable-modified rw) t
                (rw-locked rw) t)
          (let ((sync2 (sync-object rw)))
            (goto-char 19)
            (insert (format "[%s->%s]" sync1 sync2))
            (setf (marker-position m) 25)
            (put-text-property 19 (+ 19 (length (format "[%s->%s]" sync1 sync2)))
                              'sync-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (bs (buffer-string))
              (content (readable-content rwobj))
              (locked (rw-locked rwobj))
              (mod (writable-modified rwobj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 bs content locked mod
                (marker-position m)
                (buffer-string)
                rwobj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_c3_linearization_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass animal-base ()
    ((species :initarg :species :accessor ab-species :initform "unknown")))
  (defclass mammal (animal-base) ())
  (defclass flyer (animal-base) ())
  (defclass bat (mammal flyer)
    ((echolocation :initarg :echolocation :accessor bat-echolocation :initform t)))
  (defgeneric classify (obj))
  (defmethod classify ((obj animal-base))
    (cons 'animal-base (ab-species obj)))
  (defmethod classify ((obj mammal))
    (cons 'mammal (cl-call-next-method)))
  (defmethod classify ((obj flyer))
    (cons 'flyer (cl-call-next-method)))
  (defmethod classify ((obj bat))
    (cons 'bat (cl-call-next-method)))
  (let* ((buf (generate-new-buffer "c31"))
         (b (bat :species "little-brown" :echolocation t)))
    (with-current-buffer buf
      (insert "BAT:little-brown:mammal+flyer")
      (put-text-property 1 4 'type 'bat)
      (put-text-property 5 17 'field 'species)
      (put-text-property 18 30 'field 'traits)
      (setq-local batobj b)
      (let* ((ov (make-overlay 5 17))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let* ((cpl (eieio-class-precedence-list (eieio-object-class b)))
               (cpl-names (mapcar (lambda (c) (eieio-class-name c)) cpl))
               (classification (classify b)))
          (setf (ab-species b) "big-brown")
          (let ((new-class (classify b)))
            (goto-char 5)
            (insert (format "%s->%s" classification new-class))
            (setf (marker-position m) 15)
            (put-text-property 5 (+ 5 (length (format "%s->%s" classification new-class)))
                              'class-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (species (ab-species batobj))
              (echo (bat-echolocation batobj))
              (class (classify batobj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs species echo class
                (marker-position m)
                (buffer-string)
                batobj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_multi_inherit_accessor_conflict() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tagged ()
    ((tag :initarg :tag :accessor item-tag :initform nil)))
  (defclass numbered ()
    ((num :initarg :num :accessor item-num :initform 0)))
  (defclass named-obj ()
    ((name :initarg :name :accessor obj-name :initform "")))
  (defclass composite (tagged numbered named-obj)
    ((data :initarg :data :accessor comp-data :initform nil)))
  (let* ((buf (generate-new-buffer "ac1"))
         (c1 (composite :tag 'important :num 1 :name "first" :data '(a b c)))
         (c2 (composite :tag 'trivial :num 2 :name "second" :data '(d e f))))
    (with-current-buffer buf
      (insert "COMP:important:1:first-COMP:trivial:2:second")
      (put-text-property 1 5 'comp 1)
      (put-text-property 6 15 'field 'tag1)
      (put-text-property 16 17 'field 'num1)
      (put-text-property 18 23 'field 'name1)
      (put-text-property 24 28 'comp 2)
      (put-text-property 29 36 'field 'tag2)
      (put-text-property 37 38 'field 'num2)
      (put-text-property 39 45 'field 'name2)
      (setq-local items (list c1 c2))
      (let* ((ov1 (make-overlay 6 23))
             (ov2 (make-overlay 29 45))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let ((t1 (item-tag c1))
              (n1 (item-num c1))
              (nm1 (obj-name c1))
              (d1 (comp-data c1)))
          (setf (item-tag c1) 'critical
                (item-num c1) 100
                (obj-name c1) "updated")
          (let ((t1a (item-tag c1))
                (n1a (item-num c1))
                (nm1a (obj-name c1)))
            (goto-char 6)
            (insert (format "%s:%d:%s->%s:%d:%s" t1 n1 nm1 t1a n1a nm1a))
            (setf (marker-position m) 20)
            (put-text-property 6 (+ 6 (length (format "%s:%d:%s->%s:%d:%s"
                                                        t1 n1 nm1 t1a n1a nm1a)))
                              'mutated t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe1 (overlay-end ov1))
              (os2 (overlay-start ov2))
              (oe2 (overlay-end ov2))
              (bs (buffer-string))
              (c1-tag (item-tag (car items)))
              (c1-num (item-num (car items)))
              (c1-name (obj-name (car items)))
              (c1-data (comp-data (car items)))
              (c2-tag (item-tag (cadr items)))
              (c2-data (comp-data (cadr items))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe1 os2 oe2 bs
                c1-tag c1-num c1-name c1-data c2-tag c2-data
                (marker-position m)
                (buffer-string)
                items)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_hierarchy_buffer_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 22 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ast-node ()
    ((node-type :initarg :node-type :accessor node-type :initform 'leaf)))
  (defclass expr-node (ast-node)
    ((op :initarg :op :accessor node-op :initform nil)))
  (defclass binary-node (expr-node)
    ((left :initarg :left :accessor node-left :initform nil)
     (right :initarg :right :accessor node-right :initform nil)))
  (defclass comparison-node (binary-node)
    ((comparison :initarg :comparison :accessor node-comparison :initform nil)))
  (let* ((buf (generate-new-buffer "dh1"))
         (n (comparison-node
             :node-type 'comparison
             :op 'compare
             :left (binary-node :node-type 'expr :op '+
                                :left (expr-node :node-type 'expr :op 'x)
                                :right (expr-node :node-type 'expr :op 'y))
             :right (expr-node :node-type 'expr :op 'z)
             :comparison '(> . 0))))
    (with-current-buffer buf
      (insert "AST:comp(+,x,y),z:gt0")
      (put-text-property 1 4 'layer 'ast)
      (put-text-property 5 9 'layer 'comp)
      (put-text-property 10 18 'layer 'binary)
      (put-text-property 19 21 'layer 'right)
      (put-text-property 22 25 'layer 'pred)
      (setq-local root-node n)
      (let* ((ov (make-overlay 5 18))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 5 21)
        (undo-boundary)
        (let ((nt (node-type n))
              (n-op (node-op n))
              (n-comp (node-comparison n))
              (left-type (and (node-left n) (node-type (node-left n))))
              (right-op (and (node-right n) (node-op (node-right n)))))
          (setf (node-comparison n) '(>= . 0))
          (goto-char (point-min))
          (insert (format "[%s:%s->%s]" nt n-comp (node-comparison n)))
          (setf (marker-position m) (+ (point-min) 5))
          (put-text-property (point-min) (+ (point-min) 10) 'ast-change t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max)))
              (comp (node-comparison root-node)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe bs comp
                (marker-position m)
                (buffer-string)
                root-node)))
      (kill-buffer buf))))"#,
        expect,
    );
}
