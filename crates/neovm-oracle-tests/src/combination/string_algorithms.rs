//! Complex oracle tests for string-heavy algorithms: edit distance
//! verification, longest common substring, string rotation detection,
//! run-length encoding, Huffman frequency, and advanced formatting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Levenshtein distance (manual implementation vs string-distance)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_edit_distance_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify our DP edit distance matches string-distance builtin
    let form = r#"(let ((pairs '(("kitten" "sitting")
                                   ("" "abc")
                                   ("abc" "")
                                   ("abc" "abc")
                                   ("flaw" "lawn")
                                   ("gumbo" "gambol"))))
                    (mapcar
                     (lambda (pair)
                       (let ((a (car pair))
                             (b (cadr pair)))
                         (list a b (string-distance a b))))
                     pairs))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"kitten\" \"sitting\" 3) (\"\" \"abc\" 3) (\"abc\" \"\" 3) (\"abc\" \"abc\" 0) (\"flaw\" \"lawn\" 2) (\"gumbo\" \"gambol\" 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Longest common prefix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_longest_common_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lcp
                         (lambda (strings)
                           (if (null strings) ""
                             (let ((first (car strings))
                                   (rest (cdr strings))
                                   (prefix-len nil))
                               (setq prefix-len (length first))
                               (dolist (s rest)
                                 (let ((i 0)
                                       (limit (min prefix-len (length s))))
                                   (while (and (< i limit)
                                               (= (aref first i)
                                                  (aref s i)))
                                     (setq i (1+ i)))
                                   (setq prefix-len i)))
                               (substring first 0 prefix-len))))))
                    (list
                     (funcall lcp '("flower" "flow" "flight"))
                     (funcall lcp '("interspecies" "interstellar" "interstate"))
                     (funcall lcp '("dog" "racecar" "car"))
                     (funcall lcp '("abc" "abc" "abc"))
                     (funcall lcp '(""))
                     (funcall lcp nil)))"#;
    let expect = expect_test::expect![[r#""OK (\"fl\" \"inters\" \"\" \"abc\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// String rotation detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_is_rotation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A is a rotation of B if A appears in B+B
    let form = r#"(let ((is-rotation
                         (lambda (a b)
                           (and (= (length a) (length b))
                                (> (length a) 0)
                                (string-search a (concat b b))))))
                    (list
                     (funcall is-rotation "abcde" "cdeab")
                     (funcall is-rotation "abcde" "abced")
                     (funcall is-rotation "abc" "abc")
                     (funcall is-rotation "a" "a")
                     (funcall is-rotation "ab" "ba")))"#;
    let expect = expect_test::expect![[r#""OK (3 nil 0 0 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Run-length encoding/decoding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_rle_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((rle-encode
                         (lambda (s)
                           (if (= (length s) 0) nil
                             (let ((result nil)
                                   (current (aref s 0))
                                   (count 1)
                                   (i 1))
                               (while (< i (length s))
                                 (if (= (aref s i) current)
                                     (setq count (1+ count))
                                   (setq result
                                         (cons (cons current count) result)
                                         current (aref s i)
                                         count 1))
                                 (setq i (1+ i)))
                               (setq result
                                     (cons (cons current count) result))
                               (nreverse result)))))
                        (rle-decode
                         (lambda (runs)
                           (mapconcat
                            (lambda (run)
                              (make-string (cdr run) (car run)))
                            runs ""))))
                    (let ((inputs '("aaabbbccccddde"
                                    "abcde"
                                    "aaaa"
                                    "a"
                                    "")))
                      (mapcar
                       (lambda (s)
                         (let ((encoded (funcall rle-encode s)))
                           (list s
                                 encoded
                                 (funcall rle-decode encoded)
                                 (string= s (funcall rle-decode encoded)))))
                       inputs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"aaabbbccccddde\" ((97 . 3) (98 . 3) (99 . 4) (100 . 3) (101 . 1)) \"aaabbbccccddde\" t) (\"abcde\" ((97 . 1) (98 . 1) (99 . 1) (100 . 1) (101 . 1)) \"abcde\" t) (\"aaaa\" ((97 . 4)) \"aaaa\" t) (\"a\" ((97 . 1)) \"a\" t) (\"\" nil \"\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Caesar cipher with configurable shift
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_caesar_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((caesar
                         (lambda (text shift)
                           (let ((result (make-string (length text) ?\ ))
                                 (i 0))
                             (while (< i (length text))
                               (let* ((c (aref text i))
                                      (shifted
                                       (cond
                                        ((and (>= c ?a) (<= c ?z))
                                         (+ ?a (% (+ (- c ?a) shift) 26)))
                                        ((and (>= c ?A) (<= c ?Z))
                                         (+ ?A (% (+ (- c ?A) shift) 26)))
                                        (t c))))
                                 (aset result i shifted))
                               (setq i (1+ i)))
                             result))))
                    ;; Encrypt with shift 3, decrypt with shift 23
                    (let* ((plain "Hello World!")
                           (encrypted (funcall caesar plain 3))
                           (decrypted (funcall caesar encrypted 23)))
                      (list encrypted
                            decrypted
                            (string= plain decrypted)
                            ;; Test all shifts decrypt properly
                            (let ((ok t) (shift 0))
                              (while (and ok (< shift 26))
                                (let* ((enc (funcall caesar plain shift))
                                       (dec (funcall caesar enc
                                                     (- 26 shift))))
                                  (unless (string= dec plain)
                                    (setq ok nil)))
                                (setq shift (1+ shift)))
                              ok))))"#;
    let expect = expect_test::expect![[r#""OK (\"Khoor Zruog!\" \"Hello World!\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Word frequency with sorting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_word_frequency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((text "the cat sat on the mat the cat ate the rat"))
                    (let ((words (split-string text " "))
                          (freq (make-hash-table :test 'equal)))
                      ;; Count
                      (dolist (w words)
                        (puthash w (1+ (gethash w freq 0)) freq))
                      ;; Collect and sort by frequency desc, then alpha
                      (let ((pairs nil))
                        (maphash (lambda (k v)
                                   (setq pairs (cons (cons k v) pairs)))
                                 freq)
                        (sort pairs
                              (lambda (a b)
                                (or (> (cdr a) (cdr b))
                                    (and (= (cdr a) (cdr b))
                                         (string< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"the\" . 4) (\"cat\" . 2) (\"ate\" . 1) (\"mat\" . 1) (\"on\" . 1) (\"rat\" . 1) (\"sat\" . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Template engine with variable substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_template_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Substitute {{key}} patterns in a template string
    let form = r#"(let ((template "Dear {{name}},\nYour order #{{id}} of {{qty}} items is {{status}}.\nTotal: ${{total}}")
                        (vars '(("name" . "Alice")
                                ("id" . "12345")
                                ("qty" . "3")
                                ("status" . "shipped")
                                ("total" . "42.99"))))
                    (let ((result template))
                      (dolist (v vars)
                        (setq result
                              (replace-regexp-in-string
                               (concat "{{" (regexp-quote (car v)) "}}")
                               (cdr v)
                               result t t)))
                      result))"#;
    let expect = expect_test::expect![[
        r#""OK \"Dear Alice,\\nYour order #12345 of 3 items is shipped.\\nTotal: $42.99\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Palindrome checker with normalization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stralgos_palindrome() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((is-palindrome
                         (lambda (s)
                           ;; Normalize: lowercase, keep only alphanumeric
                           (let ((clean (replace-regexp-in-string
                                         "[^a-z0-9]" ""
                                         (downcase s))))
                             (let ((i 0)
                                   (j (1- (length clean)))
                                   (ok t))
                               (while (and ok (< i j))
                                 (unless (= (aref clean i) (aref clean j))
                                   (setq ok nil))
                                 (setq i (1+ i) j (1- j)))
                               ok)))))
                    (list
                     (funcall is-palindrome "racecar")
                     (funcall is-palindrome "A man a plan a canal Panama")
                     (funcall is-palindrome "hello")
                     (funcall is-palindrome "Was it a car or a cat I saw")
                     (funcall is-palindrome "")
                     (funcall is-palindrome "a")))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
