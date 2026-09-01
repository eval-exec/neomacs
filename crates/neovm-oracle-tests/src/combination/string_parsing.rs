//! Oracle parity tests for string parsing algorithm patterns in Elisp.
//!
//! Covers: recursive descent arithmetic parser with operator precedence,
//! balanced parentheses validator with error positions, INI/config file parser,
//! URL parser, CSV parser with quoted fields, and a simple JSON-like parser.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Recursive descent parser for arithmetic with precedence and evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_arithmetic_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full recursive descent with +, -, *, /, %, unary minus, and parens.
    // Returns both the parsed AST and evaluated result.
    let form = r#"(progn
  (defvar neovm--test-arith-pos 0)
  (defvar neovm--test-arith-input "")

  (fset 'neovm--test-arith-peek
    (lambda ()
      (when (< neovm--test-arith-pos (length neovm--test-arith-input))
        (aref neovm--test-arith-input neovm--test-arith-pos))))

  (fset 'neovm--test-arith-advance
    (lambda () (setq neovm--test-arith-pos (1+ neovm--test-arith-pos))))

  (fset 'neovm--test-arith-skip-ws
    (lambda ()
      (while (and (< neovm--test-arith-pos (length neovm--test-arith-input))
                  (= (aref neovm--test-arith-input neovm--test-arith-pos) ?\s))
        (funcall 'neovm--test-arith-advance))))

  ;; Parse a number (multi-digit integer)
  (fset 'neovm--test-arith-parse-number
    (lambda ()
      (let ((start neovm--test-arith-pos))
        (while (and (funcall 'neovm--test-arith-peek)
                    (>= (funcall 'neovm--test-arith-peek) ?0)
                    (<= (funcall 'neovm--test-arith-peek) ?9))
          (funcall 'neovm--test-arith-advance))
        (string-to-number
         (substring neovm--test-arith-input start neovm--test-arith-pos)))))

  ;; factor = NUMBER | '(' expr ')' | '-' factor
  (fset 'neovm--test-arith-parse-factor
    (lambda ()
      (funcall 'neovm--test-arith-skip-ws)
      (let ((ch (funcall 'neovm--test-arith-peek)))
        (cond
         ((and ch (= ch ?\())
          (funcall 'neovm--test-arith-advance)
          (let ((val (funcall 'neovm--test-arith-parse-expr)))
            (funcall 'neovm--test-arith-skip-ws)
            (funcall 'neovm--test-arith-advance) ;; skip ')'
            val))
         ((and ch (= ch ?-))
          (funcall 'neovm--test-arith-advance)
          (- (funcall 'neovm--test-arith-parse-factor)))
         (t (funcall 'neovm--test-arith-parse-number))))))

  ;; term = factor (('*'|'/'|'%') factor)*
  (fset 'neovm--test-arith-parse-term
    (lambda ()
      (let ((val (funcall 'neovm--test-arith-parse-factor)))
        (funcall 'neovm--test-arith-skip-ws)
        (while (and (funcall 'neovm--test-arith-peek)
                    (memq (funcall 'neovm--test-arith-peek) '(?* ?/ ?%)))
          (let ((op (funcall 'neovm--test-arith-peek)))
            (funcall 'neovm--test-arith-advance)
            (let ((right (funcall 'neovm--test-arith-parse-factor)))
              (cond
               ((= op ?*) (setq val (* val right)))
               ((= op ?/) (setq val (/ val right)))
               ((= op ?%) (setq val (% val right))))))
          (funcall 'neovm--test-arith-skip-ws))
        val)))

  ;; expr = term (('+'|'-') term)*
  (fset 'neovm--test-arith-parse-expr
    (lambda ()
      (let ((val (funcall 'neovm--test-arith-parse-term)))
        (funcall 'neovm--test-arith-skip-ws)
        (while (and (funcall 'neovm--test-arith-peek)
                    (memq (funcall 'neovm--test-arith-peek) '(?+ ?-)))
          (let ((op (funcall 'neovm--test-arith-peek)))
            (funcall 'neovm--test-arith-advance)
            (let ((right (funcall 'neovm--test-arith-parse-term)))
              (if (= op ?+)
                  (setq val (+ val right))
                (setq val (- val right)))))
          (funcall 'neovm--test-arith-skip-ws))
        val)))

  (fset 'neovm--test-arith-eval
    (lambda (input)
      (setq neovm--test-arith-pos 0
            neovm--test-arith-input input)
      (funcall 'neovm--test-arith-parse-expr)))

  (unwind-protect
      (list
        (funcall 'neovm--test-arith-eval "42")
        (funcall 'neovm--test-arith-eval "3 + 4")
        (funcall 'neovm--test-arith-eval "3 + 4 * 2")
        (funcall 'neovm--test-arith-eval "(3 + 4) * 2")
        (funcall 'neovm--test-arith-eval "100 - 20 * 3 + 5")
        (funcall 'neovm--test-arith-eval "100 / 5 / 4")
        (funcall 'neovm--test-arith-eval "((2 + 3) * (7 - 2))")
        (funcall 'neovm--test-arith-eval "10 % 3")
        (funcall 'neovm--test-arith-eval "-5 + 3")
        (funcall 'neovm--test-arith-eval "-(3 + 4) * 2")
        (funcall 'neovm--test-arith-eval "2 * 3 + 4 * 5 - 6 / 2"))
    (fmakunbound 'neovm--test-arith-peek)
    (fmakunbound 'neovm--test-arith-advance)
    (fmakunbound 'neovm--test-arith-skip-ws)
    (fmakunbound 'neovm--test-arith-parse-number)
    (fmakunbound 'neovm--test-arith-parse-factor)
    (fmakunbound 'neovm--test-arith-parse-term)
    (fmakunbound 'neovm--test-arith-parse-expr)
    (fmakunbound 'neovm--test-arith-eval)
    (makunbound 'neovm--test-arith-pos)
    (makunbound 'neovm--test-arith-input)))"#;
    let expect = expect_test::expect![[r#""OK (42 7 11 14 45 5 25 1 -2 -14 23)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Balanced parentheses validator with error position reporting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_balanced_parens_validator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Validate balanced (), [], {}, <> with detailed error info:
    // Returns 'ok or (error-type position context-string)
    let form = r#"(progn
  (fset 'neovm--test-balance-check
    (lambda (input)
      (let ((stack nil)
            (i 0)
            (len (length input))
            (openers '((?\( . ?\)) (?\[ . ?\]) (?\{ . ?\}) (?< . ?>)))
            (closers '((?\) . ?\() (?\] . ?\[) (?\} . ?\{) (?> . ?<))))
        (catch 'neovm--test-balance-done
          (while (< i len)
            (let ((ch (aref input i)))
              (cond
               ;; Opening delimiter
               ((assq ch openers)
                (setq stack (cons (cons ch i) stack)))
               ;; Closing delimiter
               ((assq ch closers)
                (if (null stack)
                    (throw 'neovm--test-balance-done
                           (list 'unexpected-close
                                 i
                                 (char-to-string ch)
                                 (substring input
                                            (max 0 (- i 3))
                                            (min len (+ i 4)))))
                  (let ((expected-open (cdr (assq ch closers))))
                    (if (/= (caar stack) expected-open)
                        (throw 'neovm--test-balance-done
                               (list 'mismatch
                                     (cdar stack)
                                     (char-to-string (caar stack))
                                     i
                                     (char-to-string ch)))
                      (setq stack (cdr stack))))))))
            (setq i (1+ i)))
          (if stack
              (list 'unclosed
                    (length stack)
                    (mapcar (lambda (entry)
                              (list (char-to-string (car entry))
                                    (cdr entry)))
                            stack))
            'ok)))))

  (unwind-protect
      (list
        ;; Balanced cases
        (funcall 'neovm--test-balance-check "()")
        (funcall 'neovm--test-balance-check "([{<>}])")
        (funcall 'neovm--test-balance-check "a(b[c{d<e>f}g]h)i")
        (funcall 'neovm--test-balance-check "")
        (funcall 'neovm--test-balance-check "no delimiters here")
        ;; Error cases
        (funcall 'neovm--test-balance-check "([)]")
        (funcall 'neovm--test-balance-check "((())")
        (funcall 'neovm--test-balance-check ")")
        (funcall 'neovm--test-balance-check "hello(world")
        (funcall 'neovm--test-balance-check "{[(<>)]}")
        ;; Complex nested
        (funcall 'neovm--test-balance-check
                 "func(arr[i], map{key: val}, <type>)"))
    (fmakunbound 'neovm--test-balance-check)))"#;
    let expect = expect_test::expect![[
        r#""OK (ok ok ok ok ok (mismatch 1 \"[\" 2 \")\") (unclosed 1 ((\"(\" 0))) (unexpected-close 0 \")\" \")\") (unclosed 1 ((\"(\" 5))) ok ok)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// INI/config file parser with sections, keys, values, and comments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_ini_config_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Split string by character
  (fset 'neovm--test-ini-split-lines
    (lambda (input)
      (let ((lines nil) (start 0) (i 0) (len (length input)))
        (while (<= i len)
          (when (or (= i len) (= (aref input i) ?\n))
            (setq lines (cons (substring input start i) lines)
                  start (1+ i)))
          (setq i (1+ i)))
        (nreverse lines))))

  ;; Trim leading/trailing whitespace
  (fset 'neovm--test-ini-trim
    (lambda (s)
      (let ((start 0) (end (length s)))
        (while (and (< start end)
                    (memq (aref s start) '(?\s ?\t)))
          (setq start (1+ start)))
        (while (and (> end start)
                    (memq (aref s (1- end)) '(?\s ?\t)))
          (setq end (1- end)))
        (substring s start end))))

  ;; Parse INI: returns alist of (section . ((key . value) ...))
  ;; Supports # and ; comments, blank lines, whitespace around =
  (fset 'neovm--test-ini-parse
    (lambda (input)
      (let ((lines (funcall 'neovm--test-ini-split-lines input))
            (sections nil)
            (current-section nil)
            (current-pairs nil))
        (mapc
         (lambda (raw-line)
           (let ((line (funcall 'neovm--test-ini-trim raw-line)))
             (cond
              ;; Empty or comment
              ((or (= (length line) 0)
                   (and (> (length line) 0)
                        (memq (aref line 0) '(?# ?\;))))
               nil)
              ;; Section header
              ((and (> (length line) 2)
                    (= (aref line 0) ?\[)
                    (= (aref line (1- (length line))) ?\]))
               (when current-section
                 (setq sections
                       (cons (cons current-section (nreverse current-pairs))
                             sections)))
               (setq current-section
                     (funcall 'neovm--test-ini-trim
                              (substring line 1 (1- (length line))))
                     current-pairs nil))
              ;; Key = value
              (t
               (let ((eq-pos (string-match "=" line)))
                 (when eq-pos
                   (let ((key (funcall 'neovm--test-ini-trim
                                       (substring line 0 eq-pos)))
                         (val (funcall 'neovm--test-ini-trim
                                       (substring line (1+ eq-pos)))))
                     ;; Strip inline comments (after unquoted ;)
                     (let ((comment-pos (string-match " [;#]" val)))
                       (when comment-pos
                         (setq val (funcall 'neovm--test-ini-trim
                                            (substring val 0 comment-pos)))))
                     (setq current-pairs
                           (cons (cons key val) current-pairs)))))))))
         lines)
        ;; Save final section
        (when current-section
          (setq sections
                (cons (cons current-section (nreverse current-pairs))
                      sections)))
        (nreverse sections))))

  (unwind-protect
      (let ((config (funcall 'neovm--test-ini-parse
                     "[database]\nhost = localhost\nport = 5432\nname = mydb\n\n; Database credentials\n[credentials]\nuser = admin\npassword = secret123\n\n[server]\nbind = 0.0.0.0\nport = 8080\nworkers = 4 ; number of threads\ndebug = false\n\n# Logging configuration\n[logging]\nlevel = info\npath = /var/log/app.log\nmax_size = 10M\n")))
        (list
          ;; Number of sections
          (length config)
          ;; Section names
          (mapcar 'car config)
          ;; Database values
          (cdr (assoc "database" config))
          ;; Credentials
          (cdr (assoc "password" (cdr (assoc "credentials" config))))
          ;; Server: workers should have comment stripped
          (cdr (assoc "workers" (cdr (assoc "server" config))))
          ;; Logging path
          (cdr (assoc "path" (cdr (assoc "logging" config))))
          ;; Total key-value count
          (apply '+ (mapcar (lambda (section) (length (cdr section))) config))))
    (fmakunbound 'neovm--test-ini-split-lines)
    (fmakunbound 'neovm--test-ini-trim)
    (fmakunbound 'neovm--test-ini-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK (4 (\"database\" \"credentials\" \"server\" \"logging\") ((\"host\" . \"localhost\") (\"port\" . \"5432\") (\"name\" . \"mydb\")) \"secret123\" \"4\" \"/var/log/app.log\" 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// URL parser (scheme, host, port, path, query, fragment)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_url_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse URL into components. Returns alist with scheme, host, port, path,
    // query (as alist), fragment.
    let form = r#"(progn
  (fset 'neovm--test-url-parse
    (lambda (url)
      (let ((pos 0) (len (length url))
            (scheme nil) (host nil) (port nil)
            (path nil) (query nil) (fragment nil))
        ;; Parse scheme
        (let ((colon-pos (string-match "://" url)))
          (when colon-pos
            (setq scheme (substring url 0 colon-pos)
                  pos (+ colon-pos 3))))
        ;; Parse host (and optional port)
        (let ((host-start pos)
              (host-end nil))
          ;; Find end of host: /, ?, #, or end of string
          (let ((i pos))
            (while (and (< i len)
                        (not (memq (aref url i) '(?/ ?? ?#))))
              (setq i (1+ i)))
            (setq host-end i))
          (let ((host-str (substring url host-start host-end)))
            ;; Check for port
            (let ((colon-in-host (string-match ":" host-str)))
              (if colon-in-host
                  (progn
                    (setq host (substring host-str 0 colon-in-host))
                    (setq port (string-to-number
                                (substring host-str (1+ colon-in-host)))))
                (setq host host-str)))
            (setq pos host-end)))
        ;; Parse path
        (when (and (< pos len) (= (aref url pos) ?/))
          (let ((path-start pos) (i pos))
            (while (and (< i len)
                        (not (memq (aref url i) '(?? ?#))))
              (setq i (1+ i)))
            (setq path (substring url path-start i)
                  pos i)))
        ;; Parse query string
        (when (and (< pos len) (= (aref url pos) ??))
          (setq pos (1+ pos))
          (let ((q-start pos) (i pos))
            (while (and (< i len) (/= (aref url i) ?#))
              (setq i (1+ i)))
            (let ((q-str (substring url q-start i)))
              ;; Split query by & into key=value pairs
              (let ((pairs nil) (start 0) (j 0) (qlen (length q-str)))
                (while (<= j qlen)
                  (when (or (= j qlen) (= (aref q-str j) ?&))
                    (let ((pair-str (substring q-str start j)))
                      (let ((eq-pos (string-match "=" pair-str)))
                        (if eq-pos
                            (setq pairs (cons (cons (substring pair-str 0 eq-pos)
                                                    (substring pair-str (1+ eq-pos)))
                                              pairs))
                          (setq pairs (cons (cons pair-str "") pairs))))
                      (setq start (1+ j))))
                  (setq j (1+ j)))
                (setq query (nreverse pairs))))
            (setq pos i)))
        ;; Parse fragment
        (when (and (< pos len) (= (aref url pos) ?#))
          (setq fragment (substring url (1+ pos))))
        ;; Return alist
        (list (cons 'scheme scheme)
              (cons 'host host)
              (cons 'port port)
              (cons 'path path)
              (cons 'query query)
              (cons 'fragment fragment)))))

  (unwind-protect
      (list
        ;; Full URL
        (funcall 'neovm--test-url-parse
                 "https://example.com:8080/api/users?page=1&limit=10#results")
        ;; Simple URL
        (funcall 'neovm--test-url-parse
                 "http://localhost/index.html")
        ;; With port, no path
        (funcall 'neovm--test-url-parse
                 "http://db.local:5432")
        ;; Query only, no fragment
        (funcall 'neovm--test-url-parse
                 "https://search.engine/find?q=elisp&lang=en")
        ;; Fragment only
        (funcall 'neovm--test-url-parse
                 "https://docs.site/guide#chapter-3")
        ;; Complex query with multiple params
        (funcall 'neovm--test-url-parse
                 "https://api.service.com/v2/data?format=json&key=abc123&verbose=true&offset=0"))
    (fmakunbound 'neovm--test-url-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK (((scheme . \"https\") (host . \"example.com\") (port . 8080) (path . \"/api/users\") (query (\"page\" . \"1\") (\"limit\" . \"10\")) (fragment . \"results\")) ((scheme . \"http\") (host . \"localhost\") (port) (path . \"/index.html\") (query) (fragment)) ((scheme . \"http\") (host . \"db.local\") (port . 5432) (path) (query) (fragment)) ((scheme . \"https\") (host . \"search.engine\") (port) (path . \"/find\") (query (\"q\" . \"elisp\") (\"lang\" . \"en\")) (fragment)) ((scheme . \"https\") (host . \"docs.site\") (port) (path . \"/guide\") (query) (fragment . \"chapter-3\")) ((scheme . \"https\") (host . \"api.service.com\") (port) (path . \"/v2/data\") (query (\"format\" . \"json\") (\"key\" . \"abc123\") (\"verbose\" . \"true\") (\"offset\" . \"0\")) (fragment)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSV parser handling quoted fields with embedded commas
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV rows: handles quoted fields (double-quote delimited),
    // embedded commas inside quotes, escaped quotes ("").
    let form = r#"(progn
  ;; Parse one CSV row into a list of field strings
  (fset 'neovm--test-csv-parse-row
    (lambda (line)
      (let ((fields nil)
            (i 0)
            (len (length line))
            (field-start 0)
            (in-quotes nil)
            (current nil))
        (setq current nil)
        (while (<= i len)
          (cond
           ;; End of line or comma outside quotes: emit field
           ((and (or (= i len)
                     (and (not in-quotes)
                          (< i len)
                          (= (aref line i) ?,)))
                 (not in-quotes))
            (let ((raw (if current
                           (apply 'concat (nreverse current))
                         "")))
              (setq fields (cons raw fields)))
            (setq current nil)
            (setq i (1+ i)))
           ;; Quote character
           ((and (< i len) (= (aref line i) ?\"))
            (if (not in-quotes)
                ;; Start quoted region
                (progn
                  (setq in-quotes t)
                  (setq i (1+ i)))
              ;; Inside quotes: check for escaped quote
              (if (and (< (1+ i) len) (= (aref line (1+ i)) ?\"))
                  ;; Escaped quote: emit one quote, skip two chars
                  (progn
                    (setq current (cons "\"" current))
                    (setq i (+ i 2)))
                ;; End of quoted region
                (setq in-quotes nil)
                (setq i (1+ i)))))
           ;; Regular character
           (t
            (setq current (cons (char-to-string (aref line i)) current))
            (setq i (1+ i)))))
        (nreverse fields))))

  ;; Parse multi-line CSV (split by newline, parse each row)
  (fset 'neovm--test-csv-parse
    (lambda (input)
      (let ((lines nil) (start 0) (i 0) (len (length input))
            (in-quotes nil))
        ;; Split respecting quoted newlines
        (while (<= i len)
          (cond
           ((and (< i len) (= (aref input i) ?\"))
            (setq in-quotes (not in-quotes))
            (setq i (1+ i)))
           ((and (or (= i len)
                     (= (aref input i) ?\n))
                 (not in-quotes))
            (let ((line (substring input start i)))
              (when (> (length line) 0)
                (setq lines (cons line lines))))
            (setq start (1+ i))
            (setq i (1+ i)))
           (t (setq i (1+ i)))))
        (mapcar (lambda (line)
                  (funcall 'neovm--test-csv-parse-row line))
                (nreverse lines)))))

  (unwind-protect
      (list
        ;; Simple row
        (funcall 'neovm--test-csv-parse-row "Alice,30,Boston")
        ;; Quoted field with comma
        (funcall 'neovm--test-csv-parse-row "\"Smith, John\",42,\"New York, NY\"")
        ;; Escaped quotes
        (funcall 'neovm--test-csv-parse-row "\"He said \"\"hello\"\"\",test")
        ;; Empty fields
        (funcall 'neovm--test-csv-parse-row "a,,b,,c")
        ;; Multi-line CSV
        (funcall 'neovm--test-csv-parse
                 "name,age,city\nAlice,30,Boston\nBob,25,LA")
        ;; CSV with quoted commas
        (funcall 'neovm--test-csv-parse
                 "id,name,address\n1,Alice,\"123 Main St, Apt 4\"\n2,Bob,\"456 Oak Ave\"")
        ;; Field counts
        (let ((rows (funcall 'neovm--test-csv-parse
                     "a,b,c\n1,2,3\nx,y,z")))
          (list (length rows)
                (mapcar 'length rows))))
    (fmakunbound 'neovm--test-csv-parse-row)
    (fmakunbound 'neovm--test-csv-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"30\" \"Boston\") (\"Smith, John\" \"42\" \"New York, NY\") (\"He said \\\"hello\\\"\" \"test\") (\"a\" \"\" \"b\" \"\" \"c\") ((\"name\" \"age\" \"city\") (\"Alice\" \"30\" \"Boston\") (\"Bob\" \"25\" \"LA\")) ((\"id\" \"name\" \"address\") (\"1\" \"Alice\" \"123 Main St, Apt 4\") (\"2\" \"Bob\" \"456 Oak Ave\")) (3 (3 3 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple JSON-like parser (objects, arrays, strings, numbers, booleans)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_json_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse simplified JSON into Elisp structures:
    //   object -> alist, array -> list, string -> string,
    //   number -> number, true -> t, false -> nil, null -> 'null
    let form = r#"(progn
  (defvar neovm--test-jp-pos 0)
  (defvar neovm--test-jp-input "")

  (fset 'neovm--test-jp-peek
    (lambda ()
      (when (< neovm--test-jp-pos (length neovm--test-jp-input))
        (aref neovm--test-jp-input neovm--test-jp-pos))))

  (fset 'neovm--test-jp-next
    (lambda ()
      (let ((ch (funcall 'neovm--test-jp-peek)))
        (setq neovm--test-jp-pos (1+ neovm--test-jp-pos))
        ch)))

  (fset 'neovm--test-jp-skip-ws
    (lambda ()
      (while (and (funcall 'neovm--test-jp-peek)
                  (memq (funcall 'neovm--test-jp-peek) '(?\s ?\t ?\n ?\r)))
        (funcall 'neovm--test-jp-next))))

  (fset 'neovm--test-jp-parse-string
    (lambda ()
      (funcall 'neovm--test-jp-next) ;; skip "
      (let ((parts nil))
        (while (and (funcall 'neovm--test-jp-peek)
                    (/= (funcall 'neovm--test-jp-peek) ?\"))
          (let ((ch (funcall 'neovm--test-jp-next)))
            (if (= ch ?\\)
                (let ((escaped (funcall 'neovm--test-jp-next)))
                  (setq parts (cons (cond
                                     ((= escaped ?n) "\n")
                                     ((= escaped ?t) "\t")
                                     ((= escaped ?\\) "\\")
                                     ((= escaped ?\") "\"")
                                     (t (char-to-string escaped)))
                                    parts)))
              (setq parts (cons (char-to-string ch) parts)))))
        (funcall 'neovm--test-jp-next) ;; skip closing "
        (apply 'concat (nreverse parts)))))

  (fset 'neovm--test-jp-parse-number
    (lambda ()
      (let ((start neovm--test-jp-pos))
        (when (and (funcall 'neovm--test-jp-peek)
                   (= (funcall 'neovm--test-jp-peek) ?-))
          (funcall 'neovm--test-jp-next))
        (while (and (funcall 'neovm--test-jp-peek)
                    (>= (funcall 'neovm--test-jp-peek) ?0)
                    (<= (funcall 'neovm--test-jp-peek) ?9))
          (funcall 'neovm--test-jp-next))
        ;; Handle decimal point
        (when (and (funcall 'neovm--test-jp-peek)
                   (= (funcall 'neovm--test-jp-peek) ?.))
          (funcall 'neovm--test-jp-next)
          (while (and (funcall 'neovm--test-jp-peek)
                      (>= (funcall 'neovm--test-jp-peek) ?0)
                      (<= (funcall 'neovm--test-jp-peek) ?9))
            (funcall 'neovm--test-jp-next)))
        (string-to-number
         (substring neovm--test-jp-input start neovm--test-jp-pos)))))

  ;; Match a literal keyword
  (fset 'neovm--test-jp-match-keyword
    (lambda (word)
      (let ((wlen (length word))
            (i 0)
            (ok t))
        (while (and ok (< i wlen))
          (if (and (funcall 'neovm--test-jp-peek)
                   (= (funcall 'neovm--test-jp-peek) (aref word i)))
              (progn (funcall 'neovm--test-jp-next) (setq i (1+ i)))
            (setq ok nil)))
        ok)))

  (fset 'neovm--test-jp-parse-value
    (lambda ()
      (funcall 'neovm--test-jp-skip-ws)
      (let ((ch (funcall 'neovm--test-jp-peek)))
        (cond
         ((null ch) nil)
         ((= ch ?\") (funcall 'neovm--test-jp-parse-string))
         ((or (and (>= ch ?0) (<= ch ?9)) (= ch ?-))
          (funcall 'neovm--test-jp-parse-number))
         ((= ch ?\{) (funcall 'neovm--test-jp-parse-object))
         ((= ch ?\[) (funcall 'neovm--test-jp-parse-array))
         ((= ch ?t)
          (funcall 'neovm--test-jp-match-keyword "true") t)
         ((= ch ?f)
          (funcall 'neovm--test-jp-match-keyword "false") nil)
         ((= ch ?n)
          (funcall 'neovm--test-jp-match-keyword "null") 'null)
         (t nil)))))

  (fset 'neovm--test-jp-parse-array
    (lambda ()
      (funcall 'neovm--test-jp-next) ;; skip [
      (funcall 'neovm--test-jp-skip-ws)
      (if (and (funcall 'neovm--test-jp-peek)
               (= (funcall 'neovm--test-jp-peek) ?\]))
          (progn (funcall 'neovm--test-jp-next) nil)
        (let ((items (list (funcall 'neovm--test-jp-parse-value))))
          (funcall 'neovm--test-jp-skip-ws)
          (while (and (funcall 'neovm--test-jp-peek)
                      (= (funcall 'neovm--test-jp-peek) ?,))
            (funcall 'neovm--test-jp-next)
            (setq items (cons (funcall 'neovm--test-jp-parse-value) items))
            (funcall 'neovm--test-jp-skip-ws))
          (funcall 'neovm--test-jp-next) ;; skip ]
          (nreverse items)))))

  (fset 'neovm--test-jp-parse-object
    (lambda ()
      (funcall 'neovm--test-jp-next) ;; skip {
      (funcall 'neovm--test-jp-skip-ws)
      (if (and (funcall 'neovm--test-jp-peek)
               (= (funcall 'neovm--test-jp-peek) ?\}))
          (progn (funcall 'neovm--test-jp-next) nil)
        (let ((pairs nil))
          (let ((parse-pair
                 (lambda ()
                   (funcall 'neovm--test-jp-skip-ws)
                   (let ((key (funcall 'neovm--test-jp-parse-string)))
                     (funcall 'neovm--test-jp-skip-ws)
                     (funcall 'neovm--test-jp-next) ;; skip :
                     (let ((val (funcall 'neovm--test-jp-parse-value)))
                       (setq pairs (cons (cons key val) pairs)))))))
            (funcall parse-pair)
            (funcall 'neovm--test-jp-skip-ws)
            (while (and (funcall 'neovm--test-jp-peek)
                        (= (funcall 'neovm--test-jp-peek) ?,))
              (funcall 'neovm--test-jp-next)
              (funcall parse-pair)
              (funcall 'neovm--test-jp-skip-ws)))
          (funcall 'neovm--test-jp-next) ;; skip }
          (nreverse pairs)))))

  (fset 'neovm--test-jp-parse
    (lambda (input)
      (setq neovm--test-jp-pos 0
            neovm--test-jp-input input)
      (funcall 'neovm--test-jp-parse-value)))

  (unwind-protect
      (list
        ;; Simple types
        (funcall 'neovm--test-jp-parse "42")
        (funcall 'neovm--test-jp-parse "-7")
        (funcall 'neovm--test-jp-parse "\"hello world\"")
        (funcall 'neovm--test-jp-parse "true")
        (funcall 'neovm--test-jp-parse "false")
        (funcall 'neovm--test-jp-parse "null")
        ;; Arrays
        (funcall 'neovm--test-jp-parse "[1, 2, 3]")
        (funcall 'neovm--test-jp-parse "[]")
        (funcall 'neovm--test-jp-parse "[\"a\", \"b\", \"c\"]")
        ;; Objects
        (funcall 'neovm--test-jp-parse "{\"name\": \"Alice\", \"age\": 30}")
        (funcall 'neovm--test-jp-parse "{}")
        ;; Nested structures
        (funcall 'neovm--test-jp-parse
                 "{\"users\": [{\"id\": 1, \"name\": \"Alice\"}, {\"id\": 2, \"name\": \"Bob\"}]}")
        ;; Mixed nesting
        (funcall 'neovm--test-jp-parse
                 "{\"config\": {\"debug\": true, \"ports\": [80, 443]}, \"version\": 2}")
        ;; Escaped strings
        (funcall 'neovm--test-jp-parse "\"line1\\nline2\\ttab\"")
        ;; Boolean/null in arrays
        (funcall 'neovm--test-jp-parse "[true, false, null, 0, \"\"]"))
    (fmakunbound 'neovm--test-jp-peek)
    (fmakunbound 'neovm--test-jp-next)
    (fmakunbound 'neovm--test-jp-skip-ws)
    (fmakunbound 'neovm--test-jp-parse-string)
    (fmakunbound 'neovm--test-jp-parse-number)
    (fmakunbound 'neovm--test-jp-match-keyword)
    (fmakunbound 'neovm--test-jp-parse-value)
    (fmakunbound 'neovm--test-jp-parse-array)
    (fmakunbound 'neovm--test-jp-parse-object)
    (fmakunbound 'neovm--test-jp-parse)
    (makunbound 'neovm--test-jp-pos)
    (makunbound 'neovm--test-jp-input)))"#;
    let expect = expect_test::expect![[
        r#""OK (42 -7 \"hello world\" t nil null (1 2 3) nil (\"a\" \"b\" \"c\") ((\"name\" . \"Alice\") (\"age\" . 30)) nil ((\"users\" ((\"id\" . 1) (\"name\" . \"Alice\")) ((\"id\" . 2) (\"name\" . \"Bob\")))) ((\"config\" (\"debug\" . t) (\"ports\" 80 443)) (\"version\" . 2)) \"line1\\nline2\ttab\" (t nil null 0 \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tokenizer/lexer for a C-like language
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_strparse_c_like_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize a C-like snippet into typed tokens:
    // keywords, identifiers, numbers, strings, operators, delimiters
    let form = r#"(progn
  (defvar neovm--test-tok-keywords
    '("if" "else" "while" "return" "int" "void" "for"))

  (fset 'neovm--test-tokenize-c
    (lambda (input)
      (let ((tokens nil) (i 0) (len (length input)))
        (while (< i len)
          (let ((ch (aref input i)))
            (cond
             ;; Whitespace
             ((memq ch '(?\s ?\t ?\n ?\r))
              (setq i (1+ i)))
             ;; Single-line comment
             ((and (= ch ?/) (< (1+ i) len) (= (aref input (1+ i)) ?/))
              (while (and (< i len) (/= (aref input i) ?\n))
                (setq i (1+ i))))
             ;; String literal
             ((= ch ?\")
              (let ((start (1+ i)))
                (setq i (1+ i))
                (while (and (< i len) (/= (aref input i) ?\"))
                  (when (= (aref input i) ?\\)
                    (setq i (1+ i))) ;; skip escaped char
                  (setq i (1+ i)))
                (setq tokens (cons (list 'string
                                         (substring input start i))
                                   tokens))
                (setq i (1+ i)))) ;; skip closing "
             ;; Number
             ((and (>= ch ?0) (<= ch ?9))
              (let ((start i))
                (while (and (< i len)
                            (>= (aref input i) ?0)
                            (<= (aref input i) ?9))
                  (setq i (1+ i)))
                (setq tokens (cons (list 'number
                                         (string-to-number
                                          (substring input start i)))
                                   tokens))))
             ;; Identifier or keyword
             ((or (and (>= ch ?a) (<= ch ?z))
                  (and (>= ch ?A) (<= ch ?Z))
                  (= ch ?_))
              (let ((start i))
                (while (and (< i len)
                            (let ((c (aref input i)))
                              (or (and (>= c ?a) (<= c ?z))
                                  (and (>= c ?A) (<= c ?Z))
                                  (and (>= c ?0) (<= c ?9))
                                  (= c ?_))))
                  (setq i (1+ i)))
                (let ((word (substring input start i)))
                  (if (member word neovm--test-tok-keywords)
                      (setq tokens (cons (list 'keyword word) tokens))
                    (setq tokens (cons (list 'ident word) tokens))))))
             ;; Two-character operators
             ((and (< (1+ i) len)
                   (member (substring input i (+ i 2))
                           '("==" "!=" "<=" ">=" "&&" "||" "++" "--")))
              (setq tokens (cons (list 'op (substring input i (+ i 2)))
                                 tokens))
              (setq i (+ i 2)))
             ;; Single-character operators/delimiters
             ((memq ch '(?+ ?- ?* ?/ ?= ?< ?> ?! ?& ?|))
              (setq tokens (cons (list 'op (char-to-string ch)) tokens))
              (setq i (1+ i)))
             ;; Delimiters
             ((memq ch '(?\( ?\) ?\{ ?\} ?\; ?,))
              (setq tokens (cons (list 'delim (char-to-string ch)) tokens))
              (setq i (1+ i)))
             ;; Unknown
             (t (setq i (1+ i))))))
        (nreverse tokens))))

  (unwind-protect
      (list
        ;; Simple declaration
        (funcall 'neovm--test-tokenize-c "int x = 42;")
        ;; If statement
        (funcall 'neovm--test-tokenize-c "if (x >= 10) { return x; }")
        ;; While loop with operators
        (funcall 'neovm--test-tokenize-c "while (i < n && arr != 0) { i++; }")
        ;; Function-like
        (funcall 'neovm--test-tokenize-c "void foo(int a, int b) { return a + b; }")
        ;; String literal
        (funcall 'neovm--test-tokenize-c "printf(\"hello world\");")
        ;; Token counts
        (let ((tokens (funcall 'neovm--test-tokenize-c
                       "for (int i = 0; i < 10; i++) { sum = sum + arr; }")))
          (list (length tokens)
                (length (seq-filter (lambda (t) (eq (car t) 'keyword)) tokens))
                (length (seq-filter (lambda (t) (eq (car t) 'ident)) tokens)))))
    (fmakunbound 'neovm--test-tokenize-c)
    (makunbound 'neovm--test-tok-keywords)))"#;
    let expect = expect_test::expect![[
        r#""OK (((keyword \"int\") (ident \"x\") (op \"=\") (number 42) (delim \";\")) ((keyword \"if\") (delim \"(\") (ident \"x\") (op \">=\") (number 10) (delim \")\") (delim \"{\") (keyword \"return\") (ident \"x\") (delim \";\") (delim \"}\")) ((keyword \"while\") (delim \"(\") (ident \"i\") (op \"<\") (ident \"n\") (op \"&&\") (ident \"arr\") (op \"!=\") (number 0) (delim \")\") (delim \"{\") (ident \"i\") (op \"++\") (delim \";\") (delim \"}\")) ((keyword \"void\") (ident \"foo\") (delim \"(\") (keyword \"int\") (ident \"a\") (delim \",\") (keyword \"int\") (ident \"b\") (delim \")\") (delim \"{\") (keyword \"return\") (ident \"a\") (op \"+\") (ident \"b\") (delim \";\") (delim \"}\")) ((ident \"printf\") (delim \"(\") (string \"hello world\") (delim \")\") (delim \";\")) (22 2 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
