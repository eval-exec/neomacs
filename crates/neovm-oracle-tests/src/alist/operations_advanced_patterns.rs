//! Oracle parity tests for advanced alist patterns:
//! `assoc` with TEST param (eq, equal, string=, custom), `rassoc` with TEST,
//! `assq` vs `assoc` differences, `alist-get` with KEY ALIST &optional DEFAULT
//! REMOVE TESTFN, nested alists (tree-shaped), alist as environment (variable
//! lookup chains), alist merge/override patterns, destructive vs non-destructive
//! alist operations, and converting between alists and hash tables and plists.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// assoc with all TEST param variants: eq, equal, string=, string-equal-ignore-case, custom lambdas
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_test_param_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((num-al '((1 . "one") (2 . "two") (3 . "three") (1.0 . "one-float")))
                        (str-al '(("FOO" . 1) ("bar" . 2) ("Baz" . 3) ("quux" . 4)))
                        (sym-al '((a . 10) (b . 20) (c . 30)))
                        (list-al '(((1 2) . "pair-a") ((3 4) . "pair-b") ((1 2) . "pair-c"))))
                    (list
                     ;; assoc with #'eq: symbols are eq, fixnums are eq in most impls
                     (assoc 'b sym-al #'eq)
                     (assoc 'z sym-al #'eq)
                     ;; assoc with #'equal (default): works on lists
                     (assoc '(1 2) list-al #'equal)
                     (assoc '(1 2) list-al)
                     ;; assoc with #'eq on lists: won't find copied list
                     (assoc '(1 2) list-al #'eq)
                     ;; assoc with #'string= on strings (case-sensitive)
                     (assoc "FOO" str-al #'string=)
                     (assoc "foo" str-al #'string=)
                     ;; assoc with #'string-equal (synonym for string=)
                     (assoc "bar" str-al #'string-equal)
                     ;; assoc with #'string-equal-ignore-case
                     (assoc "foo" str-al #'string-equal-ignore-case)
                     (assoc "BAZ" str-al #'string-equal-ignore-case)
                     (assoc "QUUX" str-al #'string-equal-ignore-case)
                     ;; assoc with custom lambda: match if key is within +/- 0.5
                     (assoc 1.3 num-al
                            (lambda (key elt)
                              (< (abs (- key elt)) 0.5)))
                     ;; assoc with custom: match by string prefix
                     (assoc "qu" str-al
                            (lambda (prefix elt)
                              (and (stringp elt)
                                   (>= (length elt) (length prefix))
                                   (string= prefix (substring elt 0 (length prefix))))))
                     ;; nil testfn => default to equal
                     (assoc '(3 4) list-al nil)
                     ;; assoc on empty alist
                     (assoc 'x nil)
                     (assoc 'x nil #'eq)))"#;
    let expect = expect_test::expect![[
        r#""OK ((b . 20) nil ((1 2) . \"pair-a\") ((1 2) . \"pair-a\") nil (\"FOO\" . 1) nil (\"bar\" . 2) (\"FOO\" . 1) (\"Baz\" . 3) (\"quux\" . 4) (1 . \"one\") nil ((3 4) . \"pair-b\") nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rassoc with TEST param: eq, equal, string=, custom
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rassoc_test_param_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((al '((a . "hello") (b . "world") (c . "HELLO") (d . (1 2 3)) (e . (1 2 3)))))
                    (list
                     ;; rassoc with default (equal)
                     (rassoc "hello" al)
                     (rassoc '(1 2 3) al)
                     ;; rassoc with #'string-equal-ignore-case
                     (rassoc "HELLO" al #'string-equal-ignore-case)
                     (rassoc "World" al #'string-equal-ignore-case)
                     ;; rassoc with #'eq: strings won't match copies
                     (rassoc "hello" al #'eq)
                     ;; rassoc with custom lambda: match value length
                     (rassoc 5
                             '((x . "abcde") (y . "fg") (z . "hijkl"))
                             (lambda (target val)
                               (and (stringp val) (= target (length val)))))
                     ;; rassoc not found
                     (rassoc "missing" al)
                     (rassoc nil al)
                     ;; rassoc on empty alist
                     (rassoc 'x nil)
                     ;; rassoc with #'equal explicitly
                     (rassoc '(1 2 3) al #'equal)))"#;
    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments rassoc 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assq vs assoc: identity vs structural equality
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assq_vs_assoc_identity_differences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((shared-list (list 1 2 3))
                          (al (list (cons shared-list "shared")
                                    (cons (list 1 2 3) "copy")
                                    (cons 'sym "symbol")
                                    (cons 42 "fixnum")
                                    (cons "hello" "str-a")
                                    (cons "hello" "str-b"))))
                    (list
                     ;; assq uses eq: finds shared reference
                     (cdr (assq shared-list al))
                     ;; assq won't find structural copy
                     (assq (list 1 2 3) al)
                     ;; assoc with equal finds both shared and copy (first match)
                     (cdr (assoc (list 1 2 3) al))
                     ;; Symbols: assq and assoc both find (eq for symbols)
                     (cdr (assq 'sym al))
                     (cdr (assoc 'sym al))
                     ;; Fixnums: assq finds (fixnums are eq)
                     (cdr (assq 42 al))
                     (cdr (assoc 42 al))
                     ;; Strings: assq uses eq (may or may not find)
                     ;; assoc uses equal (always finds)
                     (cdr (assoc "hello" al))
                     ;; rassq vs rassoc
                     (rassq "symbol" al)
                     (rassoc "symbol" al)
                     ;; Testing with nil keys
                     (let ((nil-al '((nil . "nil-val") (t . "t-val"))))
                       (list (assq nil nil-al)
                             (assoc nil nil-al)
                             (assq t nil-al)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"shared\" nil \"shared\" \"symbol\" \"symbol\" \"fixnum\" \"fixnum\" \"str-a\" nil (sym . \"symbol\") ((nil . \"nil-val\") (nil . \"nil-val\") (t . \"t-val\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get: exhaustive parameter combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_get_exhaustive_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((al '((name . "Alice") (age . 30) (score . nil)
                              (active . t) (tags . (x y z)))))
                    (list
                     ;; (alist-get KEY ALIST) — basic
                     (alist-get 'name al)
                     (alist-get 'age al)
                     (alist-get 'missing al)
                     ;; (alist-get KEY ALIST DEFAULT) — with default
                     (alist-get 'missing al "fallback")
                     (alist-get 'missing al 0)
                     (alist-get 'missing al nil)
                     (alist-get 'missing al '(a b c))
                     ;; Key found, value is nil — DEFAULT ignored since key exists
                     (alist-get 'score al 999)
                     ;; (alist-get KEY ALIST DEFAULT REMOVE)
                     ;; REMOVE=t: treat nil-valued entries as absent
                     (alist-get 'score al nil nil)
                     (alist-get 'score al nil t)
                     (alist-get 'score al 'default-for-nil t)
                     ;; REMOVE=t but value is non-nil: no effect
                     (alist-get 'name al nil t)
                     (alist-get 'active al nil t)
                     ;; (alist-get KEY ALIST DEFAULT REMOVE TESTFN)
                     (let ((str-al '(("Name" . "Bob") ("AGE" . 25) ("score" . nil))))
                       (list
                        ;; TESTFN=#'equal for string keys
                        (alist-get "Name" str-al nil nil #'equal)
                        ;; TESTFN=#'string-equal-ignore-case
                        (alist-get "name" str-al nil nil #'string-equal-ignore-case)
                        (alist-get "AGE" str-al nil nil #'equal)
                        (alist-get "age" str-al nil nil #'string-equal-ignore-case)
                        ;; TESTFN + DEFAULT + REMOVE combo
                        (alist-get "SCORE" str-al 'fallback t #'string-equal-ignore-case)
                        (alist-get "missing" str-al 'nope nil #'equal)
                        ;; TESTFN=nil defaults to eq
                        (alist-get "Name" str-al nil nil nil)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 30 nil \"fallback\" 0 nil (a b c) nil nil nil nil \"Alice\" t (\"Bob\" \"Bob\" 25 25 nil nope nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested (tree-shaped) alists: deep lookup, path-based access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_alist_tree_shaped() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((config
                          '((server . ((host . "localhost")
                                       (port . 8080)
                                       (ssl . ((enabled . t)
                                               (cert . "/path/cert.pem")
                                               (key . "/path/key.pem")))))
                            (database . ((primary . ((host . "db1.local")
                                                     (port . 5432)
                                                     (name . "mydb")))
                                          (replica . ((host . "db2.local")
                                                      (port . 5432)
                                                      (name . "mydb")))))
                            (logging . ((level . "info")
                                        (file . "/var/log/app.log"))))))
                    ;; Deep accessor: follow chain of keys through nested alists
                    (let ((deep-get
                           (lambda (tree keys)
                             (let ((node tree))
                               (while (and keys node)
                                 (setq node (cdr (assq (car keys) node)))
                                 (setq keys (cdr keys)))
                               node))))
                      (list
                       ;; Top-level keys
                       (mapcar #'car config)
                       ;; Deep access: server.host
                       (funcall deep-get config '(server host))
                       ;; Deep access: server.ssl.cert
                       (funcall deep-get config '(server ssl cert))
                       ;; Deep access: database.primary.port
                       (funcall deep-get config '(database primary port))
                       ;; Deep access: database.replica.host
                       (funcall deep-get config '(database replica host))
                       ;; Non-existent path returns nil
                       (funcall deep-get config '(server ssl nonexistent))
                       (funcall deep-get config '(nope deeper still))
                       ;; Collect all leaf values under server.ssl
                       (let ((ssl-section (funcall deep-get config '(server ssl))))
                         (mapcar #'cdr ssl-section))
                       ;; Compare database primary vs replica
                       (let ((pri (funcall deep-get config '(database primary)))
                             (rep (funcall deep-get config '(database replica))))
                         (list
                          (equal (cdr (assq 'port pri)) (cdr (assq 'port rep)))
                          (equal (cdr (assq 'name pri)) (cdr (assq 'name rep)))
                          (equal (cdr (assq 'host pri)) (cdr (assq 'host rep))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((server database logging) \"localhost\" \"/path/cert.pem\" 5432 \"db2.local\" nil nil (t \"/path/cert.pem\" \"/path/key.pem\") (t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist as environment: variable lookup chains with shadowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_environment_lookup_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simulate lexical environment frames using alist chains
  (fset 'neovm--env-lookup
    (lambda (var frames)
      "Look up VAR in chain of environment FRAMES (list of alists)."
      (let ((result nil) (found nil))
        (while (and frames (not found))
          (let ((binding (assq var (car frames))))
            (when binding
              (setq result (cdr binding))
              (setq found t)))
          (setq frames (cdr frames)))
        (if found result (list :unbound var)))))

  (fset 'neovm--env-extend
    (lambda (bindings frames)
      "Add a new frame of BINDINGS (alist) to FRAMES."
      (cons bindings frames)))

  (unwind-protect
      (let* ((global-env '((pi . 3) (e . 2) (max-int . 999)))
             (frames (list global-env))
             ;; Extend with function scope
             (frames (funcall 'neovm--env-extend
                              '((x . 10) (y . 20))
                              frames))
             ;; Extend with inner block scope that shadows x
             (frames (funcall 'neovm--env-extend
                              '((x . 42) (z . 100))
                              frames)))
        (list
         ;; x is shadowed: innermost wins
         (funcall 'neovm--env-lookup 'x frames)
         ;; y from middle frame
         (funcall 'neovm--env-lookup 'y frames)
         ;; z from innermost
         (funcall 'neovm--env-lookup 'z frames)
         ;; pi from global
         (funcall 'neovm--env-lookup 'pi frames)
         ;; unbound variable
         (funcall 'neovm--env-lookup 'missing frames)
         ;; Pop innermost: x should be 10 again
         (funcall 'neovm--env-lookup 'x (cdr frames))
         ;; Pop two: only global
         (funcall 'neovm--env-lookup 'x (cddr frames))
         ;; All visible variable names (outermost to innermost, first occurrence)
         (let ((all-vars nil)
               (seen nil)
               (fs frames))
           (while fs
             (dolist (binding (car fs))
               (unless (memq (car binding) seen)
                 (setq all-vars (cons (car binding) all-vars))
                 (setq seen (cons (car binding) seen))))
             (setq fs (cdr fs)))
           (sort all-vars (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))
    (fmakunbound 'neovm--env-lookup)
    (fmakunbound 'neovm--env-extend)))"#;
    let expect = expect_test::expect![
        r#""OK (42 20 100 3 (:unbound missing) 10 (:unbound x) (e max-int pi x y z))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist merge/override: multiple strategies (last-wins, first-wins, deep merge)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_merge_override_strategies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((base '((a . 1) (b . 2) (c . 3) (d . 4)))
                        (patch1 '((b . 20) (d . 40) (e . 50)))
                        (patch2 '((a . 100) (c . 300) (f . 600))))
                    ;; Strategy 1: last-wins merge (patch overrides base)
                    (let ((last-wins
                           (lambda (base patch)
                             (let ((result (copy-alist base)))
                               (dolist (pair patch)
                                 (let ((existing (assq (car pair) result)))
                                   (if existing
                                       (setcdr existing (cdr pair))
                                     (setq result (append result (list (cons (car pair) (cdr pair))))))))
                               result))))
                      ;; Strategy 2: first-wins merge (base wins over patch)
                      (let ((first-wins
                             (lambda (base patch)
                               (let ((result (copy-alist base)))
                                 (dolist (pair patch)
                                   (unless (assq (car pair) result)
                                     (setq result (append result (list (cons (car pair) (cdr pair)))))))
                                 result))))
                        ;; Strategy 3: merge with combiner function
                        (let ((merge-with
                               (lambda (combiner base patch)
                                 (let ((result (copy-alist base)))
                                   (dolist (pair patch)
                                     (let ((existing (assq (car pair) result)))
                                       (if existing
                                           (setcdr existing (funcall combiner (cdr existing) (cdr pair)))
                                         (setq result (append result (list (cons (car pair) (cdr pair))))))))
                                   result))))
                          (list
                           ;; Last-wins
                           (funcall last-wins base patch1)
                           ;; First-wins
                           (funcall first-wins base patch1)
                           ;; Merge-with +
                           (funcall merge-with #'+ base patch1)
                           ;; Chain: base <- patch1 <- patch2, last-wins
                           (funcall last-wins (funcall last-wins base patch1) patch2)
                           ;; Verify keys
                           (sort (mapcar #'car (funcall last-wins base patch1))
                                 (lambda (a b) (string< (symbol-name a) (symbol-name b))))
                           ;; Merge with max
                           (funcall merge-with #'max base patch1)
                           ;; Merge empty
                           (funcall last-wins base nil)
                           (funcall last-wins nil patch1))))))"#;
    let expect = expect_test::expect![
        r#""OK (((a . 1) (b . 20) (c . 3) (d . 40) (e . 50)) ((a . 1) (b . 2) (c . 3) (d . 4) (e . 50)) ((a . 1) (b . 22) (c . 3) (d . 44) (e . 50)) ((a . 100) (b . 20) (c . 300) (d . 40) (e . 50) (f . 600)) (a b c d e) ((a . 1) (b . 20) (c . 3) (d . 40) (e . 50)) ((a . 1) (b . 2) (c . 3) (d . 4)) ((b . 20) (d . 40) (e . 50)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Destructive vs non-destructive alist operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_destructive_vs_nondestructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; setcdr on assq result mutates in-place
  (let ((al (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
    (setcdr (assq 'b al) 200)
    (list (cdr (assq 'a al)) (cdr (assq 'b al)) (cdr (assq 'c al))))

  ;; delq removes destructively (first occurrence)
  (let ((al (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
    (let ((pair (assq 'b al)))
      (setq al (delq pair al))
      (list (length al)
            (assq 'b al)
            (mapcar #'car al))))

  ;; Non-destructive removal via seq-filter
  (let ((al '((a . 1) (b . 2) (c . 3) (d . 4))))
    (let ((filtered (seq-filter (lambda (p) (not (eq (car p) 'c))) al)))
      (list
       ;; Original unchanged
       (length al)
       (assq 'c al)
       ;; Filtered version
       (length filtered)
       (assq 'c filtered))))

  ;; nconc for destructive append of alists
  (let ((al1 (list (cons 'a 1) (cons 'b 2)))
        (al2 (list (cons 'c 3) (cons 'd 4))))
    (nconc al1 al2)
    (list (length al1)
          (mapcar #'car al1)))

  ;; Non-destructive append
  (let ((al1 '((a . 1) (b . 2)))
        (al2 '((c . 3) (d . 4))))
    (let ((combined (append al1 al2)))
      (list (length al1) (length combined)
            (mapcar #'car combined))))

  ;; sort is destructive
  (let ((al (list (cons 'c 3) (cons 'a 1) (cons 'b 2))))
    (let ((sorted (sort al (lambda (x y) (string< (symbol-name (car x))
                                                    (symbol-name (car y)))))))
      (mapcar #'car sorted))))"#;
    let expect = expect_test::expect![
        r#""OK ((1 200 3) (2 nil (a c)) (4 (c . 3) 3 nil) (4 (a b c d)) (2 4 (a b c d)) (a b c))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Converting between alists, hash tables, and plists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_conversion_hashtable_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Alist -> hash table -> alist roundtrip
  (let* ((original '((name . "Alice") (age . 30) (city . "Boston")))
         (ht (make-hash-table :test 'equal))
         (_ (dolist (pair original)
              (puthash (car pair) (cdr pair) ht)))
         ;; Convert back
         (back nil)
         (_ (maphash (lambda (k v) (setq back (cons (cons k v) back))) ht))
         (back (sort back (lambda (a b) (string< (symbol-name (car a))
                                                   (symbol-name (car b)))))))
    (list
     (gethash 'name ht)
     (gethash 'age ht)
     (length back)
     (mapcar #'car back)))

  ;; Alist -> plist -> alist roundtrip
  (let* ((al '((x . 10) (y . 20) (z . 30)))
         ;; alist to plist: (x . 10) -> :x 10
         (pl nil)
         (_ (dolist (pair (reverse al))
              (setq pl (cons (intern (concat ":" (symbol-name (car pair)))) pl))
              (setq pl (cons (cdr pair) pl))))
         ;; plist back to alist
         (al2 nil)
         (rest pl))
    (while rest
      (let ((key (intern (substring (symbol-name (car rest)) 1)))
            (val (cadr rest)))
        (setq al2 (cons (cons key val) al2)))
      (setq rest (cddr rest)))
    (setq al2 (nreverse al2))
    (list pl al2
          (equal al al2)))

  ;; Plist -> alist
  (let* ((pl '(:name "Bob" :age 25 :active t))
         (al nil)
         (rest pl))
    (while rest
      (let ((key (intern (substring (symbol-name (car rest)) 1)))
            (val (cadr rest)))
        (setq al (cons (cons key val) al)))
      (setq rest (cddr rest)))
    (nreverse al))

  ;; Hash table -> alist (sorted)
  (let ((ht (make-hash-table :test 'eq)))
    (puthash 'c 3 ht)
    (puthash 'a 1 ht)
    (puthash 'b 2 ht)
    (let ((al nil))
      (maphash (lambda (k v) (setq al (cons (cons k v) al))) ht)
      (sort al (lambda (x y) (string< (symbol-name (car x))
                                        (symbol-name (car y))))))))"#;
    let expect = expect_test::expect![r#""ERR (wrong-type-argument symbolp 10)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assoc-string: case-sensitive and case-insensitive with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_string_advanced_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((al '(("Content-Type" . "text/html")
                              ("X-Custom-Header" . "value")
                              ("content-length" . "1024")
                              ("" . "empty-key")
                              ("ACCEPT" . "*/*"))))
                    (list
                     ;; Case-sensitive (default CASE-FOLD=nil)
                     (assoc-string "Content-Type" al)
                     (assoc-string "content-type" al)
                     ;; Case-insensitive
                     (assoc-string "CONTENT-TYPE" al t)
                     (assoc-string "content-length" al t)
                     (assoc-string "CONTENT-LENGTH" al t)
                     (assoc-string "accept" al t)
                     ;; Empty key
                     (assoc-string "" al)
                     (assoc-string "" al t)
                     ;; Not found
                     (assoc-string "missing" al)
                     (assoc-string "MISSING" al t)
                     ;; Duplicate keys with different case
                     (let ((dupes '(("Key" . 1) ("KEY" . 2) ("key" . 3) ("KeY" . 4))))
                       (list
                        ;; Case-sensitive: finds exact match
                        (assoc-string "Key" dupes)
                        (assoc-string "KEY" dupes)
                        (assoc-string "key" dupes)
                        ;; Case-insensitive: finds first match
                        (assoc-string "key" dupes t)
                        (assoc-string "KEY" dupes t)))
                     ;; With nil alist
                     (assoc-string "x" nil)
                     (assoc-string "x" nil t)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Content-Type\" . \"text/html\") nil (\"Content-Type\" . \"text/html\") (\"content-length\" . \"1024\") (\"content-length\" . \"1024\") (\"ACCEPT\" . \"*/*\") (\"\" . \"empty-key\") (\"\" . \"empty-key\") nil nil ((\"Key\" . 1) (\"KEY\" . 2) (\"key\" . 3) (\"Key\" . 1) (\"Key\" . 1)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_assoc_string_matches_atom_string_and_symbol_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/minibuf.c:Fassoc_string uses the element itself when an alist
    // item is not a cons cell.  Single string and symbol entries can therefore
    // match and are returned directly, before any later cons entry.
    let form = r#"
(list
 (assoc-string "solo" '("solo" ("solo" . pair-hit)))
 (assoc-string "SOLO" '("solo" ("SOLO" . pair-hit)) t)
 (assoc-string 'symbol-key '(symbol-key (symbol-key . pair-hit)))
 (assoc-string "symbol-key" '(symbol-key ("symbol-key" . string-pair)))
 (assoc-string "missing" '("other" symbol-key . bad-tail)))
"#;
    let expect = expect_test::expect![[r#""OK (\"solo\" \"solo\" symbol-key symbol-key nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_assoc_string_strict_edge_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/minibuf.c:Fassoc_string only converts symbol keys up front.
    // Other key types are accepted until compare-strings is reached; non-string
    // alist cars are skipped, and non-cons LIST tails terminate the scan.
    let form = r#"
(let ((capture (lambda (form)
                 (condition-case err
                     (eval form)
                   (error (cons (car err) (cdr err)))))))
  (list
   (assoc-string "hit" '((1 . skip-number)
                         (nil . skip-nil)
                         ((hit) . skip-list)
                         ("hit" . string-hit)))
   (assoc-string "sym" '(42 sym ("sym" . string-hit)))
   (assoc-string 'Sym '(("sym" . lower-string)
                        (Sym . symbol-hit))
                 '(any-non-nil-case-fold))
   (assoc-string "x" "not-a-list")
   (assoc-string 42 nil)
   (funcall capture '(assoc-string 42 '(("42" . would-error))))
   (funcall capture '(assoc-string 42 '(not-string-symbol-entry
                                        ("42" . delayed-error))))
   (funcall capture '(assoc-string "x"))
   (funcall capture '(assoc-string "x" nil nil 'extra))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hit\" . string-hit) sym (\"sym\" . lower-string) nil nil (wrong-type-argument stringp 42) (wrong-type-argument stringp 42) (wrong-number-of-arguments assoc-string 1) (wrong-number-of-arguments assoc-string 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist deduplication and key canonicalization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_dedup_and_canonicalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Remove duplicate keys, keeping first occurrence
  (let ((al '((a . 1) (b . 2) (a . 3) (c . 4) (b . 5) (a . 6))))
    (let ((seen nil) (result nil))
      (dolist (pair al)
        (unless (memq (car pair) seen)
          (setq result (cons pair result))
          (setq seen (cons (car pair) seen))))
      (nreverse result)))

  ;; Remove duplicate keys, keeping last occurrence
  (let ((al '((a . 1) (b . 2) (a . 3) (c . 4) (b . 5) (a . 6))))
    (let ((seen nil) (result nil))
      (dolist (pair (reverse al))
        (unless (memq (car pair) seen)
          (setq result (cons pair result))
          (setq seen (cons (car pair) seen))))
      result))

  ;; Canonicalize: sort by key name
  (let ((al '((z . 26) (a . 1) (m . 13) (f . 6) (b . 2))))
    (sort (copy-sequence al)
          (lambda (x y) (string< (symbol-name (car x))
                                  (symbol-name (car y))))))

  ;; Count occurrences of each key
  (let ((al '((a . 1) (b . 2) (a . 3) (c . 4) (b . 5) (a . 6))))
    (let ((counts nil))
      (dolist (pair al)
        (let ((entry (assq (car pair) counts)))
          (if entry
              (setcdr entry (1+ (cdr entry)))
            (setq counts (cons (cons (car pair) 1) counts)))))
      (sort counts (lambda (x y) (string< (symbol-name (car x))
                                            (symbol-name (car y)))))))

  ;; Group values by key
  (let ((al '((a . 1) (b . 2) (a . 3) (c . 4) (b . 5) (a . 6))))
    (let ((groups nil))
      (dolist (pair al)
        (let ((entry (assq (car pair) groups)))
          (if entry
              (setcdr entry (append (cdr entry) (list (cdr pair))))
            (setq groups (cons (list (car pair) (cdr pair)) groups)))))
      (sort groups (lambda (x y) (string< (symbol-name (car x))
                                            (symbol-name (car y))))))))"#;
    let expect = expect_test::expect![
        r#""OK (((a . 1) (b . 2) (c . 4)) ((c . 4) (b . 5) (a . 6)) ((a . 1) (b . 2) (f . 6) (m . 13) (z . 26)) ((a . 3) (b . 2) (c . 1)) ((a 1 3 6) (b 2 5) (c 4)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist difference, intersection, symmetric difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((al1 '((a . 1) (b . 2) (c . 3) (d . 4)))
                        (al2 '((b . 20) (d . 40) (e . 50) (f . 60))))
                    (list
                     ;; Difference: keys in al1 not in al2
                     (let ((diff nil))
                       (dolist (pair al1)
                         (unless (assq (car pair) al2)
                           (setq diff (cons pair diff))))
                       (nreverse diff))
                     ;; Difference: keys in al2 not in al1
                     (let ((diff nil))
                       (dolist (pair al2)
                         (unless (assq (car pair) al1)
                           (setq diff (cons pair diff))))
                       (nreverse diff))
                     ;; Intersection: keys in both (using al1 values)
                     (let ((inter nil))
                       (dolist (pair al1)
                         (when (assq (car pair) al2)
                           (setq inter (cons pair inter))))
                       (nreverse inter))
                     ;; Intersection with both values as pair
                     (let ((inter nil))
                       (dolist (pair al1)
                         (let ((other (assq (car pair) al2)))
                           (when other
                             (setq inter (cons (list (car pair) (cdr pair) (cdr other)) inter)))))
                       (nreverse inter))
                     ;; Symmetric difference
                     (let ((sym-diff nil))
                       (dolist (pair al1)
                         (unless (assq (car pair) al2)
                           (setq sym-diff (cons pair sym-diff))))
                       (dolist (pair al2)
                         (unless (assq (car pair) al1)
                           (setq sym-diff (cons pair sym-diff))))
                       (sort (nreverse sym-diff)
                             (lambda (a b) (string< (symbol-name (car a))
                                                     (symbol-name (car b))))))
                     ;; Union (al1 takes precedence for shared keys)
                     (let ((result (copy-alist al1)))
                       (dolist (pair al2)
                         (unless (assq (car pair) result)
                           (setq result (append result (list pair)))))
                       (sort result
                             (lambda (a b) (string< (symbol-name (car a))
                                                     (symbol-name (car b))))))))"#;
    let expect = expect_test::expect![
        r#""OK (((a . 1) (c . 3)) ((e . 50) (f . 60)) ((b . 2) (d . 4)) ((b 2 20) (d 4 40)) ((a . 1) (c . 3) (e . 50) (f . 60)) ((a . 1) (b . 2) (c . 3) (d . 4) (e . 50) (f . 60)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist zipping: combine two lists into an alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_zip_and_unzip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (list
  ;; Zip two lists into alist
  (let ((keys '(a b c d e))
        (vals '(1 2 3 4 5)))
    (cl-mapcar #'cons keys vals))

  ;; Zip with unequal lengths (shorter determines length)
  (let ((keys '(a b c))
        (vals '(1 2 3 4 5)))
    (cl-mapcar #'cons keys vals))

  (let ((keys '(a b c d e))
        (vals '(1 2)))
    (cl-mapcar #'cons keys vals))

  ;; Unzip: alist to two lists
  (let ((al '((a . 1) (b . 2) (c . 3))))
    (list (mapcar #'car al)
          (mapcar #'cdr al)))

  ;; Zip with index
  (let ((items '("apple" "banana" "cherry")))
    (let ((i 0) (result nil))
      (dolist (item items)
        (setq result (cons (cons i item) result))
        (setq i (1+ i)))
      (nreverse result)))

  ;; Transpose: alist of lists -> list of alists
  (let ((table '((name . ("Alice" "Bob" "Carol"))
                  (age . (30 25 35))
                  (city . ("Boston" "NYC" "Chicago")))))
    (let ((n (length (cdar table)))
          (result nil))
      (dotimes (i n)
        (let ((row nil))
          (dolist (col table)
            (setq row (cons (cons (car col) (nth i (cdr col))) row)))
          (setq result (cons (nreverse row) result))))
      (nreverse result)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b . 2) (c . 3) (d . 4) (e . 5)) ((a . 1) (b . 2) (c . 3)) ((a . 1) (b . 2)) ((a b c) (1 2 3)) ((0 . \"apple\") (1 . \"banana\") (2 . \"cherry\")) (((name . \"Alice\") (age . 30) (city . \"Boston\")) ((name . \"Bob\") (age . 25) (city . \"NYC\")) ((name . \"Carol\") (age . 35) (city . \"Chicago\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setf on alist-get: generalized variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setf_alist_get_generalized_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; setf existing key
  (let ((al (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
    (setf (alist-get 'b al) 200)
    (mapcar #'cdr al))

  ;; setf new key: prepends to alist
  (let ((al (list (cons 'a 1))))
    (setf (alist-get 'z al) 999)
    (sort (copy-sequence al)
          (lambda (x y) (string< (symbol-name (car x))
                                  (symbol-name (car y))))))

  ;; setf with REMOVE=t and setting to nil removes entry
  (let ((al (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
    (setf (alist-get 'b al nil t) nil)
    al)

  ;; Multiple setf operations building up
  (let ((al nil))
    (setf (alist-get 'x al) 10)
    (setf (alist-get 'y al) 20)
    (setf (alist-get 'z al) 30)
    (setf (alist-get 'x al) 100)
    (list (alist-get 'x al)
          (alist-get 'y al)
          (alist-get 'z al)
          (length al)))

  ;; setf with TESTFN for string alists
  (let ((al (list (cons "name" "Alice") (cons "age" "30"))))
    (setf (alist-get "name" al nil nil #'equal) "Bob")
    (mapcar #'cdr al)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 200 3) ((a . 1) (z . 999)) ((a . 1) (c . 3)) (100 20 30 3) (\"Bob\" \"30\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist with non-cons elements (atoms in alist spine)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_with_atom_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((mixed '((a . 1) foo (b . 2) nil (c . 3) 42)))
                    (list
                     ;; assq skips non-cons elements
                     (assq 'a mixed)
                     (assq 'b mixed)
                     (assq 'c mixed)
                     (assq 'foo mixed)
                     ;; assoc also skips non-cons
                     (assoc 'a mixed)
                     ;; copy-alist handles non-cons elements
                     (let ((cp (copy-alist mixed)))
                       (list (equal mixed cp)
                             (length cp)))
                     ;; Count only cons-pair elements
                     (length (seq-filter #'consp mixed))
                     ;; Extract just the proper key-value pairs
                     (seq-filter #'consp mixed)))"#;
    let expect = expect_test::expect![
        r#""OK ((a . 1) (b . 2) (c . 3) nil (a . 1) (t 6) 3 ((a . 1) (b . 2) (c . 3)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_alist_shallow_copy_and_malformed_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fcopy_alist checks LISTP first, then copies the alist
    // spine via Fcopy_sequence, then copies only cons elements one level deep.
    // Non-cons elements remain shared, and improper-tail errors come from
    // copy-sequence's final CHECK_LIST_END.
    let form = r#"
(list
 (condition-case e
     (copy-alist 'atom)
   (error (list (car e) (cdr e))))
 (let* ((key (list 'k))
        (value (list 'v))
        (pair (cons key value))
        (loose (list 'loose))
        (alist (list pair loose (cons 'x (list 'y))))
        (copy (copy-alist alist)))
   (setcar (car copy) 'new-key)
   (setcdr (car copy) 'new-value)
   (list
    alist
    copy
    (eq alist copy)
    (eq (car alist) (car copy))
    (eq (caar alist) key)
    (eq (cdar alist) value)
    (eq (cadr alist) (cadr copy))))
 (let ((alist (cons (cons 'a 1) (cons 'loose 'tail))))
   (list
    (condition-case e
        (copy-alist alist)
      (error (list (car e) (cdr e))))
    alist)))
"#;

    let expect = expect_test::expect![
        r#""OK ((wrong-type-argument (listp atom)) ((((k) v) (loose) (x y)) ((new-key . new-value) (loose) (x y)) nil nil t t nil) ((wrong-type-argument (listp tail)) ((a . 1) loose . tail)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
