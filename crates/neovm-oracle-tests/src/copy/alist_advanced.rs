//! Advanced oracle parity tests for `copy-alist`:
//! deep vs shallow copy semantics, modification independence,
//! nested alists, merge, difference, record patterns, schema
//! migration, and serialization roundtrips.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Deep vs shallow: cons cells are copied, but cdr values are shared
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_deep_vs_shallow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // copy-alist copies the top-level cons cells but shares the cdr values.
    // Mutating the cdr of the copy's cell does NOT affect original (new cell).
    // But if the cdr is itself a cons, the inner structure IS shared.
    let form = r#"(let* ((inner (list 1 2 3))
                          (orig (list (cons 'a inner) (cons 'b "hello")))
                          (cp (copy-alist orig)))
                    ;; The top-level cells are different objects
                    (let ((cells-differ (not (eq (car orig) (car cp)))))
                      ;; But the cdr values are shared (eq)
                      (let ((cdr-shared (eq (cdar orig) (cdar cp))))
                        ;; Mutate inner list through copy — affects original too
                        (setcar (cdar cp) 999)
                        (list cells-differ
                              cdr-shared
                              (cdar orig)
                              (cdar cp)))))"#;
    let expect = expect_test::expect![[r#""OK (t t (999 2 3) (999 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modification independence: setcdr on copy cell doesn't affect original
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_modification_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setcdr on the copied cell replaces the binding in the copy only.
    // The original alist retains its original values.
    let form = r#"(let* ((orig (list (cons 'x 10) (cons 'y 20) (cons 'z 30)))
                          (cp (copy-alist orig)))
                    ;; Modify every cdr in the copy
                    (setcdr (car cp) 100)
                    (setcdr (cadr cp) 200)
                    (setcdr (caddr cp) 300)
                    ;; Also push a new entry onto the copy
                    (setq cp (cons (cons 'w 400) cp))
                    (list
                     ;; Original unchanged
                     (mapcar #'cdr orig)
                     ;; Copy has new values and extra entry
                     (mapcar #'cdr cp)
                     ;; Lengths differ
                     (list (length orig) (length cp))))"#;
    let expect = expect_test::expect![[r#""OK ((10 20 30) (400 100 200 300) (3 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested alist of alists: copy-alist is shallow, inner alists shared
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_nested_alist_of_alists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An alist where values are themselves alists.
    // copy-alist copies the outer cells but inner alists are shared.
    let form = r#"(let* ((orig (list (cons 'user (list (cons 'name "Alice")
                                                         (cons 'age 30)))
                                 (cons 'settings (list (cons 'theme "dark")
                                                        (cons 'font-size 14)))))
                          (cp (copy-alist orig)))
                    ;; Inner alists are eq (shared)
                    (let ((user-shared (eq (cdr (assq 'user orig))
                                           (cdr (assq 'user cp))))
                          (settings-shared (eq (cdr (assq 'settings orig))
                                               (cdr (assq 'settings cp)))))
                      ;; Replace entire value in copy (setcdr on copied cell)
                      (setcdr (assq 'user cp) '((name . "Bob") (age . 25)))
                      (list user-shared
                            settings-shared
                            ;; Original user unchanged
                            (cdr (assq 'name (cdr (assq 'user orig))))
                            ;; Copy user changed
                            (cdr (assq 'name (cdr (assq 'user cp)))))))"#;
    let expect = expect_test::expect![[r#""OK (t t \"Alice\" \"Bob\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist merge: newer entries overwrite older ones
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two alists: entries from `updates` overwrite those in `base`.
    // Non-overlapping entries from both are preserved.
    let form = r#"(let ((base '((a . 1) (b . 2) (c . 3) (d . 4)))
                        (updates '((b . 20) (d . 40) (e . 50))))
                    (let ((merge
                           (lambda (base updates)
                             (let ((result (copy-alist base)))
                               ;; Apply updates: if key exists, replace; else append
                               (dolist (entry updates)
                                 (let ((existing (assq (car entry) result)))
                                   (if existing
                                       (setcdr existing (cdr entry))
                                     (setq result (append result (list (cons (car entry) (cdr entry))))))))
                               result))))
                      (let ((merged (funcall merge base updates)))
                        (list
                         ;; Merged result
                         merged
                         ;; Original base unchanged
                         base
                         ;; Verify specific values
                         (cdr (assq 'b merged))
                         (cdr (assq 'e merged))
                         (cdr (assq 'a merged))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b . 20) (c . 3) (d . 40) (e . 50)) ((a . 1) (b . 2) (c . 3) (d . 4)) 20 50 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist difference: entries only in one alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find entries in alist-a that have no matching key in alist-b,
    // and entries in alist-b not in alist-a.
    let form = r#"(let ((alist-a '((x . 1) (y . 2) (z . 3) (w . 4)))
                        (alist-b '((y . 20) (z . 30) (v . 50))))
                    (let ((diff
                           (lambda (a b)
                             (let ((only-a nil)
                                   (only-b nil)
                                   (common nil))
                               (dolist (entry a)
                                 (if (assq (car entry) b)
                                     (setq common (cons (car entry) common))
                                   (setq only-a (cons entry only-a))))
                               (dolist (entry b)
                                 (unless (assq (car entry) a)
                                   (setq only-b (cons entry only-b))))
                               (list (nreverse only-a)
                                     (nreverse only-b)
                                     (nreverse common))))))
                      (funcall diff alist-a alist-b)))"#;
    let expect = expect_test::expect![[r#""OK (((x . 1) (w . 4)) ((v . 50)) (y z))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist-based record with computed fields
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_computed_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use alists as records with derived/computed fields.
    let form = r#"(let ((make-rect
                         (lambda (w h)
                           (let ((r (list (cons 'width w) (cons 'height h))))
                             ;; Add computed fields
                             (setq r (append r
                                             (list (cons 'area (* w h))
                                                   (cons 'perimeter (* 2 (+ w h)))
                                                   (cons 'diagonal
                                                         (sqrt (+ (* w w) (* h h)))))))
                             r)))
                        (scale-rect
                         (lambda (rect factor)
                           (let ((w (* (cdr (assq 'width rect)) factor))
                                 (h (* (cdr (assq 'height rect)) factor)))
                             ;; Return new record, original unchanged
                             (let ((r (list (cons 'width w) (cons 'height h))))
                               (setq r (append r
                                               (list (cons 'area (* w h))
                                                     (cons 'perimeter (* 2 (+ w h)))
                                                     (cons 'diagonal
                                                           (sqrt (+ (* w w) (* h h)))))))
                               r)))))
                    (let* ((r1 (funcall make-rect 3 4))
                           (r2 (funcall scale-rect r1 2)))
                      (list (cdr (assq 'area r1))
                            (cdr (assq 'perimeter r1))
                            (cdr (assq 'diagonal r1))
                            (cdr (assq 'area r2))
                            (cdr (assq 'perimeter r2))
                            ;; Original unchanged after scaling
                            (cdr (assq 'width r1)))))"#;
    let expect = expect_test::expect![[r#""OK (12 14 5.0 48 28 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based schema migration (add/remove/rename fields)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_schema_migration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate database schema migration on alist "rows":
    // v1 -> v2: rename 'name -> 'full-name, add 'active field, remove 'legacy
    let form = r#"(let ((v1-rows (list
                                   (list (cons 'name "Alice") (cons 'score 95) (cons 'legacy t))
                                   (list (cons 'name "Bob") (cons 'score 80) (cons 'legacy nil))
                                   (list (cons 'name "Carol") (cons 'score 88) (cons 'legacy t))))
                        (migrate-v1-to-v2
                         (lambda (row)
                           (let ((result (copy-alist row)))
                             ;; Rename: name -> full-name
                             (let ((name-cell (assq 'name result)))
                               (when name-cell
                                 (setcar name-cell 'full-name)))
                             ;; Add: active (default t)
                             (setq result (append result (list (cons 'active t))))
                             ;; Remove: legacy
                             (setq result
                                   (let ((filtered nil))
                                     (dolist (entry result)
                                       (unless (eq (car entry) 'legacy)
                                         (setq filtered (cons entry filtered))))
                                     (nreverse filtered)))
                             result))))
                    (let ((v2-rows (mapcar migrate-v1-to-v2 v1-rows)))
                      (list
                       ;; v1 rows unchanged (copy-alist preserved originals)
                       (mapcar (lambda (r) (cdr (assq 'name r))) v1-rows)
                       ;; v2 has renamed field
                       (mapcar (lambda (r) (cdr (assq 'full-name r))) v2-rows)
                       ;; v2 has no legacy field
                       (mapcar (lambda (r) (assq 'legacy r)) v2-rows)
                       ;; v2 has active field
                       (mapcar (lambda (r) (cdr (assq 'active r))) v2-rows)
                       ;; Scores preserved
                       (mapcar (lambda (r) (cdr (assq 'score r))) v2-rows))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Carol\") (\"Alice\" \"Bob\" \"Carol\") (nil nil nil) (t t t) (95 80 88))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist serialization/deserialization roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_serialization_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize an alist to a flat "key=value" string list,
    // then deserialize back, and verify roundtrip fidelity.
    let form = r#"(let ((serialize
                         (lambda (alist)
                           (mapcar (lambda (entry)
                                     (format "%s=%s"
                                             (symbol-name (car entry))
                                             (prin1-to-string (cdr entry))))
                                   alist)))
                        (deserialize
                         (lambda (strings)
                           (mapcar (lambda (s)
                                     (let ((pos (string-match "=" s)))
                                       (when pos
                                         (cons (intern (substring s 0 pos))
                                               (car (read-from-string
                                                     (substring s (1+ pos))))))))
                                   strings))))
                    (let* ((original (list (cons 'name "Alice")
                                          (cons 'age 30)
                                          (cons 'scores (list 95 88 92))
                                          (cons 'active t)
                                          (cons 'meta nil)))
                           (serialized (funcall serialize original))
                           (roundtripped (funcall deserialize serialized)))
                      (list
                       ;; Serialized form
                       serialized
                       ;; Roundtrip matches original
                       (equal original roundtripped)
                       ;; Individual field checks
                       (equal (cdr (assq 'name roundtripped)) "Alice")
                       (equal (cdr (assq 'age roundtripped)) 30)
                       (equal (cdr (assq 'scores roundtripped)) '(95 88 92))
                       (equal (cdr (assq 'active roundtripped)) t)
                       (equal (cdr (assq 'meta roundtripped)) nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name=\\\"Alice\\\"\" \"age=30\" \"scores=(95 88 92)\" \"active=t\" \"meta=nil\") t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
