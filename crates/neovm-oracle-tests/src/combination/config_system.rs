//! Oracle parity tests for a configuration management system:
//! hierarchical config (defaults/user/env), config validation with
//! type checking and range checking, config merge with precedence,
//! computed/derived config values, config diff, config serialization
//! to alist, and config inheritance.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Hierarchical config with defaults, user overrides, and env overrides
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_hierarchical_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Config is an alist of (key . value) pairs.
  ;; Merge: later layers override earlier layers.
  (fset 'neovm--test-config-merge
    (lambda (base override)
      (let ((result (copy-alist base)))
        (dolist (entry override)
          (let ((existing (assoc (car entry) result)))
            (if existing
                (setcdr existing (cdr entry))
              (setq result (cons entry result)))))
        result)))

  ;; Deep merge for nested alists
  (fset 'neovm--test-config-deep-merge
    (lambda (base override)
      (let ((result (copy-alist base)))
        (dolist (entry override)
          (let ((key (car entry))
                (val (cdr entry))
                (existing (assoc (car entry) result)))
            (if existing
                (if (and (listp val) (listp (cdr existing))
                         (consp (car val)) (consp (car (cdr existing))))
                    ;; Both are alists: recurse
                    (setcdr existing
                            (funcall 'neovm--test-config-deep-merge
                                     (cdr existing) val))
                  ;; Simple override
                  (setcdr existing val))
              (setq result (cons entry result)))))
        result)))

  ;; Get nested value by key path
  (fset 'neovm--test-config-get
    (lambda (config path)
      (let ((current config))
        (dolist (key path)
          (setq current (cdr (assoc key current))))
        current)))

  (unwind-protect
      (let* ((defaults '((host . "localhost")
                         (port . 8080)
                         (debug . nil)
                         (log-level . "info")
                         (max-connections . 100)
                         (timeout . 30)))
             (user-config '((port . 3000)
                            (debug . t)
                            (log-level . "debug")))
             (env-config '((host . "0.0.0.0")
                           (port . 9090)))
             ;; Merge: defaults < user < env
             (merged1 (funcall 'neovm--test-config-merge defaults user-config))
             (merged2 (funcall 'neovm--test-config-merge merged1 env-config)))
        (list
          ;; After user override
          (cdr (assoc 'port merged1))       ;; 3000
          (cdr (assoc 'debug merged1))      ;; t
          (cdr (assoc 'host merged1))       ;; "localhost" (not overridden)
          ;; After env override
          (cdr (assoc 'port merged2))       ;; 9090 (env overrides user)
          (cdr (assoc 'host merged2))       ;; "0.0.0.0"
          (cdr (assoc 'debug merged2))      ;; t (from user, not overridden by env)
          (cdr (assoc 'log-level merged2))  ;; "debug" (from user)
          (cdr (assoc 'timeout merged2))    ;; 30 (from defaults)
          ;; Deep merge test
          (let* ((base '((server . ((host . "localhost") (port . 8080)))
                         (db . ((host . "db.local") (port . 5432) (name . "mydb")))))
                 (override '((server . ((port . 3000)))
                             (db . ((host . "db.prod")))))
                 (deep (funcall 'neovm--test-config-deep-merge base override)))
            (list
              (funcall 'neovm--test-config-get deep '(server host))  ;; "localhost"
              (funcall 'neovm--test-config-get deep '(server port))  ;; 3000
              (funcall 'neovm--test-config-get deep '(db host))      ;; "db.prod"
              (funcall 'neovm--test-config-get deep '(db port))      ;; 5432
              (funcall 'neovm--test-config-get deep '(db name))))))  ;; "mydb"
    (fmakunbound 'neovm--test-config-merge)
    (fmakunbound 'neovm--test-config-deep-merge)
    (fmakunbound 'neovm--test-config-get)))"#;
    let expect = expect_test::expect![[
        r#""OK (3000 t \"localhost\" 9090 \"0.0.0.0\" t \"debug\" 30 (\"localhost\" 3000 \"db.prod\" 5432 \"mydb\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Config validation: type checking, range checking, required keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Schema entry: (key type required min max allowed-values)
  ;; type: 'integer, 'string, 'boolean, 'symbol
  (fset 'neovm--test-validate-field
    (lambda (key value schema-entry)
      (let ((expected-type (nth 0 schema-entry))
            (required (nth 1 schema-entry))
            (min-val (nth 2 schema-entry))
            (max-val (nth 3 schema-entry))
            (allowed (nth 4 schema-entry))
            (errors nil))
        ;; Type check
        (cond
          ((eq expected-type 'integer)
           (unless (integerp value)
             (setq errors (cons (format "%s: expected integer, got %S" key value) errors))))
          ((eq expected-type 'string)
           (unless (stringp value)
             (setq errors (cons (format "%s: expected string, got %S" key value) errors))))
          ((eq expected-type 'boolean)
           (unless (or (eq value t) (eq value nil))
             (setq errors (cons (format "%s: expected boolean, got %S" key value) errors))))
          ((eq expected-type 'symbol)
           (unless (symbolp value)
             (setq errors (cons (format "%s: expected symbol, got %S" key value) errors)))))
        ;; Range check for numbers
        (when (and (integerp value) min-val (< value min-val))
          (setq errors (cons (format "%s: %d below minimum %d" key value min-val) errors)))
        (when (and (integerp value) max-val (> value max-val))
          (setq errors (cons (format "%s: %d above maximum %d" key value max-val) errors)))
        ;; Allowed values check
        (when (and allowed (not (member value allowed)))
          (setq errors (cons (format "%s: %S not in allowed values" key value) errors)))
        (nreverse errors))))

  (fset 'neovm--test-validate-config
    (lambda (config schema)
      (let ((all-errors nil))
        ;; Check each schema entry
        (dolist (entry schema)
          (let ((key (car entry))
                (spec (cdr entry)))
            (let ((value (cdr (assoc key config)))
                  (required (nth 1 spec)))
              (if value
                  (let ((field-errors (funcall 'neovm--test-validate-field
                                               key value spec)))
                    (setq all-errors (append all-errors field-errors)))
                (when required
                  (setq all-errors
                        (cons (format "%s: required but missing" key)
                              all-errors)))))))
        (if all-errors
            (list 'invalid all-errors)
          (list 'valid)))))

  (unwind-protect
      (let ((schema '((port    integer t 1 65535 nil)
                      (host    string  t nil nil nil)
                      (debug   boolean nil nil nil nil)
                      (log-level string nil nil nil ("debug" "info" "warn" "error"))
                      (workers integer nil 1 64 nil))))
        (list
          ;; Valid config
          (funcall 'neovm--test-validate-config
            '((port . 8080) (host . "localhost") (debug . t)
              (log-level . "info") (workers . 4))
            schema)
          ;; Missing required field
          (funcall 'neovm--test-validate-config
            '((port . 8080))
            schema)
          ;; Port out of range
          (funcall 'neovm--test-validate-config
            '((port . 99999) (host . "localhost"))
            schema)
          ;; Wrong type
          (funcall 'neovm--test-validate-config
            '((port . "not-a-number") (host . "localhost"))
            schema)
          ;; Invalid log-level
          (funcall 'neovm--test-validate-config
            '((port . 80) (host . "localhost") (log-level . "verbose"))
            schema)
          ;; Workers below minimum
          (funcall 'neovm--test-validate-config
            '((port . 80) (host . "localhost") (workers . 0))
            schema)
          ;; All defaults, only required fields
          (funcall 'neovm--test-validate-config
            '((port . 443) (host . "example.com"))
            schema)))
    (fmakunbound 'neovm--test-validate-field)
    (fmakunbound 'neovm--test-validate-config)))"#;
    let expect = expect_test::expect![[
        r#""OK ((valid) (invalid (\"host: required but missing\")) (invalid (\"port: 99999 above maximum 65535\")) (invalid (\"port: expected integer, got \\\"not-a-number\\\"\")) (invalid (\"log-level: \\\"verbose\\\" not in allowed values\")) (invalid (\"workers: 0 below minimum 1\")) (valid))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Computed/derived config values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_computed_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Derived config: some values are computed from others
  ;; A derivation is (target-key . (lambda (config) ...))
  (fset 'neovm--test-apply-derivations
    (lambda (config derivations)
      (let ((result (copy-alist config))
            (changed t)
            (max-passes 10)
            (pass 0))
        ;; Fixed-point iteration: keep applying until stable
        (while (and changed (< pass max-passes))
          (setq changed nil)
          (setq pass (1+ pass))
          (dolist (d derivations)
            (let* ((key (car d))
                   (fn (cdr d))
                   (new-val (funcall fn result))
                   (old (assoc key result)))
              (cond
                ((and old (not (equal (cdr old) new-val)))
                 (setcdr old new-val)
                 (setq changed t))
                ((not old)
                 (setq result (cons (cons key new-val) result))
                 (setq changed t))))))
        (cons (list 'passes pass) result))))

  (unwind-protect
      (let* ((config '((host . "db.example.com")
                       (port . 5432)
                       (db-name . "myapp")
                       (ssl . t)
                       (pool-size . 10)
                       (max-idle . 5)))
             ;; Derivations
             (derivations
              (list
               ;; Connection string derived from host, port, db-name, ssl
               (cons 'conn-string
                     (lambda (c)
                       (format "%s://%s:%d/%s"
                               (if (cdr (assoc 'ssl c)) "postgresql+ssl" "postgresql")
                               (cdr (assoc 'host c))
                               (cdr (assoc 'port c))
                               (cdr (assoc 'db-name c)))))
               ;; Max idle must not exceed pool size
               (cons 'max-idle
                     (lambda (c)
                       (let ((pool (cdr (assoc 'pool-size c)))
                             (idle (cdr (assoc 'max-idle c))))
                         (if (and pool idle (> idle pool))
                             pool
                           idle))))
               ;; Total possible connections = pool-size * 2
               (cons 'max-total
                     (lambda (c)
                       (* (or (cdr (assoc 'pool-size c)) 1) 2)))))
             (result (funcall 'neovm--test-apply-derivations config derivations)))
        (list
          (cdr (assoc 'conn-string result))
          (cdr (assoc 'max-total result))
          (cdr (assoc 'max-idle result))
          (cdr (assoc 'passes result))
          ;; Test with max-idle > pool-size (should be clamped)
          (let* ((config2 '((host . "localhost") (port . 3306) (db-name . "test")
                            (ssl . nil) (pool-size . 5) (max-idle . 20)))
                 (result2 (funcall 'neovm--test-apply-derivations config2 derivations)))
            (list
              (cdr (assoc 'conn-string result2))
              (cdr (assoc 'max-idle result2))
              (cdr (assoc 'max-total result2))))))
    (fmakunbound 'neovm--test-apply-derivations)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"postgresql+ssl://db.example.com:5432/myapp\" 20 5 (2) (\"postgresql://localhost:3306/test\" 5 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Config diff: detect what changed between two configs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute diff between old and new config
  ;; Returns: ((added . alist) (removed . alist) (changed . alist))
  ;; where changed entries are (key old-val new-val)
  (fset 'neovm--test-config-diff
    (lambda (old-config new-config)
      (let ((added nil) (removed nil) (changed nil))
        ;; Find added and changed
        (dolist (entry new-config)
          (let ((old-entry (assoc (car entry) old-config)))
            (if old-entry
                (unless (equal (cdr old-entry) (cdr entry))
                  (setq changed (cons (list (car entry) (cdr old-entry) (cdr entry))
                                      changed)))
              (setq added (cons entry added)))))
        ;; Find removed
        (dolist (entry old-config)
          (unless (assoc (car entry) new-config)
            (setq removed (cons entry removed))))
        (list (cons 'added (nreverse added))
              (cons 'removed (nreverse removed))
              (cons 'changed (nreverse changed))))))

  ;; Apply diff to produce new config from old + diff
  (fset 'neovm--test-config-apply-diff
    (lambda (old-config diff)
      (let ((result (copy-alist old-config)))
        ;; Remove
        (dolist (entry (cdr (assoc 'removed diff)))
          (setq result (assq-delete-all (car entry) result)))
        ;; Add
        (dolist (entry (cdr (assoc 'added diff)))
          (setq result (cons entry result)))
        ;; Change
        (dolist (entry (cdr (assoc 'changed diff)))
          (let ((existing (assoc (car entry) result)))
            (when existing
              (setcdr existing (nth 2 entry)))))
        result)))

  (unwind-protect
      (let* ((old '((host . "localhost") (port . 8080) (debug . nil)
                    (log-level . "info") (workers . 4)))
             (new '((host . "0.0.0.0") (port . 8080) (debug . t)
                    (log-level . "debug") (ssl . t)))
             (diff (funcall 'neovm--test-config-diff old new)))
        (list
          ;; Diff contents
          (cdr (assoc 'added diff))     ;; ((ssl . t))
          (cdr (assoc 'removed diff))   ;; ((workers . 4))
          (cdr (assoc 'changed diff))   ;; host, debug, log-level
          ;; Count changes
          (length (cdr (assoc 'added diff)))
          (length (cdr (assoc 'removed diff)))
          (length (cdr (assoc 'changed diff)))
          ;; Apply diff to old should produce equivalent to new
          (let* ((reconstructed (funcall 'neovm--test-config-apply-diff old diff))
                 (all-match t))
            (dolist (entry new)
              (unless (equal (cdr (assoc (car entry) reconstructed))
                             (cdr entry))
                (setq all-match nil)))
            all-match)
          ;; Empty diff when configs are equal
          (let ((diff2 (funcall 'neovm--test-config-diff old old)))
            (list (length (cdr (assoc 'added diff2)))
                  (length (cdr (assoc 'removed diff2)))
                  (length (cdr (assoc 'changed diff2)))))))
    (fmakunbound 'neovm--test-config-diff)
    (fmakunbound 'neovm--test-config-apply-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ssl . t)) ((workers . 4)) ((host \"localhost\" \"0.0.0.0\") (debug nil t) (log-level \"info\" \"debug\")) 1 1 3 t (0 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Config inheritance: profiles inherit from base profiles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-config-profiles nil)

  ;; Register a profile with optional parent
  (fset 'neovm--test-register-profile
    (lambda (name parent config)
      (setq neovm--test-config-profiles
            (cons (list name parent config) neovm--test-config-profiles))))

  ;; Resolve a profile: walk up inheritance chain, merge configs
  (fset 'neovm--test-resolve-profile
    (lambda (name)
      (let ((profile (assoc name neovm--test-config-profiles))
            (chain nil))
        (while profile
          (setq chain (cons (nth 2 profile) chain))
          (let ((parent-name (nth 1 profile)))
            (setq profile
                  (if parent-name
                      (assoc parent-name neovm--test-config-profiles)
                    nil))))
        ;; Merge from root to leaf (later overrides earlier)
        (let ((result nil))
          (dolist (config chain)
            (dolist (entry config)
              (let ((existing (assoc (car entry) result)))
                (if existing
                    (setcdr existing (cdr entry))
                  (setq result (cons (cons (car entry) (cdr entry)) result))))))
          result))))

  ;; List the inheritance chain
  (fset 'neovm--test-inheritance-chain
    (lambda (name)
      (let ((profile (assoc name neovm--test-config-profiles))
            (chain nil))
        (while profile
          (setq chain (cons (car profile) chain))
          (let ((parent-name (nth 1 profile)))
            (setq profile
                  (if parent-name
                      (assoc parent-name neovm--test-config-profiles)
                    nil))))
        (nreverse chain))))

  (unwind-protect
      (progn
        (setq neovm--test-config-profiles nil)
        ;; Base profile
        (funcall 'neovm--test-register-profile 'base nil
                 '((host . "localhost") (port . 8080) (debug . nil)
                   (log-level . "info") (workers . 2) (ssl . nil)))
        ;; Development inherits base
        (funcall 'neovm--test-register-profile 'development 'base
                 '((debug . t) (log-level . "debug") (workers . 1)))
        ;; Staging inherits base
        (funcall 'neovm--test-register-profile 'staging 'base
                 '((host . "staging.example.com") (ssl . t) (workers . 4)))
        ;; Production inherits staging
        (funcall 'neovm--test-register-profile 'production 'staging
                 '((host . "prod.example.com") (workers . 16)
                   (log-level . "warn")))
        ;; CI inherits development
        (funcall 'neovm--test-register-profile 'ci 'development
                 '((host . "ci.local") (workers . 2)))

        (let ((dev (funcall 'neovm--test-resolve-profile 'development))
              (staging (funcall 'neovm--test-resolve-profile 'staging))
              (prod (funcall 'neovm--test-resolve-profile 'production))
              (ci (funcall 'neovm--test-resolve-profile 'ci)))
          (list
            ;; Dev: debug from dev, host from base
            (cdr (assoc 'debug dev))
            (cdr (assoc 'host dev))
            (cdr (assoc 'workers dev))
            ;; Staging: ssl from staging, port from base
            (cdr (assoc 'ssl staging))
            (cdr (assoc 'port staging))
            (cdr (assoc 'workers staging))
            ;; Production: inherits staging ssl, overrides host and workers
            (cdr (assoc 'host prod))
            (cdr (assoc 'ssl prod))
            (cdr (assoc 'workers prod))
            (cdr (assoc 'log-level prod))
            (cdr (assoc 'port prod))  ;; from base through staging
            ;; CI: inherits dev debug, overrides host
            (cdr (assoc 'debug ci))
            (cdr (assoc 'host ci))
            (cdr (assoc 'log-level ci))
            ;; Inheritance chains
            (funcall 'neovm--test-inheritance-chain 'production)
            (funcall 'neovm--test-inheritance-chain 'ci)
            (funcall 'neovm--test-inheritance-chain 'base))))
    (fmakunbound 'neovm--test-register-profile)
    (fmakunbound 'neovm--test-resolve-profile)
    (fmakunbound 'neovm--test-inheritance-chain)
    (makunbound 'neovm--test-config-profiles)))"#;
    let expect = expect_test::expect![[
        r#""OK (t \"localhost\" 1 t 8080 4 \"prod.example.com\" t 16 \"warn\" 8080 t \"ci.local\" \"debug\" (production staging base) (ci development base) (base))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Config serialization and deserialization to/from string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Serialize config alist to a sorted, deterministic string representation
  (fset 'neovm--test-config-serialize
    (lambda (config)
      (let ((sorted (sort (copy-alist config)
                          (lambda (a b) (string-lessp (symbol-name (car a))
                                                      (symbol-name (car b)))))))
        (mapconcat
          (lambda (entry)
            (format "%s=%S" (symbol-name (car entry)) (cdr entry)))
          sorted
          "\n"))))

  ;; Parse a simple key=value string back to alist
  ;; Values are read as Elisp objects
  (fset 'neovm--test-config-parse-line
    (lambda (line)
      (let ((eq-pos (string-match "=" line)))
        (when eq-pos
          (let ((key (intern (substring line 0 eq-pos)))
                (val-str (substring line (1+ eq-pos))))
            (cons key (car (read-from-string val-str))))))))

  (fset 'neovm--test-config-deserialize
    (lambda (str)
      (let ((lines (split-string str "\n" t))
            (result nil))
        (dolist (line lines)
          (let ((entry (funcall 'neovm--test-config-parse-line line)))
            (when entry
              (setq result (cons entry result)))))
        (nreverse result))))

  (unwind-protect
      (let* ((config '((port . 8080)
                       (host . "example.com")
                       (debug . t)
                       (workers . 4)
                       (tags . (web api public))))
             (serialized (funcall 'neovm--test-config-serialize config))
             (deserialized (funcall 'neovm--test-config-deserialize serialized)))
        (list
          ;; Serialized string
          serialized
          ;; Roundtrip: all values match
          (let ((all-match t))
            (dolist (entry config)
              (let ((restored (assoc (car entry) deserialized)))
                (unless (and restored (equal (cdr restored) (cdr entry)))
                  (setq all-match nil))))
            all-match)
          ;; Serialization is deterministic (sorted)
          (equal (funcall 'neovm--test-config-serialize config)
                 (funcall 'neovm--test-config-serialize (reverse config)))
          ;; Empty config
          (funcall 'neovm--test-config-serialize nil)
          ;; Single entry
          (funcall 'neovm--test-config-serialize '((x . 42)))))
    (fmakunbound 'neovm--test-config-serialize)
    (fmakunbound 'neovm--test-config-parse-line)
    (fmakunbound 'neovm--test-config-deserialize)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"debug=t\\nhost=\\\"example.com\\\"\\nport=8080\\ntags=(web api public)\\nworkers=4\" t t \"\" \"x=42\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Config watcher: track changes and notify observers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_config_watcher_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--test-cw-config nil)
  (defvar neovm--test-cw-watchers nil)
  (defvar neovm--test-cw-change-log nil)

  (fset 'neovm--test-cw-init
    (lambda (initial)
      (setq neovm--test-cw-config (copy-alist initial))
      (setq neovm--test-cw-watchers nil)
      (setq neovm--test-cw-change-log nil)))

  (fset 'neovm--test-cw-watch
    (lambda (key callback)
      (setq neovm--test-cw-watchers
            (cons (cons key callback) neovm--test-cw-watchers))))

  (fset 'neovm--test-cw-set
    (lambda (key value)
      (let ((old (cdr (assoc key neovm--test-cw-config)))
            (existing (assoc key neovm--test-cw-config)))
        (if existing
            (setcdr existing value)
          (setq neovm--test-cw-config
                (cons (cons key value) neovm--test-cw-config)))
        ;; Notify watchers
        (unless (equal old value)
          (setq neovm--test-cw-change-log
                (cons (list key old value) neovm--test-cw-change-log))
          (dolist (w neovm--test-cw-watchers)
            (when (eq (car w) key)
              (funcall (cdr w) key old value)))))))

  (fset 'neovm--test-cw-get
    (lambda (key)
      (cdr (assoc key neovm--test-cw-config))))

  (unwind-protect
      (let ((notifications nil))
        (funcall 'neovm--test-cw-init
                 '((theme . light) (font-size . 14) (auto-save . t)))
        ;; Add watchers
        (funcall 'neovm--test-cw-watch 'theme
                 (lambda (k old new)
                   (setq notifications
                         (cons (format "theme: %s -> %s" old new)
                               notifications))))
        (funcall 'neovm--test-cw-watch 'font-size
                 (lambda (k old new)
                   (setq notifications
                         (cons (format "font: %s -> %s" old new)
                               notifications))))
        ;; Make changes
        (funcall 'neovm--test-cw-set 'theme 'dark)
        (funcall 'neovm--test-cw-set 'font-size 16)
        (funcall 'neovm--test-cw-set 'font-size 18)
        (funcall 'neovm--test-cw-set 'auto-save nil)  ;; no watcher
        ;; Set same value (no notification)
        (funcall 'neovm--test-cw-set 'theme 'dark)
        (list
          ;; Current values
          (funcall 'neovm--test-cw-get 'theme)
          (funcall 'neovm--test-cw-get 'font-size)
          (funcall 'neovm--test-cw-get 'auto-save)
          ;; Notifications received (3: theme, font x2)
          (length notifications)
          (nreverse notifications)
          ;; Full change log (4 actual changes, not 5 because theme=dark was no-op)
          (length neovm--test-cw-change-log)
          (nreverse neovm--test-cw-change-log)))
    (fmakunbound 'neovm--test-cw-init)
    (fmakunbound 'neovm--test-cw-watch)
    (fmakunbound 'neovm--test-cw-set)
    (fmakunbound 'neovm--test-cw-get)
    (makunbound 'neovm--test-cw-config)
    (makunbound 'neovm--test-cw-watchers)
    (makunbound 'neovm--test-cw-change-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (dark 18 nil 3 (\"theme: light -> dark\" \"font: 14 -> 16\" \"font: 16 -> 18\") 4 ((theme light dark) (font-size 14 16) (font-size 16 18) (auto-save t nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
