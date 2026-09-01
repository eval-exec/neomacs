//! Advanced oracle parity tests for interactive and command patterns.
//!
//! Tests interactive lambda with different interactive specs, commandp on
//! various function types, funcall/apply with interactive functions,
//! command dispatch systems, and key binding + command + execution pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interactive lambda with different interactive specs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_lambda_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Various interactive forms: nil spec, string spec, list spec
    let form = r#"(let ((cmd-no-args (lambda () (interactive) 'no-args))
                        (cmd-with-spec (lambda (x) (interactive "p") x))
                        (cmd-list-spec (lambda (a b)
                                         (interactive (list 1 2))
                                         (+ a b))))
                    (list
                      ;; All should be commandp
                      (commandp cmd-no-args)
                      (commandp cmd-with-spec)
                      (commandp cmd-list-spec)
                      ;; funcall still works normally
                      (funcall cmd-no-args)
                      (funcall cmd-with-spec 42)
                      (funcall cmd-list-spec 10 20)))"#;
    let expect = expect_test::expect![[r#""OK (t t t no-args 42 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// commandp on various function types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_commandp_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // commandp returns t for interactive functions, nil for non-interactive
    let form = "(list
                  ;; Non-interactive lambda
                  (commandp (lambda (x) x))
                  ;; Interactive lambda (no args)
                  (commandp (lambda () (interactive) t))
                  ;; Interactive lambda with spec
                  (commandp (lambda (n) (interactive \"p\") n))
                  ;; Symbols: built-in commands
                  (commandp 'forward-char)
                  (commandp 'goto-char)
                  ;; Symbols: non-command functions
                  (commandp 'car)
                  (commandp 'cons)
                  ;; Not functions at all
                  (commandp 42)
                  (commandp nil)
                  (commandp t)
                  (commandp \"string\")
                  ;; Quoted lambda (not a closure)
                  (commandp '(lambda () (interactive) t)))";
    let expect = expect_test::expect![[r#""OK (nil t t t t nil nil nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall/apply with interactive functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_apply_interactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interactive functions can still be called with funcall/apply normally
    let form = "(progn
  (fset 'neovm--test-icmd-add
    (lambda (a b)
      (interactive (list 0 0))
      (+ a b)))
  (fset 'neovm--test-icmd-greet
    (lambda (name)
      (interactive \"sName: \")
      (format \"Hello, %s!\" name)))
  (unwind-protect
      (list
        ;; funcall with explicit args
        (funcall 'neovm--test-icmd-add 3 7)
        (funcall 'neovm--test-icmd-greet \"World\")
        ;; apply
        (apply 'neovm--test-icmd-add '(10 20))
        (apply 'neovm--test-icmd-greet '(\"Emacs\"))
        ;; Nested funcall
        (funcall 'neovm--test-icmd-add
                 (funcall 'neovm--test-icmd-add 1 2)
                 (funcall 'neovm--test-icmd-add 3 4))
        ;; commandp check
        (commandp 'neovm--test-icmd-add)
        (commandp 'neovm--test-icmd-greet))
    (fmakunbound 'neovm--test-icmd-add)
    (fmakunbound 'neovm--test-icmd-greet)))";
    let expect =
        expect_test::expect![[r#""OK (10 \"Hello, World!\" 30 \"Hello, Emacs!\" 10 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interactive spec with list form and side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interactive_list_form_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interactive spec can be a list form that is evaluated
    let form = "(let ((counter 0))
                  (let ((cmd (lambda (n)
                               (interactive (list (setq counter (1+ counter))))
                               (* n n))))
                    (list
                      ;; funcall bypasses interactive spec
                      (funcall cmd 5)
                      (funcall cmd 3)
                      ;; counter should not have changed from funcall
                      counter
                      ;; commandp still true
                      (commandp cmd)
                      ;; But we can call the interactive spec manually
                      (funcall cmd 10))))";
    let expect = expect_test::expect![[r#""OK (25 9 0 t 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: command dispatch system with alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_command_dispatch_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a simple command dispatch table and execute commands
    let form = "(progn
  (fset 'neovm--test-dispatch-add
    (lambda (a b) (interactive (list 0 0)) (+ a b)))
  (fset 'neovm--test-dispatch-mul
    (lambda (a b) (interactive (list 1 1)) (* a b)))
  (fset 'neovm--test-dispatch-neg
    (lambda (a) (interactive \"p\") (- a)))
  (fset 'neovm--test-dispatch-sq
    (lambda (a) (interactive \"p\") (* a a)))
  (unwind-protect
      (let ((dispatch-table
              '((add . neovm--test-dispatch-add)
                (mul . neovm--test-dispatch-mul)
                (neg . neovm--test-dispatch-neg)
                (sq  . neovm--test-dispatch-sq)))
            (results nil))
        ;; Execute a sequence of commands through dispatch
        (let ((commands '((add 3 4)
                          (mul 5 6)
                          (neg 7)
                          (sq 8)
                          (add 100 200))))
          (dolist (cmd-call commands)
            (let* ((cmd-name (car cmd-call))
                   (cmd-args (cdr cmd-call))
                   (cmd-fn (cdr (assq cmd-name dispatch-table))))
              (when cmd-fn
                (let ((result (apply cmd-fn cmd-args)))
                  (setq results (cons (cons cmd-name result) results)))))))
        ;; Verify all dispatched commands are interactive
        (let ((all-interactive t))
          (dolist (entry dispatch-table)
            (unless (commandp (cdr entry))
              (setq all-interactive nil)))
          (list (nreverse results) all-interactive)))
    (fmakunbound 'neovm--test-dispatch-add)
    (fmakunbound 'neovm--test-dispatch-mul)
    (fmakunbound 'neovm--test-dispatch-neg)
    (fmakunbound 'neovm--test-dispatch-sq)))";
    let expect = expect_test::expect![[
        r#""OK (((add . 7) (mul . 30) (neg . -7) (sq . 64) (add . 300)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: key binding + command + execution pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_keybinding_command_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a keymap, bind commands, look up keys, and execute the bound commands
    let form = r#"(progn
  (fset 'neovm--test-kb-upcase
    (lambda (s) (interactive "s") (upcase s)))
  (fset 'neovm--test-kb-reverse
    (lambda (s) (interactive "s") (concat (nreverse (append s nil)))))
  (fset 'neovm--test-kb-repeat
    (lambda (s n) (interactive (list "" 1))
      (let ((result ""))
        (dotimes (_ n) (setq result (concat result s)))
        result)))
  (unwind-protect
      (let ((my-map (make-sparse-keymap)))
        ;; Bind commands to keys
        (define-key my-map [?u] 'neovm--test-kb-upcase)
        (define-key my-map [?r] 'neovm--test-kb-reverse)
        (define-key my-map [?p] 'neovm--test-kb-repeat)
        ;; Look up and execute each binding
        (let ((results nil))
          (dolist (key-and-args '(([?u] "hello")
                                  ([?r] "abcdef")
                                  ([?p] "ha" 3)))
            (let* ((key (car key-and-args))
                   (args (cdr key-and-args))
                   (cmd (lookup-key my-map key)))
              (when (and cmd (commandp cmd))
                (setq results
                      (cons (list (car key-and-args)
                                  cmd
                                  (apply cmd args))
                            results)))))
          ;; Verify unbound key returns nil
          (let ((unbound (lookup-key my-map [?z])))
            (list (nreverse results)
                  (null unbound)
                  ;; Verify the map structure
                  (keymapp my-map)))))
    (fmakunbound 'neovm--test-kb-upcase)
    (fmakunbound 'neovm--test-kb-reverse)
    (fmakunbound 'neovm--test-kb-repeat))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: command registration and introspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_command_registration_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Register commands with metadata (via symbol properties),
    // then introspect the registry
    let form = "(progn
  (fset 'neovm--test-reg-cmd1
    (lambda () (interactive) 'cmd1-result))
  (put 'neovm--test-reg-cmd1 'doc \"Command one\")
  (put 'neovm--test-reg-cmd1 'category 'editing)

  (fset 'neovm--test-reg-cmd2
    (lambda (n) (interactive \"p\") (* n 2)))
  (put 'neovm--test-reg-cmd2 'doc \"Command two\")
  (put 'neovm--test-reg-cmd2 'category 'navigation)

  (fset 'neovm--test-reg-cmd3
    (lambda (a b) (interactive (list 0 0)) (list a b)))
  (put 'neovm--test-reg-cmd3 'doc \"Command three\")
  (put 'neovm--test-reg-cmd3 'category 'editing)

  (unwind-protect
      (let ((registry '(neovm--test-reg-cmd1
                         neovm--test-reg-cmd2
                         neovm--test-reg-cmd3)))
        ;; Introspect: collect interactive commands with their categories
        (let ((editing-cmds nil)
              (nav-cmds nil)
              (all-docs nil))
          (dolist (sym registry)
            (when (commandp sym)
              (let ((cat (get sym 'category))
                    (doc (get sym 'doc)))
                (setq all-docs (cons (cons sym doc) all-docs))
                (cond
                  ((eq cat 'editing)
                   (setq editing-cmds (cons sym editing-cmds)))
                  ((eq cat 'navigation)
                   (setq nav-cmds (cons sym nav-cmds)))))))
          ;; Execute all editing commands
          (let ((edit-results nil))
            (dolist (cmd (nreverse editing-cmds))
              (setq edit-results
                    (cons (cond
                            ((eq cmd 'neovm--test-reg-cmd1)
                             (funcall cmd))
                            ((eq cmd 'neovm--test-reg-cmd3)
                             (funcall cmd 10 20)))
                          edit-results)))
            (list (nreverse editing-cmds)
                  (nreverse nav-cmds)
                  (nreverse all-docs)
                  (nreverse edit-results)))))
    (fmakunbound 'neovm--test-reg-cmd1)
    (fmakunbound 'neovm--test-reg-cmd2)
    (fmakunbound 'neovm--test-reg-cmd3)
    (put 'neovm--test-reg-cmd1 'doc nil)
    (put 'neovm--test-reg-cmd1 'category nil)
    (put 'neovm--test-reg-cmd2 'doc nil)
    (put 'neovm--test-reg-cmd2 'category nil)
    (put 'neovm--test-reg-cmd3 'doc nil)
    (put 'neovm--test-reg-cmd3 'category nil)))";
    let expect = expect_test::expect![[
        r#""OK ((neovm--test-reg-cmd3) (neovm--test-reg-cmd2) ((neovm--test-reg-cmd1 . \"Command one\") (neovm--test-reg-cmd2 . \"Command two\") (neovm--test-reg-cmd3 . \"Command three\")) (cmd1-result (10 20)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: mode-like setup with hooks and local variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_mode_setup_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a simple mode setup: define commands, set up keymap,
    // run mode hooks, track activation state
    let form = r#"(progn
  (fset 'neovm--test-mode-action1
    (lambda () (interactive) 'action1-done))
  (fset 'neovm--test-mode-action2
    (lambda (arg) (interactive "p") (list 'action2 arg)))
  (unwind-protect
      (let ((mode-map (make-sparse-keymap))
            (mode-hooks nil)
            (mode-active nil)
            (hook-log nil))
        ;; Set up keymap
        (define-key mode-map [?a] 'neovm--test-mode-action1)
        (define-key mode-map [?b] 'neovm--test-mode-action2)
        ;; Set up hooks
        (setq mode-hooks
              (list (lambda () (setq hook-log (cons 'hook1 hook-log)))
                    (lambda () (setq hook-log (cons 'hook2 hook-log)))
                    (lambda () (setq hook-log (cons 'hook3 hook-log)))))
        ;; "Activate" mode: run hooks
        (dolist (hook mode-hooks)
          (funcall hook))
        (setq mode-active t)
        ;; Execute commands through the keymap
        (let ((cmd-a (lookup-key mode-map [?a]))
              (cmd-b (lookup-key mode-map [?b])))
          (list
            ;; Mode state
            mode-active
            (nreverse hook-log)
            ;; Command execution
            (and cmd-a (commandp cmd-a) (funcall cmd-a))
            (and cmd-b (commandp cmd-b) (funcall cmd-b 99))
            ;; Keymap structure
            (keymapp mode-map)
            (null (lookup-key mode-map [?z])))))
    (fmakunbound 'neovm--test-mode-action1)
    (fmakunbound 'neovm--test-mode-action2))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
