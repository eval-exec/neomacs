//! Oracle parity tests for `symbol-plist`, `setplist`, `get`, `put` with
//! complex patterns: basic get/put, full plist retrieval, setplist replacement,
//! multiple properties, object systems, property inheritance, and metadata
//! annotation systems.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic get/put round-trip with various value types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_basic_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--spp-basic nil)
  (unwind-protect
      (progn
        (put 'neovm--spp-basic 'name "test-widget")
        (put 'neovm--spp-basic 'count 42)
        (put 'neovm--spp-basic 'ratio 2.718)
        (put 'neovm--spp-basic 'tag 'important)
        (put 'neovm--spp-basic 'items '(a b c d))
        (put 'neovm--spp-basic 'empty nil)
        (put 'neovm--spp-basic 'nested '((1 2) (3 4) (5)))
        (list
          (get 'neovm--spp-basic 'name)
          (get 'neovm--spp-basic 'count)
          (get 'neovm--spp-basic 'ratio)
          (get 'neovm--spp-basic 'tag)
          (get 'neovm--spp-basic 'items)
          (get 'neovm--spp-basic 'empty)
          (get 'neovm--spp-basic 'nested)
          ;; Non-existent property returns nil
          (get 'neovm--spp-basic 'nonexistent)
          ;; put returns the value
          (put 'neovm--spp-basic 'new-prop 'hello)))
    (setplist 'neovm--spp-basic nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"test-widget\" 42 2.718 important (a b c d) nil ((1 2) (3 4) (5)) nil hello)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setplist to replace entire plist, then verify old properties gone
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_setplist_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--spp-repl nil)
  (unwind-protect
      (let ((trace nil))
        ;; Set up initial properties
        (put 'neovm--spp-repl 'x 10)
        (put 'neovm--spp-repl 'y 20)
        (put 'neovm--spp-repl 'z 30)
        (setq trace (cons (length (symbol-plist 'neovm--spp-repl)) trace))
        (setq trace (cons (get 'neovm--spp-repl 'x) trace))

        ;; Replace with completely different plist
        (setplist 'neovm--spp-repl '(alpha 100 beta 200 gamma 300 delta 400))
        (setq trace (cons (length (symbol-plist 'neovm--spp-repl)) trace))
        ;; Old properties gone
        (setq trace (cons (get 'neovm--spp-repl 'x) trace))
        (setq trace (cons (get 'neovm--spp-repl 'y) trace))
        ;; New properties accessible
        (setq trace (cons (get 'neovm--spp-repl 'alpha) trace))
        (setq trace (cons (get 'neovm--spp-repl 'delta) trace))

        ;; Replace with empty
        (setplist 'neovm--spp-repl nil)
        (setq trace (cons (symbol-plist 'neovm--spp-repl) trace))
        (setq trace (cons (get 'neovm--spp-repl 'alpha) trace))

        ;; Replace with single property
        (setplist 'neovm--spp-repl '(solo 999))
        (setq trace (cons (get 'neovm--spp-repl 'solo) trace))
        (setq trace (cons (length (symbol-plist 'neovm--spp-repl)) trace))

        (nreverse trace))
    (setplist 'neovm--spp-repl nil)))"#;
    let expect = expect_test::expect![[r#""OK (6 10 8 nil nil 100 400 nil nil 999 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-plist returns the full list; plist-get works on it
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_full_list_retrieval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--spp-full nil)
  (unwind-protect
      (progn
        ;; Empty plist
        (let ((empty-pl (symbol-plist 'neovm--spp-full)))
          (put 'neovm--spp-full 'a 1)
          (put 'neovm--spp-full 'b 2)
          (put 'neovm--spp-full 'c 3)
          (let ((pl (symbol-plist 'neovm--spp-full)))
            (list
              ;; Empty plist is nil
              empty-pl
              (null empty-pl)
              ;; Full plist is a proper list
              (listp pl)
              ;; Length is 2 * num-properties
              (length pl)
              ;; Can use plist-get on retrieved plist
              (plist-get pl 'a)
              (plist-get pl 'b)
              (plist-get pl 'c)
              (plist-get pl 'nonexistent)
              ;; plist-member returns tail starting at key
              (not (null (plist-member pl 'b)))
              (null (plist-member pl 'zzz))
              ;; Mutating the retrieved plist does NOT affect the symbol
              (progn
                (plist-put pl 'a 999)
                (get 'neovm--spp-full 'a))))))
    (setplist 'neovm--spp-full nil)))"#;
    let expect = expect_test::expect![[r#""OK (nil t t 6 1 2 3 nil t t 999)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple properties on one symbol — independent updates, overwrites
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_multiple_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--spp-multi nil)
  (unwind-protect
      (let ((snapshots nil))
        ;; Add properties one at a time
        (dotimes (i 8)
          (put 'neovm--spp-multi (intern (format "prop-%d" i)) (* i i)))
        ;; Verify all 8
        (setq snapshots
              (cons (mapcar (lambda (i)
                              (get 'neovm--spp-multi (intern (format "prop-%d" i))))
                            '(0 1 2 3 4 5 6 7))
                    snapshots))
        ;; Overwrite even-numbered properties
        (dolist (i '(0 2 4 6))
          (put 'neovm--spp-multi (intern (format "prop-%d" i)) (+ 1000 i)))
        ;; Verify mixed state
        (setq snapshots
              (cons (mapcar (lambda (i)
                              (get 'neovm--spp-multi (intern (format "prop-%d" i))))
                            '(0 1 2 3 4 5 6 7))
                    snapshots))
        ;; Length should still be 16 (8 props * 2)
        (setq snapshots (cons (length (symbol-plist 'neovm--spp-multi)) snapshots))
        ;; Adding a new one grows it
        (put 'neovm--spp-multi 'extra 'bonus)
        (setq snapshots (cons (length (symbol-plist 'neovm--spp-multi)) snapshots))
        (nreverse snapshots))
    (setplist 'neovm--spp-multi nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 4 9 16 25 36 49) (1000 1 1002 9 1004 25 1006 49) 16 18)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: simple object system using symbol plists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_object_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each "object" is a symbol. Properties are fields. Methods are
    // lambda-valued properties. Constructor sets initial fields.
    let form = r#"(progn
  (dolist (s '(neovm--spp-obj-counter1 neovm--spp-obj-counter2
               neovm--spp-obj-account1))
    (setplist s nil))

  (unwind-protect
      (let ((make-counter
             (lambda (sym initial)
               (setplist sym nil)
               (put sym :type 'counter)
               (put sym :value initial)
               (put sym :history (list initial))
               (put sym :increment
                    (lambda (self n)
                      (let ((new-val (+ (get self :value) n)))
                        (put self :value new-val)
                        (put self :history
                             (append (get self :history) (list new-val)))
                        new-val)))
               (put sym :reset
                    (lambda (self)
                      (put self :value 0)
                      (put self :history
                           (append (get self :history) (list 0)))
                      0))
               sym))
            (send
             (lambda (obj method &rest args)
               (let ((fn (get obj method)))
                 (if fn
                     (apply fn obj args)
                   (list 'error (format "no method %s" method)))))))

        ;; Create counters
        (funcall make-counter 'neovm--spp-obj-counter1 0)
        (funcall make-counter 'neovm--spp-obj-counter2 100)

        ;; Operate on counter1
        (funcall send 'neovm--spp-obj-counter1 :increment 5)
        (funcall send 'neovm--spp-obj-counter1 :increment 3)
        (funcall send 'neovm--spp-obj-counter1 :increment 12)
        (funcall send 'neovm--spp-obj-counter1 :reset)
        (funcall send 'neovm--spp-obj-counter1 :increment 1)

        ;; Operate on counter2
        (funcall send 'neovm--spp-obj-counter2 :increment 50)
        (funcall send 'neovm--spp-obj-counter2 :increment -30)

        (list
          (get 'neovm--spp-obj-counter1 :value)
          (get 'neovm--spp-obj-counter1 :history)
          (get 'neovm--spp-obj-counter1 :type)
          (get 'neovm--spp-obj-counter2 :value)
          (get 'neovm--spp-obj-counter2 :history)
          ;; Unknown method
          (funcall send 'neovm--spp-obj-counter1 :nonexistent)))
    (dolist (s '(neovm--spp-obj-counter1 neovm--spp-obj-counter2
                 neovm--spp-obj-account1))
      (setplist s nil))))"#;
    let expect = expect_test::expect![[
        r#""OK (1 (0 5 8 20 0 1) counter 120 (100 150 120) (error \"no method :nonexistent\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property inheritance (child symbol inherits parent properties)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_property_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement property inheritance where a child symbol's get first
    // checks its own plist, then walks up a :parent chain.
    let form = r#"(progn
  (dolist (s '(neovm--spp-inh-base neovm--spp-inh-mid neovm--spp-inh-leaf))
    (setplist s nil))
  (unwind-protect
      (let ((inh-get nil))
        ;; Recursive getter that walks parent chain
        (setq inh-get
              (lambda (sym prop)
                (let ((val (get sym prop)))
                  (if val
                      val
                    (let ((parent (get sym :parent)))
                      (if parent
                          (funcall inh-get parent prop)
                        nil))))))

        ;; Set up hierarchy: base -> mid -> leaf
        ;; Base: defines defaults
        (put 'neovm--spp-inh-base :color 'red)
        (put 'neovm--spp-inh-base :size 10)
        (put 'neovm--spp-inh-base :visible t)
        (put 'neovm--spp-inh-base :label "base-item")

        ;; Mid: overrides color, adds border
        (put 'neovm--spp-inh-mid :parent 'neovm--spp-inh-base)
        (put 'neovm--spp-inh-mid :color 'blue)
        (put 'neovm--spp-inh-mid :border 'solid)

        ;; Leaf: overrides size, adds tooltip
        (put 'neovm--spp-inh-leaf :parent 'neovm--spp-inh-mid)
        (put 'neovm--spp-inh-leaf :size 25)
        (put 'neovm--spp-inh-leaf :tooltip "I'm a leaf")

        (list
          ;; Leaf: color inherited from mid (not base)
          (funcall inh-get 'neovm--spp-inh-leaf :color)
          ;; Leaf: size overridden locally
          (funcall inh-get 'neovm--spp-inh-leaf :size)
          ;; Leaf: visible inherited from base (skips mid)
          (funcall inh-get 'neovm--spp-inh-leaf :visible)
          ;; Leaf: label inherited from base
          (funcall inh-get 'neovm--spp-inh-leaf :label)
          ;; Leaf: border inherited from mid
          (funcall inh-get 'neovm--spp-inh-leaf :border)
          ;; Leaf: tooltip is local
          (funcall inh-get 'neovm--spp-inh-leaf :tooltip)
          ;; Mid: no tooltip (not inherited upward or from parent)
          (funcall inh-get 'neovm--spp-inh-mid :tooltip)
          ;; Base: color is own
          (funcall inh-get 'neovm--spp-inh-base :color)
          ;; Nonexistent property at all levels
          (funcall inh-get 'neovm--spp-inh-leaf :nonexistent)
          ;; Dynamic override: change base color, leaf still gets mid's blue
          (progn
            (put 'neovm--spp-inh-base :color 'green)
            (list (funcall inh-get 'neovm--spp-inh-leaf :color)
                  (funcall inh-get 'neovm--spp-inh-mid :color)
                  (funcall inh-get 'neovm--spp-inh-base :color)))))
    (dolist (s '(neovm--spp-inh-base neovm--spp-inh-mid neovm--spp-inh-leaf))
      (setplist s nil))))"#;
    let expect = expect_test::expect![[
        r#""OK (blue 25 t \"base-item\" solid \"I'm a leaf\" nil red nil (blue blue green))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: metadata annotation system using symbol plists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_metadata_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an annotation system: annotate symbols with typed metadata,
    // query by annotation type, validate annotation constraints.
    let form = r#"(progn
  (defvar neovm--spp-ann-registry nil)

  (dolist (s '(neovm--spp-ann-fn1 neovm--spp-ann-fn2 neovm--spp-ann-fn3
               neovm--spp-ann-fn4))
    (setplist s nil))

  (unwind-protect
      (let ((annotate
             (lambda (sym annotation-type value)
               ;; Store annotation under a namespaced key
               (let ((key (intern (concat ":ann-" (symbol-name annotation-type)))))
                 (put sym key value)
                 ;; Track in registry
                 (unless (memq sym neovm--spp-ann-registry)
                   (setq neovm--spp-ann-registry
                         (cons sym neovm--spp-ann-registry)))
                 value)))
            (get-annotation
             (lambda (sym annotation-type)
               (let ((key (intern (concat ":ann-" (symbol-name annotation-type)))))
                 (get sym key))))
            (has-annotation
             (lambda (sym annotation-type)
               (let ((key (intern (concat ":ann-" (symbol-name annotation-type)))))
                 (not (null (plist-member (symbol-plist sym) key))))))
            (find-by-annotation
             (lambda (annotation-type pred)
               "Find all symbols in registry where PRED returns non-nil for annotation value."
               (let ((matches nil))
                 (dolist (sym neovm--spp-ann-registry)
                   (let ((key (intern (concat ":ann-" (symbol-name annotation-type)))))
                     (let ((val (get sym key)))
                       (when (and val (funcall pred val))
                         (setq matches (cons sym matches))))))
                 (nreverse matches)))))

        (setq neovm--spp-ann-registry nil)

        ;; Annotate functions
        (funcall annotate 'neovm--spp-ann-fn1 'author "Alice")
        (funcall annotate 'neovm--spp-ann-fn1 'version '(1 0 0))
        (funcall annotate 'neovm--spp-ann-fn1 'category 'math)
        (funcall annotate 'neovm--spp-ann-fn1 'complexity 'O-n)

        (funcall annotate 'neovm--spp-ann-fn2 'author "Bob")
        (funcall annotate 'neovm--spp-ann-fn2 'version '(2 1 0))
        (funcall annotate 'neovm--spp-ann-fn2 'category 'string)
        (funcall annotate 'neovm--spp-ann-fn2 'complexity 'O-1)

        (funcall annotate 'neovm--spp-ann-fn3 'author "Alice")
        (funcall annotate 'neovm--spp-ann-fn3 'version '(1 5 0))
        (funcall annotate 'neovm--spp-ann-fn3 'category 'math)
        (funcall annotate 'neovm--spp-ann-fn3 'deprecated t)

        (funcall annotate 'neovm--spp-ann-fn4 'author "Charlie")
        (funcall annotate 'neovm--spp-ann-fn4 'version '(3 0 0))
        (funcall annotate 'neovm--spp-ann-fn4 'category 'io)

        (list
          ;; Get specific annotations
          (funcall get-annotation 'neovm--spp-ann-fn1 'author)
          (funcall get-annotation 'neovm--spp-ann-fn2 'version)
          ;; Check annotation existence
          (funcall has-annotation 'neovm--spp-ann-fn3 'deprecated)
          (funcall has-annotation 'neovm--spp-ann-fn1 'deprecated)
          ;; Find by author = "Alice"
          (length (funcall find-by-annotation 'author
                           (lambda (v) (equal v "Alice"))))
          ;; Find by category = 'math
          (length (funcall find-by-annotation 'category
                           (lambda (v) (eq v 'math))))
          ;; Find high version (major >= 2)
          (length (funcall find-by-annotation 'version
                           (lambda (v) (>= (car v) 2))))
          ;; Registry size
          (length neovm--spp-ann-registry)
          ;; Update annotation and verify
          (progn
            (funcall annotate 'neovm--spp-ann-fn1 'author "Alice (updated)")
            (funcall get-annotation 'neovm--spp-ann-fn1 'author))))

    (dolist (s '(neovm--spp-ann-fn1 neovm--spp-ann-fn2 neovm--spp-ann-fn3
                 neovm--spp-ann-fn4))
      (setplist s nil))
    (makunbound 'neovm--spp-ann-registry)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"Alice\" (2 1 0) t nil 2 2 2 4 \"Alice (updated)\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
