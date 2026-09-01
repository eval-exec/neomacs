//! Complex combo batch 62 — cl-lib metaprogramming depth: cl-defstruct with
//! included slots, eieio method qualifiers with multiple inheritance, advice-augment
//! forms (`add-function` `:before-until`/`:after-while`), format-spec edge cases,
//! and `rx` macro evaluation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx62_cl_defstruct_included_and_print_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx62-base (:constructor neo-cx62-make-base)
                             (:copier neo-cx62-copy-base)
                             (:conc-name neo-cx62-b-))
  tag comment)
(cl-defstruct (neo-cx62-child (:include neo-cx62-base)
                              (:conc-name neo-cx62-c-))
  value extra)
(let* ((base (neo-cx62-make-base :tag :a :comment "hello"))
       (child (make-neo-cx62-child :tag :b :comment "kid" :value 42 :extra :bonus)))
  (list (neo-cx62-b-tag base)
        (neo-cx62-b-comment base)
        (neo-cx62-b-tag child)
        (neo-cx62-b-comment child)
        (neo-cx62-c-value child)
        (neo-cx62-c-extra child)
        (copy-neo-cx62-base base)
        (let ((print-circle nil)) (prin1-to-string child))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_cl_defstruct_with_named_conc_str_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-defstruct (neo-cx62-rec (:type list) :named) a b c)
(let ((r (make-neo-cx62-rec :a 1 :b 2 :c 3)))
  (list (neo-cx62-rec-a r)
        (neo-cx62-rec-b r)
        (neo-cx62-rec-c r)
        (neo-cx62-rec-p r)
        (copy-neo-cx62-rec r)
        (setf (neo-cx62-rec-a r) 99)
        (neo-cx62-rec-a r)
        r))
"##,
        expect,
    );
}

