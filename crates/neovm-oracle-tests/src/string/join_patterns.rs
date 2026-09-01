//! Oracle parity tests for `string-join` with complex patterns:
//! separator variations, empty inputs, single-element lists,
//! multi-character separators, path building, CSV/TSV construction,
//! nested joins for 2D data, and roundtrip split-then-join.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic string-join with various separators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_basic_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Comma separator
  (string-join '("alpha" "beta" "gamma") ",")
  ;; Space separator
  (string-join '("hello" "beautiful" "world") " ")
  ;; Dash separator
  (string-join '("2026" "03" "02") "-")
  ;; Slash separator (path-like)
  (string-join '("usr" "local" "bin") "/")
  ;; Dot separator (version-like)
  (string-join '("1" "2" "3") ".")
  ;; Pipe separator
  (string-join '("field1" "field2" "field3") "|")
  ;; Colon separator
  (string-join '("/usr/bin" "/usr/local/bin" "/home/user/bin") ":")
  ;; Semicolon separator
  (string-join '("a=1" "b=2" "c=3") ";"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"alpha,beta,gamma\" \"hello beautiful world\" \"2026-03-02\" \"usr/local/bin\" \"1.2.3\" \"field1|field2|field3\" \"/usr/bin:/usr/local/bin:/home/user/bin\" \"a=1;b=2;c=3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty separator (concatenation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_empty_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty separator concatenates
  (string-join '("a" "b" "c") "")
  ;; Concatenate longer strings
  (string-join '("foo" "bar" "baz") "")
  ;; Concatenate single chars
  (string-join '("h" "e" "l" "l" "o") "")
  ;; Concatenate mixed-length strings
  (string-join '("" "a" "" "bc" "" "def" "") "")
  ;; Single element with empty separator
  (string-join '("only") "")
  ;; Two elements with empty separator
  (string-join '("left" "right") "")
  ;; Empty strings only with empty separator
  (string-join '("" "" "" "") ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"abc\" \"foobarbaz\" \"hello\" \"abcdef\" \"only\" \"leftright\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty list and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_empty_list_and_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty list returns empty string
  (string-join nil ",")
  (string-join nil "")
  (string-join nil "---")
  ;; Single-element list: separator is not used
  (string-join '("alone") ",")
  (string-join '("alone") "|||")
  (string-join '("alone") "")
  ;; Two-element list: one separator occurrence
  (string-join '("first" "second") " and ")
  ;; Verify return type is string
  (stringp (string-join nil ","))
  (stringp (string-join '("x") ","))
  ;; Length checks
  (length (string-join nil ","))
  (length (string-join '("ab" "cd") "-")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"\" \"\" \"alone\" \"alone\" \"alone\" \"first and second\" t t 0 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-character separators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_multi_char_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Arrow separator
  (string-join '("start" "middle" "end") " -> ")
  ;; Double-arrow separator
  (string-join '("A" "B" "C") " => ")
  ;; Comma-space (common formatting)
  (string-join '("apples" "oranges" "bananas") ", ")
  ;; Newline separator
  (string-join '("line1" "line2" "line3") "\n")
  ;; Tab separator
  (string-join '("col1" "col2" "col3") "\t")
  ;; HTML break tag
  (string-join '("para1" "para2" "para3") "<br>")
  ;; Long separator
  (string-join '("X" "Y" "Z") "---separator---")
  ;; Separator with special chars
  (string-join '("a" "b" "c") " & ")
  ;; Repeated char separator
  (string-join '("1" "2" "3") ":::"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"start -> middle -> end\" \"A => B => C\" \"apples, oranges, bananas\" \"line1\\nline2\\nline3\" \"col1\tcol2\tcol3\" \"para1<br>para2<br>para3\" \"X---separator---Y---separator---Z\" \"a & b & c\" \"1:::2:::3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building file paths with string-join
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_path_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Build absolute Unix paths
  (let ((path (concat "/" (string-join '("home" "user" "documents" "file.txt") "/"))))
    (setq results (cons path results)))

  ;; Build relative path
  (let ((path (string-join '("src" "main" "java" "App.java") "/")))
    (setq results (cons path results)))

  ;; Build path from dynamically computed components
  (let* ((base-parts '("var" "log"))
         (app "myapp")
         (file "error.log"))
    (setq results (cons (concat "/" (string-join (append base-parts (list app file)) "/")) results)))

  ;; Roundtrip: split a path and rebuild it
  (let* ((original "/usr/local/share/emacs/site-lisp")
         (parts (split-string original "/" t))
         (rebuilt (concat "/" (string-join parts "/"))))
    (setq results (cons (string= original rebuilt) results))
    (setq results (cons rebuilt results)))

  ;; Build classpath-like string
  (let ((jars '("/lib/a.jar" "/lib/b.jar" "/lib/c.jar")))
    (setq results (cons (string-join jars ":") results)))

  ;; Build URL from components
  (let* ((scheme "https")
         (host "example.com")
         (path-parts '("api" "v2" "users"))
         (url (concat scheme "://" host "/" (string-join path-parts "/"))))
    (setq results (cons url results)))

  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"/home/user/documents/file.txt\" \"src/main/java/App.java\" \"/var/log/myapp/error.log\" t \"/usr/local/share/emacs/site-lisp\" \"/lib/a.jar:/lib/b.jar:/lib/c.jar\" \"https://example.com/api/v2/users\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CSV/TSV row construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_csv_tsv_construction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Simple CSV row
  (setq results (cons (string-join '("name" "age" "city") ",") results))

  ;; CSV with data
  (setq results (cons (string-join '("Alice" "30" "NYC") ",") results))
  (setq results (cons (string-join '("Bob" "25" "LA") ",") results))

  ;; TSV row (tab-separated)
  (setq results (cons (string-join '("id" "value" "timestamp") "\t") results))

  ;; Build multiple CSV rows from data and join with newline
  (let* ((header '("product" "price" "qty"))
         (row1 '("Widget" "9.99" "100"))
         (row2 '("Gadget" "19.99" "50"))
         (row3 '("Doohickey" "4.99" "200"))
         (csv-rows (list (string-join header ",")
                         (string-join row1 ",")
                         (string-join row2 ",")
                         (string-join row3 ",")))
         (full-csv (string-join csv-rows "\n")))
    (setq results (cons full-csv results)))

  ;; Build pipe-delimited format (like many database exports)
  (let ((fields (mapcar (lambda (n) (format "%03d" n)) '(1 5 10 42 100))))
    (setq results (cons (string-join fields "|") results)))

  ;; Construct a header+separator+rows table
  (let* ((cols '("Name" "Score" "Grade"))
         (header (string-join cols " | "))
         (sep (string-join (mapcar (lambda (c) (make-string (length c) ?-)) cols) "-+-"))
         (row1 (string-join '("Alice" "95   " "A    ") " | "))
         (table (string-join (list header sep row1) "\n")))
    (setq results (cons table results)))

  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"name,age,city\" \"Alice,30,NYC\" \"Bob,25,LA\" \"id\tvalue\ttimestamp\" \"product,price,qty\\nWidget,9.99,100\\nGadget,19.99,50\\nDoohickey,4.99,200\" \"001|005|010|042|100\" \"Name | Score | Grade\\n-----+-------+------\\nAlice | 95    | A    \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: nested string-join for 2D data (matrix/table)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_nested_2d_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; 2D grid: join inner rows with comma, outer with newline
  (let* ((matrix '(("1" "2" "3")
                   ("4" "5" "6")
                   ("7" "8" "9")))
         (rows (mapcar (lambda (row) (string-join row ",")) matrix))
         (grid (string-join rows "\n")))
    (setq results (cons grid results)))

  ;; Build a multiplication table snippet (3x3)
  (let* ((table nil))
    (dotimes (i 3)
      (let ((row nil))
        (dotimes (j 3)
          (setq row (cons (format "%2d" (* (1+ i) (1+ j))) row)))
        (setq table (cons (string-join (nreverse row) " ") table))))
    (setq results (cons (string-join (nreverse table) "\n") results)))

  ;; Build key=value pairs per section, then join sections
  (let* ((section1 (string-join '("host=localhost" "port=5432") "&"))
         (section2 (string-join '("user=admin" "pass=secret") "&"))
         (config (string-join (list section1 section2) "\n")))
    (setq results (cons config results)))

  ;; Nested JSON-like structure (manual)
  (let* ((items (mapcar (lambda (pair)
                          (format "\"%s\": \"%s\"" (car pair) (cdr pair)))
                        '(("name" . "test") ("version" . "1.0") ("lang" . "elisp"))))
         (json-body (string-join items ", "))
         (json (concat "{" json-body "}")))
    (setq results (cons json results)))

  ;; Transpose a matrix then join
  (let* ((m '(("a" "b") ("c" "d") ("e" "f")))
         ;; Transpose: columns become rows
         (t1 (mapcar (lambda (i) (mapcar (lambda (row) (nth i row)) m))
                     '(0 1)))
         (original (string-join (mapcar (lambda (r) (string-join r ",")) m) ";"))
         (transposed (string-join (mapcar (lambda (r) (string-join r ",")) t1) ";")))
    (setq results (cons (list original transposed) results)))

  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"1,2,3\\n4,5,6\\n7,8,9\" \" 1  2  3\\n 2  4  6\\n 3  6  9\" \"host=localhost&port=5432\\nuser=admin&pass=secret\" \"{\\\"name\\\": \\\"test\\\", \\\"version\\\": \\\"1.0\\\", \\\"lang\\\": \\\"elisp\\\"}\" (\"a,b;c,d;e,f\" \"a,c,e;b,d,f\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: roundtrip and composition with split-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_roundtrip_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Roundtrip: split then join with same separator
  (string-join (split-string "a,b,c" ",") ",")
  (string-join (split-string "hello world foo" " ") " ")
  (string-join (split-string "x::y::z" "::") "::")

  ;; Split on one separator, join on another (transform)
  (string-join (split-string "a.b.c.d" "\\.") "/")
  (string-join (split-string "one two three" " ") "-")
  (string-join (split-string "key=value" "=") ": ")

  ;; Join result of mapcar over split
  (string-join (mapcar #'upcase (split-string "hello world" " ")) "_")
  (string-join (mapcar (lambda (s) (concat "[" s "]")) (split-string "a,b,c" ",")) " ")

  ;; Chain: split, filter, join
  (string-join (delq nil (mapcar (lambda (s)
                                   (if (> (length s) 2) s nil))
                                 (split-string "a,bb,ccc,dd,eeee" ",")))
               ",")

  ;; Reverse words
  (string-join (nreverse (split-string "one two three four" " ")) " ")

  ;; Number list to formatted string
  (string-join (mapcar #'number-to-string '(1 2 3 4 5)) ", "))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a,b,c\" \"hello world foo\" \"x::y::z\" \"a/b/c/d\" \"one-two-three\" \"key: value\" \"HELLO_WORLD\" \"[a] [b] [c]\" \"ccc,eeee\" \"four three two one\" \"1, 2, 3, 4, 5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: strings containing separator characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_join_separator_in_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Elements contain the separator character (no escaping in string-join)
  (string-join '("a,b" "c,d" "e,f") ",")
  ;; Elements contain spaces, joined by space
  (string-join '("hello world" "foo bar") " ")
  ;; Elements contain newlines, joined by newline
  (string-join '("line1\nline2" "line3\nline4") "\n")
  ;; Empty strings interspersed
  (string-join '("" "a" "" "b" "") ",")
  ;; All empty strings
  (string-join '("" "" "") ",")
  ;; Very long list
  (let ((elts nil))
    (dotimes (i 20)
      (setq elts (cons (format "e%d" i) elts)))
    (string-join (nreverse elts) ","))
  ;; Single character elements
  (string-join '("a" "b" "c" "d" "e" "f" "g") ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a,b,c,d,e,f\" \"hello world foo bar\" \"line1\\nline2\\nline3\\nline4\" \",a,,b,\" \",,\" \"e0,e1,e2,e3,e4,e5,e6,e7,e8,e9,e10,e11,e12,e13,e14,e15,e16,e17,e18,e19\" \"abcdefg\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
