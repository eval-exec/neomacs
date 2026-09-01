//! Oracle parity tests for string processing pipeline patterns.
//!
//! Covers: string-match + match-string + replace-match pipelines,
//! split-string + mapconcat combinations, format with all specifiers,
//! concat + substring + string-to-number chains, case conversion
//! pipelines, and multi-byte string operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-match + match-string + replace-match pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_match_replace_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((input "Date: 2025-12-31, Author: JohnDoe, Rev: 42")
       ;; Extract date components
       (_ (string-match "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)" input))
       (year (match-string 1 input))
       (month (match-string 2 input))
       (day (match-string 3 input))
       (full-match (match-string 0 input))
       ;; Reformat date as DD/MM/YYYY
       (reformatted (replace-match (concat day "/" month "/" year) nil nil input))
       ;; Extract author
       (_ (string-match "Author: \\([A-Za-z]+\\)" input))
       (author (match-string 1 input))
       ;; Extract revision number
       (_ (string-match "Rev: \\([0-9]+\\)" input))
       (rev-str (match-string 1 input))
       (rev-num (string-to-number rev-str)))
  (list year month day full-match
        reformatted
        author
        rev-str rev-num
        (= rev-num 42)
        (string= year "2025")))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"2025\" \"12\" \"31\" \"2025-12-31\" \"Date: 31/12/2025, Author: JohnDoe, Rev: 42\" \"JohnDoe\" \"42\" 42 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chained string-match with groups for nested patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_nested_group_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((input "func(arg1, arg2, arg3)")
       ;; Extract function name and arg list
       (_ (string-match "\\([a-z]+\\)(\\(.*\\))" input))
       (func-name (match-string 1 input))
       (args-str (match-string 2 input))
       ;; Split args
       (args (split-string args-str ", *"))
       ;; Build reversed call
       (reversed-args (mapconcat #'identity (reverse args) ", "))
       (reversed-call (concat func-name "(" reversed-args ")"))
       ;; Count args
       (arg-count (length args))
       ;; Extract each arg using successive matches on args-str
       (first-arg (car args))
       (last-arg (car (last args))))
  (list (string= func-name "func")
        (= arg-count 3)
        (string= first-arg "arg1")
        (string= last-arg "arg3")
        reversed-call
        (string= reversed-args "arg3, arg2, arg1")
        (string= args-str "arg1, arg2, arg3")))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t \"func(arg3, arg2, arg1)\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// split-string + mapconcat pipeline with transformations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_split_mapconcat_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((csv "alice,30,engineer;bob,25,designer;carol,35,manager")
       ;; Split by rows
       (rows (split-string csv ";"))
       ;; Parse each row into fields
       (parsed (mapcar (lambda (row)
                         (let ((fields (split-string row ",")))
                           (list :name (nth 0 fields)
                                 :age (string-to-number (nth 1 fields))
                                 :role (nth 2 fields))))
                       rows))
       ;; Extract just names
       (names (mapcar (lambda (r) (plist-get r :name)) parsed))
       ;; Sum of ages
       (total-age (apply #'+ (mapcar (lambda (r) (plist-get r :age)) parsed)))
       ;; Join names with " & "
       (name-str (mapconcat #'identity names " & "))
       ;; Format as "Name (age)" entries
       (formatted (mapconcat (lambda (r)
                               (format "%s (%d)" (plist-get r :name) (plist-get r :age)))
                             parsed
                             " | "))
       ;; Uppercase all roles
       (roles-upper (mapconcat (lambda (r) (upcase (plist-get r :role))) parsed ", ")))
  (list (= (length rows) 3)
        (= (length parsed) 3)
        (equal names '("alice" "bob" "carol"))
        (= total-age 90)
        (string= name-str "alice & bob & carol")
        formatted
        (string= roles-upper "ENGINEER, DESIGNER, MANAGER")))
"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t \"alice (30) | bob (25) | carol (35)\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format with all specifier types: %s %d %o %x %X %e %f %g %c %%
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_format_all_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; %s - string representation
  (format "%s" "hello")
  (format "%s" 42)
  (format "%s" nil)
  (format "%s" t)
  (format "%s" '(1 2 3))
  ;; %d - decimal integer
  (format "%d" 255)
  (format "%d" -42)
  (format "%05d" 42)
  (format "%-10d" 42)
  (format "%+d" 42)
  (format "%+d" -42)
  ;; %o - octal
  (format "%o" 255)
  (format "%o" 8)
  (format "%#o" 255)
  ;; %x - lowercase hex
  (format "%x" 255)
  (format "%x" 4096)
  (format "%#x" 255)
  ;; %X - uppercase hex
  (format "%X" 255)
  (format "%X" 48879)
  ;; %e - scientific notation
  (format "%e" 3.14159)
  (format "%e" 0.001)
  ;; %f - fixed float
  (format "%f" 3.14159)
  (format "%.2f" 3.14159)
  (format "%10.3f" 3.14)
  ;; %g - general float (shorter of %e and %f)
  (format "%g" 3.14)
  (format "%g" 0.0001)
  (format "%g" 100000.0)
  ;; %c - character
  (format "%c" 65)
  (format "%c" 122)
  ;; %% - literal percent
  (format "100%%")
  (format "%d%%done" 50))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"42\" \"nil\" \"t\" \"(1 2 3)\" \"255\" \"-42\" \"00042\" \"42        \" \"+42\" \"-42\" \"377\" \"10\" \"0377\" \"ff\" \"1000\" \"0xff\" \"FF\" \"BEEF\" \"3.141590e+00\" \"1.000000e-03\" \"3.141590\" \"3.14\" \"     3.140\" \"3.14\" \"0.0001\" \"100000\" \"A\" \"z\" \"100%\" \"50%done\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// concat + substring + string-to-number chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_concat_substring_number_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((parts '("192" "168" "1" "100"))
       ;; Build IP address string
       (ip (mapconcat #'identity parts "."))
       ;; Extract octets back via substring and string-match
       (_ (string-match "\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)" ip))
       (o1 (string-to-number (match-string 1 ip)))
       (o2 (string-to-number (match-string 2 ip)))
       (o3 (string-to-number (match-string 3 ip)))
       (o4 (string-to-number (match-string 4 ip)))
       ;; Compute numeric IP: o1*16777216 + o2*65536 + o3*256 + o4
       (numeric-ip (+ (* o1 16777216) (* o2 65536) (* o3 256) o4))
       ;; Convert back to string
       (rebuilt (concat (number-to-string (/ numeric-ip 16777216)) "."
                        (number-to-string (/ (% numeric-ip 16777216) 65536)) "."
                        (number-to-string (/ (% numeric-ip 65536) 256)) "."
                        (number-to-string (% numeric-ip 256))))
       ;; Substring operations on the IP
       (first-octet-str (substring ip 0 3))
       (last-octet-str (substring ip -3))
       ;; Multi-concat
       (padded (concat "[" (make-string (- 15 (length ip)) ?\s) ip "]")))
  (list (string= ip "192.168.1.100")
        (= o1 192) (= o2 168) (= o3 1) (= o4 100)
        numeric-ip
        (string= rebuilt ip)
        (string= first-octet-str "192")
        (string= last-octet-str "100")
        padded))
"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t 3232235876 t t t \"[  192.168.1.100]\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case conversion pipelines: upcase, downcase, capitalize, upcase-initials
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_case_conversion_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((input "the quick BROWN fox Jumps OVER the Lazy Dog")
       ;; Title case: capitalize each word
       (words (split-string input " "))
       (title-cased (mapconcat #'capitalize words " "))
       ;; Snake_case to camelCase conversion
       (snake "hello_world_foo_bar")
       (snake-parts (split-string snake "_"))
       (camel (concat (downcase (car snake-parts))
                      (mapconcat #'capitalize (cdr snake-parts) "")))
       ;; CamelCase: capitalize all including first
       (pascal (mapconcat #'capitalize snake-parts ""))
       ;; All upper
       (screaming (upcase (mapconcat #'identity snake-parts "_")))
       ;; All lower
       (all-lower (downcase input))
       ;; All upper
       (all-upper (upcase input))
       ;; upcase-initials
       (initials-up (upcase-initials "hello world"))
       ;; Mixed pipeline: downcase then capitalize first letter
       (normalized (concat (upcase (substring input 0 1))
                           (downcase (substring input 1)))))
  (list title-cased
        (string= camel "helloWorldFooBar")
        (string= pascal "HelloWorldFooBar")
        (string= screaming "HELLO_WORLD_FOO_BAR")
        (string= all-lower "the quick brown fox jumps over the lazy dog")
        (string= all-upper "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG")
        (string= initials-up "Hello World")
        normalized))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"The Quick Brown Fox Jumps Over The Lazy Dog\" t t t t t t \"The quick brown fox jumps over the lazy dog\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-pass replace-regexp-in-string pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_multi_pass_regexp_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((input "  Hello,   World!   How   are    you?  ")
       ;; Step 1: trim leading/trailing whitespace
       (trimmed (string-trim input))
       ;; Step 2: collapse multiple spaces to single
       (collapsed (replace-regexp-in-string "  +" " " trimmed))
       ;; Step 3: remove punctuation
       (no-punct (replace-regexp-in-string "[,!?]" "" collapsed))
       ;; Step 4: downcase
       (lower (downcase no-punct))
       ;; Step 5: replace spaces with hyphens (slug)
       (slug (replace-regexp-in-string " " "-" lower))
       ;; Also do a digit insertion replacement
       (with-digits (replace-regexp-in-string "\\([a-z]\\)\\([A-Z]\\)" "\\1-\\2" "helloWorld"))
       ;; Replace with function: upcase each match
       (func-replace (replace-regexp-in-string "[a-z]+"
                       (lambda (m) (upcase m))
                       "hello world 123 test")))
  (list (string= trimmed "Hello,   World!   How   are    you?")
        (string= collapsed "Hello, World! How are you?")
        (string= no-punct "Hello World How are you")
        (string= lower "hello world how are you")
        (string= slug "hello-world-how-are-you")
        with-digits
        func-replace))
"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t \"h-el-lo-Wo-rl-d\" \"HELLO WORLD 123 TEST\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format with padding, width, and precision combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_format_padding_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Right-aligned strings with width
  (format "%10s" "hello")
  (format "%10s" "hi")
  (format "%3s" "toolong")
  ;; Left-aligned strings with width
  (format "%-10s" "hello")
  (format "%-10s" "hi")
  ;; Zero-padded integers
  (format "%08d" 42)
  (format "%08d" -42)
  (format "%08x" 255)
  ;; Precision on floats
  (format "%.0f" 3.7)
  (format "%.1f" 3.14159)
  (format "%.5f" 3.14)
  (format "%12.4f" 3.14)
  ;; Combining width and precision
  (format "%10.2f" 3.14159)
  (format "%-10.2f|" 3.14159)
  ;; Multiple args in one format
  (format "[%s] %05d %8.2f %%" "test" 42 3.14)
  ;; Nested format calls
  (format "outer(%s)" (format "inner(%d)" 99))
  ;; Format with characters
  (format "%c%c%c" 72 101 108))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"     hello\" \"        hi\" \"toolong\" \"hello     \" \"hi        \" \"00000042\" \"-0000042\" \"000000ff\" \"4\" \"3.1\" \"3.14000\" \"      3.1400\" \"      3.14\" \"3.14      |\" \"[test] 00042     3.14 %\" \"outer(inner(99))\" \"Hel\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: tokenizer using string-match in a loop
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_tokenizer_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((input "  foo = 123 + bar * 45.6 - \"hello world\" ")
       (pos 0)
       (tokens nil)
       (patterns '(("[ \t]+" . ws)
                   ("[a-zA-Z_][a-zA-Z0-9_]*" . ident)
                   ("[0-9]+\\(\\.[0-9]+\\)?" . number)
                   ("\"[^\"]*\"" . string)
                   ("[=+*/-]" . op))))
  ;; Tokenize loop
  (while (< pos (length input))
    (let ((matched nil))
      (dolist (pat patterns)
        (unless matched
          (when (string-match (concat "\\=" (car pat)) input pos)
            (let ((tok-text (match-string 0 input))
                  (tok-type (cdr pat)))
              (unless (eq tok-type 'ws)
                (push (list tok-type tok-text) tokens))
              (setq pos (match-end 0))
              (setq matched t)))))
      (unless matched
        (setq pos (1+ pos)))))
  (let ((result (nreverse tokens)))
    (list (length result)
          result
          ;; Verify types
          (equal (mapcar #'car result)
                 '(ident op number op ident op number op string)))))
"#;
    let expect = expect_test::expect![[r#""OK (0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-byte / unicode string operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_process_string_multibyte_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((ascii "Hello")
       (latin "cafe\u0301")
       (cjk "\u4e16\u754c")
       (emoji-text "AB")
       ;; Length vs string-bytes
       (ascii-len (length ascii))
       (ascii-bytes (string-bytes ascii))
       (cjk-len (length cjk))
       (cjk-bytes (string-bytes cjk))
       ;; Concatenation of mixed scripts
       (mixed (concat ascii " " cjk))
       (mixed-len (length mixed))
       ;; Substring on multi-byte
       (sub-cjk (substring cjk 0 1))
       (sub-cjk-char (aref cjk 0))
       ;; String comparison
       (ascii-less (string< "a" "b"))
       (case-fold (let ((case-fold-search t))
                    (string-match "hello" "HELLO")))
       ;; char-to-string and string-to-char round-trip
       (ch (aref ascii 0))
       (ch-str (char-to-string ch))
       (ch-back (string-to-char ch-str))
       ;; Multibyte predicate
       (ascii-multi (multibyte-string-p ascii))
       (cjk-multi (multibyte-string-p cjk)))
  (list (= ascii-len 5)
        (= ascii-bytes 5)
        (= cjk-len 2)
        (> cjk-bytes 2)
        mixed
        mixed-len
        (string= sub-cjk "\u4e16")
        (= sub-cjk-char #x4e16)
        ascii-less
        (integerp case-fold)
        (= ch 72)
        (string= ch-str "H")
        (= ch-back 72)
        cjk-multi))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t \"Hello 世界\" 8 t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
