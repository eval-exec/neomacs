//! Oracle parity tests implementing a simple in-memory database in Elisp:
//! tables as lists of alists (rows), SELECT (filter columns), WHERE (filter
//! rows), ORDER BY, GROUP BY with aggregation (COUNT, SUM, AVG, MAX, MIN),
//! JOIN (inner join on key), INSERT, UPDATE, DELETE.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Full database engine: INSERT, SELECT with WHERE, column projection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_insert_select_where_project() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--db-tables (make-hash-table :test 'equal))
  (defvar neovm--db-next-ids (make-hash-table :test 'equal))

  (fset 'neovm--db-create-table
    (lambda (name)
      (puthash name nil neovm--db-tables)
      (puthash name 1 neovm--db-next-ids)))

  (fset 'neovm--db-insert
    (lambda (table-name row-data)
      "Insert ROW-DATA (alist without id) into TABLE-NAME. Returns the new row."
      (let* ((id (gethash table-name neovm--db-next-ids))
             (row (cons (cons 'id id) row-data))
             (existing (gethash table-name neovm--db-tables)))
        (puthash table-name (1+ id) neovm--db-next-ids)
        (puthash table-name (append existing (list row)) neovm--db-tables)
        row)))

  (fset 'neovm--db-select
    (lambda (table-name columns where-fn)
      "Select COLUMNS from TABLE-NAME where WHERE-FN returns non-nil.
       If COLUMNS is nil, select all. If WHERE-FN is nil, select all rows."
      (let ((rows (gethash table-name neovm--db-tables))
            (result nil))
        (dolist (row rows)
          (when (or (null where-fn) (funcall where-fn row))
            (if columns
                (let ((projected nil))
                  (dolist (col columns)
                    (let ((cell (assq col row)))
                      (when cell (setq projected (cons cell projected)))))
                  (setq result (cons (nreverse projected) result)))
              (setq result (cons row result)))))
        (nreverse result))))

  (unwind-protect
      (progn
        ;; Create table and insert rows
        (funcall 'neovm--db-create-table "employees")
        (funcall 'neovm--db-insert "employees"
                 '((name . "Alice") (dept . "Engineering") (salary . 90000)))
        (funcall 'neovm--db-insert "employees"
                 '((name . "Bob") (dept . "Sales") (salary . 75000)))
        (funcall 'neovm--db-insert "employees"
                 '((name . "Charlie") (dept . "Engineering") (salary . 95000)))
        (funcall 'neovm--db-insert "employees"
                 '((name . "Diana") (dept . "Marketing") (salary . 80000)))
        (funcall 'neovm--db-insert "employees"
                 '((name . "Eve") (dept . "Engineering") (salary . 85000)))
        (funcall 'neovm--db-insert "employees"
                 '((name . "Frank") (dept . "Sales") (salary . 70000)))

        (list
          ;; Select all
          (length (funcall 'neovm--db-select "employees" nil nil))
          ;; Select with WHERE: Engineering dept
          (funcall 'neovm--db-select "employees" '(name salary)
                   (lambda (row) (equal (cdr (assq 'dept row)) "Engineering")))
          ;; Select with WHERE: salary > 80000
          (funcall 'neovm--db-select "employees" '(name dept)
                   (lambda (row) (> (cdr (assq 'salary row)) 80000)))
          ;; Select specific columns from all rows
          (funcall 'neovm--db-select "employees" '(name) nil)
          ;; Compound WHERE: Engineering AND salary >= 90000
          (funcall 'neovm--db-select "employees" '(name salary)
                   (lambda (row) (and (equal (cdr (assq 'dept row)) "Engineering")
                                      (>= (cdr (assq 'salary row)) 90000))))))
    (fmakunbound 'neovm--db-create-table)
    (fmakunbound 'neovm--db-insert)
    (fmakunbound 'neovm--db-select)
    (makunbound 'neovm--db-tables)
    (makunbound 'neovm--db-next-ids)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 (((name . \"Alice\") (salary . 90000)) ((name . \"Charlie\") (salary . 95000)) ((name . \"Eve\") (salary . 85000))) (((name . \"Alice\") (dept . \"Engineering\")) ((name . \"Charlie\") (dept . \"Engineering\")) ((name . \"Eve\") (dept . \"Engineering\"))) (((name . \"Alice\")) ((name . \"Bob\")) ((name . \"Charlie\")) ((name . \"Diana\")) ((name . \"Eve\")) ((name . \"Frank\"))) (((name . \"Alice\") (salary . 90000)) ((name . \"Charlie\") (salary . 95000))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// UPDATE and DELETE operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_update_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((table (list
                   '((id . 1) (name . "Alice") (status . active) (score . 85))
                   '((id . 2) (name . "Bob") (status . active) (score . 72))
                   '((id . 3) (name . "Charlie") (status . inactive) (score . 90))
                   '((id . 4) (name . "Diana") (status . active) (score . 68))
                   '((id . 5) (name . "Eve") (status . inactive) (score . 95)))))
  (let ((db-update
         (lambda (tbl where-fn set-fn)
           "Update rows matching WHERE-FN by applying SET-FN. Returns (updated-count . new-table)."
           (let ((count 0)
                 (result nil))
             (dolist (row tbl)
               (if (funcall where-fn row)
                   (progn
                     (setq count (1+ count))
                     (setq result (cons (funcall set-fn row) result)))
                 (setq result (cons row result))))
             (cons count (nreverse result)))))
        (db-delete
         (lambda (tbl where-fn)
           "Delete rows matching WHERE-FN. Returns (deleted-count . new-table)."
           (let ((count 0)
                 (result nil))
             (dolist (row tbl)
               (if (funcall where-fn row)
                   (setq count (1+ count))
                 (setq result (cons row result))))
             (cons count (nreverse result))))))

    ;; UPDATE: set status to 'promoted for score >= 90
    (let* ((update-result
            (funcall db-update table
                     (lambda (row) (>= (cdr (assq 'score row)) 90))
                     (lambda (row)
                       (mapcar (lambda (cell)
                                 (if (eq (car cell) 'status)
                                     (cons 'status 'promoted)
                                   cell))
                               row))))
           (updated-count (car update-result))
           (table-after-update (cdr update-result)))

      ;; DELETE: remove inactive (not promoted) rows
      (let* ((delete-result
              (funcall db-delete table-after-update
                       (lambda (row) (eq (cdr (assq 'status row)) 'inactive))))
             (deleted-count (car delete-result))
             (table-after-delete (cdr delete-result)))

        (list
          ;; Update count
          updated-count
          ;; Names of promoted
          (let ((promoted nil))
            (dolist (row table-after-update)
              (when (eq (cdr (assq 'status row)) 'promoted)
                (setq promoted (cons (cdr (assq 'name row)) promoted))))
            (nreverse promoted))
          ;; Delete count
          deleted-count
          ;; Final table size
          (length table-after-delete)
          ;; Final table names
          (mapcar (lambda (row) (cdr (assq 'name row))) table-after-delete))))))"#;
    let expect = expect_test::expect![[
        r#""OK (2 (\"Charlie\" \"Eve\") 0 5 (\"Alice\" \"Bob\" \"Charlie\" \"Diana\" \"Eve\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ORDER BY: sorting by column(s)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_order_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((table (list
                   '((id . 1) (name . "Charlie") (dept . "B") (salary . 80000))
                   '((id . 2) (name . "Alice") (dept . "A") (salary . 90000))
                   '((id . 3) (name . "Eve") (dept . "A") (salary . 85000))
                   '((id . 4) (name . "Bob") (dept . "B") (salary . 95000))
                   '((id . 5) (name . "Diana") (dept . "A") (salary . 70000)))))
  (let ((order-by
         (lambda (tbl col comparator)
           "Sort table by COL using COMPARATOR."
           (sort (copy-sequence tbl)
                 (lambda (a b)
                   (funcall comparator
                            (cdr (assq col a))
                            (cdr (assq col b)))))))
        (order-by-multi
         (lambda (tbl specs)
           "Sort by multiple columns. SPECS is list of (col . comparator).
            First spec is primary, second is tiebreaker, etc."
           (sort (copy-sequence tbl)
                 (lambda (a b)
                   (let ((result nil)
                         (remaining specs))
                     (while (and remaining (null result))
                       (let* ((spec (car remaining))
                              (col (car spec))
                              (cmp (cdr spec))
                              (va (cdr (assq col a)))
                              (vb (cdr (assq col b))))
                         (cond
                           ((funcall cmp va vb) (setq result t))
                           ((funcall cmp vb va) (setq result nil) (setq remaining nil))
                           (t (setq remaining (cdr remaining))))))
                     result))))))

    (list
      ;; ORDER BY name ASC
      (mapcar (lambda (r) (cdr (assq 'name r)))
              (funcall order-by table 'name #'string<))
      ;; ORDER BY salary DESC
      (mapcar (lambda (r) (list (cdr (assq 'name r)) (cdr (assq 'salary r))))
              (funcall order-by table 'salary #'>))
      ;; ORDER BY dept ASC, salary DESC
      (mapcar (lambda (r) (list (cdr (assq 'dept r))
                                (cdr (assq 'name r))
                                (cdr (assq 'salary r))))
              (funcall order-by-multi table
                       (list (cons 'dept #'string<)
                             (cons 'salary #'>))))
      ;; ORDER BY dept ASC, name ASC
      (mapcar (lambda (r) (list (cdr (assq 'dept r)) (cdr (assq 'name r))))
              (funcall order-by-multi table
                       (list (cons 'dept #'string<)
                             (cons 'name #'string<)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Charlie\" \"Diana\" \"Eve\") ((\"Bob\" 95000) (\"Alice\" 90000) (\"Eve\" 85000) (\"Charlie\" 80000) (\"Diana\" 70000)) ((\"A\" \"Alice\" 90000) (\"A\" \"Eve\" 85000) (\"A\" \"Diana\" 70000) (\"B\" \"Bob\" 95000) (\"B\" \"Charlie\" 80000)) ((\"A\" \"Alice\") (\"A\" \"Diana\") (\"A\" \"Eve\") (\"B\" \"Bob\") (\"B\" \"Charlie\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// GROUP BY with aggregation: COUNT, SUM, AVG, MAX, MIN
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_group_by_aggregation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((sales (list
                   '((id . 1) (product . "Widget") (region . "East") (amount . 100) (qty . 5))
                   '((id . 2) (product . "Gadget") (region . "West") (amount . 200) (qty . 3))
                   '((id . 3) (product . "Widget") (region . "West") (amount . 150) (qty . 8))
                   '((id . 4) (product . "Gadget") (region . "East") (amount . 300) (qty . 2))
                   '((id . 5) (product . "Widget") (region . "East") (amount . 120) (qty . 6))
                   '((id . 6) (product . "Doohickey") (region . "East") (amount . 80) (qty . 10))
                   '((id . 7) (product . "Gadget") (region . "East") (amount . 250) (qty . 4))
                   '((id . 8) (product . "Doohickey") (region . "West") (amount . 90) (qty . 7))
                   '((id . 9) (product . "Widget") (region . "West") (amount . 180) (qty . 3)))))
  (let ((group-by
         (lambda (tbl key-col)
           "Group rows by KEY-COL. Returns hash-table of key -> list of rows."
           (let ((groups (make-hash-table :test 'equal)))
             (dolist (row tbl)
               (let* ((key (cdr (assq key-col row)))
                      (existing (gethash key groups)))
                 (puthash key (cons row existing) groups)))
             ;; Reverse each group to maintain insertion order
             (let ((keys nil))
               (maphash (lambda (k v) (setq keys (cons k keys))
                          (puthash k (nreverse v) groups))
                        groups))
             groups)))
        (agg-count (lambda (rows _col) (length rows)))
        (agg-sum (lambda (rows col)
                   (let ((total 0))
                     (dolist (row rows) (setq total (+ total (cdr (assq col row)))))
                     total)))
        (agg-avg (lambda (rows col)
                   (let ((total 0))
                     (dolist (row rows) (setq total (+ total (cdr (assq col row)))))
                     (/ total (length rows)))))
        (agg-max (lambda (rows col)
                   (let ((best nil))
                     (dolist (row rows)
                       (let ((val (cdr (assq col row))))
                         (when (or (null best) (> val best))
                           (setq best val))))
                     best)))
        (agg-min (lambda (rows col)
                   (let ((best nil))
                     (dolist (row rows)
                       (let ((val (cdr (assq col row))))
                         (when (or (null best) (< val best))
                           (setq best val))))
                     best))))

    ;; GROUP BY product: COUNT, SUM(amount), AVG(amount), MAX(amount), MIN(qty)
    (let ((by-product (funcall group-by sales 'product)))
      (let ((results nil))
        (maphash
         (lambda (product rows)
           (setq results
                 (cons (list product
                             (cons 'count (funcall agg-count rows nil))
                             (cons 'sum-amt (funcall agg-sum rows 'amount))
                             (cons 'avg-amt (funcall agg-avg rows 'amount))
                             (cons 'max-amt (funcall agg-max rows 'amount))
                             (cons 'min-qty (funcall agg-min rows 'qty)))
                       results)))
         by-product)

        ;; GROUP BY region: SUM(amount), COUNT
        (let ((by-region (funcall group-by sales 'region))
              (region-results nil))
          (maphash
           (lambda (region rows)
             (setq region-results
                   (cons (list region
                               (cons 'count (funcall agg-count rows nil))
                               (cons 'total (funcall agg-sum rows 'amount)))
                         region-results)))
           by-region)

          (list
            (sort results (lambda (a b) (string< (car a) (car b))))
            (sort region-results (lambda (a b) (string< (car a) (car b))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Doohickey\" (count . 2) (sum-amt . 170) (avg-amt . 85) (max-amt . 90) (min-qty . 7)) (\"Gadget\" (count . 3) (sum-amt . 750) (avg-amt . 250) (max-amt . 300) (min-qty . 2)) (\"Widget\" (count . 4) (sum-amt . 550) (avg-amt . 137) (max-amt . 180) (min-qty . 3))) ((\"East\" (count . 5) (total . 850)) (\"West\" (count . 4) (total . 620))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// INNER JOIN on key column
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_inner_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((employees (list
                   '((emp-id . 1) (name . "Alice") (dept-id . 100))
                   '((emp-id . 2) (name . "Bob") (dept-id . 200))
                   '((emp-id . 3) (name . "Charlie") (dept-id . 100))
                   '((emp-id . 4) (name . "Diana") (dept-id . 300))
                   '((emp-id . 5) (name . "Eve") (dept-id . 200))
                   '((emp-id . 6) (name . "Frank") (dept-id . 999))))
              (departments (list
                   '((dept-id . 100) (dept-name . "Engineering") (budget . 500000))
                   '((dept-id . 200) (dept-name . "Sales") (budget . 300000))
                   '((dept-id . 300) (dept-name . "Marketing") (budget . 200000))))
              (projects (list
                   '((proj-id . 1) (dept-id . 100) (proj-name . "Alpha"))
                   '((proj-id . 2) (dept-id . 100) (proj-name . "Beta"))
                   '((proj-id . 3) (dept-id . 200) (proj-name . "Gamma"))
                   '((proj-id . 4) (dept-id . 300) (proj-name . "Delta")))))
  (let ((inner-join
         (lambda (left right left-key right-key)
           "Inner join LEFT and RIGHT tables on LEFT-KEY = RIGHT-KEY.
            Returns list of merged rows."
           ;; Build index on right table
           (let ((right-idx (make-hash-table :test 'equal)))
             (dolist (row right)
               (let* ((key (cdr (assq right-key row)))
                      (existing (gethash key right-idx)))
                 (puthash key (cons row existing) right-idx)))
             ;; Join
             (let ((result nil))
               (dolist (lrow left)
                 (let* ((key (cdr (assq left-key lrow)))
                        (rrows (gethash key right-idx)))
                   (dolist (rrow rrows)
                     ;; Merge: all columns from left, then non-key columns from right
                     (let ((merged (copy-sequence lrow)))
                       (dolist (cell rrow)
                         (unless (assq (car cell) merged)
                           (setq merged (append merged (list cell)))))
                       (setq result (cons merged result))))))
               (nreverse result))))))

    (list
      ;; employees JOIN departments ON dept-id
      (mapcar (lambda (r) (list (cdr (assq 'name r))
                                (cdr (assq 'dept-name r))))
              (funcall inner-join employees departments 'dept-id 'dept-id))
      ;; Frank (dept-id 999) should NOT appear (no matching department)
      (let ((joined (funcall inner-join employees departments 'dept-id 'dept-id)))
        (length joined))
      ;; departments JOIN projects ON dept-id (one-to-many)
      (mapcar (lambda (r) (list (cdr (assq 'dept-name r))
                                (cdr (assq 'proj-name r))))
              (funcall inner-join departments projects 'dept-id 'dept-id))
      ;; Three-way join: employees -> departments -> projects
      (let* ((emp-dept (funcall inner-join employees departments 'dept-id 'dept-id))
             (emp-dept-proj (funcall inner-join emp-dept projects 'dept-id 'dept-id)))
        (sort
         (mapcar (lambda (r) (list (cdr (assq 'name r))
                                   (cdr (assq 'dept-name r))
                                   (cdr (assq 'proj-name r))))
                 emp-dept-proj)
         (lambda (a b) (string< (car a) (car b))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Alice\" \"Engineering\") (\"Bob\" \"Sales\") (\"Charlie\" \"Engineering\") (\"Diana\" \"Marketing\") (\"Eve\" \"Sales\")) 5 ((\"Engineering\" \"Beta\") (\"Engineering\" \"Alpha\") (\"Sales\" \"Gamma\") (\"Marketing\" \"Delta\")) ((\"Alice\" \"Engineering\" \"Beta\") (\"Alice\" \"Engineering\" \"Alpha\") (\"Bob\" \"Sales\" \"Gamma\") (\"Charlie\" \"Engineering\" \"Beta\") (\"Charlie\" \"Engineering\" \"Alpha\") (\"Diana\" \"Marketing\" \"Delta\") (\"Eve\" \"Sales\" \"Gamma\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full query pipeline: SELECT...FROM...WHERE...GROUP BY...ORDER BY
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_full_query_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((orders (list
                   '((oid . 1) (customer . "Alice") (product . "A") (amount . 100) (date . 1))
                   '((oid . 2) (customer . "Bob") (product . "B") (amount . 200) (date . 1))
                   '((oid . 3) (customer . "Alice") (product . "A") (amount . 150) (date . 2))
                   '((oid . 4) (customer . "Charlie") (product . "C") (amount . 50) (date . 2))
                   '((oid . 5) (customer . "Bob") (product . "A") (amount . 300) (date . 3))
                   '((oid . 6) (customer . "Alice") (product . "B") (amount . 250) (date . 3))
                   '((oid . 7) (customer . "Charlie") (product . "A") (amount . 175) (date . 4))
                   '((oid . 8) (customer . "Alice") (product . "C") (amount . 80) (date . 4))
                   '((oid . 9) (customer . "Bob") (product . "C") (amount . 120) (date . 5))
                   '((oid . 10) (customer . "Diana") (product . "A") (amount . 400) (date . 5)))))
  (let ((query-pipeline
         (lambda (tbl where-fn group-col agg-specs order-col order-cmp limit)
           "Full query: WHERE -> GROUP BY -> AGGREGATE -> ORDER BY -> LIMIT."
           ;; Step 1: WHERE
           (let ((filtered nil))
             (dolist (row tbl)
               (when (or (null where-fn) (funcall where-fn row))
                 (setq filtered (cons row filtered))))
             (setq filtered (nreverse filtered))
             ;; Step 2: GROUP BY
             (if (null group-col)
                 ;; No grouping: just order and limit
                 (let ((ordered (if order-col
                                    (sort (copy-sequence filtered)
                                          (lambda (a b)
                                            (funcall order-cmp
                                                     (cdr (assq order-col a))
                                                     (cdr (assq order-col b)))))
                                  filtered)))
                   (if limit (let ((result nil) (i 0))
                               (dolist (row ordered)
                                 (when (< i limit)
                                   (setq result (cons row result))
                                   (setq i (1+ i))))
                               (nreverse result))
                     ordered))
               ;; Grouping
               (let ((groups (make-hash-table :test 'equal)))
                 (dolist (row filtered)
                   (let* ((key (cdr (assq group-col row)))
                          (existing (gethash key groups)))
                     (puthash key (cons row existing) groups)))
                 ;; Step 3: AGGREGATE
                 (let ((agg-results nil))
                   (maphash
                    (lambda (key rows)
                      (let ((row-result (list (cons group-col key))))
                        (dolist (spec agg-specs)
                          (let ((agg-name (car spec))
                                (agg-type (cadr spec))
                                (agg-col  (caddr spec)))
                            (let ((val
                                   (cond
                                     ((eq agg-type 'count) (length rows))
                                     ((eq agg-type 'sum)
                                      (let ((s 0))
                                        (dolist (r rows) (setq s (+ s (cdr (assq agg-col r)))))
                                        s))
                                     ((eq agg-type 'avg)
                                      (let ((s 0))
                                        (dolist (r rows) (setq s (+ s (cdr (assq agg-col r)))))
                                        (/ s (length rows))))
                                     ((eq agg-type 'max)
                                      (let ((m nil))
                                        (dolist (r rows)
                                          (let ((v (cdr (assq agg-col r))))
                                            (when (or (null m) (> v m)) (setq m v))))
                                        m))
                                     ((eq agg-type 'min)
                                      (let ((m nil))
                                        (dolist (r rows)
                                          (let ((v (cdr (assq agg-col r))))
                                            (when (or (null m) (< v m)) (setq m v))))
                                        m)))))
                              (setq row-result (append row-result (list (cons agg-name val)))))))
                        (setq agg-results (cons row-result agg-results))))
                    groups)
                   ;; Step 4: ORDER BY
                   (let ((ordered (if order-col
                                      (sort agg-results
                                            (lambda (a b)
                                              (funcall order-cmp
                                                       (cdr (assq order-col a))
                                                       (cdr (assq order-col b)))))
                                    agg-results)))
                     ;; Step 5: LIMIT
                     (if limit
                         (let ((result nil) (i 0))
                           (dolist (row ordered)
                             (when (< i limit)
                               (setq result (cons row result))
                               (setq i (1+ i))))
                           (nreverse result))
                       ordered)))))))))

    (list
      ;; Query 1: Total amount by customer, ordered by total DESC
      (funcall query-pipeline orders nil 'customer
               '((total sum amount) (num-orders count nil))
               'total #'> nil)
      ;; Query 2: Product A orders only, grouped by customer, sum amount
      (funcall query-pipeline orders
               (lambda (r) (equal (cdr (assq 'product r)) "A"))
               'customer
               '((total sum amount) (cnt count nil))
               'total #'> nil)
      ;; Query 3: Top 3 orders by amount (no grouping)
      (mapcar (lambda (r) (list (cdr (assq 'customer r))
                                (cdr (assq 'amount r))))
              (funcall query-pipeline orders nil nil nil 'amount #'> 3))
      ;; Query 4: By product, max and min amounts
      (funcall query-pipeline orders nil 'product
               '((max-amt max amount) (min-amt min amount) (avg-amt avg amount))
               'product #'string< nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((((customer . \"Bob\") (total . 620) (num-orders . 3)) ((customer . \"Alice\") (total . 580) (num-orders . 4)) ((customer . \"Diana\") (total . 400) (num-orders . 1)) ((customer . \"Charlie\") (total . 225) (num-orders . 2))) (((customer . \"Diana\") (total . 400) (cnt . 1)) ((customer . \"Bob\") (total . 300) (cnt . 1)) ((customer . \"Alice\") (total . 250) (cnt . 2)) ((customer . \"Charlie\") (total . 175) (cnt . 1))) ((\"Diana\" 400) (\"Bob\" 300) (\"Alice\" 250)) (((product . \"A\") (max-amt . 400) (min-amt . 100) (avg-amt . 225)) ((product . \"B\") (max-amt . 250) (min-amt . 200) (avg-amt . 225)) ((product . \"C\") (max-amt . 120) (min-amt . 50) (avg-amt . 83))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HAVING: filter groups after aggregation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_having_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((transactions (list
                   '((tid . 1) (account . "A001") (type . credit) (amount . 500))
                   '((tid . 2) (account . "A002") (type . debit) (amount . 200))
                   '((tid . 3) (account . "A001") (type . credit) (amount . 300))
                   '((tid . 4) (account . "A003") (type . credit) (amount . 1000))
                   '((tid . 5) (account . "A002") (type . credit) (amount . 150))
                   '((tid . 6) (account . "A001") (type . debit) (amount . 100))
                   '((tid . 7) (account . "A003") (type . debit) (amount . 50))
                   '((tid . 8) (account . "A002") (type . credit) (amount . 400))
                   '((tid . 9) (account . "A001") (type . credit) (amount . 200)))))
  ;; Group by account, compute net balance (credits - debits)
  ;; HAVING net balance > 500
  (let ((groups (make-hash-table :test 'equal)))
    (dolist (txn transactions)
      (let* ((acct (cdr (assq 'account txn)))
             (existing (gethash acct groups)))
        (puthash acct (cons txn existing) groups)))
    (let ((results nil))
      (maphash
       (lambda (acct txns)
         (let ((credits 0) (debits 0) (count 0))
           (dolist (txn txns)
             (setq count (1+ count))
             (if (eq (cdr (assq 'type txn)) 'credit)
                 (setq credits (+ credits (cdr (assq 'amount txn))))
               (setq debits (+ debits (cdr (assq 'amount txn))))))
           (let ((net (- credits debits)))
             ;; HAVING: net > 500
             (when (> net 500)
               (setq results
                     (cons (list acct
                                 (cons 'credits credits)
                                 (cons 'debits debits)
                                 (cons 'net net)
                                 (cons 'txn-count count))
                           results))))))
       groups)
      ;; Also compute without HAVING for comparison
      (let ((all-results nil))
        (maphash
         (lambda (acct txns)
           (let ((credits 0) (debits 0))
             (dolist (txn txns)
               (if (eq (cdr (assq 'type txn)) 'credit)
                   (setq credits (+ credits (cdr (assq 'amount txn))))
                 (setq debits (+ debits (cdr (assq 'amount txn))))))
             (setq all-results
                   (cons (list acct (cons 'net (- credits debits)))
                         all-results))))
         groups)
        (list
          (sort all-results (lambda (a b) (string< (car a) (car b))))
          (sort results (lambda (a b) (string< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"A001\" (net . 900)) (\"A002\" (net . 350)) (\"A003\" (net . 950))) ((\"A001\" (credits . 1000) (debits . 100) (net . 900) (txn-count . 4)) (\"A003\" (credits . 1000) (debits . 50) (net . 950) (txn-count . 2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subquery pattern: correlated subquery using nested iteration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_ops_subquery_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((products (list
                   '((pid . 1) (pname . "Laptop") (category . "Electronics") (price . 1200))
                   '((pid . 2) (pname . "Phone") (category . "Electronics") (price . 800))
                   '((pid . 3) (pname . "Tablet") (category . "Electronics") (price . 600))
                   '((pid . 4) (pname . "Chair") (category . "Furniture") (price . 300))
                   '((pid . 5) (pname . "Desk") (category . "Furniture") (price . 500))
                   '((pid . 6) (pname . "Lamp") (category . "Furniture") (price . 100))
                   '((pid . 7) (pname . "Novel") (category . "Books") (price . 25))
                   '((pid . 8) (pname . "Textbook") (category . "Books") (price . 80)))))
  (list
    ;; Subquery: products with price above average for their category
    (let ((results nil))
      ;; For each product, compute avg of its category, compare
      (dolist (prod products)
        (let* ((cat (cdr (assq 'category prod)))
               (cat-prices nil))
          ;; Correlated subquery: find all prices in same category
          (dolist (p2 products)
            (when (equal (cdr (assq 'category p2)) cat)
              (setq cat-prices (cons (cdr (assq 'price p2)) cat-prices))))
          (let ((avg (/ (apply #'+ cat-prices) (length cat-prices))))
            (when (> (cdr (assq 'price prod)) avg)
              (setq results (cons (list (cdr (assq 'pname prod))
                                        (cdr (assq 'category prod))
                                        (cdr (assq 'price prod))
                                        avg)
                                  results))))))
      (sort (nreverse results) (lambda (a b) (string< (car a) (car b)))))

    ;; Subquery: categories where max price > 500
    (let ((categories nil)
          (seen (make-hash-table :test 'equal)))
      (dolist (prod products)
        (let ((cat (cdr (assq 'category prod))))
          (unless (gethash cat seen)
            (puthash cat t seen)
            (setq categories (cons cat categories)))))
      (let ((result nil))
        (dolist (cat (nreverse categories))
          (let ((max-price 0))
            (dolist (prod products)
              (when (equal (cdr (assq 'category prod)) cat)
                (let ((p (cdr (assq 'price prod))))
                  (when (> p max-price) (setq max-price p)))))
            (when (> max-price 500)
              (setq result (cons (cons cat max-price) result)))))
        (sort (nreverse result) (lambda (a b) (string< (car a) (car b))))))

    ;; Subquery: rank products by price within category
    (let ((ranked nil))
      (dolist (prod products)
        (let* ((cat (cdr (assq 'category prod)))
               (price (cdr (assq 'price prod)))
               (rank 1))
          (dolist (p2 products)
            (when (and (equal (cdr (assq 'category p2)) cat)
                       (> (cdr (assq 'price p2)) price))
              (setq rank (1+ rank))))
          (setq ranked (cons (list (cdr (assq 'pname prod)) cat rank) ranked))))
      (sort (nreverse ranked)
            (lambda (a b) (if (equal (cadr a) (cadr b))
                              (< (caddr a) (caddr b))
                            (string< (cadr a) (cadr b))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Desk\" \"Furniture\" 500 300) (\"Laptop\" \"Electronics\" 1200 866) (\"Textbook\" \"Books\" 80 52)) ((\"Electronics\" . 1200)) ((\"Textbook\" \"Books\" 1) (\"Novel\" \"Books\" 2) (\"Laptop\" \"Electronics\" 1) (\"Phone\" \"Electronics\" 2) (\"Tablet\" \"Electronics\" 3) (\"Desk\" \"Furniture\" 1) (\"Chair\" \"Furniture\" 2) (\"Lamp\" \"Furniture\" 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
