//! Oracle parity tests for advanced `make-string` usage:
//! various lengths (0, 1, large), multibyte characters, combined with
//! `aset` to modify specific positions, `make-string` + `concat`,
//! `make-string` as padding, `make-string` with MULTIBYTE arg (3rd parameter),
//! and use in formatting utilities.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Various lengths: 0, 1, boundary, and moderately large
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_various_lengths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Zero length
                    (make-string 0 ?x)
                    (length (make-string 0 ?x))
                    (stringp (make-string 0 ?x))
                    ;; Length 1
                    (make-string 1 ?Z)
                    (length (make-string 1 ?Z))
                    ;; Moderate length
                    (length (make-string 100 ?-))
                    (string= (make-string 5 ?.) ".....")
                    ;; Large length — verify length and first/last chars
                    (let ((s (make-string 500 ?#)))
                      (list (length s)
                            (aref s 0)
                            (aref s 499)
                            (string= s (make-string 500 ?#))))
                    ;; Very large
                    (length (make-string 10000 ?a)))"#;
    let expect = expect_test::expect![[r#""OK (\"\" 0 t \"Z\" 1 100 t (500 35 35 t) 10000)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_multibyte_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use various non-ASCII characters and verify length, string contents
    let form = r#"(list
                    ;; Latin extended
                    (let ((s (make-string 3 ?\u00e9)))  ; e-acute
                      (list (length s) (aref s 0) (aref s 1)
                            (multibyte-string-p s)))
                    ;; CJK character
                    (let ((s (make-string 4 ?\u4e16)))  ; "world" in Chinese
                      (list (length s) (aref s 0) (aref s 3)))
                    ;; Emoji-range character
                    (let ((s (make-string 2 ?\u2764)))  ; heart
                      (list (length s) (aref s 0)))
                    ;; Greek letter
                    (let ((s (make-string 5 ?\u03b1)))  ; alpha
                      (list (length s)
                            (string= s (concat (make-string 3 ?\u03b1)
                                               (make-string 2 ?\u03b1)))))
                    ;; Mixing: make-string produces uniform, then compare with hand-built
                    (let ((s (make-string 3 ?\u00f1)))  ; n-tilde
                      (equal s (string ?\u00f1 ?\u00f1 ?\u00f1))))"#;
    let expect =
        expect_test::expect![[r#""OK ((3 233 233 t) (4 19990 19990) (2 10084) (5 t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string combined with aset to modify specific positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_aset_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create strings with make-string, then use aset to modify them
    let form = r#"(list
                    ;; Basic: create and modify single position
                    (let ((s (make-string 5 ?a)))
                      (aset s 2 ?X)
                      s)
                    ;; Modify first and last positions
                    (let ((s (make-string 6 ?-)))
                      (aset s 0 ?[)
                      (aset s 5 ?])
                      s)
                    ;; Build a pattern by modifying alternating positions
                    (let ((s (make-string 10 ?.)))
                      (let ((i 0))
                        (while (< i 10)
                          (when (= (% i 2) 0)
                            (aset s i ?*))
                          (setq i (1+ i))))
                      s)
                    ;; Create a string and fill it with ascending letters
                    (let ((s (make-string 26 ?a)))
                      (let ((i 0))
                        (while (< i 26)
                          (aset s i (+ ?a i))
                          (setq i (1+ i))))
                      s)
                    ;; Modify to create a "frame" border
                    (let ((s (make-string 8 ?\s)))
                      (aset s 0 ?+)
                      (aset s 7 ?+)
                      (let ((i 1))
                        (while (< i 7)
                          (aset s i ?-)
                          (setq i (1+ i))))
                      s)
                    ;; Verify original is mutated (not a copy)
                    (let ((s (make-string 3 ?a)))
                      (let ((ref s))
                        (aset ref 1 ?B)
                        (list s ref (eq s ref)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"aaXaa\" \"[----]\" \"*.*.*.*.*.\" \"abcdefghijklmnopqrstuvwxyz\" \"+------+\" (\"aBa\" \"aBa\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string combined with concat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_with_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build complex strings by concatenating make-string pieces
    let form = r#"(list
                    ;; Simple concat of two make-strings
                    (concat (make-string 3 ?a) (make-string 3 ?b))
                    ;; Build a horizontal rule with decorative ends
                    (concat "<<" (make-string 20 ?=) ">>")
                    ;; Multi-part assembly
                    (concat (make-string 5 ?*)
                            " TITLE "
                            (make-string 5 ?*))
                    ;; Nested structure: indent levels
                    (let ((result nil))
                      (dotimes (level 5)
                        (setq result
                              (cons (concat (make-string (* level 2) ?\s) "- item")
                                    result)))
                      (nreverse result))
                    ;; Table row with fixed-width columns
                    (let ((pad (lambda (s width)
                                 (if (>= (length s) width) s
                                   (concat s (make-string (- width (length s)) ?\s))))))
                      (concat "|"
                              (funcall pad "Name" 10) "|"
                              (funcall pad "Age" 5) "|"
                              (funcall pad "City" 12) "|"))
                    ;; Repeat pattern using concat in a loop
                    (let ((s ""))
                      (dotimes (i 5)
                        (setq s (concat s (make-string (1+ i) ?#) " ")))
                      s))"#;
    let expect = expect_test::expect![[
        r##""OK (\"aaabbb\" \"<<====================>>\" \"***** TITLE *****\" (\"- item\" \"  - item\" \"    - item\" \"      - item\" \"        - item\") \"|Name      |Age  |City        |\" \"# ## ### #### ##### \")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string as padding utility
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_padding_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use make-string to implement left-pad, right-pad, and center-pad
    let form = r#"(progn
  ;; Right-pad: add spaces on the right to reach target width
  (fset 'neovm--test-rpad
    (lambda (s width)
      (let ((len (length s)))
        (if (>= len width) s
          (concat s (make-string (- width len) ?\s))))))

  ;; Left-pad: add fill char on the left
  (fset 'neovm--test-lpad
    (lambda (s width ch)
      (let ((len (length s)))
        (if (>= len width) s
          (concat (make-string (- width len) ch) s)))))

  ;; Center: pad equally on both sides, prefer extra on right
  (fset 'neovm--test-center
    (lambda (s width)
      (let* ((len (length s))
             (total-pad (max 0 (- width len)))
             (left-pad (/ total-pad 2))
             (right-pad (- total-pad left-pad)))
        (concat (make-string left-pad ?\s)
                s
                (make-string right-pad ?\s)))))

  (unwind-protect
      (list
       ;; Right-pad
       (funcall 'neovm--test-rpad "hi" 10)
       (length (funcall 'neovm--test-rpad "hi" 10))
       (funcall 'neovm--test-rpad "toolong" 3)
       ;; Left-pad with zeros (number formatting)
       (funcall 'neovm--test-lpad "42" 6 ?0)
       (funcall 'neovm--test-lpad "1" 4 ?0)
       (funcall 'neovm--test-lpad "12345" 3 ?0)
       ;; Center alignment
       (funcall 'neovm--test-center "hi" 10)
       (length (funcall 'neovm--test-center "hi" 10))
       (funcall 'neovm--test-center "title" 11)
       (funcall 'neovm--test-center "x" 6)
       ;; Build a formatted table
       (let ((rows '(("Alice" "95") ("Bob" "82") ("Carol" "100"))))
         (mapcar (lambda (row)
                   (concat "| "
                           (funcall 'neovm--test-rpad (nth 0 row) 8)
                           " | "
                           (funcall 'neovm--test-lpad (nth 1 row) 5 ?\s)
                           " |"))
                 rows)))
    (fmakunbound 'neovm--test-rpad)
    (fmakunbound 'neovm--test-lpad)
    (fmakunbound 'neovm--test-center)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hi        \" 10 \"toolong\" \"000042\" \"0001\" \"12345\" \"    hi    \" 10 \"   title   \" \"  x   \" (\"| Alice    |    95 |\" \"| Bob      |    82 |\" \"| Carol    |   100 |\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string with MULTIBYTE arg (3rd parameter)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_multibyte_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The 3rd argument to make-string controls whether the result is multibyte.
    // When nil and char < 256, result is unibyte. When non-nil, result is multibyte.
    let form = r#"(list
                    ;; Default (no 3rd arg): multibyte for ASCII
                    (let ((s (make-string 3 ?a)))
                      (list (length s) (multibyte-string-p s) s))
                    ;; Explicit nil: unibyte
                    (let ((s (make-string 3 ?a nil)))
                      (list (length s) (multibyte-string-p s)))
                    ;; Explicit t: multibyte
                    (let ((s (make-string 3 ?a t)))
                      (list (length s) (multibyte-string-p s)))
                    ;; Unibyte with raw byte value (e.g., 200)
                    (let ((s (make-string 4 200 nil)))
                      (list (length s) (multibyte-string-p s) (aref s 0)))
                    ;; Compare unibyte vs multibyte versions
                    (let ((uni (make-string 5 ?z nil))
                          (multi (make-string 5 ?z t)))
                      (list (string= uni multi)
                            (equal uni multi)
                            (multibyte-string-p uni)
                            (multibyte-string-p multi)
                            (length uni)
                            (length multi)))
                    ;; Multibyte char always produces multibyte regardless of 3rd arg
                    (let ((s1 (make-string 2 ?\u00e9))
                          (s2 (make-string 2 ?\u00e9 t)))
                      (list (multibyte-string-p s1)
                            (multibyte-string-p s2)
                            (string= s1 s2))))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 nil \"aaa\") (3 nil) (3 t) (4 t 200) (t t nil t 5 5) (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string in formatting utilities
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_formatting_utilities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use make-string to build complex formatted output
    let form = r#"(progn
  ;; Box-drawing: create a text box around content
  (fset 'neovm--test-text-box
    (lambda (lines)
      (let* ((max-width (apply #'max (mapcar #'length lines)))
             (border (concat "+" (make-string (+ max-width 2) ?-) "+"))
             (padded (mapcar
                      (lambda (line)
                        (concat "| " line
                                (make-string (- max-width (length line)) ?\s)
                                " |"))
                      lines)))
        (append (list border) padded (list border)))))

  ;; Progress bar
  (fset 'neovm--test-progress-bar
    (lambda (current total width)
      (let* ((filled (/ (* current width) total))
             (empty (- width filled)))
        (concat "[" (make-string filled ?#)
                (make-string empty ?.) "]"
                (format " %d%%" (/ (* current 100) total))))))

  ;; Tree indent
  (fset 'neovm--test-tree-indent
    (lambda (depth is-last)
      (if (= depth 0) ""
        (concat (make-string (* (1- depth) 4) ?\s)
                (if is-last "`-- " "|-- ")))))

  (unwind-protect
      (list
       ;; Text box
       (funcall 'neovm--test-text-box '("Hello" "World!" "OK"))
       ;; Progress bars at various levels
       (funcall 'neovm--test-progress-bar 0 100 20)
       (funcall 'neovm--test-progress-bar 50 100 20)
       (funcall 'neovm--test-progress-bar 100 100 20)
       (funcall 'neovm--test-progress-bar 75 100 20)
       ;; Tree indentation
       (funcall 'neovm--test-tree-indent 0 nil)
       (funcall 'neovm--test-tree-indent 1 nil)
       (funcall 'neovm--test-tree-indent 1 t)
       (funcall 'neovm--test-tree-indent 2 nil)
       (funcall 'neovm--test-tree-indent 3 t)
       ;; Combined: formatted list
       (let ((items '("root"
                       ("child-1" ("grandchild-a") ("grandchild-b"))
                       ("child-2" ("leaf")))))
         (let ((result nil))
           (dolist (item items)
             (if (stringp item)
                 (setq result (cons item result))
               (setq result (cons (concat "  " (car item)) result))
               (dolist (sub (cdr item))
                 (setq result (cons (concat "    " (car sub)) result)))))
           (nreverse result))))
    (fmakunbound 'neovm--test-text-box)
    (fmakunbound 'neovm--test-progress-bar)
    (fmakunbound 'neovm--test-tree-indent)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"+--------+\" \"| Hello  |\" \"| World! |\" \"| OK     |\" \"+--------+\") \"[....................] 0%\" \"[##########..........] 50%\" \"[####################] 100%\" \"[###############.....] 75%\" \"\" \"|-- \" \"`-- \" \"    |-- \" \"        `-- \" (\"root\" \"  child-1\" \"    grandchild-a\" \"    grandchild-b\" \"  child-2\" \"    leaf\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-string used for string building algorithms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_string_adv_string_building_algorithms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use make-string as a mutable buffer for algorithms
    let form = r#"(list
                    ;; Caesar cipher: shift each char by 3
                    (let ((plaintext "helloworld")
                          (result (make-string 10 ?a)))
                      (let ((i 0))
                        (while (< i 10)
                          (let* ((ch (aref plaintext i))
                                 (shifted (+ (% (+ (- ch ?a) 3) 26) ?a)))
                            (aset result i shifted))
                          (setq i (1+ i))))
                      (list result
                            ;; Decrypt: shift back by 3
                            (let ((decrypted (make-string 10 ?a)))
                              (let ((j 0))
                                (while (< j 10)
                                  (let* ((ch (aref result j))
                                         (unshifted (+ (% (+ (- ch ?a) 23) 26) ?a)))
                                    (aset decrypted j unshifted))
                                  (setq j (1+ j))))
                              decrypted)))
                    ;; Run-length encoding output using make-string
                    (let ((input "aaabbbccddddde"))
                      (let ((result "")
                            (i 0)
                            (len (length input)))
                        (while (< i len)
                          (let ((ch (aref input i))
                                (count 1))
                            (while (and (< (+ i count) len)
                                        (= (aref input (+ i count)) ch))
                              (setq count (1+ count)))
                            (setq result (concat result
                                                 (number-to-string count)
                                                 (make-string 1 ch)))
                            (setq i (+ i count))))
                        result))
                    ;; Reverse a string using make-string as buffer
                    (let* ((s "abcdefghij")
                           (len (length s))
                           (rev (make-string len ?x)))
                      (let ((i 0))
                        (while (< i len)
                          (aset rev (- len 1 i) (aref s i))
                          (setq i (1+ i))))
                      rev))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"khoorzruog\" \"helloworld\") \"3a3b2c5d1e\" \"jihgfedcba\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
