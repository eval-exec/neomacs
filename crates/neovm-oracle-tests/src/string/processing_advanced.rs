//! Advanced oracle parity tests for string processing combinations:
//! multi-step transformation pipelines, string-based state machine tokenizer,
//! template engine with nested variable resolution, CSV parser with quoted fields,
//! longest common subsequence, and base64-like encoding/decoding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multi-step string transformation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_transformation_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pipeline: normalize whitespace -> lowercase -> remove punctuation ->
    // split into words -> sort -> deduplicate -> rejoin
    let form = r#"(progn
  (fset 'neovm--test-str-pipeline
    (lambda (input)
      ;; Step 1: collapse multiple spaces/tabs/newlines into single space
      (let ((s (replace-regexp-in-string "[ \t\n]+" " " input)))
        ;; Step 2: lowercase
        (setq s (downcase s))
        ;; Step 3: remove punctuation (keep alphanumeric and space)
        (let ((result "")
              (i 0)
              (len (length s)))
          (while (< i len)
            (let ((ch (aref s i)))
              (when (or (and (>= ch ?a) (<= ch ?z))
                        (and (>= ch ?0) (<= ch ?9))
                        (= ch ?\s))
                (setq result (concat result (char-to-string ch)))))
            (setq i (1+ i)))
          ;; Step 4: split into words
          (let ((words (split-string result " " t)))
            ;; Step 5: sort alphabetically
            (setq words (sort words #'string-lessp))
            ;; Step 6: deduplicate
            (let ((deduped nil)
                  (prev nil))
              (dolist (w words)
                (unless (and prev (string= w prev))
                  (setq deduped (cons w deduped)))
                (setq prev w))
              ;; Step 7: rejoin
              (mapconcat #'identity (nreverse deduped) " ")))))))

  (unwind-protect
      (list
       (funcall 'neovm--test-str-pipeline
                "Hello,  World!   Hello  again,  WORLD!")
       (funcall 'neovm--test-str-pipeline
                "The  quick\tbrown\nfox... jumps! over? the LAZY dog.")
       (funcall 'neovm--test-str-pipeline
                "AAA aaa BBB bbb AAA CCC")
       (funcall 'neovm--test-str-pipeline "")
       (funcall 'neovm--test-str-pipeline "single"))
    (fmakunbound 'neovm--test-str-pipeline)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"again hello world\" \"brown dog fox jumps lazy over quick the\" \"aaa bbb ccc\" \"\" \"single\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String-based tokenizer with state machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_state_machine_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenizer for a mini expression language with states:
    // :start, :in-number, :in-ident, :in-string
    let form = r#"(progn
  (fset 'neovm--test-sm-tokenize
    (lambda (input)
      (let ((tokens nil)
            (state 'start)
            (buf "")
            (i 0)
            (len (length input)))
        (while (<= i len)
          (let ((ch (if (< i len) (aref input i) 0)))
            (cond
             ;; START state
             ((eq state 'start)
              (cond
               ((and (> ch 0) (or (= ch ?\s) (= ch ?\t) (= ch ?\n)))
                nil)  ;; skip whitespace
               ((and (>= ch ?0) (<= ch ?9))
                (setq state 'in-number buf (char-to-string ch)))
               ((and (> ch 0) (or (and (>= ch ?a) (<= ch ?z))
                                   (and (>= ch ?A) (<= ch ?Z))
                                   (= ch ?_)))
                (setq state 'in-ident buf (char-to-string ch)))
               ((and (> ch 0) (= ch ?\"))
                (setq state 'in-string buf ""))
               ((and (> ch 0) (memq ch '(?+ ?- ?* ?/ ?= ?\( ?\) ?, ?\;)))
                (setq tokens (cons (cons 'punct (char-to-string ch)) tokens)))
               (t nil)))
             ;; IN-NUMBER state
             ((eq state 'in-number)
              (if (and (> ch 0) (or (and (>= ch ?0) (<= ch ?9))
                                     (= ch ?.)))
                  (setq buf (concat buf (char-to-string ch)))
                (progn
                  (setq tokens (cons (cons 'number buf) tokens)
                        state 'start buf "")
                  ;; Re-process current char
                  (setq i (1- i)))))
             ;; IN-IDENT state
             ((eq state 'in-ident)
              (if (and (> ch 0) (or (and (>= ch ?a) (<= ch ?z))
                                     (and (>= ch ?A) (<= ch ?Z))
                                     (and (>= ch ?0) (<= ch ?9))
                                     (= ch ?_)))
                  (setq buf (concat buf (char-to-string ch)))
                (progn
                  (setq tokens (cons (cons 'ident buf) tokens)
                        state 'start buf "")
                  (setq i (1- i)))))
             ;; IN-STRING state
             ((eq state 'in-string)
              (cond
               ((= ch ?\")
                (setq tokens (cons (cons 'string buf) tokens)
                      state 'start buf ""))
               ((= ch ?\\)
                (setq i (1+ i))
                (when (< i len)
                  (setq buf (concat buf (char-to-string (aref input i))))))
               ((> ch 0)
                (setq buf (concat buf (char-to-string ch))))))))
          (setq i (1+ i)))
        (nreverse tokens))))

  (unwind-protect
      (list
       (funcall 'neovm--test-sm-tokenize "x = 42 + y")
       (funcall 'neovm--test-sm-tokenize "foo(1, 2.5, \"hello\")")
       (funcall 'neovm--test-sm-tokenize "a_b + c3 * 100")
       (funcall 'neovm--test-sm-tokenize "\"escaped \\\"quote\\\"\"")
       (funcall 'neovm--test-sm-tokenize ""))
    (fmakunbound 'neovm--test-sm-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((ident . \"x\") (punct . \"=\") (number . \"42\") (punct . \"+\") (ident . \"y\")) ((ident . \"foo\") (punct . \"(\") (number . \"1\") (punct . \",\") (number . \"2.5\") (punct . \",\") (string . \"hello\") (punct . \")\")) ((ident . \"a_b\") (punct . \"+\") (ident . \"c3\") (punct . \"*\") (number . \"100\")) ((string . \"escaped \\\"quote\\\"\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String template engine with nested variable resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_template_engine_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Template: "Hello {{name}}, your {{item}} costs {{price}}."
    // Supports nested resolution: a variable can reference another template
    // that itself contains variables (with max depth limit).
    let form = r#"(progn
  (fset 'neovm--test-tpl-resolve
    (lambda (template env depth)
      (if (> depth 5) template  ;; prevent infinite recursion
        (let ((result "")
              (i 0)
              (len (length template)))
          (while (< i len)
            (if (and (< (1+ i) len)
                     (= (aref template i) ?{)
                     (= (aref template (1+ i)) ?{))
                ;; Found {{, extract variable name until }}
                (let ((var-start (+ i 2))
                      (j (+ i 2)))
                  (while (and (< (1+ j) len)
                              (not (and (= (aref template j) ?})
                                        (= (aref template (1+ j)) ?}))))
                    (setq j (1+ j)))
                  (let* ((var-name (substring template var-start j))
                         (binding (assoc var-name env))
                         (val (if binding
                                  (format "%s" (cdr binding))
                                (concat "{{" var-name "}}"))))
                    ;; Recursively resolve if the value itself has templates
                    (setq val (funcall 'neovm--test-tpl-resolve val env (1+ depth)))
                    (setq result (concat result val))
                    (setq i (+ j 2))))
              ;; Regular character
              (progn
                (setq result (concat result (char-to-string (aref template i))))
                (setq i (1+ i)))))
          result))))

  (unwind-protect
      (list
       ;; Basic substitution
       (funcall 'neovm--test-tpl-resolve
                "Hello {{name}}!" '(("name" . "Alice")) 0)
       ;; Multiple variables
       (funcall 'neovm--test-tpl-resolve
                "{{greeting}} {{name}}, you have {{n}} items."
                '(("greeting" . "Hi") ("name" . "Bob") ("n" . 5)) 0)
       ;; Nested: greeting itself is a template
       (funcall 'neovm--test-tpl-resolve
                "{{salutation}}"
                '(("salutation" . "Dear {{title}} {{name}}")
                  ("title" . "Dr")
                  ("name" . "Smith")) 0)
       ;; Missing variable stays as-is
       (funcall 'neovm--test-tpl-resolve
                "{{known}} and {{unknown}}"
                '(("known" . "found")) 0)
       ;; No variables at all
       (funcall 'neovm--test-tpl-resolve
                "just plain text" nil 0)
       ;; Double nesting
       (funcall 'neovm--test-tpl-resolve
                "{{a}}"
                '(("a" . "{{b}}") ("b" . "{{c}}") ("c" . "final")) 0))
    (fmakunbound 'neovm--test-tpl-resolve)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello Alice!\" \"Hi Bob, you have 5 items.\" \"Dear Dr Smith\" \"found and {{unknown}}\" \"just plain text\" \"final\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSV parser handling quoted fields with embedded commas and newlines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_csv_parser_quoted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full CSV parser: handles quoted fields, escaped quotes (""), commas inside quotes
    let form = r#"(progn
  (fset 'neovm--test-csv-parse-row
    (lambda (row)
      (let ((fields nil)
            (buf "")
            (in-quote nil)
            (i 0)
            (len (length row)))
        (while (< i len)
          (let ((ch (aref row i)))
            (cond
             ;; Inside quoted field
             (in-quote
              (cond
               ;; Escaped quote: "" inside quoted field
               ((and (= ch ?\") (< (1+ i) len) (= (aref row (1+ i)) ?\"))
                (setq buf (concat buf "\""))
                (setq i (1+ i)))
               ;; End of quoted field
               ((= ch ?\")
                (setq in-quote nil))
               ;; Regular char inside quotes
               (t (setq buf (concat buf (char-to-string ch))))))
             ;; Outside quoted field
             (t
              (cond
               ;; Start of quoted field
               ((= ch ?\")
                (setq in-quote t))
               ;; Field separator
               ((= ch ?,)
                (setq fields (cons buf fields)
                      buf ""))
               ;; Regular char
               (t (setq buf (concat buf (char-to-string ch))))))))
          (setq i (1+ i)))
        ;; Don't forget last field
        (setq fields (cons buf fields))
        (nreverse fields))))

  (fset 'neovm--test-csv-parse
    (lambda (csv-text)
      (let ((rows nil)
            (lines (split-string csv-text "\n" t)))
        (dolist (line lines)
          (setq rows (cons (funcall 'neovm--test-csv-parse-row line) rows)))
        (nreverse rows))))

  (unwind-protect
      (list
       ;; Simple CSV
       (funcall 'neovm--test-csv-parse "a,b,c\n1,2,3")
       ;; Quoted fields
       (funcall 'neovm--test-csv-parse
                "name,desc,value\nAlice,\"Hello, World\",42")
       ;; Escaped quotes inside quoted field
       (funcall 'neovm--test-csv-parse
                "text\n\"He said \"\"hi\"\"\"")
       ;; Empty fields
       (funcall 'neovm--test-csv-parse "a,,c\n,b,\n,,")
       ;; Single field per row
       (funcall 'neovm--test-csv-parse "one\ntwo\nthree")
       ;; Complex mixed
       (funcall 'neovm--test-csv-parse
                "id,name,bio\n1,Alice,\"Likes coding, reading\"\n2,Bob,\"Said \"\"hello\"\"\"\n3,Carol,simple"))
    (fmakunbound 'neovm--test-csv-parse-row)
    (fmakunbound 'neovm--test-csv-parse)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\" \"c\") (\"1\" \"2\" \"3\")) ((\"name\" \"desc\" \"value\") (\"Alice\" \"Hello, World\" \"42\")) ((\"text\") (\"He said \\\"hi\\\"\")) ((\"a\" \"\" \"c\") (\"\" \"b\" \"\") (\"\" \"\" \"\")) ((\"one\") (\"two\") (\"three\")) ((\"id\" \"name\" \"bio\") (\"1\" \"Alice\" \"Likes coding, reading\") (\"2\" \"Bob\" \"Said \\\"hello\\\"\") (\"3\" \"Carol\" \"simple\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String diff: longest common subsequence (LCS)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_longest_common_subsequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute LCS of two strings using dynamic programming (vector-based 2D table)
    let form = r#"(progn
  (fset 'neovm--test-lcs
    (lambda (s1 s2)
      (let* ((m (length s1))
             (n (length s2))
             ;; dp table: (m+1) x (n+1) stored as flat vector
             (dp (make-vector (* (1+ m) (1+ n)) 0)))
        ;; Fill DP table
        (let ((i 1))
          (while (<= i m)
            (let ((j 1))
              (while (<= j n)
                (if (= (aref s1 (1- i)) (aref s2 (1- j)))
                    (aset dp (+ (* i (1+ n)) j)
                          (1+ (aref dp (+ (* (1- i) (1+ n)) (1- j)))))
                  (aset dp (+ (* i (1+ n)) j)
                        (max (aref dp (+ (* (1- i) (1+ n)) j))
                             (aref dp (+ (* i (1+ n)) (1- j))))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Backtrack to find the actual subsequence
        (let ((result "")
              (i m) (j n))
          (while (and (> i 0) (> j 0))
            (cond
             ((= (aref s1 (1- i)) (aref s2 (1- j)))
              (setq result (concat (char-to-string (aref s1 (1- i))) result))
              (setq i (1- i) j (1- j)))
             ((> (aref dp (+ (* (1- i) (1+ n)) j))
                 (aref dp (+ (* i (1+ n)) (1- j))))
              (setq i (1- i)))
             (t (setq j (1- j)))))
          result))))

  (unwind-protect
      (list
       (funcall 'neovm--test-lcs "abcde" "ace")
       (funcall 'neovm--test-lcs "ABCBDAB" "BDCAB")
       (funcall 'neovm--test-lcs "hello" "hello")
       (funcall 'neovm--test-lcs "abc" "xyz")
       (funcall 'neovm--test-lcs "" "abc")
       (funcall 'neovm--test-lcs "abcdef" "fbdamn")
       ;; Longer strings
       (funcall 'neovm--test-lcs "thequickbrownfox" "thefastbrowndog"))
    (fmakunbound 'neovm--test-lcs)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"ace\" \"BDAB\" \"hello\" \"\" \"\" \"bd\" \"thebrowno\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Base64-like encoding/decoding in pure Elisp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_base16_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Hex (base-16) encode/decode as a simpler but non-trivial codec test
    let form = r#"(progn
  (fset 'neovm--test-hex-encode
    (lambda (input)
      (let ((result "")
            (hex-chars "0123456789abcdef")
            (i 0)
            (len (length input)))
        (while (< i len)
          (let ((byte (aref input i)))
            (setq result
                  (concat result
                          (char-to-string (aref hex-chars (/ byte 16)))
                          (char-to-string (aref hex-chars (% byte 16))))))
          (setq i (1+ i)))
        result)))

  (fset 'neovm--test-hex-val
    (lambda (ch)
      (cond
       ((and (>= ch ?0) (<= ch ?9)) (- ch ?0))
       ((and (>= ch ?a) (<= ch ?f)) (+ 10 (- ch ?a)))
       ((and (>= ch ?A) (<= ch ?F)) (+ 10 (- ch ?A)))
       (t 0))))

  (fset 'neovm--test-hex-decode
    (lambda (input)
      (let ((result "")
            (i 0)
            (len (length input)))
        (while (< (1+ i) len)
          (let ((high (funcall 'neovm--test-hex-val (aref input i)))
                (low (funcall 'neovm--test-hex-val (aref input (1+ i)))))
            (setq result
                  (concat result
                          (char-to-string (+ (* high 16) low)))))
          (setq i (+ i 2)))
        result)))

  (unwind-protect
      (let ((test-strings '("Hello" "World" "" "abc123" "  spaces  "
                            "Special: ~!@#")))
        (list
         ;; Encode results
         (mapcar (lambda (s) (funcall 'neovm--test-hex-encode s)) test-strings)
         ;; Roundtrip: encode then decode should return original
         (mapcar (lambda (s)
                   (string= s (funcall 'neovm--test-hex-decode
                                       (funcall 'neovm--test-hex-encode s))))
                 test-strings)
         ;; Specific encode checks
         (funcall 'neovm--test-hex-encode "AB")
         (funcall 'neovm--test-hex-decode "4142")))
    (fmakunbound 'neovm--test-hex-encode)
    (fmakunbound 'neovm--test-hex-val)
    (fmakunbound 'neovm--test-hex-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"48656c6c6f\" \"576f726c64\" \"\" \"616263313233\" \"20207370616365732020\" \"5370656369616c3a207e214023\") (t t t t t t) \"4142\" \"AB\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: run-length encoding and decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_run_length_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RLE encode: "aaabbc" -> ((3 . ?a) (2 . ?b) (1 . ?c))
    // RLE decode back to string
    let form = r#"(progn
  (fset 'neovm--test-rle-encode
    (lambda (input)
      (if (= (length input) 0) nil
        (let ((runs nil)
              (cur-char (aref input 0))
              (cur-count 1)
              (i 1)
              (len (length input)))
          (while (< i len)
            (if (= (aref input i) cur-char)
                (setq cur-count (1+ cur-count))
              (setq runs (cons (cons cur-count cur-char) runs)
                    cur-char (aref input i)
                    cur-count 1))
            (setq i (1+ i)))
          (setq runs (cons (cons cur-count cur-char) runs))
          (nreverse runs)))))

  (fset 'neovm--test-rle-decode
    (lambda (runs)
      (let ((result ""))
        (dolist (run runs)
          (let ((count (car run))
                (ch (cdr run)))
            (dotimes (_ count)
              (setq result (concat result (char-to-string ch))))))
        result)))

  (unwind-protect
      (let ((test-strings '("aaabbbcc" "aaa" "abcdef" "aaaaaaaa" ""
                            "aabbccddee" "xxyyzz")))
        (list
         ;; Encode results
         (mapcar (lambda (s) (funcall 'neovm--test-rle-encode s)) test-strings)
         ;; Roundtrip
         (mapcar (lambda (s)
                   (string= s (funcall 'neovm--test-rle-decode
                                       (funcall 'neovm--test-rle-encode s))))
                 test-strings)
         ;; Compression ratio for repetitive string
         (let* ((s "aaaaaaaaaaaaaaaaaaaabbbbbbbbbb")
                (encoded (funcall 'neovm--test-rle-encode s)))
           (list (length s) (length encoded)))))
    (fmakunbound 'neovm--test-rle-encode)
    (fmakunbound 'neovm--test-rle-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((3 . 97) (3 . 98) (2 . 99)) ((3 . 97)) ((1 . 97) (1 . 98) (1 . 99) (1 . 100) (1 . 101) (1 . 102)) ((8 . 97)) nil ((2 . 97) (2 . 98) (2 . 99) (2 . 100) (2 . 101)) ((2 . 120) (2 . 121) (2 . 122))) (t t t t t t t) (30 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
