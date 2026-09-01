//! Advanced oracle parity tests for `intern`, `intern-soft`, and obarray
//! operations with complex patterns: symbol registries, property
//! manipulation, dynamic dispatch tables, namespace simulation, and
//! symbol lifecycle after makunbound.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// intern-soft returns nil for non-existent symbols, intern creates them
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_basic_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify the full lifecycle: intern-soft nil -> intern creates -> intern-soft finds
    // Also check that multiple intern calls return eq symbols
    let form = r#"(let ((names '("neovm--isa-life-alpha-3921"
                          "neovm--isa-life-beta-3921"
                          "neovm--isa-life-gamma-3921"
                          "neovm--isa-life-delta-3921")))
  ;; Phase 1: none exist
  (let ((pre-results (mapcar #'intern-soft names)))
    ;; Phase 2: intern first two
    (let ((sym-a (intern (nth 0 names)))
          (sym-b (intern (nth 1 names))))
      ;; Phase 3: check intern-soft after partial interning
      (let ((mid-results (mapcar #'intern-soft names)))
        ;; Phase 4: intern remaining
        (let ((sym-c (intern (nth 2 names)))
              (sym-d (intern (nth 3 names))))
          ;; Phase 5: all should be found
          (let ((post-results (mapcar #'intern-soft names)))
            (list
              ;; All nil before
              (equal pre-results '(nil nil nil nil))
              ;; First two found, last two nil after partial
              (eq (nth 0 mid-results) sym-a)
              (eq (nth 1 mid-results) sym-b)
              (null (nth 2 mid-results))
              (null (nth 3 mid-results))
              ;; All found after full interning
              (eq (nth 0 post-results) sym-a)
              (eq (nth 1 post-results) sym-b)
              (eq (nth 2 post-results) sym-c)
              (eq (nth 3 post-results) sym-d)
              ;; Re-interning returns eq symbol
              (eq (intern (nth 0 names)) sym-a)
              (eq (intern (nth 2 names)) sym-c)
              ;; symbol-name roundtrips
              (equal (symbol-name sym-a) (nth 0 names))
              (equal (symbol-name sym-d) (nth 3 names)))))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern-soft after makunbound: symbol persists in obarray
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_after_makunbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // makunbound removes the value binding but the symbol remains interned.
    // fmakunbound removes function binding but symbol remains interned.
    let form = r#"(let ((name "neovm--isa-mkub-test-6714"))
  (let ((sym (intern name)))
    ;; Set value and function
    (set sym 42)
    (fset sym (lambda (x) (+ x 1)))
    (let ((val-before (symbol-value sym))
          (fn-result-before (funcall sym 10))
          (boundp-before (boundp sym))
          (fboundp-before (fboundp sym)))
      ;; makunbound the value
      (makunbound sym)
      (let ((boundp-after-val (boundp sym))
            (fboundp-after-val (fboundp sym))
            ;; Symbol still interned!
            (still-interned (eq (intern-soft name) sym)))
        ;; fmakunbound the function
        (fmakunbound sym)
        (let ((fboundp-after-fn (fboundp sym))
              ;; Still interned even with no bindings
              (still-interned-2 (eq (intern-soft name) sym))
              ;; Can re-bind
              (can-rebind (progn (set sym 99) (symbol-value sym))))
          ;; Cleanup
          (makunbound sym)
          (list
            val-before fn-result-before
            boundp-before fboundp-before
            boundp-after-val fboundp-after-val
            still-interned
            fboundp-after-fn still-interned-2
            can-rebind))))))"#;
    let expect = expect_test::expect![[r#""OK (42 11 t t nil t t nil t 99)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol interning with property manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_symbol_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Intern symbols and attach properties, verify retrieval via intern-soft
    let form = r#"(let ((names '("neovm--isa-prop-x-8201"
                          "neovm--isa-prop-y-8201"
                          "neovm--isa-prop-z-8201")))
  (let ((syms (mapcar #'intern names)))
    ;; Attach properties to each symbol
    (put (nth 0 syms) 'type 'integer)
    (put (nth 0 syms) 'range '(0 100))
    (put (nth 0 syms) 'doc "An integer variable")
    (put (nth 1 syms) 'type 'string)
    (put (nth 1 syms) 'max-length 256)
    (put (nth 2 syms) 'type 'boolean)
    (put (nth 2 syms) 'default t)
    ;; Retrieve via intern-soft and verify properties
    (let ((found-syms (mapcar #'intern-soft names)))
      (list
        ;; Properties are accessible through intern-soft result
        (get (nth 0 found-syms) 'type)
        (get (nth 0 found-syms) 'range)
        (get (nth 0 found-syms) 'doc)
        (get (nth 1 found-syms) 'type)
        (get (nth 1 found-syms) 'max-length)
        (get (nth 2 found-syms) 'type)
        (get (nth 2 found-syms) 'default)
        ;; Verify eq identity
        (eq (nth 0 found-syms) (nth 0 syms))
        ;; symbol-plist returns all properties
        (let ((plist (symbol-plist (nth 0 found-syms))))
          (list (plist-get plist 'type)
                (plist-get plist 'range)
                (plist-get plist 'doc)))
        ;; Modifying property through intern-soft symbol affects original
        (progn
          (put (nth 1 found-syms) 'extra 'added-later)
          (get (nth 1 syms) 'extra))))))"#;
    let expect = expect_test::expect![[
        r#""OK (integer (0 100) \"An integer variable\" string 256 boolean t t (integer (0 100) \"An integer variable\") added-later)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Build a symbol registry with intern/intern-soft/symbol-plist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_symbol_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a registry: register items with metadata, look them up,
    // enumerate registered items, remove entries.
    let form = r#"(progn
  (fset 'neovm--isa-reg-make
    (lambda ()
      (let ((ht (make-hash-table :test 'equal)))
        ht)))

  (fset 'neovm--isa-reg-register
    (lambda (registry name metadata)
      (let ((sym (intern (concat "neovm--isa-reg-item-" name "-5589"))))
        ;; Store metadata as plist
        (setplist sym metadata)
        ;; Track in hash table for enumeration
        (puthash name sym registry)
        sym)))

  (fset 'neovm--isa-reg-lookup
    (lambda (registry name)
      (let ((sym (gethash name registry)))
        (when sym
          (list (symbol-name sym) (symbol-plist sym))))))

  (fset 'neovm--isa-reg-all-names
    (lambda (registry)
      (let ((result nil))
        (maphash (lambda (k _v) (setq result (cons k result))) registry)
        (sort result #'string<))))

  (fset 'neovm--isa-reg-filter
    (lambda (registry prop value)
      (let ((result nil))
        (maphash (lambda (k sym)
                   (when (equal (get sym prop) value)
                     (setq result (cons k result))))
                 registry)
        (sort result #'string<))))

  (unwind-protect
      (let ((reg (funcall 'neovm--isa-reg-make)))
        ;; Register items
        (funcall 'neovm--isa-reg-register reg "widget-a"
                 '(category ui priority 1 enabled t))
        (funcall 'neovm--isa-reg-register reg "widget-b"
                 '(category ui priority 2 enabled nil))
        (funcall 'neovm--isa-reg-register reg "service-x"
                 '(category backend priority 1 enabled t))
        (funcall 'neovm--isa-reg-register reg "service-y"
                 '(category backend priority 3 enabled t))
        (list
          ;; Lookup existing
          (funcall 'neovm--isa-reg-lookup reg "widget-a")
          ;; Lookup missing
          (funcall 'neovm--isa-reg-lookup reg "nonexistent")
          ;; All names sorted
          (funcall 'neovm--isa-reg-all-names reg)
          ;; Filter by category
          (funcall 'neovm--isa-reg-filter reg 'category 'ui)
          (funcall 'neovm--isa-reg-filter reg 'category 'backend)
          ;; Filter by enabled
          (funcall 'neovm--isa-reg-filter reg 'enabled t)
          ;; Update property and re-filter
          (let ((sym (gethash "widget-b" reg)))
            (put sym 'enabled t)
            (funcall 'neovm--isa-reg-filter reg 'enabled t))
          ;; Count
          (hash-table-count reg)))
    (fmakunbound 'neovm--isa-reg-make)
    (fmakunbound 'neovm--isa-reg-register)
    (fmakunbound 'neovm--isa-reg-lookup)
    (fmakunbound 'neovm--isa-reg-all-names)
    (fmakunbound 'neovm--isa-reg-filter)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"neovm--isa-reg-item-widget-a-5589\" (category ui priority 1 enabled t)) nil (\"service-x\" \"service-y\" \"widget-a\" \"widget-b\") (\"widget-a\" \"widget-b\") (\"service-x\" \"service-y\") (\"service-x\" \"service-y\" \"widget-a\") (\"service-x\" \"service-y\" \"widget-a\" \"widget-b\") 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dynamic dispatch table using interned symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_dispatch_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a dispatch table: map operation names to handler functions
    // using interned symbols as keys. Support fallback dispatch.
    let form = r#"(progn
  (fset 'neovm--isa-disp-create
    (lambda ()
      (make-hash-table :test 'eq)))

  (fset 'neovm--isa-disp-register
    (lambda (table op-name handler)
      (puthash (intern op-name) handler table)))

  (fset 'neovm--isa-disp-call
    (lambda (table op-name args)
      (let ((sym (intern-soft op-name)))
        (if (and sym (gethash sym table))
            (apply (gethash sym table) args)
          (list 'error 'unknown-op op-name)))))

  (fset 'neovm--isa-disp-has-op
    (lambda (table op-name)
      (let ((sym (intern-soft op-name)))
        (and sym (not (null (gethash sym table nil)))))))

  (unwind-protect
      (let ((table (funcall 'neovm--isa-disp-create)))
        ;; Register operations
        (funcall 'neovm--isa-disp-register table "add"
                 (lambda (a b) (+ a b)))
        (funcall 'neovm--isa-disp-register table "mul"
                 (lambda (a b) (* a b)))
        (funcall 'neovm--isa-disp-register table "negate"
                 (lambda (x) (- x)))
        (funcall 'neovm--isa-disp-register table "format-pair"
                 (lambda (a b) (format "(%s, %s)" a b)))
        (funcall 'neovm--isa-disp-register table "list-of"
                 (lambda (&rest args) args))
        (list
          ;; Dispatch known operations
          (funcall 'neovm--isa-disp-call table "add" '(3 4))
          (funcall 'neovm--isa-disp-call table "mul" '(5 6))
          (funcall 'neovm--isa-disp-call table "negate" '(42))
          (funcall 'neovm--isa-disp-call table "format-pair" '(1 2))
          (funcall 'neovm--isa-disp-call table "list-of" '(a b c))
          ;; Dispatch unknown operation
          (funcall 'neovm--isa-disp-call table "unknown-op" '(1))
          ;; has-op checks
          (funcall 'neovm--isa-disp-has-op table "add")
          (funcall 'neovm--isa-disp-has-op table "missing")
          ;; Override an operation
          (funcall 'neovm--isa-disp-register table "add"
                   (lambda (a b) (+ a b 100)))
          (funcall 'neovm--isa-disp-call table "add" '(3 4))
          ;; Chain dispatches
          (let ((r1 (funcall 'neovm--isa-disp-call table "add" '(10 20)))
                (r2 (funcall 'neovm--isa-disp-call table "mul" '(2 3))))
            (funcall 'neovm--isa-disp-call table "add" (list r1 r2)))))
    (fmakunbound 'neovm--isa-disp-create)
    (fmakunbound 'neovm--isa-disp-register)
    (fmakunbound 'neovm--isa-disp-call)
    (fmakunbound 'neovm--isa-disp-has-op)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 30 -42 \"(1, 2)\" (a b c) (error unknown-op \"unknown-op\") t nil (closure (t) (a b) (+ a b 100)) 107 236)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Namespace simulation with prefixed symbol interning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_namespace_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate namespaces by prefixing symbol names. Each namespace has its
    // own prefix. Symbols in different namespaces don't collide.
    let form = r#"(progn
  (fset 'neovm--isa-ns-create
    (lambda (prefix)
      (list prefix (make-hash-table :test 'equal))))

  (fset 'neovm--isa-ns-prefix (lambda (ns) (car ns)))
  (fset 'neovm--isa-ns-table (lambda (ns) (cadr ns)))

  (fset 'neovm--isa-ns-full-name
    (lambda (ns name)
      (concat (funcall 'neovm--isa-ns-prefix ns) "/" name)))

  (fset 'neovm--isa-ns-define
    (lambda (ns name value)
      (let* ((full (funcall 'neovm--isa-ns-full-name ns name))
             (sym (intern full)))
        (set sym value)
        (puthash name sym (funcall 'neovm--isa-ns-table ns))
        sym)))

  (fset 'neovm--isa-ns-resolve
    (lambda (ns name)
      (let ((sym (gethash name (funcall 'neovm--isa-ns-table ns))))
        (when sym (symbol-value sym)))))

  (fset 'neovm--isa-ns-exports
    (lambda (ns)
      (let ((result nil))
        (maphash (lambda (k _v) (setq result (cons k result)))
                 (funcall 'neovm--isa-ns-table ns))
        (sort result #'string<))))

  (fset 'neovm--isa-ns-import
    (lambda (target-ns source-ns name)
      "Import a binding from source-ns into target-ns."
      (let ((sym (gethash name (funcall 'neovm--isa-ns-table source-ns))))
        (when sym
          (puthash name sym (funcall 'neovm--isa-ns-table target-ns))
          t))))

  (unwind-protect
      (let ((ns-math (funcall 'neovm--isa-ns-create "neovm-isa-math-4492"))
            (ns-str  (funcall 'neovm--isa-ns-create "neovm-isa-str-4492"))
            (ns-main (funcall 'neovm--isa-ns-create "neovm-isa-main-4492")))
        ;; Define in math namespace
        (funcall 'neovm--isa-ns-define ns-math "pi" 3.14159)
        (funcall 'neovm--isa-ns-define ns-math "e" 2.71828)
        (funcall 'neovm--isa-ns-define ns-math "zero" 0)
        ;; Define in string namespace (same short names, different full names)
        (funcall 'neovm--isa-ns-define ns-str "pi" "3.14159")
        (funcall 'neovm--isa-ns-define ns-str "greeting" "hello")
        (list
          ;; Resolve in math namespace
          (funcall 'neovm--isa-ns-resolve ns-math "pi")
          (funcall 'neovm--isa-ns-resolve ns-math "e")
          ;; Resolve in string namespace (same name, different value)
          (funcall 'neovm--isa-ns-resolve ns-str "pi")
          (funcall 'neovm--isa-ns-resolve ns-str "greeting")
          ;; No collision: math/pi and str/pi are different interned symbols
          (let ((math-sym (gethash "pi" (funcall 'neovm--isa-ns-table ns-math)))
                (str-sym  (gethash "pi" (funcall 'neovm--isa-ns-table ns-str))))
            (list (eq math-sym str-sym)
                  (symbol-name math-sym)
                  (symbol-name str-sym)))
          ;; Exports
          (funcall 'neovm--isa-ns-exports ns-math)
          (funcall 'neovm--isa-ns-exports ns-str)
          ;; Import from math to main
          (funcall 'neovm--isa-ns-import ns-main ns-math "pi")
          (funcall 'neovm--isa-ns-import ns-main ns-math "e")
          (funcall 'neovm--isa-ns-resolve ns-main "pi")
          (funcall 'neovm--isa-ns-resolve ns-main "e")
          ;; Missing resolution
          (funcall 'neovm--isa-ns-resolve ns-main "zero")
          (funcall 'neovm--isa-ns-resolve ns-math "nonexistent")))
    (fmakunbound 'neovm--isa-ns-create)
    (fmakunbound 'neovm--isa-ns-prefix)
    (fmakunbound 'neovm--isa-ns-table)
    (fmakunbound 'neovm--isa-ns-full-name)
    (fmakunbound 'neovm--isa-ns-define)
    (fmakunbound 'neovm--isa-ns-resolve)
    (fmakunbound 'neovm--isa-ns-exports)
    (fmakunbound 'neovm--isa-ns-import)))"#;
    let expect = expect_test::expect![[
        r#""OK (3.14159 2.71828 \"3.14159\" \"hello\" (nil \"neovm-isa-math-4492/pi\" \"neovm-isa-str-4492/pi\") (\"e\" \"pi\" \"zero\") (\"greeting\" \"pi\") t t 3.14159 2.71828 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// intern/intern-soft with dynamically constructed names and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_dynamic_names_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test with dynamically built names: concatenation, number-to-string,
    // format, and edge cases like empty string, very long names, special chars.
    let form = r#"(let ((results nil))
  ;; Dynamic name construction
  (let ((base "neovm--isa-dyn-")
        (suffixes '("alpha" "beta" "gamma")))
    (let ((syms (mapcar (lambda (s) (intern (concat base s "-7823"))) suffixes)))
      ;; All unique symbols
      (setq results
            (cons (list
                    (not (eq (nth 0 syms) (nth 1 syms)))
                    (not (eq (nth 1 syms) (nth 2 syms)))
                    ;; But same name re-interned gives eq
                    (eq (intern (concat base "alpha" "-7823")) (nth 0 syms)))
                  results))))
  ;; Numeric suffixes
  (let ((num-syms (mapcar (lambda (n)
                            (intern (format "neovm--isa-num-%d-7823" n)))
                          '(0 1 2 3 4))))
    (setq results
          (cons (list
                  (length num-syms)
                  ;; All distinct
                  (let ((all-diff t))
                    (let ((i 0))
                      (while (< i (length num-syms))
                        (let ((j (1+ i)))
                          (while (< j (length num-syms))
                            (when (eq (nth i num-syms) (nth j num-syms))
                              (setq all-diff nil))
                            (setq j (1+ j))))
                        (setq i (1+ i))))
                    all-diff)
                  ;; intern-soft finds them
                  (not (null (intern-soft "neovm--isa-num-3-7823"))))
                results)))
  ;; Built-in symbols: intern-soft finds them
  (setq results
        (cons (list
                (not (null (intern-soft "car")))
                (not (null (intern-soft "cdr")))
                (not (null (intern-soft "cons")))
                (not (null (intern-soft "lambda")))
                (eq (intern-soft "car") 'car)
                (eq (intern-soft "nil") nil)
                (eq (intern-soft "t") t))
              results))
  ;; Symbols with special characters
  (let ((special-names '("neovm--isa-sp-with.dot-7823"
                          "neovm--isa-sp-with/slash-7823"
                          "neovm--isa-sp-with+plus-7823"
                          "neovm--isa-sp-with=equal-7823")))
    (let ((ssyms (mapcar #'intern special-names)))
      (setq results
            (cons (mapcar (lambda (pair)
                            (eq (car pair)
                                (intern-soft (symbol-name (car pair)))))
                          (mapcar #'list ssyms))
                  results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK ((t t t) (5 t t) (t t t t t t t) (t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: symbol-based event system with intern/intern-soft
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_intern_soft_adv_event_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an event system where event types are interned symbols.
    // Listeners are stored as property lists on the event symbols.
    let form = r#"(progn
  (fset 'neovm--isa-evt-register
    (lambda (event-name handler)
      (let ((sym (intern (concat "neovm--isa-evt-" event-name "-9130"))))
        (put sym 'handlers (cons handler (or (get sym 'handlers) nil)))
        sym)))

  (fset 'neovm--isa-evt-emit
    (lambda (event-name data)
      (let ((sym (intern-soft (concat "neovm--isa-evt-" event-name "-9130"))))
        (when sym
          (let ((handlers (get sym 'handlers))
                (results nil))
            (dolist (h handlers)
              (setq results (cons (funcall h data) results)))
            (nreverse results))))))

  (fset 'neovm--isa-evt-handler-count
    (lambda (event-name)
      (let ((sym (intern-soft (concat "neovm--isa-evt-" event-name "-9130"))))
        (if sym (length (or (get sym 'handlers) nil)) 0))))

  (unwind-protect
      (progn
        ;; Register handlers
        (funcall 'neovm--isa-evt-register "click"
                 (lambda (data) (list 'click-handler-1 data)))
        (funcall 'neovm--isa-evt-register "click"
                 (lambda (data) (list 'click-handler-2 (* data 2))))
        (funcall 'neovm--isa-evt-register "keypress"
                 (lambda (data) (list 'key data)))
        (list
          ;; Emit to registered event
          (funcall 'neovm--isa-evt-emit "click" 42)
          ;; Emit to single-handler event
          (funcall 'neovm--isa-evt-emit "keypress" 65)
          ;; Emit to unregistered event (intern-soft returns nil)
          (funcall 'neovm--isa-evt-emit "scroll" 10)
          ;; Handler counts
          (funcall 'neovm--isa-evt-handler-count "click")
          (funcall 'neovm--isa-evt-handler-count "keypress")
          (funcall 'neovm--isa-evt-handler-count "scroll")
          ;; Add more handlers and re-emit
          (funcall 'neovm--isa-evt-register "click"
                   (lambda (data) (list 'click-handler-3 (+ data 100))))
          (funcall 'neovm--isa-evt-handler-count "click")
          (funcall 'neovm--isa-evt-emit "click" 7)))
    (fmakunbound 'neovm--isa-evt-register)
    (fmakunbound 'neovm--isa-evt-emit)
    (fmakunbound 'neovm--isa-evt-handler-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (((click-handler-2 84) (click-handler-1 42)) ((key 65)) nil 2 1 0 neovm--isa-evt-click-9130 3 ((click-handler-3 107) (click-handler-2 14) (click-handler-1 7)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
