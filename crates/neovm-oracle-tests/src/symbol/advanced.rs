//! Oracle parity tests for advanced symbol operations:
//! `intern`, `intern-soft`, `obarray` access, `symbol-name`,
//! `symbol-value`, `symbol-function`, `symbol-plist` with
//! complex patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// intern / intern-soft
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (eq (intern "car") 'car)
                        (eq (intern "nil") nil)
                        (eq (intern "t") t)
                        (symbolp (intern "car"))
                        (symbolp (intern "some-name")))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_intern_soft_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (intern-soft "car")
                        (intern-soft "+")
                        (intern-soft "neovm--surely-not-interned-xyz")
                        (eq (intern-soft "car") 'car))"#;
    let expect = expect_test::expect![[r#""OK (car + nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_intern_creates_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // intern creates a new symbol; intern-soft returns nil if not found
    let form = r#"(let ((name "neovm--test-intern-temp-sym"))
                    ;; Probably not interned yet
                    (let ((before (intern-soft name)))
                      ;; Now intern it
                      (let ((sym (intern name)))
                        ;; intern-soft should now find it
                        (let ((after (intern-soft name)))
                          (list (null before)
                                (symbolp sym)
                                (eq sym after)
                                (symbol-name sym))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t \"neovm--test-intern-temp-sym\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-name
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (symbol-name 'foo)
                        (symbol-name 'bar-baz)
                        (symbol-name t)
                        (symbol-name nil)
                        (symbol-name '+)
                        (symbol-name 'with\ space))"#;
    let expect =
        expect_test::expect![[r#""OK (\"foo\" \"bar-baz\" \"t\" \"nil\" \"+\" \"with space\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-value / boundp / set / makunbound
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_value_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
                    (defvar neovm--test-sv-var 42)
                    (unwind-protect
                        (let ((v1 (symbol-value 'neovm--test-sv-var))
                              (b1 (boundp 'neovm--test-sv-var)))
                          (set 'neovm--test-sv-var 99)
                          (let ((v2 (symbol-value 'neovm--test-sv-var)))
                            (makunbound 'neovm--test-sv-var)
                            (let ((b2 (boundp 'neovm--test-sv-var)))
                              (list v1 b1 v2 b2))))
                      (makunbound 'neovm--test-sv-var)))"#;
    let expect = expect_test::expect![[r#""OK (42 t 99 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-plist / put / get
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
                    (setplist 'neovm--test-plist-sym nil)
                    (unwind-protect
                        (progn
                          (put 'neovm--test-plist-sym 'color 'red)
                          (put 'neovm--test-plist-sym 'size 42)
                          (put 'neovm--test-plist-sym 'active t)
                          (list (get 'neovm--test-plist-sym 'color)
                                (get 'neovm--test-plist-sym 'size)
                                (get 'neovm--test-plist-sym 'active)
                                (get 'neovm--test-plist-sym 'missing)
                                (symbol-plist 'neovm--test-plist-sym)))
                      (setplist 'neovm--test-plist-sym nil)))"#;
    let expect = expect_test::expect![[r#""OK (red 42 t nil (color red size 42 active t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol registry pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use symbol plists as a lightweight object system
    let form = r#"(let ((register-type
                         (lambda (name &rest props)
                           (let ((sym (intern
                                       (concat "neovm--test-type-"
                                               (symbol-name name)))))
                             (setplist sym nil)
                             (while props
                               (put sym (car props) (cadr props))
                               (setq props (cddr props)))
                             sym)))
                        (get-type
                         (lambda (name prop)
                           (get (intern-soft
                                 (concat "neovm--test-type-"
                                         (symbol-name name)))
                                prop))))
                    (unwind-protect
                        (progn
                          (funcall register-type 'point
                                   'fields '(x y)
                                   'constructor 'make-point
                                   'mutable t)
                          (funcall register-type 'line
                                   'fields '(start end)
                                   'constructor 'make-line
                                   'mutable nil)
                          (list
                           (funcall get-type 'point 'fields)
                           (funcall get-type 'point 'mutable)
                           (funcall get-type 'line 'fields)
                           (funcall get-type 'line 'mutable)
                           (funcall get-type 'circle 'fields)))
                      (setplist (intern "neovm--test-type-point") nil)
                      (setplist (intern "neovm--test-type-line") nil)))"#;
    let expect = expect_test::expect![[r#""OK ((x y) t (start end) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: dynamic dispatch using symbol properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple method dispatch via symbol properties
    let form = r#"(progn
                    (put 'neovm--test-shape-circle 'area
                         (lambda (s) (* 3.14159 (plist-get s :radius)
                                       (plist-get s :radius))))
                    (put 'neovm--test-shape-circle 'perimeter
                         (lambda (s) (* 2 3.14159 (plist-get s :radius))))
                    (put 'neovm--test-shape-rect 'area
                         (lambda (s) (* (plist-get s :width)
                                        (plist-get s :height))))
                    (put 'neovm--test-shape-rect 'perimeter
                         (lambda (s) (* 2 (+ (plist-get s :width)
                                              (plist-get s :height)))))
                    (unwind-protect
                        (let ((dispatch
                               (lambda (shape method)
                                 (let ((type (plist-get shape :type)))
                                   (funcall (get type method) shape)))))
                          (let ((c '(:type neovm--test-shape-circle
                                     :radius 5))
                                (r '(:type neovm--test-shape-rect
                                     :width 3 :height 4)))
                            (list
                             (< (abs (- (funcall dispatch c 'area)
                                        78.5397)) 0.01)
                             (funcall dispatch r 'area)
                             (funcall dispatch r 'perimeter))))
                      (setplist 'neovm--test-shape-circle nil)
                      (setplist 'neovm--test-shape-rect nil)))"#;
    let expect = expect_test::expect![[r#""OK (t 12 14)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
