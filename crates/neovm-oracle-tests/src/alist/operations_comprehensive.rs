//! Oracle parity tests for comprehensive alist operations:
//! `assoc` with TEST parameter, `rassoc`/`rassq` with various types,
//! `alist-get` with all parameters, `copy-alist` deep vs shallow semantics,
//! nested alists, insertion order preservation, and merge/update patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// assoc with custom TEST parameter — numeric, string, and predicate-based
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_test_param_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // assoc with various TESTFN arguments including lambda predicates
    let form = r#"(let ((num-alist '((1.0 . "one") (2.0 . "two") (3.0 . "three") (2.5 . "two-half")))
                        (str-alist '(("HELLO" . 1) ("World" . 2) ("foo" . 3) ("BAR" . 4)))
                        (sym-alist '((apple . red) (banana . yellow) (cherry . red) (date . brown))))
                    (list
                     ;; assoc with = for numeric comparison (not eql)
                     (assoc 2.0 num-alist #'=)
                     (assoc 2.0 num-alist)
                     ;; assoc with string-equal (case-sensitive)
                     (assoc "HELLO" str-alist #'string-equal)
                     (assoc "hello" str-alist #'string-equal)
                     ;; assoc with string-equal-ignore-case
                     (assoc "hello" str-alist #'string-equal-ignore-case)
                     (assoc "bar" str-alist #'string-equal-ignore-case)
                     ;; assoc with custom lambda — match prefix
                     (assoc "hel"
                            str-alist
                            (lambda (key elt)
                              (and (>= (length elt) (length key))
                                   (string-equal-ignore-case
                                    key (substring elt 0 (length key))))))
                     ;; assoc with eq (like assq but via testfn)
                     (assoc 'banana sym-alist #'eq)
                     ;; assoc with a testfn that reverses comparison order
                     (assoc 'red
                            sym-alist
                            (lambda (key elt) (eq elt key)))
                     ;; nil testfn falls back to equal
                     (assoc "HELLO" str-alist nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2.0 . \"two\") (2.0 . \"two\") (\"HELLO\" . 1) nil (\"HELLO\" . 1) (\"BAR\" . 4) nil (banana . yellow) nil (\"HELLO\" . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rassoc and rassq with various value types — lists, vectors, mixed
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rassoc_rassq_various_value_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((alist-nums '((a . 1) (b . 2) (c . 1) (d . 3) (e . 2)))
                        (alist-lists '((x . (1 2 3)) (y . (4 5 6)) (z . (1 2 3))))
                        (alist-vecs '((p . [1 2]) (q . [3 4]) (r . [1 2])))
                        (alist-mixed '((m . nil) (n . t) (o . 0) (p . "") (q . ()))))
                    (list
                     ;; rassoc with numeric values — finds first match
                     (rassoc 1 alist-nums)
                     (rassoc 2 alist-nums)
                     (rassoc 99 alist-nums)
                     ;; rassq with numeric values — eq comparison for fixnums
                     (rassq 1 alist-nums)
                     (rassq 3 alist-nums)
                     ;; rassoc with list values (uses equal, deep comparison)
                     (rassoc '(1 2 3) alist-lists)
                     (rassoc '(4 5 6) alist-lists)
                     (rassoc '(7 8 9) alist-lists)
                     ;; rassq with list values — eq, so won't find copy
                     (rassq '(1 2 3) alist-lists)
                     ;; rassoc with vector values
                     (rassoc [1 2] alist-vecs)
                     (rassoc [3 4] alist-vecs)
                     ;; rassq won't find vector copies
                     (rassq [1 2] alist-vecs)
                     ;; Edge: nil vs () vs empty string vs 0
                     (rassoc nil alist-mixed)
                     (rassq nil alist-mixed)
                     (rassoc t alist-mixed)
                     (rassq t alist-mixed)
                     (rassoc 0 alist-mixed)
                     (rassoc "" alist-mixed)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a . 1) (b . 2) nil (a . 1) (d . 3) (x 1 2 3) (y 4 5 6) nil nil (p . [1 2]) (q . [3 4]) nil (m) (m) (n . t) (n . t) (o . 0) (p . \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get with KEY, ALIST, DEFAULT, REMOVE, TESTFN — all combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_get_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((al '((name . "Alice") (age . 30) (active . t)
                              (score . nil) (tags . (a b c)))))
                    (list
                     ;; Basic lookup — key found
                     (alist-get 'name al)
                     (alist-get 'age al)
                     (alist-get 'tags al)
                     ;; Key not found, no default
                     (alist-get 'missing al)
                     ;; Key not found, with default
                     (alist-get 'missing al "fallback")
                     (alist-get 'missing al 42)
                     (alist-get 'missing al '(default list))
                     ;; Key found but value is nil — without REMOVE
                     (alist-get 'score al)
                     (alist-get 'score al 'default-val)
                     ;; Key found but value is nil — with REMOVE=t
                     ;; REMOVE means "treat nil values as absent"
                     (alist-get 'score al nil t)
                     (alist-get 'score al 'default-val t)
                     ;; Key found with non-nil value — REMOVE doesn't affect
                     (alist-get 'name al nil t)
                     (alist-get 'active al nil t)
                     ;; TESTFN parameter — custom comparison
                     (let ((str-al '(("Name" . "Bob") ("AGE" . 25))))
                       (list
                        (alist-get "Name" str-al nil nil #'equal)
                        (alist-get "name" str-al nil nil #'string-equal-ignore-case)
                        (alist-get "age" str-al nil nil #'string-equal-ignore-case)
                        (alist-get "missing" str-al 'nope nil #'equal)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 30 (a b c) nil \"fallback\" 42 (default list) nil nil nil nil \"Alice\" t (\"Bob\" \"Bob\" 25 nope))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-alist: deep vs shallow copy semantics, cons cell independence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_deep_shallow_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((inner-list (list 1 2 3))
                         (inner-vec (vector 10 20))
                         (orig (list (cons 'a inner-list)
                                     (cons 'b "hello")
                                     (cons 'c inner-vec)
                                     (cons 'd 42)
                                     (cons 'e nil)))
                         (copy (copy-alist orig)))
                    (list
                     ;; Top-level spine is different (not eq)
                     (eq orig copy)
                     ;; Each cons pair in copy is a new cons
                     (eq (car orig) (car copy))
                     (eq (nth 1 orig) (nth 1 copy))
                     ;; But values (cdrs) are shared (shallow copy)
                     (eq (cdr (assq 'a orig)) (cdr (assq 'a copy)))
                     (eq (cdr (assq 'b orig)) (cdr (assq 'b copy)))
                     (eq (cdr (assq 'c orig)) (cdr (assq 'c copy)))
                     ;; Modifying copy's cons cell doesn't affect original
                     (progn
                       (setcdr (assq 'a copy) 'replaced)
                       (list (cdr (assq 'a orig))
                             (cdr (assq 'a copy))))
                     ;; But mutating shared structure is visible in both
                     (progn
                       (aset (cdr (assq 'c orig)) 0 999)
                       (list (cdr (assq 'c orig))
                             (cdr (assq 'c copy))))
                     ;; copy-alist with nil
                     (copy-alist nil)
                     ;; copy-alist preserves order
                     (equal (mapcar #'car orig) (mapcar #'car copy))
                     ;; copy-alist with non-cons elements in alist (just copied)
                     (let* ((mixed '((a . 1) b (c . 3) nil (d . 4)))
                            (mixed-copy (copy-alist mixed)))
                       (list (equal mixed mixed-copy)
                             (eq (nth 1 mixed) (nth 1 mixed-copy))))))"#;
    let expect = expect_test::expect![
        r#""OK (nil nil nil t t t ((1 2 3) replaced) ([999 20] [999 20]) nil t (t t))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building complex nested alists and querying deeply
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_alist_construction_and_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((db (list
                             (list (cons 'id 1)
                                   (cons 'name "Alice")
                                   (cons 'address
                                         (list (cons 'street "123 Main St")
                                               (cons 'city "Boston")
                                               (cons 'zip "02101")))
                                   (cons 'scores (list 95 88 92)))
                             (list (cons 'id 2)
                                   (cons 'name "Bob")
                                   (cons 'address
                                         (list (cons 'street "456 Oak Ave")
                                               (cons 'city "Cambridge")
                                               (cons 'zip "02139")))
                                   (cons 'scores (list 78 85 90)))
                             (list (cons 'id 3)
                                   (cons 'name "Carol")
                                   (cons 'address
                                         (list (cons 'street "789 Pine Rd")
                                               (cons 'city "Boston")
                                               (cons 'zip "02102")))
                                   (cons 'scores (list 99 97 100))))))
                    ;; Deep query helper
                    (let ((get-nested (lambda (record &rest keys)
                                       (let ((val record))
                                         (dolist (k keys)
                                           (setq val (cdr (assq k val))))
                                         val)))
                          ;; Average helper
                          (avg (lambda (nums)
                                 (if nums
                                     (/ (apply #'+ nums) (length nums))
                                   0))))
                      (list
                       ;; Query nested fields
                       (funcall get-nested (nth 0 db) 'address 'city)
                       (funcall get-nested (nth 1 db) 'address 'zip)
                       (funcall get-nested (nth 2 db) 'name)
                       ;; Filter: people from Boston
                       (mapcar (lambda (r) (cdr (assq 'name r)))
                               (seq-filter
                                (lambda (r)
                                  (string= "Boston"
                                           (cdr (assq 'city (cdr (assq 'address r))))))
                                db))
                       ;; Compute average scores
                       (mapcar (lambda (r)
                                 (cons (cdr (assq 'name r))
                                       (funcall avg (cdr (assq 'scores r)))))
                               db)
                       ;; Find record by id
                       (cdr (assq 'name
                                  (seq-find (lambda (r)
                                              (= 2 (cdr (assq 'id r))))
                                            db))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Boston\" \"02139\" \"Carol\" (\"Alice\" \"Carol\") ((\"Alice\" . 91) (\"Bob\" . 84) (\"Carol\" . 98)) \"Bob\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist as ordered map — insertion order preservation, shadow behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_ordered_map_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((al nil))
                    ;; Build alist by consing onto front (LIFO order for assq)
                    (setq al (cons (cons 'x 1) al))
                    (setq al (cons (cons 'y 2) al))
                    (setq al (cons (cons 'z 3) al))
                    ;; "Update" x by shadowing (cons new pair on front)
                    (setq al (cons (cons 'x 10) al))
                    ;; Now al = ((x . 10) (z . 3) (y . 2) (x . 1))
                    (list
                     ;; assq finds the first (shadowing) entry
                     (assq 'x al)
                     ;; The old entry is still there
                     (length al)
                     ;; Collecting all entries (including shadowed)
                     (let ((xs nil))
                       (dolist (pair al)
                         (when (eq (car pair) 'x)
                           (setq xs (cons (cdr pair) xs))))
                       (nreverse xs))
                     ;; "Delete" by removing all occurrences of a key
                     (let ((cleaned (seq-filter (lambda (pair)
                                                  (not (eq (car pair) 'x)))
                                                al)))
                       (list (length cleaned)
                             (assq 'x cleaned)
                             (assq 'y cleaned)
                             (assq 'z cleaned)))
                     ;; Insertion order of keys (first occurrence)
                     (let ((seen nil)
                           (order nil))
                       (dolist (pair (reverse al))
                         (unless (memq (car pair) seen)
                           (setq order (cons (car pair) order))
                           (setq seen (cons (car pair) seen))))
                       (nreverse order))
                     ;; Keys in alist order (with duplicates)
                     (mapcar #'car al)))"#;
    let expect = expect_test::expect![
        r#""OK ((x . 10) 4 (10 1) (2 nil (y . 2) (z . 3)) (x y z) (x z y x))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist merge, update, and diff patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_merge_update_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((defaults '((color . "blue") (size . 12) (bold . nil)
                                     (font . "mono") (indent . 4)))
                        (user '((color . "red") (size . 14) (newkey . "yes")))
                        (override '((bold . t) (indent . 2) (font . "sans"))))
                    ;; Merge: user overrides defaults, then override overrides that
                    (let ((merge (lambda (base updates)
                                   "Return new alist: updates shadow base."
                                   (let ((result (copy-alist base)))
                                     (dolist (pair updates)
                                       (let ((existing (assq (car pair) result)))
                                         (if existing
                                             (setcdr existing (cdr pair))
                                           (setq result (append result (list (cons (car pair) (cdr pair))))))))
                                     result))))
                      (let* ((step1 (funcall merge defaults user))
                             (step2 (funcall merge step1 override)))
                        (list
                         ;; After merging user into defaults
                         (cdr (assq 'color step1))
                         (cdr (assq 'size step1))
                         (cdr (assq 'bold step1))
                         (cdr (assq 'newkey step1))
                         ;; After merging override into step1
                         (cdr (assq 'bold step2))
                         (cdr (assq 'indent step2))
                         (cdr (assq 'font step2))
                         (cdr (assq 'color step2))
                         ;; Diff: keys in step2 not in defaults
                         (let ((diff nil))
                           (dolist (pair step2)
                             (unless (assq (car pair) defaults)
                               (setq diff (cons pair diff))))
                           (nreverse diff))
                         ;; Changed: keys with different values
                         (let ((changed nil))
                           (dolist (pair step2)
                             (let ((orig (assq (car pair) defaults)))
                               (when (and orig (not (equal (cdr orig) (cdr pair))))
                                 (setq changed (cons (list (car pair)
                                                           (cdr orig)
                                                           (cdr pair))
                                                     changed)))))
                           (nreverse changed))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"red\" 14 nil \"yes\" t 2 \"sans\" \"red\" ((newkey . \"yes\")) ((color \"blue\" \"red\") (size 12 14) (bold nil t) (font \"mono\" \"sans\") (indent 4 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist as multi-map (multiple values per key) with grouping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_multimap_grouping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((events '((mon . "meeting") (tue . "code") (mon . "lunch")
                                   (wed . "deploy") (tue . "review") (mon . "standup")
                                   (wed . "test") (thu . "retro") (tue . "deploy"))))
                    ;; Group by key: day -> list of events
                    (let ((grouped nil))
                      (dolist (pair events)
                        (let ((existing (assq (car pair) grouped)))
                          (if existing
                              (setcdr existing (append (cdr existing) (list (cdr pair))))
                            (setq grouped (cons (list (car pair) (cdr pair)) grouped)))))
                      (let ((sorted-grouped (sort (copy-sequence grouped)
                                                   (lambda (a b) (string< (symbol-name (car a))
                                                                          (symbol-name (car b)))))))
                        (list
                         ;; All groups sorted by day
                         sorted-grouped
                         ;; Count per day
                         (mapcar (lambda (g) (cons (car g) (length (cdr g))))
                                 sorted-grouped)
                         ;; Days with more than 2 events
                         (mapcar #'car
                                 (seq-filter (lambda (g) (> (length (cdr g)) 2))
                                             sorted-grouped))
                         ;; Flatten back: all events in day-sorted order
                         (apply #'append
                                (mapcar (lambda (g)
                                          (mapcar (lambda (e) (cons (car g) e))
                                                  (cdr g)))
                                        sorted-grouped))
                         ;; Unique events across all days
                         (let ((uniq nil))
                           (dolist (pair events)
                             (unless (member (cdr pair) uniq)
                               (setq uniq (cons (cdr pair) uniq))))
                           (sort (nreverse uniq) #'string<))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((mon \"meeting\" \"lunch\" \"standup\") (thu \"retro\") (tue \"code\" \"review\" \"deploy\") (wed \"deploy\" \"test\")) ((mon . 3) (thu . 1) (tue . 3) (wed . 2)) (mon tue) ((mon . \"meeting\") (mon . \"lunch\") (mon . \"standup\") (thu . \"retro\") (tue . \"code\") (tue . \"review\") (tue . \"deploy\") (wed . \"deploy\") (wed . \"test\")) (\"code\" \"deploy\" \"lunch\" \"meeting\" \"retro\" \"review\" \"standup\" \"test\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist-based bidirectional map and inversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_bidirectional_map_inversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((forward '((a . 1) (b . 2) (c . 3) (d . 1) (e . 4)))
                        ;; Invert: value -> list-of-keys
                        (invert-multi
                         (lambda (al)
                           (let ((inv nil))
                             (dolist (pair al)
                               (let ((existing (assoc (cdr pair) inv)))
                                 (if existing
                                     (setcdr existing (append (cdr existing)
                                                              (list (car pair))))
                                   (setq inv (cons (list (cdr pair) (car pair)) inv)))))
                             (sort inv (lambda (a b) (< (car a) (car b)))))))
                        ;; Invert simple: value -> first key (lossy)
                        (invert-simple
                         (lambda (al)
                           (mapcar (lambda (pair) (cons (cdr pair) (car pair))) al))))
                    (let ((multi-inv (funcall invert-multi forward))
                          (simple-inv (funcall invert-simple forward)))
                      (list
                       ;; Multi-inversion preserves all keys
                       multi-inv
                       ;; Value 1 maps to multiple keys
                       (cdr (assoc 1 multi-inv))
                       ;; Value 4 maps to one key
                       (cdr (assoc 4 multi-inv))
                       ;; Simple inversion (last wins for duplicate values)
                       simple-inv
                       ;; Round-trip: invert(invert(al)) — not identity because of dups
                       (let* ((inv1 (funcall invert-simple forward))
                              (inv2 (funcall invert-simple inv1)))
                         (list (length forward)
                               (length inv1)
                               (length inv2)))
                       ;; Check bijectivity: are all values unique?
                       (let ((vals (mapcar #'cdr forward))
                             (unique t))
                         (while (and vals unique)
                           (when (member (car vals) (cdr vals))
                             (setq unique nil))
                           (setq vals (cdr vals)))
                         unique))))"#;
    let expect = expect_test::expect![
        r#""OK (((1 a d) (2 b) (3 c) (4 e)) (a d) (e) ((1 . a) (2 . b) (3 . c) (1 . d) (4 . e)) (5 5 5) nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get as setf place and complex alist surgery
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_setf_surgery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setf on alist-get to modify alist in place
    let form = r#"(let ((al (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
                    (list
                     ;; setf existing key
                     (progn (setf (alist-get 'b al) 20) (copy-alist al))
                     ;; setf non-existing key — adds it
                     (progn (setf (alist-get 'd al) 40) (copy-alist al))
                     ;; setf to nil
                     (progn (setf (alist-get 'a al) nil) (copy-alist al))
                     ;; setf with REMOVE — setting to nil with remove=t deletes entry
                     (let ((al2 (list (cons 'x 10) (cons 'y 20) (cons 'z 30))))
                       (setf (alist-get 'y al2 nil t) nil)
                       al2)
                     ;; Multiple setf operations
                     (let ((al3 nil))
                       (setf (alist-get 'first al3) 1)
                       (setf (alist-get 'second al3) 2)
                       (setf (alist-get 'third al3) 3)
                       (setf (alist-get 'second al3) 22)
                       (list al3
                             (alist-get 'first al3)
                             (alist-get 'second al3)
                             (alist-get 'third al3)))))"#;
    let expect = expect_test::expect![
        r#""OK (((a . 1) (b . 20) (c . 3)) ((d . 40) (a . 1) (b . 20) (c . 3)) ((d . 40) (a) (b . 20) (c . 3)) ((x . 10) (z . 30)) (((third . 3) (second . 22) (first . 1)) 1 22 3))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assoc-string and case-insensitive alist patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_string_case_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((headers '(("Content-Type" . "text/html")
                                    ("X-Custom" . "foo")
                                    ("content-length" . "1234")
                                    ("Accept" . "*/*")
                                    ("AUTHORIZATION" . "Bearer xyz"))))
                    (list
                     ;; assoc-string case-sensitive (default)
                     (assoc-string "Content-Type" headers)
                     (assoc-string "content-type" headers)
                     ;; assoc-string case-insensitive
                     (assoc-string "content-type" headers t)
                     (assoc-string "CONTENT-LENGTH" headers t)
                     (assoc-string "authorization" headers t)
                     ;; Not found
                     (assoc-string "X-Missing" headers t)
                     ;; Empty string key
                     (assoc-string "" headers)
                     ;; Build case-insensitive lookup helper
                     (let ((ci-get (lambda (key al)
                                     (cdr (assoc-string key al t)))))
                       (list
                        (funcall ci-get "content-type" headers)
                        (funcall ci-get "x-custom" headers)
                        (funcall ci-get "ACCEPT" headers)))
                     ;; Duplicate keys with different casing
                     (let ((dupes '(("Key" . "first") ("KEY" . "second") ("key" . "third"))))
                       (list
                        (assoc-string "Key" dupes)
                        (assoc-string "key" dupes t)
                        (assoc-string "KEY" dupes)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Content-Type\" . \"text/html\") nil (\"Content-Type\" . \"text/html\") (\"content-length\" . \"1234\") (\"AUTHORIZATION\" . \"Bearer xyz\") nil nil (\"text/html\" \"foo\" \"*/*\") ((\"Key\" . \"first\") (\"Key\" . \"first\") (\"KEY\" . \"second\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
