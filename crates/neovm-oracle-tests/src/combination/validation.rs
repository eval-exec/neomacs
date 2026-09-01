//! Oracle parity tests for data validation patterns in Elisp.
//!
//! Covers: email format validation, date validation with leap year logic,
//! nested structure validation (schema checking), constraint propagation
//! (inter-field dependencies), form validation with error collection,
//! and type coercion with validation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Email format validation using regex
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_email_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate email addresses using a regex-based validator.
    // Returns an alist of (email . valid-p) for each test case.
    let form = r#"(unwind-protect
      (progn
        (defun test--validate-email (email)
          "Validate EMAIL format. Returns t if valid, error string if not."
          (cond
            ((not (stringp email))
             "not a string")
            ((= (length email) 0)
             "empty string")
            ((not (string-match
                    "\\`[a-zA-Z0-9._%+\\-]+@[a-zA-Z0-9.\\-]+\\.[a-zA-Z]\\{2,\\}\\'"
                    email))
             "invalid format")
            ;; Check for consecutive dots in local part
            ((string-match "\\.\\." (car (split-string email "@")))
             "consecutive dots in local part")
            ;; Check local part doesn't start/end with dot
            ((string-match "\\`\\." (car (split-string email "@")))
             "local part starts with dot")
            ((string-match "\\.@" email)
             "local part ends with dot")
            (t t)))

        (let ((test-emails
                '("user@example.com"
                  "first.last@company.org"
                  "user+tag@domain.co.uk"
                  "a@b.cd"
                  ""
                  "@missing-local.com"
                  "missing-at-sign"
                  "user@"
                  "user@.com"
                  "user@domain"
                  "user..name@example.com"
                  ".user@example.com"
                  "user.@example.com"
                  "valid_underscore@test.io"
                  "UPPER@CASE.COM")))
          (mapcar (lambda (e)
                    (cons e (test--validate-email e)))
                  test-emails)))
      ;; Cleanup
      (fmakunbound 'test--validate-email))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"user@example.com\" . t) (\"first.last@company.org\" . t) (\"user+tag@domain.co.uk\" . t) (\"a@b.cd\" . t) (\"\" . \"empty string\") (\"@missing-local.com\" . \"invalid format\") (\"missing-at-sign\" . \"invalid format\") (\"user@\" . \"invalid format\") (\"user@.com\" . \"invalid format\") (\"user@domain\" . \"invalid format\") (\"user..name@example.com\" . \"consecutive dots in local part\") (\".user@example.com\" . \"local part starts with dot\") (\"user.@example.com\" . \"local part ends with dot\") (\"valid_underscore@test.io\" . t) (\"UPPER@CASE.COM\" . t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Date validation with leap year logic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_date_leap_year() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate dates including leap year rules:
    // - Year divisible by 4 is leap, except centuries
    // - Centuries divisible by 400 are leap
    let form = r#"(unwind-protect
      (progn
        (defun test--leap-year-p (year)
          "Return t if YEAR is a leap year."
          (or (and (= (% year 4) 0)
                   (/= (% year 100) 0))
              (= (% year 400) 0)))

        (defun test--days-in-month (month year)
          "Return number of days in MONTH of YEAR."
          (cond
            ((memq month '(1 3 5 7 8 10 12)) 31)
            ((memq month '(4 6 9 11)) 30)
            ((= month 2) (if (test--leap-year-p year) 29 28))
            (t 0)))

        (defun test--validate-date (year month day)
          "Validate a date. Return t or an error description."
          (cond
            ((not (and (integerp year) (integerp month) (integerp day)))
             "non-integer component")
            ((< year 1) "year < 1")
            ((or (< month 1) (> month 12))
             (format "month %d out of range" month))
            ((< day 1)
             "day < 1")
            ((> day (test--days-in-month month year))
             (format "day %d exceeds max %d for month %d year %d"
                     day (test--days-in-month month year) month year))
            (t t)))

        (list
          ;; Valid dates
          (test--validate-date 2024 2 29)   ; leap year
          (test--validate-date 2023 2 28)   ; non-leap year
          (test--validate-date 2000 2 29)   ; century leap year (div by 400)
          (test--validate-date 2026 12 31)  ; last day of year
          (test--validate-date 2026 1 1)    ; first day of year
          ;; Invalid dates
          (test--validate-date 2023 2 29)   ; not a leap year
          (test--validate-date 1900 2 29)   ; century non-leap year
          (test--validate-date 2026 4 31)   ; April has 30 days
          (test--validate-date 2026 13 1)   ; month out of range
          (test--validate-date 2026 0 15)   ; month zero
          (test--validate-date 2026 6 0)    ; day zero
          (test--validate-date 0 5 15)      ; year zero
          ;; Leap year checks
          (test--leap-year-p 2000)
          (test--leap-year-p 1900)
          (test--leap-year-p 2024)
          (test--leap-year-p 2023)
          ;; Days-in-month spot checks
          (test--days-in-month 2 2024)
          (test--days-in-month 2 2023)
          (test--days-in-month 2 1900)))
      ;; Cleanup
      (fmakunbound 'test--leap-year-p)
      (fmakunbound 'test--days-in-month)
      (fmakunbound 'test--validate-date))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t \"day 29 exceeds max 28 for month 2 year 2023\" \"day 29 exceeds max 28 for month 2 year 1900\" \"day 31 exceeds max 30 for month 4 year 2026\" \"month 13 out of range\" \"month 0 out of range\" \"day < 1\" \"year < 1\" t nil t nil 29 28 28)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested structure validation (schema checking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_nested_schema_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate nested alist structures against a schema.
    // Schema is an alist of (key . validator) where validator is a function.
    // Nested schemas are supported via recursive validation.
    let form = r#"(unwind-protect
      (progn
        (defun test--validate-schema (data schema)
          "Validate DATA alist against SCHEMA. Returns list of errors or nil."
          (let ((errors nil))
            ;; Check each schema field
            (dolist (spec schema)
              (let* ((key (car spec))
                     (validator (cdr spec))
                     (value (cdr (assoc key data))))
                (cond
                  ;; Missing required field (validator is not 'optional)
                  ((and (null (assoc key data))
                        (not (eq validator 'optional)))
                   (setq errors (cons (format "missing required field: %s" key) errors)))
                  ;; Field present and has a sub-schema (list of validators)
                  ((and value (listp validator) (not (functionp validator)))
                   ;; Recursive validation
                   (let ((sub-errors (test--validate-schema value validator)))
                     (when sub-errors
                       (setq errors (append
                                      (mapcar (lambda (e) (format "%s.%s" key e))
                                              sub-errors)
                                      errors)))))
                  ;; Field present with function validator
                  ((and value (functionp validator))
                   (let ((result (funcall validator value)))
                     (unless (eq result t)
                       (setq errors (cons (format "%s: %s" key result) errors))))))))
            (nreverse errors)))

        (let* ((string-validator
                 (lambda (v)
                   (if (stringp v) t "expected string")))
               (positive-int-validator
                 (lambda (v)
                   (if (and (integerp v) (> v 0)) t "expected positive integer")))
               (email-validator
                 (lambda (v)
                   (if (and (stringp v)
                            (string-match "\\`[^@]+@[^@]+\\.[^@]+\\'" v))
                       t "invalid email")))
               ;; Schema for a user record
               (address-schema
                 (list (cons 'street string-validator)
                       (cons 'city string-validator)
                       (cons 'zip string-validator)))
               (user-schema
                 (list (cons 'name string-validator)
                       (cons 'age positive-int-validator)
                       (cons 'email email-validator)
                       (cons 'address address-schema)))
               ;; Valid user
               (valid-user
                 '((name . "Alice")
                   (age . 30)
                   (email . "alice@example.com")
                   (address . ((street . "123 Main St")
                               (city . "Springfield")
                               (zip . "62701")))))
               ;; Invalid user: bad age, missing email, bad zip type
               (invalid-user
                 '((name . "Bob")
                   (age . -5)
                   (address . ((street . "456 Oak Ave")
                               (city . "Shelbyville"))))))
          (list
            (test--validate-schema valid-user user-schema)
            (test--validate-schema invalid-user user-schema)
            ;; Minimal valid
            (test--validate-schema
              '((name . "C") (age . 1) (email . "c@d.e")
                (address . ((street . "x") (city . "y") (zip . "z"))))
              user-schema))))
      ;; Cleanup
      (fmakunbound 'test--validate-schema))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (\"age: expected positive integer\" \"missing required field: email\" \"address.missing required field: zip\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint propagation (inter-field dependencies)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_constraint_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate a record where fields have inter-dependencies:
    // - If role=admin, must have mfa_enabled=t
    // - If age < 18, cannot have role=admin
    // - If country="US", zip must be 5 digits
    // - If country="UK", zip must match letter-number pattern
    // - start_date must be <= end_date (as YYYYMMDD integers)
    let form = r#"(unwind-protect
      (progn
        (defun test--validate-constraints (record)
          "Validate inter-field constraints. Returns list of violations."
          (let ((errors nil)
                (role (cdr (assoc 'role record)))
                (mfa (cdr (assoc 'mfa_enabled record)))
                (age (cdr (assoc 'age record)))
                (country (cdr (assoc 'country record)))
                (zip (cdr (assoc 'zip record)))
                (start (cdr (assoc 'start_date record)))
                (end (cdr (assoc 'end_date record))))
            ;; Admin requires MFA
            (when (and (equal role "admin") (not (eq mfa t)))
              (setq errors (cons "admin role requires mfa_enabled=t" errors)))
            ;; Under 18 cannot be admin
            (when (and (numberp age) (< age 18) (equal role "admin"))
              (setq errors (cons "age < 18 cannot have admin role" errors)))
            ;; Country-specific zip validation
            (when (and (equal country "US") (stringp zip))
              (unless (string-match "\\`[0-9]\\{5\\}\\'" zip)
                (setq errors (cons "US zip must be 5 digits" errors))))
            (when (and (equal country "UK") (stringp zip))
              (unless (string-match "\\`[A-Z]\\{1,2\\}[0-9][0-9A-Z]? [0-9][A-Z]\\{2\\}\\'" zip)
                (setq errors (cons "UK zip format invalid" errors))))
            ;; Date range check
            (when (and (numberp start) (numberp end) (> start end))
              (setq errors (cons "start_date must be <= end_date" errors)))
            (nreverse errors)))

        (list
          ;; Valid admin record
          (test--validate-constraints
            '((role . "admin") (mfa_enabled . t) (age . 30)
              (country . "US") (zip . "90210")
              (start_date . 20260101) (end_date . 20261231)))
          ;; Admin without MFA
          (test--validate-constraints
            '((role . "admin") (mfa_enabled . nil) (age . 25)
              (country . "US") (zip . "12345")
              (start_date . 20260101) (end_date . 20260630)))
          ;; Under 18 admin (two violations)
          (test--validate-constraints
            '((role . "admin") (mfa_enabled . nil) (age . 16)
              (country . "US") (zip . "00000")
              (start_date . 20260101) (end_date . 20260301)))
          ;; Bad US zip
          (test--validate-constraints
            '((role . "user") (age . 40)
              (country . "US") (zip . "ABCDE")
              (start_date . 20260101) (end_date . 20260201)))
          ;; Valid UK zip
          (test--validate-constraints
            '((role . "user") (age . 25)
              (country . "UK") (zip . "SW1A 1AA")
              (start_date . 20260301) (end_date . 20260401)))
          ;; Bad UK zip
          (test--validate-constraints
            '((role . "user") (age . 25)
              (country . "UK") (zip . "12345")
              (start_date . 20260301) (end_date . 20260401)))
          ;; Inverted dates
          (test--validate-constraints
            '((role . "user") (age . 30)
              (country . "US") (zip . "55555")
              (start_date . 20261231) (end_date . 20260101)))))
      ;; Cleanup
      (fmakunbound 'test--validate-constraints))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (\"admin role requires mfa_enabled=t\") (\"admin role requires mfa_enabled=t\" \"age < 18 cannot have admin role\") (\"US zip must be 5 digits\") nil (\"UK zip format invalid\") (\"start_date must be <= end_date\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Form validation with error collection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_form_error_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate all fields of a "registration form", collecting ALL errors
    // rather than stopping at the first one. Returns (:ok data) or
    // (:errors error-list).
    let form = r#"(unwind-protect
      (progn
        (defun test--validate-field (name value rules)
          "Validate a single field against a list of RULES.
Each rule is (check-fn . error-msg). Returns list of error strings."
          (let ((errors nil))
            (dolist (rule rules)
              (unless (funcall (car rule) value)
                (setq errors (cons (format "%s: %s" name (cdr rule)) errors))))
            (nreverse errors)))

        (defun test--validate-form (form-data field-specs)
          "Validate FORM-DATA alist against FIELD-SPECS.
FIELD-SPECS is ((field-name . rules-list) ...).
Returns (:ok form-data) or (:errors error-list)."
          (let ((all-errors nil))
            (dolist (spec field-specs)
              (let* ((field-name (car spec))
                     (rules (cdr spec))
                     (value (cdr (assoc field-name form-data)))
                     (field-errors (test--validate-field
                                     (symbol-name field-name) value rules)))
                (setq all-errors (append all-errors field-errors))))
            (if all-errors
                (list :errors all-errors)
              (list :ok form-data))))

        (let* ((required-string
                 (cons (lambda (v) (and (stringp v) (> (length v) 0)))
                       "required non-empty string"))
               (min-length-3
                 (cons (lambda (v) (and (stringp v) (>= (length v) 3)))
                       "minimum length 3"))
               (max-length-50
                 (cons (lambda (v) (and (stringp v) (<= (length v) 50)))
                       "maximum length 50"))
               (valid-age
                 (cons (lambda (v) (and (integerp v) (>= v 0) (<= v 150)))
                       "must be integer 0-150"))
               (positive-number
                 (cons (lambda (v) (and (numberp v) (> v 0)))
                       "must be positive number"))
               (specs
                 (list
                   (cons 'username (list required-string min-length-3 max-length-50))
                   (cons 'age (list valid-age))
                   (cons 'score (list positive-number)))))
          (list
            ;; All valid
            (test--validate-form
              '((username . "alice") (age . 30) (score . 95.5))
              specs)
            ;; Multiple errors: empty username, negative age, zero score
            (test--validate-form
              '((username . "") (age . -1) (score . 0))
              specs)
            ;; Username too short
            (test--validate-form
              '((username . "ab") (age . 25) (score . 10))
              specs)
            ;; Missing field (nil value)
            (test--validate-form
              '((username . nil) (age . 50) (score . 1))
              specs)
            ;; Edge case: age at boundary
            (test--validate-form
              '((username . "bob") (age . 0) (score . 0.001))
              specs)
            (test--validate-form
              '((username . "bob") (age . 150) (score . 1))
              specs))))
      ;; Cleanup
      (fmakunbound 'test--validate-field)
      (fmakunbound 'test--validate-form))"#;
    let expect = expect_test::expect![[
        r#""OK ((:ok ((username . \"alice\") (age . 30) (score . 95.5))) (:errors (\"username: required non-empty string\" \"username: minimum length 3\" \"age: must be integer 0-150\" \"score: must be positive number\")) (:errors (\"username: minimum length 3\")) (:errors (\"username: required non-empty string\" \"username: minimum length 3\" \"username: maximum length 50\")) (:ok ((username . \"bob\") (age . 0) (score . 0.001))) (:ok ((username . \"bob\") (age . 150) (score . 1))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type coercion with validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_type_coercion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse and validate typed values from string representations:
    // coerce strings to numbers/booleans, validate ranges, collect results.
    let form = r#"(unwind-protect
      (progn
        (defun test--coerce-and-validate (value type-spec)
          "Coerce VALUE (a string) according to TYPE-SPEC.
TYPE-SPEC is one of:
  (integer MIN MAX) - parse integer, check range
  (float MIN MAX) - parse float, check range
  (boolean) - \"true\"/\"false\" to t/nil
  (enum . ALLOWED-VALUES) - check membership
Returns (ok . coerced-value) or (error . message)."
          (cond
            ;; Integer coercion
            ((and (listp type-spec) (eq (car type-spec) 'integer))
             (let ((n (string-to-number value))
                   (min-val (nth 1 type-spec))
                   (max-val (nth 2 type-spec)))
               (cond
                 ((not (string-match "\\`-?[0-9]+\\'" value))
                  (cons 'error (format "not a valid integer: %s" value)))
                 ((and min-val (< n min-val))
                  (cons 'error (format "%d < minimum %d" n min-val)))
                 ((and max-val (> n max-val))
                  (cons 'error (format "%d > maximum %d" n max-val)))
                 (t (cons 'ok n)))))
            ;; Float coercion
            ((and (listp type-spec) (eq (car type-spec) 'float))
             (let ((n (string-to-number value))
                   (min-val (nth 1 type-spec))
                   (max-val (nth 2 type-spec)))
               (cond
                 ((not (string-match "\\`-?[0-9]+\\(?:\\.[0-9]+\\)?\\'" value))
                  (cons 'error (format "not a valid float: %s" value)))
                 ((and min-val (< n min-val))
                  (cons 'error (format "%s < minimum %s" value min-val)))
                 ((and max-val (> n max-val))
                  (cons 'error (format "%s > maximum %s" value max-val)))
                 (t (cons 'ok n)))))
            ;; Boolean coercion
            ((and (listp type-spec) (eq (car type-spec) 'boolean))
             (cond
               ((member value '("true" "yes" "1" "on")) (cons 'ok t))
               ((member value '("false" "no" "0" "off")) (cons 'ok nil))
               (t (cons 'error (format "not a boolean: %s" value)))))
            ;; Enum coercion
            ((and (listp type-spec) (eq (car type-spec) 'enum))
             (let ((allowed (cdr type-spec)))
               (if (member value allowed)
                   (cons 'ok value)
                 (cons 'error (format "%s not in allowed values: %s"
                                      value allowed)))))
            (t (cons 'error "unknown type-spec"))))

        ;; Process a batch of config values
        (let ((config-raw '(("port" . "8080")
                            ("workers" . "4")
                            ("timeout" . "30.5")
                            ("debug" . "true")
                            ("mode" . "production")
                            ;; Invalid entries
                            ("bad-port" . "abc")
                            ("neg-workers" . "-3")
                            ("bad-bool" . "maybe")
                            ("bad-mode" . "staging")))
              (type-specs '(("port" . (integer 1 65535))
                            ("workers" . (integer 1 64))
                            ("timeout" . (float 0.0 300.0))
                            ("debug" . (boolean))
                            ("mode" . (enum "development" "testing" "production"))
                            ("bad-port" . (integer 1 65535))
                            ("neg-workers" . (integer 1 64))
                            ("bad-bool" . (boolean))
                            ("bad-mode" . (enum "development" "testing" "production")))))
          (mapcar (lambda (entry)
                    (let* ((key (car entry))
                           (raw-val (cdr entry))
                           (spec (cdr (assoc key type-specs))))
                      (cons key (test--coerce-and-validate raw-val spec))))
                  config-raw)))
      ;; Cleanup
      (fmakunbound 'test--coerce-and-validate))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"port\" ok . 8080) (\"workers\" ok . 4) (\"timeout\" ok . 30.5) (\"debug\" ok . t) (\"mode\" ok . \"production\") (\"bad-port\" error . \"not a valid integer: abc\") (\"neg-workers\" error . \"-3 < minimum 1\") (\"bad-bool\" error . \"not a boolean: maybe\") (\"bad-mode\" error . \"staging not in allowed values: (development testing production)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Composite validator: chained validations with short-circuit and transform
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_validation_chained_validators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build composable validators that can be chained: each validator
    // either passes (returning transformed value) or fails (returning error).
    // Validators: trim, non-empty, max-length, matches-pattern, transform.
    let form = r#"(unwind-protect
      (progn
        (defun test--chain-validate (value validators)
          "Run VALUE through a chain of VALIDATORS.
Each validator is (fn . error-msg). fn takes value, returns new value or nil on failure.
Returns (ok . final-value) or (error . first-error-msg)."
          (let ((current value)
                (failed nil)
                (error-msg nil))
            (dolist (v validators)
              (unless failed
                (let ((result (funcall (car v) current)))
                  (if result
                      (setq current result)
                    (setq failed t)
                    (setq error-msg (cdr v))))))
            (if failed
                (cons 'error error-msg)
              (cons 'ok current))))

        ;; Build validator chains
        (let ((trim-v (cons (lambda (v) (string-trim v)) ""))
              (non-empty-v (cons (lambda (v) (if (> (length v) 0) v nil))
                                 "must not be empty"))
              (max-20-v (cons (lambda (v) (if (<= (length v) 20) v nil))
                              "exceeds 20 chars"))
              (alpha-only-v (cons (lambda (v)
                                    (if (string-match "\\`[a-zA-Z]+\\'" v) v nil))
                                  "must be alphabetic only"))
              (downcase-v (cons (lambda (v) (downcase v)) "")))
          (let ((username-chain (list trim-v non-empty-v max-20-v
                                      alpha-only-v downcase-v)))
            (list
              ;; Valid: gets trimmed and downcased
              (test--chain-validate "  Alice  " username-chain)
              ;; Valid: already clean
              (test--chain-validate "bob" username-chain)
              ;; Fail: empty after trim
              (test--chain-validate "   " username-chain)
              ;; Fail: too long
              (test--chain-validate "abcdefghijklmnopqrstuvwxyz" username-chain)
              ;; Fail: non-alphabetic
              (test--chain-validate "user123" username-chain)
              ;; Fail: has spaces (after trim, still has internal space)
              (test--chain-validate "hello world" username-chain)))))
      ;; Cleanup
      (fmakunbound 'test--chain-validate))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok . \"alice\") (ok . \"bob\") (error . \"must not be empty\") (error . \"exceeds 20 chars\") (error . \"must be alphabetic only\") (error . \"must be alphabetic only\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
