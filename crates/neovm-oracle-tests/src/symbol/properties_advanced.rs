//! Oracle parity tests for advanced symbol property operations:
//! `get`/`put` on symbol plists, `symbol-plist` retrieval,
//! `setplist` replacing entire plists, multiple properties on
//! the same symbol, metadata storage, and property-based dispatch.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// get/put basic round-trip with multiple types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_get_put_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--test-spa-rt nil)
  (unwind-protect
      (progn
        (put 'neovm--test-spa-rt 'int-val 42)
        (put 'neovm--test-spa-rt 'str-val "hello world")
        (put 'neovm--test-spa-rt 'sym-val 'some-symbol)
        (put 'neovm--test-spa-rt 'list-val '(a b c))
        (put 'neovm--test-spa-rt 'nil-val nil)
        (put 'neovm--test-spa-rt 'float-val 3.14)
        (list (get 'neovm--test-spa-rt 'int-val)
              (get 'neovm--test-spa-rt 'str-val)
              (get 'neovm--test-spa-rt 'sym-val)
              (get 'neovm--test-spa-rt 'list-val)
              (get 'neovm--test-spa-rt 'nil-val)
              (get 'neovm--test-spa-rt 'float-val)
              (get 'neovm--test-spa-rt 'nonexistent)))
    (setplist 'neovm--test-spa-rt nil)))"#;
    let expect =
        expect_test::expect![[r#""OK (42 \"hello world\" some-symbol (a b c) nil 3.14 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-plist retrieval reflects put mutations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_plist_reflects_mutations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--test-spa-mut nil)
  (unwind-protect
      (let ((results nil))
        ;; Initially empty
        (setq results (cons (symbol-plist 'neovm--test-spa-mut) results))
        ;; After one put
        (put 'neovm--test-spa-mut 'alpha 1)
        (setq results (cons (length (symbol-plist 'neovm--test-spa-mut)) results))
        ;; After three puts
        (put 'neovm--test-spa-mut 'beta 2)
        (put 'neovm--test-spa-mut 'gamma 3)
        (let ((pl (symbol-plist 'neovm--test-spa-mut)))
          (setq results (cons (length pl) results))
          ;; Verify each value is accessible through the plist directly
          (setq results (cons (plist-get pl 'alpha) results))
          (setq results (cons (plist-get pl 'beta) results))
          (setq results (cons (plist-get pl 'gamma) results)))
        ;; Overwrite alpha
        (put 'neovm--test-spa-mut 'alpha 100)
        (setq results (cons (get 'neovm--test-spa-mut 'alpha) results))
        ;; Length should NOT grow on overwrite
        (setq results (cons (length (symbol-plist 'neovm--test-spa-mut)) results))
        (nreverse results))
    (setplist 'neovm--test-spa-mut nil)))"#;
    let expect = expect_test::expect![[r#""OK (nil 2 6 1 2 3 100 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setplist replacing entire plist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_setplist_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--test-spa-repl nil)
  (unwind-protect
      (progn
        ;; Populate with several properties
        (put 'neovm--test-spa-repl 'x 10)
        (put 'neovm--test-spa-repl 'y 20)
        (put 'neovm--test-spa-repl 'z 30)
        (let ((before-x (get 'neovm--test-spa-repl 'x))
              (before-len (length (symbol-plist 'neovm--test-spa-repl))))
          ;; Replace entire plist with a completely different one
          (setplist 'neovm--test-spa-repl '(new-a 111 new-b 222))
          (let ((after-x (get 'neovm--test-spa-repl 'x))
                (after-new-a (get 'neovm--test-spa-repl 'new-a))
                (after-new-b (get 'neovm--test-spa-repl 'new-b))
                (after-len (length (symbol-plist 'neovm--test-spa-repl))))
            ;; Replace with empty to clear
            (setplist 'neovm--test-spa-repl nil)
            (let ((empty-pl (symbol-plist 'neovm--test-spa-repl)))
              (list before-x before-len
                    after-x after-new-a after-new-b after-len
                    empty-pl)))))
    (setplist 'neovm--test-spa-repl nil)))"#;
    let expect = expect_test::expect![[r#""OK (10 6 nil 111 222 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple properties on same symbol — independent updates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_multiple_independent_updates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--test-spa-multi nil)
  (unwind-protect
      (let ((snapshots nil))
        ;; Build up 5 properties one at a time, snapshot after each
        (put 'neovm--test-spa-multi 'p1 'a)
        (setq snapshots (cons (copy-sequence (symbol-plist 'neovm--test-spa-multi)) snapshots))
        (put 'neovm--test-spa-multi 'p2 'b)
        (setq snapshots (cons (copy-sequence (symbol-plist 'neovm--test-spa-multi)) snapshots))
        (put 'neovm--test-spa-multi 'p3 'c)
        (put 'neovm--test-spa-multi 'p4 'd)
        (put 'neovm--test-spa-multi 'p5 'e)
        ;; Now selectively update only p2 and p4
        (put 'neovm--test-spa-multi 'p2 'B-UPDATED)
        (put 'neovm--test-spa-multi 'p4 'D-UPDATED)
        ;; Remaining properties should be unchanged
        (list (get 'neovm--test-spa-multi 'p1)
              (get 'neovm--test-spa-multi 'p2)
              (get 'neovm--test-spa-multi 'p3)
              (get 'neovm--test-spa-multi 'p4)
              (get 'neovm--test-spa-multi 'p5)
              ;; Verify the earlier snapshots captured the right state
              (length (car snapshots))    ;; should be 4 (2 entries = 4 elts)
              (length (cadr snapshots)))) ;; should be 2 (1 entry = 2 elts)
    (setplist 'neovm--test-spa-multi nil)))"#;
    let expect = expect_test::expect![[r#""OK (a B-UPDATED c D-UPDATED e 4 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// put returns the value stored
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_put_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (setplist 'neovm--test-spa-retv nil)
  (unwind-protect
      (list (put 'neovm--test-spa-retv 'a 42)
            (put 'neovm--test-spa-retv 'b "string-val")
            (put 'neovm--test-spa-retv 'c '(1 2 3))
            (put 'neovm--test-spa-retv 'a 99)  ;; overwrite returns new value
            ;; Verify the return value equals what get retrieves
            (equal (put 'neovm--test-spa-retv 'd 'sym)
                   (get 'neovm--test-spa-retv 'd)))
    (setplist 'neovm--test-spa-retv nil)))"#;
    let expect = expect_test::expect![[r#""OK (42 \"string-val\" (1 2 3) 99 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol properties as a metadata / annotation system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_metadata_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use symbol properties to implement a function annotation/metadata system.
    // Annotate functions with :doc, :version, :deprecated, :args metadata
    // and query/filter by annotations.
    let form = r#"(progn
  (dolist (s '(neovm--test-meta-fn-add
               neovm--test-meta-fn-sub
               neovm--test-meta-fn-mul
               neovm--test-meta-fn-old-div))
    (setplist s nil))
  (unwind-protect
      (let ((annotate
             (lambda (fn-sym &rest props)
               "Store metadata properties on a function symbol."
               (while props
                 (put fn-sym (car props) (cadr props))
                 (setq props (cddr props)))
               fn-sym))
            (get-annotation
             (lambda (fn-sym key)
               (get fn-sym key)))
            (all-annotated nil))
        ;; Annotate several "functions"
        (funcall annotate 'neovm--test-meta-fn-add
                 :doc "Add two numbers"
                 :version "1.0"
                 :args '(a b)
                 :pure t)
        (funcall annotate 'neovm--test-meta-fn-sub
                 :doc "Subtract b from a"
                 :version "1.2"
                 :args '(a b)
                 :pure t)
        (funcall annotate 'neovm--test-meta-fn-mul
                 :doc "Multiply numbers"
                 :version "2.0"
                 :args '(a b)
                 :pure t)
        (funcall annotate 'neovm--test-meta-fn-old-div
                 :doc "Divide (deprecated)"
                 :version "0.5"
                 :args '(a b)
                 :deprecated t
                 :replacement 'neovm--test-meta-fn-safe-div)
        ;; Query: collect non-deprecated pure functions
        (setq all-annotated '(neovm--test-meta-fn-add
                              neovm--test-meta-fn-sub
                              neovm--test-meta-fn-mul
                              neovm--test-meta-fn-old-div))
        (let ((pure-fns nil)
              (deprecated-fns nil))
          (dolist (fn all-annotated)
            (when (funcall get-annotation fn :pure)
              (unless (funcall get-annotation fn :deprecated)
                (setq pure-fns (cons fn pure-fns))))
            (when (funcall get-annotation fn :deprecated)
              (setq deprecated-fns
                    (cons (list fn
                                (funcall get-annotation fn :replacement)
                                (funcall get-annotation fn :version))
                          deprecated-fns))))
          (list (length (nreverse pure-fns))
                deprecated-fns
                (funcall get-annotation 'neovm--test-meta-fn-add :doc)
                (funcall get-annotation 'neovm--test-meta-fn-mul :version)
                (funcall get-annotation 'neovm--test-meta-fn-old-div :deprecated))))
    ;; Cleanup
    (dolist (s '(neovm--test-meta-fn-add
                 neovm--test-meta-fn-sub
                 neovm--test-meta-fn-mul
                 neovm--test-meta-fn-old-div))
      (setplist s nil))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 ((neovm--test-meta-fn-old-div neovm--test-meta-fn-safe-div \"0.5\")) \"Add two numbers\" \"2.0\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property-based method dispatch with inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_method_dispatch_with_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple prototype-based OOP system using symbol properties.
    // Each "type" symbol stores methods as properties. :parent enables lookup
    // chain (inheritance). dispatch walks the chain until it finds the method.
    let form = r#"(progn
  (dolist (s '(neovm--test-dispatch-animal
               neovm--test-dispatch-dog
               neovm--test-dispatch-puppy))
    (setplist s nil))
  (unwind-protect
      (let ((define-method
             (lambda (type-sym method-name fn)
               (put type-sym method-name fn)))
            (set-parent
             (lambda (child-sym parent-sym)
               (put child-sym :parent parent-sym)))
            (lookup-method nil))
        ;; lookup-method walks the inheritance chain
        (setq lookup-method
              (lambda (type-sym method-name)
                (let ((found (get type-sym method-name)))
                  (if found
                      found
                    (let ((parent (get type-sym :parent)))
                      (if parent
                          (funcall lookup-method parent method-name)
                        nil))))))
        ;; Define base type: animal
        (funcall define-method 'neovm--test-dispatch-animal 'speak
                 (lambda (self) (concat (plist-get self :name) " makes a sound")))
        (funcall define-method 'neovm--test-dispatch-animal 'describe
                 (lambda (self) (format "Animal: %s (age %d)"
                                        (plist-get self :name)
                                        (plist-get self :age))))
        ;; Define dog inheriting from animal, overriding speak
        (funcall set-parent 'neovm--test-dispatch-dog 'neovm--test-dispatch-animal)
        (funcall define-method 'neovm--test-dispatch-dog 'speak
                 (lambda (self) (concat (plist-get self :name) " barks: Woof!")))
        (funcall define-method 'neovm--test-dispatch-dog 'fetch
                 (lambda (self item) (format "%s fetches the %s"
                                             (plist-get self :name) item)))
        ;; Define puppy inheriting from dog (multi-level)
        (funcall set-parent 'neovm--test-dispatch-puppy 'neovm--test-dispatch-dog)
        (funcall define-method 'neovm--test-dispatch-puppy 'speak
                 (lambda (self) (concat (plist-get self :name) " yips!")))
        ;; Create instances
        (let ((rex '(:type neovm--test-dispatch-dog :name "Rex" :age 5))
              (tiny '(:type neovm--test-dispatch-puppy :name "Tiny" :age 1))
              (dispatch
               (lambda (obj method &rest args)
                 (let ((fn (funcall lookup-method (plist-get obj :type) method)))
                   (if fn
                       (apply fn obj args)
                     (format "No method %s on %s" method (plist-get obj :type)))))))
          (list
           ;; Dog speaks (own method)
           (funcall dispatch rex 'speak)
           ;; Dog describe (inherited from animal)
           (funcall dispatch rex 'describe)
           ;; Dog fetch (own method)
           (funcall dispatch rex 'fetch "ball")
           ;; Puppy speaks (own override)
           (funcall dispatch tiny 'speak)
           ;; Puppy describe (inherited from animal via dog)
           (funcall dispatch tiny 'describe)
           ;; Puppy fetch (inherited from dog)
           (funcall dispatch tiny 'fetch "stick")
           ;; Missing method
           (funcall dispatch rex 'swim))))
    ;; Cleanup
    (dolist (s '(neovm--test-dispatch-animal
                 neovm--test-dispatch-dog
                 neovm--test-dispatch-puppy))
      (setplist s nil))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Rex barks: Woof!\" \"Animal: Rex (age 5)\" \"Rex fetches the ball\" \"Tiny yips!\" \"Animal: Tiny (age 1)\" \"Tiny fetches the stick\" \"No method swim on neovm--test-dispatch-dog\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property diffing — detect what changed between two states
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_props_property_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Snapshot symbol properties, mutate them, then compute a diff
    // (added, removed, changed properties).
    let form = r#"(progn
  (setplist 'neovm--test-spa-diff nil)
  (unwind-protect
      (let ((snapshot-plist
             (lambda (sym)
               "Return a copy of the symbol's plist."
               (copy-sequence (symbol-plist sym))))
            (plist-keys
             (lambda (pl)
               (let ((keys nil) (rest pl))
                 (while rest
                   (setq keys (cons (car rest) keys))
                   (setq rest (cddr rest)))
                 (nreverse keys))))
            (diff-plists
             (lambda (old-pl new-pl plist-keys-fn)
               (let ((added nil) (removed nil) (changed nil)
                     (old-keys (funcall plist-keys-fn old-pl))
                     (new-keys (funcall plist-keys-fn new-pl)))
                 ;; Find added and changed
                 (dolist (k new-keys)
                   (if (not (plist-member old-pl k))
                       (setq added (cons (cons k (plist-get new-pl k)) added))
                     (unless (equal (plist-get old-pl k) (plist-get new-pl k))
                       (setq changed (cons (list k
                                                  (plist-get old-pl k)
                                                  (plist-get new-pl k))
                                           changed)))))
                 ;; Find removed
                 (dolist (k old-keys)
                   (unless (plist-member new-pl k)
                     (setq removed (cons (cons k (plist-get old-pl k)) removed))))
                 (list :added (nreverse added)
                       :removed (nreverse removed)
                       :changed (nreverse changed))))))
        ;; Set initial state
        (put 'neovm--test-spa-diff 'color 'red)
        (put 'neovm--test-spa-diff 'size 10)
        (put 'neovm--test-spa-diff 'weight 50)
        (let ((snap1 (funcall snapshot-plist 'neovm--test-spa-diff)))
          ;; Mutate: change color, remove weight (via setplist rebuild), add shape
          (put 'neovm--test-spa-diff 'color 'blue)
          (put 'neovm--test-spa-diff 'shape 'circle)
          ;; Simulate remove by rebuilding plist without weight
          (let ((pl (symbol-plist 'neovm--test-spa-diff))
                (new-pl nil))
            (while pl
              (unless (eq (car pl) 'weight)
                (setq new-pl (cons (cadr pl) (cons (car pl) new-pl))))
              (setq pl (cddr pl)))
            (setplist 'neovm--test-spa-diff (nreverse new-pl)))
          (let ((snap2 (funcall snapshot-plist 'neovm--test-spa-diff)))
            (funcall diff-plists snap1 snap2 plist-keys))))
    (setplist 'neovm--test-spa-diff nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (:added ((shape . circle)) :removed ((weight . 50)) :changed ((color red blue)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
