//! Oracle parity tests for comprehensive property list operations:
//! plist-get with various key types, plist-put update semantics,
//! plist-member tail semantics, symbol-plist/setplist, get/put on symbols,
//! nested plists, plist iteration patterns, plist merge/update patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// plist-get with various key types and missing keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_get_various_key_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // plist-get uses eq by default for keyword/symbol keys,
    // but also test with integer keys, string keys (which won't match via eq),
    // and nested expressions as values.
    let form = r#"(let ((kw-pl '(:alpha 1 :beta 2 :gamma 3 :delta 4 :epsilon 5))
                        (sym-pl '(foo 10 bar 20 baz 30 quux 40))
                        (int-pl '(0 zero 1 one 2 two 3 three)))
                    (list
                     ;; Keyword keys: first, middle, last
                     (plist-get kw-pl :alpha)
                     (plist-get kw-pl :gamma)
                     (plist-get kw-pl :epsilon)
                     ;; Missing keyword key
                     (plist-get kw-pl :omega)
                     ;; Symbol keys
                     (plist-get sym-pl 'foo)
                     (plist-get sym-pl 'quux)
                     (plist-get sym-pl 'nonexistent)
                     ;; Integer keys (eq for fixnums)
                     (plist-get int-pl 0)
                     (plist-get int-pl 3)
                     (plist-get int-pl 99)
                     ;; Empty plist
                     (plist-get nil :anything)
                     (plist-get '() 'x)
                     ;; Single-entry plist
                     (plist-get '(:only 42) :only)
                     (plist-get '(:only 42) :other)
                     ;; plist-get with default nil for missing
                     (plist-get '(:a 1) :b)))"#;
    let expect =
        expect_test::expect![[r#""OK (1 3 5 nil 10 40 nil zero three nil nil nil 42 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-put returning new plist, updating existing keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_put_new_and_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that plist-put creates new entries and updates existing ones,
    // preserving order and handling edge cases.
    let form = r#"(let* ((pl nil)
                         ;; Build up from nil
                         (pl (plist-put pl :a 1))
                         (pl (plist-put pl :b 2))
                         (pl (plist-put pl :c 3))
                         (snap1 (copy-sequence pl))
                         ;; Update first key
                         (pl (plist-put pl :a 100))
                         (snap2 (copy-sequence pl))
                         ;; Update middle key
                         (pl (plist-put pl :b 200))
                         ;; Update last key
                         (pl (plist-put pl :c 300))
                         (snap3 (copy-sequence pl))
                         ;; Add more after updates
                         (pl (plist-put pl :d 4))
                         (pl (plist-put pl :e 5))
                         ;; Update to nil value (different from removing)
                         (pl (plist-put pl :b nil))
                         ;; Update to different type
                         (pl (plist-put pl :a "string-now"))
                         (pl (plist-put pl :c '(list now))))
                    (list snap1 snap2 snap3
                          ;; Final state reads
                          (plist-get pl :a)
                          (plist-get pl :b)
                          (plist-get pl :c)
                          (plist-get pl :d)
                          (plist-get pl :e)
                          ;; Verify nil value is stored (not removed)
                          (plist-member pl :b)
                          ;; Multiple rapid updates to same key
                          (let* ((q '(:x 1))
                                 (q (plist-put q :x 2))
                                 (q (plist-put q :x 3))
                                 (q (plist-put q :x 4)))
                            (list (plist-get q :x) (length q)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 2 :c 3) (:a 100 :b 2 :c 3) (:a 100 :b 200 :c 300) \"string-now\" nil (list now) 4 5 (:b nil :c (list now) :d 4 :e 5) (4 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-member returning tail of plist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_member_tail_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // plist-member returns the tail starting at the matching key.
    // Test all positions and edge cases.
    let form = r#"(let ((pl '(:w 10 :x 20 :y 30 :z 40)))
                    (list
                     ;; First key: returns entire list
                     (equal (plist-member pl :w) '(:w 10 :x 20 :y 30 :z 40))
                     ;; Second key
                     (equal (plist-member pl :x) '(:x 20 :y 30 :z 40))
                     ;; Last key
                     (equal (plist-member pl :z) '(:z 40))
                     ;; Missing key returns nil
                     (plist-member pl :missing)
                     ;; Can extract value via cadr of result
                     (cadr (plist-member pl :y))
                     ;; Can extract rest via cddr
                     (cddr (plist-member pl :x))
                     ;; Length of returned tail
                     (length (plist-member pl :w))
                     (length (plist-member pl :y))
                     ;; plist-member on empty list
                     (plist-member nil :a)
                     ;; plist-member distinguishes nil value from missing key
                     (let ((pl2 '(:present nil :also-here 42)))
                       (list (plist-member pl2 :present)
                             (plist-member pl2 :missing)
                             ;; present returns non-nil tail, missing returns nil
                             (if (plist-member pl2 :present) 'found 'not-found)
                             (if (plist-member pl2 :missing) 'found 'not-found)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil 30 (:y 30 :z 40) 8 4 nil ((:present nil :also-here 42) nil found not-found))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-plist, setplist, get, put on symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_get_put_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full exercise of symbol property list operations: setplist, symbol-plist,
    // put, get, multiple symbols, property overwrite, clearing.
    let form = r#"(progn
  (unwind-protect
      (let ((results nil))
        ;; Clear any existing properties
        (setplist 'neovm--cptest-sym-a nil)
        (setplist 'neovm--cptest-sym-b nil)

        ;; Verify empty
        (push (symbol-plist 'neovm--cptest-sym-a) results)

        ;; Add properties to sym-a
        (put 'neovm--cptest-sym-a 'prop1 'val1)
        (put 'neovm--cptest-sym-a 'prop2 42)
        (put 'neovm--cptest-sym-a 'prop3 '(a b c))

        ;; Add properties to sym-b
        (put 'neovm--cptest-sym-b 'prop1 'different)
        (put 'neovm--cptest-sym-b 'prop4 "string-val")

        ;; Read from both symbols
        (push (list (get 'neovm--cptest-sym-a 'prop1)
                    (get 'neovm--cptest-sym-a 'prop2)
                    (get 'neovm--cptest-sym-a 'prop3)
                    (get 'neovm--cptest-sym-b 'prop1)
                    (get 'neovm--cptest-sym-b 'prop4))
              results)

        ;; Overwrite property
        (put 'neovm--cptest-sym-a 'prop1 'overwritten)
        (push (get 'neovm--cptest-sym-a 'prop1) results)

        ;; put returns the value
        (push (put 'neovm--cptest-sym-a 'ret-test 999) results)

        ;; Verify symbol-plist returns the full plist
        (let ((pl (symbol-plist 'neovm--cptest-sym-a)))
          (push (plist-get pl 'prop2) results))

        ;; setplist replaces everything
        (setplist 'neovm--cptest-sym-a '(replaced t))
        (push (list (get 'neovm--cptest-sym-a 'prop1)
                    (get 'neovm--cptest-sym-a 'replaced))
              results)

        ;; Clear with setplist nil
        (setplist 'neovm--cptest-sym-a nil)
        (push (symbol-plist 'neovm--cptest-sym-a) results)

        ;; get on missing property returns nil
        (push (get 'neovm--cptest-sym-a 'anything) results)

        (nreverse results))
    (setplist 'neovm--cptest-sym-a nil)
    (setplist 'neovm--cptest-sym-b nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (val1 42 (a b c) different \"string-val\") overwritten 999 42 (nil t) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist as lightweight key-value store with functional operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_as_kv_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use plists as lightweight key-value stores with helper functions
    // for batch operations, filtering, and transformation.
    let form = r#"(let ((kv-set
                         (lambda (store key val)
                           (plist-put store key val)))
                        (kv-get
                         (lambda (store key default)
                           (if (plist-member store key)
                               (plist-get store key)
                             default)))
                        (kv-keys
                         (lambda (store)
                           (let ((keys nil) (rest store))
                             (while rest
                               (setq keys (cons (car rest) keys))
                               (setq rest (cddr rest)))
                             (nreverse keys))))
                        (kv-values
                         (lambda (store)
                           (let ((vals nil) (rest store))
                             (while rest
                               (setq vals (cons (cadr rest) vals))
                               (setq rest (cddr rest)))
                             (nreverse vals))))
                        (kv-count
                         (lambda (store)
                           (/ (length store) 2)))
                        (kv-map-values
                         (lambda (store fn)
                           (let ((result nil) (rest store))
                             (while rest
                               (setq result (plist-put result
                                                       (car rest)
                                                       (funcall fn (cadr rest))))
                               (setq rest (cddr rest)))
                             result)))
                        (kv-filter
                         (lambda (store pred)
                           (let ((result nil) (rest store))
                             (while rest
                               (when (funcall pred (car rest) (cadr rest))
                                 (setq result (plist-put result
                                                         (car rest)
                                                         (cadr rest))))
                               (setq rest (cddr rest)))
                             result))))
                    (let* ((store nil)
                           (store (funcall kv-set store :name "Alice"))
                           (store (funcall kv-set store :age 30))
                           (store (funcall kv-set store :score 85))
                           (store (funcall kv-set store :level 3)))
                      (list
                       ;; Basic operations
                       (funcall kv-get store :name "default")
                       (funcall kv-get store :missing "fallback")
                       (funcall kv-keys store)
                       (funcall kv-values store)
                       (funcall kv-count store)
                       ;; Map: double all numeric values
                       (let ((nums '(:x 10 :y 20 :z 30)))
                         (funcall kv-map-values nums (lambda (v) (* v 2))))
                       ;; Filter: keep only values > 20
                       (let ((nums '(:a 5 :b 25 :c 10 :d 50 :e 30)))
                         (funcall kv-filter nums
                                  (lambda (_k v) (> v 20)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" \"fallback\" (:name :age :score :level) (\"Alice\" 30 85 3) 4 (:x 20 :y 40 :z 60) (:b 25 :d 50 :e 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested plists (plist values that are plists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_plists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Plists containing plists as values, with deep access and update patterns.
    let form = r#"(let ((deep-get
                         (lambda (plist path)
                           "Get value from nested plist using a path (list of keys)."
                           (let ((current plist))
                             (dolist (key path)
                               (setq current (plist-get current key)))
                             current)))
                        (deep-set
                         (lambda (plist path val)
                           "Set value in nested plist, creating intermediate plists."
                           (if (= (length path) 1)
                               (plist-put plist (car path) val)
                             (let ((child (plist-get plist (car path))))
                               (plist-put plist (car path)
                                          (funcall 'neovm--cptest-deep-set
                                                   (or child nil)
                                                   (cdr path) val)))))))
                    ;; Build a nested structure
                    (let ((config '(:database (:host "localhost"
                                    :port 5432
                                    :credentials (:user "admin" :pass "secret"))
                                   :cache (:enabled t :ttl 300)
                                   :logging (:level :info :file "/var/log/app.log"))))
                      (list
                       ;; Deep access
                       (funcall deep-get config '(:database :host))
                       (funcall deep-get config '(:database :port))
                       (funcall deep-get config '(:database :credentials :user))
                       (funcall deep-get config '(:database :credentials :pass))
                       (funcall deep-get config '(:cache :enabled))
                       (funcall deep-get config '(:cache :ttl))
                       (funcall deep-get config '(:logging :level))
                       ;; Missing nested path returns nil
                       (funcall deep-get config '(:database :nonexistent))
                       (funcall deep-get config '(:missing :anything))
                       ;; Verify structure
                       (plist-get (plist-get config :database) :host)
                       ;; Nested plist is itself a valid plist
                       (let ((creds (plist-get (plist-get config :database) :credentials)))
                         (list (plist-get creds :user)
                               (plist-get creds :pass)
                               (plist-member creds :user))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"localhost\" 5432 \"admin\" \"secret\" t 300 :info nil nil \"localhost\" (\"admin\" \"secret\" (:user \"admin\" :pass \"secret\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist iteration patterns (using while loop on cddr)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_iteration_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Various iteration patterns over plists: while/cddr, collecting,
    // reducing, and transforming.
    let form = r#"(let ((pl '(:name "Bob" :age 25 :city "NYC" :score 92 :rank 3)))
                    (list
                     ;; Collect all key-value pairs as alist
                     (let ((pairs nil) (rest pl))
                       (while rest
                         (setq pairs (cons (cons (car rest) (cadr rest)) pairs))
                         (setq rest (cddr rest)))
                       (nreverse pairs))

                     ;; Count properties
                     (let ((count 0) (rest pl))
                       (while rest
                         (setq count (1+ count))
                         (setq rest (cddr rest)))
                       count)

                     ;; Find all keys with numeric values
                     (let ((numeric-keys nil) (rest pl))
                       (while rest
                         (when (numberp (cadr rest))
                           (setq numeric-keys (cons (car rest) numeric-keys)))
                         (setq rest (cddr rest)))
                       (nreverse numeric-keys))

                     ;; Sum all numeric values
                     (let ((sum 0) (rest pl))
                       (while rest
                         (when (numberp (cadr rest))
                           (setq sum (+ sum (cadr rest))))
                         (setq rest (cddr rest)))
                       sum)

                     ;; Build string description "key=val, key=val, ..."
                     (let ((parts nil) (rest pl))
                       (while rest
                         (setq parts
                               (cons (format "%s=%S" (car rest) (cadr rest))
                                     parts))
                         (setq rest (cddr rest)))
                       (mapconcat #'identity (nreverse parts) ", "))

                     ;; Transform: prefix all keys with "my-"
                     (let ((result nil) (rest pl))
                       (while rest
                         (let ((new-key (intern (concat "my-"
                                                        (substring (symbol-name (car rest)) 1)))))
                           (setq result (plist-put result new-key (cadr rest))))
                         (setq rest (cddr rest)))
                       result)

                     ;; Iterate empty plist (no iterations)
                     (let ((count 0) (rest nil))
                       (while rest
                         (setq count (1+ count))
                         (setq rest (cddr rest)))
                       count)))"#;
    let expect = expect_test::expect![[
        r#""OK (((:name . \"Bob\") (:age . 25) (:city . \"NYC\") (:score . 92) (:rank . 3)) 5 (:age :score :rank) 120 \":name=\\\"Bob\\\", :age=25, :city=\\\"NYC\\\", :score=92, :rank=3\" (my-name \"Bob\" my-age 25 my-city \"NYC\" my-score 92 my-rank 3) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist merge/update patterns with conflict resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_merge_update_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Advanced merge patterns: merge-with (custom conflict resolution),
    // selective update, plist-remove simulation, plist-equal.
    let form = r#"(let ((plist-merge-with
                         (lambda (resolver base overlay)
                           "Merge overlay into base. When both have same key,
                            call (resolver base-val overlay-val)."
                           (let ((result (copy-sequence base))
                                 (rest overlay))
                             (while rest
                               (let ((key (car rest))
                                     (new-val (cadr rest)))
                                 (if (plist-member result key)
                                     (setq result
                                           (plist-put result key
                                                      (funcall resolver
                                                               (plist-get result key)
                                                               new-val)))
                                   (setq result (plist-put result key new-val))))
                               (setq rest (cddr rest)))
                             result)))
                        (plist-remove
                         (lambda (plist key)
                           "Return new plist without the given key."
                           (let ((result nil) (rest plist))
                             (while rest
                               (unless (eq (car rest) key)
                                 (setq result (plist-put result
                                                         (car rest)
                                                         (cadr rest))))
                               (setq rest (cddr rest)))
                             result)))
                        (plist-equal-p
                         (lambda (a b)
                           "Check if two plists have same key-value pairs (order-independent)."
                           (and (= (length a) (length b))
                                (let ((equal t) (rest a))
                                  (while (and rest equal)
                                    (unless (and (plist-member b (car rest))
                                                 (equal (plist-get b (car rest))
                                                        (cadr rest)))
                                      (setq equal nil))
                                    (setq rest (cddr rest)))
                                  equal))))
                        (plist-select
                         (lambda (plist keys)
                           "Return plist with only the specified keys."
                           (let ((result nil))
                             (dolist (key keys)
                               (when (plist-member plist key)
                                 (setq result (plist-put result key
                                                         (plist-get plist key)))))
                             result))))
                    (let ((p1 '(:a 1 :b 2 :c 3 :d 4))
                          (p2 '(:b 20 :c 30 :e 50)))
                      (list
                       ;; Merge with addition for conflicts
                       (funcall plist-merge-with
                                (lambda (old new) (+ old new))
                                p1 p2)
                       ;; Merge keeping old value on conflict
                       (funcall plist-merge-with
                                (lambda (old _new) old)
                                p1 p2)
                       ;; Merge keeping new value on conflict
                       (funcall plist-merge-with
                                (lambda (_old new) new)
                                p1 p2)
                       ;; Merge building pair on conflict
                       (funcall plist-merge-with
                                (lambda (old new) (list old new))
                                p1 p2)
                       ;; Remove key
                       (funcall plist-remove '(:x 1 :y 2 :z 3) :y)
                       ;; Remove missing key (no change)
                       (funcall plist-remove '(:x 1 :y 2) :w)
                       ;; Equality checks
                       (funcall plist-equal-p '(:a 1 :b 2) '(:b 2 :a 1))
                       (funcall plist-equal-p '(:a 1 :b 2) '(:a 1 :b 3))
                       (funcall plist-equal-p '(:a 1) '(:a 1 :b 2))
                       ;; Select subset of keys
                       (funcall plist-select '(:a 1 :b 2 :c 3 :d 4) '(:b :d))
                       (funcall plist-select '(:a 1 :b 2) '(:x :y)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 22 :c 33 :d 4 :e 50) (:a 1 :b 2 :c 3 :d 4 :e 50) (:a 1 :b 20 :c 30 :d 4 :e 50) (:a 1 :b (2 20) :c (3 30) :d 4 :e 50) (:x 1 :z 3) (:x 1 :y 2) t nil nil (:b 2 :d 4) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-get with custom comparison predicate (Emacs 29+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_get_with_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // plist-get with optional PREDICATE argument (Emacs 29+).
    // By default uses eq, but can use equal or string= for string keys.
    let form = r#"(list
                   ;; Default eq: keywords match
                   (plist-get '(:a 1 :b 2) :a)
                   ;; With equal predicate: string keys work
                   (plist-get '("name" "Alice" "age" 30) "name" #'equal)
                   (plist-get '("name" "Alice" "age" 30) "age" #'equal)
                   (plist-get '("name" "Alice" "age" 30) "missing" #'equal)
                   ;; With equal: list keys
                   (plist-get '((1 2) "first" (3 4) "second") '(3 4) #'equal)
                   ;; With string=: string keys
                   (plist-get '("x" 10 "y" 20) "x" #'string=)
                   (plist-get '("x" 10 "y" 20) "y" #'string=)
                   ;; plist-member with predicate
                   (plist-member '("a" 1 "b" 2) "b" #'equal)
                   ;; plist-put with predicate
                   (let ((pl (plist-put '("x" 1) "x" 99 #'equal)))
                     (plist-get pl "x" #'equal))
                   ;; Mix: build plist with string keys via equal
                   (let* ((pl nil)
                          (pl (plist-put pl "host" "localhost" #'equal))
                          (pl (plist-put pl "port" 8080 #'equal))
                          (pl (plist-put pl "host" "prod.com" #'equal)))
                     (list (plist-get pl "host" #'equal)
                           (plist-get pl "port" #'equal))))"#;
    let expect = expect_test::expect![[
        r#""OK (1 \"Alice\" 30 nil \"second\" 10 20 (\"b\" 2) 99 (\"prod.com\" 8080))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist-based event system with handlers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_event_handler_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an event dispatch system using plists for handler registration
    // and event data.
    let form = r#"(let ((make-dispatcher
                         (lambda ()
                           "Create a new event dispatcher (plist of event-name -> handler-list)."
                           nil))
                        (register-handler
                         (lambda (dispatcher event handler)
                           "Add a handler for an event. Returns new dispatcher."
                           (let ((handlers (plist-get dispatcher event)))
                             (plist-put dispatcher event
                                        (append handlers (list handler))))))
                        (dispatch
                         (lambda (dispatcher event data)
                           "Dispatch event to all registered handlers. Collect results."
                           (let ((handlers (plist-get dispatcher event))
                                 (results nil))
                             (dolist (h handlers)
                               (setq results (cons (funcall h data) results)))
                             (nreverse results))))
                        (handler-count
                         (lambda (dispatcher event)
                           (length (plist-get dispatcher event)))))
                    (let* ((d (funcall make-dispatcher))
                           ;; Register handlers for :click
                           (d (funcall register-handler d :click
                                       (lambda (data) (format "logged: %S" data))))
                           (d (funcall register-handler d :click
                                       (lambda (data) (plist-get data :x))))
                           (d (funcall register-handler d :click
                                       (lambda (data) (plist-get data :y))))
                           ;; Register handler for :hover
                           (d (funcall register-handler d :hover
                                       (lambda (data) (format "hover at %s" data))))
                           ;; Register multiple for :keypress
                           (d (funcall register-handler d :keypress
                                       (lambda (data) (upcase data))))
                           (d (funcall register-handler d :keypress
                                       (lambda (data) (length data)))))
                      (list
                       ;; Dispatch click event
                       (funcall dispatch d :click '(:x 100 :y 200))
                       ;; Dispatch hover
                       (funcall dispatch d :hover "button-1")
                       ;; Dispatch keypress
                       (funcall dispatch d :keypress "hello")
                       ;; Dispatch unregistered event
                       (funcall dispatch d :scroll '(:delta 5))
                       ;; Handler counts
                       (funcall handler-count d :click)
                       (funcall handler-count d :hover)
                       (funcall handler-count d :keypress)
                       (funcall handler-count d :scroll))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"logged: (:x 100 :y 200)\" 100 200) (\"hover at button-1\") (\"HELLO\" 5) nil 3 1 2 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
