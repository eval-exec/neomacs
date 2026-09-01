//! Advanced oracle parity tests for `match-string` usage patterns.
//!
//! Covers: numbered groups (0, 1, 2, ...), match-string after string-match
//! vs after re-search-forward in buffers, multiple consecutive regex matches
//! accessing different groups, optional groups that may not participate,
//! and complex structured text parsing (dates, URLs, key=value pairs).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// match-string with numbered groups after string-match
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_numbered_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Match a date pattern with 3 capture groups, verify all group indices
    // including group 0 (whole match), group 1, 2, 3
    let form = r#"(progn
  (let ((s "Today is 2026-03-02 and tomorrow is 2026-03-03"))
    (string-match "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)" s)
    (list
      (match-string 0 s)
      (match-string 1 s)
      (match-string 2 s)
      (match-string 3 s)
      (match-beginning 0)
      (match-end 0)
      (match-beginning 1)
      (match-end 1)
      (match-beginning 2)
      (match-end 2)
      (match-beginning 3)
      (match-end 3))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2026-03-02\" \"2026\" \"03\" \"02\" 9 19 9 13 14 16 17 19)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-string after string-match vs re-search-forward in a buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_string_match_vs_buffer_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare match-string results when using string-match on a string
    // vs re-search-forward in a temp buffer with the same content
    let form = r#"(progn
  (fset 'neovm--test-extract-via-string-match
    (lambda (re s)
      (when (string-match re s)
        (let ((groups nil)
              (i 0))
          (while (<= i 3)
            (setq groups (cons (match-string i s) groups))
            (setq i (1+ i)))
          (nreverse groups)))))
  (fset 'neovm--test-extract-via-buffer-search
    (lambda (re s)
      (with-temp-buffer
        (insert s)
        (goto-char (point-min))
        (when (re-search-forward re nil t)
          (let ((groups nil)
                (i 0))
            (while (<= i 3)
              (setq groups (cons (match-string i) groups))
              (setq i (1+ i)))
            (nreverse groups))))))
  (unwind-protect
      (let ((re "\\(\\w+\\)@\\(\\w+\\)\\.\\(\\w+\\)")
            (text "Contact: user@example.com for info"))
        (list
          (funcall 'neovm--test-extract-via-string-match re text)
          (funcall 'neovm--test-extract-via-buffer-search re text)))
    (fmakunbound 'neovm--test-extract-via-string-match)
    (fmakunbound 'neovm--test-extract-via-buffer-search)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"user@example.com\" \"user\" \"example\" \"com\") (\"user@example.com\" \"user\" \"example\" \"com\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple consecutive regex matches, each accessing different groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_consecutive_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Perform multiple string-match calls sequentially on the same string,
    // each time advancing the start position, collecting different group data
    let form = r#"(progn
  (fset 'neovm--test-find-all-matches
    (lambda (re s)
      (let ((results nil)
            (start 0))
        (while (string-match re s start)
          (let ((whole (match-string 0 s))
                (g1 (match-string 1 s))
                (g2 (match-string 2 s)))
            (setq results (cons (list whole g1 g2) results))
            (setq start (match-end 0))))
        (nreverse results))))
  (unwind-protect
      (list
        ;; Find all key=value pairs
        (funcall 'neovm--test-find-all-matches
                 "\\(\\w+\\)=\\(\\w+\\)"
                 "name=alice age=30 city=tokyo role=admin")
        ;; Find all time patterns HH:MM
        (funcall 'neovm--test-find-all-matches
                 "\\([0-2][0-9]\\):\\([0-5][0-9]\\)"
                 "Wake 06:30, lunch 12:00, sleep 23:45"))
    (fmakunbound 'neovm--test-find-all-matches)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"name=alice\" \"name\" \"alice\") (\"age=30\" \"age\" \"30\") (\"city=tokyo\" \"city\" \"tokyo\") (\"role=admin\" \"role\" \"admin\")) ((\"06:30\" \"06\" \"30\") (\"12:00\" \"12\" \"00\") (\"23:45\" \"23\" \"45\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-string with optional groups (group might not have participated)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_optional_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use \\? to make groups optional; match-string returns nil for
    // groups that did not participate in the match
    let form = r#"(progn
  (let ((results nil))
    ;; Pattern: word optionally followed by (parenthesized-qualifier)
    ;; Group 1 = word, Group 2 = qualifier (optional)
    (let ((re "\\(\\w+\\)\\(?:(\\(\\w+\\))\\)?"))
      ;; Case 1: both groups match
      (string-match re "hello(world)")
      (setq results (cons (list (match-string 1 "hello(world)")
                                (match-string 2 "hello(world)"))
                          results))
      ;; Case 2: only group 1 matches (no parenthesized part)
      (string-match re "hello")
      (setq results (cons (list (match-string 1 "hello")
                                (match-string 2 "hello"))
                          results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK ((\"hello\" \"world\") (\"hello\" nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse structured date strings using match-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_parse_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse dates in multiple formats and normalize them
    let form = r#"(progn
  (fset 'neovm--test-parse-date
    (lambda (s)
      (cond
        ;; ISO format: YYYY-MM-DD
        ((string-match "\\`\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)\\'" s)
         (list 'iso
               (string-to-number (match-string 1 s))
               (string-to-number (match-string 2 s))
               (string-to-number (match-string 3 s))))
        ;; US format: MM/DD/YYYY
        ((string-match "\\`\\([0-9]\\{1,2\\}\\)/\\([0-9]\\{1,2\\}\\)/\\([0-9]\\{4\\}\\)\\'" s)
         (list 'us
               (string-to-number (match-string 3 s))
               (string-to-number (match-string 1 s))
               (string-to-number (match-string 2 s))))
        ;; European format: DD.MM.YYYY
        ((string-match "\\`\\([0-9]\\{1,2\\}\\)\\.\\([0-9]\\{1,2\\}\\)\\.\\([0-9]\\{4\\}\\)\\'" s)
         (list 'eu
               (string-to-number (match-string 3 s))
               (string-to-number (match-string 2 s))
               (string-to-number (match-string 1 s))))
        (t (list 'unknown s)))))
  (unwind-protect
      (list
        (funcall 'neovm--test-parse-date "2026-03-02")
        (funcall 'neovm--test-parse-date "3/2/2026")
        (funcall 'neovm--test-parse-date "02.03.2026")
        (funcall 'neovm--test-parse-date "12/25/2025")
        (funcall 'neovm--test-parse-date "not-a-date"))
    (fmakunbound 'neovm--test-parse-date)))"#;
    let expect = expect_test::expect![[
        r#""OK ((iso 2026 3 2) (us 2026 3 2) (eu 2026 3 2) (us 2025 12 25) (unknown \"not-a-date\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse key=value config lines with match-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_parse_config_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse configuration file lines of the form:
    //   key = "quoted value"
    //   key = bare_value
    //   # comment
    //   [section]
    let form = r#"(progn
  (fset 'neovm--test-parse-config-line
    (lambda (line)
      (cond
        ;; Comment line
        ((string-match "\\`[ \t]*#" line)
         (list 'comment))
        ;; Section header
        ((string-match "\\`[ \t]*\\[\\([^]]+\\)\\][ \t]*\\'" line)
         (list 'section (match-string 1 line)))
        ;; Quoted value assignment
        ((string-match "\\`[ \t]*\\([a-zA-Z_][a-zA-Z0-9_]*\\)[ \t]*=[ \t]*\"\\([^\"]*\\)\"[ \t]*\\'" line)
         (list 'assign (match-string 1 line) (match-string 2 line)))
        ;; Bare value assignment
        ((string-match "\\`[ \t]*\\([a-zA-Z_][a-zA-Z0-9_]*\\)[ \t]*=[ \t]*\\([^ \t\n]+\\)[ \t]*\\'" line)
         (list 'assign (match-string 1 line) (match-string 2 line)))
        ;; Empty or whitespace-only
        ((string-match "\\`[ \t]*\\'" line)
         (list 'empty))
        (t (list 'unknown line)))))
  (fset 'neovm--test-parse-config
    (lambda (text)
      (let ((lines (split-string text "\n"))
            (result nil))
        (dolist (line lines)
          (setq result (cons (funcall 'neovm--test-parse-config-line line) result)))
        (nreverse result))))
  (unwind-protect
      (funcall 'neovm--test-parse-config
               "[database]\nhost = \"localhost\"\nport = 5432\n# timeout in seconds\ntimeout = 30\n\n[logging]\nlevel = \"debug\"\nfile = \"/var/log/app.log\"")
    (fmakunbound 'neovm--test-parse-config-line)
    (fmakunbound 'neovm--test-parse-config)))"#;
    let expect = expect_test::expect![[
        r#""OK ((section \"database\") (assign \"host\" \"localhost\") (assign \"port\" \"5432\") (comment) (assign \"timeout\" \"30\") (empty) (section \"logging\") (assign \"level\" \"debug\") (assign \"file\" \"/var/log/app.log\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: extract and transform URL components with match-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_url_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse URLs into scheme, host, optional port, and path components
    let form = r#"(progn
  (fset 'neovm--test-parse-url
    (lambda (url)
      (cond
        ;; URL with port: scheme://host:port/path
        ((string-match "\\`\\([a-z]+\\)://\\([^/:]+\\):\\([0-9]+\\)\\(/[^ ]*\\)?\\'" url)
         (list 'url
               (match-string 1 url)
               (match-string 2 url)
               (string-to-number (match-string 3 url))
               (or (match-string 4 url) "/")))
        ;; URL without port: scheme://host/path
        ((string-match "\\`\\([a-z]+\\)://\\([^/:]+\\)\\(/[^ ]*\\)?\\'" url)
         (list 'url
               (match-string 1 url)
               (match-string 2 url)
               nil
               (or (match-string 3 url) "/")))
        (t (list 'invalid url)))))
  (unwind-protect
      (list
        (funcall 'neovm--test-parse-url "https://example.com:8080/api/v1/users")
        (funcall 'neovm--test-parse-url "http://localhost:3000/")
        (funcall 'neovm--test-parse-url "ftp://files.server.org/pub/data")
        (funcall 'neovm--test-parse-url "https://simple.host")
        (funcall 'neovm--test-parse-url "not-a-url"))
    (fmakunbound 'neovm--test-parse-url)))"#;
    let expect = expect_test::expect![[
        r#""OK ((url \"https\" \"example.com\" 8080 \"/api/v1/users\") (url \"http\" \"localhost\" 3000 \"/\") (url \"ftp\" \"files.server.org\" nil \"/pub/data\") (url \"https\" \"simple.host\" nil \"/\") (invalid \"not-a-url\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-string with deeply nested groups and backreferences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_nested_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested parenthetical groups: group numbering follows opening-paren order
    let form = r#"(progn
  (let ((results nil))
    ;; Pattern: ((word)-(word)) captures:
    ;;   group 1 = outer "word-word"
    ;;   group 2 = first word
    ;;   group 3 = second word
    (let ((re "\\(\\([a-z]+\\)-\\([a-z]+\\)\\)")
          (s "prefix hello-world suffix"))
      (string-match re s)
      (setq results
            (cons (list
                    (match-string 0 s)
                    (match-string 1 s)
                    (match-string 2 s)
                    (match-string 3 s))
                  results)))
    ;; More nesting: ((a)(b(c)))
    ;; group 1 = whole, group 2 = a part, group 3 = b+c part, group 4 = c part
    (let ((re "\\(\\([0-9]+\\):\\(\\([a-z]+\\)\\)\\)")
          (s "data 42:hello end"))
      (string-match re s)
      (setq results
            (cons (list
                    (match-string 0 s)
                    (match-string 1 s)
                    (match-string 2 s)
                    (match-string 3 s)
                    (match-string 4 s))
                  results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello-world\" \"hello-world\" \"hello\" \"world\") (\"42:hello\" \"42:hello\" \"42\" \"hello\" \"hello\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
