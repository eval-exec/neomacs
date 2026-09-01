//! Oracle parity tests for a JSON-like data processor in Elisp.
//!
//! Builds a processor that: parses JSON-ish strings (using `read`-compatible
//! alist/plist representation), queries nested structures with path expressions
//! (like "a.b.c"), transforms/maps over arrays, filters, flattens nested
//! structures, and diffs two structures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Path-based querying of nested alist structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_path_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a path-query function: given a nested alist and a dotted path
    // string like "a.b.c", navigate through nested alists to extract the value.
    let form = r#"(progn
  ;; Split a string by a separator character
  (fset 'neovm--jp-split
    (lambda (str sep)
      (let ((result nil) (start 0) (len (length str)))
        (let ((i 0))
          (while (<= i len)
            (if (or (= i len) (= (aref str i) sep))
                (progn
                  (setq result (cons (substring str start i) result))
                  (setq start (1+ i))))
            (setq i (1+ i))))
        (nreverse result))))

  ;; Navigate into a nested alist by key path
  (fset 'neovm--jp-get-path
    (lambda (data path-str)
      (let ((keys (funcall 'neovm--jp-split path-str ?.))
            (current data))
        (while (and keys current)
          (let ((key (intern (car keys))))
            (if (listp current)
                (let ((found (assq key current)))
                  (setq current (if found (cdr found) nil)))
              (setq current nil)))
          (setq keys (cdr keys)))
        current)))

  (unwind-protect
      (let ((data '((user . ((name . "Alice")
                              (age . 30)
                              (address . ((city . "NYC")
                                          (zip . "10001")
                                          (coords . ((lat . 40)
                                                     (lon . -74)))))
                              (tags . (admin active))))
                    (meta . ((version . 2)
                             (created . "2024-01-01"))))))
        (list
          ;; Simple key
          (funcall 'neovm--jp-get-path data "meta")
          ;; Two-level path
          (funcall 'neovm--jp-get-path data "user.name")
          (funcall 'neovm--jp-get-path data "user.age")
          ;; Three-level path
          (funcall 'neovm--jp-get-path data "user.address.city")
          (funcall 'neovm--jp-get-path data "user.address.zip")
          ;; Four-level path
          (funcall 'neovm--jp-get-path data "user.address.coords.lat")
          ;; Path to a list value
          (funcall 'neovm--jp-get-path data "user.tags")
          ;; Non-existent path
          (funcall 'neovm--jp-get-path data "user.email")
          (funcall 'neovm--jp-get-path data "user.address.state")
          ;; Meta paths
          (funcall 'neovm--jp-get-path data "meta.version")))
    (fmakunbound 'neovm--jp-split)
    (fmakunbound 'neovm--jp-get-path)))"#;
    let expect = expect_test::expect![[
        r#""OK (((version . 2) (created . \"2024-01-01\")) \"Alice\" 30 \"NYC\" \"10001\" 40 (admin active) nil nil 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Setting values at paths in nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_path_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set a value at a given path in a nested alist, creating
    // intermediate nodes as needed.
    let form = r#"(progn
  (fset 'neovm--jp-split2
    (lambda (str sep)
      (let ((result nil) (start 0) (len (length str)))
        (let ((i 0))
          (while (<= i len)
            (if (or (= i len) (= (aref str i) sep))
                (progn
                  (setq result (cons (substring str start i) result))
                  (setq start (1+ i))))
            (setq i (1+ i))))
        (nreverse result))))

  ;; Deep-set: set value at path, creating intermediate alists
  (fset 'neovm--jp-set-path
    (lambda (data path-str value)
      (let ((keys (funcall 'neovm--jp-split2 path-str ?.)))
        (if (null (cdr keys))
            ;; Single key: update or add at top level
            (let* ((key (intern (car keys)))
                   (existing (assq key data)))
              (if existing
                  (progn (setcdr existing value) data)
                (cons (cons key value) data)))
          ;; Multiple keys: recurse
          (let* ((key (intern (car keys)))
                 (rest-path (mapconcat #'identity (cdr keys) "."))
                 (existing (assq key data))
                 (sub-data (if existing (cdr existing) nil))
                 (new-sub (funcall 'neovm--jp-set-path sub-data rest-path value)))
            (if existing
                (progn (setcdr existing new-sub) data)
              (cons (cons key new-sub) data)))))))

  ;; Get helper for verification
  (fset 'neovm--jp-get-path2
    (lambda (data path-str)
      (let ((keys (funcall 'neovm--jp-split2 path-str ?.))
            (current data))
        (while (and keys current)
          (let ((key (intern (car keys))))
            (if (listp current)
                (let ((found (assq key current)))
                  (setq current (if found (cdr found) nil)))
              (setq current nil)))
          (setq keys (cdr keys)))
        current)))

  (unwind-protect
      (let ((data (copy-tree '((a . 1) (b . ((c . 2) (d . 3)))))))
        (list
          ;; Set existing top-level key
          (progn
            (funcall 'neovm--jp-set-path data "a" 99)
            (funcall 'neovm--jp-get-path2 data "a"))
          ;; Set existing nested key
          (progn
            (funcall 'neovm--jp-set-path data "b.c" 42)
            (funcall 'neovm--jp-get-path2 data "b.c"))
          ;; d should be unchanged
          (funcall 'neovm--jp-get-path2 data "b.d")
          ;; Set new top-level key
          (progn
            (funcall 'neovm--jp-set-path data "x" 'new-val)
            (funcall 'neovm--jp-get-path2 data "x"))
          ;; Set deeply nested new path (creates intermediate alists)
          (progn
            (funcall 'neovm--jp-set-path data "p.q.r" 'deep)
            (funcall 'neovm--jp-get-path2 data "p.q.r"))))
    (fmakunbound 'neovm--jp-split2)
    (fmakunbound 'neovm--jp-set-path)
    (fmakunbound 'neovm--jp-get-path2)))"#;
    let expect = expect_test::expect![[r#""OK (99 42 3 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map/transform over arrays in nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_array_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a nested data structure containing lists that act as "arrays",
    // apply transformations: map, filter, reduce over those arrays.
    let form = r#"(let ((data '((students . ((name . "Alice") (score . 90)))
                    (students . ((name . "Bob") (score . 75)))
                    (students . ((name . "Carol") (score . 88)))
                    (students . ((name . "Dave") (score . 62)))
                    (students . ((name . "Eve") (score . 95))))))
  ;; Extract all student records (entries with key 'students)
  (let* ((students (mapcar #'cdr
                           (seq-filter (lambda (pair) (eq (car pair) 'students))
                                       data)))
         ;; Map: extract just names
         (names (mapcar (lambda (s) (cdr (assq 'name s))) students))
         ;; Map: extract scores
         (scores (mapcar (lambda (s) (cdr (assq 'score s))) students))
         ;; Filter: students with score >= 80
         (passing (seq-filter (lambda (s) (>= (cdr (assq 'score s)) 80))
                              students))
         ;; Reduce: compute average score
         (total (apply #'+ scores))
         (avg (/ total (length scores)))
         ;; Transform: add a "grade" field based on score
         (graded (mapcar
                  (lambda (s)
                    (let ((score (cdr (assq 'score s))))
                      (append s (list (cons 'grade
                                            (cond ((>= score 90) "A")
                                                  ((>= score 80) "B")
                                                  ((>= score 70) "C")
                                                  (t "F")))))))
                  students)))
    (list
      names
      scores
      (mapcar (lambda (s) (cdr (assq 'name s))) passing)
      avg
      (mapcar (lambda (s) (cons (cdr (assq 'name s))
                                (cdr (assq 'grade s))))
              graded))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Carol\" \"Dave\" \"Eve\") (90 75 88 62 95) (\"Alice\" \"Carol\" \"Eve\") 82 ((\"Alice\" . \"A\") (\"Bob\" . \"C\") (\"Carol\" . \"B\") (\"Dave\" . \"F\") (\"Eve\" . \"A\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Flatten nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_flatten() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flatten a nested alist into a flat alist with dotted-path keys.
    // E.g., ((a . ((b . 1) (c . 2)))) -> (("a.b" . 1) ("a.c" . 2))
    let form = r#"(progn
  (fset 'neovm--jp-flatten
    (lambda (data prefix)
      (let ((result nil))
        (dolist (pair data)
          (let ((key (if (string= prefix "")
                         (symbol-name (car pair))
                       (concat prefix "." (symbol-name (car pair)))))
                (val (cdr pair)))
            (if (and (listp val) val (consp (car val)))
                ;; Nested alist: recurse
                (setq result (append result
                                     (funcall 'neovm--jp-flatten val key)))
              ;; Leaf value
              (setq result (append result (list (cons key val)))))))
        result)))

  (unwind-protect
      (let ((data '((user . ((name . "Alice")
                              (age . 30)
                              (address . ((city . "NYC")
                                          (zip . "10001")))))
                    (meta . ((version . 2)))
                    (active . t))))
        (let ((flat (funcall 'neovm--jp-flatten data "")))
          (list
            ;; All flattened keys sorted
            (sort (mapcar #'car flat) #'string-lessp)
            ;; Specific values
            (cdr (assoc "user.name" flat))
            (cdr (assoc "user.address.city" flat))
            (cdr (assoc "meta.version" flat))
            (cdr (assoc "active" flat))
            ;; Total number of leaf entries
            (length flat))))
    (fmakunbound 'neovm--jp-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"active\" \"meta.version\" \"user.address.city\" \"user.address.zip\" \"user.age\" \"user.name\") \"Alice\" \"NYC\" 2 t 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Diff two structures: find added, removed, changed keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare two flat alists (or flatten first), report:
    // - added: keys in new but not old
    // - removed: keys in old but not new
    // - changed: keys in both but with different values
    // - unchanged: keys in both with same values
    let form = r#"(progn
  (fset 'neovm--jp-diff
    (lambda (old new)
      (let ((added nil) (removed nil) (changed nil) (unchanged nil))
        ;; Check old keys against new
        (dolist (pair old)
          (let ((new-pair (assoc (car pair) new)))
            (if new-pair
                (if (equal (cdr pair) (cdr new-pair))
                    (setq unchanged (cons (car pair) unchanged))
                  (setq changed (cons (list (car pair) (cdr pair) (cdr new-pair))
                                      changed)))
              (setq removed (cons (car pair) removed)))))
        ;; Check new keys not in old
        (dolist (pair new)
          (unless (assoc (car pair) old)
            (setq added (cons (car pair) added))))
        (list
          (cons 'added (sort (nreverse added) #'string-lessp))
          (cons 'removed (sort (nreverse removed) #'string-lessp))
          (cons 'changed (sort (nreverse changed)
                               (lambda (a b) (string-lessp (car a) (car b)))))
          (cons 'unchanged (sort (nreverse unchanged) #'string-lessp))))))

  (unwind-protect
      (let ((old '(("name" . "Alice")
                   ("age" . 30)
                   ("city" . "NYC")
                   ("role" . "admin")
                   ("active" . t)))
            (new '(("name" . "Alice")
                   ("age" . 31)
                   ("city" . "SFO")
                   ("email" . "alice@example.com")
                   ("active" . t))))
        (let ((diff (funcall 'neovm--jp-diff old new)))
          (list
            ;; Added keys
            (cdr (assq 'added diff))
            ;; Removed keys
            (cdr (assq 'removed diff))
            ;; Changed entries (key old-val new-val)
            (cdr (assq 'changed diff))
            ;; Unchanged keys
            (cdr (assq 'unchanged diff)))))
    (fmakunbound 'neovm--jp-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"email\") (\"role\") ((\"age\" 30 31) (\"city\" \"NYC\" \"SFO\")) (\"active\" \"name\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: full pipeline - build, query, transform, diff
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_full_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // End-to-end pipeline:
    // 1. Build a data structure representing a config file
    // 2. Query specific settings
    // 3. Apply a "migration" transformation (rename keys, update values)
    // 4. Diff old vs new config
    let form = r#"(progn
  (fset 'neovm--jp-fp-split
    (lambda (str sep)
      (let ((result nil) (start 0) (len (length str)))
        (let ((i 0))
          (while (<= i len)
            (if (or (= i len) (= (aref str i) sep))
                (progn
                  (setq result (cons (substring str start i) result))
                  (setq start (1+ i))))
            (setq i (1+ i))))
        (nreverse result))))

  (fset 'neovm--jp-fp-get
    (lambda (data path-str)
      (let ((keys (funcall 'neovm--jp-fp-split path-str ?.))
            (current data))
        (while (and keys current)
          (let ((key (intern (car keys))))
            (if (listp current)
                (let ((found (assq key current)))
                  (setq current (if found (cdr found) nil)))
              (setq current nil)))
          (setq keys (cdr keys)))
        current)))

  ;; Flatten for diffing
  (fset 'neovm--jp-fp-flatten
    (lambda (data prefix)
      (let ((result nil))
        (dolist (pair data)
          (let ((key (if (string= prefix "")
                         (symbol-name (car pair))
                       (concat prefix "." (symbol-name (car pair)))))
                (val (cdr pair)))
            (if (and (listp val) val (consp (car val)))
                (setq result (append result
                                     (funcall 'neovm--jp-fp-flatten val key)))
              (setq result (append result (list (cons key val)))))))
        result)))

  (unwind-protect
      (let* (;; Step 1: Build config
             (config '((database . ((host . "localhost")
                                    (port . 5432)
                                    (name . "mydb")
                                    (pool-size . 10)))
                       (cache . ((enabled . t)
                                 (ttl . 300)
                                 (max-size . 1000)))
                       (logging . ((level . "info")
                                   (file . "/var/log/app.log")))))

             ;; Step 2: Query
             (db-host (funcall 'neovm--jp-fp-get config "database.host"))
             (cache-ttl (funcall 'neovm--jp-fp-get config "cache.ttl"))
             (log-level (funcall 'neovm--jp-fp-get config "logging.level"))

             ;; Step 3: Migration - apply transformations
             (new-config (copy-tree config))
             ;; Update: change db port, increase pool, add ssl, change log level
             (_ (progn
                  (setcdr (assq 'port (cdr (assq 'database new-config))) nil)
                  ;; Rebuild database section
                  (setcdr (assq 'database new-config)
                          '((host . "db.prod.internal")
                            (port . 5433)
                            (name . "mydb")
                            (pool-size . 20)
                            (ssl . t)))
                  (setcdr (assq 'logging new-config)
                          '((level . "warn")
                            (file . "/var/log/app.log")
                            (format . "json")))))

             ;; Step 4: Flatten both and diff
             (old-flat (funcall 'neovm--jp-fp-flatten config ""))
             (new-flat (funcall 'neovm--jp-fp-flatten new-config ""))

             ;; Compute changes
             (changed-keys nil)
             (added-keys nil))
        ;; Find changed and added
        (dolist (pair new-flat)
          (let ((old-pair (assoc (car pair) old-flat)))
            (if old-pair
                (unless (equal (cdr old-pair) (cdr pair))
                  (setq changed-keys
                        (cons (list (car pair) (cdr old-pair) (cdr pair))
                              changed-keys)))
              (setq added-keys (cons (car pair) added-keys)))))

        (list
          ;; Queries
          (list db-host cache-ttl log-level)
          ;; Changed keys (sorted)
          (sort (nreverse changed-keys)
                (lambda (a b) (string-lessp (car a) (car b))))
          ;; Added keys (sorted)
          (sort (nreverse added-keys) #'string-lessp)
          ;; Verify new values via query
          (funcall 'neovm--jp-fp-get new-config "database.ssl")
          (funcall 'neovm--jp-fp-get new-config "database.pool-size")
          (funcall 'neovm--jp-fp-get new-config "logging.format")))
    (fmakunbound 'neovm--jp-fp-split)
    (fmakunbound 'neovm--jp-fp-get)
    (fmakunbound 'neovm--jp-fp-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"localhost\" 300 \"info\") ((\"database.host\" \"localhost\" \"db.prod.internal\") (\"database.pool-size\" 10 20) (\"database.port\" 5432 5433) (\"logging.level\" \"info\" \"warn\")) (\"database.ssl\" \"logging.format\") t 20 \"json\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: merge two data structures with conflict resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_proc_merge_with_strategy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deep merge two nested alists with configurable conflict resolution:
    // - :left = keep left value on conflict
    // - :right = keep right value on conflict
    // - :concat = if both are lists, concatenate; otherwise keep right
    let form = r#"(progn
  (fset 'neovm--jp-merge
    (lambda (left right strategy)
      (let ((result (copy-tree left)))
        ;; Add/merge entries from right
        (dolist (r-pair right)
          (let* ((key (car r-pair))
                 (r-val (cdr r-pair))
                 (l-entry (assq key result)))
            (if l-entry
                ;; Key exists in both: resolve conflict
                (let ((l-val (cdr l-entry)))
                  (cond
                    ;; Both are nested alists: recurse
                    ((and (listp l-val) (consp (car-safe l-val))
                          (listp r-val) (consp (car-safe r-val)))
                     (setcdr l-entry
                             (funcall 'neovm--jp-merge l-val r-val strategy)))
                    ;; Conflict resolution
                    ((eq strategy :left) nil)  ; keep left, do nothing
                    ((eq strategy :right)
                     (setcdr l-entry r-val))
                    ((eq strategy :concat)
                     (if (and (listp l-val) (listp r-val))
                         (setcdr l-entry (append l-val r-val))
                       (setcdr l-entry r-val)))
                    (t (setcdr l-entry r-val))))
              ;; Key only in right: add
              (setq result (append result (list (cons key r-val)))))))
        result)))

  (unwind-protect
      (let ((base '((name . "App")
                    (version . 1)
                    (features . (search filter))
                    (db . ((host . "localhost")
                           (port . 5432)))))
            (override '((version . 2)
                        (features . (export import))
                        (db . ((host . "db.prod")
                               (ssl . t)))
                        (new-key . "added"))))
        (list
          ;; Merge with :right strategy (override wins)
          (let ((merged (funcall 'neovm--jp-merge base override :right)))
            (list
              (cdr (assq 'name merged))
              (cdr (assq 'version merged))
              (cdr (assq 'features merged))
              (cdr (assq 'host (cdr (assq 'db merged))))
              (cdr (assq 'ssl (cdr (assq 'db merged))))
              (cdr (assq 'new-key merged))))
          ;; Merge with :left strategy (base wins)
          (let ((merged (funcall 'neovm--jp-merge base override :left)))
            (list
              (cdr (assq 'version merged))
              (cdr (assq 'features merged))
              (cdr (assq 'host (cdr (assq 'db merged))))))
          ;; Merge with :concat strategy (lists are concatenated)
          (let ((merged (funcall 'neovm--jp-merge base override :concat)))
            (cdr (assq 'features merged)))))
    (fmakunbound 'neovm--jp-merge)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"App\" 2 (export import) \"db.prod\" t \"added\") (1 (search filter) \"localhost\") (search filter export import))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
