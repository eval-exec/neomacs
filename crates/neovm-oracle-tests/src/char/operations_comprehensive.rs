//! Comprehensive oracle parity tests for character operations:
//! char-to-string / string-to-char roundtrip, char-width for ASCII/CJK/combining,
//! char-equal with case-fold-search, characterp predicate, char-after / char-before /
//! following-char / preceding-char, char-syntax with syntax tables, downcase / upcase
//! on characters, char-table-p / char-table-range / set-char-table-range, max-char.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// char-to-string and string-to-char roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Basic ASCII roundtrip
 (string-to-char (char-to-string ?a))
 (string-to-char (char-to-string ?Z))
 (string-to-char (char-to-string ?0))
 (string-to-char (char-to-string ?\ ))
 ;; Multibyte roundtrip
 (string-to-char (char-to-string ?日))
 (string-to-char (char-to-string ?é))
 (string-to-char (char-to-string ?λ))
 (string-to-char (char-to-string ?α))
 ;; char-to-string produces single-char strings
 (length (char-to-string ?a))
 (length (char-to-string ?日))
 (length (char-to-string ?é))
 ;; string-to-char returns first char of multi-char string
 (= (string-to-char "hello") ?h)
 (= (string-to-char "日本語") ?日)
 ;; string-to-char of empty string
 (string-to-char "")
 ;; Roundtrip on control characters
 (= (string-to-char (char-to-string 1)) 1)
 (= (string-to-char (char-to-string 0)) 0)
 (= (string-to-char (char-to-string 127)) 127)
 ;; char-to-string on newline, tab
 (string= (char-to-string ?\n) "\n")
 (string= (char-to-string ?\t) "\t"))
