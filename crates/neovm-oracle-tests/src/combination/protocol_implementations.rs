//! Complex oracle parity tests for protocol/format implementations:
//! MIME header parser, URL parser, S-expression pretty printer,
//! key-value protocol (Redis-like) command parser,
//! simple email parser, and HTTP response builder.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// MIME header parser
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_mime_header_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((raw-headers "Content-Type: text/html; charset=utf-8\nContent-Length: 1234\nX-Custom-Header: some value\nContent-Disposition: attachment; filename=\"report.pdf\"\nAccept: text/html, application/json; q=0.9")
         ;; Parse headers into alist of (name . value)
         (lines (split-string raw-headers "\n"))
         (headers
          (mapcar
           (lambda (line)
             (let ((colon-pos (seq-position line ?:)))
               (if colon-pos
                   (cons (string-trim (substring line 0 colon-pos))
                         (string-trim (substring line (1+ colon-pos))))
                 (cons line ""))))
           lines))
         ;; Parse Content-Type parameters
         (ct-value (cdr (assoc "Content-Type" headers)))
         (ct-parts (split-string ct-value ";"))
         (ct-type (string-trim (car ct-parts)))
         (ct-params
          (mapcar
           (lambda (part)
             (let* ((trimmed (string-trim part))
                    (eq-pos (seq-position trimmed ?=)))
               (if eq-pos
                   (cons (string-trim (substring trimmed 0 eq-pos))
                         (string-trim (substring trimmed (1+ eq-pos))))
                 (cons trimmed ""))))
           (cdr ct-parts)))
         ;; Parse Content-Disposition similarly
         (cd-value (cdr (assoc "Content-Disposition" headers)))
         (cd-parts (split-string cd-value ";"))
         (cd-type (string-trim (car cd-parts)))
         (cd-params
          (mapcar
           (lambda (part)
             (let* ((trimmed (string-trim part))
                    (eq-pos (seq-position trimmed ?=)))
               (if eq-pos
                   (cons (string-trim (substring trimmed 0 eq-pos))
                         (string-trim (substring trimmed (1+ eq-pos)) "\""))
                 (cons trimmed ""))))
           (cdr cd-parts)))
         ;; Extract Accept quality values
         (accept-value (cdr (assoc "Accept" headers)))
         (accept-parts (split-string accept-value ","))
         (accept-parsed
          (mapcar
           (lambda (part)
             (let* ((trimmed (string-trim part))
                    (semi-pos (seq-position trimmed ?\;)))
               (if semi-pos
                   (let* ((type (string-trim (substring trimmed 0 semi-pos)))
                          (param (string-trim (substring trimmed (1+ semi-pos))))
                          (eq-pos (seq-position param ?=))
                          (q-val (if eq-pos
                                     (string-to-number (substring param (1+ eq-pos)))
                                   1.0)))
                     (cons type q-val))
                 (cons trimmed 1.0))))
           accept-parts)))
  (list
   (length headers)
   ct-type
   ct-params
   cd-type
   cd-params
   accept-parsed
   (cdr (assoc "Content-Length" headers))
   (cdr (assoc "X-Custom-Header" headers))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 \"text/html\" ((\"charset\" . \"utf-8\")) \"attachment\" ((\"filename\" . \"report.pdf\\\"\")) ((\"text/html\" . 1.0) (\"application/json\" . 0.9)) \"1234\" \"some value\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// URL parser (scheme, host, port, path, query, fragment)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_url_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((parse-url
       (lambda (url)
         (let ((scheme nil) (host nil) (port nil) (path nil) (query nil) (fragment nil)
               (rest url))
           ;; Extract fragment (#...)
           (let ((hash-pos (seq-position rest ?#)))
             (when hash-pos
               (setq fragment (substring rest (1+ hash-pos)))
               (setq rest (substring rest 0 hash-pos))))
           ;; Extract query (?...)
           (let ((q-pos (seq-position rest ??)))
             (when q-pos
               (setq query (substring rest (1+ q-pos)))
               (setq rest (substring rest 0 q-pos))))
           ;; Extract scheme (xxx://)
           (when (string-match "^\\([a-zA-Z]+\\)://" rest)
             (setq scheme (match-string 1 rest))
             (setq rest (substring rest (match-end 0))))
           ;; Extract path (from first /)
           (let ((slash-pos (seq-position rest ?/)))
             (when slash-pos
               (setq path (substring rest slash-pos))
               (setq rest (substring rest 0 slash-pos))))
           ;; Extract port (:NNN)
           (when (string-match ":\\([0-9]+\\)$" rest)
             (setq port (string-to-number (match-string 1 rest)))
             (setq rest (substring rest 0 (match-beginning 0))))
           ;; Remaining is host
           (setq host rest)
           ;; Parse query string into alist
           (let ((query-params nil))
             (when query
               (dolist (pair (split-string query "&"))
                 (let ((eq-pos (seq-position pair ?=)))
                   (if eq-pos
                       (setq query-params
                             (cons (cons (substring pair 0 eq-pos)
                                         (substring pair (1+ eq-pos)))
                                   query-params))
                     (setq query-params
                           (cons (cons pair "") query-params))))))
             (list (cons 'scheme scheme)
                   (cons 'host host)
                   (cons 'port port)
                   (cons 'path path)
                   (cons 'query (nreverse query-params))
                   (cons 'fragment fragment)))))))
  (list
   (funcall parse-url "https://example.com:8080/api/v1/users?name=alice&age=30#section1")
   (funcall parse-url "http://localhost/index.html")
   (funcall parse-url "ftp://files.example.com/pub/data.tar.gz")
   (funcall parse-url "https://search.example.com/search?q=hello+world")
   (funcall parse-url "https://example.com#top")
   (funcall parse-url "https://example.com:443/path/to/resource?key=value&foo=bar&baz=")))"#;
    let expect = expect_test::expect![[
        r#""OK (((scheme . \"https\") (host . \"example.com\") (port . 8080) (path . \"/api/v1/users\") (query (\"name\" . \"alice\") (\"age\" . \"30\")) (fragment . \"section1\")) ((scheme . \"http\") (host . \"localhost\") (port) (path . \"/index.html\") (query) (fragment)) ((scheme . \"ftp\") (host . \"files.example.com\") (port) (path . \"/pub/data.tar.gz\") (query) (fragment)) ((scheme . \"https\") (host . \"search.example.com\") (port) (path . \"/search\") (query (\"q\" . \"hello+world\")) (fragment)) ((scheme . \"https\") (host . \"example.com\") (port) (path) (query) (fragment . \"top\")) ((scheme . \"https\") (host . \"example.com\") (port . 443) (path . \"/path/to/resource\") (query (\"key\" . \"value\") (\"foo\" . \"bar\") (\"baz\" . \"\")) (fragment)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// S-expression pretty printer with indentation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_sexp_pretty_printer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((pp-sexp
       (lambda (expr indent-level)
         (let ((indent (make-string (* indent-level 2) ? )))
           (cond
            ;; Atom: just print it
            ((null expr) (concat indent "nil"))
            ((symbolp expr) (concat indent (symbol-name expr)))
            ((numberp expr) (concat indent (number-to-string expr)))
            ((stringp expr) (concat indent "\"" expr "\""))
            ;; Short list (all atoms, fits on one line): print inline
            ((and (listp expr)
                  (< (length expr) 5)
                  (not (seq-find #'listp expr)))
             (concat indent "("
                     (mapconcat
                      (lambda (e)
                        (cond
                         ((null e) "nil")
                         ((symbolp e) (symbol-name e))
                         ((numberp e) (number-to-string e))
                         ((stringp e) (concat "\"" e "\""))
                         (t "?")))
                      expr " ")
                     ")"))
            ;; Long or nested list: multi-line
            ((listp expr)
             (concat indent "(\n"
                     (mapconcat
                      (lambda (e)
                        (funcall pp-sexp e (1+ indent-level)))
                      expr "\n")
                     "\n" indent ")"))
            (t (concat indent "?")))))))
  (list
   ;; Simple list
   (funcall pp-sexp '(a b c) 0)
   ;; Nested list
   (funcall pp-sexp '(defun foo (x y) (+ x y)) 0)
   ;; Deeply nested
   (funcall pp-sexp '(let ((a 1) (b 2)) (+ a b)) 0)
   ;; With strings
   (funcall pp-sexp '(message "hello" "world") 0)
   ;; Empty
   (funcall pp-sexp nil 0)
   ;; Single element
   (funcall pp-sexp '(42) 0)
   ;; Nested with indent
   (funcall pp-sexp '(if (> x 0) (print x) (print y)) 1)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable pp-sexp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Key-value protocol (Redis-like command parser)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_kv_command_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((store nil)
        (parse-command
         (lambda (cmd-str)
           (let* ((parts (split-string (string-trim cmd-str) " "))
                  (command (upcase (car parts)))
                  (args (cdr parts)))
             (cons command args))))
        (execute-command
         (lambda (parsed)
           (let ((cmd (car parsed))
                 (args (cdr parsed)))
             (cond
              ((string= cmd "SET")
               (if (>= (length args) 2)
                   (let ((key (car args))
                         (val (mapconcat #'identity (cdr args) " ")))
                     (let ((entry (assoc key store)))
                       (if entry
                           (setcdr entry val)
                         (setq store (cons (cons key val) store))))
                     "OK")
                 "ERR wrong number of arguments"))
              ((string= cmd "GET")
               (if (= (length args) 1)
                   (let ((entry (assoc (car args) store)))
                     (if entry (cdr entry) "(nil)"))
                 "ERR wrong number of arguments"))
              ((string= cmd "DEL")
               (let ((count 0))
                 (dolist (key args)
                   (when (assoc key store)
                     (setq store (assq-delete-all
                                  (intern key) store))
                     ;; Use string-based deletion
                     (setq store (seq-filter
                                  (lambda (e) (not (string= (car e) key)))
                                  store))
                     (setq count (1+ count))))
                 (number-to-string count)))
              ((string= cmd "EXISTS")
               (let ((count 0))
                 (dolist (key args)
                   (when (assoc key store)
                     (setq count (1+ count))))
                 (number-to-string count)))
              ((string= cmd "KEYS")
               (mapcar #'car store))
              ((string= cmd "MSET")
               (let ((i 0))
                 (while (< (1+ i) (length args))
                   (let ((key (nth i args))
                         (val (nth (1+ i) args)))
                     (let ((entry (assoc key store)))
                       (if entry
                           (setcdr entry val)
                         (setq store (cons (cons key val) store)))))
                   (setq i (+ i 2)))
                 "OK"))
              (t (concat "ERR unknown command '" cmd "'")))))))
  ;; Execute a series of commands
  (let ((results nil))
    (dolist (cmd '("SET name Alice"
                   "SET age 30"
                   "SET city New York"
                   "GET name"
                   "GET age"
                   "GET missing"
                   "EXISTS name age missing"
                   "MSET x 1 y 2 z 3"
                   "GET x"
                   "GET y"
                   "KEYS"
                   "DEL age city"
                   "EXISTS age"
                   "GET name"))
      (let ((parsed (funcall parse-command cmd)))
        (setq results (cons (funcall execute-command parsed) results))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"OK\" \"OK\" \"OK\" \"Alice\" \"30\" \"(nil)\" \"2\" \"OK\" \"1\" \"2\" (\"z\" \"y\" \"x\" \"city\" \"age\" \"name\") \"2\" \"0\" \"Alice\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple email parser (From, To, Subject, Body)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_email_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((parse-email
       (lambda (raw)
         (let* ((parts (split-string raw "\n\n" t))
                (header-block (car parts))
                (body (if (cdr parts) (mapconcat #'identity (cdr parts) "\n\n") ""))
                (header-lines (split-string header-block "\n"))
                (headers nil))
           ;; Parse headers
           (dolist (line header-lines)
             (let ((colon-pos (seq-position line ?:)))
               (when colon-pos
                 (let ((name (string-trim (substring line 0 colon-pos)))
                       (value (string-trim (substring line (1+ colon-pos)))))
                   (setq headers (cons (cons name value) headers))))))
           (setq headers (nreverse headers))
           ;; Extract common fields
           (let* ((from (cdr (assoc "From" headers)))
                  (to (cdr (assoc "To" headers)))
                  (subject (cdr (assoc "Subject" headers)))
                  (date (cdr (assoc "Date" headers)))
                  ;; Parse From field: "Name <email>" or just "email"
                  (from-parsed
                   (if (and from (string-match "\\(.*\\)<\\(.*\\)>" from))
                       (cons (string-trim (match-string 1 from))
                             (string-trim (match-string 2 from)))
                     (cons "" (or from ""))))
                  ;; Parse To field: may have multiple recipients
                  (to-list
                   (when to
                     (mapcar #'string-trim (split-string to ","))))
                  ;; Word count in body
                  (body-words (length (split-string (string-trim body) "[ \t\n]+")))
                  ;; Line count in body
                  (body-lines (length (split-string body "\n"))))
             (list
              (cons 'headers headers)
              (cons 'from from-parsed)
              (cons 'to to-list)
              (cons 'subject subject)
              (cons 'date date)
              (cons 'body-words body-words)
              (cons 'body-lines body-lines)))))))
  ;; Parse multiple emails
  (list
   (funcall parse-email
            "From: Alice Smith <alice@example.com>\nTo: bob@example.com, carol@example.com\nSubject: Meeting Tomorrow\nDate: Mon, 01 Jan 2024\n\nHi team,\n\nLet us meet tomorrow at 10am.\nPlease confirm your availability.\n\nThanks,\nAlice")
   (funcall parse-email
            "From: system@server.com\nTo: admin@server.com\nSubject: Alert: Disk Space Low\n\nWarning: /dev/sda1 is 95% full.")
   ;; Verify specific fields
   (let* ((email (funcall parse-email
                          "From: Test <test@test.com>\nTo: user@test.com\nSubject: Hello\n\nWorld"))
          (from-pair (cdr (assoc 'from email))))
     (list (car from-pair) (cdr from-pair)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((headers (\"From\" . \"Alice Smith <alice@example.com>\") (\"To\" . \"bob@example.com, carol@example.com\") (\"Subject\" . \"Meeting Tomorrow\") (\"Date\" . \"Mon, 01 Jan 2024\")) (from \"Alice Smith\" . \"alice@example.com\") (to \"bob@example.com\" \"carol@example.com\") (subject . \"Meeting Tomorrow\") (date . \"Mon, 01 Jan 2024\") (body-words . 14) (body-lines . 7)) ((headers (\"From\" . \"system@server.com\") (\"To\" . \"admin@server.com\") (\"Subject\" . \"Alert: Disk Space Low\")) (from \"\" . \"system@server.com\") (to \"admin@server.com\") (subject . \"Alert: Disk Space Low\") (date) (body-words . 5) (body-lines . 1)) (\"Test\" \"test@test.com\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HTTP response builder
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_protocol_http_response_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((status-messages
         '((200 . "OK") (201 . "Created") (204 . "No Content")
           (301 . "Moved Permanently") (302 . "Found")
           (400 . "Bad Request") (401 . "Unauthorized")
           (403 . "Forbidden") (404 . "Not Found")
           (500 . "Internal Server Error")))
        (build-response
         (lambda (status headers body)
           (let* ((status-msg (or (cdr (assq status status-messages))
                                  "Unknown"))
                  (status-line (format "HTTP/1.1 %d %s" status status-msg))
                  ;; Add Content-Length if body present
                  (all-headers
                   (if (and body (> (length body) 0))
                       (append headers
                               (list (cons "Content-Length"
                                           (number-to-string (length body)))))
                     headers))
                  ;; Format headers
                  (header-lines
                   (mapconcat
                    (lambda (h)
                      (format "%s: %s" (car h) (cdr h)))
                    all-headers "\r\n"))
                  ;; Build full response
                  (response
                   (if (and body (> (length body) 0))
                       (concat status-line "\r\n" header-lines "\r\n\r\n" body)
                     (concat status-line "\r\n" header-lines "\r\n\r\n"))))
             response)))
        (build-json-response
         (lambda (status data)
           (let ((body (format "{\"status\":%d,\"data\":%s}"
                               status (prin1-to-string data))))
             (funcall build-response status
                      (list (cons "Content-Type" "application/json")
                            (cons "Server" "NeoVM/1.0"))
                      body))))
        (build-redirect
         (lambda (status url)
           (funcall build-response status
                    (list (cons "Location" url)
                          (cons "Server" "NeoVM/1.0"))
                    nil))))
  (let* ((r200 (funcall build-response 200
                        '(("Content-Type" . "text/html")
                          ("Server" . "NeoVM/1.0"))
                        "<h1>Hello World</h1>"))
         (r404 (funcall build-response 404
                        '(("Content-Type" . "text/plain"))
                        "Not Found"))
         (r301 (funcall build-redirect 301 "https://example.com/new-page"))
         (json-r (funcall build-json-response 200 "hello"))
         (r204 (funcall build-response 204
                        '(("Server" . "NeoVM/1.0"))
                        nil))
         ;; Verify status lines
         (starts-200 (string-match-p "^HTTP/1.1 200 OK" r200))
         (starts-404 (string-match-p "^HTTP/1.1 404 Not Found" r404))
         (starts-301 (string-match-p "^HTTP/1.1 301 Moved Permanently" r301))
         ;; Verify Content-Length present in 200
         (has-cl (string-match-p "Content-Length:" r200))
         ;; Verify body separator
         (has-sep (string-match-p "\r\n\r\n" r200)))
    (list r200 r404 r301 json-r r204
          (numberp starts-200)
          (numberp starts-404)
          (numberp starts-301)
          (numberp has-cl)
          (numberp has-sep))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"HTTP/1.1 200 OK\\r\\nContent-Type: text/html\\r\\nServer: NeoVM/1.0\\r\\nContent-Length: 20\\r\\n\\r\\n<h1>Hello World</h1>\" \"HTTP/1.1 404 Not Found\\r\\nContent-Type: text/plain\\r\\nContent-Length: 9\\r\\n\\r\\nNot Found\" \"HTTP/1.1 301 Moved Permanently\\r\\nLocation: https://example.com/new-page\\r\\nServer: NeoVM/1.0\\r\\n\\r\\n\" \"HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nServer: NeoVM/1.0\\r\\nContent-Length: 29\\r\\n\\r\\n{\\\"status\\\":200,\\\"data\\\":\\\"hello\\\"}\" \"HTTP/1.1 204 No Content\\r\\nServer: NeoVM/1.0\\r\\n\\r\\n\" t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
