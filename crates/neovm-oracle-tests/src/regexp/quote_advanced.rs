//! Advanced oracle parity tests for `regexp-quote` and dynamic pattern building.
//!
//! Tests regexp-quote on all special metacharacters, dynamic search
//! pattern construction, looking-at vs looking-at-p differences,
//! re-search-backward, and complex regex-based extractors.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// regexp-quote on every regex metacharacter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_metacharacters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // regexp-quote must escape every special regex character so that
    // the resulting pattern matches the literal string.
    let form = r####"
(let ((specials '("." "*" "+" "?" "[" "]" "^" "$" "\\" "|"
                  "(" ")" "{" "}")))
  (mapcar (lambda (ch)
            (let ((quoted (regexp-quote ch)))
              (list ch quoted
                    ;; The quoted pattern should match the literal char
                    (if (string-match-p (concat "\\`" quoted "\\'") ch)
                        t nil))))
          specials))
"####;
    let expect = expect_test::expect![[
        r#""OK ((\".\" \"\\\\.\" t) (\"*\" \"\\\\*\" t) (\"+\" \"\\\\+\" t) (\"?\" \"\\\\?\" t) (\"[\" \"\\\\[\" t) (\"]\" \"]\" t) (\"^\" \"\\\\^\" t) (\"$\" \"\\\\$\" t) (\"\\\\\" \"\\\\\\\\\" t) (\"|\" \"|\" t) (\"(\" \"(\" t) (\")\" \")\" t) (\"{\" \"{\" t) (\"}\" \"}\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building dynamic search patterns with regexp-quote
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_dynamic_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build search patterns dynamically by quoting user input, ensuring
    // special characters in the search term don't break the regex.
    let form = r####"
(let ((search-terms '("hello" "foo.bar" "a+b" "price$" "[tag]"
                      "c:\\path" "x|y" "end?" "star*" "(group)")))
  (mapcar (lambda (term)
            (let* ((text (concat "prefix " term " suffix"))
                   (pattern (concat "\\b" (regexp-quote term) "\\b"))
                   ;; Also test: can we find the literal term?
                   (found-pos (string-match (regexp-quote term) text)))
              (list term found-pos)))
          search-terms))
"####;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" 7) (\"foo.bar\" 7) (\"a+b\" 7) (\"price$\" 7) (\"[tag]\" 7) (\"c:\\\\path\" 7) (\"x|y\" 7) (\"end?\" 7) (\"star*\" 7) (\"(group)\" 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// regexp-opt-like manual pattern construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_manual_regexp_opt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Manually build an optimized alternation pattern from a list of
    // literal strings, quoting each one, and test matching.
    let form = r####"
(progn
  (fset 'neovm--rq-make-alt-pattern
    (lambda (strings)
      (concat "\\(?:"
              (mapconcat #'regexp-quote strings "\\|")
              "\\)")))

  (unwind-protect
      (let* ((keywords '("if" "then" "else" "while" "for" "return"))
             (pattern (funcall 'neovm--rq-make-alt-pattern keywords))
             (texts '("if x > 0 then return x"
                      "while running for office"
                      "otherwise do nothing"
                      "for each element")))
        (mapcar (lambda (text)
                  (let ((pos 0) (matches nil))
                    (while (string-match pattern text pos)
                      (setq matches (cons (match-string 0 text) matches)
                            pos (match-end 0)))
                    (list text (nreverse matches))))
                texts))
    (fmakunbound 'neovm--rq-make-alt-pattern)))
"####;
    let expect = expect_test::expect![[
        r#""OK ((\"if x > 0 then return x\" (\"if\" \"then\" \"return\")) (\"while running for office\" (\"while\" \"for\")) (\"otherwise do nothing\" nil) (\"for each element\" (\"for\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at vs looking-at-p differences (match-data side effects)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_vs_looking_at_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // looking-at sets match-data, looking-at-p does NOT.
    // Verify this difference explicitly.
    let form = r####"
(with-temp-buffer
  (insert "hello world 123")
  (goto-char (point-min))
  ;; First, clear match data by doing a match we know about
  (string-match "zzz" "zzz")
  (let ((md-before (match-data)))
    ;; looking-at-p should NOT change match-data
    (let ((result-p (looking-at-p "\\([a-z]+\\)")))
      (let ((md-after-p (match-data)))
        ;; Now use looking-at which SHOULD change match-data
        (let ((result (looking-at "\\([a-z]+\\) \\([a-z]+\\)")))
          (let ((md-after (match-data)))
            (list
              ;; Both should find a match
              (if result-p t nil)
              (if result t nil)
              ;; match-data unchanged after looking-at-p
              (equal md-before md-after-p)
              ;; match-data changed after looking-at
              (not (equal md-before md-after))
              ;; looking-at captured groups
              (match-string 1)
              (match-string 2))))))))
"####;
    let expect = expect_test::expect![[r#""OK (t t t t \"hello\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-backward with all parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_re_search_backward_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test re-search-backward with BOUND, NOERROR, and COUNT parameters.
    let form = r####"
(with-temp-buffer
  (insert "aaa bbb aaa ccc aaa ddd aaa")
  (let ((results nil))
    ;; Search backward from end, find last "aaa"
    (goto-char (point-max))
    (setq results (cons (re-search-backward "aaa" nil t) results))
    (setq results (cons (point) results))

    ;; Search backward again from current point, find previous "aaa"
    (goto-char (1- (point)))
    (setq results (cons (re-search-backward "aaa" nil t) results))
    (setq results (cons (point) results))

    ;; Search backward with BOUND: don't go before position 10
    (goto-char (point-max))
    (setq results (cons (re-search-backward "aaa" 10 t) results))
    (setq results (cons (point) results))

    ;; Search backward with COUNT=2: find 2nd occurrence backward
    (goto-char (point-max))
    (setq results (cons (re-search-backward "aaa" nil t 2) results))
    (setq results (cons (point) results))

    ;; Search for non-existent pattern with NOERROR=t: returns nil
    (goto-char (point-max))
    (setq results (cons (re-search-backward "zzz" nil t) results))
    (setq results (cons (point) results))

    (nreverse results)))
"####;
    let expect = expect_test::expect![[r#""OK (25 25 17 17 25 25 17 17 nil 28)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based code syntax highlighter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_syntax_highlighter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize a simple expression language using regex patterns.
    // Classify each token as keyword, number, string, operator, or identifier.
    let form = r####"
(progn
  (fset 'neovm--rq-tokenize
    (lambda (code)
      (let ((tokens nil)
            (pos 0)
            (keywords '("if" "else" "while" "return" "let" "fn"))
            (patterns
             (list (cons "\\`[ \t\n]+" 'skip)
                   (cons "\\`[0-9]+\\(?:\\.[0-9]+\\)?" 'number)
                   (cons "\\`\"[^\"]*\"" 'string)
                   (cons "\\`\\(?:==\\|!=\\|<=\\|>=\\|&&\\|||\\|->\\)" 'operator)
                   (cons "\\`[-+*/=<>!&|(){}\\[\\];,.]" 'operator)
                   (cons "\\`[a-zA-Z_][a-zA-Z0-9_]*" 'identifier))))
        (while (< pos (length code))
          (let ((rest (substring code pos))
                (matched nil))
            (dolist (pat patterns)
              (unless matched
                (when (string-match (car pat) rest)
                  (let ((text (match-string 0 rest))
                        (kind (cdr pat)))
                    (setq pos (+ pos (length text)))
                    (unless (eq kind 'skip)
                      (when (eq kind 'identifier)
                        (when (member text keywords)
                          (setq kind 'keyword)))
                      (setq tokens (cons (list kind text) tokens)))
                    (setq matched t)))))
            (unless matched
              ;; skip unknown character
              (setq pos (1+ pos)))))
        (nreverse tokens))))

  (unwind-protect
      (funcall 'neovm--rq-tokenize
               "let x = 42; if x >= 10 && x != 0 { return x + 1; }")
    (fmakunbound 'neovm--rq-tokenize)))
"####;
    let expect = expect_test::expect![[
        r#""OK ((keyword \"let\") (identifier \"x\") (number \"42\") (keyword \"if\") (identifier \"x\") (operator \">=\") (number \"10\") (operator \"&&\") (identifier \"x\") (operator \"!=\") (number \"0\") (keyword \"return\") (identifier \"x\") (number \"1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based structured data extractor
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regex_structured_data_extractor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a multi-line "config file" format using regex patterns.
    // Format: "key = value" with [section] headers and # comments.
    let form = r####"
(progn
  (fset 'neovm--rq-parse-config
    (lambda (text)
      (let ((lines (split-string text "\n"))
            (current-section "default")
            (result nil))
        (dolist (line lines)
          ;; Strip leading/trailing whitespace
          (let ((trimmed (replace-regexp-in-string
                          "\\`[ \t]+" ""
                          (replace-regexp-in-string "[ \t]+\\'" "" line))))
            (cond
              ;; Skip empty lines and comments
              ((or (string-equal trimmed "")
                   (string-match-p "\\`#" trimmed))
               nil)
              ;; Section header: [name]
              ((string-match "\\`\\[\\([^]]+\\)\\]\\'" trimmed)
               (setq current-section (match-string 1 trimmed)))
              ;; Key = value pair
              ((string-match "\\`\\([^ \t=]+\\)[ \t]*=[ \t]*\\(.*\\)\\'" trimmed)
               (let ((key (match-string 1 trimmed))
                     (val (match-string 2 trimmed)))
                 (setq result (cons (list current-section key val) result)))))))
        (nreverse result))))

  (unwind-protect
      (funcall 'neovm--rq-parse-config
               "# Database config\n[database]\nhost = localhost\nport = 5432\nname = mydb\n\n# Server config\n[server]\nport = 8080\nworkers = 4\ndebug = true\n\n[logging]\nlevel = info\nfile = /var/log/app.log")
    (fmakunbound 'neovm--rq-parse-config)))
"####;
    let expect = expect_test::expect![[
        r#""OK ((\"database\" \"host\" \"localhost\") (\"database\" \"port\" \"5432\") (\"database\" \"name\" \"mydb\") (\"server\" \"port\" \"8080\") (\"server\" \"workers\" \"4\") (\"server\" \"debug\" \"true\") (\"logging\" \"level\" \"info\") (\"logging\" \"file\" \"/var/log/app.log\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
