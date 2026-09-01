//! Oracle parity tests for EIEIO (Emacs Lisp object system).
//!
//! Covers: defclass with slots, :initarg/:initform/:type/:documentation,
//! make-instance, oref/oset, slot-value/set-slot-value, object-p,
//! cl-defmethod, method dispatch, :before/:after/:around methods,
//! inheritance, slot-boundp, initialize-instance.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic defclass with slots, :initarg, :initform, make-instance, oref/oset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_basic_class_and_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-point ()
    ((x :initarg :x :initform 0 :type number :documentation "X coordinate")
     (y :initarg :y :initform 0 :type number :documentation "Y coordinate")
     (label :initarg :label :initform "origin" :type string))
    "A 2D point with label.")

  (unwind-protect
      (let ((p (make-instance 'neovm--test-point :x 3 :y 4 :label "test")))
        (list
          ;; oref access
          (oref p x)
          (oref p y)
          (oref p label)
          ;; oset mutation
          (progn (oset p x 10) (oref p x))
          (progn (oset p label "moved") (oref p label))
          ;; object-p
          (object-p p)
          (object-p 42)
          (object-p nil)
          ;; Default initforms
          (let ((q (make-instance 'neovm--test-point)))
            (list (oref q x) (oref q y) (oref q label)))))
    (fmakunbound 'neovm--test-point)))"#;
    let expect =
        expect_test::expect![[r#""OK (3 4 \"test\" 10 \"moved\" t nil nil (0 0 \"origin\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// slot-value, set-slot-value, slot-boundp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_slot_value_and_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-container ()
    ((items :initarg :items :initform nil)
     (capacity :initarg :capacity)
     (metadata :initarg :metadata :initform nil)))

  (unwind-protect
      (let ((c (make-instance 'neovm--test-container :items '(1 2 3) :capacity 10)))
        (list
          ;; slot-value reads
          (slot-value c 'items)
          (slot-value c 'capacity)
          (slot-value c 'metadata)
          ;; slot-boundp
          (slot-boundp c 'items)
          (slot-boundp c 'capacity)
          (slot-boundp c 'metadata)
          ;; set-slot-value (using setf with slot-value)
          (progn (setf (slot-value c 'items) '(a b c d))
                 (slot-value c 'items))
          ;; Unbound slot: create instance without providing :capacity
          (let ((c2 (make-instance 'neovm--test-container :items '(x))))
            (list (slot-boundp c2 'items)
                  (slot-boundp c2 'capacity)
                  (slot-value c2 'items)))))
    (fmakunbound 'neovm--test-container)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) 10 nil t t t (a b c d) (t nil (x)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-defmethod: basic method dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_basic_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-shape ()
    ((name :initarg :name :initform "unknown")))

  (defclass neovm--test-circle (neovm--test-shape)
    ((radius :initarg :radius :initform 1)))

  (defclass neovm--test-rectangle (neovm--test-shape)
    ((width :initarg :width :initform 1)
     (height :initarg :height :initform 1)))

  (cl-defmethod neovm--test-area ((s neovm--test-circle))
    (let ((r (oref s radius)))
      ;; Use integer approximation: pi ~ 314/100
      (/ (* 314 r r) 100)))

  (cl-defmethod neovm--test-area ((s neovm--test-rectangle))
    (* (oref s width) (oref s height)))

  (cl-defmethod neovm--test-describe ((s neovm--test-shape))
    (format "Shape: %s" (oref s name)))

  (unwind-protect
      (let ((c (make-instance 'neovm--test-circle :name "C1" :radius 5))
            (r (make-instance 'neovm--test-rectangle :name "R1" :width 3 :height 7)))
        (list
          (neovm--test-area c)
          (neovm--test-area r)
          (neovm--test-describe c)
          (neovm--test-describe r)
          ;; Polymorphic dispatch through a list
          (mapcar #'neovm--test-area (list c r))))
    (fmakunbound 'neovm--test-area)
    (fmakunbound 'neovm--test-describe)
    (fmakunbound 'neovm--test-shape)
    (fmakunbound 'neovm--test-circle)
    (fmakunbound 'neovm--test-rectangle)))"#;
    let expect = expect_test::expect![[r#""OK (78 21 \"Shape: C1\" \"Shape: R1\" (78 21))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Inheritance: slot inheritance and method override
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_inheritance_and_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-animal ()
    ((species :initarg :species :initform "unknown")
     (legs :initarg :legs :initform 4)
     (sound :initarg :sound :initform "...")))

  (defclass neovm--test-dog (neovm--test-animal)
    ((breed :initarg :breed :initform "mutt")
     (sound :initform "woof")))

  (defclass neovm--test-bird (neovm--test-animal)
    ((wingspan :initarg :wingspan :initform 30)
     (legs :initform 2)
     (sound :initform "tweet")))

  (cl-defmethod neovm--test-speak ((a neovm--test-animal))
    (format "%s says %s" (oref a species) (oref a sound)))

  (cl-defmethod neovm--test-speak ((d neovm--test-dog))
    (format "%s (%s) says %s!" (oref d species) (oref d breed) (oref d sound)))

  (unwind-protect
      (let ((dog (make-instance 'neovm--test-dog :species "dog" :breed "labrador"))
            (bird (make-instance 'neovm--test-bird :species "parrot" :wingspan 40))
            (cat (make-instance 'neovm--test-animal :species "cat" :sound "meow")))
        (list
          ;; Inherited + overridden slots
          (oref dog legs)
          (oref dog sound)
          (oref dog breed)
          (oref bird legs)
          (oref bird wingspan)
          (oref bird sound)
          (oref cat legs)
          (oref cat sound)
          ;; Method dispatch (dog overrides, bird/cat use base)
          (neovm--test-speak dog)
          (neovm--test-speak bird)
          (neovm--test-speak cat)
          ;; Type checking
          (object-of-class-p dog 'neovm--test-dog)
          (object-of-class-p dog 'neovm--test-animal)
          (object-of-class-p bird 'neovm--test-dog)))
    (fmakunbound 'neovm--test-speak)
    (fmakunbound 'neovm--test-animal)
    (fmakunbound 'neovm--test-dog)
    (fmakunbound 'neovm--test-bird)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 \"woof\" \"labrador\" 2 40 \"tweet\" 4 \"meow\" \"dog (labrador) says woof!\" \"parrot says tweet\" \"cat says meow\" t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :before, :after, and :around methods
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_method_qualifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defvar neovm--test-method-log nil)

  (defclass neovm--test-processor ()
    ((name :initarg :name :initform "proc")))

  (cl-defmethod neovm--test-process ((p neovm--test-processor) data)
    (push (format "primary(%s): %s" (oref p name) data) neovm--test-method-log)
    (format "processed:%s" data))

  (cl-defmethod neovm--test-process :before ((p neovm--test-processor) data)
    (push (format "before(%s): %s" (oref p name) data) neovm--test-method-log))

  (cl-defmethod neovm--test-process :after ((p neovm--test-processor) data)
    (push (format "after(%s): %s" (oref p name) data) neovm--test-method-log))

  (unwind-protect
      (progn
        (setq neovm--test-method-log nil)
        (let ((proc (make-instance 'neovm--test-processor :name "alpha")))
          (let ((result (neovm--test-process proc "hello")))
            (list result (nreverse neovm--test-method-log)))))
    (fmakunbound 'neovm--test-process)
    (fmakunbound 'neovm--test-processor)
    (makunbound 'neovm--test-method-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"processed:hello\" (\"before(alpha): hello\" \"primary(alpha): hello\" \"after(alpha): hello\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :around method with cl-call-next-method
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_around_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defvar neovm--test-around-log nil)

  (defclass neovm--test-cache ()
    ((store :initform nil)
     (hits :initform 0)
     (misses :initform 0)))

  (cl-defmethod neovm--test-fetch ((c neovm--test-cache) key)
    (oset c misses (1+ (oref c misses)))
    (push (format "miss:%s" key) neovm--test-around-log)
    (format "computed:%s" key))

  (cl-defmethod neovm--test-fetch :around ((c neovm--test-cache) key)
    (let ((cached (assoc key (oref c store))))
      (if cached
          (progn
            (oset c hits (1+ (oref c hits)))
            (push (format "hit:%s" key) neovm--test-around-log)
            (cdr cached))
        ;; Cache miss: call primary, cache result
        (let ((result (cl-call-next-method)))
          (oset c store (cons (cons key result) (oref c store)))
          result))))

  (unwind-protect
      (progn
        (setq neovm--test-around-log nil)
        (let ((cache (make-instance 'neovm--test-cache)))
          (let ((r1 (neovm--test-fetch cache "x"))
                (r2 (neovm--test-fetch cache "y"))
                (r3 (neovm--test-fetch cache "x"))
                (r4 (neovm--test-fetch cache "x")))
            (list r1 r2 r3 r4
                  (oref cache hits)
                  (oref cache misses)
                  (nreverse neovm--test-around-log)))))
    (fmakunbound 'neovm--test-fetch)
    (fmakunbound 'neovm--test-cache)
    (makunbound 'neovm--test-around-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"computed:x\" \"computed:y\" \"computed:x\" \"computed:x\" 2 2 (\"miss:x\" \"miss:y\" \"hit:x\" \"hit:x\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// initialize-instance customization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_initialize_instance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-validated-range ()
    ((low :initarg :low :initform 0)
     (high :initarg :high :initform 100)
     (span :initform nil)
     (mid :initform nil)))

  (cl-defmethod initialize-instance :after ((r neovm--test-validated-range) &rest _slots)
    ;; Swap if low > high, compute derived slots
    (when (> (oref r low) (oref r high))
      (let ((tmp (oref r low)))
        (oset r low (oref r high))
        (oset r high tmp)))
    (oset r span (- (oref r high) (oref r low)))
    (oset r mid (/ (+ (oref r low) (oref r high)) 2)))

  (unwind-protect
      (let ((r1 (make-instance 'neovm--test-validated-range :low 10 :high 50))
            (r2 (make-instance 'neovm--test-validated-range :low 80 :high 20)))
        (list
          ;; r1: normal order
          (oref r1 low) (oref r1 high) (oref r1 span) (oref r1 mid)
          ;; r2: swapped during init
          (oref r2 low) (oref r2 high) (oref r2 span) (oref r2 mid)))
    (fmakunbound 'neovm--test-validated-range)))"#;
    let expect = expect_test::expect![[r#""OK (10 50 40 30 20 80 60 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple inheritance (mixin pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_multiple_inheritance_mixin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-named ()
    ((name :initarg :name :initform "anon")))

  (defclass neovm--test-timestamped ()
    ((created :initarg :created :initform 0)
     (modified :initarg :modified :initform 0)))

  (defclass neovm--test-taggable ()
    ((tags :initarg :tags :initform nil)))

  ;; Multiple inheritance: combine all three
  (defclass neovm--test-document (neovm--test-named
                                   neovm--test-timestamped
                                   neovm--test-taggable)
    ((content :initarg :content :initform "")))

  (cl-defmethod neovm--test-add-tag ((d neovm--test-document) tag)
    (unless (member tag (oref d tags))
      (oset d tags (cons tag (oref d tags)))))

  (cl-defmethod neovm--test-summary ((d neovm--test-document))
    (format "%s (tags: %d, len: %d)"
            (oref d name)
            (length (oref d tags))
            (length (oref d content))))

  (unwind-protect
      (let ((doc (make-instance 'neovm--test-document
                   :name "readme"
                   :created 1000
                   :content "hello world"
                   :tags '(important))))
        (neovm--test-add-tag doc 'draft)
        (neovm--test-add-tag doc 'important)  ;; duplicate, no-op
        (neovm--test-add-tag doc 'v2)
        (oset doc modified 2000)
        (list
          (oref doc name)
          (oref doc created)
          (oref doc modified)
          (sort (mapcar #'symbol-name (oref doc tags)) #'string<)
          (neovm--test-summary doc)
          (oref doc content)
          ;; Type checks
          (object-of-class-p doc 'neovm--test-document)
          (object-of-class-p doc 'neovm--test-named)
          (object-of-class-p doc 'neovm--test-timestamped)
          (object-of-class-p doc 'neovm--test-taggable)))
    (fmakunbound 'neovm--test-add-tag)
    (fmakunbound 'neovm--test-summary)
    (fmakunbound 'neovm--test-document)
    (fmakunbound 'neovm--test-named)
    (fmakunbound 'neovm--test-timestamped)
    (fmakunbound 'neovm--test-taggable)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"readme\" 1000 2000 (\"draft\" \"important\" \"v2\") \"readme (tags: 3, len: 11)\" \"hello world\" t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deep class hierarchy with method resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_deep_hierarchy_method_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-base ()
    ((val :initarg :val :initform 0)))

  (defclass neovm--test-mid (neovm--test-base)
    ((factor :initarg :factor :initform 2)))

  (defclass neovm--test-leaf (neovm--test-mid)
    ((offset :initarg :offset :initform 10)))

  (cl-defmethod neovm--test-compute ((b neovm--test-base))
    (oref b val))

  (cl-defmethod neovm--test-compute ((m neovm--test-mid))
    (* (cl-call-next-method) (oref m factor)))

  (cl-defmethod neovm--test-compute ((l neovm--test-leaf))
    (+ (cl-call-next-method) (oref l offset)))

  (unwind-protect
      (let ((b (make-instance 'neovm--test-base :val 5))
            (m (make-instance 'neovm--test-mid :val 5 :factor 3))
            (l (make-instance 'neovm--test-leaf :val 5 :factor 3 :offset 100)))
        (list
          ;; base: just val => 5
          (neovm--test-compute b)
          ;; mid: val * factor => 5 * 3 = 15
          (neovm--test-compute m)
          ;; leaf: (val * factor) + offset => (5*3) + 100 = 115
          (neovm--test-compute l)
          ;; class hierarchy checks
          (object-of-class-p l 'neovm--test-leaf)
          (object-of-class-p l 'neovm--test-mid)
          (object-of-class-p l 'neovm--test-base)))
    (fmakunbound 'neovm--test-compute)
    (fmakunbound 'neovm--test-base)
    (fmakunbound 'neovm--test-mid)
    (fmakunbound 'neovm--test-leaf)))"#;
    let expect = expect_test::expect![[r#""OK (5 15 115 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Slot access on instances: dynamic slot iteration via class info
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eieio_class_info_and_slot_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'eieio)
  (defclass neovm--test-record ()
    ((id :initarg :id :initform 0)
     (name :initarg :name :initform "")
     (active :initarg :active :initform t)))

  (unwind-protect
      (let* ((r (make-instance 'neovm--test-record :id 42 :name "test" :active nil))
             (slots (eieio-class-slots (eieio-object-class r)))
             (slot-names (mapcar #'cl--slot-descriptor-name slots)))
        (list
          ;; Slot names from class introspection
          slot-names
          ;; Read each slot dynamically
          (mapcar (lambda (sn) (slot-value r sn)) slot-names)
          ;; Object class name
          (eieio-object-class-name r)
          ;; same-class check
          (let ((r2 (make-instance 'neovm--test-record :id 99)))
            (same-class-p r r2))))
    (fmakunbound 'neovm--test-record)))"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument eieio--class #s(neovm--test-record 99 \"\" t) class)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
