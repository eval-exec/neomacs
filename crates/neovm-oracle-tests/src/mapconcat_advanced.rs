//! Advanced oracle parity tests for `mapconcat`.
//!
//! Tests `mapconcat` with various separators, identity, complex lambdas,
//! vectors, empty inputs, and complex string-building patterns (SQL-like
//! queries, HTML-like markup).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// mapconcat with various separator strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (mapconcat #'symbol-name '(foo bar baz) ", ")
  (mapconcat #'symbol-name '(foo bar baz) " | ")
  (mapconcat #'symbol-name '(foo bar baz) " -> ")
  (mapconcat #'symbol-name '(foo bar baz) "\n")
  (mapconcat #'symbol-name '(foo bar baz) " AND ")
  (mapconcat #'number-to-string '(1 2 3 4 5) " + ")
  (mapconcat #'number-to-string '(10 20 30) "---"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"foo, bar, baz\" \"foo | bar | baz\" \"foo -> bar -> baz\" \"foo\\nbar\\nbaz\" \"foo AND bar AND baz\" \"1 + 2 + 3 + 4 + 5\" \"10---20---30\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with identity function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (mapconcat #'identity '("hello" "world") " ")
  (mapconcat #'identity '("a" "b" "c" "d") "")
  (mapconcat #'identity '("one") ", ")
  (mapconcat #'identity '("alpha" "beta" "gamma" "delta") "::")
  ;; Single element: no separator at all
  (mapconcat #'identity '("solo") "|||")
  ;; Multi-char strings
  (mapconcat #'identity '("the" "quick" "brown" "fox") " "))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"abcd\" \"one\" \"alpha::beta::gamma::delta\" \"solo\" \"the quick brown fox\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with complex transformation lambdas
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_complex_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Wrap each element in brackets
  (mapconcat (lambda (s) (concat "[" s "]")) '("a" "b" "c") ", ")
  ;; Transform numbers to padded hex strings
  (mapconcat (lambda (n) (format "%04X" n)) '(10 255 4096 65535) " ")
  ;; Apply string transformations
  (mapconcat (lambda (s) (upcase (substring s 0 1))) '("hello" "world" "test") "")
  ;; Conditional transformation
  (mapconcat (lambda (n)
               (cond
                 ((= (% n 15) 0) "FizzBuzz")
                 ((= (% n 3) 0) "Fizz")
                 ((= (% n 5) 0) "Buzz")
                 (t (number-to-string n))))
             '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)
             ", ")
  ;; Repeat each character
  (mapconcat (lambda (s) (concat s s)) '("ha" "ho" "he") "-"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[a], [b], [c]\" \"000A 00FF 1000 FFFF\" \"HWT\" \"1, 2, Fizz, 4, Buzz, Fizz, 7, 8, Fizz, Buzz, 11, Fizz, 13, 14, FizzBuzz\" \"haha-hoho-hehe\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat over vectors (not just lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Vector of strings
  (mapconcat #'identity ["hello" "from" "vector"] " ")
  ;; Vector of numbers
  (mapconcat #'number-to-string [1 2 3 4 5] ", ")
  ;; Vector of symbols
  (mapconcat #'symbol-name ['alpha 'beta 'gamma] "/")
  ;; Mixed with lambda
  (mapconcat (lambda (n) (make-string n ?*)) [1 3 5 3 1] " ")
  ;; Empty vector
  (mapconcat #'identity [] ", ")
  ;; Single-element vector
  (mapconcat #'number-to-string [42] "---"))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 'alpha)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with empty separator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_empty_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Concatenate without separator
  (mapconcat #'identity '("a" "b" "c") "")
  (mapconcat #'identity '("hello" " " "world") "")
  ;; Build a string char by char
  (mapconcat #'char-to-string '(72 101 108 108 111) "")
  ;; Number digits
  (mapconcat #'number-to-string '(1 2 3 4 5) "")
  ;; upcase each word
  (mapconcat #'upcase '("foo" "bar" "baz") "")
  ;; Alternating case
  (let ((idx -1))
    (mapconcat (lambda (s)
                 (setq idx (1+ idx))
                 (if (= (% idx 2) 0) (upcase s) (downcase s)))
               '("a" "B" "c" "D" "e")
               "")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"abc\" \"hello world\" \"Hello\" \"12345\" \"FOOBARBAZ\" \"AbCdE\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with empty list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_empty_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (mapconcat #'identity '() ", ")
  (mapconcat #'identity '() "")
  (mapconcat #'identity nil " ")
  (mapconcat #'number-to-string nil "-")
  (mapconcat #'identity [] "X")
  ;; Empty result should be empty string
  (string= (mapconcat #'identity nil ", ") "")
  (length (mapconcat #'identity nil "LONG-SEP")))"#;
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" t 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build SQL-like query strings using mapconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_sql_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; SQL query builder using mapconcat
  (fset 'neovm--sql-select
    (lambda (columns table conditions order-by limit)
      (let ((parts nil))
        ;; SELECT clause
        (setq parts
              (cons (concat "SELECT "
                            (mapconcat #'identity columns ", "))
                    parts))
        ;; FROM clause
        (setq parts (cons (concat "FROM " table) parts))
        ;; WHERE clause
        (when conditions
          (setq parts
                (cons (concat "WHERE "
                              (mapconcat
                               (lambda (cond)
                                 (concat (car cond) " " (cadr cond) " " (caddr cond)))
                               conditions
                               " AND "))
                      parts)))
        ;; ORDER BY
        (when order-by
          (setq parts
                (cons (concat "ORDER BY "
                              (mapconcat
                               (lambda (o) (concat (car o) " " (cdr o)))
                               order-by
                               ", "))
                      parts)))
        ;; LIMIT
        (when limit
          (setq parts (cons (concat "LIMIT " (number-to-string limit)) parts)))
        (mapconcat #'identity (nreverse parts) " "))))

  ;; INSERT builder
  (fset 'neovm--sql-insert
    (lambda (table columns values)
      (concat "INSERT INTO " table
              " (" (mapconcat #'identity columns ", ") ")"
              " VALUES (" (mapconcat (lambda (v)
                                       (if (stringp v)
                                           (concat "'" v "'")
                                         (number-to-string v)))
                                     values ", ")
              ")")))

  (unwind-protect
      (list
        ;; Simple select
        (funcall 'neovm--sql-select
                 '("name" "age" "email")
                 "users"
                 nil nil nil)
        ;; Select with conditions
        (funcall 'neovm--sql-select
                 '("*")
                 "products"
                 '(("price" ">" "100") ("category" "=" "'electronics'"))
                 nil nil)
        ;; Full query
        (funcall 'neovm--sql-select
                 '("id" "name" "score")
                 "students"
                 '(("score" ">=" "90") ("active" "=" "1"))
                 '(("score" . "DESC") ("name" . "ASC"))
                 10)
        ;; Insert
        (funcall 'neovm--sql-insert
                 "users"
                 '("name" "age" "city")
                 '("Alice" 30 "NYC"))
        ;; Empty conditions
        (funcall 'neovm--sql-select
                 '("count(*)")
                 "logs"
                 nil nil nil))
    (fmakunbound 'neovm--sql-select)
    (fmakunbound 'neovm--sql-insert)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"SELECT name, age, email FROM users\" \"SELECT * FROM products WHERE price > 100 AND category = 'electronics'\" \"SELECT id, name, score FROM students WHERE score >= 90 AND active = 1 ORDER BY score DESC, name ASC LIMIT 10\" \"INSERT INTO users (name, age, city) VALUES ('Alice', 30, 'NYC')\" \"SELECT count(*) FROM logs\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: generate HTML-like markup using nested mapconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_html_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; HTML tag builder
  (fset 'neovm--html-tag
    (lambda (tag attrs content)
      (let ((attr-str
             (if attrs
                 (concat " "
                         (mapconcat
                          (lambda (a) (concat (car a) "=\"" (cdr a) "\""))
                          attrs
                          " "))
               "")))
        (concat "<" tag attr-str ">" content "</" tag ">"))))

  ;; Table builder
  (fset 'neovm--html-table
    (lambda (headers rows)
      (let* ((header-row
              (funcall 'neovm--html-tag "tr" nil
                       (mapconcat
                        (lambda (h) (funcall 'neovm--html-tag "th" nil h))
                        headers
                        "")))
             (body-rows
              (mapconcat
               (lambda (row)
                 (funcall 'neovm--html-tag "tr" nil
                          (mapconcat
                           (lambda (cell) (funcall 'neovm--html-tag "td" nil cell))
                           row
                           "")))
               rows
               "")))
        (funcall 'neovm--html-tag "table"
                 '(("class" . "data-table"))
                 (concat header-row body-rows)))))

  ;; List builder
  (fset 'neovm--html-list
    (lambda (items ordered)
      (let ((tag (if ordered "ol" "ul")))
        (funcall 'neovm--html-tag tag nil
                 (mapconcat
                  (lambda (item) (funcall 'neovm--html-tag "li" nil item))
                  items
                  "")))))

  (unwind-protect
      (list
        ;; Simple tag
        (funcall 'neovm--html-tag "p" nil "Hello World")
        ;; Tag with attributes
        (funcall 'neovm--html-tag "a"
                 '(("href" . "https://example.com") ("class" . "link"))
                 "Click here")
        ;; Unordered list
        (funcall 'neovm--html-list '("Apple" "Banana" "Cherry") nil)
        ;; Ordered list
        (funcall 'neovm--html-list '("First" "Second" "Third") t)
        ;; Table
        (funcall 'neovm--html-table
                 '("Name" "Age" "City")
                 '(("Alice" "30" "NYC")
                   ("Bob" "25" "LA")
                   ("Charlie" "35" "Chicago")))
        ;; Nested: list inside a div
        (funcall 'neovm--html-tag "div"
                 '(("id" . "content"))
                 (concat
                  (funcall 'neovm--html-tag "h1" nil "Title")
                  (funcall 'neovm--html-list '("Item A" "Item B") nil)))
        ;; Navigation bar using mapconcat
        (let ((nav-items '(("/" . "Home") ("/about" . "About") ("/contact" . "Contact"))))
          (funcall 'neovm--html-tag "nav" nil
                   (mapconcat
                    (lambda (item)
                      (funcall 'neovm--html-tag "a"
                               (list (cons "href" (car item)))
                               (cdr item)))
                    nav-items
                    " | "))))
    (fmakunbound 'neovm--html-tag)
    (fmakunbound 'neovm--html-table)
    (fmakunbound 'neovm--html-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"<p>Hello World</p>\" \"<a href=\\\"https://example.com\\\" class=\\\"link\\\">Click here</a>\" \"<ul><li>Apple</li><li>Banana</li><li>Cherry</li></ul>\" \"<ol><li>First</li><li>Second</li><li>Third</li></ol>\" \"<table class=\\\"data-table\\\"><tr><th>Name</th><th>Age</th><th>City</th></tr><tr><td>Alice</td><td>30</td><td>NYC</td></tr><tr><td>Bob</td><td>25</td><td>LA</td></tr><tr><td>Charlie</td><td>35</td><td>Chicago</td></tr></table>\" \"<div id=\\\"content\\\"><h1>Title</h1><ul><li>Item A</li><li>Item B</li></ul></div>\" \"<nav><a href=\\\"/\\\">Home</a> | <a href=\\\"/about\\\">About</a> | <a href=\\\"/contact\\\">Contact</a></nav>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with number-to-string and format for path building
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_path_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build file paths
  (fset 'neovm--build-path
    (lambda (components) (mapconcat #'identity components "/")))

  ;; Build key-value query strings
  (fset 'neovm--build-query
    (lambda (pairs)
      (mapconcat (lambda (p)
                   (concat (car p) "=" (cdr p)))
                 pairs
                 "&")))

  ;; CSV row builder
  (fset 'neovm--build-csv-row
    (lambda (fields)
      (mapconcat (lambda (f)
                   (if (string-match-p "," f)
                       (concat "\"" f "\"")
                     f))
                 fields
                 ",")))

  (unwind-protect
      (list
        (funcall 'neovm--build-path '("home" "user" "documents" "file.txt"))
        (funcall 'neovm--build-path '(""))
        (funcall 'neovm--build-path '("root"))
        (funcall 'neovm--build-query
                 '(("name" . "alice") ("age" . "30") ("city" . "nyc")))
        (funcall 'neovm--build-query '(("q" . "hello world")))
        (funcall 'neovm--build-csv-row '("Alice" "30" "New York, NY" "active"))
        (funcall 'neovm--build-csv-row '("simple" "row" "here"))
        ;; Nested mapconcat: CSV with multiple rows
        (mapconcat (lambda (row) (funcall 'neovm--build-csv-row row))
                   '(("Name" "Age" "City")
                     ("Alice" "30" "NYC")
                     ("Bob" "25" "LA, CA"))
                   "\n"))
    (fmakunbound 'neovm--build-path)
    (fmakunbound 'neovm--build-query)
    (fmakunbound 'neovm--build-csv-row)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"home/user/documents/file.txt\" \"\" \"root\" \"name=alice&age=30&city=nyc\" \"q=hello world\" \"Alice,30,\\\"New York, NY\\\",active\" \"simple,row,here\" \"Name,Age,City\\nAlice,30,NYC\\nBob,25,\\\"LA, CA\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
