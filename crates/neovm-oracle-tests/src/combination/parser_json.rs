//! Oracle parity tests implementing a complete JSON parser in Elisp.
//! Parses strings (with escape sequences), numbers (integers, floats,
//! negative, scientific notation), booleans, null, arrays (nested),
//! objects (nested), and handles error cases for malformed JSON.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

/// Shared JSON parser preamble: all parser functions.
/// The parser works by maintaining a position index into the input string,
/// returning (parsed-value . new-position) pairs.
const PARSER_PREAMBLE: &str = r#"
  ;; Skip whitespace
  (fset 'neovm--jp2-skip-ws
    (lambda (str pos)
      (let ((len (length str)))
        (while (and (< pos len)
                    (memq (aref str pos) '(?\s ?\t ?\n ?\r)))
          (setq pos (1+ pos)))
        pos)))

  ;; Peek at current character (nil if at end)
  (fset 'neovm--jp2-peek
    (lambda (str pos)
      (if (< pos (length str))
          (aref str pos)
        nil)))

  ;; Parse a JSON string (expects pos at opening quote)
  (fset 'neovm--jp2-parse-string
    (lambda (str pos)
      (if (not (= (aref str pos) ?\"))
          (cons 'error pos)
        (let ((result nil)
              (i (1+ pos))
              (len (length str))
              (done nil))
          (while (and (< i len) (not done))
            (let ((ch (aref str i)))
              (cond
                ((= ch ?\")
                 (setq done t))
                ((= ch ?\\)
                 (setq i (1+ i))
                 (if (>= i len)
                     (setq done t)
                   (let ((esc (aref str i)))
                     (cond
                       ((= esc ?\") (setq result (cons ?\" result)))
                       ((= esc ?\\) (setq result (cons ?\\ result)))
                       ((= esc ?/) (setq result (cons ?/ result)))
                       ((= esc ?n) (setq result (cons ?\n result)))
                       ((= esc ?t) (setq result (cons ?\t result)))
                       ((= esc ?r) (setq result (cons ?\r result)))
                       ((= esc ?b) (setq result (cons ?\b result)))
                       ((= esc ?f) (setq result (cons ?\f result)))
                       ((= esc ?u)
                        ;; Parse 4 hex digits
                        (if (> (+ i 4) len)
                            (setq done t)
                          (let ((hex-str (substring str (1+ i) (+ i 5))))
                            (let ((code (string-to-number hex-str 16)))
                              (setq result (cons code result))
                              (setq i (+ i 4))))))
                       (t (setq result (cons esc result))))))
                 (setq i (1+ i)))
                (t
                 (setq result (cons ch result))
                 (setq i (1+ i))))))
          (if done
              (cons (concat (nreverse result)) (1+ i))
            (cons 'error i))))))

  ;; Parse a JSON number
  (fset 'neovm--jp2-parse-number
    (lambda (str pos)
      (let ((start pos)
            (len (length str))
            (i pos))
        ;; Optional minus
        (when (and (< i len) (= (aref str i) ?-))
          (setq i (1+ i)))
        ;; Integer part
        (while (and (< i len) (>= (aref str i) ?0) (<= (aref str i) ?9))
          (setq i (1+ i)))
        ;; Decimal part
        (when (and (< i len) (= (aref str i) ?.))
          (setq i (1+ i))
          (while (and (< i len) (>= (aref str i) ?0) (<= (aref str i) ?9))
            (setq i (1+ i))))
        ;; Exponent
        (when (and (< i len) (memq (aref str i) '(?e ?E)))
          (setq i (1+ i))
          (when (and (< i len) (memq (aref str i) '(?+ ?-)))
            (setq i (1+ i)))
          (while (and (< i len) (>= (aref str i) ?0) (<= (aref str i) ?9))
            (setq i (1+ i))))
        (if (= i start)
            (cons 'error pos)
          (let ((num-str (substring str start i)))
            (cons (string-to-number num-str) i))))))

  ;; Forward declaration for mutual recursion
  (fset 'neovm--jp2-parse-value (lambda (str pos) nil))

  ;; Parse a JSON array
  (fset 'neovm--jp2-parse-array
    (lambda (str pos)
      (if (not (= (aref str pos) ?\[))
          (cons 'error pos)
        (let ((i (funcall 'neovm--jp2-skip-ws str (1+ pos)))
              (elements nil)
              (done nil)
              (err nil))
          ;; Empty array
          (if (and (< i (length str)) (= (aref str i) ?\]))
              (cons (list) (1+ i))
            ;; Parse elements
            (while (and (not done) (not err))
              (let ((result (funcall 'neovm--jp2-parse-value str i)))
                (if (eq (car result) 'error)
                    (setq err t)
                  (setq elements (cons (car result) elements))
                  (setq i (funcall 'neovm--jp2-skip-ws str (cdr result)))
                  (if (and (< i (length str)) (= (aref str i) ?,))
                      (setq i (funcall 'neovm--jp2-skip-ws str (1+ i)))
                    (if (and (< i (length str)) (= (aref str i) ?\]))
                        (progn (setq done t) (setq i (1+ i)))
                      (setq err t)))))))
          (if err
              (cons 'error i)
            (cons (nreverse elements) i))))))

  ;; Parse a JSON object
  (fset 'neovm--jp2-parse-object
    (lambda (str pos)
      (if (not (= (aref str pos) ?\{))
          (cons 'error pos)
        (let ((i (funcall 'neovm--jp2-skip-ws str (1+ pos)))
              (pairs nil)
              (done nil)
              (err nil))
          ;; Empty object
          (if (and (< i (length str)) (= (aref str i) ?\}))
              (cons (list) (1+ i))
            ;; Parse key-value pairs
            (while (and (not done) (not err))
              ;; Parse key (must be string)
              (let ((key-result (funcall 'neovm--jp2-parse-string str i)))
                (if (eq (car key-result) 'error)
                    (setq err t)
                  (setq i (funcall 'neovm--jp2-skip-ws str (cdr key-result)))
                  ;; Expect colon
                  (if (or (>= i (length str)) (not (= (aref str i) ?:)))
                      (setq err t)
                    (setq i (funcall 'neovm--jp2-skip-ws str (1+ i)))
                    ;; Parse value
                    (let ((val-result (funcall 'neovm--jp2-parse-value str i)))
                      (if (eq (car val-result) 'error)
                          (setq err t)
                        (setq pairs (cons (cons (car key-result) (car val-result))
                                          pairs))
                        (setq i (funcall 'neovm--jp2-skip-ws str (cdr val-result)))
                        (if (and (< i (length str)) (= (aref str i) ?,))
                            (setq i (funcall 'neovm--jp2-skip-ws str (1+ i)))
                          (if (and (< i (length str)) (= (aref str i) ?\}))
                              (progn (setq done t) (setq i (1+ i)))
                            (setq err t))))))))))
          (if err
              (cons 'error i)
            (cons (nreverse pairs) i))))))

  ;; Main parse-value dispatcher
  (fset 'neovm--jp2-parse-value
    (lambda (str pos)
      (let ((i (funcall 'neovm--jp2-skip-ws str pos)))
        (if (>= i (length str))
            (cons 'error i)
          (let ((ch (aref str i)))
            (cond
              ;; String
              ((= ch ?\")
               (funcall 'neovm--jp2-parse-string str i))
              ;; Number (digit or minus)
              ((or (and (>= ch ?0) (<= ch ?9)) (= ch ?-))
               (funcall 'neovm--jp2-parse-number str i))
              ;; Array
              ((= ch ?\[)
               (funcall 'neovm--jp2-parse-array str i))
              ;; Object
              ((= ch ?\{)
               (funcall 'neovm--jp2-parse-object str i))
              ;; true
              ((and (<= (+ i 4) (length str))
                    (string= (substring str i (+ i 4)) "true"))
               (cons t (+ i 4)))
              ;; false
              ((and (<= (+ i 5) (length str))
                    (string= (substring str i (+ i 5)) "false"))
               (cons nil (+ i 5)))
              ;; null
              ((and (<= (+ i 4) (length str))
                    (string= (substring str i (+ i 4)) "null"))
               (cons 'null (+ i 4)))
              (t (cons 'error i))))))))

  ;; Convenience: parse a complete JSON string
  (fset 'neovm--jp2-parse
    (lambda (str)
      (let ((result (funcall 'neovm--jp2-parse-value str 0)))
        (car result))))
"#;

const PARSER_CLEANUP: &str = r#"
    (fmakunbound 'neovm--jp2-skip-ws)
    (fmakunbound 'neovm--jp2-peek)
    (fmakunbound 'neovm--jp2-parse-string)
    (fmakunbound 'neovm--jp2-parse-number)
    (fmakunbound 'neovm--jp2-parse-array)
    (fmakunbound 'neovm--jp2-parse-object)
    (fmakunbound 'neovm--jp2-parse-value)
    (fmakunbound 'neovm--jp2-parse)
"#;

// ---------------------------------------------------------------------------
// Parse strings with escape sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r##"(progn
  {PARSER_PREAMBLE}
  (unwind-protect
      (list
        ;; Simple string
        (funcall 'neovm--jp2-parse "\"hello\"")
        ;; Empty string
        (funcall 'neovm--jp2-parse "\"\"")
        ;; String with spaces
        (funcall 'neovm--jp2-parse "\"hello world\"")
        ;; Escaped quotes
        (funcall 'neovm--jp2-parse "\"say \\\"hi\\\"\"")
        ;; Escaped backslash
        (funcall 'neovm--jp2-parse "\"path\\\\to\\\\file\"")
        ;; Escaped newline and tab
        (funcall 'neovm--jp2-parse "\"line1\\nline2\\ttab\"")
        ;; Unicode escape
        (funcall 'neovm--jp2-parse "\"\\u0041\\u0042\\u0043\"")
        ;; Mixed escapes
        (funcall 'neovm--jp2-parse "\"a\\tb\\nc\\\\d\\\"e\""))
    {PARSER_CLEANUP}))"##,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Parse numbers: integers, floats, negative, scientific
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {PARSER_PREAMBLE}
  (unwind-protect
      (list
        ;; Zero
        (funcall 'neovm--jp2-parse "0")
        ;; Positive integer
        (funcall 'neovm--jp2-parse "42")
        (funcall 'neovm--jp2-parse "12345")
        ;; Negative integer
        (funcall 'neovm--jp2-parse "-1")
        (funcall 'neovm--jp2-parse "-999")
        ;; Float
        (funcall 'neovm--jp2-parse "3.14")
        (funcall 'neovm--jp2-parse "0.5")
        (funcall 'neovm--jp2-parse "-2.718")
        ;; Scientific notation
        (funcall 'neovm--jp2-parse "1e10")
        (funcall 'neovm--jp2-parse "2.5e3")
        (funcall 'neovm--jp2-parse "1.5E-4")
        (funcall 'neovm--jp2-parse "-3.14e+2"))
    {PARSER_CLEANUP}))"#,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Parse booleans and null
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_booleans_null() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {PARSER_PREAMBLE}
  (unwind-protect
      (list
        ;; true -> t
        (funcall 'neovm--jp2-parse "true")
        ;; false -> nil
        (funcall 'neovm--jp2-parse "false")
        ;; null -> 'null
        (funcall 'neovm--jp2-parse "null")
        ;; With surrounding whitespace
        (funcall 'neovm--jp2-parse "  true  ")
        (funcall 'neovm--jp2-parse "\n\tfalse\n")
        (funcall 'neovm--jp2-parse "  null  ")
        ;; Verify types
        (eq (funcall 'neovm--jp2-parse "true") t)
        (null (funcall 'neovm--jp2-parse "false"))
        (eq (funcall 'neovm--jp2-parse "null") 'null))
    {PARSER_CLEANUP}))"#,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Parse arrays (nested)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_arrays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r##"(progn
  {PARSER_PREAMBLE}
  (unwind-protect
      (list
        ;; Empty array
        (funcall 'neovm--jp2-parse "[]")
        ;; Single element
        (funcall 'neovm--jp2-parse "[1]")
        ;; Multiple integers
        (funcall 'neovm--jp2-parse "[1, 2, 3]")
        ;; Mixed types
        (funcall 'neovm--jp2-parse "[1, \"hello\", true, null, 3.14]")
        ;; Nested arrays
        (funcall 'neovm--jp2-parse "[[1, 2], [3, 4]]")
        ;; Deeply nested
        (funcall 'neovm--jp2-parse "[[[1]], [[2]], [[3]]]")
        ;; Array with objects
        (funcall 'neovm--jp2-parse "[{{\"a\": 1}}, {{\"b\": 2}}]")
        ;; Whitespace variations
        (funcall 'neovm--jp2-parse "[ 1 , 2 , 3 ]")
        ;; Empty nested
        (funcall 'neovm--jp2-parse "[[], [], []]"))
    {PARSER_CLEANUP}))"##,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Parse objects (nested)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r##"(progn
  {PARSER_PREAMBLE}
  (unwind-protect
      (list
        ;; Empty object
        (funcall 'neovm--jp2-parse "{{}}")
        ;; Single pair
        (funcall 'neovm--jp2-parse "{{\"name\": \"Alice\"}}")
        ;; Multiple pairs
        (funcall 'neovm--jp2-parse "{{\"name\": \"Bob\", \"age\": 30}}")
        ;; Nested object
        (funcall 'neovm--jp2-parse "{{\"person\": {{\"name\": \"Carol\", \"age\": 25}}}}")
        ;; Object with array value
        (funcall 'neovm--jp2-parse "{{\"numbers\": [1, 2, 3]}}")
        ;; Object with mixed values
        (funcall 'neovm--jp2-parse "{{\"str\": \"hello\", \"num\": 42, \"bool\": true, \"nil\": null}}")
        ;; Complex nested structure
        (funcall 'neovm--jp2-parse
          "{{\"users\": [{{\"name\": \"Alice\", \"scores\": [90, 85]}}, {{\"name\": \"Bob\", \"scores\": [75, 92]}}]}}")
        ;; Access nested values from parsed result
        (let ((parsed (funcall 'neovm--jp2-parse
                        "{{\"a\": {{\"b\": {{\"c\": 42}}}}}}")))
          (cdr (assoc "c" (cdr (assoc "b" (cdr (assoc "a" parsed))))))))
    {PARSER_CLEANUP}))"##,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Error handling for malformed JSON
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r##"(progn
  {PARSER_PREAMBLE}
  (unwind-protect
      (list
        ;; Completely empty input
        (funcall 'neovm--jp2-parse "")
        ;; Only whitespace
        (funcall 'neovm--jp2-parse "   ")
        ;; Truncated string
        (funcall 'neovm--jp2-parse "\"unterminated")
        ;; Invalid token
        (funcall 'neovm--jp2-parse "undefined")
        ;; Trailing comma in array
        ;; (our parser may or may not handle this gracefully)
        ;; Object missing value
        (funcall 'neovm--jp2-parse "{{\"key\":}}")
        ;; Missing closing bracket
        (funcall 'neovm--jp2-parse "[1, 2, 3")
        ;; Missing closing brace
        (funcall 'neovm--jp2-parse "{{\"a\": 1"))
    {PARSER_CLEANUP}))"##,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Complex: full round-trip parse and query
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_json_parse_complex_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a complex JSON structure and extract values from it using
    // standard alist operations.
    let form = format!(
        r##"(progn
  {PARSER_PREAMBLE}

  ;; Helper: get nested value from parsed JSON by path (list of string keys)
  (fset 'neovm--jp2-get-in
    (lambda (data keys)
      (let ((current data))
        (while (and keys current)
          (cond
            ((and (listp current) (consp (car-safe current))
                  (stringp (caar current)))
             ;; Object (alist): look up key
             (let ((found (assoc (car keys) current)))
               (setq current (if found (cdr found) nil))))
            ((and (listp current) (integerp (car keys)))
             ;; Array (list): index into it
             (setq current (nth (car keys) current)))
            (t (setq current nil)))
          (setq keys (cdr keys)))
        current)))

  (unwind-protect
      (let ((json-str "{{\"company\": \"Acme\", \"employees\": [{{\"name\": \"Alice\", \"dept\": \"eng\", \"skills\": [\"lisp\", \"rust\"]}}, {{\"name\": \"Bob\", \"dept\": \"sales\", \"skills\": [\"marketing\"]}}, {{\"name\": \"Carol\", \"dept\": \"eng\", \"skills\": [\"python\", \"lisp\", \"go\"]}}], \"active\": true, \"headcount\": 3}}"))
        (let ((data (funcall 'neovm--jp2-parse json-str)))
          (list
            ;; Top-level string
            (funcall 'neovm--jp2-get-in data '("company"))
            ;; Top-level boolean
            (funcall 'neovm--jp2-get-in data '("active"))
            ;; Top-level number
            (funcall 'neovm--jp2-get-in data '("headcount"))
            ;; Number of employees
            (length (funcall 'neovm--jp2-get-in data '("employees")))
            ;; First employee name
            (let ((emp0 (nth 0 (funcall 'neovm--jp2-get-in data '("employees")))))
              (cdr (assoc "name" emp0)))
            ;; Second employee dept
            (let ((emp1 (nth 1 (funcall 'neovm--jp2-get-in data '("employees")))))
              (cdr (assoc "dept" emp1)))
            ;; Third employee skills (list)
            (let ((emp2 (nth 2 (funcall 'neovm--jp2-get-in data '("employees")))))
              (cdr (assoc "skills" emp2)))
            ;; Count total skills across all employees
            (let ((emps (funcall 'neovm--jp2-get-in data '("employees"))))
              (apply #'+ (mapcar (lambda (e)
                                   (length (cdr (assoc "skills" e))))
                                 emps))))))
    {PARSER_CLEANUP}
    (fmakunbound 'neovm--jp2-get-in)))"##,
        PARSER_PREAMBLE = PARSER_PREAMBLE,
        PARSER_CLEANUP = PARSER_CLEANUP
    );
    crate::common::assert_oracle_parity(&form);
}
