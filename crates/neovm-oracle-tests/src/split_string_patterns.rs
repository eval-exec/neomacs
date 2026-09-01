//! Oracle parity tests for `split-string` with ALL parameter combinations:
//! default separator, explicit regex separators, OMIT-NULLS parameter,
//! TRIM parameter, separators at boundaries, consecutive separators,
//! CSV parsing, path splitting, and log line parsing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Default separator (whitespace) behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_default_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When no separator is given, split-string uses "[ \f\t\n\r\v]+"
    // and omit-nulls defaults to t
    let form = r#"(list
  ;; Basic whitespace splitting
  (split-string "hello world")
  ;; Multiple spaces
  (split-string "a   b    c")
  ;; Tabs and newlines
  (split-string "foo\tbar\nbaz")
  ;; Mixed whitespace types
  (split-string "  alpha \t beta \n gamma  ")
  ;; Single word (no whitespace)
  (split-string "onlyone")
  ;; All whitespace (should give empty list since omit-nulls defaults to t)
  (split-string "   \t  \n  ")
  ;; Empty string
  (split-string "")
  ;; Vertical tab and form feed
  (split-string "a\vb\fc"))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\") (\"a\" \"b\" \"c\") (\"foo\" \"bar\" \"baz\") (\"alpha\" \"beta\" \"gamma\") (\"onlyone\") nil nil (\"a\" \"b\" \"c\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Explicit separator regex - various patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_explicit_regex_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Single character separator
  (split-string "a:b:c:d" ":")
  ;; Character class separator
  (split-string "hello123world456foo" "[0-9]+")
  ;; Alternation separator
  (split-string "one,two;three:four" "[,;:]")
  ;; Dot as separator (escaped)
  (split-string "192.168.1.100" "\\.")
  ;; Optional separator (zero-width possible; uses + to avoid)
  (split-string "camelCase" "\\(?:[a-z]\\)\\(\\)")
  ;; Pipe separator
  (split-string "field1|field2|field3" "|")
  ;; Multiple whitespace chars as single separator
  (split-string "one , two ; three" "[ \t]*[,;][ \t]*")
  ;; Newlines only as separator
  (split-string "line1\nline2\nline3" "\n"))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\" \"d\") (\"hello\" \"world\" \"foo\") (\"one\" \"two\" \"three\" \"four\") (\"192\" \"168\" \"1\" \"100\") (\"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\") (\"field1\" \"field2\" \"field3\") (\"one\" \"two\" \"three\") (\"line1\" \"line2\" \"line3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// OMIT-NULLS parameter: nil vs t in all combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_omit_nulls_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Leading separator: nil preserves empty first element
  (split-string ",a,b,c" "," nil)
  (split-string ",a,b,c" "," t)
  ;; Trailing separator
  (split-string "a,b,c," "," nil)
  (split-string "a,b,c," "," t)
  ;; Both leading and trailing
  (split-string ",a,b,c," "," nil)
  (split-string ",a,b,c," "," t)
  ;; Consecutive separators produce empty strings when nil
  (split-string "a,,b,,,c" "," nil)
  (split-string "a,,b,,,c" "," t)
  ;; All separators (no content)
  (split-string ",,," "," nil)
  (split-string ",,," "," t)
  ;; No separator matches: whole string as one element regardless
  (split-string "hello" "," nil)
  (split-string "hello" "," t)
  ;; Regex separator with omit-nulls
  (split-string "::a::b::" ":+" nil)
  (split-string "::a::b::" ":+" t))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\" \"a\" \"b\" \"c\") (\"a\" \"b\" \"c\") (\"a\" \"b\" \"c\" \"\") (\"a\" \"b\" \"c\") (\"\" \"a\" \"b\" \"c\" \"\") (\"a\" \"b\" \"c\") (\"a\" \"\" \"b\" \"\" \"\" \"c\") (\"a\" \"b\" \"c\") (\"\" \"\" \"\" \"\") nil (\"hello\") (\"hello\") (\"\" \"a\" \"b\" \"\") (\"a\" \"b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// TRIM parameter: regex to trim from each element
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_trim_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Trim whitespace from each piece
  (split-string " a , b , c " "," t "[ \t]+")
  ;; Trim specific characters (brackets)
  (split-string "[one]|[two]|[three]" "|" t "[][]")
  ;; Trim digits from edges
  (split-string "123abc456,789def012" "," nil "[0-9]+")
  ;; Trim combined with omit-nulls nil: empty trimmed pieces kept
  (split-string "  , hello ,  , world ,  " "," nil "[ \t]+")
  ;; Trim combined with omit-nulls t: empty trimmed pieces removed
  (split-string "  , hello ,  , world ,  " "," t "[ \t]+")
  ;; Trim quotes from each piece
  (split-string "\"alpha\",\"beta\",\"gamma\"" "," t "\"")
  ;; Trim with regex: remove parenthesized suffixes
  (split-string "foo(1),bar(2),baz(3)" "," t "([0-9]+)")
  ;; Trim that removes everything from some pieces, with omit-nulls
  (split-string "123,abc,456,def" "," t "[0-9]+"))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\") (\"one\" \"two\" \"three\") (\"abc\" \"def\") (\"\" \"hello\" \"\" \"world\" \"\") (\"hello\" \"world\") (\"alpha\" \"beta\" \"gamma\") (\"foo\" \"bar\" \"baz\") (\"abc\" \"def\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Separator at start/end of string, multiple consecutive separators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_boundary_and_consecutive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Only separator at start
  (split-string "/usr/local/bin" "/" nil)
  (split-string "/usr/local/bin" "/" t)
  ;; Only separator at end
  (split-string "usr/local/bin/" "/" nil)
  (split-string "usr/local/bin/" "/" t)
  ;; Both start and end
  (split-string "/usr/local/bin/" "/" nil)
  (split-string "/usr/local/bin/" "/" t)
  ;; Multiple consecutive separators in middle
  (split-string "a///b////c" "/" nil)
  (split-string "a///b////c" "/" t)
  ;; Consecutive regex separators
  (split-string "one---two===three" "[-=]+" t)
  ;; Single char input with separator matching it
  (split-string "x" "x" nil)
  (split-string "x" "x" t)
  ;; Two consecutive separator chars only
  (split-string "::" ":" nil)
  (split-string "::" ":" t))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\" \"usr\" \"local\" \"bin\") (\"usr\" \"local\" \"bin\") (\"usr\" \"local\" \"bin\" \"\") (\"usr\" \"local\" \"bin\") (\"\" \"usr\" \"local\" \"bin\" \"\") (\"usr\" \"local\" \"bin\") (\"a\" \"\" \"\" \"b\" \"\" \"\" \"\" \"c\") (\"a\" \"b\" \"c\") (\"one\" \"two\" \"three\") (\"\" \"\") nil (\"\" \"\" \"\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CSV parsing with split-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_csv_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Parse CSV: split rows, then fields, apply type conversion
  (fset 'neovm--csv-parse-field
    (lambda (field)
      (let ((trimmed (if (string-match "\\`[ \t]*\\(.*?\\)[ \t]*\\'" field)
                         (match-string 1 field)
                       field)))
        (cond
         ((string-match "\\`-?[0-9]+\\'" trimmed)
          (string-to-number trimmed))
         ((string-match "\\`-?[0-9]+\\.[0-9]+\\'" trimmed)
          (string-to-number trimmed))
         ((string= trimmed "true") t)
         ((string= trimmed "false") nil)
         ((string= trimmed "") nil)
         (t trimmed)))))

  (fset 'neovm--csv-parse-row
    (lambda (row)
      (mapcar (lambda (f) (funcall 'neovm--csv-parse-field f))
              (split-string row "," nil))))

  (fset 'neovm--csv-parse
    (lambda (text)
      (let ((rows (split-string text "\n" t "[ \t]+")))
        (mapcar (lambda (r) (funcall 'neovm--csv-parse-row r)) rows))))

  (unwind-protect
      (let ((csv-data "name, age, score, active\nAlice, 30, 95.5, true\nBob, 25, 87.3, false\nCharlie, 35, , true"))
        (list
         ;; Full parse
         (funcall 'neovm--csv-parse csv-data)
         ;; Single row parsing
         (funcall 'neovm--csv-parse-row "hello, 42, 3.14, true")
         ;; Edge: row with trailing comma
         (funcall 'neovm--csv-parse-row "a, b, c,")
         ;; Edge: row with leading comma
         (funcall 'neovm--csv-parse-row ", x, y, z")))
    (fmakunbound 'neovm--csv-parse-field)
    (fmakunbound 'neovm--csv-parse-row)
    (fmakunbound 'neovm--csv-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"name\" \"age\" \"score\" \"active\") (\"Alice\" 30 95.5 t) (\"Bob\" 25 87.3 nil) (\"Charlie\" 35 nil t)) (\"hello\" 42 3.14 t) (\"a\" \"b\" \"c\" nil) (nil \"x\" \"y\" \"z\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: path splitting and manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_path_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Path utilities using split-string
  (fset 'neovm--path-components
    (lambda (path)
      (split-string path "/" t)))

  (fset 'neovm--path-dirname
    (lambda (path)
      (let* ((abs (string-prefix-p "/" path))
             (parts (funcall 'neovm--path-components path))
             (dir-parts (butlast parts)))
        (if dir-parts
            (concat (if abs "/" "") (mapconcat #'identity dir-parts "/"))
          (if abs "/" ".")))))

  (fset 'neovm--path-basename
    (lambda (path)
      (let ((parts (funcall 'neovm--path-components path)))
        (if parts (car (last parts)) ""))))

  (fset 'neovm--path-extension
    (lambda (path)
      (let ((base (funcall 'neovm--path-basename path)))
        (if (string-match "\\.\\([^.]*\\)\\'" base)
            (match-string 1 base)
          nil))))

  (fset 'neovm--path-normalize
    (lambda (path)
      (let* ((abs (string-prefix-p "/" path))
             (parts (funcall 'neovm--path-components path))
             (result nil))
        (dolist (p parts)
          (cond
           ((string= p "."))   ;; skip
           ((string= p "..")
            (when result (setq result (cdr result))))
           (t (setq result (cons p result)))))
        (let ((joined (mapconcat #'identity (nreverse result) "/")))
          (if abs (concat "/" joined) joined)))))

  (unwind-protect
      (list
       ;; Components
       (funcall 'neovm--path-components "/usr/local/bin/emacs")
       (funcall 'neovm--path-components "relative/path/file.txt")
       (funcall 'neovm--path-components "/")
       ;; Dirname
       (funcall 'neovm--path-dirname "/usr/local/bin/emacs")
       (funcall 'neovm--path-dirname "file.txt")
       (funcall 'neovm--path-dirname "/root")
       ;; Basename
       (funcall 'neovm--path-basename "/usr/local/bin/emacs")
       (funcall 'neovm--path-basename "/path/to/file.tar.gz")
       ;; Extension
       (funcall 'neovm--path-extension "file.txt")
       (funcall 'neovm--path-extension "archive.tar.gz")
       (funcall 'neovm--path-extension "no-extension")
       ;; Normalize
       (funcall 'neovm--path-normalize "/usr/local/../bin/./emacs")
       (funcall 'neovm--path-normalize "a/b/../c/./d/../e")
       (funcall 'neovm--path-normalize "/a/b/../../c"))
    (fmakunbound 'neovm--path-components)
    (fmakunbound 'neovm--path-dirname)
    (fmakunbound 'neovm--path-basename)
    (fmakunbound 'neovm--path-extension)
    (fmakunbound 'neovm--path-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"usr\" \"local\" \"bin\" \"emacs\") (\"relative\" \"path\" \"file.txt\") nil \"/usr/local/bin\" \".\" \"/\" \"emacs\" \"file.tar.gz\" \"txt\" \"gz\" nil \"/usr/bin/emacs\" \"a/c/e\" \"/c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: log line parsing (timestamp, level, message)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_log_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Parse structured log lines: "TIMESTAMP LEVEL [COMPONENT] MESSAGE"
  (fset 'neovm--parse-log-line
    (lambda (line)
      (let ((parts nil))
        ;; Extract timestamp (first two whitespace-delimited tokens: date + time)
        (if (string-match "\\`\\([0-9-]+\\)[ \t]+\\([0-9:.]+\\)[ \t]+\\(.*\\)" line)
            (let ((date (match-string 1 line))
                  (time (match-string 2 line))
                  (rest (match-string 3 line)))
              ;; Extract level
              (if (string-match "\\`\\([A-Z]+\\)[ \t]+\\(.*\\)" rest)
                  (let ((level (match-string 1 rest))
                        (rest2 (match-string 2 rest)))
                    ;; Extract optional component in brackets
                    (if (string-match "\\`\\[\\([^]]+\\)\\][ \t]*\\(.*\\)" rest2)
                        (list :date date :time time :level level
                              :component (match-string 1 rest2)
                              :message (match-string 2 rest2))
                      (list :date date :time time :level level
                            :component nil :message rest2)))
                (list :raw line)))
          (list :raw line)))))

  (fset 'neovm--filter-logs
    (lambda (lines level)
      (let ((results nil))
        (dolist (line (split-string lines "\n" t))
          (let ((parsed (funcall 'neovm--parse-log-line line)))
            (when (and (plist-get parsed :level)
                       (string= (plist-get parsed :level) level))
              (setq results (cons parsed results)))))
        (nreverse results))))

  (fset 'neovm--log-summary
    (lambda (lines)
      (let ((counts nil))
        (dolist (line (split-string lines "\n" t))
          (let ((parsed (funcall 'neovm--parse-log-line line)))
            (when (plist-get parsed :level)
              (let* ((level (plist-get parsed :level))
                     (current (assoc level counts)))
                (if current
                    (setcdr current (1+ (cdr current)))
                  (setq counts (cons (cons level 1) counts)))))))
        (sort counts (lambda (a b) (string< (car a) (car b)))))))

  (unwind-protect
      (let ((logs "2024-01-15 10:30:00.123 INFO [server] Started on port 8080
2024-01-15 10:30:01.456 DEBUG [db] Connection pool initialized
2024-01-15 10:30:02.789 WARN [auth] Token expiry approaching
2024-01-15 10:30:03.012 ERROR [handler] Request timeout after 30s
2024-01-15 10:30:04.345 INFO [server] Processing request /api/users
2024-01-15 10:30:05.678 DEBUG [cache] Cache miss for key user:123
2024-01-15 10:30:06.901 ERROR [db] Connection refused"))
        (list
         ;; Parse individual lines
         (funcall 'neovm--parse-log-line
                  "2024-01-15 10:30:00.123 INFO [server] Started on port 8080")
         (funcall 'neovm--parse-log-line
                  "2024-01-15 10:30:03.012 ERROR [handler] Request timeout after 30s")
         ;; Filter by level
         (length (funcall 'neovm--filter-logs logs "ERROR"))
         (length (funcall 'neovm--filter-logs logs "INFO"))
         (length (funcall 'neovm--filter-logs logs "DEBUG"))
         ;; Summary counts
         (funcall 'neovm--log-summary logs)
         ;; Extract messages from errors only
         (mapcar (lambda (parsed) (plist-get parsed :message))
                 (funcall 'neovm--filter-logs logs "ERROR"))
         ;; Extract components from all entries
         (mapcar (lambda (parsed) (plist-get parsed :component))
                 (mapcar (lambda (line) (funcall 'neovm--parse-log-line line))
                         (split-string logs "\n" t)))))
    (fmakunbound 'neovm--parse-log-line)
    (fmakunbound 'neovm--filter-logs)
    (fmakunbound 'neovm--log-summary)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:date \"2024-01-15\" :time \"10:30:00.123\" :level \"INFO\" :component \"server\" :message \"Started on port 8080\") (:date \"2024-01-15\" :time \"10:30:03.012\" :level \"ERROR\" :component \"handler\" :message \"Request timeout after 30s\") 2 2 2 ((\"DEBUG\" . 2) (\"ERROR\" . 2) (\"INFO\" . 2) (\"WARN\" . 1)) (\"Request timeout after 30s\" \"Connection refused\") (\"server\" \"db\" \"auth\" \"handler\" \"server\" \"cache\" \"db\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// All four parameters combined in one test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_all_params_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test every combination of (separator, omit-nulls, trim)
    let form = r#"(let ((input "  [hello] , [world] , [] , [foo]  "))
  (list
    ;; 1-arg: default separator, default omit-nulls
    (split-string "  hello  world  ")
    ;; 2-arg: explicit separator only
    (split-string input ",")
    ;; 3-arg: separator + omit-nulls nil
    (split-string input "," nil)
    ;; 3-arg: separator + omit-nulls t
    (split-string input "," t)
    ;; 4-arg: separator + omit-nulls nil + trim
    (split-string input "," nil "[ \t]+")
    ;; 4-arg: separator + omit-nulls t + trim (whitespace)
    (split-string input "," t "[ \t]+")
    ;; 4-arg: separator + omit-nulls nil + trim (brackets+whitespace)
    (split-string input "," nil "[][ \t]+")
    ;; 4-arg: separator + omit-nulls t + trim (brackets+whitespace)
    (split-string input "," t "[][ \t]+")
    ;; Trim that produces empty strings, then omit-nulls removes them
    (split-string "aaa,bbb,ccc" "," t "[a-c]+")
    ;; Same without omit-nulls: keeps the empty strings
    (split-string "aaa,bbb,ccc" "," nil "[a-c]+")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\") (\"  [hello] \" \" [world] \" \" [] \" \" [foo]  \") (\"  [hello] \" \" [world] \" \" [] \" \" [foo]  \") (\"  [hello] \" \" [world] \" \" [] \" \" [foo]  \") (\"[hello]\" \"[world]\" \"[]\" \"[foo]\") (\"[hello]\" \"[world]\" \"[]\" \"[foo]\") (\"hello\" \"world\" \"\" \"foo\") (\"hello\" \"world\" \"foo\") nil (\"\" \"\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
