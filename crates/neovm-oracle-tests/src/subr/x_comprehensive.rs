//! Oracle parity tests for subr-x.el library comprehensive coverage:
//! string-trim, string-trim-left, string-trim-right, string-blank-p,
//! string-join, string-remove-prefix, string-remove-suffix,
//! string-clean-whitespace, string-fill, string-limit, string-pad,
//! string-chop-newline, string-replace, when-let, if-let,
//! thread-first, thread-last, named-let.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-trim, string-trim-left, string-trim-right
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_string_trim_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; string-trim: both sides
    (string-trim "  hello world  ")
    (string-trim "\t\n  spaced  \n\t")
    (string-trim "   ")
    (string-trim "")
    (string-trim "no-spaces")
    ;; string-trim with custom regexp
    (string-trim "xxxhelloxxx" "x+" "x+")
    (string-trim "---test---" "-+")
    (string-trim "...dots..." "\\.+" "\\.+")
    ;; string-trim-left
    (string-trim-left "   hello")
    (string-trim-left "\t\n  hello")
    (string-trim-left "hello   ")
    (string-trim-left "")
    (string-trim-left "xxxhello" "x+")
    ;; string-trim-right
    (string-trim-right "hello   ")
    (string-trim-right "hello\t\n  ")
    (string-trim-right "   hello")
    (string-trim-right "")
    (string-trim-right "helloxxxx" "x+")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"spaced\" \"\" \"\" \"no-spaces\" \"hello\" \"test---\" \"dots\" \"hello\" \"hello\" \"hello   \" \"\" \"hello\" \"hello\" \"hello\" \"   hello\" \"\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-blank-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_empty_p_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((empty (copy-sequence ""))
      (nonempty (copy-sequence "x")))
  (add-text-properties 0 (length nonempty) '(face bold) nonempty)
  (list
   ;; GNU lisp/simple.el defines string-empty-p as (string= string "").
   (string-empty-p "")
   (string-empty-p empty)
   (string-empty-p nonempty)
   (string-empty-p "\0")
   (string-empty-p " ")
   (condition-case err
       (string-empty-p nil)
     (error (list (car err) (cadr err))))
   (condition-case err
       (string-empty-p 'symbol)
     (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_x_string_blank_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    (string-blank-p "")
    (string-blank-p "   ")
    (string-blank-p "\t\n\r ")
    (string-blank-p "a")
    (string-blank-p " a ")
    (string-blank-p "hello world")
    (string-blank-p "\t")
    (string-blank-p "\n")
    ;; Technically not blank
    (string-blank-p "0")))"#;
    let expect = expect_test::expect![[r#""OK (0 0 0 nil nil nil 0 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_x_string_blank_p_argument_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (let ((blank (copy-sequence " \t\n\r"))
        (vertical-tab "\v")
        (form-feed "\f"))
    (add-text-properties 0 (length blank) '(face shadow) blank)
    (list
     ;; GNU subr-x.el only treats space, tab, newline, and carriage return as blank.
     (string-blank-p blank)
     (string-blank-p vertical-tab)
     (string-blank-p form-feed)
     (string-blank-p " \t\n\rx")
     (condition-case err
         (string-blank-p nil)
       (error (list (car err) (cadr err))))
     (condition-case err
         (string-blank-p 'symbol)
       (error (list (car err) (cadr err)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (0 nil nil nil (wrong-type-argument stringp) (wrong-type-argument stringp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-join
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_string_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; Basic join with separator
    (string-join '("hello" "world") " ")
    (string-join '("a" "b" "c" "d") ", ")
    (string-join '("one" "two" "three") "-")
    ;; Join with empty separator
    (string-join '("a" "b" "c") "")
    ;; Join single element
    (string-join '("alone") " ")
    ;; Join empty list
    (string-join nil " ")
    ;; Join with no separator (default)
    (string-join '("hello" "world"))
    ;; Join with multi-char separator
    (string-join '("foo" "bar" "baz") " :: ")
    ;; Join with newline
    (string-join '("line1" "line2" "line3") "\n")
    ;; Join empty strings
    (string-join '("" "" "") ",")
    ;; Join mixed empty and non-empty
    (string-join '("a" "" "b" "" "c") ",")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"a, b, c, d\" \"one-two-three\" \"abc\" \"alone\" \"\" \"helloworld\" \"foo :: bar :: baz\" \"line1\\nline2\\nline3\" \",,\" \"a,,b,,c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-remove-prefix, string-remove-suffix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_remove_prefix_suffix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; string-remove-prefix
    (string-remove-prefix "pre-" "pre-fix")
    (string-remove-prefix "pre-" "no-match")
    (string-remove-prefix "" "hello")
    (string-remove-prefix "hello" "hello")
    (string-remove-prefix "hello" "hello world")
    (string-remove-prefix "xyz" "hello")
    ;; string-remove-suffix
    (string-remove-suffix ".el" "init.el")
    (string-remove-suffix ".el" "no-match")
    (string-remove-suffix "" "hello")
    (string-remove-suffix "hello" "hello")
    (string-remove-suffix "world" "hello world")
    (string-remove-suffix ".txt" "file.el")
    ;; Chained prefix/suffix removal
    (string-remove-suffix ".el"
      (string-remove-prefix "~/.emacs.d/" "~/.emacs.d/init.el"))
    ;; Remove prefix that is longer than string
    (string-remove-prefix "very-long-prefix" "hi")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"fix\" \"no-match\" \"hello\" \"\" \" world\" \"hello\" \"init\" \"no-match\" \"hello\" \"\" \"hello \" \"file.el\" \"init\" \"hi\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_x_remove_prefix_suffix_identity_and_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (let ((s (copy-sequence "prefix-body-suffix")))
    (add-text-properties 0 (length s) '(face bold owner source) s)
    (let ((no-prefix (string-remove-prefix "missing-" s))
          (no-suffix (string-remove-suffix ".el" s))
          (prefix-hit (string-remove-prefix "prefix-" s))
          (suffix-hit (string-remove-suffix "-suffix" s)))
      (list
        ;; GNU lisp/emacs-lisp/subr-x.el returns STRING itself when
        ;; string-prefix-p/string-suffix-p says there is no match.
        (eq no-prefix s)
        (eq no-suffix s)
        ;; Match paths use substring, producing a distinct string with
        ;; retained text properties copied from the selected range.
        (equal prefix-hit "body-suffix")
        (not (eq prefix-hit s))
        (get-text-property 0 'face prefix-hit)
        (get-text-property 0 'owner prefix-hit)
        (equal suffix-hit "prefix-body")
        (not (eq suffix-hit s))
        (get-text-property 0 'face suffix-hit)
        (get-text-property 0 'owner suffix-hit)
        ;; Argument checking is inherited from string-prefix-p/suffix-p.
        (condition-case err
            (string-remove-prefix 'pre s)
          (error (list (car err) (cadr err))))
        (condition-case err
            (string-remove-suffix 'suf s)
          (error (list (car err) (cadr err))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t bold source t t bold source (wrong-type-argument sequencep) (wrong-type-argument sequencep))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-clean-whitespace, string-chop-newline, string-replace
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_string_clean_chop_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; string-clean-whitespace: collapse internal whitespace and trim
    (string-clean-whitespace "  hello   world  ")
    (string-clean-whitespace "multiple   spaces   here")
    (string-clean-whitespace "\t tabs \t and \t spaces \t")
    (string-clean-whitespace "already clean")
    (string-clean-whitespace "")
    (string-clean-whitespace "   ")
    ;; string-chop-newline
    (string-chop-newline "hello\n")
    (string-chop-newline "hello\r\n")
    (string-chop-newline "hello")
    (string-chop-newline "\n")
    (string-chop-newline "")
    (string-chop-newline "hello\n\n")
    ;; string-replace (non-regexp replacement)
    (string-replace "world" "Elisp" "hello world")
    (string-replace "o" "0" "hello world foo")
    (string-replace "aa" "b" "aaaaaa")
    (string-replace "x" "y" "no match")
    (string-replace "" "x" "hello")
    (string-replace "hello" "" "hello world hello")))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_x_string_clean_whitespace_exact_blank_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (let ((vertical-tab (string 11))
        (form-feed (string 12)))
    (list
      ;; GNU lisp/emacs-lisp/subr-x.el uses exactly
      ;; "[[:blank:]\r\n]+": space, tab, CR, and LF are collapsed/trimmed.
      (string-clean-whitespace " \t alpha \r\n beta \t ")
      ;; Vertical tab and form feed are not matched by that regexp.
      (string-clean-whitespace (concat " " vertical-tab "alpha" form-feed " "))
      (string-clean-whitespace (concat "alpha" vertical-tab " \t " form-feed "beta"))
      (string-clean-whitespace (concat vertical-tab " \t " form-feed))
      (condition-case err
          (string-clean-whitespace nil)
        (error (list (car err) (cadr err))))
      (condition-case err
          (string-clean-whitespace 'symbol)
        (error (list (car err) (cadr err)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"alpha beta\" \"\u{b}alpha\\f\" \"alpha\u{b} \\fbeta\" \"\u{b} \\f\" (wrong-type-argument arrayp) (wrong-type-argument sequencep))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-fill, string-limit, string-pad
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_string_fill_limit_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; string-fill: wrap text at column
    (string-fill "This is a long sentence that should be wrapped at a reasonable column width" 20)
    (string-fill "short" 80)
    (string-fill "" 10)
    ;; string-limit: truncate to max length
    (string-limit "hello world" 5)
    (string-limit "hello" 10)
    (string-limit "hello world" 5 t)
    (string-limit "" 5)
    (string-limit "abcdefghij" 3)
    ;; string-pad: pad to specified width
    (string-pad "hello" 10)
    (string-pad "hello" 10 ?.)
    (string-pad "hello" 10 nil t)
    (string-pad "hello" 3)
    (string-pad "" 5)
    (string-pad "hi" 8 ?- t)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"This is a long\\nsentence that should\\nbe wrapped at a\\nreasonable column\\nwidth\" \"short\" \"\" \"hello\" \"hello\" \"world\" \"\" \"abc\" \"hello     \" \"hello.....\" \"     hello\" \"hello\" \"     \" \"------hi\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_x_string_limit_pad_identity_errors_and_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (let ((ascii (copy-sequence "hello"))
        (wide (copy-sequence "éé"))
        (pad-source (copy-sequence "pad")))
    (list
      ;; GNU string-limit returns STRING itself when no truncation is needed.
      (eq (string-limit ascii 5) ascii)
      (eq (string-limit ascii 6) ascii)
      (string-limit ascii 0)
      (string-limit ascii 0 t)
      (string-limit ascii 2)
      (string-limit ascii 2 t)
      ;; With CODING-SYSTEM, LENGTH is a byte limit and partial encoded
      ;; characters must not be returned.
      (string-limit wide 1 nil 'utf-8)
      (string-limit wide 2 nil 'utf-8)
      (string-limit wide 3 nil 'utf-8)
      (string-limit wide 4 nil 'utf-8)
      (string-limit wide 2 t 'utf-8)
      (string-bytes (string-limit wide 3 nil 'utf-8))
      (multibyte-string-p (string-limit wide 3 nil 'utf-8))
      ;; GNU string-pad also returns STRING itself when no padding is needed.
      (eq (string-pad pad-source 3) pad-source)
      (eq (string-pad pad-source 2) pad-source)
      (string-pad pad-source 5)
      (string-pad pad-source 5 ?.)
      (string-pad pad-source 5 ?. t)
      (condition-case err
          (string-limit ascii -1)
        (error (list (car err) (cadr err))))
      (condition-case err
          (string-limit ascii 1.5)
        (error (list (car err) (cadr err))))
      (condition-case err
          (string-pad pad-source -1)
        (error (list (car err) (cadr err))))
      (condition-case err
          (string-pad pad-source 5 "x")
        (error (list (car err) (cadr err)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t \"\" \"\" \"he\" \"lo\" \"\" \"é\" \"é\" \"éé\" \"é\" 2 nil t t \"pad  \" \"pad..\" \"..pad\" (wrong-type-argument natnump) (wrong-type-argument natnump) (wrong-type-argument natnump) (wrong-type-argument characterp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// when-let and if-let macros
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_when_let_if_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; when-let: binding is non-nil
    (when-let ((x (+ 1 2)))
      (* x x))
    ;; when-let: binding is nil
    (when-let ((x nil))
      (error "should not reach"))
    ;; when-let with multiple bindings
    (when-let ((a 10)
               (b 20))
      (+ a b))
    ;; when-let: second binding nil stops evaluation
    (when-let ((a 10)
               (b nil))
      (error "should not reach"))
    ;; if-let: truthy branch
    (if-let ((x (assoc 'key '((key . value) (other . stuff)))))
      (cdr x)
      'not-found)
    ;; if-let: falsy branch
    (if-let ((x (assoc 'missing '((key . value)))))
      (cdr x)
      'not-found)
    ;; if-let with multiple bindings
    (if-let ((a (car '(1 2 3)))
             (b (cadr '(1 2 3))))
      (+ a b)
      'failed)
    ;; if-let where second binding is nil
    (if-let ((a 10)
             (b (memq 'z '(a b c))))
      (+ a (car b))
      'nope)
    ;; Nested when-let
    (when-let ((x '(1 2 3)))
      (when-let ((y (nth 2 x)))
        (* y 10)))))"#;
    let expect = expect_test::expect![[r#""OK (9 nil 30 nil value not-found 3 nope 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// thread-first and thread-last macros
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_thread_first_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; thread-first: value threaded as first argument
    (thread-first 5
      (+ 3)
      (* 2)
      (- 1))
    ;; thread-first with string operations
    (thread-first "  Hello World  "
      string-trim
      downcase)
    ;; thread-first with list ops
    (thread-first '(3 1 4 1 5 9)
      (append '(2 6))
      (sort #'<))
    ;; thread-first: single step
    (thread-first 42
      1+)
    ;; thread-first: no steps
    (thread-first 42)
    ;; thread-last: value threaded as last argument
    (thread-last 5
      (+ 3)
      (* 2)
      (- 1))
    ;; thread-last with sequences
    (thread-last '(1 2 3 4 5)
      (mapcar #'1+)
      (seq-filter #'cl-evenp)
      (seq-reduce #'+ 0))
    ;; thread-last: single step
    (thread-last '(1 2 3)
      (mapcar #'1+))
    ;; thread-last: no steps
    (thread-last 99)
    ;; Compare thread-first vs thread-last
    ;; (- 1 5) vs (- 5 1)
    (list (thread-first 5 (- 1))
          (thread-last 5 (- 1)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_subr_x_thread_macroexpansion_and_named_let_lexical_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr-x.el implements thread-first/thread-last through
    // internal--thread-argument, and named-let rejects expansion through eval
    // without lexical binding.
    let form = r#"(progn
  (require 'subr-x)
  (list
    (macroexpand-all '(thread-first 5 (+ 3) (* 2) -))
    (macroexpand-all '(thread-last 5 (+ 3) (* 2) -))
    (macroexpand-1 '(thread-first x f))
    (macroexpand-1 '(thread-last x f))
    (macroexpand-all '(thread-first (list 1 2)
                       (append '(3))
                       car))
    (condition-case err
        (eval '(named-let f ((x 1)) x) nil)
      (error (list (car err) (cadr err))))
    (condition-case err
        (macroexpand '(named-let f ((x 1)) x))
      (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((- (* (+ 5 3) 2)) (- (* 2 (+ 3 5))) (internal--thread-argument t x f) (internal--thread-argument nil x f) (car (append (list 1 2) '(3))) (error \"‘named-let’ requires lexical binding\") (funcall (cl-labels ((f (x) x)) #'f) 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// named-let (tail-recursive loop pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_named_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; Factorial via named-let
    (named-let fact ((n 10) (acc 1))
      (if (<= n 1)
          acc
        (fact (1- n) (* acc n))))
    ;; Fibonacci via named-let
    (named-let fib ((n 15) (a 0) (b 1))
      (if (= n 0)
          a
        (fib (1- n) b (+ a b))))
    ;; Sum of list via named-let
    (named-let sum-list ((lst '(1 2 3 4 5 6 7 8 9 10)) (acc 0))
      (if (null lst)
          acc
        (sum-list (cdr lst) (+ acc (car lst)))))
    ;; Reverse via named-let
    (named-let my-rev ((lst '(a b c d e)) (acc nil))
      (if (null lst)
          acc
        (my-rev (cdr lst) (cons (car lst) acc))))
    ;; Count down collecting
    (named-let countdown ((n 5) (acc nil))
      (if (= n 0)
          (nreverse acc)
        (countdown (1- n) (cons n acc))))
    ;; GCD via named-let
    (named-let gcd-loop ((a 48) (b 18))
      (if (= b 0)
          a
        (gcd-loop b (% a b))))
    ;; Flatten a nested list
    (named-let flatten ((tree '(1 (2 (3 4) 5) (6 7))) (acc nil))
      (cond
       ((null tree) (nreverse acc))
       ((consp (car tree))
        (flatten (cdr tree) (nreverse (flatten (car tree) (nreverse acc)))))
       (t (flatten (cdr tree) (cons (car tree) acc)))))
    ;; Binary search via named-let
    (named-let bsearch ((vec [2 5 8 12 16 23 38 56 72 91])
                        (target 23)
                        (lo 0)
                        (hi 9))
      (if (> lo hi)
          -1
        (let ((mid (/ (+ lo hi) 2)))
          (cond
           ((= (aref vec mid) target) mid)
           ((< (aref vec mid) target) (bsearch vec target (1+ mid) hi))
           (t (bsearch vec target lo (1- mid)))))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (3628800 610 55 (e d c b a) (5 4 3 2 1) 6 (5 4 3 1 2 6 7) 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// thread-first and thread-last with bare function forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_thread_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; thread-first with bare function (not a list form)
    (thread-first "  HELLO  "
      string-trim
      downcase
      upcase)
    ;; thread-last with bare function
    (thread-last '(3 1 4 1 5 9 2 6)
      (seq-uniq)
      (length))
    ;; Nested thread-first inside thread-last
    (thread-last '("  Hello " " World  " "  Foo  ")
      (mapcar (lambda (s) (thread-first s string-trim downcase)))
      (string-join " "))
    ;; thread-first with lambda
    (thread-first 10
      (+ 5)
      (* 2)
      (- 3))
    ;; thread-last building a pipeline for numbers
    (thread-last (number-sequence 1 10)
      (seq-filter #'cl-oddp)
      (seq-map (lambda (x) (* x x)))
      (seq-reduce #'+ 0))
    ;; thread-first identity: single value
    (thread-first "hello")
    ;; thread-last identity: single value
    (thread-last 42)
    ;; when-let with body that returns nil
    (when-let ((x 5))
      nil)
    ;; if-let with complex else branch
    (if-let ((x (assoc 'missing '((a . 1) (b . 2)))))
      (cdr x)
      (+ 10 20 30))))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 32)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combining subr-x functions for text processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_combined_text_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (require 'seq)
  (list
    ;; Process a CSV-like string
    (let ((csv-line "  Alice , 28 , Engineer  "))
      (thread-last (split-string csv-line ",")
        (mapcar #'string-trim)))
    ;; Build a formatted list from data
    (let ((items '("apple" "BANANA" "Cherry" "  date  ")))
      (thread-last items
        (mapcar (lambda (s) (string-trim s)))
        (mapcar #'downcase)
        (seq-sort #'string<)
        (string-join ", ")))
    ;; Process file-path-like strings
    (let ((paths '("~/.emacs.d/init.el"
                   "~/.emacs.d/lisp/utils.el"
                   "~/.emacs.d/themes/custom-theme.el")))
      (mapcar (lambda (p)
                (thread-first p
                  (string-remove-prefix "~/.emacs.d/")
                  (string-remove-suffix ".el")))
              paths))
    ;; Named-let to implement run-length encoding
    (named-let rle ((chars (string-to-list "aaabbbccddddee"))
                    (acc nil))
      (if (null chars)
          (nreverse acc)
        (named-let count-run ((rest (cdr chars))
                              (ch (car chars))
                              (cnt 1))
          (if (and rest (= (car rest) ch))
              (count-run (cdr rest) ch (1+ cnt))
            (rle rest (cons (cons (char-to-string ch) cnt) acc))))))
    ;; if-let chain for safe property access
    (let ((config '((database . ((host . "localhost")
                                 (port . 5432)
                                 (name . "mydb"))))))
      (if-let ((db (alist-get 'database config))
               (host (alist-get 'host db))
               (port (alist-get 'port db)))
        (format "%s:%d" host port)
        "not configured"))))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: named-let for complex recursive algorithms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_subr_x_named_let_advanced_algorithms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'subr-x)
  (list
    ;; Merge sort via named-let
    (named-let msort ((lst '(38 27 43 3 9 82 10)))
      (if (or (null lst) (null (cdr lst)))
          lst
        (let* ((mid (/ (length lst) 2))
               (left (named-let take ((l lst) (n mid) (acc nil))
                       (if (or (= n 0) (null l))
                           (nreverse acc)
                         (take (cdr l) (1- n) (cons (car l) acc)))))
               (right (nthcdr mid lst)))
          (named-let merge ((a (msort left))
                            (b (msort right))
                            (acc nil))
            (cond
             ((null a) (nconc (nreverse acc) b))
             ((null b) (nconc (nreverse acc) a))
             ((<= (car a) (car b))
              (merge (cdr a) b (cons (car a) acc)))
             (t (merge a (cdr b) (cons (car b) acc))))))))
    ;; Tower of Hanoi: collect moves via named-let
    (let ((moves nil))
      (named-let hanoi ((n 4) (from 'A) (to 'C) (aux 'B))
        (when (> n 0)
          (hanoi (1- n) from aux to)
          (push (list from to) moves)
          (hanoi (1- n) aux to from)))
      (list (length moves) (nth 0 (nreverse moves)) (car moves)))
    ;; Ackermann function (small values)
    (named-let ack ((m 3) (n 2))
      (cond
       ((= m 0) (1+ n))
       ((= n 0) (ack (1- m) 1))
       (t (ack (1- m) (ack m (1- n))))))
    ;; Collatz sequence length via named-let
    (named-let collatz ((n 27) (steps 0))
      (if (= n 1)
          steps
        (if (cl-evenp n)
            (collatz (/ n 2) (1+ steps))
          (collatz (+ (* 3 n) 1) (1+ steps)))))))"#;
    let expect = expect_test::expect![[r#""OK ((3 9 10 27 38 43 82) (15 (A B) (B C)) 29 111)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