"#;
    let expect =
        expect_test::expect![r#""OK (97 90 48 32 26085 233 955 945 1 1 1 t t 0 t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-width: ASCII, CJK, combining, control
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_char_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; ASCII chars all width 1
 (char-width ?a)
 (char-width ?Z)
 (char-width ?0)
 (char-width ?!)
 (char-width ?\ )
 ;; CJK characters are width 2
 (char-width ?日)
 (char-width ?本)
 (char-width ?語)
 (char-width ?中)
 ;; Latin extended characters are width 1
 (char-width ?é)
 (char-width ?ñ)
 (char-width ?ü)
 ;; Greek letters width 1
 (char-width ?α)
 (char-width ?β)
 (char-width ?Ω)
 ;; Tab has special width (usually 8 or variable)
 (> (char-width ?\t) 0)
 ;; Newline
 (char-width ?\n)
 ;; DEL and other control chars
 (char-width 0)
 (char-width 127)
 ;; Fullwidth latin A (U+FF21) should be width 2
 (char-width #xff21))
"#;
    let expect = expect_test::expect![r#""OK (1 1 1 1 1 2 2 2 2 1 1 1 1 1 1 t 0 2 2 2)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-equal with case-fold-search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_char_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Default case-fold-search (usually t in Emacs)
 ;; Explicit case-sensitive (nil)
 (let ((case-fold-search nil))
   (list
    (char-equal ?a ?a)
    (char-equal ?a ?A)
    (char-equal ?A ?A)
    (char-equal ?z ?z)
    (char-equal ?z ?Z)
    (char-equal ?0 ?0)
    (char-equal ?a ?b)))
 ;; Case-insensitive (t)
 (let ((case-fold-search t))
   (list
    (char-equal ?a ?A)
    (char-equal ?Z ?z)
    (char-equal ?M ?m)
    (char-equal ?a ?b)
    (char-equal ?0 ?0)
    ;; Non-ASCII: accented chars
    (char-equal ?é ?é)
    ;; Numbers are never case-folded
    (char-equal ?1 ?1)))
 ;; Edge cases
 (let ((case-fold-search nil))
   (list
    (char-equal ?\  ?\ )
    (char-equal ?\n ?\n)
    (char-equal ?\t ?\t)
    (char-equal ?a ?\n))))
"#;
    let expect =
        expect_test::expect![r#""OK ((t nil t t nil t nil) (t t t nil t t t) (t t t nil))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// characterp predicate: comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_characterp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Characters (valid char values)
 (characterp ?a)
 (characterp ?Z)
 (characterp 0)
 (characterp 65)
 (characterp 127)
 (characterp 128)
 (characterp 255)
 (characterp #x3B1)    ;; alpha
 (characterp #x65E5)   ;; CJK
 ;; max-char boundary
 (characterp (max-char))
 ;; Beyond max-char: not a character
 (characterp (1+ (max-char)))
 ;; Negative: not a character
 (characterp -1)
 (characterp -100)
 ;; Non-integer types: not characters
 (characterp nil)
 (characterp t)
 (characterp 3.14)
 (characterp "a")
 (characterp 'a)
 (characterp '(1))
 (characterp [65])
 ;; Very large integers
 (characterp most-positive-fixnum)
 ;; Zero is a valid character
 (characterp 0))
"#;
    let expect = expect_test::expect![
        r#""OK (t t t t t t t t t t nil nil nil nil nil nil nil nil nil nil nil t)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-after, char-before, following-char, preceding-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_buffer_char_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "Hello, World! 日本語")
  (list
   ;; char-after at various positions
   (char-after 1)               ;; ?H
   (char-after 2)               ;; ?e
   (char-after 7)               ;; ?\
   (char-after 8)               ;; ?W
   (char-after 15)              ;; ?日
   ;; char-after at end of buffer
   (char-after (point-max))     ;; nil
   ;; char-after beyond end
   (char-after (1+ (point-max)))  ;; nil
   ;; char-before at various positions
   (char-before 2)              ;; ?H
   (char-before 3)              ;; ?e
   (char-before (point-max))    ;; last char
   ;; char-before at beginning
   (char-before 1)              ;; nil
   ;; char-before at 0 (invalid)
   (char-before 0)              ;; nil
   ;; following-char and preceding-char depend on point
   (progn (goto-char 1)
          (list (following-char)       ;; ?H
                (preceding-char)))     ;; 0 (before buffer start)
   (progn (goto-char 6)
          (list (following-char)       ;; ?,
                (preceding-char)))     ;; ?o
   (progn (goto-char (point-max))
          (list (following-char)       ;; 0 (at end)
                (preceding-char)))     ;; last char
   ;; With narrowing
   (progn
     (narrow-to-region 3 8)
     (goto-char (point-min))
     (let ((r (list (following-char)
                    (preceding-char)
                    (char-after (point-min))
                    (char-before (point-min)))))
       (widen)
       r))))
"#;
    let expect = expect_test::expect![
        r#""OK (72 101 32 87 26085 nil nil 72 101 35486 nil nil (72 0) (44 111) (0 35486) (108 0 108 nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-syntax with various syntax tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_char_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Standard syntax table entries
 (char-syntax ?a)       ;; ?w (word)
 (char-syntax ?Z)       ;; ?w
 (char-syntax ?0)       ;; ?w
 (char-syntax ?9)       ;; ?w
 (char-syntax ?_)       ;; ?_ (symbol) in standard table
 (char-syntax ?\ )      ;; ?\  (whitespace)
 (char-syntax ?\t)      ;; ?\  (whitespace)
 (char-syntax ?\n)      ;; ?\  (whitespace or comment-end)
 (char-syntax ?\()      ;; ?\( (open paren)
 (char-syntax ?\))      ;; ?\) (close paren)
 (char-syntax ?\[)      ;; ?\( (open paren in standard)
 (char-syntax ?\])      ;; ?\) (close paren in standard)
 (char-syntax ?{)
 (char-syntax ?})
 (char-syntax ?.)       ;; ?. (punctuation)
 (char-syntax ?,)       ;; ?. (punctuation)
 (char-syntax ?;)
 (char-syntax ?\")      ;; ?\" (string delimiter)
 (char-syntax ?+)       ;; ?. (punctuation)
 (char-syntax ?-)       ;; ?. (punctuation)
 (char-syntax ?*)
 (char-syntax ?/)
 (char-syntax ?')       ;; ?' (expression prefix) or ?w
 ;; With a custom syntax table
 (with-syntax-table (copy-syntax-table)
   ;; Make _ a word constituent
   (modify-syntax-entry ?_ "w")
   ;; Make - a word constituent
   (modify-syntax-entry ?- "w")
   (list (char-syntax ?_)
         (char-syntax ?-)
         ;; Other entries unchanged
         (char-syntax ?a)
         (char-syntax ?\())))
"#;
    let expect = expect_test::expect![
        r#""OK (119 119 119 119 95 32 32 62 40 41 40 41 95 95 95 39 60 34 95 95 95 95 39 (119 119 119 40))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// downcase and upcase on characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_downcase_upcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; Basic ASCII upcase/downcase
 (downcase ?A)
 (downcase ?Z)
 (downcase ?a)   ;; already lowercase
 (downcase ?z)
 (upcase ?a)
 (upcase ?z)
 (upcase ?A)     ;; already uppercase
 (upcase ?Z)
 ;; Non-alphabetic: unchanged
 (downcase ?0)
 (upcase ?0)
 (downcase ?!)
 (upcase ?!)
 (downcase ?\ )
 (upcase ?\ )
 ;; Roundtrip: downcase(upcase(x)) for lowercase letters
 (= (downcase (upcase ?a)) ?a)
 (= (downcase (upcase ?m)) ?m)
 (= (downcase (upcase ?z)) ?z)
 ;; Roundtrip: upcase(downcase(x)) for uppercase letters
 (= (upcase (downcase ?A)) ?A)
 (= (upcase (downcase ?M)) ?M)
 (= (upcase (downcase ?Z)) ?Z)
 ;; Multibyte characters
 (downcase ?É)
 (upcase ?é)
 (downcase ?Ñ)
 (upcase ?ñ)
 ;; Characters without case
 (= (downcase ?日) ?日)
 (= (upcase ?日) ?日)
 (= (downcase ?1) ?1)
 ;; Check that downcase/upcase return characters
 (characterp (downcase ?A))
 (characterp (upcase ?a)))
"#;
    let expect = expect_test::expect![
        r#""OK (97 122 97 122 65 90 65 90 48 48 33 33 32 32 t t t t t t 233 201 241 209 t t t t t)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table operations: char-table-p, char-table-range, set-char-table-range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((ct (make-char-table 'test-table nil)))
  (list
   ;; char-table-p predicate
   (char-table-p ct)
   (char-table-p nil)
   (char-table-p t)
   (char-table-p 42)
   (char-table-p "string")
   (char-table-p [1 2 3])
   (char-table-p (make-hash-table))
   ;; Default value is nil
   (char-table-range ct nil)
   ;; Set ranges and retrieve
   (progn
     ;; Single character
     (set-char-table-range ct ?a 'alpha)
     (set-char-table-range ct ?b 'beta)
     ;; Range of characters
     (set-char-table-range ct '(?A . ?Z) 'uppercase)
     ;; nil means set default
     (set-char-table-range ct nil 'default-val)
     (list
      ;; Single char lookups
      (char-table-range ct ?a)
      (char-table-range ct ?b)
      (char-table-range ct ?c)         ;; not explicitly set
      ;; Range lookup - accessing a char in the uppercase range
      (aref ct ?A)
      (aref ct ?M)
      (aref ct ?Z)
      ;; Char outside uppercase range falls back to default
      (aref ct ?0)
      ;; Default value
      (char-table-range ct nil)
      ;; Overwrite a single char in the range
      (progn
        (set-char-table-range ct ?M 'special-m)
        (list (aref ct ?L)     ;; still uppercase
              (aref ct ?M)     ;; special-m
              (aref ct ?N)))   ;; still uppercase
      ;; char-table-p on the category table (built-in)
      (char-table-p (standard-syntax-table))))))
"#;
    let expect = expect_test::expect![
        r#""OK (t nil nil nil nil nil nil nil (alpha beta default-val uppercase uppercase uppercase default-val default-val (uppercase special-m uppercase) t))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// max-char: value and properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_max_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 ;; max-char returns an integer
 (integerp (max-char))
 ;; max-char is a valid character
 (characterp (max-char))
 ;; max-char is positive
 (> (max-char) 0)
 ;; max-char is larger than highest Unicode codepoint
 (>= (max-char) #x10FFFF)
 ;; max-char + 1 is not a character
 (not (characterp (1+ (max-char))))
 ;; max-char without arguments vs with unicode arg
 (= (max-char) (max-char))
 ;; Various comparisons
 (> (max-char) 255)
 (> (max-char) 65535)
 (> (max-char) #xFFFF)
 ;; char-to-string on max-char should work
 (stringp (char-to-string (max-char)))
 ;; max-char is >= all common chars
 (>= (max-char) ?Z)
 (>= (max-char) ?日)
 (>= (max-char) ?é)
 (>= (max-char) #x10FFFF))
"#;
    let expect = expect_test::expect![r#""OK (t t t t t t t t t t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Caesar cipher with char operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_ops_comprehensive_caesar_cipher() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--caesar-shift
    (lambda (text shift)
      "Apply Caesar cipher with given shift to TEXT."
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
        result)))

  (unwind-protect
      (let ((msg "The Quick Brown Fox Jumps Over The Lazy Dog 123!"))
        (list
         ;; Shift by 1
         (funcall 'neovm--caesar-shift msg 1)
         ;; Shift by 13 (ROT13)
         (funcall 'neovm--caesar-shift msg 13)
         ;; Shift by 26 = identity
         (string= (funcall 'neovm--caesar-shift msg 26) msg)
         ;; Shift and unshift = identity
         (string= (funcall 'neovm--caesar-shift
                            (funcall 'neovm--caesar-shift msg 7)
                            19)   ;; 26-7=19
                  msg)
         ;; ROT13 twice = identity
         (string= (funcall 'neovm--caesar-shift
                            (funcall 'neovm--caesar-shift msg 13)
                            13)
                  msg)
         ;; Preserves non-alpha characters
         (let ((shifted (funcall 'neovm--caesar-shift msg 5)))
           (and (= (aref shifted (1- (length shifted))) ?!)
                (= (aref shifted (- (length shifted) 2)) ?3)
                (= (aref shifted (- (length shifted) 3)) ?2)
                (= (aref shifted (- (length shifted) 4)) ?1)))
         ;; Case preservation
         (let ((shifted (funcall 'neovm--caesar-shift "AaBbZz" 1)))
           (list (>= (aref shifted 0) ?A) (<= (aref shifted 0) ?Z)
                 (>= (aref shifted 1) ?a) (<= (aref shifted 1) ?z)))))
    (fmakunbound 'neovm--caesar-shift)))
"#;
    let expect = expect_test::expect![[
        r#""OK (\"Uif Rvjdl Cspxo Gpy Kvnqt Pwfs Uif Mbaz Eph 123!\" \"Gur Dhvpx Oebja Sbk Whzcf Bire Gur Ynml Qbt 123!\" t t t t (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
