//! Oracle parity tests for advanced association list patterns:
//! nested alists as hierarchical data, alist-get with TESTFN/DEFAULT/REMOVE,
//! query building, configuration with inheritance, merge strategies,
//! and alist-based routing tables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Nested alists as hierarchical data (JSON-like)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_nested_hierarchical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((deep-get
             ;; Access nested alist by key path
             (lambda (data keys)
               (let ((current data))
                 (dolist (k keys)
                   (setq current (cdr (assq k current))))
                 current)))
            (deep-set
             ;; Set a value at a nested key path (non-destructive, returns new alist)
             (lambda (data keys value)
               (if (null (cdr keys))
                   ;; Leaf: update or add this key
                   (let ((existing (assq (car keys) data)))
                     (if existing
                         (mapcar (lambda (pair)
                                   (if (eq (car pair) (car keys))
                                       (cons (car keys) value)
                                     pair))
                                 data)
                       (append data (list (cons (car keys) value)))))
                 ;; Recurse into nested alist
                 (let* ((k (car keys))
                        (child (cdr (assq k data)))
                        (updated-child (funcall deep-set child (cdr keys) value)))
                   (mapcar (lambda (pair)
                             (if (eq (car pair) k)
                                 (cons k updated-child)
                               pair))
                           data))))))
        ;; Build a nested structure like JSON
        (let ((config '((server . ((host . "localhost")
                                   (port . 8080)
                                   (ssl . ((enabled . nil)
                                           (cert . "/path/to/cert")))))
                        (database . ((host . "db.local")
                                     (port . 5432)
                                     (name . "mydb")
                                     (pool . ((min . 2) (max . 10)))))
                        (logging . ((level . info)
                                    (file . "/var/log/app.log"))))))
          (list
           ;; Deep access
           (funcall deep-get config '(server host))
           (funcall deep-get config '(server port))
           (funcall deep-get config '(server ssl enabled))
           (funcall deep-get config '(database pool max))
           (funcall deep-get config '(logging level))
           ;; Deep set: change server port
           (let ((updated (funcall deep-set config '(server port) 9090)))
             (funcall deep-get updated '(server port)))
           ;; Deep set: enable SSL
           (let ((updated (funcall deep-set config '(server ssl enabled) t)))
             (funcall deep-get updated '(server ssl enabled)))
           ;; Original unchanged
           (funcall deep-get config '(server port)))))
    (fmakunbound 'neovm--test-alist-nested-dummy)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable deep-set)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get with TESTFN and DEFAULT parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_get_testfn_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((string-alist '(("Content-Type" . "text/html")
                                  ("Accept" . "application/json")
                                  ("X-Custom-Header" . "value123")
                                  ("content-type" . "should-not-match"))))
  (list
   ;; Default test (eq) won't find string keys
   (alist-get "Content-Type" string-alist)
   ;; With equal test, finds exact match
   (alist-get "Content-Type" string-alist nil nil 'equal)
   ;; With string-equal (case-sensitive)
   (alist-get "Accept" string-alist nil nil 'equal)
   ;; Missing key with default
   (alist-get "Authorization" string-alist "Bearer none" nil 'equal)
   ;; Multiple defaults scenarios
   (alist-get "missing" string-alist nil nil 'equal)
   (alist-get "missing" string-alist 'not-found nil 'equal)
   ;; Symbol alist with default
   (let ((sym-al '((a . 1) (b . 2) (c . 3))))
     (list (alist-get 'd sym-al 99)
           (alist-get 'a sym-al 99)
           (alist-get 'b sym-al)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil \"text/html\" \"application/json\" \"Bearer none\" nil not-found (99 1 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get with REMOVE flag
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_get_remove_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // REMOVE=non-nil: treat nil-valued entries as absent
    let form = r#"(let ((alist '((enabled . t)
                           (disabled . nil)
                           (count . 0)
                           (empty-str . "")
                           (null-val . nil)
                           (active . t))))
  (list
   ;; Without REMOVE: nil-valued keys return nil (ambiguous with absent)
   (alist-get 'disabled alist)         ;; nil (present, value is nil)
   (alist-get 'nonexistent alist)      ;; nil (absent)
   ;; With REMOVE=t: nil-valued keys behave as absent
   (alist-get 'disabled alist nil t)   ;; nil (treated as removed)
   (alist-get 'null-val alist nil t)   ;; nil (treated as removed)
   ;; Non-nil values unaffected by REMOVE flag
   (alist-get 'enabled alist nil t)    ;; t
   (alist-get 'count alist nil t)      ;; 0 (not nil, so not removed)
   (alist-get 'empty-str alist nil t)  ;; "" (not nil)
   ;; REMOVE with DEFAULT: absent returns default
   (alist-get 'disabled alist 'was-nil t)    ;; was-nil
   (alist-get 'nonexistent alist 'gone t)    ;; gone
   (alist-get 'active alist 'default t)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil t 0 \"\" nil gone t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building query results from nested alists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_query_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((employees
             '(((id . 1) (name . "Alice") (dept . "eng") (salary . 90000) (level . 3))
               ((id . 2) (name . "Bob") (dept . "qa") (salary . 75000) (level . 2))
               ((id . 3) (name . "Carol") (dept . "eng") (salary . 110000) (level . 4))
               ((id . 4) (name . "Dave") (dept . "qa") (salary . 80000) (level . 2))
               ((id . 5) (name . "Eve") (dept . "eng") (salary . 95000) (level . 3))
               ((id . 6) (name . "Frank") (dept . "mgmt") (salary . 120000) (level . 5))))
            (field (lambda (rec key) (cdr (assq key rec))))
            (where
             ;; Filter records by predicate
             (lambda (records pred)
               (let ((result nil))
                 (dolist (r records)
                   (when (funcall pred r)
                     (setq result (cons r result))))
                 (nreverse result))))
            (select
             ;; Project specific fields from records
             (lambda (records fields)
               (mapcar (lambda (r)
                         (mapcar (lambda (f) (cons f (cdr (assq f r))))
                                 fields))
                       records)))
            (aggregate
             ;; Compute aggregate on a numeric field
             (lambda (records field-key agg-fn)
               (let ((values (mapcar (lambda (r) (cdr (assq field-key r)))
                                     records)))
                 (funcall agg-fn values)))))
        ;; Query: engineers with level >= 3
        (let* ((engineers (funcall where employees
                                   (lambda (r) (equal (cdr (assq 'dept r)) "eng"))))
               (senior (funcall where engineers
                                (lambda (r) (>= (cdr (assq 'level r)) 3))))
               (names (funcall select senior '(name salary))))
          (list
           ;; Senior engineer names and salaries
           names
           ;; Count of engineers
           (length engineers)
           ;; Average salary of all employees
           (let ((total (funcall aggregate employees 'salary
                                 (lambda (vs) (apply #'+ vs)))))
             (/ total (length employees)))
           ;; Max salary
           (funcall aggregate employees 'salary
                    (lambda (vs) (apply #'max vs)))
           ;; Department counts
           (let ((depts nil))
             (dolist (e employees)
               (let* ((d (cdr (assq 'dept e)))
                      (existing (assoc d depts)))
                 (if existing
                     (setcdr existing (1+ (cdr existing)))
                   (setq depts (cons (cons d 1) depts)))))
             (sort depts (lambda (a b) (string< (car a) (car b))))))))
    (fmakunbound 'neovm--test-alist-query-dummy)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((name . \"Alice\") (salary . 90000)) ((name . \"Carol\") (salary . 110000)) ((name . \"Eve\") (salary . 95000))) 3 95000 120000 ((\"eng\" . 3) (\"mgmt\" . 1) (\"qa\" . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist-based configuration with inheritance (child overrides parent)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_config_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((config-lookup
             ;; Look up key in config chain (child -> parent -> grandparent ...)
             (lambda (chain key)
               (let ((result nil) (found nil))
                 (while (and chain (not found))
                   (let ((pair (assq key (car chain))))
                     (when pair
                       (setq result (cdr pair)
                             found t)))
                   (setq chain (cdr chain)))
                 result)))
            (config-all-keys
             ;; Collect all unique keys from entire chain
             (lambda (chain)
               (let ((keys nil))
                 (dolist (layer chain)
                   (dolist (pair layer)
                     (unless (memq (car pair) keys)
                       (setq keys (cons (car pair) keys)))))
                 (nreverse keys))))
            (config-flatten
             ;; Flatten chain into single alist (child-first override)
             (lambda (chain)
               (let ((result nil))
                 ;; Process from parent to child so child overrides
                 (dolist (layer (reverse chain))
                   (dolist (pair layer)
                     (let ((existing (assq (car pair) result)))
                       (if existing
                           (setcdr existing (cdr pair))
                         (setq result (cons (cons (car pair) (cdr pair))
                                            result))))))
                 result))))
        ;; Three-level config: base -> environment -> app-specific
        (let* ((base '((debug . nil) (log-level . warn) (timeout . 30)
                        (retries . 3) (cache . t) (workers . 4)))
               (env '((log-level . debug) (timeout . 60) (workers . 8)))
               (app '((debug . t) (timeout . 10) (app-name . "myapp")))
               (chain (list app env base)))
          (list
           ;; Lookups: child overrides parent
           (funcall config-lookup chain 'debug)       ;; t (from app)
           (funcall config-lookup chain 'log-level)   ;; debug (from env)
           (funcall config-lookup chain 'retries)     ;; 3 (from base)
           (funcall config-lookup chain 'app-name)    ;; "myapp" (only in app)
           (funcall config-lookup chain 'timeout)     ;; 10 (app overrides env)
           (funcall config-lookup chain 'missing)     ;; nil
           ;; All keys across chain
           (funcall config-all-keys chain)
           ;; Flattened config
           (let ((flat (funcall config-flatten chain)))
             (list (cdr (assq 'debug flat))
                   (cdr (assq 'log-level flat))
                   (cdr (assq 'timeout flat))
                   (cdr (assq 'workers flat))
                   (cdr (assq 'app-name flat)))))))
    (fmakunbound 'neovm--test-alist-inherit-dummy)))"#;
    let expect = expect_test::expect![[
        r#""OK (t debug 3 \"myapp\" 10 nil (debug timeout app-name log-level workers retries cache) (t debug 10 8 \"myapp\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist merge strategies (first-wins, last-wins, combine values)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_merge_strategies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((merge-first-wins
         ;; First occurrence of each key wins
         (lambda (a b)
           (let ((result (copy-alist a)))
             (dolist (pair b)
               (unless (assq (car pair) result)
                 (setq result (append result (list (cons (car pair) (cdr pair)))))))
             result)))
        (merge-last-wins
         ;; Last occurrence of each key wins
         (lambda (a b)
           (let ((result (copy-alist a)))
             (dolist (pair b)
               (let ((existing (assq (car pair) result)))
                 (if existing
                     (setcdr existing (cdr pair))
                   (setq result (append result (list (cons (car pair) (cdr pair))))))))
             result)))
        (merge-combine
         ;; Combine values into lists for duplicate keys
         (lambda (a b)
           (let ((result nil))
             ;; Start with all keys from a, wrapping values in lists
             (dolist (pair a)
               (let ((existing (assq (car pair) result)))
                 (if existing
                     (setcdr existing (append (cdr existing) (list (cdr pair))))
                   (setq result (cons (cons (car pair) (list (cdr pair))) result)))))
             ;; Add keys from b
             (dolist (pair b)
               (let ((existing (assq (car pair) result)))
                 (if existing
                     (setcdr existing (append (cdr existing) (list (cdr pair))))
                   (setq result (cons (cons (car pair) (list (cdr pair))) result)))))
             (nreverse result)))))
  (let ((al1 '((x . 1) (y . 2) (z . 3)))
        (al2 '((y . 20) (z . 30) (w . 40))))
    (list
     ;; First-wins: al1 values take precedence
     (funcall merge-first-wins al1 al2)
     ;; Last-wins: al2 values take precedence
     (funcall merge-last-wins al1 al2)
     ;; Combine: duplicate keys get list of all values
     (funcall merge-combine al1 al2)
     ;; Edge case: one empty
     (funcall merge-first-wins nil al2)
     (funcall merge-last-wins al1 nil)
     ;; Combine with repeated keys in same alist
     (funcall merge-combine
              '((tag . a) (tag . b) (tag . c))
              '((tag . d) (tag . e))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((x . 1) (y . 2) (z . 3) (w . 40)) ((x . 1) (y . 20) (z . 30) (w . 40)) ((x 1) (y 2 20) (z 3 30) (w 40)) ((y . 20) (z . 30) (w . 40)) ((x . 1) (y . 2) (z . 3)) ((tag a b c d e)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alist-based routing table (match paths to handlers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_routing_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((routes
             ;; Route table: list of (pattern . handler-name)
             ;; Patterns: exact string match or prefix with trailing *
             '(("/api/users" . "list-users")
               ("/api/users/*" . "user-detail")
               ("/api/posts" . "list-posts")
               ("/api/posts/*" . "post-detail")
               ("/health" . "health-check")
               ("/*" . "catch-all")))
            (match-route
             (lambda (routes path)
               (let ((result nil)
                     (remaining routes))
                 (while (and remaining (not result))
                   (let* ((route (car remaining))
                          (pattern (car route))
                          (handler (cdr route))
                          (pat-len (length pattern)))
                     ;; Check if pattern ends with *
                     (if (and (> pat-len 0)
                              (= (aref pattern (1- pat-len)) ?*))
                         ;; Prefix match (without the *)
                         (let ((prefix (substring pattern 0 (1- pat-len))))
                           (when (and (>= (length path) (length prefix))
                                      (equal (substring path 0 (length prefix))
                                             prefix))
                             (setq result
                                   (list handler
                                         (substring path (length prefix))))))
                       ;; Exact match
                       (when (equal path pattern)
                         (setq result (list handler nil)))))
                   (setq remaining (cdr remaining)))
                 (or result '("not-found" nil)))))
            (build-response
             (lambda (handler params path)
               (list :handler handler
                     :path path
                     :params params
                     :status 200))))
        ;; Route various paths
        (let ((paths '("/api/users"
                       "/api/users/42"
                       "/api/posts"
                       "/api/posts/hello-world"
                       "/health"
                       "/unknown/path"
                       "/api/users/42/edit")))
          (let ((results
                 (mapcar (lambda (path)
                           (let* ((match (funcall match-route routes path))
                                  (handler (car match))
                                  (param (cadr match)))
                             (funcall build-response handler param path)))
                         paths)))
            (list
             ;; All routing results
             results
             ;; Extract just handler names
             (mapcar (lambda (r) (plist-get r :handler)) results)
             ;; Extract captured params (non-nil)
             (let ((params nil))
               (dolist (r results)
                 (when (plist-get r :params)
                   (setq params (cons (list (plist-get r :path)
                                            (plist-get r :params))
                                      params))))
               (nreverse params))))))
    (fmakunbound 'neovm--test-alist-route-dummy)))"#;
    let expect = expect_test::expect![[
        r#""OK (((:handler \"list-users\" :path \"/api/users\" :params nil :status 200) (:handler \"user-detail\" :path \"/api/users/42\" :params \"42\" :status 200) (:handler \"list-posts\" :path \"/api/posts\" :params nil :status 200) (:handler \"post-detail\" :path \"/api/posts/hello-world\" :params \"hello-world\" :status 200) (:handler \"health-check\" :path \"/health\" :params nil :status 200) (:handler \"catch-all\" :path \"/unknown/path\" :params \"unknown/path\" :status 200) (:handler \"user-detail\" :path \"/api/users/42/edit\" :params \"42/edit\" :status 200)) (\"list-users\" \"user-detail\" \"list-posts\" \"post-detail\" \"health-check\" \"catch-all\" \"user-detail\") ((\"/api/users/42\" \"42\") (\"/api/posts/hello-world\" \"hello-world\") (\"/unknown/path\" \"unknown/path\") (\"/api/users/42/edit\" \"42/edit\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based state machine with transitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (unwind-protect
      (let ((transitions
             ;; State machine: alist of (state . ((event . next-state) ...))
             '((idle . ((start . running) (error . failed)))
               (running . ((pause . paused) (complete . done) (error . failed)))
               (paused . ((resume . running) (cancel . idle) (error . failed)))
               (done . ((reset . idle)))
               (failed . ((retry . running) (reset . idle)))))
            (next-state
             (lambda (transitions current-state event)
               (let* ((state-transitions (cdr (assq current-state transitions)))
                      (transition (assq event state-transitions)))
                 (if transition
                     (cdr transition)
                   nil))))
            (valid-events
             ;; List valid events for a given state
             (lambda (transitions state)
               (let ((state-transitions (cdr (assq state transitions))))
                 (mapcar #'car state-transitions))))
            (run-sequence
             ;; Run a sequence of events, return list of (state event -> new-state)
             (lambda (transitions start-state events)
               (let ((current start-state)
                     (log nil))
                 (dolist (ev events)
                   (let ((next (funcall next-state transitions current ev)))
                     (setq log (cons (list current ev next) log))
                     (when next (setq current next))))
                 (list (nreverse log) current)))))
        (list
         ;; Basic transitions
         (funcall next-state transitions 'idle 'start)
         (funcall next-state transitions 'running 'pause)
         (funcall next-state transitions 'idle 'invalid)
         ;; Valid events per state
         (funcall valid-events transitions 'idle)
         (funcall valid-events transitions 'running)
         (funcall valid-events transitions 'done)
         ;; Run a successful workflow
         (funcall run-sequence transitions 'idle
                  '(start pause resume complete reset))
         ;; Run with error and recovery
         (funcall run-sequence transitions 'idle
                  '(start error retry complete))
         ;; Run with invalid event in middle
         (funcall run-sequence transitions 'idle
                  '(start bogus-event pause cancel))))
    (fmakunbound 'neovm--test-alist-sm-dummy)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable next-state)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
