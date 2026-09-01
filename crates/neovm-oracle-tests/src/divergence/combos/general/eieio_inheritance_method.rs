//! Divergence tests: EIEIO inheritance + method combo + buffer + marker.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_hierarchy_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function child-of-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ehd-base-xxx ()
    ((value :initarg :value :initform 0 :accessor test-ehd-get-val)))
  (defclass test-ehd-child-xxx (test-ehd-base-xxx)
    ((extra :initarg :extra :initform nil :accessor test-ehd-get-extra)))
  (defclass test-ehd-grandchild-xxx (test-ehd-child-xxx)
    ((deep :initarg :deep :initform 'root :accessor test-ehd-get-deep)))
  (cl-defgeneric test-ehd-describe-xxx (obj)
    "Describe object.")
  (cl-defmethod test-ehd-describe-xxx ((obj test-ehd-base-xxx))
    (list 'base (test-ehd-get-val obj)))
  (cl-defmethod test-ehd-describe-xxx ((obj test-ehd-child-xxx))
    (append (cl-call-next-method) (list 'child (test-ehd-get-extra obj))))
  (cl-defmethod test-ehd-describe-xxx ((obj test-ehd-grandchild-xxx))
    (append (cl-call-next-method) (list 'grandchild (test-ehd-get-deep obj))))
  (let ((b (test-ehd-base-xxx :value 10))
        (c (test-ehd-child-xxx :value 20 :extra 'yes))
        (g (test-ehd-grandchild-xxx :value 30 :extra 'no :deep 'leaf)))
    (list (test-ehd-describe-xxx b)
          (test-ehd-describe-xxx c)
          (test-ehd-describe-xxx g)
          (equal (test-ehd-describe-xxx b) '(base 10))
          (equal (test-ehd-describe-xxx c) '(base 20 child yes))
          (equal (test-ehd-describe-xxx g) '(base 30 grandchild leaf))
          (child-of-p g 'test-ehd-child-xxx)
          (child-of-p g 'test-ehd-base-xxx)
          (not (child-of-p b 'test-ehd-child-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_slots_accessors_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (beta t 8 t (d a b c) t t t t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-esam-xxx ()
    ((name :initarg :name :initform 'unnamed
           :accessor test-esam-name
           :type symbol)
     (count :initarg :count :initform 0
            :accessor test-esam-count
            :type integer)
     (tags :initarg :tags :initform nil
           :accessor test-esam-tags
           :type list)))
  (let ((obj (test-esam-xxx :name 'alpha :count 5 :tags '(a b c))))
    (setf (test-esam-name obj) 'beta)
    (setf (test-esam-count obj) (+ (test-esam-count obj) 3))
    (setf (test-esam-tags obj) (cons 'd (test-esam-tags obj)))
    (list (test-esam-name obj)
          (eq (test-esam-name obj) 'beta)
          (test-esam-count obj)
          (= (test-esam-count obj) 8)
          (test-esam-tags obj)
          (equal (test-esam-tags obj) '(d a b c))
          (slot-boundp obj 'name)
          (slot-boundp obj 'count)
          (slot-boundp obj 'tags)
          (slot-exists-p obj 'name)
          (not (slot-exists-p obj 'nonexistent))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_with_buffer_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 t #(\"Hello World\" 0 11 (owner #s(test-ebo-xxx #<killed buffer> #<marker (moves after insertion) in no buffer>))) t #s(test-ebo-xxx #<killed buffer> #<marker (moves after insertion) in no buffer>) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ebo-xxx ()
    ((buf :initarg :buf :initform nil :accessor test-ebo-buf)
     (marker :initarg :marker :initform nil :accessor test-ebo-marker)))
  (cl-defmethod test-ebo-insert-and-track-xxx ((obj test-ebo-xxx) text)
    (with-current-buffer (test-ebo-buf obj)
      (let ((start (point)))
        (insert text)
        (setf (test-ebo-marker obj) (copy-marker start t))
        (put-text-property start (point) 'owner obj))))
  (cl-defmethod test-ebo-verify-xxx ((obj test-ebo-xxx))
    (with-current-buffer (test-ebo-buf obj)
      (let ((m (test-ebo-marker obj)))
        (list (marker-position m)
              (= (marker-position m) 1)
              (buffer-string)
              (string= (buffer-string) "Hello World")
              (get-text-property 1 'owner)
              (eq (get-text-property 1 'owner) obj)))))
  (let ((buf (generate-new-buffer " test-ebo-xxx"))
        (obj (test-ebo-xxx)))
    (setf (test-ebo-buf obj) buf)
    (with-current-buffer buf
      (test-ebo-insert-and-track-xxx obj "Hello World"))
    (let ((result (test-ebo-verify-xxx obj)))
      (kill-buffer buf)
      result))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_method_before_after_around() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((around-enter before (primary 42) after around-exit) t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-emba-log-xxx nil)
  (defclass test-emba-xxx () ((val :initarg :val :initform 0)))
  (cl-defgeneric test-emba-run-xxx (obj)
    "Run with all qualifiers.")
  (cl-defmethod test-emba-run-xxx :before ((obj test-emba-xxx))
    (push 'before test-emba-log-xxx))
  (cl-defmethod test-emba-run-xxx ((obj test-emba-xxx))
    (push (list 'primary (slot-value obj 'val)) test-emba-log-xxx))
  (cl-defmethod test-emba-run-xxx :after ((obj test-emba-xxx))
    (push 'after test-emba-log-xxx))
  (cl-defmethod test-emba-run-xxx :around ((obj test-emba-xxx))
    (push 'around-enter test-emba-log-xxx)
    (cl-call-next-method)
    (push 'around-exit test-emba-log-xxx))
  (let ((obj (test-emba-xxx :val 42)))
    (test-emba-run-xxx obj)
    (let ((log (nreverse test-emba-log-xxx)))
      (list log
            (>= (length log) 5)
            (eq (car log) 'around-enter)
            (eq (nth 1 log) 'before)
            (equal (nth 2 log) '(primary 42))
            (eq (nth 3 log) 'after)
            (eq (nth 4 log) 'around-exit))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_make_instance_with_initargs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument eieio--class #s(built-in-class string nil (#s(built-in-class array \"Abstract supertype of arrays.\" (#s(built-in-class sequence \"Abstract supertype of sequences.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil) #s(built-in-class atom \"Abstract supertype of anything but cons cells.\" (#s(built-in-class t \"Abstract supertype of everything.\" nil nil nil nil)) nil nil nil)) nil nil nil)) nil nil nil) class)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-emi-xxx ()
    ((a :initarg :a :initform 1)
     (b :initarg :b :initform 2)
     (c :initarg :c :initform 3)
     (computed :initform nil)))
  (cl-defmethod initialize-instance :after ((obj test-emi-xxx) &rest args)
    (setf (slot-value obj 'computed)
          (+ (slot-value obj 'a)
             (slot-value obj 'b)
             (slot-value obj 'c))))
  (let ((obj1 (test-emi-xxx))
        (obj2 (test-emi-xxx :a 10 :b 20 :c 30))
        (obj3 (test-emi-xxx :a 100)))
    (list (slot-value obj1 'a) (slot-value obj1 'b) (slot-value obj1 'c)
          (= (slot-value obj1 'a) 1)
          (= (slot-value obj1 'computed) 6)
          (slot-value obj2 'a) (slot-value obj2 'b) (slot-value obj2 'c)
          (= (slot-value obj2 'computed) 60)
          (slot-value obj3 'a)
          (= (slot-value obj3 'a) 100)
          (= (slot-value obj3 'b) 2)
          (= (slot-value obj3 'c) 3)
          (= (slot-value obj3 'computed) 105)
          (object-of-class-p obj1 'test-emi-xxx)
          (not (object-of-class-p obj1 'string))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_polymorphic_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-epc-shape-xxx ()
    ((color :initarg :color :initform 'black)))
  (defclass test-epc-circle-xxx (test-epc-shape-xxx)
    ((radius :initarg :radius :initform 1)))
  (defclass test-epc-rect-xxx (test-epc-shape-xxx)
    ((width :initarg :width :initform 1)
     (height :initarg :height :initform 1)))
  (cl-defgeneric test-epc-area-xxx (shape)
    "Compute area.")
  (cl-defmethod test-epc-area-xxx ((s test-epc-shape-xxx)) 0)
  (cl-defmethod test-epc-area-xxx ((c test-epc-circle-xxx))
    (let ((r (slot-value c 'radius)))
      (* 3 r r)))
  (cl-defmethod test-epc-area-xxx ((r test-epc-rect-xxx))
    (* (slot-value r 'width) (slot-value r 'height)))
  (let ((shapes (list (test-epc-circle-xxx :radius 2 :color 'red)
                      (test-epc-rect-xxx :width 3 :height 4 :color 'blue)
                      (test-epc-shape-xxx :color 'green)
                      (test-epc-circle-xxx :radius 5 :color 'yellow))))
    (let ((areas (mapcar 'test-epc-area-xxx shapes))
          (colors (mapcar (lambda (s) (slot-value s 'color)) shapes)))
      (list areas colors
            (equal areas '(12 12 0 75))
            (equal colors '(red blue green yellow))
            (every (lambda (s) (child-of-p s 'test-epc-shape-xxx))
                   shapes))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_slot_type_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"test\" t nil t 3.14 t (1 2 3) t got-error \"test\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-estc-xxx ()
    ((name :initarg :name :type string :initform "default")
     (active :initarg :active :type boolean :initform t)
     (score :initarg :score :type number :initform 0)
     (data :initarg :data :type list :initform nil)))
  (let ((obj (test-estc-xxx :name "test" :active nil :score 3.14
                             :data '(1 2 3))))
    (list (slot-value obj 'name)
          (string= (slot-value obj 'name) "test")
          (slot-value obj 'active)
          (null (slot-value obj 'active))
          (slot-value obj 'score)
          (= (slot-value obj 'score) 3.14)
          (slot-value obj 'data)
          (equal (slot-value obj 'data) '(1 2 3))
          (condition-case err
              (progn (setf (slot-value obj 'name) 42) 'no-error)
            (error 'got-error))
          (slot-value obj 'name)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_with_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ewcc-xxx ()
    ((items :initarg :items :initform nil :accessor test-ewcc-items)))
  (let ((store (test-ewcc-xxx :items '(a b c))))
    (let ((add-fn (lambda (item)
                    (setf (test-ewcc-items store)
                          (append (test-ewcc-items store) (list item)))))
          (get-fn (lambda () (test-ewcc-items store))))
      (funcall add-fn 'd)
      (funcall add-fn 'e)
      (let ((result (funcall get-fn)))
        (list result
              (equal result '(a b c d e))
              (= (length result) 5)
              (test-ewcc-items store)
              (eq (test-ewcc-items store) result))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_shared_and_class_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t t t first t second t third t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-escs-xxx ()
    ((instance-count :allocation :class :initform 0
                     :accessor test-escs-count)
     (name :initarg :name :initform 'anon)))
  (cl-defmethod initialize-instance :after ((obj test-escs-xxx) &rest args)
    (setf (test-escs-count obj) (+ (test-escs-count obj) 1)))
  (let ((o1 (test-escs-xxx :name 'first))
        (o2 (test-escs-xxx :name 'second))
        (o3 (test-escs-xxx :name 'third)))
    (list (test-escs-count o1)
          (= (test-escs-count o1) 3)
          (= (test-escs-count o2) 3)
          (= (test-escs-count o3) 3)
          (slot-value o1 'name)
          (eq (slot-value o1 'name) 'first)
          (slot-value o2 'name)
          (eq (slot-value o2 'name) 'second)
          (slot-value o3 'name)
          (eq (slot-value o3 'name) 'third)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_composition_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ecw-xxx ()
    ((buffer :initarg :buffer :initform nil :accessor test-ecw-buffer)
     (overlay :initarg :overlay :initform nil :accessor test-ecw-overlay)
     (marker :initarg :marker :initform nil :accessor test-ecw-marker)))
  (cl-defmethod test-ecw-setup-xxx ((obj test-ecw-xxx) content)
    (let ((buf (generate-new-buffer " test-ecw-xxx")))
      (setf (test-ecw-buffer obj) buf)
      (with-current-buffer buf
        (insert content)
        (setf (test-ecw-overlay obj)
              (make-overlay 1 (+ 1 (length content))))
        (overlay-put (test-ecw-overlay obj) 'owner obj)
        (setf (test-ecw-marker obj) (copy-marker 1 t))
        (put-text-property 1 (+ 1 (length content)) 'managed obj))))
  (cl-defmethod test-ecw-modify-xxx ((obj test-ecw-xxx) pos text)
    (with-current-buffer (test-ecw-buffer obj)
      (undo-boundary)
      (goto-char pos)
      (insert text)))
  (cl-defmethod test-ecw-state-xxx ((obj test-ecw-xxx))
    (with-current-buffer (test-ecw-buffer obj)
      (list (buffer-string)
            (marker-position (test-ecw-marker obj))
            (overlay-start (test-ecw-overlay obj))
            (overlay-end (test-ecw-overlay obj))
            (overlay-get (test-ecw-overlay obj) 'owner)
            (eq (overlay-get (test-ecw-overlay obj) 'owner) obj)
            (get-text-property 1 'managed)
            (eq (get-text-property 1 'managed) obj))))
  (let ((widget (test-ecw-xxx)))
    (test-ecw-setup-xxx widget "BaseContent")
    (test-ecw-modify-xxx widget 5 "INSERTED")
    (let ((state1 (test-ecw-state-xxx widget)))
      (with-current-buffer (test-ecw-buffer widget)
        (primitive-undo 1 buffer-undo-list))
      (let ((state2 (test-ecw-state-xxx widget)))
        (kill-buffer (test-ecw-buffer widget))
        (list state1 state2
              (string= (car state2) "BaseContent")
              (eq (nth 4 state2) widget)
              (eq (nth 7 state2) widget)))))) "#,
        expect,
    );
}
