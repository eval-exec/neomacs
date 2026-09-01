//! Oracle parity tests for `cl-defstruct` (require 'cl-lib): defining
//! structs, constructors, copiers, predicates, slot access, `setf` on
//! slots, `:type list`/`:type vector`, `:named` option, `:initial-offset`,
//! `:include` (inheritance), `:conc-name`, `:constructor` with custom
//! args, `cl-struct-slot-info`, `cl-struct-slot-value`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic cl-defstruct: constructor, predicate, slot access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_basic_constructor_and_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (cl-defstruct neovm--test-point x y z)
  (unwind-protect
      (let ((p (make-neovm--test-point :x 10 :y 20 :z 30)))
        (list
          ;; Predicate
          (neovm--test-point-p p)
          (neovm--test-point-p 42)
          (neovm--test-point-p nil)
          ;; Slot accessors
          (neovm--test-point-x p)
          (neovm--test-point-y p)
          (neovm--test-point-z p)
          ;; Constructor with partial args (rest default to nil)
          (let ((q (make-neovm--test-point :x 5)))
            (list (neovm--test-point-x q)
                  (neovm--test-point-y q)
                  (neovm--test-point-z q)))
          ;; Copy
          (let* ((p2 (copy-neovm--test-point p)))
            (list (neovm--test-point-x p2)
                  (neovm--test-point-y p2)
                  (equal p p2)
                  (eq p p2)))))
    (fmakunbound 'make-neovm--test-point)
    (fmakunbound 'copy-neovm--test-point)
    (fmakunbound 'neovm--test-point-p)
    (fmakunbound 'neovm--test-point-x)
    (fmakunbound 'neovm--test-point-y)
    (fmakunbound 'neovm--test-point-z)))"#;
    let expect = expect_test::expect![r#""OK (t nil nil 10 20 30 (5 nil nil) (10 20 t nil))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setf on struct slots
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_setf_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (cl-defstruct neovm--test-counter name value step)
  (unwind-protect
      (let ((c (make-neovm--test-counter :name "hits" :value 0 :step 1)))
        ;; Mutate with setf
        (setf (neovm--test-counter-value c) 42)
        (let ((v1 (neovm--test-counter-value c)))
          ;; Increment via setf
          (setf (neovm--test-counter-value c)
                (+ (neovm--test-counter-value c)
                   (neovm--test-counter-step c)))
          (let ((v2 (neovm--test-counter-value c)))
            ;; Change name
            (setf (neovm--test-counter-name c) "misses")
            ;; Change step
            (setf (neovm--test-counter-step c) 10)
            (list v1 v2
                  (neovm--test-counter-name c)
                  (neovm--test-counter-step c)
                  (neovm--test-counter-value c)))))
    (fmakunbound 'make-neovm--test-counter)
    (fmakunbound 'copy-neovm--test-counter)
    (fmakunbound 'neovm--test-counter-p)
    (fmakunbound 'neovm--test-counter-name)
    (fmakunbound 'neovm--test-counter-value)
    (fmakunbound 'neovm--test-counter-step)))"#;
    let expect = expect_test::expect![[r#""OK (42 43 \"misses\" 10 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :type list struct
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_type_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (cl-defstruct (neovm--test-pair (:type list)) first second)
  (unwind-protect
      (let ((p (make-neovm--test-pair :first 'a :second 'b)))
        (list
          ;; It's a plain list
          (listp p)
          p
          ;; Access via accessors
          (neovm--test-pair-first p)
          (neovm--test-pair-second p)
          ;; Also accessible via car/cadr since it's a list
          (car p)
          (cadr p)
          ;; Mutation
          (setf (neovm--test-pair-first p) 'x)
          (neovm--test-pair-first p)
          p
          ;; Length
          (length p)))
    (fmakunbound 'make-neovm--test-pair)
    (fmakunbound 'neovm--test-pair-first)
    (fmakunbound 'neovm--test-pair-second)))"#;
    let expect = expect_test::expect![r#""OK (t (x b) a b a b x x (x b) 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :type vector struct
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_type_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (cl-defstruct (neovm--test-rgb (:type vector)) red green blue)
  (unwind-protect
      (let ((c (make-neovm--test-rgb :red 255 :green 128 :blue 0)))
        (list
          ;; It's a plain vector
          (vectorp c)
          c
          ;; Accessor
          (neovm--test-rgb-red c)
          (neovm--test-rgb-green c)
          (neovm--test-rgb-blue c)
          ;; Also accessible via aref since it's a vector
          (aref c 0)
          (aref c 1)
          (aref c 2)
          ;; Mutation
          (setf (neovm--test-rgb-blue c) 64)
          (neovm--test-rgb-blue c)
          ;; Length
          (length c)))
    (fmakunbound 'make-neovm--test-rgb)
    (fmakunbound 'neovm--test-rgb-red)
    (fmakunbound 'neovm--test-rgb-green)
    (fmakunbound 'neovm--test-rgb-blue)))"#;
    let expect = expect_test::expect![r#""OK (t [255 128 64] 255 128 0 255 128 0 64 64 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :named with :type list and :type vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_named_option() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; :named with :type list includes the type tag as first element
  (cl-defstruct (neovm--test-tagged-pair (:type list) :named) left right)
  ;; :named with :type vector includes the type tag at index 0
  (cl-defstruct (neovm--test-tagged-vec (:type vector) :named) alpha beta)
  (unwind-protect
      (let ((lp (make-neovm--test-tagged-pair :left 1 :right 2))
            (vp (make-neovm--test-tagged-vec :alpha 10 :beta 20)))
        (list
          ;; List form includes type name as first element
          lp
          (car lp)  ;; should be 'neovm--test-tagged-pair
          (neovm--test-tagged-pair-left lp)
          (neovm--test-tagged-pair-right lp)
          ;; Predicate works with :named
          (neovm--test-tagged-pair-p lp)
          (neovm--test-tagged-pair-p '(wrong 1 2))
          ;; Vector form includes type name at index 0
          vp
          (aref vp 0)  ;; should be 'neovm--test-tagged-vec
          (neovm--test-tagged-vec-alpha vp)
          (neovm--test-tagged-vec-beta vp)
          ;; Predicate
          (neovm--test-tagged-vec-p vp)
          (neovm--test-tagged-vec-p [wrong 10 20])))
    (fmakunbound 'make-neovm--test-tagged-pair)
    (fmakunbound 'neovm--test-tagged-pair-p)
    (fmakunbound 'neovm--test-tagged-pair-left)
    (fmakunbound 'neovm--test-tagged-pair-right)
    (fmakunbound 'make-neovm--test-tagged-vec)
    (fmakunbound 'neovm--test-tagged-vec-p)
    (fmakunbound 'neovm--test-tagged-vec-alpha)
    (fmakunbound 'neovm--test-tagged-vec-beta)))"#;
    let expect = expect_test::expect![
        r#""OK ((neovm--test-tagged-pair 1 2) neovm--test-tagged-pair 1 2 t nil [neovm--test-tagged-vec 10 20] neovm--test-tagged-vec 10 20 t nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :initial-offset with :type list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_initial_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; :initial-offset 2 leaves first 2 slots as nil padding
  (cl-defstruct (neovm--test-offset-rec (:type list) (:initial-offset 2))
    name value)
  (unwind-protect
      (let ((r (make-neovm--test-offset-rec :name "foo" :value 42)))
        (list
          ;; The raw list has nil padding at the start
          r
          (length r)
          ;; Accessors skip the padding
          (neovm--test-offset-rec-name r)
          (neovm--test-offset-rec-value r)
          ;; First two elements are nil
          (nth 0 r)
          (nth 1 r)
          ;; Data starts at index 2
          (nth 2 r)
          (nth 3 r)))
    (fmakunbound 'make-neovm--test-offset-rec)
    (fmakunbound 'neovm--test-offset-rec-name)
    (fmakunbound 'neovm--test-offset-rec-value)))"#;
    let expect =
        expect_test::expect![[r#""OK ((nil nil \"foo\" 42) 4 \"foo\" 42 nil nil \"foo\" 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :conc-name customization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_conc_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; Custom prefix for accessors
  (cl-defstruct (neovm--test-item (:conc-name neovm--test-item/))
    id label priority)
  (unwind-protect
      (let ((item (make-neovm--test-item :id 1 :label "urgent" :priority 5)))
        (list
          ;; Accessors use custom conc-name
          (neovm--test-item/id item)
          (neovm--test-item/label item)
          (neovm--test-item/priority item)
          ;; Predicate still uses the struct name
          (neovm--test-item-p item)
          ;; Mutation with custom accessor
          (setf (neovm--test-item/priority item) 10)
          (neovm--test-item/priority item)))
    (fmakunbound 'make-neovm--test-item)
    (fmakunbound 'copy-neovm--test-item)
    (fmakunbound 'neovm--test-item-p)
    (fmakunbound 'neovm--test-item/id)
    (fmakunbound 'neovm--test-item/label)
    (fmakunbound 'neovm--test-item/priority)))"#;
    let expect = expect_test::expect![[r#""OK (1 \"urgent\" 5 t 10 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :constructor with custom argument list (boa constructor)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_boa_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; BOA (By Order of Arguments) constructor
  (cl-defstruct (neovm--test-range
                 (:constructor neovm--test-make-range (start end &optional (step 1))))
    start end step)
  (unwind-protect
      (list
        ;; Positional constructor
        (let ((r (neovm--test-make-range 0 10)))
          (list (neovm--test-range-start r)
                (neovm--test-range-end r)
                (neovm--test-range-step r)))
        ;; With optional step
        (let ((r (neovm--test-make-range 0 100 5)))
          (list (neovm--test-range-start r)
                (neovm--test-range-end r)
                (neovm--test-range-step r)))
        ;; Iterate over range
        (let* ((r (neovm--test-make-range 0 10 2))
               (result nil)
               (i (neovm--test-range-start r)))
          (while (< i (neovm--test-range-end r))
            (setq result (cons i result))
            (setq i (+ i (neovm--test-range-step r))))
          (nreverse result)))
    (fmakunbound 'neovm--test-make-range)
    (fmakunbound 'copy-neovm--test-range)
    (fmakunbound 'neovm--test-range-p)
    (fmakunbound 'neovm--test-range-start)
    (fmakunbound 'neovm--test-range-end)
    (fmakunbound 'neovm--test-range-step)))"#;
    let expect = expect_test::expect![r#""OK ((0 10 1) (0 100 5) (0 2 4 6 8))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :include (struct inheritance)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_include_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; Base struct
  (cl-defstruct neovm--test-shape kind area)
  ;; Derived struct inheriting from shape
  (cl-defstruct (neovm--test-circle (:include neovm--test-shape))
    radius)
  ;; Another derived struct
  (cl-defstruct (neovm--test-rect (:include neovm--test-shape))
    width height)
  (unwind-protect
      (let ((circ (make-neovm--test-circle :kind "circle" :area 314 :radius 10))
            (rect (make-neovm--test-rect :kind "rect" :area 200 :width 10 :height 20)))
        (list
          ;; Inherited slots accessible
          (neovm--test-shape-kind circ)
          (neovm--test-shape-area circ)
          (neovm--test-circle-radius circ)
          (neovm--test-shape-kind rect)
          (neovm--test-shape-area rect)
          (neovm--test-rect-width rect)
          (neovm--test-rect-height rect)
          ;; Predicate: child is-a parent
          (neovm--test-shape-p circ)
          (neovm--test-shape-p rect)
          (neovm--test-circle-p circ)
          (neovm--test-rect-p rect)
          ;; But circle is not a rect
          (neovm--test-rect-p circ)
          (neovm--test-circle-p rect)
          ;; setf on inherited slot
          (setf (neovm--test-shape-area circ) 628)
          (neovm--test-shape-area circ)))
    (fmakunbound 'make-neovm--test-shape)
    (fmakunbound 'copy-neovm--test-shape)
    (fmakunbound 'neovm--test-shape-p)
    (fmakunbound 'neovm--test-shape-kind)
    (fmakunbound 'neovm--test-shape-area)
    (fmakunbound 'make-neovm--test-circle)
    (fmakunbound 'copy-neovm--test-circle)
    (fmakunbound 'neovm--test-circle-p)
    (fmakunbound 'neovm--test-circle-radius)
    (fmakunbound 'make-neovm--test-rect)
    (fmakunbound 'copy-neovm--test-rect)
    (fmakunbound 'neovm--test-rect-p)
    (fmakunbound 'neovm--test-rect-width)
    (fmakunbound 'neovm--test-rect-height)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"circle\" 314 10 \"rect\" 200 10 20 t t t t nil nil 628 628)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Slot defaults and complex initial values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_slot_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (cl-defstruct neovm--test-config
    (host "localhost")
    (port 8080)
    (debug nil)
    (tags '(default))
    (retries 3))
  (unwind-protect
      (list
        ;; All defaults
        (let ((c (make-neovm--test-config)))
          (list (neovm--test-config-host c)
                (neovm--test-config-port c)
                (neovm--test-config-debug c)
                (neovm--test-config-tags c)
                (neovm--test-config-retries c)))
        ;; Override some
        (let ((c (make-neovm--test-config :port 443 :debug t)))
          (list (neovm--test-config-host c)
                (neovm--test-config-port c)
                (neovm--test-config-debug c)
                (neovm--test-config-retries c)))
        ;; Override all
        (let ((c (make-neovm--test-config
                  :host "example.com" :port 9090
                  :debug t :tags '(prod live) :retries 0)))
          (list (neovm--test-config-host c)
                (neovm--test-config-port c)
                (neovm--test-config-debug c)
                (neovm--test-config-tags c)
                (neovm--test-config-retries c))))
    (fmakunbound 'make-neovm--test-config)
    (fmakunbound 'copy-neovm--test-config)
    (fmakunbound 'neovm--test-config-p)
    (fmakunbound 'neovm--test-config-host)
    (fmakunbound 'neovm--test-config-port)
    (fmakunbound 'neovm--test-config-debug)
    (fmakunbound 'neovm--test-config-tags)
    (fmakunbound 'neovm--test-config-retries)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"localhost\" 8080 nil (default) 3) (\"localhost\" 443 t 3) (\"example.com\" 9090 t (prod live) 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-struct-slot-info and cl-struct-slot-value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_slot_info_and_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (cl-defstruct neovm--test-record
    (id 0)
    (label "")
    (active t))
  (unwind-protect
      (let ((r (make-neovm--test-record :id 7 :label "test" :active nil)))
        (list
          ;; cl-struct-slot-info returns slot descriptors
          (let ((info (cl-struct-slot-info 'neovm--test-record)))
            ;; Each element is (name default-value . options)
            (length info))
          ;; cl-struct-slot-value accesses by slot name
          (cl-struct-slot-value 'neovm--test-record 'id r)
          (cl-struct-slot-value 'neovm--test-record 'label r)
          (cl-struct-slot-value 'neovm--test-record 'active r)
          ;; Iterate over all slots dynamically
          (let ((result nil))
            (dolist (slot-desc (cl-struct-slot-info 'neovm--test-record))
              (let ((slot-name (car slot-desc)))
                (setq result
                      (cons (cons slot-name
                                  (cl-struct-slot-value 'neovm--test-record
                                                        slot-name r))
                            result))))
            (nreverse result))))
    (fmakunbound 'make-neovm--test-record)
    (fmakunbound 'copy-neovm--test-record)
    (fmakunbound 'neovm--test-record-p)
    (fmakunbound 'neovm--test-record-id)
    (fmakunbound 'neovm--test-record-label)
    (fmakunbound 'neovm--test-record-active)))"#;
    let expect =
        expect_test::expect![r#""ERR (cl-struct-unknown-slot neovm--test-record cl-tag-slot)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Structs as data structures: linked list node, binary tree node
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_defstruct_as_data_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  ;; Binary tree node
  (cl-defstruct neovm--test-bst-node
    key
    (left nil)
    (right nil))

  (fset 'neovm--test-bst-insert
    (lambda (tree key)
      (if (null tree)
          (make-neovm--test-bst-node :key key)
        (cond
          ((< key (neovm--test-bst-node-key tree))
           (setf (neovm--test-bst-node-left tree)
                 (funcall 'neovm--test-bst-insert
                          (neovm--test-bst-node-left tree) key))
           tree)
          ((> key (neovm--test-bst-node-key tree))
           (setf (neovm--test-bst-node-right tree)
                 (funcall 'neovm--test-bst-insert
                          (neovm--test-bst-node-right tree) key))
           tree)
          (t tree)))))  ;; duplicate: ignore

  (fset 'neovm--test-bst-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--test-bst-inorder
                         (neovm--test-bst-node-left tree))
                (list (neovm--test-bst-node-key tree))
                (funcall 'neovm--test-bst-inorder
                         (neovm--test-bst-node-right tree))))))

  (fset 'neovm--test-bst-search
    (lambda (tree key)
      (if (null tree) nil
        (cond
          ((= key (neovm--test-bst-node-key tree)) t)
          ((< key (neovm--test-bst-node-key tree))
           (funcall 'neovm--test-bst-search
                    (neovm--test-bst-node-left tree) key))
          (t (funcall 'neovm--test-bst-search
                      (neovm--test-bst-node-right tree) key))))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert several values
        (dolist (k '(5 3 7 1 4 6 8 2))
          (setq tree (funcall 'neovm--test-bst-insert tree k)))
        (list
          ;; In-order traversal should be sorted
          (funcall 'neovm--test-bst-inorder tree)
          ;; Search
          (funcall 'neovm--test-bst-search tree 4)
          (funcall 'neovm--test-bst-search tree 9)
          (funcall 'neovm--test-bst-search tree 1)
          ;; Root key
          (neovm--test-bst-node-key tree)
          ;; Predicate
          (neovm--test-bst-node-p tree)
          (neovm--test-bst-node-p nil)))
    (fmakunbound 'make-neovm--test-bst-node)
    (fmakunbound 'copy-neovm--test-bst-node)
    (fmakunbound 'neovm--test-bst-node-p)
    (fmakunbound 'neovm--test-bst-node-key)
    (fmakunbound 'neovm--test-bst-node-left)
    (fmakunbound 'neovm--test-bst-node-right)
    (fmakunbound 'neovm--test-bst-insert)
    (fmakunbound 'neovm--test-bst-inorder)
    (fmakunbound 'neovm--test-bst-search)))"#;
    let expect = expect_test::expect![r#""OK ((1 2 3 4 5 6 7 8) t nil t 5 t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
