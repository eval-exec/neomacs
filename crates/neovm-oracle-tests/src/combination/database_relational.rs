//! Oracle parity tests for relational database operations:
//! SELECT/WHERE/ORDER-BY, JOIN (inner, left, cross), GROUP BY with
//! aggregation (COUNT, SUM, AVG, MAX, MIN), HAVING clause, DISTINCT,
//! UNION, INTERSECT, INSERT/UPDATE/DELETE, index-based lookup, multi-table queries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// SELECT with WHERE, projection, and compound conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_select_where_compound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((employees (list
                   '((id . 1) (name . "Alice") (dept . "Eng") (salary . 95000) (level . 3))
                   '((id . 2) (name . "Bob") (dept . "Sales") (salary . 72000) (level . 2))
                   '((id . 3) (name . "Carol") (dept . "Eng") (salary . 105000) (level . 4))
                   '((id . 4) (name . "Dave") (dept . "HR") (salary . 68000) (level . 2))
                   '((id . 5) (name . "Eve") (dept . "Eng") (salary . 88000) (level . 3))
                   '((id . 6) (name . "Frank") (dept . "Sales") (salary . 78000) (level . 2))
                   '((id . 7) (name . "Grace") (dept . "Eng") (salary . 110000) (level . 5))
                   '((id . 8) (name . "Hank") (dept . "HR") (salary . 62000) (level . 1)))))
  (let ((db-select
         (lambda (tbl cols where-fn)
           "SELECT cols FROM tbl WHERE where-fn."
           (let ((result nil))
             (dolist (row tbl)
               (when (or (null where-fn) (funcall where-fn row))
                 (if (null cols)
                     (setq result (cons row result))
                   (let ((projected nil))
                     (dolist (c cols)
                       (let ((cell (assq c row)))
                         (when cell (setq projected (cons cell projected)))))
                     (setq result (cons (nreverse projected) result))))))
             (nreverse result)))))

    (list
      ;; SELECT name, salary FROM employees WHERE dept='Eng' AND salary > 90000
      (funcall db-select employees '(name salary)
               (lambda (r) (and (equal (cdr (assq 'dept r)) "Eng")
                                (> (cdr (assq 'salary r)) 90000))))
      ;; SELECT * FROM employees WHERE level >= 3 OR dept='HR'
      (mapcar (lambda (r) (cdr (assq 'name r)))
              (funcall db-select employees nil
                       (lambda (r) (or (>= (cdr (assq 'level r)) 3)
                                       (equal (cdr (assq 'dept r)) "HR")))))
      ;; SELECT name FROM employees WHERE NOT dept='Eng'
      (funcall db-select employees '(name)
               (lambda (r) (not (equal (cdr (assq 'dept r)) "Eng"))))
      ;; SELECT name, level FROM employees WHERE salary BETWEEN 70000 AND 90000
      (funcall db-select employees '(name level)
               (lambda (r) (let ((s (cdr (assq 'salary r))))
                             (and (>= s 70000) (<= s 90000)))))
      ;; SELECT COUNT(*) equivalent
      (length (funcall db-select employees nil nil))
      ;; SELECT DISTINCT dept
      (let ((depts nil))
        (dolist (row employees)
          (let ((d (cdr (assq 'dept row))))
            (unless (member d depts)
              (setq depts (cons d depts)))))
        (sort (nreverse depts) #'string<)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((((name . \"Alice\") (salary . 95000)) ((name . \"Carol\") (salary . 105000)) ((name . \"Grace\") (salary . 110000))) (\"Alice\" \"Carol\" \"Dave\" \"Eve\" \"Grace\" \"Hank\") (((name . \"Bob\")) ((name . \"Dave\")) ((name . \"Frank\")) ((name . \"Hank\"))) (((name . \"Bob\") (level . 2)) ((name . \"Eve\") (level . 3)) ((name . \"Frank\") (level . 2))) 8 (\"Eng\" \"HR\" \"Sales\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LEFT JOIN: preserve all left rows, NULL for unmatched right
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_left_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((orders (list
                   '((oid . 1) (cust-id . 101) (product . "Laptop") (amount . 1200))
                   '((oid . 2) (cust-id . 102) (product . "Phone") (amount . 800))
                   '((oid . 3) (cust-id . 101) (product . "Tablet") (amount . 500))
                   '((oid . 4) (cust-id . 103) (product . "Monitor") (amount . 400))
                   '((oid . 5) (cust-id . 999) (product . "Mouse") (amount . 50))))
              (customers (list
                   '((cust-id . 101) (cname . "Alice") (city . "NYC"))
                   '((cust-id . 102) (cname . "Bob") (city . "LA"))
                   '((cust-id . 103) (cname . "Carol") (city . "CHI"))
                   '((cust-id . 104) (cname . "Dave") (city . "SF")))))
  (let ((left-join
         (lambda (left right lkey rkey)
           "LEFT JOIN: all left rows, NULL fields for unmatched right."
           (let ((right-idx (make-hash-table :test 'equal))
                 (result nil))
             ;; Index right table
             (dolist (row right)
               (let* ((key (cdr (assq rkey row)))
                      (existing (gethash key right-idx)))
                 (puthash key (cons row existing) right-idx)))
             ;; Join
             (dolist (lrow left)
               (let* ((key (cdr (assq lkey lrow)))
                      (rrows (gethash key right-idx)))
                 (if rrows
                     (dolist (rrow rrows)
                       (let ((merged (copy-sequence lrow)))
                         (dolist (cell rrow)
                           (unless (assq (car cell) merged)
                             (setq merged (append merged (list cell)))))
                         (setq result (cons merged result))))
                   ;; No match: include left row with nil right columns
                   (let ((merged (copy-sequence lrow)))
                     ;; Add nil for expected right columns
                     (when right
                       (dolist (cell (car right))
                         (unless (or (eq (car cell) rkey) (assq (car cell) merged))
                           (setq merged (append merged (list (cons (car cell) nil)))))))
                     (setq result (cons merged result))))))
             (nreverse result)))))

    (list
      ;; LEFT JOIN orders ON customers: order 5 (cust-id 999) has no match
      (mapcar (lambda (r) (list (cdr (assq 'oid r))
                                (cdr (assq 'cname r))
                                (cdr (assq 'product r))))
              (funcall left-join orders customers 'cust-id 'cust-id))
      ;; Total rows: 5 (all left rows preserved)
      (length (funcall left-join orders customers 'cust-id 'cust-id))
      ;; LEFT JOIN customers ON orders: Dave (104) has no orders
      (mapcar (lambda (r) (list (cdr (assq 'cname r))
                                (cdr (assq 'product r))))
              (funcall left-join customers orders 'cust-id 'cust-id))
      ;; Alice appears twice (two orders)
      (length (funcall left-join customers orders 'cust-id 'cust-id)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 \"Alice\" \"Laptop\") (2 \"Bob\" \"Phone\") (3 \"Alice\" \"Tablet\") (4 \"Carol\" \"Monitor\") (5 nil \"Mouse\")) 5 ((\"Alice\" \"Tablet\") (\"Alice\" \"Laptop\") (\"Bob\" \"Phone\") (\"Carol\" \"Monitor\") (\"Dave\" nil)) 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CROSS JOIN: cartesian product
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_cross_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((colors '(((color . "Red")) ((color . "Blue")) ((color . "Green"))))
              (sizes '(((size . "S")) ((size . "M")) ((size . "L")))))
  (let ((cross-join
         (lambda (left right)
           "CROSS JOIN: every combination of left x right."
           (let ((result nil))
             (dolist (lrow left)
               (dolist (rrow right)
                 (setq result (cons (append lrow rrow) result))))
             (nreverse result)))))

    (let ((product (funcall cross-join colors sizes)))
      (list
        ;; 3 x 3 = 9 combinations
        (length product)
        ;; All combinations
        (mapcar (lambda (r) (list (cdr (assq 'color r)) (cdr (assq 'size r))))
                product)
        ;; Cross join with single-row table
        (let ((singles '(((label . "X")))))
          (length (funcall cross-join colors singles)))
        ;; Cross join with empty table
        (length (funcall cross-join colors nil))))))"#;
    let expect = expect_test::expect![[
        r#""OK (9 ((\"Red\" \"S\") (\"Red\" \"M\") (\"Red\" \"L\") (\"Blue\" \"S\") (\"Blue\" \"M\") (\"Blue\" \"L\") (\"Green\" \"S\") (\"Green\" \"M\") (\"Green\" \"L\")) 3 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// GROUP BY with HAVING: aggregation + filter on aggregated results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_group_by_having() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((sales (list
                   '((sid . 1) (rep . "Alice") (region . "East") (amount . 500) (quarter . 1))
                   '((sid . 2) (rep . "Bob") (region . "West") (amount . 300) (quarter . 1))
                   '((sid . 3) (rep . "Alice") (region . "East") (amount . 700) (quarter . 2))
                   '((sid . 4) (rep . "Carol") (region . "East") (amount . 200) (quarter . 1))
                   '((sid . 5) (rep . "Bob") (region . "West") (amount . 600) (quarter . 2))
                   '((sid . 6) (rep . "Alice") (region . "East") (amount . 400) (quarter . 3))
                   '((sid . 7) (rep . "Carol") (region . "West") (amount . 350) (quarter . 2))
                   '((sid . 8) (rep . "Bob") (region . "East") (amount . 450) (quarter . 3))
                   '((sid . 9) (rep . "Alice") (region . "West") (amount . 550) (quarter . 3))
                   '((sid . 10) (rep . "Carol") (region . "East") (amount . 150) (quarter . 3)))))
  (let ((group-by-agg
         (lambda (tbl group-col agg-specs having-fn)
           "GROUP BY group-col, compute aggs, filter by HAVING."
           (let ((groups (make-hash-table :test 'equal)))
             ;; Partition into groups
             (dolist (row tbl)
               (let* ((key (cdr (assq group-col row)))
                      (existing (gethash key groups)))
                 (puthash key (cons row existing) groups)))
             ;; Aggregate each group
             (let ((results nil))
               (maphash
                (lambda (key rows)
                  (let ((agg-row (list (cons group-col key))))
                    (dolist (spec agg-specs)
                      (let* ((agg-name (nth 0 spec))
                             (agg-fn (nth 1 spec))
                             (agg-col (nth 2 spec))
                             (val (cond
                                    ((eq agg-fn 'count) (length rows))
                                    ((eq agg-fn 'sum)
                                     (let ((s 0))
                                       (dolist (r rows) (setq s (+ s (cdr (assq agg-col r)))))
                                       s))
                                    ((eq agg-fn 'avg)
                                     (let ((s 0))
                                       (dolist (r rows) (setq s (+ s (cdr (assq agg-col r)))))
                                       (/ s (length rows))))
                                    ((eq agg-fn 'max)
                                     (let ((m nil))
                                       (dolist (r rows)
                                         (let ((v (cdr (assq agg-col r))))
                                           (when (or (null m) (> v m)) (setq m v))))
                                       m))
                                    ((eq agg-fn 'min)
                                     (let ((m nil))
                                       (dolist (r rows)
                                         (let ((v (cdr (assq agg-col r))))
                                           (when (or (null m) (< v m)) (setq m v))))
                                       m)))))
                        (setq agg-row (append agg-row (list (cons agg-name val))))))
                    ;; HAVING filter
                    (when (or (null having-fn) (funcall having-fn agg-row))
                      (setq results (cons agg-row results)))))
                groups)
               (sort results (lambda (a b) (string< (cdr (assq group-col a))
                                                     (cdr (assq group-col b)))))))))

    (list
      ;; GROUP BY rep: SUM(amount), COUNT, AVG(amount)
      (funcall group-by-agg sales 'rep
               '((total-sales sum amount) (num-sales count nil) (avg-sale avg amount))
               nil)
      ;; GROUP BY rep HAVING total_sales > 1000
      (funcall group-by-agg sales 'rep
               '((total-sales sum amount) (num-sales count nil))
               (lambda (r) (> (cdr (assq 'total-sales r)) 1000)))
      ;; GROUP BY region: MAX(amount), MIN(amount)
      (funcall group-by-agg sales 'region
               '((max-sale max amount) (min-sale min amount) (total sum amount))
               nil)
      ;; GROUP BY region HAVING COUNT > 4
      (funcall group-by-agg sales 'region
               '((cnt count nil) (total sum amount))
               (lambda (r) (> (cdr (assq 'cnt r)) 4))))))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DISTINCT, UNION, INTERSECT — set operations on result sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_distinct_union_intersect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((table-a (list
                   '((name . "Alice") (skill . "Python"))
                   '((name . "Bob") (skill . "Java"))
                   '((name . "Carol") (skill . "Python"))
                   '((name . "Alice") (skill . "Rust"))
                   '((name . "Bob") (skill . "Python"))))
              (table-b (list
                   '((name . "Carol") (skill . "Go"))
                   '((name . "Dave") (skill . "Python"))
                   '((name . "Alice") (skill . "Python"))
                   '((name . "Eve") (skill . "Java")))))
  (let ((db-distinct
         (lambda (tbl col)
           "SELECT DISTINCT col FROM tbl."
           (let ((seen nil))
             (dolist (row tbl)
               (let ((val (cdr (assq col row))))
                 (unless (member val seen)
                   (setq seen (cons val seen)))))
             (sort (nreverse seen) #'string<))))
        (db-union
         (lambda (tbl1 tbl2)
           "UNION: combine rows, remove duplicates."
           (let ((result nil))
             (dolist (row (append tbl1 tbl2))
               (unless (member row result)
                 (setq result (cons row result))))
             (nreverse result))))
        (db-union-all
         (lambda (tbl1 tbl2)
           "UNION ALL: combine all rows, keep duplicates."
           (append tbl1 tbl2)))
        (db-intersect
         (lambda (tbl1 tbl2)
           "INTERSECT: rows in both tables."
           (let ((result nil))
             (dolist (row tbl1)
               (when (member row tbl2)
                 (unless (member row result)
                   (setq result (cons row result)))))
             (nreverse result)))))

    (list
      ;; DISTINCT names from A
      (funcall db-distinct table-a 'name)
      ;; DISTINCT skills from A
      (funcall db-distinct table-a 'skill)
      ;; UNION of A and B (deduped)
      (length (funcall db-union table-a table-b))
      ;; UNION ALL
      (length (funcall db-union-all table-a table-b))
      ;; INTERSECT: rows in both
      (funcall db-intersect table-a table-b)
      ;; INTERSECT names (via distinct on each, then intersect)
      (let ((names-a (funcall db-distinct table-a 'name))
            (names-b (funcall db-distinct table-b 'name)))
        (let ((common nil))
          (dolist (n names-a)
            (when (member n names-b)
              (setq common (cons n common))))
          (sort common #'string<)))
      ;; EXCEPT (A minus B): rows in A but not in B
      (let ((except nil))
        (dolist (row table-a)
          (unless (member row table-b)
            (unless (member row except)
              (setq except (cons row except)))))
        (length except)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Carol\") (\"Java\" \"Python\" \"Rust\") 8 9 (((name . \"Alice\") (skill . \"Python\"))) (\"Alice\" \"Carol\") 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// INSERT/UPDATE/DELETE with constraint checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_insert_update_delete_constraints() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((table (list
                   '((id . 1) (name . "Alice") (email . "alice@test.com") (active . t))
                   '((id . 2) (name . "Bob") (email . "bob@test.com") (active . t))
                   '((id . 3) (name . "Carol") (email . "carol@test.com") (active . nil)))))
  (let ((next-id 4)
        (db-insert
         (lambda (tbl row-data next-id-ref)
           "INSERT with auto-increment id and unique email check."
           (let ((email (cdr (assq 'email row-data)))
                 (duplicate nil))
             ;; Check unique constraint on email
             (dolist (existing tbl)
               (when (equal (cdr (assq 'email existing)) email)
                 (setq duplicate t)))
             (if duplicate
                 (cons 'error tbl)
               (let ((new-row (cons (cons 'id (car next-id-ref)) row-data)))
                 (setcar next-id-ref (1+ (car next-id-ref)))
                 (cons 'ok (append tbl (list new-row))))))))
        (db-update
         (lambda (tbl where-fn updates)
           "UPDATE tbl SET updates WHERE where-fn. UPDATES is alist of (col . new-val)."
           (let ((count 0) (result nil))
             (dolist (row tbl)
               (if (funcall where-fn row)
                   (let ((updated (copy-sequence row)))
                     (dolist (upd updates)
                       (let ((cell (assq (car upd) updated)))
                         (if cell
                             (setcdr cell (cdr upd))
                           (setq updated (append updated (list upd))))))
                     (setq count (1+ count))
                     (setq result (cons updated result)))
                 (setq result (cons row result))))
             (cons count (nreverse result)))))
        (db-delete
         (lambda (tbl where-fn)
           "DELETE FROM tbl WHERE where-fn."
           (let ((count 0) (result nil))
             (dolist (row tbl)
               (if (funcall where-fn row)
                   (setq count (1+ count))
                 (setq result (cons row result))))
             (cons count (nreverse result))))))

    (let* ((id-ref (list next-id))
           ;; INSERT valid row
           (r1 (funcall db-insert table '((name . "Dave") (email . "dave@test.com") (active . t)) id-ref))
           (t1 (cdr r1))
           ;; INSERT duplicate email (should fail)
           (r2 (funcall db-insert t1 '((name . "Fake") (email . "alice@test.com") (active . t)) id-ref))
           ;; UPDATE: set active=nil WHERE name='Bob'
           (r3 (funcall db-update t1
                        (lambda (r) (equal (cdr (assq 'name r)) "Bob"))
                        '((active . nil))))
           (t3 (cdr r3))
           ;; UPDATE: give raise to active employees
           (r4 (funcall db-update t3
                        (lambda (r) (eq (cdr (assq 'active r)) t))
                        '((bonus . 1000))))
           ;; DELETE inactive
           (r5 (funcall db-delete t3
                        (lambda (r) (null (cdr (assq 'active r))))))
           (t5 (cdr r5)))
      (list
        ;; Insert success
        (car r1) (length t1)
        ;; Insert failure (duplicate email)
        (car r2)
        ;; Update count
        (car r3)
        ;; Bob is now inactive
        (cdr (assq 'active (nth 1 t3)))
        ;; Update with bonus
        (car r4)
        ;; Delete count and remaining
        (car r5)
        (length t5)
        (mapcar (lambda (r) (cdr (assq 'name r))) t5)))))"#;
    let expect = expect_test::expect![[r#""OK (ok 4 error 1 nil 2 2 2 (\"Alice\" \"Dave\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Index-based lookup: hash-table index for O(1) lookups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_index_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((users (list
                   '((uid . 1) (uname . "Alice") (age . 30) (city . "NYC"))
                   '((uid . 2) (uname . "Bob") (age . 25) (city . "LA"))
                   '((uid . 3) (uname . "Carol") (age . 35) (city . "NYC"))
                   '((uid . 4) (uname . "Dave") (age . 28) (city . "CHI"))
                   '((uid . 5) (uname . "Eve") (age . 32) (city . "NYC"))
                   '((uid . 6) (uname . "Frank") (age . 25) (city . "LA"))
                   '((uid . 7) (uname . "Grace") (age . 30) (city . "SF"))
                   '((uid . 8) (uname . "Hank") (age . 35) (city . "NYC")))))
  (let ((build-index
         (lambda (tbl col)
           "Build a hash-table index: col-value -> list of rows."
           (let ((idx (make-hash-table :test 'equal)))
             (dolist (row tbl)
               (let* ((key (cdr (assq col row)))
                      (existing (gethash key idx)))
                 (puthash key (cons row existing) idx)))
             ;; Reverse to maintain insertion order
             (maphash (lambda (k v) (puthash k (nreverse v) idx)) idx)
             idx)))
        (index-lookup
         (lambda (idx key)
           "Lookup by index key."
           (gethash key idx))))

    (let* ((city-idx (funcall build-index users 'city))
           (age-idx (funcall build-index users 'age)))
      (list
        ;; Lookup by city
        (mapcar (lambda (r) (cdr (assq 'uname r)))
                (funcall index-lookup city-idx "NYC"))
        (mapcar (lambda (r) (cdr (assq 'uname r)))
                (funcall index-lookup city-idx "LA"))
        ;; Lookup non-existent city
        (funcall index-lookup city-idx "MARS")
        ;; Lookup by age
        (mapcar (lambda (r) (cdr (assq 'uname r)))
                (funcall index-lookup age-idx 25))
        (mapcar (lambda (r) (cdr (assq 'uname r)))
                (funcall index-lookup age-idx 30))
        ;; Count by city using index
        (let ((city-counts nil))
          (maphash (lambda (k v)
                     (setq city-counts (cons (cons k (length v)) city-counts)))
                   city-idx)
          (sort city-counts (lambda (a b) (string< (car a) (car b)))))
        ;; Composite lookup: city='NYC' AND age=35
        (let ((nyc-rows (funcall index-lookup city-idx "NYC")))
          (let ((result nil))
            (dolist (r nyc-rows)
              (when (= (cdr (assq 'age r)) 35)
                (setq result (cons (cdr (assq 'uname r)) result))))
            (nreverse result)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Carol\" \"Eve\" \"Hank\") (\"Bob\" \"Frank\") nil (\"Bob\" \"Frank\") (\"Alice\" \"Grace\") ((\"CHI\" . 1) (\"LA\" . 2) (\"NYC\" . 4) (\"SF\" . 1)) (\"Carol\" \"Hank\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-table query: three-way join with aggregation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_multi_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((departments '(((did . 10) (dname . "Engineering") (budget . 500000))
                       ((did . 20) (dname . "Marketing") (budget . 200000))
                       ((did . 30) (dname . "Sales") (budget . 300000))))
              (employees '(((eid . 1) (ename . "Alice") (did . 10) (salary . 95000))
                          ((eid . 2) (ename . "Bob") (did . 30) (salary . 72000))
                          ((eid . 3) (ename . "Carol") (did . 10) (salary . 88000))
                          ((eid . 4) (ename . "Dave") (did . 20) (salary . 76000))
                          ((eid . 5) (ename . "Eve") (did . 10) (salary . 105000))
                          ((eid . 6) (ename . "Frank") (did . 30) (salary . 68000))))
              (projects '(((pid . 101) (pname . "Alpha") (did . 10) (status . active))
                         ((pid . 102) (pname . "Beta") (did . 10) (status . complete))
                         ((pid . 103) (pname . "Gamma") (did . 20) (status . active))
                         ((pid . 104) (pname . "Delta") (did . 30) (status . active))
                         ((pid . 105) (pname . "Epsilon") (did . 10) (status . active)))))
  (let ((inner-join
         (lambda (left right lkey rkey)
           (let ((ridx (make-hash-table :test 'equal))
                 (result nil))
             (dolist (row right)
               (let* ((key (cdr (assq rkey row)))
                      (ex (gethash key ridx)))
                 (puthash key (cons row ex) ridx)))
             (dolist (lrow left)
               (let ((rrows (gethash (cdr (assq lkey lrow)) ridx)))
                 (dolist (rrow rrows)
                   (let ((merged (copy-sequence lrow)))
                     (dolist (cell rrow)
                       (unless (assq (car cell) merged)
                         (setq merged (append merged (list cell)))))
                     (setq result (cons merged result))))))
             (nreverse result)))))

    ;; Query: For each department, show department name, employee count,
    ;; total salary, active project count, and salary/budget ratio
    (let* ((emp-dept (funcall inner-join employees departments 'did 'did))
           ;; Group employees by department
           (dept-groups (make-hash-table :test 'equal)))
      (dolist (row emp-dept)
        (let* ((dname (cdr (assq 'dname row)))
               (ex (gethash dname dept-groups)))
          (puthash dname (cons row ex) dept-groups)))

      ;; Count active projects per department
      (let ((proj-dept (funcall inner-join projects departments 'did 'did))
            (proj-counts (make-hash-table :test 'equal)))
        (dolist (row proj-dept)
          (when (eq (cdr (assq 'status row)) 'active)
            (let ((dname (cdr (assq 'dname row))))
              (puthash dname (1+ (or (gethash dname proj-counts) 0)) proj-counts))))

        ;; Build final report
        (let ((report nil))
          (maphash
           (lambda (dname emp-rows)
             (let ((emp-count (length emp-rows))
                   (total-salary 0)
                   (budget nil))
               (dolist (r emp-rows)
                 (setq total-salary (+ total-salary (cdr (assq 'salary r))))
                 (unless budget (setq budget (cdr (assq 'budget r)))))
               (setq report
                     (cons (list dname
                                 (cons 'emp-count emp-count)
                                 (cons 'total-salary total-salary)
                                 (cons 'active-projects (or (gethash dname proj-counts) 0))
                                 (cons 'budget budget)
                                 (cons 'salary-ratio (/ (* total-salary 100) budget)))
                           report))))
           dept-groups)
          (sort report (lambda (a b) (string< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Engineering\" (emp-count . 3) (total-salary . 288000) (active-projects . 2) (budget . 500000) (salary-ratio . 57)) (\"Marketing\" (emp-count . 1) (total-salary . 76000) (active-projects . 1) (budget . 200000) (salary-ratio . 38)) (\"Sales\" (emp-count . 2) (total-salary . 140000) (active-projects . 1) (budget . 300000) (salary-ratio . 46)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ORDER BY with multiple sort keys and mixed directions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_db_relational_order_by_multi_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((students (list
                   '((sid . 1) (sname . "Alice") (grade . "A") (score . 95) (year . 3))
                   '((sid . 2) (sname . "Bob") (grade . "B") (score . 85) (year . 2))
                   '((sid . 3) (sname . "Carol") (grade . "A") (score . 92) (year . 3))
                   '((sid . 4) (sname . "Dave") (grade . "C") (score . 78) (year . 1))
                   '((sid . 5) (sname . "Eve") (grade . "A") (score . 95) (year . 2))
                   '((sid . 6) (sname . "Frank") (grade . "B") (score . 82) (year . 3))
                   '((sid . 7) (sname . "Grace") (grade . "A") (score . 98) (year . 1))
                   '((sid . 8) (sname . "Hank") (grade . "B") (score . 85) (year . 1)))))
  (let ((order-by-multi
         (lambda (tbl specs)
           "Sort by multiple keys. SPECS: list of (col direction) where direction is 'asc or 'desc."
           (sort (copy-sequence tbl)
                 (lambda (a b)
                   (let ((result nil) (remaining specs))
                     (while (and remaining (null result))
                       (let* ((spec (car remaining))
                              (col (car spec))
                              (dir (cadr spec))
                              (va (cdr (assq col a)))
                              (vb (cdr (assq col b)))
                              (less (cond
                                      ((and (stringp va) (stringp vb)) (string< va vb))
                                      ((and (numberp va) (numberp vb)) (< va vb))
                                      (t nil)))
                              (greater (cond
                                         ((and (stringp va) (stringp vb)) (string< vb va))
                                         ((and (numberp va) (numberp vb)) (< vb va))
                                         (t nil))))
                         (cond
                           ((and (eq dir 'asc) less) (setq result t))
                           ((and (eq dir 'asc) greater) (setq result nil) (setq remaining nil))
                           ((and (eq dir 'desc) greater) (setq result t))
                           ((and (eq dir 'desc) less) (setq result nil) (setq remaining nil))
                           (t (setq remaining (cdr remaining))))))
                     result))))))

    (list
      ;; ORDER BY grade ASC, score DESC
      (mapcar (lambda (r) (list (cdr (assq 'sname r))
                                (cdr (assq 'grade r))
                                (cdr (assq 'score r))))
              (funcall order-by-multi students '((grade asc) (score desc))))
      ;; ORDER BY year ASC, grade ASC, sname ASC
      (mapcar (lambda (r) (list (cdr (assq 'year r))
                                (cdr (assq 'grade r))
                                (cdr (assq 'sname r))))
              (funcall order-by-multi students '((year asc) (grade asc) (sname asc))))
      ;; ORDER BY score DESC (simple single column)
      (mapcar (lambda (r) (list (cdr (assq 'sname r)) (cdr (assq 'score r))))
              (funcall order-by-multi students '((score desc))))
      ;; Top 3 by score DESC
      (let ((sorted (funcall order-by-multi students '((score desc)))))
        (mapcar (lambda (r) (cdr (assq 'sname r)))
                (let ((result nil) (i 0))
                  (dolist (row sorted)
                    (when (< i 3)
                      (setq result (cons row result))
                      (setq i (1+ i))))
                  (nreverse result)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Grace\" \"A\" 98) (\"Alice\" \"A\" 95) (\"Eve\" \"A\" 95) (\"Carol\" \"A\" 92) (\"Bob\" \"B\" 85) (\"Hank\" \"B\" 85) (\"Frank\" \"B\" 82) (\"Dave\" \"C\" 78)) ((1 \"A\" \"Grace\") (1 \"B\" \"Hank\") (1 \"C\" \"Dave\") (2 \"A\" \"Eve\") (2 \"B\" \"Bob\") (3 \"A\" \"Alice\") (3 \"A\" \"Carol\") (3 \"B\" \"Frank\")) ((\"Grace\" 98) (\"Alice\" 95) (\"Eve\" 95) (\"Carol\" 92) (\"Bob\" 85) (\"Hank\" 85) (\"Frank\" 82) (\"Dave\" 78)) (\"Grace\" \"Alice\" \"Eve\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
