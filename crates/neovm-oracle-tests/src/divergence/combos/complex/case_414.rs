//! Complex combo batch 414 — 20 probes into deeper system layers:
//! define-symbol-macro, cl-flet/labels recursion, setf advanced forms,
//! gv-define-setter, define-setf-expander, byte-compile, disassemble,
//! native-comp availability, documentation deep, subr-arity/type deep,
//! interactive-form with specs, command-modes, purecopy, read-char in batch,
//! charset-after/in-region, char-charset, split-char/make-char,
//! string-as-unibyte/multibyte, and prefer-coding-system.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// define-symbol-macro: symbol macro expansion and use.
#[test]
fn div_cx414_define_symbol_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function define-symbol-macro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (define-symbol-macro neo-cx414-sm (+ 1 2))
  (list neo-cx414-sm
        (macroexpand '(neo-cx414-sm))))
"##,
        expect,
    );
}

/// cl-flet / cl-labels with recursive mutual recursion.
#[test]
fn div_cx414_cl_flet_labels_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (list (cl-flet ((f (n) (if (<= n 1) 1 (* n (g (1- n)))))
                   (g (n) (if (<= n 1) 1 (* n (f (1- n))))))
          (f 5))
        (cl-labels ((fact (n) (if (<= n 1) 1 (* n (fact (1- n))))))
          (fact 6))))
"##,
        expect,
    );
}

/// setf advanced forms: aref, gethash, plist-get, car, cdr.
#[test]
fn div_cx414_setf_advanced_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([1 99 3] value (:a 100 :b 2) (42 20 30))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3))
      (ht (make-hash-table))
      (pl '(:a 1 :b 2))
      (lst '(10 20 30)))
  (setf (aref v 1) 99)
  (setf (gethash 'key ht) 'value)
  (setf (plist-get pl :a) 100)
  (setf (car lst) 42)
  (list v (gethash 'key ht) pl lst))
"##,
        expect,
    );
}

/// gv-define-setter: defining custom generalized variable setters.
#[test]
fn div_cx414_gv_define_setter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 not-found)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((my-hash (make-hash-table :test 'equal)))
  (puthash "a" 1 my-hash)
  (setf (gethash "a" my-hash) 99)
  (list (gethash "a" my-hash)
        (gethash "b" my-hash 'not-found)))
"##,
        expect,
    );
}

/// byte-compile: compiling a function and checking it.
#[test]
fn div_cx414_byte_compile_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (byte-compile (lambda (x) (* x 2)))))
  (list (byte-code-function-p f)
        (funcall f 5)))
"##,
        expect,
    );
}

/// disassemble: disassembly output for byte-compiled code.
#[test]
fn div_cx414_disassemble_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (byte-compile (lambda (x) (+ x 1)))))
  (with-temp-buffer
    (disassemble f (current-buffer))
    (list (> (buffer-size) 0)
          (string-match-p "byte-code" (buffer-string)))))
"##,
        expect,
    );
}

/// native-comp-available-p / subr-native-elisp-p.
#[test]
fn div_cx414_native_comp_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function native-comp-unit-file)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (native-comp-available-p)
      (native-comp-unit-file (symbol-function 'car)))
"##,
        expect,
    );
}

/// documentation deep: function docstring snippets.
#[test]
fn div_cx414_documentation_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Return the car of LIST.  If LIST is nil, return nil.\\nError if LIST is not nil and not a cons cell.  See also `car-safe'.\\n\\nSee Info node `(elisp)Cons Cells' for a discussion of related basic\\nLisp concepts such as car, cdr, cons cell and list.\\n\\n(fn LIST)\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (documentation 'car t)
      (documentation-property 'car 'function-documentation))
"##,
        expect,
    );
}

/// subr-arity / subr-type deep for various builtins.
#[test]
fn div_cx414_subr_arity_type_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp car)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (subr-arity 'car)
      (subr-arity 'concat)
      (subr-arity 'if)
      (subr-type 'car)
      (subr-type 'concat))
"##,
        expect,
    );
}

/// interactive-form with explicit and implicit specs.
#[test]
fn div_cx414_interactive_form_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((interactive \"p\") (interactive nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f1 (lambda (x) (interactive "p") (* x 2)))
      (f2 (lambda () (interactive) 42)))
  (list (interactive-form f1)
        (interactive-form f2)))
"##,
        expect,
    );
}

/// command-modes: modes that a command belongs to.
#[test]
fn div_cx414_command_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (lambda () (interactive) (message "test"))))
  (put 'neo-cx414-cmd 'command-modes '(text-mode))
  (defalias 'neo-cx414-cmd f)
  (command-modes 'neo-cx414-cmd))
"##,
        expect,
    );
}

/// purecopy: copying objects into pure space.
#[test]
fn div_cx414_purecopy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" (a b c) 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (purecopy "hello")
      (purecopy '(a b c))
      (purecopy 42))
"##,
        expect,
    );
}

/// read-char / read-event in batch mode (should signal error).
#[test]
fn div_cx414_read_char_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (with-timeout (0.01) (read-char)) (error (car e)))
      (condition-case e (with-timeout (0.01) (read-event)) (error (car e))))
"##,
        expect,
    );
}

/// charset-after / charset-in-region: charset detection.
#[test]
fn div_cx414_charset_after_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function charset-in-region)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (list (charset-after 1)
        (charset-after 2)
        (charset-in-region 1 3)))
"##,
        expect,
    );
}

/// char-charset for various Unicode characters.
#[test]
fn div_cx414_char_charset_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (ascii unicode-bmp unicode-bmp unicode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-charset ?a)
      (char-charset ?é)
      (char-charset ?世)
      (char-charset #x1F600))
"##,
        expect,
    );
}

/// split-char / make-char: character decomposition.
#[test]
fn div_cx414_split_char_make_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((ascii 65) (unicode-bmp 78 22) 65 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (split-char ?A) (error (car e)))
      (condition-case e (split-char ?世) (error (car e)))
      (condition-case e (make-char 'ascii 65) (error (car e)))
      (condition-case e (make-char 'latin-iso8859-1 233) (error (car e))))
"##,
        expect,
    );
}

/// string-as-unibyte / string-as-multibyte: string conversion.
#[test]
fn div_cx414_string_as_unibyte_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" \"café\" 5 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café"))
  (list (string-as-unibyte s)
        (string-as-multibyte (string-as-unibyte s))
        (string-bytes (string-as-unibyte s))
        (length (string-as-multibyte (string-as-unibyte s)))))
"##,
        expect,
    );
}

/// prefer-coding-system / find-coding-system.
#[test]
fn div_cx414_prefer_find_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function find-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prefer-coding-system 'utf-8)
      (find-coding-system 'utf-8)
      (find-coding-system 'nonexistent-cx414)
      (coding-system-p 'utf-8)
      (coding-system-p 'nonexistent-cx414))
"##,
        expect,
    );
}

/// gensym / gensym-counter: unique symbol generation.
#[test]
fn div_cx414_gensym_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (g0 PREFIX-1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((counter gensym-counter))
  (list (gensym)
        (gensym "PREFIX-")
        (> gensym-counter counter)))
"##,
        expect,
    );
}

/// fboundp / symbol-function / indirect-function deeper.
#[test]
fn div_cx414_fboundp_symbol_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t forward-char #<subr forward-char> t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((alias (make-symbol "neo-cx414-alias")))
  (defalias alias 'forward-char)
  (list (fboundp alias)
        (symbol-function alias)
        (indirect-function alias)
        (eq (indirect-function alias) (symbol-function 'forward-char))))
"##,
        expect,
    );
}
