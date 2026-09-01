//! Oracle parity tests for advanced property list operations:
//! all value types, create vs update, plist-member tail semantics,
//! symbol-plist / setplist, put / get shorthand, record systems,
//! merging/diffing, configuration with inheritance.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// plist-get / plist-put / plist-member with all value types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_all_value_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Store and retrieve every value type in a plist
    let form = r#"(let* ((pl nil)
                         (pl (plist-put pl :int 42))
                         (pl (plist-put pl :float 3.14))
                         (pl (plist-put pl :str "hello"))
                         (pl (plist-put pl :sym 'world))
                         (pl (plist-put pl :kw :keyword-val))
                         (pl (plist-put pl :bool t))
                         (pl (plist-put pl :null nil))
                         (pl (plist-put pl :list '(1 2 3)))
                         (pl (plist-put pl :vec [4 5 6]))
                         (pl (plist-put pl :pair '(a . b)))
                         (pl (plist-put pl :nested '(:x 1 :y 2))))
                    (list (plist-get pl :int)
                          (plist-get pl :float)
                          (plist-get pl :str)
                          (plist-get pl :sym)
                          (plist-get pl :kw)
                          (plist-get pl :bool)
                          (plist-get pl :null)
                          (plist-get pl :list)
                          (plist-get pl :vec)
                          (plist-get pl :pair)
                          (plist-get pl :nested)))"#;
    let expect = expect_test::expect![[
        r#""OK (42 3.14 \"hello\" world :keyword-val t nil (1 2 3) [4 5 6] (a . b) (:x 1 :y 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-put: creating new vs updating existing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_create_vs_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track the evolution of a plist through creates and updates
    let form = r#"(let* ((pl nil)
                         ;; Create three entries
                         (pl (plist-put pl :a 1))
                         (snap1 (copy-sequence pl))
                         (pl (plist-put pl :b 2))
                         (snap2 (copy-sequence pl))
                         (pl (plist-put pl :c 3))
                         (snap3 (copy-sequence pl))
                         ;; Update existing entries
                         (pl (plist-put pl :a 100))
                         (snap4 (copy-sequence pl))
                         (pl (plist-put pl :b 200))
                         (pl (plist-put pl :c 300))
                         ;; Update to different types
                         (pl (plist-put pl :a "now-a-string"))
                         (pl (plist-put pl :b '(now a list)))
                         (pl (plist-put pl :c nil)))
                    (list snap1
                          snap3
                          (plist-get pl :a)
                          (plist-get pl :b)
                          (plist-get pl :c)
                          (length pl)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1) (:a 1 :b 2 :c 3) \"now-a-string\" (now a list) nil 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-member returning the tail (not just the value)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_member_tail_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // plist-member returns the tail starting at the key
    let form = r#"(let ((pl '(:a 1 :b 2 :c 3 :d 4)))
                    (list
                     ;; Returns (:a 1 :b 2 :c 3 :d 4)
                     (plist-member pl :a)
                     ;; Returns (:c 3 :d 4)
                     (plist-member pl :c)
                     ;; Returns (:d 4)
                     (plist-member pl :d)
                     ;; Returns nil for missing
                     (plist-member pl :z)
                     ;; Can extract value from tail via cadr
                     (cadr (plist-member pl :b))
                     ;; Can get remaining keys from tail
                     (let ((tail (plist-member pl :b)))
                       (length tail))))"#;
    let expect =
        expect_test::expect![[r#""OK ((:a 1 :b 2 :c 3 :d 4) (:c 3 :d 4) (:d 4) nil 2 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-plist / setplist for per-symbol storage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_full_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full lifecycle: empty -> populate -> read -> replace -> clear
    let form = r#"(progn
  (unwind-protect
      (let ((results nil))
        ;; Start empty
        (setplist 'neovm--test-sym-pl nil)
        (setq results (cons (symbol-plist 'neovm--test-sym-pl) results))
        ;; Add properties via put
        (put 'neovm--test-sym-pl 'color 'red)
        (put 'neovm--test-sym-pl 'size 42)
        (put 'neovm--test-sym-pl 'label "test")
        ;; Read back via get
        (setq results (cons (list (get 'neovm--test-sym-pl 'color)
                                  (get 'neovm--test-sym-pl 'size)
                                  (get 'neovm--test-sym-pl 'label))
                            results))
        ;; Update existing property
        (put 'neovm--test-sym-pl 'color 'blue)
        (setq results (cons (get 'neovm--test-sym-pl 'color) results))
        ;; Replace entire plist via setplist
        (setplist 'neovm--test-sym-pl '(new-key new-val))
        (setq results (cons (list (get 'neovm--test-sym-pl 'color)
                                  (get 'neovm--test-sym-pl 'new-key))
                            results))
        ;; Get missing property returns nil
        (setq results (cons (get 'neovm--test-sym-pl 'nonexistent) results))
        (nreverse results))
    (setplist 'neovm--test-sym-pl nil)))"#;
    let expect = expect_test::expect![[r#""OK (nil (red 42 \"test\") blue (nil new-val) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// put / get as symbol property shorthand
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_get_shorthand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // put/get are shorthands for (plist-put (symbol-plist s) ...) etc.
    let form = r#"(progn
  (unwind-protect
      (progn
        ;; Use put to store, get to retrieve
        (put 'neovm--test-pg 'x 10)
        (put 'neovm--test-pg 'y 20)
        (put 'neovm--test-pg 'z 30)
        ;; put returns the value
        (let ((ret (put 'neovm--test-pg 'w 99)))
          (list ret
                (get 'neovm--test-pg 'x)
                (get 'neovm--test-pg 'y)
                (get 'neovm--test-pg 'z)
                (get 'neovm--test-pg 'w)
                ;; Verify consistency with symbol-plist
                (plist-get (symbol-plist 'neovm--test-pg) 'x)
                ;; Missing key
                (get 'neovm--test-pg 'missing))))
    (setplist 'neovm--test-pg nil)))"#;
    let expect = expect_test::expect![[r#""OK (99 10 20 30 99 10 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist-based record system with validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_record_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define a record schema and validate instances
    let form = r#"(let ((schema '((:name  . stringp)
                                   (:age   . integerp)
                                   (:score . numberp)
                                   (:tags  . listp)))
                        (validate
                         (lambda (schema record)
                           (let ((errors nil)
                                 (valid t))
                             (dolist (field schema)
                               (let ((key (car field))
                                     (pred (cdr field)))
                                 (let ((val (plist-get record key)))
                                   (unless (funcall pred val)
                                     (setq valid nil)
                                     (setq errors
                                           (cons (format "%s: expected %s, got %S"
                                                         key pred val)
                                                 errors))))))
                             (list valid (nreverse errors)))))
                        (make-record
                         (lambda (name age score tags)
                           (list :name name :age age :score score :tags tags)))
                        (update-field
                         (lambda (record key val)
                           (plist-put (copy-sequence record) key val))))
                    ;; Test valid record
                    (let ((r1 (funcall make-record "Alice" 30 95.5 '(fast smart))))
                      (let ((v1 (funcall validate schema r1))
                            ;; Test invalid record
                            (r2 (funcall update-field r1 :age "not-a-number"))
                            ;; Test record with wrong type for multiple fields
                            (r3 (list :name 42 :age "old" :score "high" :tags "none")))
                        (list v1
                              (funcall validate schema r2)
                              (funcall validate schema r3)
                              ;; Read fields from valid record
                              (plist-get r1 :name)
                              (plist-get r1 :tags)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil) (nil (\":age: expected integerp, got \\\"not-a-number\\\"\")) (nil (\":name: expected stringp, got 42\" \":age: expected integerp, got \\\"old\\\"\" \":score: expected numberp, got \\\"high\\\"\" \":tags: expected listp, got \\\"none\\\"\")) \"Alice\" (fast smart))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist merging and diffing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_merge_and_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deep merge, diff, and intersection of plists
    let form = r#"(let ((plist-keys
                         (lambda (pl)
                           (let ((keys nil) (remaining pl))
                             (while remaining
                               (setq keys (cons (car remaining) keys))
                               (setq remaining (cddr remaining)))
                             (nreverse keys))))
                        (plist-merge
                         (lambda (base overlay)
                           (let ((result (copy-sequence base))
                                 (remaining overlay))
                             (while remaining
                               (setq result (plist-put result
                                                       (car remaining)
                                                       (cadr remaining)))
                               (setq remaining (cddr remaining)))
                             result)))
                        (plist-diff
                         (lambda (a b)
                           (let ((only-a nil) (only-b nil) (changed nil)
                                 (remaining-a a))
                             ;; Check keys in a
                             (while remaining-a
                               (let ((k (car remaining-a))
                                     (v (cadr remaining-a)))
                                 (if (not (plist-member b k))
                                     (setq only-a (cons (cons k v) only-a))
                                   (unless (equal v (plist-get b k))
                                     (setq changed
                                           (cons (list k v (plist-get b k))
                                                 changed)))))
                               (setq remaining-a (cddr remaining-a)))
                             ;; Check keys only in b
                             (let ((remaining-b b))
                               (while remaining-b
                                 (unless (plist-member a (car remaining-b))
                                   (setq only-b
                                         (cons (cons (car remaining-b)
                                                     (cadr remaining-b))
                                               only-b)))
                                 (setq remaining-b (cddr remaining-b))))
                             (list (nreverse only-a)
                                   (nreverse only-b)
                                   (nreverse changed)))))
                        (plist-intersect
                         (lambda (a b)
                           (let ((result nil)
                                 (remaining a))
                             (while remaining
                               (let ((k (car remaining)))
                                 (when (plist-member b k)
                                   (setq result
                                         (cons (cadr remaining)
                                               (cons k result)))))
                               (setq remaining (cddr remaining)))
                             (nreverse result)))))
                    (let ((p1 '(:a 1 :b 2 :c 3 :d 4))
                          (p2 '(:b 20 :c 3 :e 5 :f 6)))
                      (list
                       ;; Merge result
                       (funcall plist-merge p1 p2)
                       ;; Diff result: (only-in-a only-in-b changed)
                       (funcall plist-diff p1 p2)
                       ;; Intersection (common keys, values from a)
                       (funcall plist-intersect p1 p2)
                       ;; Keys of p1
                       (funcall plist-keys p1))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 20 :c 3 :d 4 :e 5 :f 6) (((:a . 1) (:d . 4)) ((:e . 5) (:f . 6)) ((:b 2 20))) (:b 2 :c 3) (:a :b :c :d))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist-based configuration with inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_config_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Configuration system with layers: defaults -> env -> user -> override
    let form = r#"(let ((merge-configs
                         (lambda (&rest configs)
                           (let ((result nil))
                             (dolist (config configs)
                               (let ((remaining config))
                                 (while remaining
                                   (setq result (plist-put result
                                                           (car remaining)
                                                           (cadr remaining)))
                                   (setq remaining (cddr remaining)))))
                             result)))
                        (config-get
                         (lambda (config key &optional default)
                           (if (plist-member config key)
                               (plist-get config key)
                             default)))
                        (config-has
                         (lambda (config key)
                           (if (plist-member config key) t nil))))
                    ;; Define config layers
                    (let* ((defaults '(:host "localhost" :port 8080
                                       :debug nil :timeout 30
                                       :max-retries 3 :log-level info))
                           (env-config '(:port 3000 :log-level debug))
                           (user-config '(:timeout 60 :debug t))
                           (override '(:host "prod.example.com" :debug nil))
                           ;; Merge in priority order
                           (final (funcall merge-configs
                                           defaults env-config
                                           user-config override)))
                      (list
                       ;; Final config values
                       (funcall config-get final :host nil)
                       (funcall config-get final :port nil)
                       (funcall config-get final :debug nil)
                       (funcall config-get final :timeout nil)
                       (funcall config-get final :max-retries nil)
                       (funcall config-get final :log-level nil)
                       ;; Missing key with default
                       (funcall config-get final :missing "fallback")
                       ;; Has checks
                       (funcall config-has final :host)
                       (funcall config-has final :nonexistent))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"prod.example.com\" 3000 nil 60 3 debug \"fallback\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
