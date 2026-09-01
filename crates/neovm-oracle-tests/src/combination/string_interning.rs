//! Oracle parity tests for string interning and symbol table patterns:
//! intern/intern-soft, obarray manipulation, symbol properties as metadata,
//! building a symbol table for a mini language, symbol-name/symbol-value/symbol-plist
//! round-trips, make-symbol (uninterned), and gensym-like patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// intern / intern-soft: comprehensive behavior with default obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_intern_soft_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test intern and intern-soft with various name patterns,
    // repeated interning, and identity guarantees.
    let form = r#"(let ((names '("neovm--si-test-alpha-4410"
                                  "neovm--si-test-beta-4410"
                                  "neovm--si-test-gamma-4410"
                                  "neovm--si-test-delta-4410")))
                    ;; Phase 1: All should be absent via intern-soft
                    (let ((absent-before (mapcar (lambda (n) (null (intern-soft n))) names)))
                      ;; Phase 2: Intern the first two only
                      (let ((s1 (intern (nth 0 names)))
                            (s2 (intern (nth 1 names))))
                        ;; Phase 3: Check intern-soft
                        (let ((after-soft (mapcar #'intern-soft names)))
                          ;; Phase 4: Re-intern first — must return same object
                          (let ((s1-again (intern (nth 0 names))))
                            ;; Phase 5: Intern remaining
                            (let ((s3 (intern (nth 2 names)))
                                  (s4 (intern (nth 3 names))))
                              ;; All four should now be found
                              (let ((all-found (mapcar (lambda (n) (not (null (intern-soft n)))) names)))
                                (list
                                 absent-before
                                 ;; First two found, last two nil
                                 (not (null (nth 0 after-soft)))
                                 (not (null (nth 1 after-soft)))
                                 (null (nth 2 after-soft))
                                 (null (nth 3 after-soft))
                                 ;; Identity: re-intern returns eq
                                 (eq s1 s1-again)
                                 ;; All symbols are symbolp
                                 (symbolp s1) (symbolp s2) (symbolp s3) (symbolp s4)
                                 ;; symbol-name round-trip
                                 (equal (symbol-name s1) (nth 0 names))
                                 (equal (symbol-name s4) (nth 3 names))
                                 ;; All found after interning
                                 all-found))))))))"#;
    let expect = expect_test::expect![[r#""OK ((t t t t) t t t t t t t t t t t (t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol properties as metadata store
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_symbol_properties_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use symbol properties (plist) to store structured metadata on symbols
    let form = r#"(let ((syms (mapcar #'intern
                                       '("neovm--si-meta-x-5521"
                                         "neovm--si-meta-y-5521"
                                         "neovm--si-meta-z-5521"))))
                    (unwind-protect
                        (progn
                          ;; Attach metadata to each symbol
                          (put (nth 0 syms) 'type 'integer)
                          (put (nth 0 syms) 'doc "An x coordinate")
                          (put (nth 0 syms) 'range '(0 100))

                          (put (nth 1 syms) 'type 'string)
                          (put (nth 1 syms) 'doc "A y label")
                          (put (nth 1 syms) 'range nil)

                          (put (nth 2 syms) 'type 'float)
                          (put (nth 2 syms) 'doc "A z value")
                          (put (nth 2 syms) 'range '(-1.0 1.0))

                          (list
                           ;; Retrieve individual properties
                           (get (nth 0 syms) 'type)
                           (get (nth 0 syms) 'doc)
                           (get (nth 0 syms) 'range)
                           (get (nth 1 syms) 'type)
                           ;; Non-existent property
                           (get (nth 1 syms) 'nonexistent)
                           ;; Full plist
                           (symbol-plist (nth 2 syms))
                           ;; Modify a property
                           (progn (put (nth 0 syms) 'range '(0 200))
                                  (get (nth 0 syms) 'range))
                           ;; Collect all types
                           (mapcar (lambda (s) (get s 'type)) syms)
                           ;; Count properties per symbol
                           (mapcar (lambda (s) (/ (length (symbol-plist s)) 2)) syms)))
                      ;; Cleanup: remove the properties
                      (dolist (s syms)
                        (put s 'type nil)
                        (put s 'doc nil)
                        (put s 'range nil))))"#;
    let expect = expect_test::expect![[
        r#""OK (integer \"An x coordinate\" (0 100) string nil (type nil doc nil range nil) (0 200) (integer string float) (3 3 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-name / symbol-value / symbol-plist round-trips
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_roundtrip_name_value_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exercise the full lifecycle: create symbol, set value, set plist,
    // read everything back, modify, read again.
    let form = r#"(let ((sym (make-symbol "neovm--si-roundtrip-test")))
                    ;; Initially: no value, no function, empty plist
                    (let ((initial (list
                                   (symbol-name sym)
                                   (boundp sym)
                                   (fboundp sym)
                                   (symbol-plist sym))))
                      ;; Set value
                      (set sym '(1 2 3))
                      ;; Set function
                      (fset sym (lambda (x) (+ x 10)))
                      ;; Set plist entries
                      (put sym 'created-at 2026)
                      (put sym 'mutable t)
                      (put sym 'tags '(test temporary))
                      (let ((after-set (list
                                        (symbol-value sym)
                                        (funcall (symbol-function sym) 5)
                                        (get sym 'created-at)
                                        (get sym 'mutable)
                                        (get sym 'tags)
                                        (boundp sym)
                                        (fboundp sym))))
                        ;; Modify value
                        (set sym "new-value")
                        ;; Modify plist
                        (put sym 'mutable nil)
                        (put sym 'version 2)
                        (let ((after-modify (list
                                             (symbol-value sym)
                                             (get sym 'mutable)
                                             (get sym 'version))))
                          ;; Make unbound
                          (makunbound sym)
                          (let ((after-makunbound (list
                                                   (boundp sym)
                                                   ;; function still bound
                                                   (fboundp sym)
                                                   ;; plist still present
                                                   (get sym 'version))))
                            (list initial after-set after-modify after-makunbound))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--si-roundtrip-test\" nil nil nil) ((1 2 3) 15 2026 t (test temporary) t t) (\"new-value\" nil 2) (nil t 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-symbol (uninterned) and gensym-like patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_gensym_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a gensym-like facility that produces unique uninterned symbols
    // with sequential names. Verify all are distinct and uninterned.
    let form = r#"(let ((counter 0))
                    (let ((my-gensym
                           (lambda (prefix)
                             (let ((sym (make-symbol (concat prefix (number-to-string counter)))))
                               (setq counter (1+ counter))
                               sym))))
                      ;; Generate a batch of symbols
                      (let ((syms (let ((result nil) (i 0))
                                    (while (< i 8)
                                      (setq result (cons (funcall my-gensym "G") result))
                                      (setq i (1+ i)))
                                    (nreverse result))))
                        (list
                         ;; All have sequential names
                         (mapcar #'symbol-name syms)
                         ;; All are symbols
                         (mapcar #'symbolp syms)
                         ;; No two are eq (all unique objects)
                         (let ((all-unique t) (i 0))
                           (while (and all-unique (< i (length syms)))
                             (let ((j (1+ i)))
                               (while (and all-unique (< j (length syms)))
                                 (when (eq (nth i syms) (nth j syms))
                                   (setq all-unique nil))
                                 (setq j (1+ j))))
                             (setq i (1+ i)))
                           all-unique)
                         ;; None are found by intern-soft in the default obarray
                         (let ((any-interned nil))
                           (dolist (s syms)
                             (when (eq s (intern-soft (symbol-name s)))
                               (setq any-interned t)))
                           any-interned)
                         ;; Can independently set values on each
                         (progn
                           (set (nth 0 syms) 'first)
                           (set (nth 7 syms) 'last)
                           (list (symbol-value (nth 0 syms))
                                 (boundp (nth 3 syms))
                                 (symbol-value (nth 7 syms))))
                         ;; Counter advanced
                         counter))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"G0\" \"G1\" \"G2\" \"G3\" \"G4\" \"G5\" \"G6\" \"G7\") (t t t t t t t t) t nil (first nil last) 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building a mini-language symbol table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_mini_language_symtab() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a symbol table for a mini language with variable declarations,
    // type checking, scope lookup, and shadowing.
    let form = r#"(progn
  ;; Symbol table: hash table mapping name -> list of (scope . symbol) pairs
  ;; Each symbol has properties: type, value, scope-depth
  (fset 'neovm--si-symtab-new
    (lambda () (make-hash-table :test 'equal)))

  (fset 'neovm--si-symtab-declare
    (lambda (table name type value depth)
      (let ((sym (make-symbol name)))
        (put sym 'var-type type)
        (set sym value)
        (put sym 'scope-depth depth)
        ;; Push onto the chain for this name (most recent first)
        (puthash name (cons (cons depth sym)
                            (gethash name table nil))
                 table)
        sym)))

  (fset 'neovm--si-symtab-lookup
    (lambda (table name)
      (let ((chain (gethash name table)))
        (if chain (cdr (car chain)) nil))))

  (fset 'neovm--si-symtab-leave-scope
    (lambda (table depth)
      ;; Remove all entries at the given depth
      (let ((keys-to-check nil))
        (maphash (lambda (k v) (setq keys-to-check (cons k keys-to-check))) table)
        (dolist (k keys-to-check)
          (let ((chain (gethash k table)))
            (let ((filtered (delq nil (mapcar (lambda (entry)
                                                (if (= (car entry) depth) nil entry))
                                              chain))))
              (if filtered
                  (puthash k filtered table)
                (remhash k table))))))))

  (unwind-protect
      (let ((st (funcall 'neovm--si-symtab-new)))
        ;; Scope 0: global declarations
        (funcall 'neovm--si-symtab-declare st "x" 'int 10 0)
        (funcall 'neovm--si-symtab-declare st "y" 'string "hello" 0)
        (funcall 'neovm--si-symtab-declare st "z" 'bool t 0)
        ;; Scope 1: shadow x, add w
        (funcall 'neovm--si-symtab-declare st "x" 'float 3.14 1)
        (funcall 'neovm--si-symtab-declare st "w" 'int 42 1)
        ;; Lookups in scope 1: x should be shadowed
        (let ((x-in-scope1 (funcall 'neovm--si-symtab-lookup st "x"))
              (y-in-scope1 (funcall 'neovm--si-symtab-lookup st "y"))
              (w-in-scope1 (funcall 'neovm--si-symtab-lookup st "w")))
          (let ((results-scope1
                 (list
                  ;; x is shadowed: float 3.14
                  (get x-in-scope1 'var-type)
                  (symbol-value x-in-scope1)
                  ;; y unchanged: string "hello"
                  (get y-in-scope1 'var-type)
                  (symbol-value y-in-scope1)
                  ;; w is new: int 42
                  (get w-in-scope1 'var-type)
                  (symbol-value w-in-scope1))))
            ;; Leave scope 1
            (funcall 'neovm--si-symtab-leave-scope st 1)
            ;; After leaving: x reverts to global, w gone
            (let ((x-after (funcall 'neovm--si-symtab-lookup st "x"))
                  (w-after (funcall 'neovm--si-symtab-lookup st "w")))
              (list
               results-scope1
               ;; x reverted: int 10
               (get x-after 'var-type)
               (symbol-value x-after)
               ;; w is gone
               (null w-after)
               ;; z still present
               (let ((z (funcall 'neovm--si-symtab-lookup st "z")))
                 (list (get z 'var-type) (symbol-value z))))))))
    (fmakunbound 'neovm--si-symtab-new)
    (fmakunbound 'neovm--si-symtab-declare)
    (fmakunbound 'neovm--si-symtab-lookup)
    (fmakunbound 'neovm--si-symtab-leave-scope)))"#;
    let expect =
        expect_test::expect![[r#""OK ((float 3.14 string \"hello\" int 42) int 10 t (bool t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Uninterned symbols: independent value/function/plist namespaces
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_uninterned_independent_namespaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create multiple uninterned symbols with the same name, verify
    // they have completely independent value cells, function cells, and plists.
    let form = r#"(let ((name "shared-name"))
                    (let ((s1 (make-symbol name))
                          (s2 (make-symbol name))
                          (s3 (make-symbol name)))
                      ;; Set different values on each
                      (set s1 'value-one)
                      (set s2 'value-two)
                      (set s3 'value-three)
                      ;; Set different functions on each
                      (fset s1 (lambda () "fn-one"))
                      (fset s2 (lambda () "fn-two"))
                      (fset s3 (lambda () "fn-three"))
                      ;; Set different properties
                      (put s1 'tag 'first)
                      (put s2 'tag 'second)
                      (put s3 'tag 'third)
                      (put s1 'extra 'only-on-s1)
                      (list
                       ;; All have same name
                       (equal (symbol-name s1) (symbol-name s2))
                       (equal (symbol-name s2) (symbol-name s3))
                       (equal (symbol-name s1) name)
                       ;; None are eq
                       (eq s1 s2)
                       (eq s2 s3)
                       (eq s1 s3)
                       ;; Independent values
                       (symbol-value s1)
                       (symbol-value s2)
                       (symbol-value s3)
                       ;; Independent functions
                       (funcall (symbol-function s1))
                       (funcall (symbol-function s2))
                       (funcall (symbol-function s3))
                       ;; Independent plists
                       (get s1 'tag) (get s2 'tag) (get s3 'tag)
                       (get s1 'extra) (get s2 'extra)
                       ;; Mutate s1 value, others unaffected
                       (progn (set s1 'mutated)
                              (list (symbol-value s1)
                                    (symbol-value s2)
                                    (symbol-value s3))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil nil nil value-one value-two value-three \"fn-one\" \"fn-two\" \"fn-three\" first second third only-on-s1 nil (mutated value-two value-three))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol table with type registry and validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_type_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a type registry using interned symbols with properties
    // for type validation, coercion rules, and hierarchy.
    let form = r#"(let ((type-table (make-hash-table :test 'eq)))
                    ;; Register types with metadata on their symbols
                    (let ((register-type
                           (lambda (name parent coerce-fn)
                             (let ((sym (intern (concat "neovm--si-type-" (symbol-name name)))))
                               (put sym 'parent parent)
                               (put sym 'coerce coerce-fn)
                               (put sym 'registered t)
                               (puthash name sym type-table)
                               sym))))
                      ;; Define type hierarchy: number -> integer, number -> float, any -> number, any -> string
                      (let ((t-any (funcall register-type 'any nil nil))
                            (t-number (funcall register-type 'number 'any nil))
                            (t-integer (funcall register-type 'integer 'number
                                                (lambda (v) (if (integerp v) v (truncate v)))))
                            (t-float (funcall register-type 'float 'number
                                              (lambda (v) (float v))))
                            (t-string (funcall register-type 'string 'any
                                               (lambda (v) (format "%s" v)))))
                        ;; Check ancestry: is type A an ancestor of type B?
                        (let ((is-ancestor
                               (lambda (ancestor-name descendant-name)
                                 (let ((current descendant-name) (found nil))
                                   (while (and current (not found))
                                     (if (eq current ancestor-name)
                                         (setq found t)
                                       (let ((sym (gethash current type-table)))
                                         (setq current (if sym (get sym 'parent) nil)))))
                                   found))))
                          (unwind-protect
                              (list
                               ;; Type lookups work
                               (get (gethash 'integer type-table) 'registered)
                               (get (gethash 'float type-table) 'parent)
                               ;; Ancestry checks
                               (funcall is-ancestor 'any 'integer)    ; t: any -> number -> integer
                               (funcall is-ancestor 'number 'integer) ; t: number -> integer
                               (funcall is-ancestor 'any 'string)     ; t: any -> string
                               (funcall is-ancestor 'integer 'float)  ; nil: integer is not ancestor of float
                               (funcall is-ancestor 'string 'number)  ; nil
                               ;; Coercion
                               (funcall (get t-integer 'coerce) 3.7)  ; 3
                               (funcall (get t-float 'coerce) 5)      ; 5.0
                               (funcall (get t-string 'coerce) 42)    ; "42"
                               ;; Count registered types
                               (hash-table-count type-table)
                               ;; Collect all type names sorted
                               (let ((names nil))
                                 (maphash (lambda (k v) (setq names (cons k names))) type-table)
                                 (sort names (lambda (a b) (string< (symbol-name a) (symbol-name b))))))
                            ;; Cleanup: remove properties from interned symbols
                            (maphash (lambda (k v)
                                       (put v 'parent nil)
                                       (put v 'coerce nil)
                                       (put v 'registered nil))
                                     type-table))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t number t t t nil nil 3 5.0 \"42\" 5 (any float integer number string))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol interning with obarray: custom obarray isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_custom_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a custom obarray and verify that symbols interned into it
    // are isolated from the global obarray.
    let form = r#"(let ((my-obarray (obarray-make 7)))
                    ;; Intern symbols into custom obarray
                    (let ((s1 (intern "alpha" my-obarray))
                          (s2 (intern "beta" my-obarray))
                          (s3 (intern "gamma" my-obarray)))
                      ;; Assign values
                      (set s1 100)
                      (set s2 200)
                      (set s3 300)
                      (list
                       ;; All are symbols
                       (symbolp s1) (symbolp s2) (symbolp s3)
                       ;; Names are correct
                       (symbol-name s1) (symbol-name s2) (symbol-name s3)
                       ;; Values are correct
                       (symbol-value s1) (symbol-value s2) (symbol-value s3)
                       ;; Re-interning in same obarray returns eq
                       (eq s1 (intern "alpha" my-obarray))
                       ;; intern-soft finds them in custom obarray
                       (eq s1 (intern-soft "alpha" my-obarray))
                       (eq s2 (intern-soft "beta" my-obarray))
                       ;; intern-soft does NOT find them in default obarray
                       ;; (unless they happen to be real Emacs symbols, which these custom names won't be)
                       ;; The custom obarray symbol is not eq to the global one (if it existed)
                       (let ((global-alpha (intern-soft "alpha")))
                         (if global-alpha
                             (not (eq s1 global-alpha))
                           t))
                       ;; Intern-soft for non-existent symbol in custom obarray
                       (null (intern-soft "nonexistent" my-obarray))
                       ;; Count of symbols: use mapatoms
                       (let ((count 0))
                         (mapatoms (lambda (s) (setq count (1+ count))) my-obarray)
                         count))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t \"alpha\" \"beta\" \"gamma\" 100 200 300 t t t t t 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
