//! Complex oracle parity tests for a simple query language evaluator in Elisp:
//! SELECT with column projection, WHERE with comparison operators, ORDER BY,
//! LIMIT, nested subqueries, aggregate functions, DISTINCT, UNION of result sets.
//! Data represented as list-of-alists.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// SELECT with column projection and WHERE with comparison operators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_select_where_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Query engine primitives
  (fset 'neovm--ql-project
    (lambda (row cols)
      "Project ROW onto COLS: return alist with only specified columns."
      (let ((result nil))
        (dolist (col cols)
          (let ((cell (assq col row)))
            (when cell (setq result (cons cell result)))))
        (nreverse result))))

  (fset 'neovm--ql-where
    (lambda (table pred)
      "Filter TABLE rows by predicate PRED."
      (let ((result nil))
        (dolist (row table)
          (when (funcall pred row)
            (setq result (cons row result))))
        (nreverse result))))

  (fset 'neovm--ql-select
    (lambda (table cols pred)
      "SELECT cols FROM table WHERE pred."
      (let ((filtered (if pred (funcall 'neovm--ql-where table pred) table)))
        (if cols
            (mapcar (lambda (row) (funcall 'neovm--ql-project row cols)) filtered)
          filtered))))

  (unwind-protect
      (let ((employees '(((id . 1) (name . "Alice") (age . 30) (dept . "eng") (salary . 90000))
                          ((id . 2) (name . "Bob") (age . 25) (dept . "sales") (salary . 60000))
                          ((id . 3) (name . "Charlie") (age . 35) (dept . "eng") (salary . 110000))
                          ((id . 4) (name . "Diana") (age . 28) (dept . "marketing") (salary . 70000))
                          ((id . 5) (name . "Eve") (age . 32) (dept . "eng") (salary . 95000))
                          ((id . 6) (name . "Frank") (age . 22) (dept . "sales") (salary . 55000))
                          ((id . 7) (name . "Grace") (age . 40) (dept . "eng") (salary . 120000))
                          ((id . 8) (name . "Hank") (age . 27) (dept . "marketing") (salary . 65000)))))
        (list
          ;; SELECT name, dept FROM employees WHERE dept = 'eng'
          (funcall 'neovm--ql-select employees '(name dept)
                   (lambda (row) (string= (cdr (assq 'dept row)) "eng")))
          ;; SELECT name, salary WHERE salary > 80000
          (funcall 'neovm--ql-select employees '(name salary)
                   (lambda (row) (> (cdr (assq 'salary row)) 80000)))
          ;; SELECT name WHERE age >= 30 AND age <= 35
          (funcall 'neovm--ql-select employees '(name)
                   (lambda (row) (let ((age (cdr (assq 'age row))))
                                   (and (>= age 30) (<= age 35)))))
          ;; SELECT * (all cols) WHERE dept = 'sales' (no projection)
          (funcall 'neovm--ql-select employees nil
                   (lambda (row) (string= (cdr (assq 'dept row)) "sales")))))
    (fmakunbound 'neovm--ql-project)
    (fmakunbound 'neovm--ql-where)
    (fmakunbound 'neovm--ql-select)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((name . \"Alice\") (dept . \"eng\")) ((name . \"Charlie\") (dept . \"eng\")) ((name . \"Eve\") (dept . \"eng\")) ((name . \"Grace\") (dept . \"eng\"))) (((name . \"Alice\") (salary . 90000)) ((name . \"Charlie\") (salary . 110000)) ((name . \"Eve\") (salary . 95000)) ((name . \"Grace\") (salary . 120000))) (((name . \"Alice\")) ((name . \"Charlie\")) ((name . \"Eve\"))) (((id . 2) (name . \"Bob\") (age . 25) (dept . \"sales\") (salary . 60000)) ((id . 6) (name . \"Frank\") (age . 22) (dept . \"sales\") (salary . 55000))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ORDER BY and LIMIT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_order_by_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--ql-order-by
    (lambda (table col direction)
      "Sort TABLE by COL in DIRECTION (:asc or :desc). Handles both string and number."
      (let ((sorted (copy-sequence table)))
        (sort sorted
              (lambda (a b)
                (let ((va (cdr (assq col a)))
                      (vb (cdr (assq col b))))
                  (let ((less (if (stringp va) (string< va vb) (< va vb))))
                    (if (eq direction :desc) (not less) less))))))))

  (fset 'neovm--ql-limit
    (lambda (table n)
      "Return first N rows of TABLE."
      (let ((result nil)
            (count 0))
        (while (and table (< count n))
          (setq result (cons (car table) result))
          (setq table (cdr table))
          (setq count (1+ count)))
        (nreverse result))))

  (fset 'neovm--ql-offset
    (lambda (table n)
      "Skip first N rows of TABLE."
      (nthcdr n table)))

  (unwind-protect
      (let ((products '(((id . 1) (name . "Widget") (price . 25) (qty . 100))
                         ((id . 2) (name . "Gadget") (price . 50) (qty . 30))
                         ((id . 3) (name . "Doohickey") (price . 15) (qty . 200))
                         ((id . 4) (name . "Thingamajig") (price . 75) (qty . 50))
                         ((id . 5) (name . "Gizmo") (price . 35) (qty . 80))
                         ((id . 6) (name . "Contraption") (price . 90) (qty . 10)))))
        (list
          ;; ORDER BY price ASC
          (mapcar (lambda (r) (cdr (assq 'name r)))
                  (funcall 'neovm--ql-order-by products 'price :asc))
          ;; ORDER BY price DESC, LIMIT 3
          (mapcar (lambda (r) (list (cdr (assq 'name r)) (cdr (assq 'price r))))
                  (funcall 'neovm--ql-limit
                    (funcall 'neovm--ql-order-by products 'price :desc) 3))
          ;; ORDER BY name ASC
          (mapcar (lambda (r) (cdr (assq 'name r)))
                  (funcall 'neovm--ql-order-by products 'name :asc))
          ;; ORDER BY qty DESC, LIMIT 2, OFFSET 1 (skip top, get 2nd and 3rd)
          (mapcar (lambda (r) (list (cdr (assq 'name r)) (cdr (assq 'qty r))))
                  (funcall 'neovm--ql-limit
                    (funcall 'neovm--ql-offset
                      (funcall 'neovm--ql-order-by products 'qty :desc) 1) 2))))
    (fmakunbound 'neovm--ql-order-by)
    (fmakunbound 'neovm--ql-limit)
    (fmakunbound 'neovm--ql-offset)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Doohickey\" \"Widget\" \"Gizmo\" \"Gadget\" \"Thingamajig\" \"Contraption\") ((\"Contraption\" 90) (\"Thingamajig\" 75) (\"Gadget\" 50)) (\"Contraption\" \"Doohickey\" \"Gadget\" \"Gizmo\" \"Thingamajig\" \"Widget\") ((\"Widget\" 100) (\"Gizmo\" 80)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Aggregate functions: COUNT, SUM, AVG, MIN, MAX, GROUP BY
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_aggregate_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--ql-group-by
    (lambda (table col)
      "Group TABLE by COL, returning alist of (value . rows)."
      (let ((groups (make-hash-table :test 'equal)))
        (dolist (row table)
          (let* ((key (cdr (assq col row)))
                 (existing (gethash key groups)))
            (puthash key (cons row existing) groups)))
        ;; Convert to sorted alist
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons (cons k (nreverse v)) result))) groups)
          (sort result (lambda (a b) (if (stringp (car a)) (string< (car a) (car b))
                                       (< (car a) (car b)))))))))

  (fset 'neovm--ql-agg-count (lambda (rows) (length rows)))
  (fset 'neovm--ql-agg-sum
    (lambda (rows col)
      (let ((total 0))
        (dolist (row rows) (setq total (+ total (cdr (assq col row)))))
        total)))
  (fset 'neovm--ql-agg-min
    (lambda (rows col)
      (let ((m nil))
        (dolist (row rows)
          (let ((v (cdr (assq col row))))
            (when (or (null m) (< v m)) (setq m v))))
        m)))
  (fset 'neovm--ql-agg-max
    (lambda (rows col)
      (let ((m nil))
        (dolist (row rows)
          (let ((v (cdr (assq col row))))
            (when (or (null m) (> v m)) (setq m v))))
        m)))

  (unwind-protect
      (let ((sales '(((product . "A") (region . "north") (amount . 100))
                      ((product . "B") (region . "south") (amount . 200))
                      ((product . "A") (region . "south") (amount . 150))
                      ((product . "C") (region . "north") (amount . 300))
                      ((product . "B") (region . "north") (amount . 250))
                      ((product . "A") (region . "north") (amount . 200))
                      ((product . "C") (region . "south") (amount . 100))
                      ((product . "B") (region . "south") (amount . 175)))))
        ;; GROUP BY product, compute aggregates
        (let ((by-product (funcall 'neovm--ql-group-by sales 'product)))
          (let ((agg-by-product
                 (mapcar (lambda (group)
                           (let ((key (car group))
                                 (rows (cdr group)))
                             (list key
                                   (cons 'count (funcall 'neovm--ql-agg-count rows))
                                   (cons 'sum (funcall 'neovm--ql-agg-sum rows 'amount))
                                   (cons 'min (funcall 'neovm--ql-agg-min rows 'amount))
                                   (cons 'max (funcall 'neovm--ql-agg-max rows 'amount)))))
                         by-product)))
            ;; GROUP BY region
            (let ((by-region (funcall 'neovm--ql-group-by sales 'region)))
              (let ((agg-by-region
                     (mapcar (lambda (group)
                               (list (car group)
                                     (cons 'count (funcall 'neovm--ql-agg-count (cdr group)))
                                     (cons 'sum (funcall 'neovm--ql-agg-sum (cdr group) 'amount))))
                             by-region)))
                ;; Global aggregates
                (list agg-by-product
                      agg-by-region
                      (funcall 'neovm--ql-agg-sum sales 'amount)
                      (funcall 'neovm--ql-agg-count sales)))))))
    (fmakunbound 'neovm--ql-group-by)
    (fmakunbound 'neovm--ql-agg-count)
    (fmakunbound 'neovm--ql-agg-sum)
    (fmakunbound 'neovm--ql-agg-min)
    (fmakunbound 'neovm--ql-agg-max)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"A\" (count . 3) (sum . 450) (min . 100) (max . 200)) (\"B\" (count . 3) (sum . 625) (min . 175) (max . 250)) (\"C\" (count . 2) (sum . 400) (min . 100) (max . 300))) ((\"north\" (count . 4) (sum . 850)) (\"south\" (count . 4) (sum . 625))) 1475 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DISTINCT and UNION of result sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_distinct_union() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--ql-distinct
    (lambda (table cols)
      "Remove duplicate rows considering only COLS for equality."
      (let ((seen (make-hash-table :test 'equal))
            (result nil))
        (dolist (row table)
          (let ((key (mapcar (lambda (col) (cdr (assq col row))) cols)))
            (unless (gethash key seen)
              (puthash key t seen)
              (setq result (cons row result)))))
        (nreverse result))))

  (fset 'neovm--ql-union
    (lambda (t1 t2)
      "UNION: combine two tables removing duplicates."
      (let ((combined (append t1 t2))
            (seen (make-hash-table :test 'equal))
            (result nil))
        (dolist (row combined)
          (unless (gethash row seen)
            (puthash row t seen)
            (setq result (cons row result))))
        (nreverse result))))

  (fset 'neovm--ql-union-all
    (lambda (t1 t2)
      "UNION ALL: combine two tables keeping duplicates."
      (append t1 t2)))

  (unwind-protect
      (let ((t1 '(((name . "Alice") (dept . "eng"))
                   ((name . "Bob") (dept . "sales"))
                   ((name . "Charlie") (dept . "eng"))
                   ((name . "Alice") (dept . "marketing"))
                   ((name . "Bob") (dept . "eng"))))
            (t2 '(((name . "Diana") (dept . "eng"))
                   ((name . "Alice") (dept . "eng"))
                   ((name . "Eve") (dept . "sales"))
                   ((name . "Bob") (dept . "sales")))))
        (list
          ;; DISTINCT on name only
          (mapcar (lambda (r) (cdr (assq 'name r)))
                  (funcall 'neovm--ql-distinct t1 '(name)))
          ;; DISTINCT on dept only
          (mapcar (lambda (r) (cdr (assq 'dept r)))
                  (funcall 'neovm--ql-distinct t1 '(dept)))
          ;; DISTINCT on (name, dept) — full row dedup
          (length (funcall 'neovm--ql-distinct t1 '(name dept)))
          ;; UNION of t1 and t2
          (length (funcall 'neovm--ql-union t1 t2))
          ;; UNION ALL of t1 and t2
          (length (funcall 'neovm--ql-union-all t1 t2))
          ;; UNION result: unique names
          (sort (mapcar (lambda (r) (cdr (assq 'name r)))
                        (funcall 'neovm--ql-distinct
                          (funcall 'neovm--ql-union t1 t2) '(name)))
                #'string<)))
    (fmakunbound 'neovm--ql-distinct)
    (fmakunbound 'neovm--ql-union)
    (fmakunbound 'neovm--ql-union-all)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Charlie\") (\"eng\" \"sales\" \"marketing\") 5 7 9 (\"Alice\" \"Bob\" \"Charlie\" \"Diana\" \"Eve\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested subqueries: WHERE col IN (SELECT ...)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_nested_subqueries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--ql-filter
    (lambda (table pred)
      (let ((result nil))
        (dolist (row table) (when (funcall pred row) (setq result (cons row result))))
        (nreverse result))))
  (fset 'neovm--ql-proj
    (lambda (table cols)
      (mapcar (lambda (row)
                (let ((r nil))
                  (dolist (c cols) (let ((cell (assq c row))) (when cell (setq r (cons cell r)))))
                  (nreverse r)))
              table)))
  (fset 'neovm--ql-col-values
    (lambda (table col)
      "Extract a flat list of values for COL from TABLE."
      (mapcar (lambda (row) (cdr (assq col row))) table)))

  (unwind-protect
      (let ((employees '(((id . 1) (name . "Alice") (dept-id . 10) (salary . 90000))
                          ((id . 2) (name . "Bob") (dept-id . 20) (salary . 60000))
                          ((id . 3) (name . "Charlie") (dept-id . 10) (salary . 110000))
                          ((id . 4) (name . "Diana") (dept-id . 30) (salary . 70000))
                          ((id . 5) (name . "Eve") (dept-id . 20) (salary . 95000))
                          ((id . 6) (name . "Frank") (dept-id . 10) (salary . 85000))))
            (departments '(((dept-id . 10) (dept-name . "Engineering") (budget . 500000))
                           ((dept-id . 20) (dept-name . "Sales") (budget . 200000))
                           ((dept-id . 30) (dept-name . "Marketing") (budget . 150000)))))
        ;; Subquery 1: SELECT name FROM employees
        ;;   WHERE dept-id IN (SELECT dept-id FROM departments WHERE budget > 180000)
        (let* ((high-budget-depts
                (funcall 'neovm--ql-col-values
                  (funcall 'neovm--ql-filter departments
                    (lambda (d) (> (cdr (assq 'budget d)) 180000)))
                  'dept-id))
               (employees-in-hb
                (funcall 'neovm--ql-filter employees
                  (lambda (e) (member (cdr (assq 'dept-id e)) high-budget-depts)))))
          ;; Subquery 2: SELECT name FROM employees
          ;;   WHERE salary > (SELECT AVG(salary) FROM employees)
          (let* ((total-salary (apply #'+ (funcall 'neovm--ql-col-values employees 'salary)))
                 (avg-salary (/ total-salary (length employees)))
                 (above-avg
                  (funcall 'neovm--ql-filter employees
                    (lambda (e) (> (cdr (assq 'salary e)) avg-salary)))))
            ;; Subquery 3: correlated — for each dept, find employee with max salary
            (let ((max-per-dept nil))
              (dolist (dept departments)
                (let* ((did (cdr (assq 'dept-id dept)))
                       (dept-emps (funcall 'neovm--ql-filter employees
                                    (lambda (e) (= (cdr (assq 'dept-id e)) did))))
                       (max-sal 0)
                       (max-emp nil))
                  (dolist (e dept-emps)
                    (when (> (cdr (assq 'salary e)) max-sal)
                      (setq max-sal (cdr (assq 'salary e)))
                      (setq max-emp e)))
                  (when max-emp
                    (setq max-per-dept
                          (cons (list (cdr (assq 'dept-name dept))
                                      (cdr (assq 'name max-emp))
                                      max-sal)
                                max-per-dept)))))
              (list
                ;; Names in high-budget departments
                (sort (funcall 'neovm--ql-col-values employees-in-hb 'name) #'string<)
                ;; Names with above-average salary
                (sort (funcall 'neovm--ql-col-values above-avg 'name) #'string<)
                avg-salary
                ;; Max salary employee per department
                (sort (nreverse max-per-dept)
                      (lambda (a b) (string< (car a) (car b)))))))))
    (fmakunbound 'neovm--ql-filter)
    (fmakunbound 'neovm--ql-proj)
    (fmakunbound 'neovm--ql-col-values)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Charlie\" \"Eve\" \"Frank\") (\"Alice\" \"Charlie\" \"Eve\") 85000 ((\"Engineering\" \"Charlie\" 110000) (\"Marketing\" \"Diana\" 70000) (\"Sales\" \"Eve\" 95000)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JOIN operations: INNER JOIN, LEFT JOIN, CROSS JOIN
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_join_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--ql-inner-join
    (lambda (t1 t2 col1 col2)
      "Inner join T1 and T2 on T1.COL1 = T2.COL2. Merge matching rows."
      (let ((result nil))
        (dolist (r1 t1)
          (dolist (r2 t2)
            (when (equal (cdr (assq col1 r1)) (cdr (assq col2 r2)))
              ;; Merge: take all fields from r1, add non-conflicting from r2
              (let ((merged (copy-sequence r1)))
                (dolist (cell r2)
                  (unless (assq (car cell) merged)
                    (setq merged (cons cell merged))))
                (setq result (cons merged result))))))
        (nreverse result))))

  (fset 'neovm--ql-left-join
    (lambda (t1 t2 col1 col2)
      "Left join: all rows from T1, matched rows from T2 (nil for unmatched)."
      (let ((result nil))
        (dolist (r1 t1)
          (let ((matched nil))
            (dolist (r2 t2)
              (when (equal (cdr (assq col1 r1)) (cdr (assq col2 r2)))
                (let ((merged (copy-sequence r1)))
                  (dolist (cell r2)
                    (unless (assq (car cell) merged)
                      (setq merged (cons cell merged))))
                  (setq result (cons merged result))
                  (setq matched t))))
            (unless matched
              (setq result (cons r1 result)))))
        (nreverse result))))

  (fset 'neovm--ql-cross-join
    (lambda (t1 t2)
      "Cross join: Cartesian product of T1 and T2."
      (let ((result nil))
        (dolist (r1 t1)
          (dolist (r2 t2)
            (let ((merged (copy-sequence r1)))
              (dolist (cell r2)
                (unless (assq (car cell) merged)
                  (setq merged (cons cell merged))))
              (setq result (cons merged result)))))
        (nreverse result))))

  (unwind-protect
      (let ((orders '(((order-id . 1) (cust-id . 10) (product . "Widget") (qty . 5))
                       ((order-id . 2) (cust-id . 20) (product . "Gadget") (qty . 3))
                       ((order-id . 3) (cust-id . 10) (product . "Gizmo") (qty . 2))
                       ((order-id . 4) (cust-id . 30) (product . "Widget") (qty . 8))
                       ((order-id . 5) (cust-id . 40) (product . "Doohickey") (qty . 1))))
            (customers '(((cust-id . 10) (cust-name . "Acme Corp"))
                         ((cust-id . 20) (cust-name . "Beta Inc"))
                         ((cust-id . 30) (cust-name . "Gamma LLC"))
                         ((cust-id . 50) (cust-name . "Delta Co")))))
        ;; INNER JOIN: only matching rows
        (let ((inner (funcall 'neovm--ql-inner-join orders customers 'cust-id 'cust-id)))
          ;; LEFT JOIN: all orders, some without customer name
          (let ((left (funcall 'neovm--ql-left-join orders customers 'cust-id 'cust-id)))
            ;; CROSS JOIN of small tables
            (let* ((colors '(((color . "red")) ((color . "blue"))))
                   (sizes '(((size . "S")) ((size . "M")) ((size . "L"))))
                   (cross (funcall 'neovm--ql-cross-join colors sizes)))
              (list
                ;; Inner join: order count (cust-id 40 has no match)
                (length inner)
                ;; Inner join: customer names from joined result
                (sort (mapcar (lambda (r) (cdr (assq 'cust-name r))) inner) #'string<)
                ;; Left join: should have all 5 orders
                (length left)
                ;; Left join: order 5 (cust-id 40) has no cust-name
                (cdr (assq 'cust-name (nth 4 left)))
                ;; Cross join: 2 * 3 = 6 combinations
                (length cross)
                ;; Cross join: verify content
                (mapcar (lambda (r) (list (cdr (assq 'color r)) (cdr (assq 'size r)))) cross))))))
    (fmakunbound 'neovm--ql-inner-join)
    (fmakunbound 'neovm--ql-left-join)
    (fmakunbound 'neovm--ql-cross-join)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 (\"Acme Corp\" \"Acme Corp\" \"Beta Inc\" \"Gamma LLC\") 5 nil 6 ((\"red\" \"S\") (\"red\" \"M\") (\"red\" \"L\") (\"blue\" \"S\") (\"blue\" \"M\") (\"blue\" \"L\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: full query pipeline combining SELECT, WHERE, JOIN, GROUP BY, ORDER BY, LIMIT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_full_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Reusable query primitives
  (fset 'neovm--ql2-filter
    (lambda (table pred)
      (let ((r nil)) (dolist (row table) (when (funcall pred row) (setq r (cons row r)))) (nreverse r))))
  (fset 'neovm--ql2-project
    (lambda (table cols)
      (mapcar (lambda (row) (let ((r nil)) (dolist (c cols) (let ((cell (assq c row))) (when cell (setq r (cons cell r))))) (nreverse r))) table)))
  (fset 'neovm--ql2-join
    (lambda (t1 t2 c1 c2)
      (let ((r nil))
        (dolist (r1 t1)
          (dolist (r2 t2)
            (when (equal (cdr (assq c1 r1)) (cdr (assq c2 r2)))
              (let ((m (copy-sequence r1)))
                (dolist (cell r2) (unless (assq (car cell) m) (setq m (cons cell m))))
                (setq r (cons m r))))))
        (nreverse r))))
  (fset 'neovm--ql2-group-by
    (lambda (table col)
      (let ((groups (make-hash-table :test 'equal)))
        (dolist (row table)
          (let* ((key (cdr (assq col row)))
                 (existing (gethash key groups)))
            (puthash key (cons row existing) groups)))
        (let ((r nil))
          (maphash (lambda (k v) (setq r (cons (cons k (nreverse v)) r))) groups)
          (sort r (lambda (a b) (if (stringp (car a)) (string< (car a) (car b)) (< (car a) (car b)))))))))
  (fset 'neovm--ql2-order-by
    (lambda (table col dir)
      (let ((s (copy-sequence table)))
        (sort s (lambda (a b)
                  (let ((va (cdr (assq col a))) (vb (cdr (assq col b))))
                    (let ((less (if (stringp va) (string< va vb) (< va vb))))
                      (if (eq dir :desc) (not less) less))))))))
  (fset 'neovm--ql2-limit
    (lambda (table n)
      (let ((r nil) (c 0))
        (while (and table (< c n))
          (setq r (cons (car table) r) table (cdr table) c (1+ c)))
        (nreverse r))))

  (unwind-protect
      (let ((orders '(((oid . 1) (cid . 1) (pid . 101) (qty . 5) (price . 20))
                       ((oid . 2) (cid . 2) (pid . 102) (qty . 3) (price . 50))
                       ((oid . 3) (cid . 1) (pid . 103) (qty . 2) (price . 30))
                       ((oid . 4) (cid . 3) (pid . 101) (qty . 8) (price . 20))
                       ((oid . 5) (cid . 2) (pid . 102) (qty . 1) (price . 50))
                       ((oid . 6) (cid . 1) (pid . 101) (qty . 4) (price . 20))))
            (customers '(((cid . 1) (cname . "Acme"))
                         ((cid . 2) (cname . "Beta"))
                         ((cid . 3) (cname . "Gamma")))))
        ;; Pipeline: JOIN orders with customers,
        ;;           add computed column (total = qty * price),
        ;;           GROUP BY customer, SUM totals,
        ;;           ORDER BY total DESC, LIMIT 2
        (let* ((joined (funcall 'neovm--ql2-join orders customers 'cid 'cid))
               ;; Add computed total column
               (with-total (mapcar (lambda (r)
                                     (cons (cons 'total (* (cdr (assq 'qty r))
                                                           (cdr (assq 'price r))))
                                           r))
                                   joined))
               ;; GROUP BY customer
               (grouped (funcall 'neovm--ql2-group-by with-total 'cname))
               ;; Aggregate: sum totals per customer
               (agg (mapcar (lambda (g)
                              (let ((total 0))
                                (dolist (row (cdr g))
                                  (setq total (+ total (cdr (assq 'total row)))))
                                (list (cons 'cname (car g))
                                      (cons 'total-spent total)
                                      (cons 'order-count (length (cdr g))))))
                            grouped))
               ;; ORDER BY total-spent DESC
               (sorted (funcall 'neovm--ql2-order-by agg 'total-spent :desc))
               ;; LIMIT 2
               (top2 (funcall 'neovm--ql2-limit sorted 2)))
          (list
            (length joined)     ;; total joined rows
            top2                ;; top 2 customers by spend
            ;; Full aggregation for verification
            (mapcar (lambda (r) (list (cdr (assq 'cname r))
                                      (cdr (assq 'total-spent r))))
                    sorted))))
    (fmakunbound 'neovm--ql2-filter)
    (fmakunbound 'neovm--ql2-project)
    (fmakunbound 'neovm--ql2-join)
    (fmakunbound 'neovm--ql2-group-by)
    (fmakunbound 'neovm--ql2-order-by)
    (fmakunbound 'neovm--ql2-limit)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 (((cname . \"Acme\") (total-spent . 240) (order-count . 3)) ((cname . \"Beta\") (total-spent . 200) (order-count . 2))) ((\"Acme\" 240) (\"Beta\" 200) (\"Gamma\" 160)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HAVING clause: filter after GROUP BY aggregation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ql_having_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((sales '(((seller . "Alice") (item . "A") (amount . 100))
                      ((seller . "Bob") (item . "B") (amount . 200))
                      ((seller . "Alice") (item . "C") (amount . 150))
                      ((seller . "Charlie") (item . "A") (amount . 50))
                      ((seller . "Bob") (item . "A") (amount . 300))
                      ((seller . "Alice") (item . "B") (amount . 250))
                      ((seller . "Charlie") (item . "C") (amount . 75))
                      ((seller . "Bob") (item . "C") (amount . 100)))))
  ;; GROUP BY seller, aggregate, then HAVING sum > 400
  (let ((groups (make-hash-table :test 'equal)))
    ;; Group
    (dolist (row sales)
      (let* ((key (cdr (assq 'seller row)))
             (existing (gethash key groups)))
        (puthash key (cons row existing) groups)))
    ;; Aggregate
    (let ((agg nil))
      (maphash (lambda (seller rows)
                 (let ((total 0) (cnt 0))
                   (dolist (r rows) (setq total (+ total (cdr (assq 'amount r))) cnt (1+ cnt)))
                   (setq agg (cons (list (cons 'seller seller)
                                         (cons 'total total)
                                         (cons 'count cnt)
                                         (cons 'avg (/ total cnt)))
                                   agg))))
               groups)
      ;; HAVING total > 400
      (let ((having-result nil))
        (dolist (row agg)
          (when (> (cdr (assq 'total row)) 400)
            (setq having-result (cons row having-result))))
        ;; Sort by seller name
        (setq having-result (sort having-result
                                   (lambda (a b) (string< (cdr (assq 'seller a))
                                                          (cdr (assq 'seller b))))))
        ;; Also: HAVING count >= 3
        (let ((having-count nil))
          (dolist (row agg)
            (when (>= (cdr (assq 'count row)) 3)
              (setq having-count (cons (cdr (assq 'seller row)) having-count))))
          (list having-result
                (sort having-count #'string<)
                ;; Full aggregation for reference
                (sort (mapcar (lambda (r) (list (cdr (assq 'seller r))
                                                (cdr (assq 'total r))))
                              agg)
                      (lambda (a b) (string< (car a) (car b))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((((seller . \"Alice\") (total . 500) (count . 3) (avg . 166)) ((seller . \"Bob\") (total . 600) (count . 3) (avg . 200))) (\"Alice\" \"Bob\") ((\"Alice\" 500) (\"Bob\" 600) (\"Charlie\" 125)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
