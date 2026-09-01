//! Oracle parity tests for `mapconcat` with complex patterns.
//!
//! Tests `mapconcat` with identity and various separators, transformation
//! functions, empty separators, empty sequences, vectors, CSV/TSV building,
//! nested mapconcat for 2D data, and multi-byte string handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// mapconcat with identity and diverse separator strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_identity_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Single-char separators
  (mapconcat #'identity '("a" "b" "c") ",")
  (mapconcat #'identity '("x" "y" "z") ";")
  (mapconcat #'identity '("hello" "world") " ")
  ;; Multi-char separators
  (mapconcat #'identity '("alpha" "beta" "gamma") " :: ")
  (mapconcat #'identity '("one" "two" "three") " ==> ")
  (mapconcat #'identity '("foo" "bar") " AND ")
  ;; Separator with special chars
  (mapconcat #'identity '("line1" "line2" "line3") "\n")
  (mapconcat #'identity '("col1" "col2" "col3") "\t")
  ;; Separator with repeated chars
  (mapconcat #'identity '("start" "end") "---")
  (mapconcat #'identity '("A" "B" "C" "D") "|||")
  ;; Very long separator
  (mapconcat #'identity '("left" "right") " <========> ")
  ;; Single element: separator should not appear
  (mapconcat #'identity '("solo") "XXXXX")
  ;; Two elements: separator appears once
  (mapconcat #'identity '("first" "second") "//"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a,b,c\" \"x;y;z\" \"hello world\" \"alpha :: beta :: gamma\" \"one ==> two ==> three\" \"foo AND bar\" \"line1\\nline2\\nline3\" \"col1\tcol2\tcol3\" \"start---end\" \"A|||B|||C|||D\" \"left <========> right\" \"solo\" \"first//second\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with various transformation functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_transformations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; upcase transformation
  (mapconcat #'upcase '("hello" "world" "test") " ")
  ;; downcase transformation
  (mapconcat #'downcase '("HELLO" "WORLD") "-")
  ;; number-to-string
  (mapconcat #'number-to-string '(1 2 3 4 5 6 7 8 9 10) ",")
  ;; symbol-name
  (mapconcat #'symbol-name '(foo bar baz quux) "/")
  ;; Lambda: wrap in parens
  (mapconcat (lambda (s) (concat "(" s ")")) '("a" "b" "c") " ")
  ;; Lambda: repeat string
  (mapconcat (lambda (s) (concat s s s)) '("ha" "ho") "-")
  ;; Lambda: reverse each string
  (mapconcat (lambda (s)
               (apply #'string (nreverse (string-to-list s))))
             '("abc" "def" "ghi") " ")
  ;; Lambda: conditional transformation
  (mapconcat (lambda (n)
               (if (= (% n 2) 0)
                   (upcase (number-to-string n))
                 (number-to-string n)))
             '(1 2 3 4 5 6) ",")
  ;; Lambda: format with padding
  (mapconcat (lambda (n) (format "%03d" n)) '(1 22 333 4 55) " ")
  ;; Lambda: char-to-string on char codes
  (mapconcat #'char-to-string '(72 101 108 108 111) ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLD TEST\" \"hello-world\" \"1,2,3,4,5,6,7,8,9,10\" \"foo/bar/baz/quux\" \"(a) (b) (c)\" \"hahaha-hohoho\" \"cba fed ihg\" \"1,2,3,4,5,6\" \"001 022 333 004 055\" \"Hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with empty separator (concatenation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_empty_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic concatenation
  (mapconcat #'identity '("a" "b" "c" "d" "e") "")
  ;; Numbers joined without separator
  (mapconcat #'number-to-string '(1 2 3 4 5) "")
  ;; Upcase concatenated
  (mapconcat #'upcase '("h" "e" "l" "l" "o") "")
  ;; Build word from char codes
  (mapconcat #'char-to-string '(69 109 97 99 115) "")
  ;; Single element with empty separator
  (mapconcat #'identity '("alone") "")
  ;; Empty strings with empty separator
  (mapconcat #'identity '("" "" "" "") "")
  ;; Mixed empty and non-empty
  (mapconcat #'identity '("a" "" "b" "" "c") "")
  ;; Result should equal concat of all
  (let ((parts '("foo" "bar" "baz")))
    (equal (mapconcat #'identity parts "")
           (apply #'concat parts)))
  ;; Stateful lambda with empty separator
  (let ((counter 0))
    (mapconcat (lambda (s)
                 (setq counter (1+ counter))
                 (format "%d:%s" counter s))
               '("a" "b" "c") "")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"abcde\" \"12345\" \"HELLO\" \"Emacs\" \"alone\" \"\" \"abc\" t \"1:a2:b3:c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with empty sequences (lists and vectors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_empty_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty list with various separators
  (mapconcat #'identity '() ",")
  (mapconcat #'identity '() "")
  (mapconcat #'identity '() " | ")
  (mapconcat #'identity nil ",")
  ;; Empty vector
  (mapconcat #'identity [] ",")
  (mapconcat #'identity [] "")
  (mapconcat #'identity [] "---")
  ;; Empty list with transformation function
  (mapconcat #'upcase '() " ")
  (mapconcat #'number-to-string nil " + ")
  ;; Result is always empty string
  (string= (mapconcat #'identity nil "any-sep") "")
  (= (length (mapconcat #'identity nil "LONG")) 0)
  (string= (mapconcat #'identity [] ",") "")
  ;; Compare empty list and empty vector results
  (equal (mapconcat #'identity nil ",")
         (mapconcat #'identity [] ",")))"#;
    let expect =
        expect_test::expect![[r#""OK (\"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapconcat with vectors (not just lists)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic vector of strings
  (mapconcat #'identity ["hello" "from" "a" "vector"] " ")
  ;; Vector of numbers
  (mapconcat #'number-to-string [10 20 30 40 50] ", ")
  ;; Vector of symbols
  (mapconcat #'symbol-name ['alpha 'beta 'gamma 'delta] ".")
  ;; Single-element vector
  (mapconcat #'identity ["only"] ",")
  ;; Vector with lambda
  (mapconcat (lambda (n) (make-string n ?#)) [1 3 5 7] " ")
  ;; Vector with format
  (mapconcat (lambda (n) (format "item-%d" n)) [1 2 3 4] "; ")
  ;; Compare vector and list results
  (let ((lst '("a" "b" "c"))
        (vec ["a" "b" "c"]))
    (equal (mapconcat #'identity lst ",")
           (mapconcat #'identity vec ",")))
  ;; Nested: mapconcat over vector of vectors (via lambda)
  (mapconcat (lambda (row)
               (mapconcat #'number-to-string (append row nil) ","))
             '([1 2 3] [4 5 6] [7 8 9])
             "\n")
  ;; Vector with upcase transformation
  (mapconcat #'upcase ["hello" "world" "test" "data"] " "))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 'alpha)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building CSV/TSV data with mapconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_csv_tsv_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; CSV: quote fields containing commas, newlines, or double-quotes
  (fset 'neovm--mc-csv-escape
    (lambda (field)
      (if (or (string-match-p "," field)
              (string-match-p "\n" field)
              (string-match-p "\"" field))
          (concat "\""
                  (let ((result "")
                        (i 0)
                        (len (length field)))
                    (while (< i len)
                      (let ((ch (aref field i)))
                        (if (= ch ?\")
                            (setq result (concat result "\"\""))
                          (setq result (concat result (char-to-string ch)))))
                      (setq i (1+ i)))
                    result)
                  "\"")
        field)))

  (fset 'neovm--mc-csv-row
    (lambda (fields)
      (mapconcat #'neovm--mc-csv-escape fields ",")))

  (fset 'neovm--mc-csv-table
    (lambda (header rows)
      (concat (funcall 'neovm--mc-csv-row header) "\n"
              (mapconcat #'neovm--mc-csv-row rows "\n"))))

  ;; TSV: simply join with tabs (no quoting needed for basic case)
  (fset 'neovm--mc-tsv-row
    (lambda (fields)
      (mapconcat #'identity fields "\t")))

  (fset 'neovm--mc-tsv-table
    (lambda (header rows)
      (concat (funcall 'neovm--mc-tsv-row header) "\n"
              (mapconcat #'neovm--mc-tsv-row rows "\n"))))

  (unwind-protect
      (list
        ;; Simple CSV
        (funcall 'neovm--mc-csv-row '("Alice" "30" "NYC"))
        ;; CSV with comma in field
        (funcall 'neovm--mc-csv-row '("Bob" "25" "Los Angeles, CA"))
        ;; CSV with double-quote in field
        (funcall 'neovm--mc-csv-row '("Charlie" "35" "Said \"hello\""))
        ;; Full CSV table
        (funcall 'neovm--mc-csv-table
                 '("Name" "Age" "City")
                 '(("Alice" "30" "NYC")
                   ("Bob" "25" "LA, CA")
                   ("Charlie" "35" "Chicago")))
        ;; TSV row
        (funcall 'neovm--mc-tsv-row '("Name" "Age" "City"))
        ;; Full TSV table
        (funcall 'neovm--mc-tsv-table
                 '("ID" "Value" "Status")
                 '(("1" "100" "active")
                   ("2" "200" "inactive")
                   ("3" "300" "active")))
        ;; Empty data
        (funcall 'neovm--mc-csv-row '())
        (funcall 'neovm--mc-csv-table '("H1" "H2") '())
        ;; Single column
        (funcall 'neovm--mc-csv-table '("Only") '(("val1") ("val2") ("val3"))))
    (fmakunbound 'neovm--mc-csv-escape)
    (fmakunbound 'neovm--mc-csv-row)
    (fmakunbound 'neovm--mc-csv-table)
    (fmakunbound 'neovm--mc-tsv-row)
    (fmakunbound 'neovm--mc-tsv-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice,30,NYC\" \"Bob,25,\\\"Los Angeles, CA\\\"\" \"Charlie,35,\\\"Said \\\"\\\"hello\\\"\\\"\\\"\" \"Name,Age,City\\nAlice,30,NYC\\nBob,25,\\\"LA, CA\\\"\\nCharlie,35,Chicago\" \"Name\tAge\tCity\" \"ID\tValue\tStatus\\n1\t100\tactive\\n2\t200\tinactive\\n3\t300\tactive\" \"\" \"H1,H2\\n\" \"Only\\nval1\\nval2\\nval3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: nested mapconcat for 2D data formatting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_nested_2d_formatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Matrix display: nested mapconcat for rows and columns
  (fset 'neovm--mc-format-matrix
    (lambda (matrix)
      "Format a matrix (list of lists of numbers) as aligned text."
      ;; Find max width per column
      (let* ((num-cols (length (car matrix)))
             (col-widths (let ((widths nil)
                               (c 0))
                           (while (< c num-cols)
                             (let ((max-w 0))
                               (dolist (row matrix)
                                 (let ((w (length (number-to-string (nth c row)))))
                                   (when (> w max-w) (setq max-w w)))))
                             (setq widths (cons max-w widths))
                             (setq c (1+ c)))
                           (nreverse widths))))
        (mapconcat
         (lambda (row)
           (concat "[ "
                   (let ((cells nil)
                         (i 0))
                     (dolist (val row)
                       (let* ((s (number-to-string val))
                              (w (nth i col-widths))
                              (padded (if w
                                         (concat (make-string (max 0 (- w (length s))) ?\s) s)
                                       s)))
                         (setq cells (cons padded cells)))
                       (setq i (1+ i)))
                     (mapconcat #'identity (nreverse cells) " "))
                   " ]"))
         matrix
         "\n"))))

  ;; Key-value config formatter
  (fset 'neovm--mc-format-config
    (lambda (sections)
      "Format config sections: ((section-name . ((key . val) ...)) ...)"
      (mapconcat
       (lambda (section)
         (concat "[" (car section) "]\n"
                 (mapconcat
                  (lambda (kv)
                    (concat (car kv) " = " (cdr kv)))
                  (cdr section)
                  "\n")))
       sections
       "\n\n")))

  ;; Nested list flattening with mapconcat
  (fset 'neovm--mc-tree-to-string
    (lambda (tree indent)
      "Format a nested list as an indented tree."
      (if (listp tree)
          (if (and (car tree) (not (listp (car tree))))
              ;; Node with children: (label . children)
              (concat (make-string indent ?\s) (symbol-name (car tree)) "\n"
                      (mapconcat
                       (lambda (child)
                         (funcall 'neovm--mc-tree-to-string child (+ indent 2)))
                       (cdr tree)
                       ""))
            ;; Just children
            (mapconcat
             (lambda (child)
               (funcall 'neovm--mc-tree-to-string child indent))
             tree
             ""))
        ;; Leaf
        (concat (make-string indent ?\s) (symbol-name tree) "\n"))))

  (unwind-protect
      (list
        ;; 3x3 matrix
        (funcall 'neovm--mc-format-matrix
                 '((1 2 3) (4 5 6) (7 8 9)))
        ;; Matrix with varying widths
        (funcall 'neovm--mc-format-matrix
                 '((1 100 3) (44 5 666) (7 88 9)))
        ;; Config file
        (funcall 'neovm--mc-format-config
                 '(("server" . (("host" . "localhost")
                                ("port" . "8080")
                                ("debug" . "true")))
                   ("database" . (("driver" . "postgres")
                                  ("name" . "mydb")))))
        ;; Tree display
        (funcall 'neovm--mc-tree-to-string
                 '(root (child1 leaf1 leaf2) (child2 leaf3 (child3 leaf4 leaf5)))
                 0)
        ;; 1x1 matrix
        (funcall 'neovm--mc-format-matrix '((42)))
        ;; Single-section config
        (funcall 'neovm--mc-format-config
                 '(("main" . (("key" . "value"))))))
    (fmakunbound 'neovm--mc-format-matrix)
    (fmakunbound 'neovm--mc-format-config)
    (fmakunbound 'neovm--mc-tree-to-string)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable max-w)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: mapconcat with multi-byte strings and complex separators
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mapconcat_patterns_multibyte_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Multi-byte strings as elements
  (mapconcat #'identity '("\u4e16\u754c" "\u4f60\u597d" "\u4e2d\u6587") ", ")
  ;; Multi-byte separator
  (mapconcat #'identity '("hello" "world") " \u2192 ")
  ;; Both multi-byte elements and separator
  (mapconcat #'identity '("\u6771\u4eac" "\u5927\u962a" "\u4eac\u90fd") "\u30fb")
  ;; Lambda producing multi-byte output
  (mapconcat (lambda (s) (concat "\u300c" s "\u300d"))
             '("one" "two" "three")
             ", ")
  ;; symbol-name with multi-byte separator
  (mapconcat #'symbol-name '(alpha beta gamma) " \u2022 ")
  ;; Mixing ASCII and CJK in elements
  (mapconcat #'identity
             '("Name:\u5f20\u4e09" "Age:25" "City:\u5317\u4eac")
             " | ")
  ;; Multi-byte in number formatting context
  (mapconcat (lambda (n) (concat (number-to-string n) "\u5206"))
             '(100 200 300)
             ", ")
  ;; Accented characters
  (mapconcat #'identity '("\u00e9t\u00e9" "caf\u00e9" "na\u00efve") " / ")
  ;; Greek letters
  (mapconcat #'identity '("\u03b1" "\u03b2" "\u03b3" "\u03b4") " + ")
  ;; Build a multi-byte path
  (mapconcat #'identity '("\u6587\u4ef6" "\u76ee\u5f55" "\u540d\u79f0") "/")
  ;; Verify length vs string-width for multi-byte results
  (let ((result (mapconcat #'identity '("\u4e16\u754c" "\u4f60\u597d") " ")))
    (list (length result) (string-width result))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"世界, 你好, 中文\" \"hello → world\" \"東京・大阪・京都\" \"「one」, 「two」, 「three」\" \"alpha • beta • gamma\" \"Name:张三 | Age:25 | City:北京\" \"100分, 200分, 300分\" \"été / café / naïve\" \"α + β + γ + δ\" \"文件/目录/名称\" (5 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
