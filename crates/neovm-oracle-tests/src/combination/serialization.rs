//! Oracle parity tests for serialization/deserialization patterns in Elisp.
//!
//! Covers: S-expression serialization (prin1-to-string + read-from-string
//! roundtrip), JSON-like alist format, CSV serialization with quoting,
//! INI-like config format, and binary-like pack/unpack of integers.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// S-expression serialization: prin1-to-string + read-from-string roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_serialization_sexp_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize various Elisp data structures to strings via prin1-to-string,
    // then deserialize with read-from-string, and verify equality.
    // Covers integers, floats, strings, symbols, lists, vectors, alists, plists.
    let form = r#"(unwind-protect
      (progn
        (defun test--sexp-roundtrip (value)
          "Serialize VALUE with prin1-to-string, deserialize with read-from-string.
Returns (serialized deserialized equal-p)."
          (let* ((serialized (prin1-to-string value))
                 (deserialized (car (read-from-string serialized)))
                 (equal-p (equal value deserialized)))
            (list serialized deserialized equal-p)))

        (let ((test-data
                (list
                  ;; Primitives
                  42
                  -999
                  3.14159
                  "hello world"
                  "string with \"quotes\" and \\backslash"
                  "multi\nline\nstring"
                  'some-symbol
                  :a-keyword
                  nil
                  t
                  ;; Lists
                  '(1 2 3)
                  '(a (b (c d)) e)
                  '((key1 . "val1") (key2 . "val2") (key3 . 42))
                  ;; Vectors
                  [1 2 3]
                  [a "b" 3 :d]
                  ;; Dotted pair
                  '(head . tail)
                  ;; Nested structure
                  '((name . "Alice")
                    (scores . [95 87 92])
                    (address . ((city . "Springfield")
                                (zip . "62701")))))))
          (mapcar #'test--sexp-roundtrip test-data)))
      ;; Cleanup
      (fmakunbound 'test--sexp-roundtrip))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"42\" 42 t) (\"-999\" -999 t) (\"3.14159\" 3.14159 t) (\"\\\"hello world\\\"\" \"hello world\" t) (\"\\\"string with \\\\\\\"quotes\\\\\\\" and \\\\\\\\backslash\\\"\" \"string with \\\"quotes\\\" and \\\\backslash\" t) (\"\\\"multi\\nline\\nstring\\\"\" \"multi\\nline\\nstring\" t) (\"some-symbol\" some-symbol t) (\":a-keyword\" :a-keyword t) (\"nil\" nil t) (\"t\" t t) (\"(1 2 3)\" (1 2 3) t) (\"(a (b (c d)) e)\" (a (b (c d)) e) t) (\"((key1 . \\\"val1\\\") (key2 . \\\"val2\\\") (key3 . 42))\" ((key1 . \"val1\") (key2 . \"val2\") (key3 . 42)) t) (\"[1 2 3]\" [1 2 3] t) (\"[a \\\"b\\\" 3 :d]\" [a \"b\" 3 :d] t) (\"(head . tail)\" (head . tail) t) (\"((name . \\\"Alice\\\") (scores . [95 87 92]) (address (city . \\\"Springfield\\\") (zip . \\\"62701\\\")))\" ((name . \"Alice\") (scores . [95 87 92]) (address (city . \"Springfield\") (zip . \"62701\"))) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// JSON-like format: alist -> string -> alist roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_serialization_json_like_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a simple JSON-like serializer/deserializer for alists.
    // Supports: strings, integers, alists (objects), lists (arrays), nil, t.
    // Uses unwind-protect with fmakunbound for cleanup.
    let form = r#"(unwind-protect
      (progn
        (defun test--json-serialize (value)
          "Serialize VALUE to a JSON-like string."
          (cond
            ((null value) "null")
            ((eq value t) "true")
            ((integerp value) (number-to-string value))
            ((stringp value)
             (concat "\""
                     (replace-regexp-in-string "\\\\" "\\\\\\\\" value t t)
                     "\""))
            ;; Alist (object) — detect by checking if first element is a cons
            ((and (listp value) (consp (car value))
                  (symbolp (car (car value))))
             (concat "{"
                     (mapconcat
                       (lambda (pair)
                         (concat "\"" (symbol-name (car pair)) "\":"
                                 (test--json-serialize (cdr pair))))
                       value
                       ",")
                     "}"))
            ;; Regular list (array)
            ((listp value)
             (concat "["
                     (mapconcat #'test--json-serialize value ",")
                     "]"))
            ;; Fallback
            (t (prin1-to-string value))))

        (let ((data1 '((name . "Alice")
                        (age . 30)
                        (active . t)
                        (tags . ("lisp" "emacs" "coding"))
                        (address . ((city . "Springfield")
                                    (zip . "62701")))))
              (data2 '((items . (1 2 3))
                        (empty . nil)
                        (nested . ((a . ((b . 42)))))))
              (data3 '((matrix . ((1 2 3) (4 5 6) (7 8 9))))))
          (list
            (test--json-serialize data1)
            (test--json-serialize data2)
            (test--json-serialize data3)
            (test--json-serialize nil)
            (test--json-serialize t)
            (test--json-serialize 42)
            (test--json-serialize "hello")
            (test--json-serialize '(1 2 3)))))
      ;; Cleanup
      (fmakunbound 'test--json-serialize))"#;
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"name\\\":\\\"Alice\\\",\\\"age\\\":30,\\\"active\\\":true,\\\"tags\\\":[\\\"lisp\\\",\\\"emacs\\\",\\\"coding\\\"],\\\"address\\\":{\\\"city\\\":\\\"Springfield\\\",\\\"zip\\\":\\\"62701\\\"}}\" \"{\\\"items\\\":[1,2,3],\\\"empty\\\":null,\\\"nested\\\":{\\\"a\\\":{\\\"b\\\":42}}}\" \"{\\\"matrix\\\":[[1,2,3],[4,5,6],[7,8,9]]}\" \"null\" \"true\" \"42\" \"\\\"hello\\\"\" \"[1,2,3]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSV serialization with quoting and escaping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_serialization_csv_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement CSV serializer and parser:
    // - Fields containing commas, quotes, or newlines are quoted
    // - Quotes inside quoted fields are doubled
    // - Parser splits on commas, handles quoted fields
    let form = r#"(unwind-protect
      (progn
        (require 'cl-lib)
        (defun test--csv-escape-field (field)
          "Escape a single CSV field. Quote if necessary."
          (let ((s (cond
                     ((numberp field) (number-to-string field))
                     ((null field) "")
                     ((stringp field) field)
                     (t (prin1-to-string field)))))
            (if (string-match "[,\"\n]" s)
                ;; Need quoting: double any internal quotes
                (concat "\""
                        (replace-regexp-in-string "\"" "\"\"" s t t)
                        "\"")
              s)))

        (defun test--csv-serialize-row (fields)
          "Serialize a list of FIELDS into a CSV row string."
          (mapconcat #'test--csv-escape-field fields ","))

        (defun test--csv-serialize-table (rows)
          "Serialize a list of ROWS into a full CSV string."
          (mapconcat #'test--csv-serialize-row rows "\n"))

        (defun test--csv-parse-row (line)
          "Parse a CSV LINE into a list of field strings."
          (let ((fields nil)
                (current "")
                (in-quote nil)
                (i 0)
                (len (length line)))
            (while (< i len)
              (let ((ch (aref line i)))
                (cond
                  ;; Quote character
                  ((= ch ?\")
                   (if in-quote
                       ;; Check for escaped quote (doubled)
                       (if (and (< (1+ i) len) (= (aref line (1+ i)) ?\"))
                           (progn
                             (setq current (concat current "\""))
                             (setq i (1+ i)))
                         ;; End of quoted field
                         (setq in-quote nil))
                     ;; Start of quoted field
                     (setq in-quote t)))
                  ;; Comma outside quotes
                  ((and (= ch ?,) (not in-quote))
                   (setq fields (cons current fields))
                   (setq current ""))
                  ;; Regular character
                  (t
                   (setq current (concat current (char-to-string ch))))))
              (setq i (1+ i)))
            ;; Last field
            (setq fields (cons current fields))
            (nreverse fields)))

        ;; Test data with tricky fields
        (let* ((headers '("Name" "City" "Quote" "Amount"))
               (rows (list
                       headers
                       '("Alice" "New York" "She said \"hello\"" "1000")
                       '("Bob" "San Francisco, CA" "plain" "2500")
                       '("Charlie" "Boston" "has,comma" "300")
                       '("Diana" "London" "" "0")))
               (csv-text (test--csv-serialize-table rows)))
          ;; Serialize and parse back
          (let ((parsed-rows
                  (mapcar #'test--csv-parse-row
                          (split-string csv-text "\n"))))
            (list
              csv-text
              parsed-rows
              ;; Verify roundtrip for each row
              (mapcar (lambda (pair)
                        (equal (car pair) (cdr pair)))
                      (cl-mapcar #'cons rows parsed-rows))))))
      ;; Cleanup
      (fmakunbound 'test--csv-escape-field)
      (fmakunbound 'test--csv-serialize-row)
      (fmakunbound 'test--csv-serialize-table)
      (fmakunbound 'test--csv-parse-row))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Name,City,Quote,Amount\\nAlice,New York,\\\"She said \\\"\\\"hello\\\"\\\"\\\",1000\\nBob,\\\"San Francisco, CA\\\",plain,2500\\nCharlie,Boston,\\\"has,comma\\\",300\\nDiana,London,,0\" ((\"Name\" \"City\" \"Quote\" \"Amount\") (\"Alice\" \"New York\" \"She said \\\"hello\\\"\" \"1000\") (\"Bob\" \"San Francisco, CA\" \"plain\" \"2500\") (\"Charlie\" \"Boston\" \"has,comma\" \"300\") (\"Diana\" \"London\" \"\" \"0\")) (t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Key-value config file format (INI-like)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_serialization_ini_config_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize and parse an INI-like config format:
    // [section]
    // key = value
    // ; comments are ignored
    let form = r#"(unwind-protect
      (progn
        (defun test--ini-serialize (config)
          "Serialize CONFIG to INI format.
CONFIG is ((section-name . ((key . value) ...)) ...)."
          (let ((parts nil))
            (dolist (section config)
              (let ((section-name (car section))
                    (entries (cdr section)))
                (setq parts (cons (format "[%s]" section-name) parts))
                (dolist (entry entries)
                  (setq parts (cons (format "%s = %s" (car entry) (cdr entry))
                                    parts)))
                (setq parts (cons "" parts))))  ; blank line between sections
            (mapconcat #'identity (nreverse parts) "\n")))

        (defun test--ini-parse (text)
          "Parse INI TEXT into ((section . ((key . value) ...)) ...).
Ignores comments (lines starting with ;) and blank lines."
          (let ((sections nil)
                (current-section nil)
                (current-entries nil))
            (dolist (line (split-string text "\n"))
              (let ((trimmed (string-trim line)))
                (cond
                  ;; Empty or comment
                  ((or (= (length trimmed) 0)
                       (string-match "\\`;" trimmed))
                   nil)
                  ;; Section header
                  ((string-match "\\`\\[\\([^]]+\\)\\]\\'" trimmed)
                   ;; Save previous section
                   (when current-section
                     (setq sections (cons (cons current-section
                                                (nreverse current-entries))
                                          sections)))
                   (setq current-section (match-string 1 trimmed))
                   (setq current-entries nil))
                  ;; Key = value
                  ((string-match "\\`\\([^=]+?\\)\\s-*=\\s-*\\(.*\\)\\'" trimmed)
                   (let ((key (string-trim (match-string 1 trimmed)))
                         (val (string-trim (match-string 2 trimmed))))
                     (setq current-entries (cons (cons key val)
                                                  current-entries)))))))
            ;; Save last section
            (when current-section
              (setq sections (cons (cons current-section
                                          (nreverse current-entries))
                                    sections)))
            (nreverse sections)))

        (let* ((config '(("database"
                           . (("host" . "localhost")
                              ("port" . "5432")
                              ("name" . "mydb")
                              ("user" . "admin")))
                          ("server"
                           . (("bind" . "0.0.0.0")
                              ("port" . "8080")
                              ("workers" . "4")
                              ("debug" . "false")))
                          ("logging"
                           . (("level" . "info")
                              ("file" . "/var/log/app.log")
                              ("rotate" . "daily")))))
               (serialized (test--ini-serialize config))
               (parsed (test--ini-parse serialized)))
          (list
            serialized
            parsed
            ;; Verify roundtrip: compare section names and keys
            (equal (mapcar #'car config) (mapcar #'car parsed))
            ;; Verify all values match
            (let ((all-match t))
              (dolist (orig-section config)
                (let ((parsed-section (assoc (car orig-section) parsed)))
                  (dolist (entry (cdr orig-section))
                    (unless (equal (cdr entry)
                                   (cdr (assoc (car entry) (cdr parsed-section))))
                      (setq all-match nil)))))
              all-match)
            ;; Parse text with comments
            (test--ini-parse
              "[main]\n; this is a comment\nkey1 = val1\n; another\nkey2 = val2\n"))))
      ;; Cleanup
      (fmakunbound 'test--ini-serialize)
      (fmakunbound 'test--ini-parse))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[database]\\nhost = localhost\\nport = 5432\\nname = mydb\\nuser = admin\\n\\n[server]\\nbind = 0.0.0.0\\nport = 8080\\nworkers = 4\\ndebug = false\\n\\n[logging]\\nlevel = info\\nfile = /var/log/app.log\\nrotate = daily\\n\" ((\"database\" (\"host\" . \"localhost\") (\"port\" . \"5432\") (\"name\" . \"mydb\") (\"user\" . \"admin\")) (\"server\" (\"bind\" . \"0.0.0.0\") (\"port\" . \"8080\") (\"workers\" . \"4\") (\"debug\" . \"false\")) (\"logging\" (\"level\" . \"info\") (\"file\" . \"/var/log/app.log\") (\"rotate\" . \"daily\"))) t t ((\"main\" (\"key1\" . \"val1\") (\"key2\" . \"val2\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binary-like encoding: pack/unpack integers into string of bytes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_serialization_binary_pack_unpack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement pack/unpack for encoding integers as sequences of byte
    // characters in a string (big-endian). Supports u8, u16, u32.
    // Uses unwind-protect with fmakunbound for cleanup.
    let form = r#"(unwind-protect
      (progn
        (defun test--pack-u8 (n)
          "Pack an unsigned 8-bit integer into a 1-char string."
          (char-to-string (logand n 255)))

        (defun test--pack-u16-be (n)
          "Pack unsigned 16-bit integer, big-endian, into 2-char string."
          (concat (char-to-string (logand (ash n -8) 255))
                  (char-to-string (logand n 255))))

        (defun test--pack-u32-be (n)
          "Pack unsigned 32-bit integer, big-endian, into 4-char string."
          (concat (char-to-string (logand (ash n -24) 255))
                  (char-to-string (logand (ash n -16) 255))
                  (char-to-string (logand (ash n -8) 255))
                  (char-to-string (logand n 255))))

        (defun test--unpack-u8 (s offset)
          "Unpack unsigned 8-bit integer from string S at OFFSET."
          (aref s offset))

        (defun test--unpack-u16-be (s offset)
          "Unpack unsigned 16-bit BE integer from S at OFFSET."
          (+ (ash (aref s offset) 8)
             (aref s (1+ offset))))

        (defun test--unpack-u32-be (s offset)
          "Unpack unsigned 32-bit BE integer from S at OFFSET."
          (+ (ash (aref s offset) 24)
             (ash (aref s (1+ offset)) 16)
             (ash (aref s (+ offset 2)) 8)
             (aref s (+ offset 3))))

        (defun test--pack-record (fields)
          "Pack a list of (type . value) pairs into a binary string.
Types: u8, u16, u32."
          (apply #'concat
                 (mapcar (lambda (f)
                           (let ((type (car f)) (val (cdr f)))
                             (cond
                               ((eq type 'u8) (test--pack-u8 val))
                               ((eq type 'u16) (test--pack-u16-be val))
                               ((eq type 'u32) (test--pack-u32-be val)))))
                         fields)))

        (defun test--unpack-record (s field-types)
          "Unpack binary string S according to FIELD-TYPES (list of u8/u16/u32).
Returns list of unpacked values."
          (let ((offset 0)
                (values nil))
            (dolist (type field-types)
              (cond
                ((eq type 'u8)
                 (setq values (cons (test--unpack-u8 s offset) values))
                 (setq offset (1+ offset)))
                ((eq type 'u16)
                 (setq values (cons (test--unpack-u16-be s offset) values))
                 (setq offset (+ offset 2)))
                ((eq type 'u32)
                 (setq values (cons (test--unpack-u32-be s offset) values))
                 (setq offset (+ offset 4)))))
            (nreverse values)))

        ;; Test various values
        (let* ((record '((u8 . 255)
                          (u16 . 1024)
                          (u32 . 70000)
                          (u8 . 0)
                          (u16 . 65535)
                          (u32 . 1)))
               (packed (test--pack-record record))
               (field-types (mapcar #'car record))
               (unpacked (test--unpack-record packed field-types))
               (original-values (mapcar #'cdr record)))
          (list
            ;; Individual pack/unpack roundtrips
            (test--unpack-u8 (test--pack-u8 42) 0)
            (test--unpack-u16-be (test--pack-u16-be 1234) 0)
            (test--unpack-u32-be (test--pack-u32-be 123456) 0)
            ;; Packed string length (1+2+4+1+2+4 = 14)
            (length packed)
            ;; Unpacked values match original
            unpacked
            (equal original-values unpacked)
            ;; Edge cases
            (test--unpack-u8 (test--pack-u8 0) 0)
            (test--unpack-u16-be (test--pack-u16-be 0) 0)
            (test--unpack-u32-be (test--pack-u32-be 0) 0)
            (test--unpack-u8 (test--pack-u8 255) 0)
            (test--unpack-u16-be (test--pack-u16-be 65535) 0))))
      ;; Cleanup
      (fmakunbound 'test--pack-u8)
      (fmakunbound 'test--pack-u16-be)
      (fmakunbound 'test--pack-u32-be)
      (fmakunbound 'test--unpack-u8)
      (fmakunbound 'test--unpack-u16-be)
      (fmakunbound 'test--unpack-u32-be)
      (fmakunbound 'test--pack-record)
      (fmakunbound 'test--unpack-record))"#;
    let expect = expect_test::expect![[
        r#""OK (42 1234 123456 14 (255 1024 70000 0 65535 1) t 0 0 0 255 65535)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Property-list based serialization with type tags
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_serialization_tagged_plist_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize complex data to tagged plists that preserve type information,
    // then deserialize back. Tags: :int, :str, :bool, :list, :map.
    let form = r#"(unwind-protect
      (progn
        (defun test--tagged-serialize (value)
          "Serialize VALUE into a tagged plist form."
          (cond
            ((null value) '(:bool nil))
            ((eq value t) '(:bool t))
            ((integerp value) (list :int value))
            ((floatp value) (list :float value))
            ((stringp value) (list :str value))
            ((and (listp value) (consp (car value)) (symbolp (caar value)))
             ;; alist -> map
             (list :map
                   (mapcar (lambda (pair)
                             (cons (car pair)
                                   (test--tagged-serialize (cdr pair))))
                           value)))
            ((listp value)
             (list :list (mapcar #'test--tagged-serialize value)))
            (t (list :unknown (prin1-to-string value)))))

        (defun test--tagged-deserialize (tagged)
          "Deserialize a tagged plist form back to value."
          (let ((tag (car tagged))
                (payload (cadr tagged)))
            (cond
              ((eq tag :bool) payload)
              ((eq tag :int) payload)
              ((eq tag :float) payload)
              ((eq tag :str) payload)
              ((eq tag :list) (mapcar #'test--tagged-deserialize payload))
              ((eq tag :map)
               (mapcar (lambda (pair)
                         (cons (car pair)
                               (test--tagged-deserialize (cdr pair))))
                       payload))
              (t payload))))

        (let ((test-values
                (list
                  42
                  "hello"
                  t
                  nil
                  3.14
                  '(1 2 3)
                  '((name . "Alice") (age . 30) (active . t))
                  '((nested . ((deep . "value")))
                    (list-field . (10 20 30))))))
          (mapcar (lambda (v)
                    (let* ((serialized (test--tagged-serialize v))
                           (deserialized (test--tagged-deserialize serialized))
                           (roundtrip-ok (equal v deserialized)))
                      (list v serialized deserialized roundtrip-ok)))
                  test-values)))
      ;; Cleanup
      (fmakunbound 'test--tagged-serialize)
      (fmakunbound 'test--tagged-deserialize))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 (:int 42) 42 t) (\"hello\" (:str \"hello\") \"hello\" t) (t (:bool t) t t) (nil (:bool nil) nil t) (3.14 (:float 3.14) 3.14 t) ((1 2 3) (:list ((:int 1) (:int 2) (:int 3))) (1 2 3) t) (((name . \"Alice\") (age . 30) (active . t)) (:map ((name :str \"Alice\") (age :int 30) (active :bool t))) ((name . \"Alice\") (age . 30) (active . t)) t) (((nested (deep . \"value\")) (list-field 10 20 30)) (:map ((nested :map ((deep :str \"value\"))) (list-field :list ((:int 10) (:int 20) (:int 30))))) ((nested (deep . \"value\")) (list-field 10 20 30)) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
