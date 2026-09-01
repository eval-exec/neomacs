//! Complex oracle tests for data transformation patterns in Elisp.
//!
//! Tests JSON-to-XML-like conversion, table pivoting, data normalization,
//! multi-source merge/join, hierarchical flattening/reconstruction,
//! and ETL pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// JSON-to-XML-like conversion (nested alist to tagged string)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dt_alist_to_xml_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert nested alists to an XML-like string representation
    let form = r#"(let ((to-xml nil))
                    (setq to-xml
                          (lambda (node indent)
                            (let ((pad (make-string (* indent 2) ?\s)))
                              (cond
                               ;; Leaf: (tag . "value")
                               ((stringp (cdr node))
                                (concat pad "<" (symbol-name (car node)) ">"
                                        (cdr node)
                                        "</" (symbol-name (car node)) ">\n"))
                               ;; Leaf: (tag . number)
                               ((numberp (cdr node))
                                (concat pad "<" (symbol-name (car node)) ">"
                                        (number-to-string (cdr node))
                                        "</" (symbol-name (car node)) ">\n"))
                               ;; Branch: (tag . ((child1 ...) (child2 ...)))
                               ((listp (cdr node))
                                (let ((tag (symbol-name (car node)))
                                      (children (cdr node))
                                      (result ""))
                                  (setq result
                                        (concat result pad "<" tag ">\n"))
                                  (dolist (child children)
                                    (setq result
                                          (concat result
                                                  (funcall to-xml child
                                                           (1+ indent)))))
                                  (concat result pad "</" tag ">\n")))
                               (t (concat pad "<!-- unknown -->\n"))))))
                    (let ((data '(person
                                  (name . "Alice")
                                  (age . 30)
                                  (address
                                   (city . "Wonderland")
                                   (zip . "12345"))
                                  (hobbies
                                   (hobby . "reading")
                                   (hobby . "chess")))))
                      (funcall to-xml data 0)))"#;
    let expect = expect_test::expect![[
        r#""OK \"<person>\\n  <name>Alice</name>\\n  <age>30</age>\\n  <address>\\n    <city>Wonderland</city>\\n    <zip>12345</zip>\\n  </address>\\n  <hobbies>\\n    <hobby>reading</hobby>\\n    <hobby>chess</hobby>\\n  </hobbies>\\n</person>\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Table pivot: rows to columns and back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dt_table_pivot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pivot a table (list of alists) from row-oriented to column-oriented and back
    let form = r#"(let ((rows '(((name . "Alice") (score . 90) (grade . "A"))
                                ((name . "Bob")   (score . 75) (grade . "B"))
                                ((name . "Carol") (score . 85) (grade . "A"))))
                        ;; Pivot rows → columns: produce alist of (key . (val1 val2 val3))
                        (pivot-to-cols
                         (lambda (rows)
                           (let ((cols nil))
                             (dolist (row rows)
                               (dolist (cell row)
                                 (let ((key (car cell))
                                       (val (cdr cell)))
                                   (let ((existing (assq key cols)))
                                     (if existing
                                         (setcdr existing
                                                 (append (cdr existing) (list val)))
                                       (setq cols
                                             (append cols
                                                     (list (list key val)))))))))
                             cols)))
                        ;; Pivot columns → rows: inverse operation
                        (pivot-to-rows
                         (lambda (cols)
                           (let ((num-rows (length (cdar cols)))
                                 (result nil))
                             (dotimes (i num-rows)
                               (let ((row nil))
                                 (dolist (col cols)
                                   (setq row
                                         (append row
                                                 (list (cons (car col)
                                                             (nth i (cdr col)))))))
                                 (setq result (append result (list row)))))
                             result))))
                    (let* ((cols (funcall pivot-to-cols rows))
                           (back (funcall pivot-to-rows cols)))
                      (list
                        ;; Column-oriented form
                        cols
                        ;; Round-trip should give back original
                        (equal back rows))))"#;
    let expect = expect_test::expect![[
        r#""OK (((name \"Alice\" \"Bob\" \"Carol\") (score 90 75 85) (grade \"A\" \"B\" \"A\")) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Data normalization pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dt_normalization_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pipeline: trim whitespace, downcase, validate format, transform
    let form = r#"(let ((trim-whitespace
                         (lambda (s)
                           (let ((start 0)
                                 (end (length s)))
                             (while (and (< start end)
                                         (= (aref s start) ?\s))
                               (setq start (1+ start)))
                             (while (and (> end start)
                                         (= (aref s (1- end)) ?\s))
                               (setq end (1- end)))
                             (substring s start end))))
                        (validate-email-like
                         (lambda (s)
                           ;; Simple check: contains exactly one @, non-empty parts
                           (let ((at-pos nil)
                                 (count 0)
                                 (i 0))
                             (while (< i (length s))
                               (when (= (aref s i) ?@)
                                 (setq at-pos i count (1+ count)))
                               (setq i (1+ i)))
                             (and (= count 1)
                                  (> at-pos 0)
                                  (< at-pos (1- (length s)))))))
                        (normalize-record
                         (lambda (record trim validate)
                           (let ((name (funcall trim (cdr (assq 'name record))))
                                 (email (downcase (funcall trim (cdr (assq 'email record)))))
                                 (role (cdr (assq 'role record))))
                             (list (cons 'name name)
                                   (cons 'email email)
                                   (cons 'valid (funcall validate email))
                                   (cons 'role (or role "unknown")))))))
                    ;; Raw data with messy whitespace, mixed case
                    (let ((records '(((name . "  Alice  ") (email . " Alice@Example.COM ") (role . "admin"))
                                     ((name . "Bob") (email . "bob-at-example") (role . "user"))
                                     ((name . "  Carol ") (email . " Carol@Test.Org  ") (role . nil)))))
                      (mapcar (lambda (r)
                                (funcall normalize-record r
                                         trim-whitespace validate-email-like))
                              records)))"#;
    let expect = expect_test::expect![[
        r#""OK (((name . \"Alice\") (email . \"alice@example.com\") (valid . t) (role . \"admin\")) ((name . \"Bob\") (email . \"bob-at-example\") (valid) (role . \"user\")) ((name . \"Carol\") (email . \"carol@test.org\") (valid . t) (role . \"unknown\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Merge/join of multiple data sources
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dt_merge_join_data_sources() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Left-join two tables on a shared key, like a database join
    let form = r#"(let ((users '((1 . "Alice") (2 . "Bob") (3 . "Carol") (4 . "Dave")))
                        (orders '((1 . "Book") (1 . "Pen") (2 . "Laptop") (3 . "Phone") (5 . "Tablet")))
                        ;; Left join: for each user, find matching orders
                        (left-join
                         (lambda (left right)
                           (let ((result nil))
                             (dolist (l left)
                               (let ((uid (car l))
                                     (uname (cdr l))
                                     (user-orders nil))
                                 (dolist (r right)
                                   (when (= (car r) uid)
                                     (setq user-orders
                                           (cons (cdr r) user-orders))))
                                 (setq result
                                       (cons (list uid uname
                                                   (nreverse user-orders))
                                             result))))
                             (nreverse result))))
                        ;; Group-by: collect orders by user id
                        (group-by-key
                         (lambda (pairs)
                           (let ((groups (make-hash-table)))
                             (dolist (p pairs)
                               (let ((existing (gethash (car p) groups)))
                                 (puthash (car p)
                                          (append (or existing nil)
                                                  (list (cdr p)))
                                          groups)))
                             groups))))
                    (let ((joined (funcall left-join users orders))
                          (grouped (funcall group-by-key orders)))
                      (list
                        joined
                        ;; Verify: user 4 has no orders
                        (nth 2 (nth 3 joined))
                        ;; Verify: user 1 has 2 orders
                        (length (nth 2 (car joined)))
                        ;; Group-by result for user 1
                        (gethash 1 grouped))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 \"Alice\" (\"Book\" \"Pen\")) (2 \"Bob\" (\"Laptop\")) (3 \"Carol\" (\"Phone\")) (4 \"Dave\" nil)) nil 2 (\"Book\" \"Pen\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hierarchical data flattening and reconstruction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dt_hierarchical_flatten_reconstruct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flatten a nested tree to a flat list with path keys, then reconstruct
    let form = r#"(let ((flatten-tree nil)
                        (unflatten nil))
                    ;; Flatten: (a (b . 1) (c (d . 2) (e . 3))) → (("a/b" . 1) ("a/c/d" . 2) ("a/c/e" . 3))
                    (setq flatten-tree
                          (lambda (node path)
                            (let ((tag (symbol-name (car node)))
                                  (children (cdr node))
                                  (current-path
                                   (if (string= path "")
                                       (symbol-name (car node))
                                     (concat path "/" (symbol-name (car node))))))
                              (cond
                               ;; Leaf: atom value
                               ((not (listp children))
                                (list (cons current-path children)))
                               ;; Branch: recurse into children
                               (t
                                (let ((result nil))
                                  (dolist (child children)
                                    (setq result
                                          (append result
                                                  (funcall flatten-tree child
                                                           current-path))))
                                  result))))))
                    ;; Unflatten: reverse the process using path splitting
                    (setq unflatten
                          (lambda (flat-list)
                            ;; Reconstruct by grouping paths into a nested alist
                            (let ((result nil))
                              (dolist (pair flat-list)
                                (setq result
                                      (cons (cons (car pair) (cdr pair))
                                            result)))
                              (nreverse result))))
                    (let ((tree '(root
                                  (config
                                   (debug . t)
                                   (level . 3))
                                  (data
                                   (name . "test")
                                   (items
                                    (count . 5)
                                    (total . 100))))))
                      (let ((flat (funcall flatten-tree tree "")))
                        (list
                          flat
                          (length flat)
                          ;; Verify all leaf values preserved
                          (cdr (assoc "root/config/debug" flat))
                          (cdr (assoc "root/config/level" flat))
                          (cdr (assoc "root/data/name" flat))
                          (cdr (assoc "root/data/items/count" flat))
                          (cdr (assoc "root/data/items/total" flat))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"root/config/debug\" . t) (\"root/config/level\" . 3) (\"root/data/name\" . \"test\") (\"root/data/items/count\" . 5) (\"root/data/items/total\" . 100)) 5 t 3 \"test\" 5 100)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ETL pipeline: extract from string, transform, load to structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_dt_etl_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV-like string, transform data, aggregate into summary structure
    let form = r#"(let ((parse-csv-line
                         (lambda (line)
                           ;; Split line by commas into a list of strings
                           (let ((fields nil)
                                 (start 0)
                                 (i 0))
                             (while (< i (length line))
                               (when (= (aref line i) ?,)
                                 (setq fields
                                       (cons (substring line start i) fields))
                                 (setq start (1+ i)))
                               (setq i (1+ i)))
                             (setq fields
                                   (cons (substring line start) fields))
                             (nreverse fields))))
                        (parse-csv
                         (lambda (text parse-line)
                           ;; Split by newlines, parse each line
                           (let ((lines nil)
                                 (start 0)
                                 (i 0))
                             (while (< i (length text))
                               (when (= (aref text i) ?\n)
                                 (let ((line (substring text start i)))
                                   (when (> (length line) 0)
                                     (setq lines (cons (funcall parse-line line) lines))))
                                 (setq start (1+ i)))
                               (setq i (1+ i)))
                             (when (< start (length text))
                               (let ((line (substring text start)))
                                 (when (> (length line) 0)
                                   (setq lines (cons (funcall parse-line line) lines)))))
                             (nreverse lines)))))
                    ;; ETL: Extract CSV, Transform (compute derived fields), Load (aggregate)
                    (let* ((csv-data "Alice,Engineering,90000\nBob,Sales,70000\nCarol,Engineering,95000\nDave,Sales,65000\nEve,Engineering,88000")
                           (rows (funcall parse-csv csv-data parse-csv-line))
                           ;; Transform: add department tag and parse salary
                           (transformed
                            (mapcar (lambda (row)
                                      (list (cons 'name (nth 0 row))
                                            (cons 'dept (nth 1 row))
                                            (cons 'salary (string-to-number (nth 2 row)))))
                                    rows))
                           ;; Load/Aggregate: group by department, compute avg salary
                           (dept-totals (make-hash-table :test 'equal))
                           (dept-counts (make-hash-table :test 'equal)))
                      ;; Aggregate
                      (dolist (rec transformed)
                        (let ((dept (cdr (assq 'dept rec)))
                              (sal (cdr (assq 'salary rec))))
                          (puthash dept (+ (gethash dept dept-totals 0) sal) dept-totals)
                          (puthash dept (1+ (gethash dept dept-counts 0)) dept-counts)))
                      ;; Build summary
                      (let ((summary nil))
                        (maphash (lambda (dept total)
                                   (let ((count (gethash dept dept-counts)))
                                     (setq summary
                                           (cons (list dept
                                                       (cons 'count count)
                                                       (cons 'total total)
                                                       (cons 'avg (/ total count)))
                                                 summary))))
                                 dept-totals)
                        (list
                          ;; Number of records parsed
                          (length transformed)
                          ;; Summary sorted by department name
                          (sort summary
                                (lambda (a b) (string< (car a) (car b))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 ((\"Engineering\" (count . 3) (total . 273000) (avg . 91000)) (\"Sales\" (count . 2) (total . 135000) (avg . 67500))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
