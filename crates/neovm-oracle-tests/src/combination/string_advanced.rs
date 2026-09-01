//! Advanced string combination oracle tests: interning semantics, multi-byte
//! operations, string-as-sequence access, comparison transitivity, word
//! wrapping, edit distance, and expression tokenization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// String interning and equality: eq vs equal on strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_interning_eq_vs_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `equal` compares contents, `eq` compares identity.
    // Two literal strings that are `equal` may or may not be `eq`
    // (implementation-defined), but `equal` must always agree on content.
    let form = r#"(let ((a "hello")
                        (b (concat "hel" "lo"))
                        (c "hello"))
                    (list (equal a b)      ;; t — same content
                          (equal a c)      ;; t
                          (equal b c)      ;; t
                          ;; eq on constructed string is always nil
                          (eq a b)         ;; nil — b is freshly consed
                          ;; string= is like equal for strings
                          (string= a b)
                          ;; copy-sequence creates a new string
                          (let ((d (copy-sequence a)))
                            (list (equal a d)
                                  (eq a d)))))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil t (t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-byte string operations: length vs string-bytes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_multibyte_length_vs_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((strings (list "ASCII only"
                                        "café"
                                        "naïve"
                                        "日本語"
                                        "emoji: λ"
                                        ""
                                        "a")))
                    (mapcar (lambda (s)
                              (list s
                                    (length s)
                                    (string-bytes s)
                                    (>= (string-bytes s) (length s))
                                    (multibyte-string-p s)))
                            strings))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"ASCII only\" 10 10 t nil) (\"café\" 4 5 t t) (\"naïve\" 5 6 t t) (\"日本語\" 3 9 t t) (\"emoji: λ\" 8 9 t t) (\"\" 0 0 t nil) (\"a\" 1 1 t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String as sequence: elt, aref, iteration via dotimes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_as_sequence_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s "Hello, World!"))
                    (let ((chars-via-aref nil)
                          (chars-via-elt nil)
                          (char-codes nil))
                      ;; Collect characters via aref
                      (dotimes (i (length s))
                        (setq chars-via-aref
                              (cons (aref s i) chars-via-aref)))
                      ;; Collect via elt
                      (dotimes (i (length s))
                        (setq chars-via-elt
                              (cons (elt s i) chars-via-elt)))
                      ;; Collect character codes
                      (dotimes (i (length s))
                        (setq char-codes
                              (cons (aref s i) char-codes)))
                      (list
                       ;; Both methods produce same result
                       (equal (nreverse chars-via-aref)
                              (nreverse chars-via-elt))
                       ;; First and last characters
                       (aref s 0)
                       (aref s (1- (length s)))
                       ;; Reconstruct string from char codes
                       (let ((rebuilt (make-string (length s) ?\ )))
                         (dotimes (i (length s))
                           (aset rebuilt i (aref s i)))
                         (string= s rebuilt)))))"#;
    let expect = expect_test::expect![[r#""OK (t 72 33 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String comparison chains: string< transitivity check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_transitivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that string< is transitive: if a < b and b < c, then a < c
    let form = r#"(let ((words '("alpha" "beta" "gamma" "delta" "epsilon"
                                  "zeta" "eta" "theta")))
                    ;; Sort the words
                    (let ((sorted (sort (copy-sequence words) #'string<)))
                      ;; Verify transitivity: each adjacent pair is in order
                      (let ((transitive t)
                            (rest sorted))
                        (while (and transitive (cdr rest))
                          (unless (string< (car rest) (cadr rest))
                            (setq transitive nil))
                          (setq rest (cdr rest)))
                        ;; Also verify non-adjacent pairs
                        (let ((all-ok t))
                          (dotimes (i (length sorted))
                            (dotimes (j (length sorted))
                              (when (< i j)
                                (unless (string< (nth i sorted)
                                                 (nth j sorted))
                                  (setq all-ok nil)))))
                          (list sorted transitive all-ok)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alpha\" \"beta\" \"delta\" \"epsilon\" \"eta\" \"gamma\" \"theta\" \"zeta\") t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: word wrapping algorithm using string operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_word_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((wrap-text
                         (lambda (text width)
                           (let ((words (split-string text " "))
                                 (lines nil)
                                 (current-line ""))
                             (dolist (word words)
                               (cond
                                ;; Empty current line: just add word
                                ((string= current-line "")
                                 (setq current-line word))
                                ;; Adding word would exceed width: start new line
                                ((> (+ (length current-line) 1 (length word))
                                    width)
                                 (setq lines (cons current-line lines))
                                 (setq current-line word))
                                ;; Otherwise append
                                (t
                                 (setq current-line
                                       (concat current-line " " word)))))
                             ;; Don't forget last line
                             (unless (string= current-line "")
                               (setq lines (cons current-line lines)))
                             (nreverse lines)))))
                    (let ((text "The quick brown fox jumps over the lazy dog and then runs away"))
                      (list
                       ;; Wrap at 20 chars
                       (funcall wrap-text text 20)
                       ;; Wrap at 10 chars
                       (funcall wrap-text text 10)
                       ;; Wrap at 80 chars (no wrapping needed)
                       (funcall wrap-text text 80)
                       ;; Verify all lines within limit for width=15
                       (let ((lines (funcall wrap-text text 15)))
                         (let ((all-ok t))
                           (dolist (line lines)
                             (when (> (length line) 15)
                               (setq all-ok nil)))
                           (list lines all-ok))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"The quick brown fox\" \"jumps over the lazy\" \"dog and then runs\" \"away\") (\"The quick\" \"brown fox\" \"jumps over\" \"the lazy\" \"dog and\" \"then runs\" \"away\") (\"The quick brown fox jumps over the lazy dog and then runs away\") ((\"The quick brown\" \"fox jumps over\" \"the lazy dog\" \"and then runs\" \"away\") t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Levenshtein distance using strings + vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_levenshtein_dp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full DP implementation of Levenshtein distance, then compare
    // with the builtin string-distance.
    let form = r#"(let ((levenshtein
                         (lambda (s1 s2)
                           (let* ((len1 (length s1))
                                  (len2 (length s2))
                                  ;; Create (len1+1) x (len2+1) matrix as vector of vectors
                                  (dp (make-vector (1+ len1) nil)))
                             ;; Initialize rows
                             (dotimes (i (1+ len1))
                               (aset dp i (make-vector (1+ len2) 0)))
                             ;; Base cases
                             (dotimes (i (1+ len1))
                               (aset (aref dp i) 0 i))
                             (dotimes (j (1+ len2))
                               (aset (aref dp 0) j j))
                             ;; Fill DP table
                             (let ((i 1))
                               (while (<= i len1)
                                 (let ((j 1))
                                   (while (<= j len2)
                                     (let ((cost (if (= (aref s1 (1- i))
                                                        (aref s2 (1- j)))
                                                     0 1)))
                                       (aset (aref dp i) j
                                             (min (1+ (aref (aref dp (1- i)) j))
                                                  (min (1+ (aref (aref dp i) (1- j)))
                                                       (+ (aref (aref dp (1- i)) (1- j))
                                                          cost)))))
                                     (setq j (1+ j))))
                                 (setq i (1+ i))))
                             (aref (aref dp len1) len2)))))
                    (let ((pairs '(("kitten" "sitting")
                                   ("" "abc")
                                   ("abc" "")
                                   ("abc" "abc")
                                   ("flaw" "lawn")
                                   ("saturday" "sunday"))))
                      (mapcar (lambda (pair)
                                (let* ((a (car pair))
                                       (b (cadr pair))
                                       (manual (funcall levenshtein a b))
                                       (builtin (string-distance a b)))
                                  (list a b manual builtin (= manual builtin))))
                              pairs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"kitten\" \"sitting\" 3 3 t) (\"\" \"abc\" 3 3 t) (\"abc\" \"\" 3 3 t) (\"abc\" \"abc\" 0 0 t) (\"flaw\" \"lawn\" 2 2 t) (\"saturday\" \"sunday\" 3 3 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string-based expression tokenizer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize a simple arithmetic expression into numbers and operators
    let form = r#"(let ((tokenize
                         (lambda (expr)
                           (let ((tokens nil)
                                 (current "")
                                 (i 0)
                                 (len (length expr)))
                             (while (< i len)
                               (let ((ch (aref expr i)))
                                 (cond
                                  ;; Whitespace: flush current token
                                  ((= ch ?\ )
                                   (unless (string= current "")
                                     (setq tokens (cons current tokens))
                                     (setq current "")))
                                  ;; Operator: flush current, add operator
                                  ((memq ch '(?+ ?- ?* ?/ ?\( ?\)))
                                   (unless (string= current "")
                                     (setq tokens (cons current tokens))
                                     (setq current ""))
                                   (setq tokens
                                         (cons (char-to-string ch) tokens)))
                                  ;; Digit or dot: accumulate
                                  (t
                                   (setq current
                                         (concat current
                                                 (char-to-string ch))))))
                               (setq i (1+ i)))
                             ;; Flush remaining
                             (unless (string= current "")
                               (setq tokens (cons current tokens)))
                             (nreverse tokens)))))
                    (list
                     (funcall tokenize "3 + 4 * 2")
                     (funcall tokenize "(10+20)*30")
                     (funcall tokenize "42")
                     (funcall tokenize "1+2-3*4/5")
                     (funcall tokenize "  100  +  200  ")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"3\" \"+\" \"4\" \"*\" \"2\") (\"(\" \"10\" \"+\" \"20\" \")\" \"*\" \"30\") (\"42\") (\"1\" \"+\" \"2\" \"-\" \"3\" \"*\" \"4\" \"/\" \"5\") (\"100\" \"+\" \"200\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string histogram (character frequency analysis)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_char_histogram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((char-freq
                         (lambda (s)
                           (let ((freq (make-hash-table :test 'equal)))
                             (dotimes (i (length s))
                               (let ((ch (char-to-string (aref s i))))
                                 (puthash ch (1+ (gethash ch freq 0)) freq)))
                             ;; Collect into sorted alist
                             (let ((pairs nil))
                               (maphash (lambda (k v)
                                          (setq pairs (cons (cons k v) pairs)))
                                        freq)
                               (sort pairs
                                     (lambda (a b)
                                       (or (> (cdr a) (cdr b))
                                           (and (= (cdr a) (cdr b))
                                                (string< (car a) (car b)))))))))))
                    (let ((text "abracadabra"))
                      (let ((freq (funcall char-freq text)))
                        (list freq
                              ;; Most frequent character
                              (car (car freq))
                              ;; Number of distinct characters
                              (length freq)
                              ;; Sum of frequencies must equal string length
                              (let ((sum 0))
                                (dolist (p freq)
                                  (setq sum (+ sum (cdr p))))
                                (= sum (length text)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"a\" . 5) (\"b\" . 2) (\"r\" . 2) (\"c\" . 1) (\"d\" . 1)) \"a\" 5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: string-based base conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_base_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert integers to arbitrary base (2-16) string representation
    let form = r#"(let ((int-to-base
                         (lambda (n base)
                           (if (= n 0) "0"
                             (let ((digits "0123456789ABCDEF")
                                   (result "")
                                   (neg (< n 0))
                                   (num (abs n)))
                               (while (> num 0)
                                 (setq result
                                       (concat (char-to-string
                                                (aref digits (% num base)))
                                               result))
                                 (setq num (/ num base)))
                               (if neg (concat "-" result) result))))))
                    (list
                     ;; Decimal 255 in various bases
                     (funcall int-to-base 255 2)
                     (funcall int-to-base 255 8)
                     (funcall int-to-base 255 10)
                     (funcall int-to-base 255 16)
                     ;; Zero
                     (funcall int-to-base 0 2)
                     ;; Powers of 2 in binary
                     (mapcar (lambda (n)
                               (funcall int-to-base (expt 2 n) 2))
                             '(0 1 2 3 4 8))
                     ;; Negative
                     (funcall int-to-base -42 10)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"11111111\" \"377\" \"255\" \"FF\" \"0\" (\"1\" \"10\" \"100\" \"1000\" \"10000\" \"100000000\") \"-42\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
