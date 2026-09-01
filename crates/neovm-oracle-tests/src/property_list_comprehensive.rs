//! Comprehensive oracle parity tests for property list operations:
//! plist-get/plist-put/plist-member with all key types (symbols, strings, ints),
//! custom comparator (Emacs 29+), symbol-plist/setplist, get/put on symbols,
//! cl-getf with default, cl-remf, large plists (100+ entries), nested plists,
//! plist iteration via while loop, plist to alist conversion, plist merge patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// plist-get with all key types: symbols, keywords, integers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_get_all_key_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Keyword keys (most common usage)
      (plist-get '(:a 1 :b 2 :c 3) :a)
      (plist-get '(:a 1 :b 2 :c 3) :b)
      (plist-get '(:a 1 :b 2 :c 3) :c)
      (plist-get '(:a 1 :b 2 :c 3) :d)
      ;; Symbol keys
      (plist-get '(foo 10 bar 20 baz 30) 'foo)
      (plist-get '(foo 10 bar 20 baz 30) 'bar)
      (plist-get '(foo 10 bar 20 baz 30) 'baz)
      (plist-get '(foo 10 bar 20 baz 30) 'quux)
      ;; Integer keys (eq works for fixnums)
      (plist-get '(1 "one" 2 "two" 3 "three") 1)
      (plist-get '(1 "one" 2 "two" 3 "three") 2)
      (plist-get '(1 "one" 2 "two" 3 "three") 3)
      (plist-get '(1 "one" 2 "two" 3 "three") 99)
      ;; nil values (distinct from missing keys)
      (plist-get '(:present nil :also t) :present)
      (plist-get '(:present nil :also t) :missing)
      ;; Character keys (fixnums, eq works)
      (plist-get (list ?a "alpha" ?b "beta") ?a)
      (plist-get (list ?a "alpha" ?b "beta") ?b)
      (plist-get (list ?a "alpha" ?b "beta") ?c)
      ;; Boolean keys
      (plist-get '(t "yes" nil "no") t)
      ;; Single-entry plist
      (plist-get '(:only 42) :only)
      (plist-get '(:only 42) :other)
      ;; Empty plist
      (plist-get nil :a)
      (plist-get '() 'x))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 3 nil 10 20 30 nil \"one\" \"two\" \"three\" nil nil nil \"alpha\" \"beta\" nil \"yes\" 42 nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-put: insertion, update, value types, chaining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_put_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Build from nil
                         (pl (plist-put nil :a 1))
                         (pl (plist-put pl :b 2))
                         (pl (plist-put pl :c 3))
                         (snap1 (copy-sequence pl))
                         ;; Update existing key (first, middle, last)
                         (pl (plist-put pl :a 100))
                         (pl (plist-put pl :b 200))
                         (pl (plist-put pl :c 300))
                         (snap2 (copy-sequence pl))
                         ;; Add new key after updates
                         (pl (plist-put pl :d 4))
                         ;; Put nil as value (not removal)
                         (pl (plist-put pl :b nil))
                         ;; Put different type as value
                         (pl (plist-put pl :a "now-a-string"))
                         (pl (plist-put pl :c '(now a list)))
                         ;; Rapid updates to same key
                         (q (plist-put nil :x 1))
                         (q (plist-put q :x 2))
                         (q (plist-put q :x 3))
                         (q (plist-put q :x 4))
                         (q (plist-put q :x 5)))
                    (list snap1
                          snap2
                          (plist-get pl :a)
                          (plist-get pl :b)
                          (plist-get pl :c)
                          (plist-get pl :d)
                          ;; nil value is stored, not removed
                          (plist-member pl :b)
                          ;; Rapid update result
                          (plist-get q :x)
                          (length q)
                          ;; plist-put on empty list creates new plist
                          (plist-put nil :z 99)
                          ;; plist-put returns the plist
                          (plist-get (plist-put '(:w 1) :w 2) :w)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 2 :c 3) (:a 100 :b 200 :c 300) \"now-a-string\" nil (now a list) 4 (:b nil :c (now a list) :d 4) 5 2 (:z 99) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-member: tail semantics, distinguishing nil values from missing keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_member_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pl '(:a 1 :b 2 :c 3 :d 4)))
                    (list
                     ;; Returns tail from matched key
                     (plist-member pl :a)
                     (plist-member pl :b)
                     (plist-member pl :c)
                     (plist-member pl :d)
                     ;; Missing key returns nil
                     (plist-member pl :e)
                     ;; Value extraction via cadr
                     (cadr (plist-member pl :c))
                     ;; Rest extraction via cddr
                     (cddr (plist-member pl :b))
                     ;; Distinguish nil value from missing
                     (let ((p2 '(:present nil :after 42)))
                       (list (plist-member p2 :present)
                             (plist-member p2 :missing)
                             (if (plist-member p2 :present) 'exists 'absent)
                             (if (plist-member p2 :missing) 'exists 'absent)))
                     ;; Empty plist
                     (plist-member nil :a)
                     ;; Length of tail
                     (length (plist-member pl :a))
                     (length (plist-member pl :d))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 2 :c 3 :d 4) (:b 2 :c 3 :d 4) (:c 3 :d 4) (:d 4) nil 3 (:c 3 :d 4) ((:present nil :after 42) nil exists absent) nil 8 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// plist-get/plist-put/plist-member with custom predicate (Emacs 29+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_custom_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; String keys with equal predicate
      (plist-get '("name" "Alice" "age" 30 "city" "NYC") "name" #'equal)
      (plist-get '("name" "Alice" "age" 30 "city" "NYC") "age" #'equal)
      (plist-get '("name" "Alice" "age" 30 "city" "NYC") "city" #'equal)
      (plist-get '("name" "Alice" "age" 30 "city" "NYC") "missing" #'equal)
      ;; List keys with equal predicate
      (plist-get '((1 2) "pair-12" (3 4) "pair-34") '(1 2) #'equal)
      (plist-get '((1 2) "pair-12" (3 4) "pair-34") '(3 4) #'equal)
      (plist-get '((1 2) "pair-12" (3 4) "pair-34") '(5 6) #'equal)
      ;; string= predicate
      (plist-get '("x" 10 "y" 20 "z" 30) "y" #'string=)
      ;; plist-put with equal predicate — update existing string key
      (let* ((pl (plist-put nil "host" "localhost" #'equal))
             (pl (plist-put pl "port" 8080 #'equal))
             (pl (plist-put pl "host" "prod.example.com" #'equal)))
        (list (plist-get pl "host" #'equal)
              (plist-get pl "port" #'equal)
              (length pl)))
      ;; plist-member with predicate
      (plist-member '("a" 1 "b" 2 "c" 3) "b" #'equal)
      ;; Using eql predicate (default-like, but explicit)
      (plist-get '(1 "one" 2 "two") 1 #'eql)
      (plist-get '(1 "one" 2 "two") 2 #'eql))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 30 \"NYC\" nil \"pair-12\" \"pair-34\" nil 20 (\"prod.example.com\" 8080 4) (\"b\" 2 \"c\" 3) \"one\" \"two\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-plist, setplist, get, put — comprehensive symbol property operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_symbol_plist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (unwind-protect
          (let ((results nil))
            ;; Start clean
            (setplist 'neovm--pltest-sym nil)
            (push (symbol-plist 'neovm--pltest-sym) results)

            ;; Add properties via put
            (put 'neovm--pltest-sym 'color 'red)
            (put 'neovm--pltest-sym 'size 42)
            (put 'neovm--pltest-sym 'tags '(a b c))
            (put 'neovm--pltest-sym 'name "test-sym")

            ;; Read back via get
            (push (list (get 'neovm--pltest-sym 'color)
                        (get 'neovm--pltest-sym 'size)
                        (get 'neovm--pltest-sym 'tags)
                        (get 'neovm--pltest-sym 'name))
                  results)

            ;; Missing property returns nil
            (push (get 'neovm--pltest-sym 'nonexistent) results)

            ;; put returns the value
            (push (put 'neovm--pltest-sym 'ret-check 999) results)

            ;; Overwrite existing property
            (put 'neovm--pltest-sym 'color 'blue)
            (push (get 'neovm--pltest-sym 'color) results)

            ;; symbol-plist returns full plist
            (let ((pl (symbol-plist 'neovm--pltest-sym)))
              (push (plist-get pl 'size) results))

            ;; setplist replaces entire plist
            (setplist 'neovm--pltest-sym '(new-prop new-val))
            (push (list (get 'neovm--pltest-sym 'color)
                        (get 'neovm--pltest-sym 'new-prop))
                  results)

            ;; setplist nil clears all properties
            (setplist 'neovm--pltest-sym nil)
            (push (symbol-plist 'neovm--pltest-sym) results)

            (nreverse results))
        (setplist 'neovm--pltest-sym nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (red 42 (a b c) \"test-sym\") nil 999 blue 42 (nil new-val) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple symbols with independent property lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multiple_symbol_plists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (unwind-protect
          (progn
            (setplist 'neovm--pltest-a nil)
            (setplist 'neovm--pltest-b nil)
            (setplist 'neovm--pltest-c nil)

            ;; Same property name on different symbols
            (put 'neovm--pltest-a 'value 100)
            (put 'neovm--pltest-b 'value 200)
            (put 'neovm--pltest-c 'value 300)

            ;; Different properties on same symbol
            (put 'neovm--pltest-a 'x 1)
            (put 'neovm--pltest-a 'y 2)
            (put 'neovm--pltest-a 'z 3)

            (list
             ;; Each symbol has its own value
             (get 'neovm--pltest-a 'value)
             (get 'neovm--pltest-b 'value)
             (get 'neovm--pltest-c 'value)
             ;; Properties are independent
             (get 'neovm--pltest-b 'x)
             (get 'neovm--pltest-c 'y)
             ;; Full plist of sym-a
             (let ((pl (symbol-plist 'neovm--pltest-a)))
               (list (plist-get pl 'x) (plist-get pl 'y) (plist-get pl 'z)))
             ;; Clearing one doesn't affect others
             (progn
               (setplist 'neovm--pltest-b nil)
               (list (get 'neovm--pltest-a 'value)
                     (get 'neovm--pltest-b 'value)
                     (get 'neovm--pltest-c 'value)))))
        (setplist 'neovm--pltest-a nil)
        (setplist 'neovm--pltest-b nil)
        (setplist 'neovm--pltest-c nil)))"#;
    let expect = expect_test::expect![[r#""OK (100 200 300 nil nil (1 2 3) (100 nil 300))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Large plist (100+ entries): build, query, iterate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_large_plist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pl nil))
      ;; Build a plist with 100 entries: :k0 => 0, :k1 => 1, ..., :k99 => 99
      (dotimes (i 100)
        (setq pl (plist-put pl (intern (format ":k%d" i)) i)))
      (list
       ;; Size = 200 elements (100 key-value pairs)
       (length pl)
       ;; Query first, middle, last
       (plist-get pl (intern ":k0"))
       (plist-get pl (intern ":k49"))
       (plist-get pl (intern ":k99"))
       ;; Query non-existent
       (plist-get pl (intern ":k100"))
       ;; Membership for first and last
       (not (null (plist-member pl (intern ":k0"))))
       (not (null (plist-member pl (intern ":k99"))))
       (plist-member pl (intern ":k100"))
       ;; Update middle entry
       (let ((pl2 (plist-put pl (intern ":k50") 9999)))
         (plist-get pl2 (intern ":k50")))
       ;; Count all entries via iteration
       (let ((count 0) (rest pl))
         (while rest
           (setq count (1+ count))
           (setq rest (cddr rest)))
         count)
       ;; Sum first 10 values
       (let ((sum 0))
         (dotimes (i 10)
           (setq sum (+ sum (plist-get pl (intern (format ":k%d" i))))))
         sum)))"#;
    let expect = expect_test::expect![[r#""OK (200 0 49 99 nil t t nil 9999 100 45)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested plists: deep access and update
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_plist_deep_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((config '(:server (:host "localhost"
                                    :port 3000
                                    :ssl (:enabled t
                                          :cert "/etc/ssl/cert.pem"
                                          :key "/etc/ssl/key.pem"))
                                  :database (:host "db.local"
                                             :port 5432
                                             :pool (:min 5 :max 20 :timeout 30))
                                  :logging (:level :info
                                            :outputs (:file "/var/log/app.log"
                                                      :console t)))))
                    (list
                     ;; Level 1 access
                     (plist-get config :server)
                     (plist-get config :logging)
                     ;; Level 2 access
                     (plist-get (plist-get config :server) :host)
                     (plist-get (plist-get config :server) :port)
                     (plist-get (plist-get config :database) :host)
                     (plist-get (plist-get config :database) :port)
                     ;; Level 3 access
                     (plist-get (plist-get (plist-get config :server) :ssl) :enabled)
                     (plist-get (plist-get (plist-get config :server) :ssl) :cert)
                     (plist-get (plist-get (plist-get config :database) :pool) :min)
                     (plist-get (plist-get (plist-get config :database) :pool) :max)
                     (plist-get (plist-get (plist-get config :database) :pool) :timeout)
                     (plist-get (plist-get (plist-get config :logging) :outputs) :file)
                     (plist-get (plist-get (plist-get config :logging) :outputs) :console)
                     ;; Missing nested key
                     (plist-get (plist-get config :server) :nonexistent)
                     ;; Missing top-level key
                     (plist-get config :missing)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:host \"localhost\" :port 3000 :ssl (:enabled t :cert \"/etc/ssl/cert.pem\" :key \"/etc/ssl/key.pem\")) (:level :info :outputs (:file \"/var/log/app.log\" :console t)) \"localhost\" 3000 \"db.local\" 5432 t \"/etc/ssl/cert.pem\" 5 20 30 \"/var/log/app.log\" t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist iteration patterns: while/cddr, collecting, transforming
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_iteration_while_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pl '(:name "Bob" :age 25 :city "NYC" :score 92 :rank 3 :active t)))
                    (list
                     ;; Collect keys
                     (let ((keys nil) (rest pl))
                       (while rest
                         (setq keys (cons (car rest) keys))
                         (setq rest (cddr rest)))
                       (nreverse keys))
                     ;; Collect values
                     (let ((vals nil) (rest pl))
                       (while rest
                         (setq vals (cons (cadr rest) vals))
                         (setq rest (cddr rest)))
                       (nreverse vals))
                     ;; Collect as alist
                     (let ((pairs nil) (rest pl))
                       (while rest
                         (setq pairs (cons (cons (car rest) (cadr rest)) pairs))
                         (setq rest (cddr rest)))
                       (nreverse pairs))
                     ;; Count entries
                     (let ((count 0) (rest pl))
                       (while rest
                         (setq count (1+ count))
                         (setq rest (cddr rest)))
                       count)
                     ;; Filter: only numeric values
                     (let ((nums nil) (rest pl))
                       (while rest
                         (when (numberp (cadr rest))
                           (setq nums (cons (car rest) nums)))
                         (setq rest (cddr rest)))
                       (nreverse nums))
                     ;; Sum numeric values
                     (let ((sum 0) (rest pl))
                       (while rest
                         (when (numberp (cadr rest))
                           (setq sum (+ sum (cadr rest))))
                         (setq rest (cddr rest)))
                       sum)
                     ;; Filter: only string values
                     (let ((strs nil) (rest pl))
                       (while rest
                         (when (stringp (cadr rest))
                           (setq strs (cons (cons (car rest) (cadr rest)) strs)))
                         (setq rest (cddr rest)))
                       (nreverse strs))
                     ;; Empty plist iteration (0 iterations)
                     (let ((count 0) (rest nil))
                       (while rest
                         (setq count (1+ count))
                         (setq rest (cddr rest)))
                       count)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:name :age :city :score :rank :active) (\"Bob\" 25 \"NYC\" 92 3 t) ((:name . \"Bob\") (:age . 25) (:city . \"NYC\") (:score . 92) (:rank . 3) (:active . t)) 6 (:age :score :rank) 120 ((:name . \"Bob\") (:city . \"NYC\")) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist to alist and alist to plist conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_alist_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist-to-alist
                         (lambda (pl)
                           (let ((result nil) (rest pl))
                             (while rest
                               (setq result (cons (cons (car rest) (cadr rest)) result))
                               (setq rest (cddr rest)))
                             (nreverse result))))
                        (alist-to-plist
                         (lambda (al)
                           (let ((result nil))
                             (dolist (pair al)
                               (setq result (append result (list (car pair) (cdr pair)))))
                             result))))
                    (let ((pl '(:x 1 :y 2 :z 3))
                          (al '((:a . 10) (:b . 20) (:c . 30))))
                      (list
                       ;; Plist -> alist
                       (funcall plist-to-alist pl)
                       ;; Alist -> plist
                       (funcall alist-to-plist al)
                       ;; Round-trip: plist -> alist -> plist
                       (equal pl (funcall alist-to-plist (funcall plist-to-alist pl)))
                       ;; Round-trip: alist -> plist -> alist
                       (equal al (funcall plist-to-alist (funcall alist-to-plist al)))
                       ;; Empty conversions
                       (funcall plist-to-alist nil)
                       (funcall alist-to-plist nil)
                       ;; Conversion preserves order
                       (funcall plist-to-alist '(:first 1 :second 2 :third 3))
                       (funcall alist-to-plist '((:first . 1) (:second . 2) (:third . 3))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((:x . 1) (:y . 2) (:z . 3)) (:a 10 :b 20 :c 30) t t nil nil ((:first . 1) (:second . 2) (:third . 3)) (:first 1 :second 2 :third 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist merge patterns with conflict resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_merge_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist-merge
                         (lambda (base overlay)
                           "Merge overlay into base, overlay wins on conflict."
                           (let ((result (copy-sequence base))
                                 (rest overlay))
                             (while rest
                               (setq result (plist-put result (car rest) (cadr rest)))
                               (setq rest (cddr rest)))
                             result)))
                        (plist-merge-with
                         (lambda (resolver base overlay)
                           "Merge with custom conflict resolution."
                           (let ((result (copy-sequence base))
                                 (rest overlay))
                             (while rest
                               (let ((key (car rest))
                                     (new-val (cadr rest)))
                                 (if (plist-member result key)
                                     (setq result (plist-put result key
                                                             (funcall resolver
                                                                      (plist-get result key)
                                                                      new-val)))
                                   (setq result (plist-put result key new-val))))
                               (setq rest (cddr rest)))
                             result)))
                        (plist-remove
                         (lambda (pl key)
                           (let ((result nil) (rest pl))
                             (while rest
                               (unless (eq (car rest) key)
                                 (setq result (plist-put result (car rest) (cadr rest))))
                               (setq rest (cddr rest)))
                             result))))
                    (let ((p1 '(:a 1 :b 2 :c 3 :d 4))
                          (p2 '(:b 20 :c 30 :e 50 :f 60)))
                      (list
                       ;; Simple merge (overlay wins)
                       (funcall plist-merge p1 p2)
                       ;; Merge empty into non-empty
                       (funcall plist-merge p1 nil)
                       ;; Merge non-empty into empty
                       (funcall plist-merge nil p2)
                       ;; Merge with addition for conflicts
                       (funcall plist-merge-with (lambda (old new) (+ old new)) p1 p2)
                       ;; Merge keeping old value on conflict
                       (funcall plist-merge-with (lambda (old _) old) p1 p2)
                       ;; Merge building list of both on conflict
                       (funcall plist-merge-with (lambda (old new) (list old new)) p1 p2)
                       ;; Remove key (first, middle, last, missing)
                       (funcall plist-remove '(:x 1 :y 2 :z 3) :x)
                       (funcall plist-remove '(:x 1 :y 2 :z 3) :y)
                       (funcall plist-remove '(:x 1 :y 2 :z 3) :z)
                       (funcall plist-remove '(:x 1 :y 2 :z 3) :w))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :b 20 :c 30 :d 4 :e 50 :f 60) (:a 1 :b 2 :c 3 :d 4) (:b 20 :c 30 :e 50 :f 60) (:a 1 :b 22 :c 33 :d 4 :e 50 :f 60) (:a 1 :b 2 :c 3 :d 4 :e 50 :f 60) (:a 1 :b (2 20) :c (3 30) :d 4 :e 50 :f 60) (:y 2 :z 3) (:x 1 :z 3) (:x 1 :y 2) (:x 1 :y 2 :z 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist as configuration store: defaults, overrides, validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_config_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((defaults '(:width 80 :height 24 :color t :font-size 12 :theme :light))
                        (user-prefs '(:width 120 :theme :dark :font-size 14))
                        (plist-get-or
                         (lambda (pl key default)
                           (if (plist-member pl key)
                               (plist-get pl key)
                             default)))
                        (apply-overrides
                         (lambda (base overrides)
                           (let ((result (copy-sequence base))
                                 (rest overrides))
                             (while rest
                               (setq result (plist-put result (car rest) (cadr rest)))
                               (setq rest (cddr rest)))
                             result))))
                    (let ((config (funcall apply-overrides defaults user-prefs)))
                      (list
                       ;; Overridden values
                       (plist-get config :width)
                       (plist-get config :theme)
                       (plist-get config :font-size)
                       ;; Defaults preserved where not overridden
                       (plist-get config :height)
                       (plist-get config :color)
                       ;; get-or-default for missing keys
                       (funcall plist-get-or config :tab-width 4)
                       (funcall plist-get-or config :width 80)
                       ;; Full config
                       config)))"#;
    let expect = expect_test::expect![[
        r#""OK (120 :dark 14 24 t 4 120 (:width 120 :height 24 :color t :font-size 14 :theme :dark))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-getf with default value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_getf_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
      (require 'cl-lib)
      (list
       ;; cl-getf with existing key
       (cl-getf '(:a 1 :b 2 :c 3) :a)
       (cl-getf '(:a 1 :b 2 :c 3) :b)
       (cl-getf '(:a 1 :b 2 :c 3) :c)
       ;; cl-getf with missing key and default
       (cl-getf '(:a 1 :b 2) :c 'default-val)
       (cl-getf '(:a 1 :b 2) :c 42)
       (cl-getf '(:a 1 :b 2) :c "fallback")
       ;; cl-getf with nil value (returns nil, not default)
       (cl-getf '(:a nil :b 2) :a 'should-not-see-this)
       ;; cl-getf on empty plist with default
       (cl-getf nil :any 'empty-default)
       ;; cl-getf without explicit default (defaults to nil)
       (cl-getf '(:a 1) :missing)))"#;
    let expect =
        expect_test::expect![[r#""OK (1 2 3 default-val 42 \"fallback\" nil empty-default nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist equality checking (order-independent)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_equality_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist-equal-p
                         (lambda (a b)
                           "Order-independent plist equality."
                           (and (= (length a) (length b))
                                (let ((eq-so-far t) (rest a))
                                  (while (and rest eq-so-far)
                                    (unless (and (plist-member b (car rest))
                                                 (equal (plist-get b (car rest))
                                                        (cadr rest)))
                                      (setq eq-so-far nil))
                                    (setq rest (cddr rest)))
                                  eq-so-far)))))
                    (list
                     ;; Same order => equal
                     (funcall plist-equal-p '(:a 1 :b 2 :c 3) '(:a 1 :b 2 :c 3))
                     ;; Different order => equal
                     (funcall plist-equal-p '(:a 1 :b 2 :c 3) '(:c 3 :a 1 :b 2))
                     ;; Different values => not equal
                     (funcall plist-equal-p '(:a 1 :b 2) '(:a 1 :b 999))
                     ;; Different keys => not equal
                     (funcall plist-equal-p '(:a 1 :b 2) '(:a 1 :c 2))
                     ;; Different lengths => not equal
                     (funcall plist-equal-p '(:a 1) '(:a 1 :b 2))
                     ;; Both empty => equal
                     (funcall plist-equal-p nil nil)
                     ;; One empty => not equal
                     (funcall plist-equal-p nil '(:a 1))
                     ;; Deep values => equal
                     (funcall plist-equal-p
                              '(:data (1 2 3) :nested (:x 1))
                              '(:nested (:x 1) :data (1 2 3)))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist-based event/handler registry
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_event_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((registry nil)
                        (register
                         (lambda (event handler)
                           (let ((handlers (plist-get registry event)))
                             (setq registry
                                   (plist-put registry event
                                              (append handlers (list handler)))))))
                        (dispatch
                         (lambda (event data)
                           (let ((handlers (plist-get registry event))
                                 (results nil))
                             (dolist (h handlers)
                               (setq results (cons (funcall h data) results)))
                             (nreverse results))))
                        (handler-count
                         (lambda (event)
                           (length (plist-get registry event)))))
                    ;; Register handlers
                    (funcall register :click (lambda (d) (format "click: %S" d)))
                    (funcall register :click (lambda (d) (plist-get d :x)))
                    (funcall register :hover (lambda (d) (upcase d)))
                    (funcall register :hover (lambda (d) (length d)))
                    (funcall register :hover (lambda (d) (concat d "!")))

                    (list
                     ;; Dispatch click
                     (funcall dispatch :click '(:x 100 :y 200))
                     ;; Dispatch hover
                     (funcall dispatch :hover "button")
                     ;; Dispatch unknown event (no handlers)
                     (funcall dispatch :scroll '(:delta 3))
                     ;; Handler counts
                     (funcall handler-count :click)
                     (funcall handler-count :hover)
                     (funcall handler-count :scroll)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable registry)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Plist select/reject/transform utilities
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_plist_select_reject_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((plist-select
                         (lambda (pl keys)
                           "Select only specified keys from plist."
                           (let ((result nil))
                             (dolist (k keys)
                               (when (plist-member pl k)
                                 (setq result (plist-put result k (plist-get pl k)))))
                             result)))
                        (plist-reject
                         (lambda (pl keys)
                           "Remove specified keys from plist."
                           (let ((result nil) (rest pl))
                             (while rest
                               (unless (memq (car rest) keys)
                                 (setq result (plist-put result (car rest) (cadr rest))))
                               (setq rest (cddr rest)))
                             result)))
                        (plist-map-values
                         (lambda (pl fn)
                           "Transform all values in plist."
                           (let ((result nil) (rest pl))
                             (while rest
                               (setq result (plist-put result (car rest) (funcall fn (cadr rest))))
                               (setq rest (cddr rest)))
                             result)))
                        (plist-map-keys
                         (lambda (pl fn)
                           "Transform all keys in plist."
                           (let ((result nil) (rest pl))
                             (while rest
                               (setq result (plist-put result (funcall fn (car rest)) (cadr rest)))
                               (setq rest (cddr rest)))
                             result))))
                    (let ((pl '(:a 1 :b 2 :c 3 :d 4 :e 5)))
                      (list
                       ;; Select subset
                       (funcall plist-select pl '(:a :c :e))
                       (funcall plist-select pl '(:b :d))
                       (funcall plist-select pl '(:z))
                       (funcall plist-select pl nil)
                       ;; Reject subset
                       (funcall plist-reject pl '(:a :c :e))
                       (funcall plist-reject pl '(:b :d))
                       (funcall plist-reject pl '(:z))
                       ;; Map values: double all
                       (funcall plist-map-values '(:x 10 :y 20 :z 30)
                                (lambda (v) (* v 2)))
                       ;; Map values: stringify
                       (funcall plist-map-values '(:a 1 :b 2)
                                (lambda (v) (format "%d" v))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:a 1 :c 3 :e 5) (:b 2 :d 4) nil nil (:b 2 :d 4) (:a 1 :c 3 :e 5) (:a 1 :b 2 :c 3 :d 4 :e 5) (:x 20 :y 40 :z 60) (:a \"1\" :b \"2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
