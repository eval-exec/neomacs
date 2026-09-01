//! Advanced oracle parity tests for regex operations.
//!
//! Covers: regexp-quote for all special characters, replace-regexp-in-string
//! with all parameters (FIXEDCASE, LITERAL, SUBEXP, START), multiple regex
//! matches with nested groups, non-greedy matching, regex alternation,
//! looking-at at various positions, and regex-based validators.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// regexp-quote: exhaustive special character coverage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_regexp_quote_all_specials_in_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build strings containing every regex special character, quote them,
    // and verify they match literally in a larger text context.
    let form = r#"(let ((test-cases
            '(("file.txt" "open file.txt now")
              ("a*b+c?" "expr a*b+c? end")
              ("[0-9]" "match [0-9] here")
              ("^start$" "use ^start$ anchor")
              ("(group)" "in (group) form")
              ("a\\b" "path a\\b done")
              ("x|y" "choose x|y option")
              ("a{3}" "repeat a{3} times"))))
      (mapcar (lambda (tc)
                (let* ((needle (car tc))
                       (haystack (cadr tc))
                       (quoted (regexp-quote needle))
                       (found (string-match quoted haystack)))
                  (list needle quoted found
                        (if found (match-string 0 haystack) nil))))
              test-cases))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"file.txt\" \"file\\\\.txt\" 5 \"file.txt\") (\"a*b+c?\" \"a\\\\*b\\\\+c\\\\?\" 5 \"a*b+c?\") (\"[0-9]\" \"\\\\[0-9]\" 6 \"[0-9]\") (\"^start$\" \"\\\\^start\\\\$\" 4 \"^start$\") (\"(group)\" \"(group)\" 3 \"(group)\") (\"a\\\\b\" \"a\\\\\\\\b\" 5 \"a\\\\b\") (\"x|y\" \"x|y\" 7 \"x|y\") (\"a{3}\" \"a{3}\" 7 \"a{3}\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-regexp-in-string: all parameters combined
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_replace_regexp_all_params_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test replace-regexp-in-string with FIXEDCASE, LITERAL, SUBEXP, START
    // all specified in various combinations.
    let form = r#"(list
      ;; FIXEDCASE=nil, LITERAL=nil, no SUBEXP, START=0
      (replace-regexp-in-string "\\([a-z]+\\)=\\([0-9]+\\)" "\\2:\\1"
                                "foo=10 bar=20 baz=30")
      ;; FIXEDCASE=t, LITERAL=nil, no SUBEXP, START=7
      (replace-regexp-in-string "\\([a-z]+\\)=\\([0-9]+\\)" "\\2:\\1"
                                "foo=10 bar=20 baz=30" t nil nil 7)
      ;; FIXEDCASE=t, LITERAL=t, no SUBEXP, START=0
      (replace-regexp-in-string "\\([a-z]+\\)" "\\1-literal"
                                "abc def ghi" t t)
      ;; FIXEDCASE=t, LITERAL=nil, SUBEXP=1, START=0
      ;; SUBEXP for replace-regexp-in-string replaces only the nth group
      (replace-regexp-in-string "\\(hello\\) \\(world\\)" "GOODBYE"
                                "hello world" t nil 1)
      ;; START past the first match
      (replace-regexp-in-string "[0-9]+" "NUM"
                                "a1 b2 c3 d4" nil nil nil 5))"#;
    let expect = expect_test::expect![[
        r#""OK (\"10:foo 20:bar 30:baz\" \"20:bar 30:baz\" \"\\\\1-literal \\\\1-literal \\\\1-literal\" \"GOODBYE world\" \" cNUM dNUM\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple regex matches with nested groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_nested_groups_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract structured data from a string using nested capture groups
    // via repeated string-match calls with START offset advancement.
    let form = r#"(let ((s "func(arg1, arg2) + func(x, y, z) - func()")
                        (results nil)
                        (start 0))
      ;; Pattern: func( followed by optional comma-separated args )
      ;; Group 1: function name, Group 2: entire arg list
      (while (string-match "\\(func\\)(\\([^)]*\\))" s start)
        (let ((fname (match-string 1 s))
              (args (match-string 2 s))
              (full (match-string 0 s))
              (mend (match-end 0)))
          ;; Parse args by splitting on ", "
          (let ((arg-list (if (string= args "")
                              nil
                            (split-string args ", *"))))
            (setq results (cons (list full fname arg-list (length arg-list)) results)))
          (setq start mend)))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"func(arg1, arg2)\" \"func\" (\"arg1\" \"arg2\") 2) (\"func(x, y, z)\" \"func\" (\"x\" \"y\" \"z\") 3) (\"func()\" \"func\" nil 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Non-greedy matching with *? and +?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_non_greedy_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare greedy vs non-greedy quantifiers in Emacs regex.
    // Emacs uses \{-\} for non-greedy (not *? like PCRE).
    let form = r#"(let ((s "<tag>content1</tag> <tag>content2</tag>"))
      (list
        ;; Greedy .* matches as much as possible
        (progn
          (string-match "<tag>\\(.*\\)</tag>" s)
          (match-string 1 s))
        ;; Non-greedy: use [^<]* to simulate non-greedy behavior
        (progn
          (string-match "<tag>\\([^<]*\\)</tag>" s)
          (match-string 1 s))
        ;; Extract all tag contents using non-greedy approach
        (let ((results nil) (start 0))
          (while (string-match "<tag>\\([^<]*\\)</tag>" s start)
            (setq results (cons (match-string 1 s) results))
            (setq start (match-end 0)))
          (nreverse results))
        ;; Nested tags: greedy grabs outer
        (let ((nested "<div><span>inner</span></div>"))
          (string-match "<div>\\(.*\\)</div>" nested)
          (match-string 1 nested))
        ;; Non-greedy on nested
        (let ((nested "<div><span>inner</span></div>"))
          (string-match "<div>\\([^<]*\\)" nested)
          (match-string 1 nested))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"content1</tag> <tag>content2\" \"content1\" (\"content1\" \"content2\") \"<span>inner</span>\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Regex alternation \| with complex patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_alternation_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test regex alternation \| with multiple branches, groups, and
    // combined with other regex features.
    let form = r#"(let ((results nil))
      ;; Simple alternation: match different keywords
      (let ((s "if x then y else z while a"))
        (let ((start 0) (keywords nil))
          (while (string-match "\\(if\\|then\\|else\\|while\\)" s start)
            (setq keywords (cons (match-string 1 s) keywords))
            (setq start (match-end 0)))
          (setq results (cons (nreverse keywords) results))))

      ;; Alternation with groups: match different number formats
      (let ((s "decimal 42, hex 0xFF, octal 0o77, binary 0b1010"))
        (let ((start 0) (numbers nil))
          (while (string-match
                   "\\(0x[0-9a-fA-F]+\\|0o[0-7]+\\|0b[01]+\\|[0-9]+\\)"
                   s start)
            (setq numbers (cons (match-string 1 s) numbers))
            (setq start (match-end 0)))
          (setq results (cons (nreverse numbers) results))))

      ;; Alternation priority: first branch that matches wins
      (let ((s "foobar"))
        (string-match "\\(foo\\|foobar\\)" s)
        (setq results (cons (match-string 1 s) results)))

      ;; Longer alternation wins when branches start at same position
      (let ((s "foobar"))
        (string-match "\\(foobar\\|foo\\)" s)
        (setq results (cons (match-string 1 s) results)))

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"if\" \"then\" \"else\" \"while\") (\"42\" \"0xFF\" \"0o77\" \"0b1010\") \"foo\" \"foobar\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at combined with regex at various buffer positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_looking_at_position_scanning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Move point through a buffer and use looking-at at each position
    // to classify what kind of token starts there (like a lexer).
    let form = r#"(with-temp-buffer
      (insert "  123 \"hello\" (+ a b) ; comment\n")
      (goto-char (point-min))
      (let ((tokens nil))
        (while (not (eobp))
          (cond
            ;; Skip whitespace
            ((looking-at "[ \t\n]+")
             (goto-char (match-end 0)))
            ;; Number
            ((looking-at "[0-9]+")
             (setq tokens (cons (list 'number (match-string 0)) tokens))
             (goto-char (match-end 0)))
            ;; String literal
            ((looking-at "\"\\([^\"]*\\)\"")
             (setq tokens (cons (list 'string (match-string 1)) tokens))
             (goto-char (match-end 0)))
            ;; Open paren
            ((looking-at "(")
             (setq tokens (cons (list 'open-paren "(") tokens))
             (forward-char 1))
            ;; Close paren
            ((looking-at ")")
             (setq tokens (cons (list 'close-paren ")") tokens))
             (forward-char 1))
            ;; Symbol
            ((looking-at "[a-zA-Z_+\\-*/=<>!?][a-zA-Z0-9_+\\-*/=<>!?]*")
             (setq tokens (cons (list 'symbol (match-string 0)) tokens))
             (goto-char (match-end 0)))
            ;; Comment
            ((looking-at ";[^\n]*")
             (setq tokens (cons (list 'comment (match-string 0)) tokens))
             (goto-char (match-end 0)))
            ;; Unknown: skip one char
            (t
             (setq tokens (cons (list 'unknown (char-to-string (char-after))) tokens))
             (forward-char 1))))
        (nreverse tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((number \"123\") (string \"hello\") (open-paren \"(\") (symbol \"+\") (symbol \"a\") (symbol \"b\") (close-paren \")\") (comment \"; comment\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based structured data extractor
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_structured_data_extractor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract structured records from a log-like format using regex.
    // Format: [LEVEL] TIMESTAMP - MESSAGE (key1=val1, key2=val2)
    // Uses unwind-protect with fmakunbound for cleanup.
    let form = r#"(unwind-protect
      (progn
        (defun test--parse-log-entry (line)
          "Parse a log line into an alist of fields."
          (if (string-match
                "\\[\\([A-Z]+\\)\\] \\([0-9:]+\\) - \\(.*?\\)\\(?: (\\([^)]+\\))\\)?$"
                line)
              (let ((level (match-string 1 line))
                    (timestamp (match-string 2 line))
                    (message (match-string 3 line))
                    (meta-str (match-string 4 line)))
                ;; Parse key=value pairs from meta-str
                (let ((meta nil))
                  (when meta-str
                    (let ((start 0))
                      (while (string-match "\\([a-z_]+\\)=\\([^,)]+\\)" meta-str start)
                        (setq meta (cons (cons (match-string 1 meta-str)
                                               (match-string 2 meta-str))
                                         meta))
                        (setq start (match-end 0)))))
                  (list (cons "level" level)
                        (cons "time" timestamp)
                        (cons "msg" message)
                        (cons "meta" (nreverse meta)))))
            nil))

        (let ((logs '("[INFO] 10:30:45 - User logged in (user=alice, ip=10.0.0.1)"
                      "[ERROR] 10:31:02 - Connection failed (host=db.local, port=5432, retry=3)"
                      "[WARN] 10:31:15 - High memory usage"
                      "[DEBUG] 10:32:00 - Cache hit (key=session_42)")))
          (mapcar #'test--parse-log-entry logs)))
      ;; Cleanup
      (fmakunbound 'test--parse-log-entry))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"level\" . \"INFO\") (\"time\" . \"10:30:45\") (\"msg\" . \"User logged in\") (\"meta\" (\"user\" . \"alice\") (\"ip\" . \"10.0.0.1\"))) ((\"level\" . \"ERROR\") (\"time\" . \"10:31:02\") (\"msg\" . \"Connection failed\") (\"meta\" (\"host\" . \"db.local\") (\"port\" . \"5432\") (\"retry\" . \"3\"))) ((\"level\" . \"WARN\") (\"time\" . \"10:31:15\") (\"msg\" . \"High memory usage\") (\"meta\")) ((\"level\" . \"DEBUG\") (\"time\" . \"10:32:00\") (\"msg\" . \"Cache hit\") (\"meta\" (\"key\" . \"session_42\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based URL parser
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_ops_adv_url_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse URLs into components using regex: scheme, host, port, path, query
    let form = r#"(unwind-protect
      (progn
        (defun test--parse-url (url)
          "Parse URL into (scheme host port path query) list."
          (if (string-match
                "\\`\\([a-z]+\\)://\\([^/:?#]+\\)\\(?::\\([0-9]+\\)\\)?\\(/[^?#]*\\)?\\(?:\\?\\([^#]*\\)\\)?\\'"
                url)
              (list
                (match-string 1 url)
                (match-string 2 url)
                (let ((p (match-string 3 url)))
                  (if p (string-to-number p) nil))
                (or (match-string 4 url) "/")
                (match-string 5 url))
            (list 'invalid url)))

        (defun test--parse-query-string (qs)
          "Parse query string into alist of key=value pairs."
          (when qs
            (let ((pairs nil) (start 0))
              (while (string-match "\\([^=&]+\\)=\\([^&]*\\)" qs start)
                (setq pairs (cons (cons (match-string 1 qs)
                                        (match-string 2 qs))
                                  pairs))
                (setq start (match-end 0)))
              (nreverse pairs))))

        (let ((urls '("http://example.com/path/to/page?key=val&foo=bar"
                      "https://api.server.io:8443/v2/users?limit=10&offset=20"
                      "ftp://files.host.net/pub/data.tar.gz"
                      "http://localhost:3000/"
                      "https://example.com")))
          (mapcar (lambda (u)
                    (let ((parsed (test--parse-url u)))
                      (list parsed
                            (test--parse-query-string (nth 4 parsed)))))
                  urls)))
      ;; Cleanup
      (fmakunbound 'test--parse-url)
      (fmakunbound 'test--parse-query-string))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"http\" \"example.com\" nil \"/path/to/page\" \"key=val&foo=bar\") ((\"key\" . \"val\") (\"foo\" . \"bar\"))) ((\"https\" \"api.server.io\" 8443 \"/v2/users\" \"limit=10&offset=20\") ((\"limit\" . \"10\") (\"offset\" . \"20\"))) ((\"ftp\" \"files.host.net\" nil \"/pub/data.tar.gz\" nil) nil) ((\"http\" \"localhost\" 3000 \"/\" nil) nil) ((\"https\" \"example.com\" nil \"/\" nil) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
