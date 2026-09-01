//! Oracle parity tests for a skip list data structure implemented in Elisp.
//!
//! A skip list is a probabilistic data structure that allows O(log n) average
//! search, insert, and delete. It consists of multiple levels of sorted linked
//! lists, where higher levels skip over more elements.
//!
//! Node representation: (key value level forward-pointers)
//! where forward-pointers is a vector of next-node references per level.
//! Skip list header: (header max-level size)

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. Skip list: create, insert, search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_list_create_insert_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a skip list with deterministic level assignment (based on key hash)
    // for reproducibility, then test insert and search operations.
    let form = r#"(unwind-protect
      (progn
        ;; Node: (key value forward-ptrs) where forward-ptrs is a vector of size level
        (defun test-sl--make-node (key value level)
          (list key value (make-vector level nil)))

        (defun test-sl--node-key (node) (car node))
        (defun test-sl--node-value (node) (cadr node))
        (defun test-sl--node-forward (node) (caddr node))

        ;; Skip list: (header max-level size)
        (defun test-sl--create (max-level)
          (list (test-sl--make-node nil nil max-level)
                max-level
                0))

        (defun test-sl--header (sl) (car sl))
        (defun test-sl--max-level (sl) (cadr sl))
        (defun test-sl--size (sl) (caddr sl))

        ;; Deterministic level: use modular hash to pick level 1..max-level
        (defun test-sl--random-level (key max-level)
          (let ((h (abs (sxhash key)))
                (level 1))
            (while (and (< level max-level)
                        (= (% h 4) 0))
              (setq level (1+ level))
              (setq h (/ h 4)))
            level))

        ;; Search: returns (value . t) if found, nil otherwise
        (defun test-sl--search (sl key)
          (let* ((header (test-sl--header sl))
                 (max-lvl (test-sl--max-level sl))
                 (current header)
                 (found nil))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl--node-forward current) lvl)))
                  (while (and next (< (test-sl--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl--node-forward current) lvl))))
                (setq lvl (1- lvl))))
            ;; Check the next node at level 0
            (let ((next (aref (test-sl--node-forward current) 0)))
              (if (and next (= (test-sl--node-key next) key))
                  (cons (test-sl--node-value next) t)
                nil))))

        ;; Insert: adds or updates key-value pair
        (defun test-sl--insert (sl key value)
          (let* ((header (test-sl--header sl))
                 (max-lvl (test-sl--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            ;; Find position and record update path
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl--node-forward current) lvl)))
                  (while (and next (< (test-sl--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            ;; Check if key already exists
            (let ((next (aref (test-sl--node-forward current) 0)))
              (if (and next (= (test-sl--node-key next) key))
                  ;; Update existing value
                  (setcar (cdr next) value)
                ;; Insert new node
                (let* ((new-level (test-sl--random-level key max-lvl))
                       (new-node (test-sl--make-node key value new-level)))
                  (let ((i 0))
                    (while (< i new-level)
                      (let ((prev (aref update i)))
                        (when prev
                          (aset (test-sl--node-forward new-node) i
                                (aref (test-sl--node-forward prev) i))
                          (aset (test-sl--node-forward prev) i new-node)))
                      (setq i (1+ i))))
                  (setcar (cddr sl) (1+ (test-sl--size sl))))))))

        ;; Test: create, insert, search
        (let ((sl (test-sl--create 4)))
          (test-sl--insert sl 10 "ten")
          (test-sl--insert sl 5 "five")
          (test-sl--insert sl 20 "twenty")
          (test-sl--insert sl 15 "fifteen")
          (test-sl--insert sl 1 "one")
          (list
            (test-sl--size sl)
            (test-sl--search sl 10)
            (test-sl--search sl 5)
            (test-sl--search sl 20)
            (test-sl--search sl 15)
            (test-sl--search sl 1)
            ;; Not found
            (test-sl--search sl 99)
            (test-sl--search sl 0)
            ;; Update existing key
            (progn (test-sl--insert sl 10 "TEN") nil)
            (test-sl--search sl 10)
            (test-sl--size sl))))
      ;; Cleanup
      (fmakunbound 'test-sl--make-node)
      (fmakunbound 'test-sl--node-key)
      (fmakunbound 'test-sl--node-value)
      (fmakunbound 'test-sl--node-forward)
      (fmakunbound 'test-sl--create)
      (fmakunbound 'test-sl--header)
      (fmakunbound 'test-sl--max-level)
      (fmakunbound 'test-sl--size)
      (fmakunbound 'test-sl--random-level)
      (fmakunbound 'test-sl--search)
      (fmakunbound 'test-sl--insert))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (\"ten\" . t) (\"five\" . t) (\"twenty\" . t) (\"fifteen\" . t) (\"one\" . t) nil nil nil (\"TEN\" . t) 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. Skip list: delete operation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_list_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement and test delete on a skip list.
    let form = r#"(unwind-protect
      (progn
        (defun test-sl2--make-node (key value level)
          (list key value (make-vector level nil)))
        (defun test-sl2--node-key (node) (car node))
        (defun test-sl2--node-value (node) (cadr node))
        (defun test-sl2--node-forward (node) (caddr node))

        (defun test-sl2--create (max-level)
          (list (test-sl2--make-node nil nil max-level) max-level 0))
        (defun test-sl2--header (sl) (car sl))
        (defun test-sl2--max-level (sl) (cadr sl))
        (defun test-sl2--size (sl) (caddr sl))

        (defun test-sl2--random-level (key max-level)
          (let ((h (abs (sxhash key))) (level 1))
            (while (and (< level max-level) (= (% h 4) 0))
              (setq level (1+ level)) (setq h (/ h 4)))
            level))

        (defun test-sl2--search (sl key)
          (let* ((current (test-sl2--header sl))
                 (max-lvl (test-sl2--max-level sl))
                 (lvl (1- max-lvl)))
            (while (>= lvl 0)
              (let ((next (aref (test-sl2--node-forward current) lvl)))
                (while (and next (< (test-sl2--node-key next) key))
                  (setq current next)
                  (setq next (aref (test-sl2--node-forward current) lvl))))
              (setq lvl (1- lvl)))
            (let ((next (aref (test-sl2--node-forward current) 0)))
              (if (and next (= (test-sl2--node-key next) key))
                  (cons (test-sl2--node-value next) t)
                nil))))

        (defun test-sl2--insert (sl key value)
          (let* ((header (test-sl2--header sl))
                 (max-lvl (test-sl2--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl2--node-forward current) lvl)))
                  (while (and next (< (test-sl2--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl2--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((next (aref (test-sl2--node-forward current) 0)))
              (if (and next (= (test-sl2--node-key next) key))
                  (setcar (cdr next) value)
                (let* ((new-level (test-sl2--random-level key max-lvl))
                       (new-node (test-sl2--make-node key value new-level)))
                  (let ((i 0))
                    (while (< i new-level)
                      (let ((prev (aref update i)))
                        (when prev
                          (aset (test-sl2--node-forward new-node) i
                                (aref (test-sl2--node-forward prev) i))
                          (aset (test-sl2--node-forward prev) i new-node)))
                      (setq i (1+ i))))
                  (setcar (cddr sl) (1+ (test-sl2--size sl))))))))

        ;; Delete: removes key, returns t if found, nil otherwise
        (defun test-sl2--delete (sl key)
          (let* ((header (test-sl2--header sl))
                 (max-lvl (test-sl2--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl2--node-forward current) lvl)))
                  (while (and next (< (test-sl2--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl2--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((target (aref (test-sl2--node-forward current) 0)))
              (if (and target (= (test-sl2--node-key target) key))
                  (progn
                    (let ((i 0)
                          (target-fwd (test-sl2--node-forward target)))
                      (while (< i (length target-fwd))
                        (let ((prev (aref update i)))
                          (when (and prev
                                     (eq (aref (test-sl2--node-forward prev) i) target))
                            (aset (test-sl2--node-forward prev) i
                                  (aref target-fwd i))))
                        (setq i (1+ i))))
                    (setcar (cddr sl) (1- (test-sl2--size sl)))
                    t)
                nil))))

        ;; Test delete
        (let ((sl (test-sl2--create 4)))
          (test-sl2--insert sl 10 "ten")
          (test-sl2--insert sl 20 "twenty")
          (test-sl2--insert sl 30 "thirty")
          (test-sl2--insert sl 40 "forty")
          (list
            (test-sl2--size sl)
            ;; Delete existing key
            (test-sl2--delete sl 20)
            (test-sl2--size sl)
            (test-sl2--search sl 20)
            ;; Other keys still present
            (test-sl2--search sl 10)
            (test-sl2--search sl 30)
            (test-sl2--search sl 40)
            ;; Delete non-existent key
            (test-sl2--delete sl 99)
            (test-sl2--size sl)
            ;; Delete first and last
            (test-sl2--delete sl 10)
            (test-sl2--delete sl 40)
            (test-sl2--size sl)
            (test-sl2--search sl 10)
            (test-sl2--search sl 40)
            (test-sl2--search sl 30))))
      ;; Cleanup
      (fmakunbound 'test-sl2--make-node)
      (fmakunbound 'test-sl2--node-key)
      (fmakunbound 'test-sl2--node-value)
      (fmakunbound 'test-sl2--node-forward)
      (fmakunbound 'test-sl2--create)
      (fmakunbound 'test-sl2--header)
      (fmakunbound 'test-sl2--max-level)
      (fmakunbound 'test-sl2--size)
      (fmakunbound 'test-sl2--random-level)
      (fmakunbound 'test-sl2--search)
      (fmakunbound 'test-sl2--insert)
      (fmakunbound 'test-sl2--delete))"#;
    let expect = expect_test::expect![[
        r#""OK (4 t 3 nil (\"ten\" . t) (\"thirty\" . t) (\"forty\" . t) nil 3 t t 1 nil nil (\"thirty\" . t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. Skip list: ordered iteration (in-order traversal)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_list_ordered_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert keys in random order, then iterate at level 0 to verify
    // they come out in sorted order.
    let form = r#"(unwind-protect
      (progn
        (defun test-sl3--make-node (key value level)
          (list key value (make-vector level nil)))
        (defun test-sl3--node-key (node) (car node))
        (defun test-sl3--node-value (node) (cadr node))
        (defun test-sl3--node-forward (node) (caddr node))
        (defun test-sl3--create (max-level)
          (list (test-sl3--make-node nil nil max-level) max-level 0))
        (defun test-sl3--header (sl) (car sl))
        (defun test-sl3--max-level (sl) (cadr sl))
        (defun test-sl3--size (sl) (caddr sl))
        (defun test-sl3--random-level (key max-level)
          (let ((h (abs (sxhash key))) (level 1))
            (while (and (< level max-level) (= (% h 4) 0))
              (setq level (1+ level)) (setq h (/ h 4)))
            level))
        (defun test-sl3--insert (sl key value)
          (let* ((header (test-sl3--header sl))
                 (max-lvl (test-sl3--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl3--node-forward current) lvl)))
                  (while (and next (< (test-sl3--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl3--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((next (aref (test-sl3--node-forward current) 0)))
              (if (and next (= (test-sl3--node-key next) key))
                  (setcar (cdr next) value)
                (let* ((new-level (test-sl3--random-level key max-lvl))
                       (new-node (test-sl3--make-node key value new-level)))
                  (let ((i 0))
                    (while (< i new-level)
                      (let ((prev (aref update i)))
                        (when prev
                          (aset (test-sl3--node-forward new-node) i
                                (aref (test-sl3--node-forward prev) i))
                          (aset (test-sl3--node-forward prev) i new-node)))
                      (setq i (1+ i))))
                  (setcar (cddr sl) (1+ (test-sl3--size sl))))))))

        ;; Collect all key-value pairs in sorted order
        (defun test-sl3--to-alist (sl)
          (let ((node (aref (test-sl3--node-forward (test-sl3--header sl)) 0))
                (result nil))
            (while node
              (push (cons (test-sl3--node-key node) (test-sl3--node-value node)) result)
              (setq node (aref (test-sl3--node-forward node) 0)))
            (nreverse result)))

        ;; Collect just keys
        (defun test-sl3--keys (sl)
          (mapcar #'car (test-sl3--to-alist sl)))

        ;; Test: insert in scrambled order, verify sorted output
        (let ((sl (test-sl3--create 4)))
          (test-sl3--insert sl 50 "fifty")
          (test-sl3--insert sl 10 "ten")
          (test-sl3--insert sl 30 "thirty")
          (test-sl3--insert sl 70 "seventy")
          (test-sl3--insert sl 20 "twenty")
          (test-sl3--insert sl 60 "sixty")
          (test-sl3--insert sl 40 "forty")
          (let ((keys (test-sl3--keys sl))
                (alist (test-sl3--to-alist sl)))
            (list
              keys
              ;; Verify sorted
              (equal keys '(10 20 30 40 50 60 70))
              ;; Verify values
              alist
              (test-sl3--size sl)))))
      ;; Cleanup
      (fmakunbound 'test-sl3--make-node)
      (fmakunbound 'test-sl3--node-key)
      (fmakunbound 'test-sl3--node-value)
      (fmakunbound 'test-sl3--node-forward)
      (fmakunbound 'test-sl3--create)
      (fmakunbound 'test-sl3--header)
      (fmakunbound 'test-sl3--max-level)
      (fmakunbound 'test-sl3--size)
      (fmakunbound 'test-sl3--random-level)
      (fmakunbound 'test-sl3--insert)
      (fmakunbound 'test-sl3--to-alist)
      (fmakunbound 'test-sl3--keys))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40 50 60 70) t ((10 . \"ten\") (20 . \"twenty\") (30 . \"thirty\") (40 . \"forty\") (50 . \"fifty\") (60 . \"sixty\") (70 . \"seventy\")) 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. Skip list: range queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_list_range_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement range query: find all keys in [lo, hi].
    let form = r#"(unwind-protect
      (progn
        (defun test-sl4--make-node (key value level)
          (list key value (make-vector level nil)))
        (defun test-sl4--node-key (node) (car node))
        (defun test-sl4--node-value (node) (cadr node))
        (defun test-sl4--node-forward (node) (caddr node))
        (defun test-sl4--create (max-level)
          (list (test-sl4--make-node nil nil max-level) max-level 0))
        (defun test-sl4--header (sl) (car sl))
        (defun test-sl4--max-level (sl) (cadr sl))
        (defun test-sl4--size (sl) (caddr sl))
        (defun test-sl4--random-level (key max-level)
          (let ((h (abs (sxhash key))) (level 1))
            (while (and (< level max-level) (= (% h 4) 0))
              (setq level (1+ level)) (setq h (/ h 4)))
            level))
        (defun test-sl4--insert (sl key value)
          (let* ((header (test-sl4--header sl))
                 (max-lvl (test-sl4--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl4--node-forward current) lvl)))
                  (while (and next (< (test-sl4--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl4--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((next (aref (test-sl4--node-forward current) 0)))
              (if (and next (= (test-sl4--node-key next) key))
                  (setcar (cdr next) value)
                (let* ((new-level (test-sl4--random-level key max-lvl))
                       (new-node (test-sl4--make-node key value new-level)))
                  (let ((i 0))
                    (while (< i new-level)
                      (let ((prev (aref update i)))
                        (when prev
                          (aset (test-sl4--node-forward new-node) i
                                (aref (test-sl4--node-forward prev) i))
                          (aset (test-sl4--node-forward prev) i new-node)))
                      (setq i (1+ i))))
                  (setcar (cddr sl) (1+ (test-sl4--size sl))))))))

        ;; Range query: return alist of all (key . value) where lo <= key <= hi
        (defun test-sl4--range (sl lo hi)
          (let* ((current (test-sl4--header sl))
                 (max-lvl (test-sl4--max-level sl))
                 (result nil))
            ;; Navigate to first key >= lo using higher levels
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl4--node-forward current) lvl)))
                  (while (and next (< (test-sl4--node-key next) lo))
                    (setq current next)
                    (setq next (aref (test-sl4--node-forward current) lvl))))
                (setq lvl (1- lvl))))
            ;; Collect at level 0
            (let ((node (aref (test-sl4--node-forward current) 0)))
              (while (and node (<= (test-sl4--node-key node) hi))
                (push (cons (test-sl4--node-key node)
                            (test-sl4--node-value node))
                      result)
                (setq node (aref (test-sl4--node-forward node) 0))))
            (nreverse result)))

        ;; Test
        (let ((sl (test-sl4--create 4)))
          (dolist (k '(5 10 15 20 25 30 35 40 45 50))
            (test-sl4--insert sl k (* k 10)))
          (list
            ;; Full range
            (test-sl4--range sl 5 50)
            ;; Sub-range
            (test-sl4--range sl 15 35)
            ;; Single element range
            (test-sl4--range sl 20 20)
            ;; Empty range (no keys in range)
            (test-sl4--range sl 6 9)
            ;; Range at boundaries
            (test-sl4--range sl 1 10)
            (test-sl4--range sl 45 100)
            ;; Size
            (test-sl4--size sl))))
      ;; Cleanup
      (fmakunbound 'test-sl4--make-node)
      (fmakunbound 'test-sl4--node-key)
      (fmakunbound 'test-sl4--node-value)
      (fmakunbound 'test-sl4--node-forward)
      (fmakunbound 'test-sl4--create)
      (fmakunbound 'test-sl4--header)
      (fmakunbound 'test-sl4--max-level)
      (fmakunbound 'test-sl4--size)
      (fmakunbound 'test-sl4--random-level)
      (fmakunbound 'test-sl4--insert)
      (fmakunbound 'test-sl4--range))"#;
    let expect = expect_test::expect![[
        r#""OK (((5 . 50) (10 . 100) (15 . 150) (20 . 200) (25 . 250) (30 . 300) (35 . 350) (40 . 400) (45 . 450) (50 . 500)) ((15 . 150) (20 . 200) (25 . 250) (30 . 300) (35 . 350)) ((20 . 200)) nil ((5 . 50) (10 . 100)) ((45 . 450) (50 . 500)) 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. Skip list: bulk insert + delete + verify integrity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_list_bulk_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bulk insert many keys, delete some, verify remaining structure.
    let form = r#"(unwind-protect
      (progn
        (defun test-sl5--make-node (key value level)
          (list key value (make-vector level nil)))
        (defun test-sl5--node-key (node) (car node))
        (defun test-sl5--node-value (node) (cadr node))
        (defun test-sl5--node-forward (node) (caddr node))
        (defun test-sl5--create (max-level)
          (list (test-sl5--make-node nil nil max-level) max-level 0))
        (defun test-sl5--header (sl) (car sl))
        (defun test-sl5--max-level (sl) (cadr sl))
        (defun test-sl5--size (sl) (caddr sl))
        (defun test-sl5--random-level (key max-level)
          (let ((h (abs (sxhash key))) (level 1))
            (while (and (< level max-level) (= (% h 4) 0))
              (setq level (1+ level)) (setq h (/ h 4)))
            level))
        (defun test-sl5--search (sl key)
          (let* ((current (test-sl5--header sl))
                 (max-lvl (test-sl5--max-level sl))
                 (lvl (1- max-lvl)))
            (while (>= lvl 0)
              (let ((next (aref (test-sl5--node-forward current) lvl)))
                (while (and next (< (test-sl5--node-key next) key))
                  (setq current next)
                  (setq next (aref (test-sl5--node-forward current) lvl))))
              (setq lvl (1- lvl)))
            (let ((next (aref (test-sl5--node-forward current) 0)))
              (if (and next (= (test-sl5--node-key next) key))
                  (cons (test-sl5--node-value next) t)
                nil))))
        (defun test-sl5--insert (sl key value)
          (let* ((header (test-sl5--header sl))
                 (max-lvl (test-sl5--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl5--node-forward current) lvl)))
                  (while (and next (< (test-sl5--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl5--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((next (aref (test-sl5--node-forward current) 0)))
              (if (and next (= (test-sl5--node-key next) key))
                  (setcar (cdr next) value)
                (let* ((new-level (test-sl5--random-level key max-lvl))
                       (new-node (test-sl5--make-node key value new-level)))
                  (let ((i 0))
                    (while (< i new-level)
                      (let ((prev (aref update i)))
                        (when prev
                          (aset (test-sl5--node-forward new-node) i
                                (aref (test-sl5--node-forward prev) i))
                          (aset (test-sl5--node-forward prev) i new-node)))
                      (setq i (1+ i))))
                  (setcar (cddr sl) (1+ (test-sl5--size sl))))))))
        (defun test-sl5--delete (sl key)
          (let* ((header (test-sl5--header sl))
                 (max-lvl (test-sl5--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl5--node-forward current) lvl)))
                  (while (and next (< (test-sl5--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl5--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((target (aref (test-sl5--node-forward current) 0)))
              (if (and target (= (test-sl5--node-key target) key))
                  (progn
                    (let ((i 0)
                          (target-fwd (test-sl5--node-forward target)))
                      (while (< i (length target-fwd))
                        (let ((prev (aref update i)))
                          (when (and prev
                                     (eq (aref (test-sl5--node-forward prev) i) target))
                            (aset (test-sl5--node-forward prev) i
                                  (aref target-fwd i))))
                        (setq i (1+ i))))
                    (setcar (cddr sl) (1- (test-sl5--size sl)))
                    t)
                nil))))
        (defun test-sl5--keys (sl)
          (let ((node (aref (test-sl5--node-forward (test-sl5--header sl)) 0))
                (result nil))
            (while node
              (push (test-sl5--node-key node) result)
              (setq node (aref (test-sl5--node-forward node) 0)))
            (nreverse result)))

        ;; Bulk test
        (let ((sl (test-sl5--create 5)))
          ;; Insert 1..20
          (let ((i 1))
            (while (<= i 20)
              (test-sl5--insert sl i (* i i))
              (setq i (1+ i))))
          (let ((size-before (test-sl5--size sl))
                (keys-before (test-sl5--keys sl)))
            ;; Delete all even keys
            (let ((i 2))
              (while (<= i 20)
                (test-sl5--delete sl i)
                (setq i (+ i 2))))
            (let ((keys-after (test-sl5--keys sl)))
              (list
                size-before
                (equal keys-before '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20))
                (test-sl5--size sl)
                keys-after
                (equal keys-after '(1 3 5 7 9 11 13 15 17 19))
                ;; Spot checks
                (test-sl5--search sl 3)
                (test-sl5--search sl 4)
                (test-sl5--search sl 19)
                (test-sl5--search sl 20))))))
      ;; Cleanup
      (fmakunbound 'test-sl5--make-node)
      (fmakunbound 'test-sl5--node-key)
      (fmakunbound 'test-sl5--node-value)
      (fmakunbound 'test-sl5--node-forward)
      (fmakunbound 'test-sl5--create)
      (fmakunbound 'test-sl5--header)
      (fmakunbound 'test-sl5--max-level)
      (fmakunbound 'test-sl5--size)
      (fmakunbound 'test-sl5--random-level)
      (fmakunbound 'test-sl5--search)
      (fmakunbound 'test-sl5--insert)
      (fmakunbound 'test-sl5--delete)
      (fmakunbound 'test-sl5--keys))"#;
    let expect = expect_test::expect![[
        r#""OK (20 t 10 (1 3 5 7 9 11 13 15 17 19) t (9 . t) nil (361 . t) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. Skip list: min, max, floor, ceiling queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_list_min_max_floor_ceiling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement min (smallest key), max (largest key),
    // floor (largest key <= given), ceiling (smallest key >= given).
    let form = r#"(unwind-protect
      (progn
        (defun test-sl6--make-node (key value level)
          (list key value (make-vector level nil)))
        (defun test-sl6--node-key (node) (car node))
        (defun test-sl6--node-value (node) (cadr node))
        (defun test-sl6--node-forward (node) (caddr node))
        (defun test-sl6--create (max-level)
          (list (test-sl6--make-node nil nil max-level) max-level 0))
        (defun test-sl6--header (sl) (car sl))
        (defun test-sl6--max-level (sl) (cadr sl))
        (defun test-sl6--size (sl) (caddr sl))
        (defun test-sl6--random-level (key max-level)
          (let ((h (abs (sxhash key))) (level 1))
            (while (and (< level max-level) (= (% h 4) 0))
              (setq level (1+ level)) (setq h (/ h 4)))
            level))
        (defun test-sl6--insert (sl key value)
          (let* ((header (test-sl6--header sl))
                 (max-lvl (test-sl6--max-level sl))
                 (update (make-vector max-lvl nil))
                 (current header))
            (let ((lvl (1- max-lvl)))
              (while (>= lvl 0)
                (let ((next (aref (test-sl6--node-forward current) lvl)))
                  (while (and next (< (test-sl6--node-key next) key))
                    (setq current next)
                    (setq next (aref (test-sl6--node-forward current) lvl))))
                (aset update lvl current)
                (setq lvl (1- lvl))))
            (let ((next (aref (test-sl6--node-forward current) 0)))
              (if (and next (= (test-sl6--node-key next) key))
                  (setcar (cdr next) value)
                (let* ((new-level (test-sl6--random-level key max-lvl))
                       (new-node (test-sl6--make-node key value new-level)))
                  (let ((i 0))
                    (while (< i new-level)
                      (let ((prev (aref update i)))
                        (when prev
                          (aset (test-sl6--node-forward new-node) i
                                (aref (test-sl6--node-forward prev) i))
                          (aset (test-sl6--node-forward prev) i new-node)))
                      (setq i (1+ i))))
                  (setcar (cddr sl) (1+ (test-sl6--size sl))))))))

        ;; Min: first node at level 0
        (defun test-sl6--min (sl)
          (let ((first (aref (test-sl6--node-forward (test-sl6--header sl)) 0)))
            (if first
                (cons (test-sl6--node-key first) (test-sl6--node-value first))
              nil)))

        ;; Max: follow level 0 to the end
        (defun test-sl6--max (sl)
          (let ((node (aref (test-sl6--node-forward (test-sl6--header sl)) 0))
                (last nil))
            (while node
              (setq last node)
              (setq node (aref (test-sl6--node-forward node) 0)))
            (if last
                (cons (test-sl6--node-key last) (test-sl6--node-value last))
              nil)))

        ;; Floor: largest key <= given key
        (defun test-sl6--floor (sl key)
          (let* ((current (test-sl6--header sl))
                 (max-lvl (test-sl6--max-level sl))
                 (lvl (1- max-lvl)))
            (while (>= lvl 0)
              (let ((next (aref (test-sl6--node-forward current) lvl)))
                (while (and next (<= (test-sl6--node-key next) key))
                  (setq current next)
                  (setq next (aref (test-sl6--node-forward current) lvl))))
              (setq lvl (1- lvl)))
            (if (test-sl6--node-key current)
                (cons (test-sl6--node-key current) (test-sl6--node-value current))
              nil)))

        ;; Ceiling: smallest key >= given key
        (defun test-sl6--ceiling (sl key)
          (let* ((current (test-sl6--header sl))
                 (max-lvl (test-sl6--max-level sl))
                 (lvl (1- max-lvl)))
            (while (>= lvl 0)
              (let ((next (aref (test-sl6--node-forward current) lvl)))
                (while (and next (< (test-sl6--node-key next) key))
                  (setq current next)
                  (setq next (aref (test-sl6--node-forward current) lvl))))
              (setq lvl (1- lvl)))
            (let ((next (aref (test-sl6--node-forward current) 0)))
              (if (and next (>= (test-sl6--node-key next) key))
                  (cons (test-sl6--node-key next) (test-sl6--node-value next))
                nil))))

        ;; Test
        (let ((sl (test-sl6--create 4)))
          (dolist (k '(10 20 30 40 50))
            (test-sl6--insert sl k (format "v%d" k)))
          (list
            (test-sl6--min sl)
            (test-sl6--max sl)
            ;; Floor
            (test-sl6--floor sl 25)   ;; -> 20
            (test-sl6--floor sl 30)   ;; -> 30 (exact)
            (test-sl6--floor sl 5)    ;; -> nil (nothing <= 5)
            (test-sl6--floor sl 50)   ;; -> 50
            (test-sl6--floor sl 55)   ;; -> 50
            ;; Ceiling
            (test-sl6--ceiling sl 25) ;; -> 30
            (test-sl6--ceiling sl 30) ;; -> 30 (exact)
            (test-sl6--ceiling sl 5)  ;; -> 10
            (test-sl6--ceiling sl 50) ;; -> 50
            (test-sl6--ceiling sl 55) ;; -> nil (nothing >= 55)
            )))
      ;; Cleanup
      (fmakunbound 'test-sl6--make-node)
      (fmakunbound 'test-sl6--node-key)
      (fmakunbound 'test-sl6--node-value)
      (fmakunbound 'test-sl6--node-forward)
      (fmakunbound 'test-sl6--create)
      (fmakunbound 'test-sl6--header)
      (fmakunbound 'test-sl6--max-level)
      (fmakunbound 'test-sl6--size)
      (fmakunbound 'test-sl6--random-level)
      (fmakunbound 'test-sl6--insert)
      (fmakunbound 'test-sl6--min)
      (fmakunbound 'test-sl6--max)
      (fmakunbound 'test-sl6--floor)
      (fmakunbound 'test-sl6--ceiling))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 . \"v10\") (50 . \"v50\") (20 . \"v20\") (30 . \"v30\") nil (50 . \"v50\") (50 . \"v50\") (30 . \"v30\") (30 . \"v30\") (10 . \"v10\") (50 . \"v50\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
