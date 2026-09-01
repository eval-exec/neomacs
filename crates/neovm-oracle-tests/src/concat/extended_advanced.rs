//! Advanced oracle parity tests for concat and string building patterns.
//!
//! Tests concat with 0/1/many args, mixed types (strings, lists of chars,
//! vectors of chars), concat vs mapconcat, incremental vs batch building,
//! string interning behavior, string builder with undo, and template
//! string expansion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// concat with 0 args, 1 arg, many args (10+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concat_arity_variations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; 0 args
      (concat)
      ;; 1 arg
      (concat "solo")
      ;; 2 args
      (concat "ab" "cd")
      ;; 5 args
      (concat "a" "b" "c" "d" "e")
      ;; 10 args
      (concat "1" "2" "3" "4" "5" "6" "7" "8" "9" "0")
      ;; 15 args
      (concat "a" "b" "c" "d" "e" "f" "g" "h" "i" "j" "k" "l" "m" "n" "o")
      ;; length verification
      (length (concat "abc" "defgh" "ij"))
      ;; empty string args mixed in
      (concat "" "a" "" "b" "" "c" "")
      ;; all empty
      (concat "" "" "" ""))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"solo\" \"abcd\" \"abcde\" \"1234567890\" \"abcdefghijklmno\" 10 \"abc\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// concat with mixed types: strings, lists of chars, vectors of chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concat_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; list of chars -> string
      (concat '(104 101 108 108 111))
      ;; vector of chars -> string
      (concat [119 111 114 108 100])
      ;; string + list of chars
      (concat "hello " '(119 111 114 108 100))
      ;; string + vector of chars
      (concat "foo" [98 97 114])
      ;; list + vector + string
      (concat '(65 66) [67 68] "EF")
      ;; multiple lists
      (concat '(72 73) '(32) '(74 75))
      ;; multiple vectors
      (concat [76 77] [78 79])
      ;; single char list
      (concat '(42))
      ;; empty list/vector
      (concat '() [] "test" '() []))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"world\" \"hello world\" \"foobar\" \"ABCDEF\" \"HI JK\" \"LMNO\" \"*\" \"test\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_concat_bool_vector_rejected_as_sequence_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/fns.c:concat_to_string accepts strings, vectors, nil, and
    // conses as character sequences.  A bool-vector is rejected at the argument
    // kind check with `sequencep`, while `vconcat` and `append` still accept it.
    let form = r#"
(let ((bv #&3"\5"))
  (list
   (condition-case err
       (concat bv)
     (error (list (car err) (cdr err))))
   (vconcat bv)
   (append bv nil)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (sequencep #&3\"\u{5}\")) [t nil t] (t nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_char_table_is_not_sequence_for_concat_vconcat_append_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/fns.c:concat_to_string, concat_to_vector, and
    // concat_to_list all reject char-tables as non-sequences.
    let form = r#"
(let ((table (make-char-table nil)))
  (list
   (condition-case err
       (concat table)
     (error (list (car err) (cdr err))))
   (condition-case err
       (vconcat table)
     (error (list (car err) (cdr err))))
   (condition-case err
       (append table nil)
     (error (list (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (sequencep #^[nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil])) (wrong-type-argument (sequencep #^[nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil])) (wrong-type-argument (sequencep #^[nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil])))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// concat vs mapconcat for joining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concat_vs_mapconcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((words '("hello" "world" "foo" "bar" "baz"))
             (numbers '(1 2 3 4 5))
             ;; mapconcat with various separators
             (joined-space (mapconcat 'identity words " "))
             (joined-comma (mapconcat 'identity words ", "))
             (joined-none (mapconcat 'identity words ""))
             (joined-dash (mapconcat 'identity words "-"))
             ;; mapconcat with transformation
             (nums-str (mapconcat 'number-to-string numbers "+"))
             ;; mapconcat with lambda
             (wrapped (mapconcat (lambda (w) (concat "[" w "]")) words " "))
             ;; compare: manual concat loop vs mapconcat
             (manual (let ((result ""))
                       (let ((first t))
                         (dolist (w words)
                           (if first
                               (setq first nil)
                             (setq result (concat result ":")))
                           (setq result (concat result w))))
                       result))
             (via-mapconcat (mapconcat 'identity words ":")))
        (list
          joined-space joined-comma joined-none joined-dash
          nums-str wrapped
          (string-equal manual via-mapconcat)
          manual via-mapconcat))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world foo bar baz\" \"hello, world, foo, bar, baz\" \"helloworldfoobarbaz\" \"hello-world-foo-bar-baz\" \"1+2+3+4+5\" \"[hello] [world] [foo] [bar] [baz]\" t \"hello:world:foo:bar:baz\" \"hello:world:foo:bar:baz\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building strings incrementally vs concat at once
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_incremental_vs_batch_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((parts '("the" " " "quick" " " "brown" " " "fox"))
             ;; Method 1: incremental concat in loop
             (incremental
              (let ((result ""))
                (dolist (p parts)
                  (setq result (concat result p)))
                result))
             ;; Method 2: apply concat all at once
             (batch (apply 'concat parts))
             ;; Method 3: format-based building
             (via-format (format "%s %s %s %s" "the" "quick" "brown" "fox"))
             ;; Method 4: mapconcat on individual chars to rebuild
             (char-by-char
              (let ((s "hello"))
                (mapconcat 'char-to-string (append s nil) "")))
             ;; Method 5: building from number-to-string
             (number-parts
              (let ((result ""))
                (dotimes (i 10)
                  (setq result (concat result (number-to-string i) " ")))
                result))
             ;; Verify equivalence
             (eq-check (string-equal incremental batch)))
        (list incremental batch via-format char-by-char number-parts eq-check))"#;
    let expect = expect_test::expect![[
        r#""OK (\"the quick brown fox\" \"the quick brown fox\" \"the quick brown fox\" \"hello\" \"0 1 2 3 4 5 6 7 8 9 \" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String interning: concat always creates new strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_concat_identity_and_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // concat always returns a new string (not eq to inputs), but equal content
    let form = r#"(let* ((a "hello")
             (b "hello")
             (c (concat "hel" "lo"))
             (d (concat a))
             (e (concat a "")))
        (list
          ;; string-equal tests (content equality)
          (string-equal a b)
          (string-equal a c)
          (string-equal a d)
          (string-equal a e)
          ;; concat of no args returns empty
          (string-equal (concat) "")
          ;; concat of single empty returns empty
          (string-equal (concat "") "")
          ;; nested concat
          (string-equal (concat (concat "a" "b") (concat "c" "d")) "abcd")
          ;; concat result is always a string
          (stringp (concat))
          (stringp (concat "a"))
          (stringp (concat '(65)))
          (stringp (concat [65]))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String builder pattern with reversible operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_builder_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a string builder that tracks operations for undo
    let form = r#"(progn
  (fset 'neovm--test-sb-create
    (lambda () (list "" nil)))  ;; (current-string history-stack)
  (fset 'neovm--test-sb-append
    (lambda (sb text)
      (let ((old (car sb)))
        (list (concat old text) (cons old (cadr sb))))))
  (fset 'neovm--test-sb-prepend
    (lambda (sb text)
      (let ((old (car sb)))
        (list (concat text old) (cons old (cadr sb))))))
  (fset 'neovm--test-sb-undo
    (lambda (sb)
      (if (cadr sb)
          (list (car (cadr sb)) (cdr (cadr sb)))
        sb)))
  (fset 'neovm--test-sb-value
    (lambda (sb) (car sb)))
  (fset 'neovm--test-sb-history-count
    (lambda (sb) (length (cadr sb))))
  (unwind-protect
      (let* ((sb (funcall 'neovm--test-sb-create))
             (sb (funcall 'neovm--test-sb-append sb "Hello"))
             (s1 (funcall 'neovm--test-sb-value sb))
             (sb (funcall 'neovm--test-sb-append sb " "))
             (sb (funcall 'neovm--test-sb-append sb "World"))
             (s2 (funcall 'neovm--test-sb-value sb))
             (h1 (funcall 'neovm--test-sb-history-count sb))
             ;; undo last append
             (sb (funcall 'neovm--test-sb-undo sb))
             (s3 (funcall 'neovm--test-sb-value sb))
             ;; undo again
             (sb (funcall 'neovm--test-sb-undo sb))
             (s4 (funcall 'neovm--test-sb-value sb))
             ;; prepend
             (sb (funcall 'neovm--test-sb-prepend sb ">> "))
             (s5 (funcall 'neovm--test-sb-value sb))
             ;; undo prepend
             (sb (funcall 'neovm--test-sb-undo sb))
             (s6 (funcall 'neovm--test-sb-value sb))
             ;; chain multiple and undo all
             (sb (funcall 'neovm--test-sb-create))
             (sb (funcall 'neovm--test-sb-append sb "a"))
             (sb (funcall 'neovm--test-sb-append sb "b"))
             (sb (funcall 'neovm--test-sb-append sb "c"))
             (sb (funcall 'neovm--test-sb-append sb "d"))
             (full (funcall 'neovm--test-sb-value sb))
             (sb (funcall 'neovm--test-sb-undo sb))
             (sb (funcall 'neovm--test-sb-undo sb))
             (sb (funcall 'neovm--test-sb-undo sb))
             (sb (funcall 'neovm--test-sb-undo sb))
             (empty (funcall 'neovm--test-sb-value sb)))
        (list s1 s2 h1 s3 s4 s5 s6 full empty))
    (fmakunbound 'neovm--test-sb-create)
    (fmakunbound 'neovm--test-sb-append)
    (fmakunbound 'neovm--test-sb-prepend)
    (fmakunbound 'neovm--test-sb-undo)
    (fmakunbound 'neovm--test-sb-value)
    (fmakunbound 'neovm--test-sb-history-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello\" \"Hello World\" 3 \"Hello \" \"Hello\" \">> Hello\" \"Hello\" \"abcd\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Template string expansion using concat + format
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_template_string_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple template engine: replace {{key}} with value from alist
    let form = r#"(progn
  (fset 'neovm--test-tmpl-expand
    (lambda (template env)
      (let ((result "") (i 0) (len (length template)))
        (while (< i len)
          (if (and (< (1+ i) len)
                   (= (aref template i) ?{)
                   (= (aref template (1+ i)) ?{))
              ;; found opening {{, find closing }}
              (let ((start (+ i 2)) (end nil) (j (+ i 2)))
                (while (and (< (1+ j) len) (not end))
                  (when (and (= (aref template j) ?})
                             (= (aref template (1+ j)) ?}))
                    (setq end j))
                  (setq j (1+ j)))
                (if end
                    (let* ((key (substring template start end))
                           (val (cdr (assoc key env))))
                      (setq result (concat result (or val (concat "{{" key "}}"))))
                      (setq i (+ end 2)))
                  (setq result (concat result (char-to-string (aref template i))))
                  (setq i (1+ i))))
            (setq result (concat result (char-to-string (aref template i))))
            (setq i (1+ i))))
        result)))
  (unwind-protect
      (let ((env '(("name" . "Alice")
                   ("age" . "30")
                   ("city" . "Wonderland")
                   ("greeting" . "Hello"))))
        (list
          ;; basic substitution
          (funcall 'neovm--test-tmpl-expand "Hello, {{name}}!" env)
          ;; multiple placeholders
          (funcall 'neovm--test-tmpl-expand
            "{{greeting}}, {{name}}! You are {{age}} from {{city}}." env)
          ;; no placeholders
          (funcall 'neovm--test-tmpl-expand "no placeholders here" env)
          ;; unknown key preserved
          (funcall 'neovm--test-tmpl-expand "{{name}} {{unknown}}" env)
          ;; adjacent placeholders
          (funcall 'neovm--test-tmpl-expand "{{name}}{{age}}" env)
          ;; empty template
          (funcall 'neovm--test-tmpl-expand "" env)
          ;; nested template expansion (two passes)
          (let* ((first-pass
                  (funcall 'neovm--test-tmpl-expand
                    "Dear {{name}}, welcome to {{city}}" env))
                 (env2 (list (cons "message" first-pass) (cons "footer" "---")))
                 (second-pass
                  (funcall 'neovm--test-tmpl-expand
                    "{{message}}\n{{footer}}" env2)))
            second-pass)
          ;; template with repeated key
          (funcall 'neovm--test-tmpl-expand
            "{{name}} is {{name}} is {{name}}" env)))
    (fmakunbound 'neovm--test-tmpl-expand)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello, Alice!\" \"Hello, Alice! You are 30 from Wonderland.\" \"no placeholders here\" \"Alice {{unknown}}\" \"Alice30\" \"\" \"Dear Alice, welcome to Wonderland\\n---\" \"Alice is Alice is Alice\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CSV row builder and parser roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csv_builder_and_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build CSV row from list of fields (quoting fields with commas)
  (fset 'neovm--test-csv-quote-field
    (lambda (field)
      (let ((needs-quote nil) (i 0) (len (length field)))
        (while (and (< i len) (not needs-quote))
          (when (or (= (aref field i) ?,)
                    (= (aref field i) ?\")
                    (= (aref field i) ?\n))
            (setq needs-quote t))
          (setq i (1+ i)))
        (if needs-quote
            (let ((escaped "") (j 0))
              (while (< j len)
                (let ((ch (aref field j)))
                  (if (= ch ?\")
                      (setq escaped (concat escaped "\"\""))
                    (setq escaped (concat escaped (char-to-string ch)))))
                (setq j (1+ j)))
              (concat "\"" escaped "\""))
          field))))
  (fset 'neovm--test-csv-row
    (lambda (fields)
      (mapconcat (lambda (f) (funcall 'neovm--test-csv-quote-field f)) fields ",")))
  ;; Parse CSV row back into fields
  (fset 'neovm--test-csv-parse-row
    (lambda (line)
      (let ((fields nil) (current "") (i 0) (len (length line)) (in-quote nil))
        (while (< i len)
          (let ((ch (aref line i)))
            (cond
              ((and in-quote (= ch ?\")
                    (< (1+ i) len) (= (aref line (1+ i)) ?\"))
               (setq current (concat current "\""))
               (setq i (+ i 2)))
              ((and in-quote (= ch ?\"))
               (setq in-quote nil)
               (setq i (1+ i)))
              ((and (not in-quote) (= ch ?\"))
               (setq in-quote t)
               (setq i (1+ i)))
              ((and (not in-quote) (= ch ?,))
               (setq fields (cons current fields))
               (setq current "")
               (setq i (1+ i)))
              (t
               (setq current (concat current (char-to-string ch)))
               (setq i (1+ i))))))
        (setq fields (cons current fields))
        (nreverse fields))))
  (unwind-protect
      (let* ((row1 '("Alice" "30" "New York"))
             (row2 '("Bob" "25" "San Francisco, CA"))
             (row3 '("Eve" "35" "She said \"hello\""))
             (row4 '("" "" ""))
             ;; Build CSV
             (csv1 (funcall 'neovm--test-csv-row row1))
             (csv2 (funcall 'neovm--test-csv-row row2))
             (csv3 (funcall 'neovm--test-csv-row row3))
             (csv4 (funcall 'neovm--test-csv-row row4))
             ;; Parse back
             (parsed1 (funcall 'neovm--test-csv-parse-row csv1))
             (parsed2 (funcall 'neovm--test-csv-parse-row csv2))
             (parsed3 (funcall 'neovm--test-csv-parse-row csv3))
             (parsed4 (funcall 'neovm--test-csv-parse-row csv4)))
        (list
          csv1 csv2 csv3 csv4
          ;; roundtrip equality
          (equal parsed1 row1)
          (equal parsed2 row2)
          (equal parsed3 row3)
          (equal parsed4 row4)))
    (fmakunbound 'neovm--test-csv-quote-field)
    (fmakunbound 'neovm--test-csv-row)
    (fmakunbound 'neovm--test-csv-parse-row)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice,30,New York\" \"Bob,25,\\\"San Francisco, CA\\\"\" \"Eve,35,\\\"She said \\\"\\\"hello\\\"\\\"\\\"\" \",,\" t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
