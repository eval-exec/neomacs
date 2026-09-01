//! Complex oracle tests for database-like patterns in Elisp.
//!
//! Tests in-memory table CRUD, index-based lookup, join operations,
//! aggregation (group-by with sum/count/avg), transaction log with undo,
//! and composable query builder predicates.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// In-memory table with insert/select/update/delete
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_table_crud() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent a table as a list of alists (rows).
    // Each row is ((col1 . val1) (col2 . val2) ...).
    let form = r#"(let ((table nil)
                        (next-id 1))
                    (let ((insert-row
                           (lambda (name age)
                             (let ((row (list (cons 'id next-id)
                                             (cons 'name name)
                                             (cons 'age age))))
                               (setq next-id (1+ next-id))
                               (setq table (cons row table))
                               row)))
                          (select-by
                           (lambda (col val)
                             (let ((results nil))
                               (dolist (row table)
                                 (when (equal (cdr (assq col row)) val)
                                   (setq results (cons row results))))
                               (nreverse results))))
                          (update-row
                           (lambda (id col new-val)
                             (dolist (row table)
                               (when (= (cdr (assq 'id row)) id)
                                 (let ((cell (assq col row)))
                                   (when cell
                                     (setcdr cell new-val)))))))
                          (delete-row
                           (lambda (id)
                             (setq table
                                   (let ((result nil))
                                     (dolist (row table)
                                       (unless (= (cdr (assq 'id row)) id)
                                         (setq result (cons row result))))
                                     (nreverse result))))))
                      ;; Insert
                      (funcall insert-row "Alice" 30)
                      (funcall insert-row "Bob" 25)
                      (funcall insert-row "Charlie" 30)
                      (funcall insert-row "Diana" 28)
                      (let ((after-insert (length table)))
                        ;; Select
                        (let ((age-30
                               (mapcar (lambda (row) (cdr (assq 'name row)))
                                       (funcall select-by 'age 30))))
                          ;; Update
                          (funcall update-row 2 'age 26)
                          (let ((bob-age
                                 (cdr (assq 'age
                                            (car (funcall select-by 'name "Bob"))))))
                            ;; Delete
                            (funcall delete-row 3)
                            (let ((after-delete (length table)))
                              (list after-insert age-30 bob-age
                                    after-delete)))))))"#;
    let expect = expect_test::expect![[r#""OK (4 (\"Charlie\" \"Alice\") 26 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Index-based lookup (hash table for fast access)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_index_based_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a primary key index (hash table mapping id -> row)
    // and a secondary index (hash table mapping name -> list of ids).
    let form = r#"(let ((rows '(((id . 1) (name . "Alice") (dept . "eng"))
                                ((id . 2) (name . "Bob") (dept . "sales"))
                                ((id . 3) (name . "Alice") (dept . "sales"))
                                ((id . 4) (name . "Charlie") (dept . "eng"))
                                ((id . 5) (name . "Bob") (dept . "eng"))))
                        (pk-index (make-hash-table))
                        (name-index (make-hash-table :test 'equal)))
                    ;; Build indices
                    (dolist (row rows)
                      (puthash (cdr (assq 'id row)) row pk-index)
                      (let* ((name (cdr (assq 'name row)))
                             (existing (gethash name name-index)))
                        (puthash name
                                 (cons (cdr (assq 'id row)) existing)
                                 name-index)))
                    ;; Lookup by primary key
                    (let ((row3 (gethash 3 pk-index)))
                      ;; Lookup by name index
                      (let ((alice-ids (sort (gethash "Alice" name-index) #'<))
                            (bob-ids (sort (gethash "Bob" name-index) #'<)))
                        ;; Resolve ids back to rows
                        (let ((alice-depts
                               (mapcar (lambda (id)
                                         (cdr (assq 'dept (gethash id pk-index))))
                                       alice-ids)))
                          (list
                            (cdr (assq 'name row3))
                            alice-ids
                            bob-ids
                            (sort alice-depts #'string<))))))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" (1 3) (2 5) (\"eng\" \"sales\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Join operation between two tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_join_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inner join employees with departments on dept-id.
    let form = r#"(let ((employees '(((id . 1) (name . "Alice") (dept-id . 10))
                                     ((id . 2) (name . "Bob") (dept-id . 20))
                                     ((id . 3) (name . "Charlie") (dept-id . 10))
                                     ((id . 4) (name . "Diana") (dept-id . 30))
                                     ((id . 5) (name . "Eve") (dept-id . 20))))
                        (departments '(((dept-id . 10) (dept-name . "Engineering"))
                                      ((dept-id . 20) (dept-name . "Sales"))
                                      ((dept-id . 30) (dept-name . "Marketing")))))
                    ;; Build department lookup
                    (let ((dept-map (make-hash-table)))
                      (dolist (dept departments)
                        (puthash (cdr (assq 'dept-id dept))
                                 (cdr (assq 'dept-name dept))
                                 dept-map))
                      ;; Inner join: for each employee, look up department name
                      (let ((joined nil))
                        (dolist (emp employees)
                          (let ((dept-name (gethash (cdr (assq 'dept-id emp)) dept-map)))
                            (when dept-name
                              (setq joined
                                    (cons (list (cdr (assq 'name emp)) dept-name)
                                          joined)))))
                        (nreverse joined))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Engineering\") (\"Bob\" \"Sales\") (\"Charlie\" \"Engineering\") (\"Diana\" \"Marketing\") (\"Eve\" \"Sales\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Aggregation (group-by with sum/count/avg)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_group_by_aggregation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group sales records by product, compute count, sum, and average.
    let form = r#"(let ((sales '(((product . "A") (amount . 100))
                                  ((product . "B") (amount . 200))
                                  ((product . "A") (amount . 150))
                                  ((product . "C") (amount . 300))
                                  ((product . "B") (amount . 250))
                                  ((product . "A") (amount . 200))
                                  ((product . "C") (amount . 100)))))
                    ;; Group by product
                    (let ((groups (make-hash-table :test 'equal)))
                      (dolist (sale sales)
                        (let* ((prod (cdr (assq 'product sale)))
                               (amt (cdr (assq 'amount sale)))
                               (existing (gethash prod groups)))
                          (puthash prod (cons amt existing) groups)))
                      ;; Aggregate each group
                      (let ((results nil))
                        (maphash
                         (lambda (prod amounts)
                           (let ((count (length amounts))
                                 (total (apply #'+ amounts)))
                             (setq results
                                   (cons (list prod
                                               (cons 'count count)
                                               (cons 'sum total)
                                               (cons 'avg (/ total count)))
                                         results))))
                         groups)
                        ;; Sort by product name for deterministic output
                        (sort results
                              (lambda (a b) (string< (car a) (car b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"A\" (count . 3) (sum . 450) (avg . 150)) (\"B\" (count . 2) (sum . 450) (avg . 225)) (\"C\" (count . 2) (sum . 400) (avg . 200)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transaction log with undo support
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_transaction_log_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Maintain a key-value store with a transaction log.
    // Each mutation is logged with an undo entry. Rollback replays
    // undo entries in reverse.
    let form = r#"(let ((store (make-hash-table :test 'equal))
                        (tx-log nil))
                    (let ((tx-set
                           (lambda (key val)
                             (let ((old-val (gethash key store 'neovm--db-absent)))
                               (setq tx-log
                                     (cons (list 'undo-set key old-val) tx-log))
                               (puthash key val store))))
                          (tx-del
                           (lambda (key)
                             (let ((old-val (gethash key store 'neovm--db-absent)))
                               (setq tx-log
                                     (cons (list 'undo-del key old-val) tx-log))
                               (remhash key store))))
                          (tx-rollback
                           (lambda ()
                             (dolist (entry tx-log)
                               (let ((op (car entry))
                                     (key (cadr entry))
                                     (old-val (caddr entry)))
                                 (cond
                                   ((eq op 'undo-set)
                                    (if (eq old-val 'neovm--db-absent)
                                        (remhash key store)
                                      (puthash key old-val store)))
                                   ((eq op 'undo-del)
                                    (unless (eq old-val 'neovm--db-absent)
                                      (puthash key old-val store))))))
                             (setq tx-log nil))))
                      ;; Initial data
                      (puthash "x" 10 store)
                      (puthash "y" 20 store)
                      (let ((before (list (gethash "x" store) (gethash "y" store))))
                        ;; Transaction: modify x, add z, delete y
                        (funcall tx-set "x" 99)
                        (funcall tx-set "z" 42)
                        (funcall tx-del "y")
                        (let ((during (list (gethash "x" store)
                                            (gethash "y" store)
                                            (gethash "z" store))))
                          ;; Rollback
                          (funcall tx-rollback)
                          (let ((after (list (gethash "x" store)
                                             (gethash "y" store)
                                             (gethash "z" store))))
                            (list before during after))))))"#;
    let expect = expect_test::expect![[r#""OK ((10 20) (99 nil 42) (10 20 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Query builder pattern (composable predicates)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_query_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build composable query predicates that can be combined with
    // AND/OR/NOT and applied to a dataset.
    let form = r#"(let ((data '(((name . "Alice") (age . 30) (dept . "eng"))
                                ((name . "Bob") (age . 25) (dept . "sales"))
                                ((name . "Charlie") (age . 35) (dept . "eng"))
                                ((name . "Diana") (age . 28) (dept . "marketing"))
                                ((name . "Eve") (age . 32) (dept . "eng"))
                                ((name . "Frank") (age . 22) (dept . "sales")))))
                    (let ((where-eq
                           (lambda (col val)
                             (lambda (row) (equal (cdr (assq col row)) val))))
                          (where-gt
                           (lambda (col val)
                             (lambda (row) (> (cdr (assq col row)) val))))
                          (where-lt
                           (lambda (col val)
                             (lambda (row) (< (cdr (assq col row)) val))))
                          (q-and
                           (lambda (p1 p2)
                             (lambda (row) (and (funcall p1 row) (funcall p2 row)))))
                          (q-or
                           (lambda (p1 p2)
                             (lambda (row) (or (funcall p1 row) (funcall p2 row)))))
                          (q-not
                           (lambda (pred)
                             (lambda (row) (not (funcall pred row)))))
                          (run-query
                           (lambda (pred rows)
                             (let ((results nil))
                               (dolist (row rows)
                                 (when (funcall pred row)
                                   (setq results (cons (cdr (assq 'name row))
                                                       results))))
                               (sort (nreverse results) #'string<)))))
                      (list
                        ;; Engineers
                        (funcall run-query
                                 (funcall where-eq 'dept "eng")
                                 data)
                        ;; Engineers over 30
                        (funcall run-query
                                 (funcall q-and
                                          (funcall where-eq 'dept "eng")
                                          (funcall where-gt 'age 30))
                                 data)
                        ;; Sales OR marketing
                        (funcall run-query
                                 (funcall q-or
                                          (funcall where-eq 'dept "sales")
                                          (funcall where-eq 'dept "marketing"))
                                 data)
                        ;; NOT engineers, age < 30
                        (funcall run-query
                                 (funcall q-and
                                          (funcall q-not (funcall where-eq 'dept "eng"))
                                          (funcall where-lt 'age 30))
                                 data))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Charlie\" \"Eve\") (\"Charlie\" \"Eve\") (\"Bob\" \"Diana\" \"Frank\") (\"Bob\" \"Diana\" \"Frank\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
