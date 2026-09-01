//! Advanced oracle parity tests for `message` and `format-message` patterns:
//! format with %s/%d, message return value, complex formatting pipelines,
//! message in loops collecting results, format-message with curly quotes,
//! multi-arg message calls, and message-based logging systems.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// message returns formatted string, format-message basics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_message_return_value_and_format_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `message` returns the formatted string (same as format).
    // `format-message` converts grave-accent quotes to curved quotes.
    // Verify return values, format specifiers, and quoting behavior.
    let form = r#"
(list
  ;; message returns formatted string
  (message "%s is %d" "Alice" 30)
  (message "hello %s" "world")
  (message "%d + %d = %d" 2 3 5)
  (message "100%%")
  (message "%S" '(a b c))

  ;; format-message basic usage
  (format-message "hello %s" "world")
  (format-message "%d items" 42)
  (format-message "%s=%S" "key" "value")

  ;; format-message with curved quote conversion
  ;; In Emacs, `foo' becomes curly quotes in format-message
  (format-message "Use `%s' for help" "C-h")
  (format-message "Variable `%s' is %S" "x" 42)
  (format-message "Try `M-x %s'" "describe-function")

  ;; message with nil argument
  (message "%s" nil)
  (message nil)

  ;; format-message with all major format specs
  (format-message "%s %d %x %o %c %%" "str" 42 255 8 65))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice is 30\" \"hello world\" \"2 + 3 = 5\" \"100%\" \"(a b c)\" \"hello world\" \"42 items\" \"key=\\\"value\\\"\" \"Use ‘C-h’ for help\" \"Variable ‘x’ is 42\" \"Try ‘M-x describe-function’\" \"nil\" nil \"str 42 ff 10 A %\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex multi-arg message and format pipelines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_message_complex_formatting_pipelines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build complex strings through chains of message/format calls,
    // including nested formatting, conditional messages, and dynamic
    // format string construction.
    let form = r#"
(progn
  (fset 'neovm--fmp-log-entry
    (lambda (level component msg)
      (format-message "[%s] %s: %s" level component msg)))

  (fset 'neovm--fmp-format-table-row
    (lambda (name value unit)
      (format-message "%-15s %8.2f %s" name value unit)))

  (unwind-protect
      (list
        ;; Log entry formatting
        (funcall 'neovm--fmp-log-entry "INFO" "server" "started on port 8080")
        (funcall 'neovm--fmp-log-entry "WARN" "db" "connection pool low")
        (funcall 'neovm--fmp-log-entry "ERROR" "auth" "invalid token")

        ;; Table rows
        (funcall 'neovm--fmp-format-table-row "Temperature" 23.45 "C")
        (funcall 'neovm--fmp-format-table-row "Pressure" 1013.25 "hPa")
        (funcall 'neovm--fmp-format-table-row "Humidity" 67.80 "%")

        ;; Nested format: build format string dynamically
        (let ((fmt (format "%%-%ds %%-%ds %%s" 10 20)))
          (format fmt "ID" "Name" "Status"))

        ;; Chain of format calls building up a report
        (let* ((header (format "=== Report: %s ===" "Monthly Summary"))
               (line1 (format "  Total: %d items" 1523))
               (line2 (format "  Average: %.1f per day" (/ 1523.0 30)))
               (line3 (format "  Peak: %d on %s" 89 "2026-01-15"))
               (footer (format "=== End of %s ===" "report")))
          (mapconcat #'identity (list header line1 line2 line3 footer) "\n")))
    (fmakunbound 'neovm--fmp-log-entry)
    (fmakunbound 'neovm--fmp-format-table-row)))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"[INFO] server: started on port 8080\" \"[WARN] db: connection pool low\" \"[ERROR] auth: invalid token\" \"Temperature        23.45 C\" \"Pressure         1013.25 hPa\" \"Humidity           67.80 %\" \"ID         Name                 Status\" \"=== Report: Monthly Summary ===\\n  Total: 1523 items\\n  Average: 50.8 per day\\n  Peak: 89 on 2026-01-15\\n=== End of report ===\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// message in loops collecting formatted results
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_message_in_loops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use message/format-message inside loops to generate formatted sequences.
    // Collect the return values and verify consistency.
    let form = r#"
(let ((results nil))
  ;; Loop 1: countdown messages
  (let ((countdown nil))
    (dotimes (i 5)
      (setq countdown
            (cons (message "T-%d seconds" (- 4 i)) countdown)))
    (setq results (cons (list 'countdown (nreverse countdown)) results)))

  ;; Loop 2: format-message with varying args
  (let ((items '(("apple" 3 1.50)
                 ("banana" 12 0.25)
                 ("cherry" 50 0.10)
                 ("date" 7 2.00)
                 ("elderberry" 1 15.00)))
        (formatted nil))
    (dolist (item items)
      (setq formatted
            (cons (format-message "%-12s qty=%-4d $%.2f total=$%.2f"
                                  (nth 0 item)
                                  (nth 1 item)
                                  (nth 2 item)
                                  (* (nth 1 item) (nth 2 item)))
                  formatted)))
    (setq results (cons (list 'invoice (nreverse formatted)) results)))

  ;; Loop 3: building numbered list
  (let ((numbered nil)
        (items '("first" "second" "third" "fourth" "fifth")))
    (let ((i 1))
      (dolist (item items)
        (setq numbered
              (cons (format-message "%d. %s" i item) numbered))
        (setq i (1+ i))))
    (setq results (cons (list 'numbered (nreverse numbered)) results)))

  ;; Loop 4: accumulating message results to verify return value
  (let ((msg-results nil))
    (dolist (n '(1 2 3 4 5))
      (let ((msg-ret (message "square(%d) = %d" n (* n n))))
        (setq msg-results (cons msg-ret msg-results))))
    (setq results (cons (list 'msg-returns (nreverse msg-results)) results)))

  (nreverse results))
"#;
    let expect = expect_test::expect![[
        r#""OK ((countdown (\"T-4 seconds\" \"T-3 seconds\" \"T-2 seconds\" \"T-1 seconds\" \"T-0 seconds\")) (invoice (\"apple        qty=3    $1.50 total=$4.50\" \"banana       qty=12   $0.25 total=$3.00\" \"cherry       qty=50   $0.10 total=$5.00\" \"date         qty=7    $2.00 total=$14.00\" \"elderberry   qty=1    $15.00 total=$15.00\")) (numbered (\"1. first\" \"2. second\" \"3. third\" \"4. fourth\" \"5. fifth\")) (msg-returns (\"square(1) = 1\" \"square(2) = 4\" \"square(3) = 9\" \"square(4) = 16\" \"square(5) = 25\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format-message curly quote conversion with various patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_message_curly_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // format-message converts `...' to curly quotes (when text-quoting-style
    // allows). Test various patterns of quoting, nesting, and edge cases.
    let form = r#"
(let ((text-quoting-style 'curve))
  (list
    ;; Basic curved quoting
    (format-message "`hello'")
    (format-message "`%s'" "world")
    (format-message "Use `%s' to %s" "M-x" "execute commands")

    ;; Multiple quoted segments
    (format-message "`%s' and `%s' are different" "car" "cdr")
    (format-message "Try `%s', `%s', or `%s'" "a" "b" "c")

    ;; Quoting with various format specifiers
    (format-message "`%d' is a number" 42)
    (format-message "`%S' is the printed form" '(1 2 3))

    ;; No backtick-quote pair -- should be unchanged
    (format-message "plain text with %s" "no quotes")
    (format-message "backtick ` alone")
    (format-message "quote ' alone")

    ;; Mixed: some quoted, some not
    (format-message "The function `%s' takes %d args" "cons" 2)
    (format-message "Set `%s' to %S for `%s'" "x" 42 "feature")))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"‘hello’\" \"‘world’\" \"Use ‘M-x’ to execute commands\" \"‘car’ and ‘cdr’ are different\" \"Try ‘a’, ‘b’, or ‘c’\" \"‘42’ is a number\" \"‘(1 2 3)’ is the printed form\" \"plain text with no quotes\" \"backtick ‘ alone\" \"quote ’ alone\" \"The function ‘cons’ takes 2 args\" \"Set ‘x’ to 42 for ‘feature’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_format_message_quoting_style_branches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `format-message` uses `text-quoting-style` from src/doc.c and
    // the quote-rewriting branch in src/editfns.c `styled_format`.
    let form = r#"
(let ((quoted (string 96 120 39)))
  (list
   (let ((text-quoting-style 'grave))
     (list (text-quoting-style) (format-message quoted)))
   (let ((text-quoting-style 'straight))
     (list (text-quoting-style) (format-message quoted)))
   (let ((text-quoting-style 'curve))
     (list (text-quoting-style) (format-message quoted)))
   (let ((text-quoting-style 'unknown-non-nil-style))
     (list (text-quoting-style) (format-message quoted)))
   (let ((text-quoting-style 'curve)
         (fmt (concat (string 96) "%s" (string 39))))
     (let ((s (format-message (propertize fmt 'face 'italic)
                              (propertize "abc" 'face 'bold))))
       (list s
             (mapcar (lambda (i) (text-properties-at i s))
                     (number-sequence 0 (1- (length s)))))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((grave \"`x'\") (straight \"'x'\") (curve \"‘x’\") (curve \"‘x’\") (#(\"‘abc’\" 0 1 (face italic) 1 4 (face bold) 4 5 (face italic)) ((face italic) (face bold) (face bold) (face bold) (face italic))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Message-based logging system with levels and formatting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_message_logging_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a structured logging system using message/format-message that
    // collects log entries, filters by level, and formats output.
    let form = r#"
(progn
  (defvar neovm--fmp-log-buffer nil)
  (defvar neovm--fmp-log-level 0)

  (fset 'neovm--fmp-level-num
    (lambda (level)
      (cond ((eq level 'debug) 0)
            ((eq level 'info) 1)
            ((eq level 'warn) 2)
            ((eq level 'error) 3)
            (t 0))))

  (fset 'neovm--fmp-log
    (lambda (level fmt &rest args)
      (when (>= (funcall 'neovm--fmp-level-num level)
                neovm--fmp-log-level)
        (let ((entry (list level (apply 'format-message fmt args))))
          (setq neovm--fmp-log-buffer
                (append neovm--fmp-log-buffer (list entry)))
          (cadr entry)))))

  (fset 'neovm--fmp-get-logs
    (lambda (&optional min-level)
      (let ((min-num (if min-level
                         (funcall 'neovm--fmp-level-num min-level)
                       0)))
        (let ((result nil))
          (dolist (entry neovm--fmp-log-buffer)
            (when (>= (funcall 'neovm--fmp-level-num (car entry)) min-num)
              (setq result (cons entry result))))
          (nreverse result)))))

  (fset 'neovm--fmp-format-logs
    (lambda (entries)
      (mapcar (lambda (e)
                (format-message "[%-5s] %s"
                                (upcase (symbol-name (car e)))
                                (cadr e)))
              entries)))

  (unwind-protect
      (progn
        (setq neovm--fmp-log-buffer nil)
        (setq neovm--fmp-log-level 0)

        ;; Generate various log entries
        (funcall 'neovm--fmp-log 'debug "initializing %s v%s" "app" "1.0")
        (funcall 'neovm--fmp-log 'info "server started on port %d" 8080)
        (funcall 'neovm--fmp-log 'debug "loading config from `%s'" "/etc/app.conf")
        (funcall 'neovm--fmp-log 'info "connected to %s:%d" "db.local" 5432)
        (funcall 'neovm--fmp-log 'warn "slow query: %dms" 1523)
        (funcall 'neovm--fmp-log 'error "failed to %s: %s" "authenticate" "invalid token")
        (funcall 'neovm--fmp-log 'info "request: %s %s -> %d" "GET" "/api/users" 200)
        (funcall 'neovm--fmp-log 'warn "rate limit: %d/%d requests" 95 100)

        (list
          ;; All logs
          (length neovm--fmp-log-buffer)
          ;; Filter by level
          (length (funcall 'neovm--fmp-get-logs 'info))
          (length (funcall 'neovm--fmp-get-logs 'warn))
          (length (funcall 'neovm--fmp-get-logs 'error))
          ;; Formatted output for warnings and above
          (funcall 'neovm--fmp-format-logs
                   (funcall 'neovm--fmp-get-logs 'warn))
          ;; All formatted
          (funcall 'neovm--fmp-format-logs neovm--fmp-log-buffer)))
    (fmakunbound 'neovm--fmp-level-num)
    (fmakunbound 'neovm--fmp-log)
    (fmakunbound 'neovm--fmp-get-logs)
    (fmakunbound 'neovm--fmp-format-logs)
    (makunbound 'neovm--fmp-log-buffer)
    (makunbound 'neovm--fmp-log-level)))
"#;
    let expect = expect_test::expect![[
        r#""OK (8 6 3 1 (\"[WARN ] slow query: 1523ms\" \"[ERROR] failed to authenticate: invalid token\" \"[WARN ] rate limit: 95/100 requests\") (\"[DEBUG] initializing app v1.0\" \"[INFO ] server started on port 8080\" \"[DEBUG] loading config from ‘/etc/app.conf’\" \"[INFO ] connected to db.local:5432\" \"[WARN ] slow query: 1523ms\" \"[ERROR] failed to authenticate: invalid token\" \"[INFO ] request: GET /api/users -> 200\" \"[WARN ] rate limit: 95/100 requests\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format-message with numeric formatting and data presentation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_message_numeric_presentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Advanced numeric formatting through format-message: percentages,
    // progress bars, aligned columns, hex dumps, and summary statistics.
    let form = r#"
(progn
  (fset 'neovm--fmp-progress-bar
    (lambda (current total width)
      (let* ((pct (/ (* 100 current) total))
             (filled (/ (* width current) total))
             (empty (- width filled)))
        (format-message "[%s%s] %3d%%"
                        (make-string filled ?#)
                        (make-string empty ?.)
                        pct))))

  (fset 'neovm--fmp-hex-dump
    (lambda (bytes)
      (let ((parts nil))
        (dolist (b bytes)
          (setq parts (cons (format "%02X" b) parts)))
        (mapconcat #'identity (nreverse parts) " "))))

  (fset 'neovm--fmp-summary-stats
    (lambda (label numbers)
      (let* ((n (length numbers))
             (sum (apply '+ numbers))
             (mean (/ (float sum) n))
             (sorted (sort (copy-sequence numbers) '<))
             (mn (car sorted))
             (mx (car (last sorted))))
        (format-message "%-10s n=%-4d sum=%-8d mean=%6.1f min=%-6d max=%-6d"
                        label n sum mean mn mx))))

  (unwind-protect
      (list
        ;; Progress bars at various stages
        (funcall 'neovm--fmp-progress-bar 0 100 20)
        (funcall 'neovm--fmp-progress-bar 25 100 20)
        (funcall 'neovm--fmp-progress-bar 50 100 20)
        (funcall 'neovm--fmp-progress-bar 75 100 20)
        (funcall 'neovm--fmp-progress-bar 100 100 20)

        ;; Hex dumps
        (funcall 'neovm--fmp-hex-dump '(0 127 255 16 32 64))
        (funcall 'neovm--fmp-hex-dump '(72 101 108 108 111))

        ;; Summary statistics
        (funcall 'neovm--fmp-summary-stats "scores"
                 '(85 92 78 95 88 76 91 83 97 72))
        (funcall 'neovm--fmp-summary-stats "latency"
                 '(12 15 8 23 45 11 9 14 17 22))

        ;; Combined: format a mini-report
        (let ((data '(("Sales" 1200 1350 1100 1500 1400)
                      ("Costs" 800 850 900 950 1000)
                      ("Profit" 400 500 200 550 400))))
          (mapcar (lambda (row)
                    (format-message "%-8s | %s | avg=%d"
                                    (car row)
                                    (mapconcat (lambda (n) (format "%5d" n))
                                               (cdr row) " ")
                                    (/ (apply '+ (cdr row))
                                       (length (cdr row)))))
                  data)))
    (fmakunbound 'neovm--fmp-progress-bar)
    (fmakunbound 'neovm--fmp-hex-dump)
    (fmakunbound 'neovm--fmp-summary-stats)))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"[....................]   0%\" \"[#####...............]  25%\" \"[##########..........]  50%\" \"[###############.....]  75%\" \"[####################] 100%\" \"00 7F FF 10 20 40\" \"48 65 6C 6C 6F\" \"scores     n=10   sum=857      mean=  85.7 min=72     max=97    \" \"latency    n=10   sum=176      mean=  17.6 min=8      max=45    \" (\"Sales    |  1200  1350  1100  1500  1400 | avg=1310\" \"Costs    |   800   850   900   950  1000 | avg=900\" \"Profit   |   400   500   200   550   400 | avg=410\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// message with conditional formatting and error message patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_message_conditional_error_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Patterns commonly used in Emacs: conditional messages based on
    // plurality, error message formatting with context, and recursive
    // structure description.
    let form = r#"
(progn
  (fset 'neovm--fmp-pluralize
    (lambda (n singular plural)
      (if (= n 1)
          (format-message "%d %s" n singular)
        (format-message "%d %s" n plural))))

  (fset 'neovm--fmp-describe-value
    (lambda (val)
      (cond
        ((null val) (format-message "nil"))
        ((integerp val) (format-message "integer %d" val))
        ((floatp val) (format-message "float %.4f" val))
        ((stringp val) (format-message "string %S (length %d)" val (length val)))
        ((symbolp val) (format-message "symbol `%s'" (symbol-name val)))
        ((consp val) (format-message "cons (%s . %s)"
                                     (funcall 'neovm--fmp-describe-value (car val))
                                     (funcall 'neovm--fmp-describe-value (cdr val))))
        ((vectorp val) (format-message "vector[%d]" (length val)))
        (t (format-message "unknown: %S" val)))))

  (unwind-protect
      (list
        ;; Pluralization
        (funcall 'neovm--fmp-pluralize 0 "file" "files")
        (funcall 'neovm--fmp-pluralize 1 "file" "files")
        (funcall 'neovm--fmp-pluralize 5 "file" "files")
        (funcall 'neovm--fmp-pluralize 1 "match" "matches")
        (funcall 'neovm--fmp-pluralize 42 "match" "matches")

        ;; Value description
        (funcall 'neovm--fmp-describe-value nil)
        (funcall 'neovm--fmp-describe-value 42)
        (funcall 'neovm--fmp-describe-value 3.14)
        (funcall 'neovm--fmp-describe-value "hello")
        (funcall 'neovm--fmp-describe-value 'foo)
        (funcall 'neovm--fmp-describe-value '(1 . 2))
        (funcall 'neovm--fmp-describe-value '(a b c))
        (funcall 'neovm--fmp-describe-value [1 2 3])

        ;; Error message patterns
        (format-message "Wrong type argument: %s, %S"
                        "numberp" "hello")
        (format-message "Args out of range: %S, %d, %d"
                        "hello" 0 10)
        (format-message "Symbol's value as variable is void: `%s'"
                        "undefined-var"))
    (fmakunbound 'neovm--fmp-pluralize)
    (fmakunbound 'neovm--fmp-describe-value)))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"0 files\" \"1 file\" \"5 files\" \"1 match\" \"42 matches\" \"nil\" \"integer 42\" \"float 3.1400\" \"string \\\"hello\\\" (length 5)\" \"symbol ‘foo’\" \"cons (integer 1 . integer 2)\" \"cons (symbol ‘a’ . cons (symbol ‘b’ . cons (symbol ‘c’ . nil)))\" \"vector[3]\" \"Wrong type argument: numberp, \\\"hello\\\"\" \"Args out of range: \\\"hello\\\", 0, 10\" \"Symbol’s value as variable is void: ‘undefined-var’\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
