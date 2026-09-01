//! Comprehensive oracle parity tests for interactive forms and command definitions.
//!
//! Tests `interactive` spec codes, `interactive-form` extraction, `commandp`
//! with various arg types, `call-interactively` mock patterns,
//! `command-execute` vs `funcall`, prefix-arg interaction, and
//! `current-prefix-arg`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// interactive-form extraction from different function types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_form_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Define various kinds of interactive functions
  (fset 'neovm--test-if-noargs
    (lambda () (interactive) 'done))
  (fset 'neovm--test-if-string-spec
    (lambda (n) (interactive "p") n))
  (fset 'neovm--test-if-list-spec
    (lambda (a b) (interactive (list 1 2)) (+ a b)))
  (fset 'neovm--test-if-multi-spec
    (lambda (str num) (interactive "sEnter: \nnNumber: ") (list str num)))
  (fset 'neovm--test-if-noninteractive
    (lambda (x) x))

  (unwind-protect
      (list
        ;; interactive-form returns the interactive form
        (interactive-form 'neovm--test-if-noargs)
        (interactive-form 'neovm--test-if-string-spec)
        (interactive-form 'neovm--test-if-list-spec)
        (interactive-form 'neovm--test-if-multi-spec)
        ;; Non-interactive returns nil
        (interactive-form 'neovm--test-if-noninteractive)
        ;; interactive-form on lambda directly
        (interactive-form (lambda () (interactive) t))
        (interactive-form (lambda (x) x))
        ;; interactive-form on built-in commands
        (not (null (interactive-form 'forward-char)))
        (null (interactive-form 'car)))
    (fmakunbound 'neovm--test-if-noargs)
    (fmakunbound 'neovm--test-if-string-spec)
    (fmakunbound 'neovm--test-if-list-spec)
    (fmakunbound 'neovm--test-if-multi-spec)
    (fmakunbound 'neovm--test-if-noninteractive)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((interactive nil) (interactive \"p\") (interactive (list 1 2)) (interactive \"sEnter: \\nnNumber: \") nil (interactive nil) nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_builtin_delete_region_uses_registered_region_interactive_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU editfns.c registers delete-region with intspec "r".  data.c
    // exposes it through interactive-form, and callint.c expands it into
    // point and mark rather than invoking the two-argument subr with no args.
    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (goto-char 2)
  (set-mark 5)
  (call-interactively 'delete-region)
  (list (interactive-form 'delete-region)
        (buffer-string)
        (point)))
"#;
    let expect = expect_test::expect![[r#""OK ((interactive \"r\") \"aef\" 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// commandp with various argument types and optional for-call-interactively
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_commandp_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Interactive lambda: t
  (commandp (lambda () (interactive) t))
  ;; Non-interactive lambda: nil
  (commandp (lambda () t))
  ;; Interactive with string spec
  (commandp (lambda (n) (interactive "p") n))
  ;; Interactive with list spec
  (commandp (lambda (a b) (interactive (list 1 2)) (+ a b)))
  ;; Built-in interactive commands
  (commandp 'forward-char)
  (commandp 'goto-char)
  (commandp 'beginning-of-buffer)
  ;; Built-in non-interactive functions
  (commandp 'car)
  (commandp 'cons)
  (commandp '+)
  (commandp 'length)
  ;; Non-function types
  (commandp 42)
  (commandp nil)
  (commandp t)
  (commandp "hello")
  (commandp '(1 2 3))
  (commandp [1 2 3])
  ;; Quoted lambda is also commandp if it has interactive
  (commandp '(lambda () (interactive) t))
  (commandp '(lambda () t))
  ;; commandp with for-call-interactively = t (second arg)
  ;; This further restricts: autoloads with non-interactive must be nil
  (commandp 'forward-char t)
  (commandp 'car t)
  (commandp (lambda () (interactive) t) t)
  (commandp (lambda () t) t))
"#;
    let expect = expect_test::expect![[
        r#""OK (t nil t t t t t nil nil nil nil nil nil nil t nil t t nil t nil t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive spec codes: testing various single-letter codes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_spec_codes_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // We can't actually read user input in tests, but we CAN verify
    // that functions with these specs are valid commands and that
    // interactive-form returns the correct spec string
    let form = r#"
(progn
  ;; Define functions with various interactive spec codes
  (fset 'neovm--test-spec-d (lambda (n) (interactive "d") n))    ;; point as number
  (fset 'neovm--test-spec-e (lambda (ev) (interactive "e") ev))  ;; last event
  (fset 'neovm--test-spec-i (lambda () (interactive "i") 42))    ;; irrelevant (no arg)
  (fset 'neovm--test-spec-m (lambda (n) (interactive "m") n))    ;; mark as number
  (fset 'neovm--test-spec-p (lambda (n) (interactive "p") n))    ;; prefix arg as number
  (fset 'neovm--test-spec-P (lambda (n) (interactive "P") n))    ;; raw prefix arg
  (fset 'neovm--test-spec-r (lambda (beg end) (interactive "r") (list beg end)))  ;; region
  (fset 'neovm--test-spec-s (lambda (s) (interactive "sInput: ") s))  ;; string
  (fset 'neovm--test-spec-S (lambda (s) (interactive "SSymbol: ") s)) ;; symbol
  (fset 'neovm--test-spec-n (lambda (n) (interactive "nNumber: ") n)) ;; number
  (fset 'neovm--test-spec-x (lambda (x) (interactive "xExpr: ") x))  ;; lisp expression
  (fset 'neovm--test-spec-X (lambda (x) (interactive "XExpr: ") x))  ;; evaluated expression

  (unwind-protect
      (list
        ;; All are commands
        (commandp 'neovm--test-spec-d)
        (commandp 'neovm--test-spec-e)
        (commandp 'neovm--test-spec-i)
        (commandp 'neovm--test-spec-m)
        (commandp 'neovm--test-spec-p)
        (commandp 'neovm--test-spec-P)
        (commandp 'neovm--test-spec-r)
        (commandp 'neovm--test-spec-s)
        (commandp 'neovm--test-spec-S)
        (commandp 'neovm--test-spec-n)
        (commandp 'neovm--test-spec-x)
        (commandp 'neovm--test-spec-X)
        ;; Extract interactive forms
        (interactive-form 'neovm--test-spec-d)
        (interactive-form 'neovm--test-spec-p)
        (interactive-form 'neovm--test-spec-r)
        (interactive-form 'neovm--test-spec-s)
        ;; funcall still works normally (bypasses interactive)
        (funcall 'neovm--test-spec-p 42)
        (funcall 'neovm--test-spec-s "hello")
        (funcall 'neovm--test-spec-n 99))
    (fmakunbound 'neovm--test-spec-d)
    (fmakunbound 'neovm--test-spec-e)
    (fmakunbound 'neovm--test-spec-i)
    (fmakunbound 'neovm--test-spec-m)
    (fmakunbound 'neovm--test-spec-p)
    (fmakunbound 'neovm--test-spec-P)
    (fmakunbound 'neovm--test-spec-r)
    (fmakunbound 'neovm--test-spec-s)
    (fmakunbound 'neovm--test-spec-S)
    (fmakunbound 'neovm--test-spec-n)
    (fmakunbound 'neovm--test-spec-x)
    (fmakunbound 'neovm--test-spec-X)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t t t t (interactive \"d\") (interactive \"p\") (interactive \"r\") (interactive \"sInput: \") 42 \"hello\" 99)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive spec codes for buffer/file operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_spec_codes_buffer_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; b: existing buffer name, B: buffer name (may not exist)
  ;; f: existing file, F: file name (may not exist)
  ;; D: directory name
  (fset 'neovm--test-spec-b (lambda (buf) (interactive "bBuffer: ") buf))
  (fset 'neovm--test-spec-B (lambda (buf) (interactive "BBuffer: ") buf))
  (fset 'neovm--test-spec-f (lambda (f) (interactive "fFile: ") f))
  (fset 'neovm--test-spec-F (lambda (f) (interactive "FFile: ") f))
  (fset 'neovm--test-spec-D (lambda (d) (interactive "DDirectory: ") d))

  (unwind-protect
      (list
        ;; All are interactive commands
        (commandp 'neovm--test-spec-b)
        (commandp 'neovm--test-spec-B)
        (commandp 'neovm--test-spec-f)
        (commandp 'neovm--test-spec-F)
        (commandp 'neovm--test-spec-D)
        ;; interactive-form returns correct specs
        (interactive-form 'neovm--test-spec-b)
        (interactive-form 'neovm--test-spec-B)
        (interactive-form 'neovm--test-spec-f)
        (interactive-form 'neovm--test-spec-F)
        (interactive-form 'neovm--test-spec-D)
        ;; funcall bypasses interactive
        (funcall 'neovm--test-spec-b "test-buffer")
        (funcall 'neovm--test-spec-F "/tmp/test.txt")
        (funcall 'neovm--test-spec-D "/tmp/"))
    (fmakunbound 'neovm--test-spec-b)
    (fmakunbound 'neovm--test-spec-B)
    (fmakunbound 'neovm--test-spec-f)
    (fmakunbound 'neovm--test-spec-F)
    (fmakunbound 'neovm--test-spec-D)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t (interactive \"bBuffer: \") (interactive \"BBuffer: \") (interactive \"fFile: \") (interactive \"FFile: \") (interactive \"DDirectory: \") \"test-buffer\" \"/tmp/test.txt\" \"/tmp/\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive spec with multiple arguments and newline separation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_multi_arg_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Multiple args separated by \n in spec string
  (fset 'neovm--test-multi1
    (lambda (s n) (interactive "sString: \nnNumber: ") (list s n)))
  (fset 'neovm--test-multi2
    (lambda (a b c)
      (interactive "nFirst: \nnSecond: \nnThird: ")
      (+ a b c)))
  (fset 'neovm--test-multi3
    (lambda (beg end str)
      (interactive "r\nsReplace with: ")
      (list beg end str)))
  ;; Mix of prompted and computed
  (fset 'neovm--test-multi4
    (lambda (p str)
      (interactive "p\nsInput: ")
      (list p str)))

  (unwind-protect
      (list
        ;; All commandp
        (commandp 'neovm--test-multi1)
        (commandp 'neovm--test-multi2)
        (commandp 'neovm--test-multi3)
        (commandp 'neovm--test-multi4)
        ;; interactive-form
        (interactive-form 'neovm--test-multi1)
        (interactive-form 'neovm--test-multi2)
        (interactive-form 'neovm--test-multi3)
        (interactive-form 'neovm--test-multi4)
        ;; funcall works
        (funcall 'neovm--test-multi1 "hello" 42)
        (funcall 'neovm--test-multi2 1 2 3)
        (funcall 'neovm--test-multi3 1 10 "replacement")
        (funcall 'neovm--test-multi4 4 "test"))
    (fmakunbound 'neovm--test-multi1)
    (fmakunbound 'neovm--test-multi2)
    (fmakunbound 'neovm--test-multi3)
    (fmakunbound 'neovm--test-multi4)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t (interactive \"sString: \\nnNumber: \") (interactive \"nFirst: \\nnSecond: \\nnThird: \") (interactive \"r\\nsReplace with: \") (interactive \"p\\nsInput: \") (\"hello\" 42) 6 (1 10 \"replacement\") (4 \"test\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive with list form (computed arguments)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_list_form_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; List form: arguments computed at call time
  (fset 'neovm--test-list-basic
    (lambda (a b) (interactive (list 10 20)) (+ a b)))
  ;; List form with runtime computation
  (fset 'neovm--test-list-computed
    (lambda (buf pt)
      (interactive (list (current-buffer) (point)))
      (list (bufferp buf) (integerp pt))))
  ;; List form with conditional logic
  (fset 'neovm--test-list-conditional
    (lambda (val)
      (interactive (list (if (> (point) 1) 'after-start 'at-start)))
      val))
  ;; Nested list form
  (fset 'neovm--test-list-nested
    (lambda (pair)
      (interactive (list (cons 'a 'b)))
      pair))

  (unwind-protect
      (list
        ;; All commandp
        (commandp 'neovm--test-list-basic)
        (commandp 'neovm--test-list-computed)
        (commandp 'neovm--test-list-conditional)
        (commandp 'neovm--test-list-nested)
        ;; interactive-form
        (interactive-form 'neovm--test-list-basic)
        (interactive-form 'neovm--test-list-computed)
        ;; funcall bypasses interactive spec
        (funcall 'neovm--test-list-basic 3 7)
        (funcall 'neovm--test-list-nested '(x . y)))
    (fmakunbound 'neovm--test-list-basic)
    (fmakunbound 'neovm--test-list-computed)
    (fmakunbound 'neovm--test-list-conditional)
    (fmakunbound 'neovm--test-list-nested)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t (interactive (list 10 20)) (interactive (list (current-buffer) (point))) 10 (x . y))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// prefix-arg interaction: current-prefix-arg, prefix-numeric-value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_prefix_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; prefix-numeric-value converts various prefix arg forms
  (prefix-numeric-value nil)          ;; no prefix -> 1
  (prefix-numeric-value 4)            ;; C-u -> 4
  (prefix-numeric-value 16)           ;; C-u C-u -> 16
  (prefix-numeric-value '-)           ;; M-- -> -1
  (prefix-numeric-value -3)           ;; M-3 M-- -> -3
  (prefix-numeric-value 7)            ;; M-7 -> 7
  (prefix-numeric-value '(4))         ;; (list 4) -> 4
  (prefix-numeric-value '(16))        ;; (list 16) -> 16
  ;; current-prefix-arg is nil when no prefix is active
  current-prefix-arg)
"#;
    let expect = expect_test::expect![[r#""OK (1 4 16 -1 -3 7 4 16 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Command registration with defun + interactive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_defun_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; defun with interactive creates a command
  (defun neovm--test-defun-cmd1 ()
    "A test command."
    (interactive)
    'cmd1-result)

  (defun neovm--test-defun-cmd2 (n)
    "A test command with prefix arg."
    (interactive "p")
    (* n n))

  (defun neovm--test-defun-cmd3 (start end)
    "A test command with region."
    (interactive "r")
    (- end start))

  ;; Non-interactive defun
  (defun neovm--test-defun-fn (x) (* x 2))

  (unwind-protect
      (list
        ;; commandp
        (commandp 'neovm--test-defun-cmd1)
        (commandp 'neovm--test-defun-cmd2)
        (commandp 'neovm--test-defun-cmd3)
        (commandp 'neovm--test-defun-fn)
        ;; interactive-form
        (interactive-form 'neovm--test-defun-cmd1)
        (interactive-form 'neovm--test-defun-cmd2)
        (interactive-form 'neovm--test-defun-cmd3)
        (interactive-form 'neovm--test-defun-fn)
        ;; funcall
        (funcall 'neovm--test-defun-cmd1)
        (funcall 'neovm--test-defun-cmd2 5)
        (funcall 'neovm--test-defun-cmd3 10 20)
        (funcall 'neovm--test-defun-fn 7))
    (fmakunbound 'neovm--test-defun-cmd1)
    (fmakunbound 'neovm--test-defun-cmd2)
    (fmakunbound 'neovm--test-defun-cmd3)
    (fmakunbound 'neovm--test-defun-fn)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil (interactive nil) (interactive \"p\") (interactive \"r\") nil cmd1-result 25 10 14)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comparing command-execute vs funcall behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_command_execute_vs_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--test-exec-log nil)

  (fset 'neovm--test-exec-cmd
    (lambda ()
      (interactive)
      (setq neovm--test-exec-log
            (cons 'executed neovm--test-exec-log))
      'result))

  (fset 'neovm--test-exec-with-arg
    (lambda (n)
      (interactive "p")
      (setq neovm--test-exec-log
            (cons (list 'with-arg n) neovm--test-exec-log))
      (* n 2)))

  (unwind-protect
      (progn
        (setq neovm--test-exec-log nil)
        ;; funcall: direct call, bypasses interactive spec
        (let ((r1 (funcall 'neovm--test-exec-cmd)))
          ;; funcall with explicit arg
          (let ((r2 (funcall 'neovm--test-exec-with-arg 5)))
            (list
              r1 r2
              (nreverse neovm--test-exec-log)
              ;; commandp checks
              (commandp 'neovm--test-exec-cmd)
              (commandp 'neovm--test-exec-with-arg)
              ;; interactive-form checks
              (interactive-form 'neovm--test-exec-cmd)
              (interactive-form 'neovm--test-exec-with-arg)))))
    (makunbound 'neovm--test-exec-log)
    (fmakunbound 'neovm--test-exec-cmd)
    (fmakunbound 'neovm--test-exec-with-arg)))
"#;
    let expect = expect_test::expect![[
        r#""OK (result 10 (executed (with-arg 5)) t t (interactive nil) (interactive \"p\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive form with defalias
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_defalias() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--test-defalias-orig
    (lambda (n) (interactive "p") (* n 3)))
  (defalias 'neovm--test-defalias-alias 'neovm--test-defalias-orig)

  (unwind-protect
      (list
        ;; Both are commands
        (commandp 'neovm--test-defalias-orig)
        (commandp 'neovm--test-defalias-alias)
        ;; interactive-form works through alias
        (interactive-form 'neovm--test-defalias-alias)
        ;; funcall works through alias
        (funcall 'neovm--test-defalias-alias 7)
        ;; symbol-function of alias
        (eq (symbol-function 'neovm--test-defalias-alias)
            'neovm--test-defalias-orig))
    (fmakunbound 'neovm--test-defalias-orig)
    (fmakunbound 'neovm--test-defalias-alias)))
"#;
    let expect = expect_test::expect![[r#""OK (t t (interactive \"p\") 21 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building a command framework with interactive dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_command_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--test-fw-registry (make-hash-table))

  (fset 'neovm--test-fw-register
    (lambda (name fn doc)
      (puthash name (list fn doc) neovm--test-fw-registry)))

  (fset 'neovm--test-fw-cmd-upcase
    (lambda (s) (interactive "sString: ") (upcase s)))
  (fset 'neovm--test-fw-cmd-repeat
    (lambda (s n)
      (interactive (list "hello" 3))
      (let ((result ""))
        (dotimes (_ n) (setq result (concat result s)))
        result)))
  (fset 'neovm--test-fw-cmd-count
    (lambda (lst) (interactive (list nil)) (length lst)))

  (funcall 'neovm--test-fw-register 'upcase 'neovm--test-fw-cmd-upcase "Upcase a string")
  (funcall 'neovm--test-fw-register 'repeat 'neovm--test-fw-cmd-repeat "Repeat a string")
  (funcall 'neovm--test-fw-register 'count 'neovm--test-fw-cmd-count "Count elements")

  (unwind-protect
      (let ((results nil))
        ;; Iterate registry, verify all are commands, execute them
        (maphash
         (lambda (name entry)
           (let ((fn (car entry))
                 (doc (cadr entry)))
             (setq results
                   (cons (list name
                               (commandp fn)
                               (not (null (interactive-form fn)))
                               doc)
                         results))))
         neovm--test-fw-registry)
        ;; Execute each command directly
        (list
          (sort results (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b)))))
          (funcall 'neovm--test-fw-cmd-upcase "hello")
          (funcall 'neovm--test-fw-cmd-repeat "ab" 4)
          (funcall 'neovm--test-fw-cmd-count '(1 2 3 4 5))))
    (makunbound 'neovm--test-fw-registry)
    (fmakunbound 'neovm--test-fw-register)
    (fmakunbound 'neovm--test-fw-cmd-upcase)
    (fmakunbound 'neovm--test-fw-cmd-repeat)
    (fmakunbound 'neovm--test-fw-cmd-count)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((count t t \"Count elements\") (repeat t t \"Repeat a string\") (upcase t t \"Upcase a string\")) \"HELLO\" \"abababab\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive with &optional and &rest parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_optional_rest_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Command with optional params
  (fset 'neovm--test-opt-cmd
    (lambda (a &optional b c)
      (interactive "p")
      (list a (or b 'default-b) (or c 'default-c))))

  ;; Command with rest params
  (fset 'neovm--test-rest-cmd
    (lambda (first &rest others)
      (interactive (list 'initial))
      (cons first others)))

  (unwind-protect
      (list
        ;; commandp
        (commandp 'neovm--test-opt-cmd)
        (commandp 'neovm--test-rest-cmd)
        ;; funcall with various arg counts
        (funcall 'neovm--test-opt-cmd 1)
        (funcall 'neovm--test-opt-cmd 1 2)
        (funcall 'neovm--test-opt-cmd 1 2 3)
        (funcall 'neovm--test-rest-cmd 'x)
        (funcall 'neovm--test-rest-cmd 'x 'y 'z)
        ;; interactive-form
        (interactive-form 'neovm--test-opt-cmd)
        (interactive-form 'neovm--test-rest-cmd))
    (fmakunbound 'neovm--test-opt-cmd)
    (fmakunbound 'neovm--test-rest-cmd)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t (1 default-b default-c) (1 2 default-c) (1 2 3) (x) (x y z) (interactive \"p\") (interactive (list 'initial)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive spec code "c" (character), "k" (key sequence), "K" (key seq)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_spec_char_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Spec code c: read a character
  (fset 'neovm--test-spec-c
    (lambda (ch) (interactive "cChar: ") ch))
  ;; Spec code k: read a key sequence
  (fset 'neovm--test-spec-k
    (lambda (key) (interactive "kKey: ") key))
  ;; Spec code K: read key sequence (no menu bar key)
  (fset 'neovm--test-spec-K
    (lambda (key) (interactive "KKey: ") key))
  ;; Spec code U: read with unicode (Emacs 28+)
  (fset 'neovm--test-spec-z
    (lambda (cs) (interactive "zCoding system: ") cs))
  (fset 'neovm--test-spec-Z
    (lambda (cs) (interactive "ZCoding system: ") cs))

  (unwind-protect
      (list
        (commandp 'neovm--test-spec-c)
        (commandp 'neovm--test-spec-k)
        (commandp 'neovm--test-spec-K)
        (commandp 'neovm--test-spec-z)
        (commandp 'neovm--test-spec-Z)
        ;; interactive-form retrieval
        (interactive-form 'neovm--test-spec-c)
        (interactive-form 'neovm--test-spec-k)
        (interactive-form 'neovm--test-spec-K)
        (interactive-form 'neovm--test-spec-z)
        (interactive-form 'neovm--test-spec-Z)
        ;; funcall works bypassing interactive
        (funcall 'neovm--test-spec-c ?x)
        (funcall 'neovm--test-spec-k [?\C-x ?\C-f]))
    (fmakunbound 'neovm--test-spec-c)
    (fmakunbound 'neovm--test-spec-k)
    (fmakunbound 'neovm--test-spec-K)
    (fmakunbound 'neovm--test-spec-z)
    (fmakunbound 'neovm--test-spec-Z)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t (interactive \"cChar: \") (interactive \"kKey: \") (interactive \"KKey: \") (interactive \"zCoding system: \") (interactive \"ZCoding system: \") 120 [24 6])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive with complex list spec using buffer state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_list_spec_buffer_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Command that computes its args from buffer state
  (fset 'neovm--test-bufstate-cmd
    (lambda (bufname pt pmax)
      (interactive (list (buffer-name) (point) (point-max)))
      (list bufname pt pmax)))

  ;; Command that reads multiple buffer properties
  (fset 'neovm--test-bufprops-cmd
    (lambda (name size modified)
      (interactive
       (list (buffer-name)
             (buffer-size)
             (buffer-modified-p)))
      (list name size modified)))

  (unwind-protect
      (with-temp-buffer
        (insert "hello world")
        (goto-char 6)
        (list
          (funcall 'neovm--test-bufstate-cmd
                   (buffer-name) (point) (point-max))
          (funcall 'neovm--test-bufprops-cmd
                   (buffer-name) (buffer-size) (buffer-modified-p))
          (commandp 'neovm--test-bufstate-cmd)
          (commandp 'neovm--test-bufprops-cmd)
          (interactive-form 'neovm--test-bufstate-cmd)
          (interactive-form 'neovm--test-bufprops-cmd)))
    (fmakunbound 'neovm--test-bufstate-cmd)
    (fmakunbound 'neovm--test-bufprops-cmd)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\" *temp*\" 6 12) (\" *temp*\" 11 t) t t (interactive (list (buffer-name) (point) (point-max))) (interactive (list (buffer-name) (buffer-size) (buffer-modified-p))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive spec with special characters: *, @, ^
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_spec_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; * at beginning: signal error if buffer is read-only
  (fset 'neovm--test-star-cmd
    (lambda () (interactive "*") 'modified-buffer))
  ;; ^ at beginning: handle shift-selection
  (fset 'neovm--test-caret-cmd
    (lambda () (interactive "^") 'shifted))
  ;; *p: read-only check + prefix arg
  (fset 'neovm--test-star-p-cmd
    (lambda (n) (interactive "*p") n))
  ;; Combine: *^p
  (fset 'neovm--test-star-caret-p
    (lambda (n) (interactive "*^p") n))

  (unwind-protect
      (list
        ;; All are commands
        (commandp 'neovm--test-star-cmd)
        (commandp 'neovm--test-caret-cmd)
        (commandp 'neovm--test-star-p-cmd)
        (commandp 'neovm--test-star-caret-p)
        ;; interactive-form
        (interactive-form 'neovm--test-star-cmd)
        (interactive-form 'neovm--test-caret-cmd)
        (interactive-form 'neovm--test-star-p-cmd)
        (interactive-form 'neovm--test-star-caret-p)
        ;; funcall works
        (funcall 'neovm--test-star-cmd)
        (funcall 'neovm--test-caret-cmd)
        (funcall 'neovm--test-star-p-cmd 42)
        (funcall 'neovm--test-star-caret-p 7))
    (fmakunbound 'neovm--test-star-cmd)
    (fmakunbound 'neovm--test-caret-cmd)
    (fmakunbound 'neovm--test-star-p-cmd)
    (fmakunbound 'neovm--test-star-caret-p)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t (interactive \"*\") (interactive \"^\") (interactive \"*p\") (interactive \"*^p\") modified-buffer shifted 42 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