#[test]
fn div_cx62_eieio_multiple_inheritance_method_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx62-a () ((x :initarg :x :initform 1)))
      (defclass neo-cx62-b () ((y :initarg :y :initform 2)))
      (defclass neo-cx62-c (neo-cx62-a neo-cx62-b) ()
        (:method-combination +))
      (cl-defmethod neo-cx62-who ((o neo-cx62-a)) :a)
      (cl-defmethod neo-cx62-who ((o neo-cx62-b)) :b)
      (let ((inst (neo-cx62-c :x 11 :y 22)))
        (list (slot-value inst 'x)
              (slot-value inst 'y)
              (neo-cx62-who inst)
              (class-of inst)
              (neo-cx62--c))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_eieio_method_qualifiers_before_after_around() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:around-enter :before :primary :after :around-exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx62-q () () )
      (let (calls)
        (cl-defmethod neo-cx62-call :before ((o neo-cx62-q)) (push :before calls))
        (cl-defmethod neo-cx62-call ((o neo-cx62-q)) (push :primary calls))
        (cl-defmethod neo-cx62-call :after ((o neo-cx62-q)) (push :after calls))
        (cl-defmethod neo-cx62-call :around ((o neo-cx62-q))
          (push :around-enter calls)
          (cl-call-next-method)
          (push :around-exit calls))
        (let ((inst (make-instance 'neo-cx62-q)))
          (neo-cx62-call inst)
          (nreverse calls))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_add_function_before_until_after_while_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable neo-cx62-fn)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (symbol-function (defalias 'neo-cx62-fn (lambda (x) (list :primary x))))))
  (let ((calls nil))
    (add-function :before (var 'neo-cx62-fn)
                  (lambda (x) (push (list :before x) calls)))
    (add-function :after (var 'neo-cx62-fn)
                  (lambda (r x) (push (list :after r x) calls)))
    (let ((result (neo-cx62-fn 42)))
      (list result (nreverse calls)))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_format_spec_edge_percent_and_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (format-spec-make ?a "alpha" ?b "beta")))
      (list (format-spec "%a-%b" spec)
            (format-spec "%%literal" spec)
            (format-spec "%a-%%-end" spec)
            (condition-case e2 (format-spec "%z-missing" spec) (error (car e2)))
            (format-spec "%a %a %a" spec)
            (condition-case e3 (format-spec "%a-%b-%c" spec) (error (car e3)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_rx_macro_evaluation_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable kw)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((kw 'foo)
       (pat (rx-to-string
             `(seq bos
                   (group (+ (any "A-Za-z")))
                   ":"
                   (or (eval kw) "bar")
                   (* digit)
                   eos))))
  (list pat
        (string-match pat "ABC:foo123")
        (string-match pat "ABC:bar")
        (string-match pat "ABC:qux")
        (match-string 0)
        (match-string 1)
        (regexp-opt '("a" "ab" "abc"))
        (regexp-quote "a.b*c?")))
"##,
        expect,
    );
}

#[test]
fn div_cx62_cl_loop_for_hash_destructure_sum_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 10 ht)
  (puthash "beta" 20 ht)
  (puthash "gamma" 30 ht)
  (list (cl-loop for k being the hash-keys of ht collect k)
        (cl-loop for v being the hash-values of ht sum v)
        (cl-loop for k being the hash-keys of ht using (hash-values v)
                 count (> v 15))
        (cl-loop for k being the hash-keys of ht
                 if (> (gethash k ht) 15) collect (cons k (gethash k ht)) into big
                 else collect (cons k (gethash k ht)) into small
                 finally (return (list :big big :small small)))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_cl_loop_with_destructuring_and_collect_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((data '(((1 . "a") (2 . "b")) ((3 . "c")) ((4 . "d") (5 . "e") (6 . "f")))))
  (list
   (cl-loop for sublist in data
            append (cl-loop for (k . v) in sublist collect (list k v)))
   (cl-loop for sublist in data
            nconc (cl-loop for (k . v) in sublist collect (* k 100)))
   (cl-loop for sublist in data
            for i from 1
            sum (length sublist) into total
            collect i into indices
            finally (return (list :total total :indices indices)))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_cl_destructuring_with_default_and_ignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((alist '((a . 1) (b . 2) (c . 3))))
  (list
   (cl-destructuring-bind (a b c) '(1 2 3) (+ a b c))
   (cl-destructuring-bind (a b &optional c) '(1 2) (list a b c))
   (cl-destructuring-bind (a &rest rest) '(1 2 3 4 5) (cons a rest))
   (cl-destructuring-bind (&key a b (c 99)) '(:a 1 :b 2) (list a b c))
   (cl-destructuring-bind (&whole whole a b) '(1 2) (list whole a b))
   (cl-destructuring-bind (a (b c) d) '(1 (2 3) 4) (list a b c d))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_cl_setf_getf_pushnew_remf_plist_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-pushnew)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p '(:a 1 :b 2)))
  (cl-pushnew 5 (cl-getf p :c) :test #'=)   ; adds new key
  (setf (cl-getf p :b) 99)
  (let ((p2 p))
    (cl-remf p2 :a))
  (list p
        (cl-getf p :a)
        (cl-getf p :b)
        (cl-getf p :c)))
"##,
        expect,
    );
}

#[test]
fn div_cx62_defmacro_macroexpand_dotted_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx62-double-when (cond form)
  `(if ,cond (progn ,form ,form) nil))
(let ((expanded (macroexpand
                 '(neo-cx62-double-when (> x 5) (incf x)))))
  (list expanded
        (eval (cons 'let '((x 10))) t)
        (eval '(let ((x 10)) (neo-cx62-double-when (> x 5) (setq x (+ x 1)))) t)))
"##,
        expect,
    );
}

#[test]
fn div_cx62_eieio_cl_defmethod_dispatch_with_qualifiers_print_undo_textprop_marker_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx62-z () ((name :initarg :name :initform "anon")))
      (let (calls)
        (cl-defmethod neo-cx62-describe :before ((o neo-cx62-z))
          (push :before calls))
        (cl-defmethod neo-cx62-describe ((o neo-cx62-z))
          (push (slot-value o 'name) calls))
        (cl-defmethod neo-cx62-describe :after ((o neo-cx62-z))
          (push :after calls))
        (let* ((inst (make-instance 'neo-cx62-z :name "alpha"))
               (disp (let ((print-circle t)) (neo-cx62-describe inst))))
          (with-temp-buffer
            (buffer-enable-undo)
            (insert "HEADER BODY")
            (put-text-property 1 6 'face 'bold)
            (let ((m (set-marker (make-marker) 5)))
              (narrow-to-region 2 8)
              (let ((state (list (buffer-string) (marker-position m)
                                 (text-properties-at 1)
                                 (text-properties-at 6))))
                (widen)
                (list (nreverse calls) disp state
                      (buffer-string) (marker-position m)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx62_eieio_static_class_slots_class_allocated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 10 10 :a :b)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eieio)
      (defclass neo-cx62-static ()
        ((counter :allocation :class :initform 0)
         (instance-tag :initarg :tag)))
      (let ((a (make-instance 'neo-cx62-static :tag :a))
            (b (make-instance 'neo-cx62-static :tag :b)))
        (oset a counter 5)
        (let ((c-a (slot-value a 'counter))
              (c-b (slot-value b 'counter)))
          (oset b counter 10)
          (list c-a c-b
                (slot-value a 'counter)
                (slot-value b 'counter)
                (slot-value a 'instance-tag)
                (slot-value b 'instance-tag)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
