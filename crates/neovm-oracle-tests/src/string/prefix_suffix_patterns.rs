//! Oracle parity tests for `string-prefix-p` and `string-suffix-p` with
//! complex patterns: basic matching, case sensitivity (IGNORE-CASE param),
//! empty string edge cases, self-identity, finding common prefix/suffix
//! in lists, and string categorization by prefix/suffix.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-prefix-p basic matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_p_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic prefix match
  (string-prefix-p "hel" "hello")
  (string-prefix-p "hello" "hello world")
  ;; Not a prefix
  (string-prefix-p "world" "hello world")
  (string-prefix-p "xyz" "hello")
  ;; Single character prefix
  (string-prefix-p "h" "hello")
  (string-prefix-p "H" "hello")
  ;; Prefix longer than string
  (string-prefix-p "hello world!" "hello")
  ;; Prefix equals string exactly
  (string-prefix-p "exact" "exact")
  ;; Numeric strings
  (string-prefix-p "123" "12345")
  (string-prefix-p "12345" "123")
  ;; Special characters
  (string-prefix-p "/" "/usr/local/bin")
  (string-prefix-p "/usr" "/usr/local/bin")
  (string-prefix-p "." ".emacs")
  (string-prefix-p ".." "..hidden")
  ;; Return type is boolean-like (t or nil)
  (if (string-prefix-p "a" "abc") 'yes 'no)
  (if (string-prefix-p "z" "abc") 'yes 'no))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil t nil nil t t nil t t t t yes no)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-suffix-p basic matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_suffix_p_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic suffix match
  (string-suffix-p "llo" "hello")
  (string-suffix-p "world" "hello world")
  ;; Not a suffix
  (string-suffix-p "hello" "hello world")
  (string-suffix-p "xyz" "hello")
  ;; Single character suffix
  (string-suffix-p "o" "hello")
  (string-suffix-p "O" "hello")
  ;; Suffix longer than string
  (string-suffix-p "hello world!" "hello")
  ;; Suffix equals string exactly
  (string-suffix-p "exact" "exact")
  ;; File extension matching
  (string-suffix-p ".el" "init.el")
  (string-suffix-p ".rs" "main.rs")
  (string-suffix-p ".txt" "readme.md")
  (string-suffix-p ".tar.gz" "archive.tar.gz")
  (string-suffix-p ".tar.gz" "archive.tar.bz2")
  ;; Path suffix
  (string-suffix-p "/bin" "/usr/local/bin")
  (string-suffix-p "/lib" "/usr/local/bin"))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil t nil nil t t t nil t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case sensitivity: IGNORE-CASE parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix_case_sensitivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; string-prefix-p case-sensitive (default)
  (string-prefix-p "Hello" "hello world")
  (string-prefix-p "HELLO" "hello world")
  (string-prefix-p "hello" "Hello World")

  ;; string-prefix-p case-insensitive (IGNORE-CASE = t)
  (string-prefix-p "Hello" "hello world" t)
  (string-prefix-p "HELLO" "hello world" t)
  (string-prefix-p "hello" "Hello World" t)
  (string-prefix-p "hElLo" "HELLO WORLD" t)

  ;; string-prefix-p case-insensitive with nil (no ignore)
  (string-prefix-p "Hello" "hello world" nil)

  ;; string-suffix-p case-sensitive (default)
  (string-suffix-p ".EL" "init.el")
  (string-suffix-p ".El" "init.el")
  (string-suffix-p ".el" "INIT.EL")

  ;; string-suffix-p case-insensitive
  (string-suffix-p ".EL" "init.el" t)
  (string-suffix-p ".El" "init.el" t)
  (string-suffix-p ".el" "INIT.EL" t)
  (string-suffix-p ".TXT" "readme.txt" t)

  ;; string-suffix-p case-insensitive with nil
  (string-suffix-p ".EL" "init.el" nil)

  ;; Mixed case with multi-byte (ASCII)
  (string-prefix-p "ABC" "abcdef" t)
  (string-suffix-p "DEF" "abcdef" t))"#;
    let expect =
        expect_test::expect![[r#""OK (nil nil nil t t t t nil nil nil nil t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty string as prefix/suffix (always true)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty string is prefix of everything
  (string-prefix-p "" "hello")
  (string-prefix-p "" "")
  (string-prefix-p "" "a")
  (string-prefix-p "" "long string with many words")

  ;; Empty string is suffix of everything
  (string-suffix-p "" "hello")
  (string-suffix-p "" "")
  (string-suffix-p "" "a")
  (string-suffix-p "" "long string with many words")

  ;; Non-empty prefix/suffix of empty string: always nil
  (string-prefix-p "a" "")
  (string-prefix-p "hello" "")
  (string-suffix-p "a" "")
  (string-suffix-p "hello" "")

  ;; Empty with ignore-case
  (string-prefix-p "" "test" t)
  (string-suffix-p "" "test" t)
  (string-prefix-p "" "" t)
  (string-suffix-p "" "" t))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil nil nil nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String equal to itself: both prefix and suffix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix_self_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; A string is always a prefix and suffix of itself
  (string-prefix-p "hello" "hello")
  (string-suffix-p "hello" "hello")
  (string-prefix-p "x" "x")
  (string-suffix-p "x" "x")
  (string-prefix-p "" "")
  (string-suffix-p "" "")
  (string-prefix-p "a longer test string" "a longer test string")
  (string-suffix-p "a longer test string" "a longer test string")

  ;; Case-insensitive self-identity with different case
  (string-prefix-p "HELLO" "hello" t)
  (string-suffix-p "HELLO" "hello" t)
  (string-prefix-p "hello" "HELLO" t)
  (string-suffix-p "hello" "HELLO" t)

  ;; Palindrome: prefix = suffix
  (let ((s "abcba"))
    (list (string-prefix-p "abc" s)
          (string-suffix-p "cba" s)
          (string-prefix-p "a" s)
          (string-suffix-p "a" s))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t (t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: finding common prefix/suffix in a list of strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix_common_in_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Find the longest common prefix of a list of strings
  (fset 'neovm--common-prefix
    (lambda (strings)
      (if (null strings) ""
        (let* ((first (car strings))
               (len (length first))
               (prefix-len len))
          ;; Shrink prefix-len until all strings share it
          (dolist (s (cdr strings))
            (let ((i 0))
              (while (and (< i prefix-len)
                          (< i (length s))
                          (= (aref first i) (aref s i)))
                (setq i (1+ i)))
              (setq prefix-len i)))
          (substring first 0 prefix-len)))))

  ;; Find the longest common suffix of a list of strings
  (fset 'neovm--common-suffix
    (lambda (strings)
      (if (null strings) ""
        (let* ((first (car strings))
               (flen (length first))
               (suffix-len flen))
          (dolist (s (cdr strings))
            (let ((slen (length s))
                  (i 0))
              (while (and (< i suffix-len)
                          (< i slen)
                          (= (aref first (- flen 1 i))
                             (aref s (- slen 1 i))))
                (setq i (1+ i)))
              (setq suffix-len i)))
          (substring first (- flen suffix-len))))))

  (unwind-protect
      (list
       ;; Common prefix tests
       (funcall 'neovm--common-prefix '("prefix_abc" "prefix_def" "prefix_ghi"))
       (funcall 'neovm--common-prefix '("hello" "help" "heap"))
       (funcall 'neovm--common-prefix '("abc" "xyz" "123"))
       (funcall 'neovm--common-prefix '("same" "same" "same"))
       (funcall 'neovm--common-prefix '("only"))
       (funcall 'neovm--common-prefix nil)

       ;; Common suffix tests
       (funcall 'neovm--common-suffix '("file.el" "init.el" "config.el"))
       (funcall 'neovm--common-suffix '("running" "jumping" "swimming"))
       (funcall 'neovm--common-suffix '("abc" "xyz" "123"))
       (funcall 'neovm--common-suffix '("test" "test" "test"))

       ;; Verify common prefix is indeed a prefix of all
       (let* ((strs '("automobile" "automatic" "autonomy"))
              (pfx (funcall 'neovm--common-prefix strs)))
         (list pfx
               (mapcar (lambda (s) (string-prefix-p pfx s)) strs)))

       ;; Verify common suffix is indeed a suffix of all
       (let* ((strs '("test_result" "final_result" "best_result"))
              (sfx (funcall 'neovm--common-suffix strs)))
         (list sfx
               (mapcar (lambda (s) (string-suffix-p sfx s)) strs))))
    (fmakunbound 'neovm--common-prefix)
    (fmakunbound 'neovm--common-suffix)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"prefix_\" \"he\" \"\" \"same\" \"only\" \"\" \".el\" \"ing\" \"\" \"test\" (\"auto\" (t t t)) (\"_result\" (t t t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string categorization by prefix/suffix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix_categorization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Categorize files by extension using string-suffix-p
  (fset 'neovm--categorize-file
    (lambda (filename)
      (cond
       ((string-suffix-p ".el" filename) 'elisp)
       ((string-suffix-p ".rs" filename) 'rust)
       ((string-suffix-p ".py" filename) 'python)
       ((string-suffix-p ".js" filename) 'javascript)
       ((string-suffix-p ".txt" filename) 'text)
       ((string-suffix-p ".md" filename) 'markdown)
       ((string-suffix-p ".tar.gz" filename) 'archive)
       (t 'unknown))))

  ;; Categorize URLs by prefix
  (fset 'neovm--categorize-url
    (lambda (url)
      (cond
       ((string-prefix-p "https://" url) 'secure)
       ((string-prefix-p "http://" url) 'insecure)
       ((string-prefix-p "ftp://" url) 'ftp)
       ((string-prefix-p "file://" url) 'local)
       ((string-prefix-p "mailto:" url) 'email)
       (t 'unknown))))

  ;; Filter strings by prefix/suffix
  (fset 'neovm--filter-by-prefix
    (lambda (prefix strings &optional ignore-case)
      (let ((result nil))
        (dolist (s strings)
          (when (string-prefix-p prefix s ignore-case)
            (setq result (cons s result))))
        (nreverse result))))

  (fset 'neovm--filter-by-suffix
    (lambda (suffix strings &optional ignore-case)
      (let ((result nil))
        (dolist (s strings)
          (when (string-suffix-p suffix s ignore-case)
            (setq result (cons s result))))
        (nreverse result))))

  (unwind-protect
      (let ((files '("init.el" "main.rs" "app.py" "index.js" "readme.md"
                     "config.el" "lib.rs" "test.py" "data.txt" "backup.tar.gz"))
            (urls '("https://example.com" "http://test.org" "ftp://files.net"
                    "file:///tmp/local" "mailto:user@host" "unknown://foo")))
        (list
         ;; Categorize all files
         (mapcar (lambda (f) (funcall 'neovm--categorize-file f)) files)

         ;; Categorize all URLs
         (mapcar (lambda (u) (funcall 'neovm--categorize-url u)) urls)

         ;; Filter .el files
         (funcall 'neovm--filter-by-suffix ".el" files)

         ;; Filter .rs files
         (funcall 'neovm--filter-by-suffix ".rs" files)

         ;; Filter by prefix (case-insensitive)
         (funcall 'neovm--filter-by-prefix "HTTP" '("HTTPS://A" "http://b" "ftp://c" "HTTP://D") t)

         ;; Count files by category
         (let ((counts nil))
           (dolist (f files)
             (let* ((cat (funcall 'neovm--categorize-file f))
                    (pair (assq cat counts)))
               (if pair
                   (setcdr pair (1+ (cdr pair)))
                 (setq counts (cons (cons cat 1) counts)))))
           (sort counts (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b))))))))
    (fmakunbound 'neovm--categorize-file)
    (fmakunbound 'neovm--categorize-url)
    (fmakunbound 'neovm--filter-by-prefix)
    (fmakunbound 'neovm--filter-by-suffix)))"#;
    let expect = expect_test::expect![[
        r#""OK ((elisp rust python javascript markdown elisp rust python text archive) (secure insecure ftp local email unknown) (\"init.el\" \"config.el\") (\"main.rs\" \"lib.rs\") (\"HTTPS://A\" \"http://b\" \"HTTP://D\") ((archive . 1) (elisp . 2) (javascript . 1) (markdown . 1) (python . 2) (rust . 2) (text . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: prefix/suffix testing with split-string and string-join
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_prefix_suffix_with_split_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; After splitting, verify prefix/suffix of joined result
  (let* ((words (split-string "hello beautiful world" " "))
         (joined (string-join words "-")))
    (list
     (string-prefix-p "hello" joined)
     (string-suffix-p "world" joined)
     (string-prefix-p "hello-" joined)
     (string-suffix-p "-world" joined)))

  ;; Strip prefix by checking and removing
  (let ((s "prefix:important-data"))
    (if (string-prefix-p "prefix:" s)
        (substring s (length "prefix:"))
      s))

  ;; Strip suffix
  (let ((s "filename.backup"))
    (if (string-suffix-p ".backup" s)
        (substring s 0 (- (length s) (length ".backup")))
      s))

  ;; Build a trie-like prefix check
  (let ((dict '("apple" "application" "apply" "banana" "band" "bandana"))
        (prefix "app"))
    (let ((matches nil))
      (dolist (word dict)
        (when (string-prefix-p prefix word)
          (setq matches (cons word matches))))
      (nreverse matches)))

  ;; Check if one string both starts and ends with another
  (let ((test-cases '(("aba" "a") ("abcabc" "abc") ("xx" "x") ("hello" "h"))))
    (mapcar (lambda (tc)
              (let ((s (car tc)) (pat (cadr tc)))
                (list s pat
                      (and (string-prefix-p pat s)
                           (string-suffix-p pat s)))))
            test-cases)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t) \"important-data\" \"filename\" (\"apple\" \"application\" \"apply\") ((\"aba\" \"a\" t) (\"abcabc\" \"abc\" t) (\"xx\" \"x\" t) (\"hello\" \"h\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
