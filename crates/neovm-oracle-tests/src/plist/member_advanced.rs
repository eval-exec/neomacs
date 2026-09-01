//! Oracle parity tests for `plist-member`, `plist-get`, `plist-put` with
//! complex patterns: tail semantics, nil vs missing key distinction,
//! multi-level nested plists, record systems, merge/diff operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// plist-member returns the tail starting from the key
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_member_tail_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pl '(:a 1 :b 2 :c 3 :d 4 :e 5)))
  (list
    ;; Returns tail from first key
    (plist-member pl :a)
    ;; Returns tail from middle key
    (plist-member pl :c)
    ;; Returns tail from last key
    (plist-member pl :e)
    ;; Returns nil for missing key
    (plist-member pl :z)
    ;; cadr of result gives the value
    (cadr (plist-member pl :b))
    ;; cddr of result gives remaining plist
    (cddr (plist-member pl :b))
    ;; Can chain: get value after :c by taking tail then cadr
    (cadr (plist-member pl :c))
    ;; Length of tail from various positions
    (length (plist-member pl :a))
    (length (plist-member pl :d))
    ;; plist-member on empty plist
    (plist-member nil :a)
    ;; plist-member on single-pair plist
    (plist-member '(:x 42) :x)
    (plist-member '(:x 42) :y)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 2 :c 3 :d 4 :e 5) (:c 3 :d 4 :e 5) (:e 5) nil 2 (:c 3 :d 4 :e 5) 3 10 4 nil (:x 42) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-get basic and with various key types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_get_various_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Keyword keys (most common)
  (plist-get '(:name "Alice" :age 30) :name)
  (plist-get '(:name "Alice" :age 30) :age)
  (plist-get '(:name "Alice" :age 30) :missing)
  ;; Symbol keys (compared with eq)
  (plist-get '(name "Bob" age 25) 'name)
  (plist-get '(name "Bob" age 25) 'age)
  ;; Integer keys (compared with eq, works for small fixnums)
  (plist-get '(1 "one" 2 "two" 3 "three") 1)
  (plist-get '(1 "one" 2 "two" 3 "three") 3)
  ;; Mixed key types
  (plist-get '(:a 1 b 2 :c 3) :a)
  (plist-get '(:a 1 b 2 :c 3) 'b)
  ;; Duplicate keys: plist-get returns first occurrence
  (plist-get '(:x 1 :y 2 :x 3) :x)
  ;; Value is nil
  (plist-get '(:present nil :other 1) :present)
  ;; plist-get on nil plist
  (plist-get nil :anything))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 30 nil \"Bob\" 25 \"one\" \"three\" 1 2 1 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nil values vs missing keys: distinction via plist-member
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_nil_vs_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // plist-get returns nil for BOTH missing keys and keys with nil value.
    // plist-member distinguishes: nil for missing, non-nil for present-with-nil-value.
    let form = r#"(let ((pl '(:exists-nil nil :exists-val 42 :exists-zero 0 :exists-empty "")))
  (list
    ;; plist-get cannot distinguish nil-value from missing
    (plist-get pl :exists-nil)     ;; nil
    (plist-get pl :missing)        ;; nil
    (eq (plist-get pl :exists-nil) (plist-get pl :missing)) ;; t - same!

    ;; plist-member CAN distinguish
    (plist-member pl :exists-nil)  ;; (:exists-nil nil :exists-val 42 ...) - truthy
    (plist-member pl :missing)     ;; nil

    ;; Build a safe-get function using plist-member
    (let ((safe-get
           (lambda (pl key default)
             (let ((tail (plist-member pl key)))
               (if tail (cadr tail) default)))))
      (list
       ;; Key present with nil value: returns nil, NOT default
       (funcall safe-get pl :exists-nil 'default-val)
       ;; Key missing: returns default
       (funcall safe-get pl :missing 'default-val)
       ;; Key present with value: returns value
       (funcall safe-get pl :exists-val 'default-val)
       ;; Key present with zero: returns 0
       (funcall safe-get pl :exists-zero 'default-val)
       ;; Key present with empty string: returns ""
       (funcall safe-get pl :exists-empty 'default-val)))

    ;; has-key predicate using plist-member
    (let ((has-key (lambda (pl key) (if (plist-member pl key) t nil))))
      (list
       (funcall has-key pl :exists-nil)
       (funcall has-key pl :exists-val)
       (funcall has-key pl :missing)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil t (:exists-nil nil :exists-val 42 :exists-zero 0 :exists-empty \"\") nil (nil default-val 42 0 \"\") (t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-put creates new or updates existing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_put_create_and_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Create from nil
  (plist-put nil :a 1)
  ;; Add to existing
  (plist-put '(:a 1) :b 2)
  ;; Update existing key
  (plist-put '(:a 1 :b 2) :a 100)
  ;; Chain of puts
  (let* ((pl nil)
         (pl (plist-put pl :x 10))
         (pl (plist-put pl :y 20))
         (pl (plist-put pl :z 30))
         (pl (plist-put pl :x 99)))
    (list pl
          (plist-get pl :x)
          (plist-get pl :y)
          (plist-get pl :z)
          (length pl)))
  ;; Update value to nil
  (let* ((pl '(:key "value"))
         (pl (plist-put pl :key nil)))
    (list pl (plist-get pl :key) (plist-member pl :key)))
  ;; Update value type
  (let* ((pl '(:data 42))
         (pl (plist-put pl :data "now a string"))
         (pl (plist-put pl :data '(now a list))))
    pl)
  ;; Put with symbol keys
  (plist-put '(a 1 b 2) 'a 100))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1) (:a 1 :b 2) (:a 100 :b 2) ((:x 99 :y 20 :z 30) 99 20 30 6) ((:key nil) nil (:key nil)) (:data (now a list)) (a 100 b 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-level nested plists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_nested_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Nested plist access and update helpers
  (fset 'neovm--plist-get-in
    (lambda (pl keys)
      (let ((current pl))
        (dolist (k keys)
          (setq current (plist-get current k)))
        current)))

  (fset 'neovm--plist-put-in
    (lambda (pl keys val)
      (if (= (length keys) 1)
          (plist-put (copy-sequence pl) (car keys) val)
        (let ((child (plist-get pl (car keys))))
          (plist-put (copy-sequence pl)
                     (car keys)
                     (funcall 'neovm--plist-put-in
                              (or child nil) (cdr keys) val))))))

  (fset 'neovm--plist-keys
    (lambda (pl)
      (let ((keys nil) (rest pl))
        (while rest
          (setq keys (cons (car rest) keys))
          (setq rest (cddr rest)))
        (nreverse keys))))

  (unwind-protect
      (let ((config '(:database (:host "localhost" :port 5432
                       :credentials (:user "admin" :password "secret"))
                      :server (:port 8080 :workers 4
                       :logging (:level "info" :file "/var/log/app.log"))
                      :features (:cache t :auth t :metrics nil))))
        (list
         ;; Deep access
         (funcall 'neovm--plist-get-in config '(:database :host))
         (funcall 'neovm--plist-get-in config '(:database :port))
         (funcall 'neovm--plist-get-in config '(:database :credentials :user))
         (funcall 'neovm--plist-get-in config '(:server :logging :level))
         (funcall 'neovm--plist-get-in config '(:features :cache))
         (funcall 'neovm--plist-get-in config '(:features :metrics))
         ;; Deep update
         (let ((updated (funcall 'neovm--plist-put-in config
                                 '(:database :port) 3306)))
           (funcall 'neovm--plist-get-in updated '(:database :port)))
         (let ((updated (funcall 'neovm--plist-put-in config
                                 '(:server :logging :level) "debug")))
           (funcall 'neovm--plist-get-in updated '(:server :logging :level)))
         ;; Top-level keys
         (funcall 'neovm--plist-keys config)
         ;; Nested keys
         (funcall 'neovm--plist-keys (plist-get config :database))
         (funcall 'neovm--plist-keys (plist-get config :features))))
    (fmakunbound 'neovm--plist-get-in)
    (fmakunbound 'neovm--plist-put-in)
    (fmakunbound 'neovm--plist-keys)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"localhost\" 5432 \"admin\" \"info\" t nil 3306 \"debug\" (:database :server :features) (:host :port :credentials) (:cache :auth :metrics))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist as a simple record system with constructors and accessors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_record_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Define a record type with schema, constructor, and validators
  (fset 'neovm--make-person
    (lambda (name age email)
      (list :type 'person :name name :age age :email email)))

  (fset 'neovm--make-address
    (lambda (street city zip)
      (list :type 'address :street street :city city :zip zip)))

  (fset 'neovm--record-type
    (lambda (rec) (plist-get rec :type)))

  (fset 'neovm--record-valid-p
    (lambda (rec)
      (and (listp rec)
           (plist-member rec :type)
           (symbolp (plist-get rec :type)))))

  (fset 'neovm--record-merge
    (lambda (r1 r2)
      (let ((result (copy-sequence r1))
            (rest r2))
        (while rest
          (when (not (eq (car rest) :type))
            (setq result (plist-put result (car rest) (cadr rest))))
          (setq rest (cddr rest)))
        result)))

  (fset 'neovm--record-to-alist
    (lambda (rec)
      (let ((result nil) (rest rec))
        (while rest
          (setq result (cons (cons (car rest) (cadr rest)) result))
          (setq rest (cddr rest)))
        (nreverse result))))

  (unwind-protect
      (let* ((p1 (funcall 'neovm--make-person "Alice" 30 "alice@example.com"))
             (p2 (funcall 'neovm--make-person "Bob" 25 "bob@example.com"))
             (a1 (funcall 'neovm--make-address "123 Main St" "Springfield" "62701")))
        (list
         ;; Access fields
         (plist-get p1 :name)
         (plist-get p1 :age)
         (plist-get a1 :city)
         ;; Type checking
         (funcall 'neovm--record-type p1)
         (funcall 'neovm--record-type a1)
         (funcall 'neovm--record-valid-p p1)
         (funcall 'neovm--record-valid-p '(not a record))
         ;; Merge (update person with new fields)
         (let ((updated (funcall 'neovm--record-merge p1
                                 '(:age 31 :phone "555-1234"))))
           (list (plist-get updated :type)
                 (plist-get updated :name)
                 (plist-get updated :age)
                 (plist-get updated :phone)))
         ;; Convert to alist
         (funcall 'neovm--record-to-alist
                  (funcall 'neovm--make-person "Charlie" 35 "c@c.com"))
         ;; Embedded records
         (let ((contact (list :type 'contact
                              :person p1
                              :address a1)))
           (list (plist-get (plist-get contact :person) :name)
                 (plist-get (plist-get contact :address) :city)))))
    (fmakunbound 'neovm--make-person)
    (fmakunbound 'neovm--make-address)
    (fmakunbound 'neovm--record-type)
    (fmakunbound 'neovm--record-valid-p)
    (fmakunbound 'neovm--record-merge)
    (fmakunbound 'neovm--record-to-alist)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 30 \"Springfield\" person address t nil (person \"Alice\" 31 \"555-1234\") ((:type . person) (:name . \"Charlie\") (:age . 35) (:email . \"c@c.com\")) (\"Alice\" \"Springfield\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: plist merge/diff operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_merge_diff_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--plist-merge
    (lambda (base overlay)
      (let ((result (copy-sequence base))
            (rest overlay))
        (while rest
          (setq result (plist-put result (car rest) (cadr rest)))
          (setq rest (cddr rest)))
        result)))

  (fset 'neovm--plist-diff
    (lambda (old new)
      ;; Returns (:added (...) :removed (...) :changed (...))
      (let ((added nil) (removed nil) (changed nil))
        ;; Check new for additions and changes
        (let ((rest new))
          (while rest
            (let ((k (car rest)) (v (cadr rest)))
              (if (not (plist-member old k))
                  (setq added (cons (cons k v) added))
                (unless (equal v (plist-get old k))
                  (setq changed (cons (list k (plist-get old k) v) changed)))))
            (setq rest (cddr rest))))
        ;; Check old for removals
        (let ((rest old))
          (while rest
            (unless (plist-member new (car rest))
              (setq removed (cons (cons (car rest) (cadr rest)) removed)))
            (setq rest (cddr rest))))
        (list :added (nreverse added)
              :removed (nreverse removed)
              :changed (nreverse changed)))))

  (fset 'neovm--plist-select
    (lambda (pl keys)
      ;; Select only specified keys from plist
      (let ((result nil))
        (dolist (k keys)
          (when (plist-member pl k)
            (setq result (plist-put result k (plist-get pl k)))))
        result)))

  (fset 'neovm--plist-reject
    (lambda (pl keys)
      ;; Remove specified keys from plist
      (let ((result nil)
            (rest pl))
        (while rest
          (unless (memq (car rest) keys)
            (setq result (plist-put result (car rest) (cadr rest))))
          (setq rest (cddr rest)))
        result)))

  (unwind-protect
      (let ((v1 '(:name "Alice" :age 30 :role "dev" :team "backend"))
            (v2 '(:name "Alice" :age 31 :role "lead" :dept "eng")))
        (list
         ;; Merge: overlay wins
         (funcall 'neovm--plist-merge v1 v2)
         ;; Diff: shows what changed between v1 and v2
         (funcall 'neovm--plist-diff v1 v2)
         ;; Select specific keys
         (funcall 'neovm--plist-select v1 '(:name :age))
         (funcall 'neovm--plist-select v1 '(:name :missing))
         ;; Reject specific keys
         (funcall 'neovm--plist-reject v1 '(:age :team))
         ;; Merge multiple layers
         (funcall 'neovm--plist-merge
                  (funcall 'neovm--plist-merge
                           '(:a 1 :b 2 :c 3)
                           '(:b 20 :d 40))
                  '(:c 300 :e 500))
         ;; Diff of identical plists
         (funcall 'neovm--plist-diff '(:x 1 :y 2) '(:x 1 :y 2))
         ;; Diff of completely different plists
         (funcall 'neovm--plist-diff '(:a 1) '(:b 2))))
    (fmakunbound 'neovm--plist-merge)
    (fmakunbound 'neovm--plist-diff)
    (fmakunbound 'neovm--plist-select)
    (fmakunbound 'neovm--plist-reject)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:name \"Alice\" :age 31 :role \"lead\" :team \"backend\" :dept \"eng\") (:added ((:dept . \"eng\")) :removed ((:team . \"backend\")) :changed ((:age 30 31) (:role \"dev\" \"lead\"))) (:name \"Alice\" :age 30) (:name \"Alice\") (:name \"Alice\" :role \"dev\") (:a 1 :b 20 :c 300 :d 40 :e 500) (:added nil :removed nil :changed nil) (:added ((:b . 2)) :removed ((:a . 1)) :changed nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
