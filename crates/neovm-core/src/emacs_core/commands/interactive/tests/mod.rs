use super::*;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::keymap::make_list_keymap;
use crate::emacs_core::load::{apply_runtime_startup_state, create_bootstrap_evaluator_cached};
use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::emacs_core::{Context, format_eval_result};
use crate::test_utils::{eval_with_ldefs_boot_autoloads, runtime_startup_eval_all};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use strum::IntoEnumIterator;

/// Create evaluator with minimal Elisp shims for interactive testing.
fn eval_with_interactive_shims() -> Context {
    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    let shims = r#"
(defalias 'set-mark #'(lambda (pos)
  (if pos (set-marker (mark-marker) pos (current-buffer)))
  nil))
(defalias 'mark #'(lambda (&optional force)
  (let ((m (mark-marker)))
    (if (and m (marker-position m)) (marker-position m)))))
(defalias 'macrop #'(lambda (object)
  (and (consp object) (eq (car object) 'macro))))
(defalias 'special-form-p #'(lambda (object)
  nil))
(defalias 'other-window #'(lambda (count &optional all-frames)
  nil))
"#;
    let _ = ev.eval_str(shims);
    ev
}

fn eval_all(src: &str) -> Vec<String> {
    let mut ev = eval_with_interactive_shims();
    ev.eval_str_each(src)
        .iter()
        .map(format_eval_result)
        .collect()
}

fn eval_one(src: &str) -> String {
    eval_all(src).into_iter().next().expect("at least one form")
}

fn plan_call_interactively_for_test(
    eval: &mut Context,
    args: &[Value],
) -> Result<CallInteractivelyPlan, Flow> {
    validate_call_interactively_args(args)?;
    let command_identity = CallInteractivelyCommandIdentity::capture(eval);
    let interactive_form =
        crate::emacs_core::builtins::symbols::builtin_interactive_form(eval, vec![args[0]])?;
    plan_call_interactively_after_interactive_form_in_state(
        &eval.obarray,
        eval.read_command_keys(),
        args,
        interactive_form,
        command_identity,
    )
}

#[test]
fn parse_interactive_code_entries_preserves_raw_unibyte_prompt_bytes() {
    let spec = crate::heap_types::LispString::from_unibyte(vec![b'a', 0xFF, b':', b' ']);
    let parsed = parse_interactive_code_entries(&spec);

    assert!(parsed.prefix_flags.is_empty());
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].0, 'a');
    assert_eq!(parsed.entries[0].1.as_bytes(), &[0xFF, b':', b' ']);
    assert!(!parsed.entries[0].1.is_multibyte());
}

#[test]
fn interactive_control_letter_domain_matches_gnu_callint() {
    let expected = [
        'a', 'b', 'B', 'c', 'C', 'd', 'D', 'e', 'f', 'F', 'G', 'i', 'k', 'K', 'U', 'm', 'M', 'N',
        'n', 'P', 'p', 'R', 'r', 's', 'S', 'v', 'x', 'X', 'Z', 'z',
    ];
    let expected_set: HashSet<char> = expected.into_iter().collect();
    let actual_set: HashSet<char> = InteractiveControlLetter::iter()
        .map(InteractiveControlLetter::letter)
        .collect();

    assert_eq!(actual_set, expected_set);
    for letter in expected {
        assert_eq!(
            InteractiveControlLetter::from_char(letter).map(InteractiveControlLetter::letter),
            Some(letter)
        );
    }
    assert_eq!(InteractiveControlLetter::from_char('+'), None);
    assert_eq!(InteractiveControlLetter::from_char('q'), None);
}

fn eval_all_with(ev: &mut Context, src: &str) -> Vec<String> {
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    let roots = ev.save_specpdl_roots();
    for form in &forms {
        ev.push_specpdl_root(*form);
    }
    let result = forms
        .iter()
        .map(|form| {
            let result = ev.eval_form(*form);
            format_eval_result(&result)
        })
        .collect();
    ev.restore_specpdl_roots(roots);
    result
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

fn bootstrap_eval_one(src: &str) -> String {
    bootstrap_eval_all(src)
        .into_iter()
        .next()
        .expect("at least one form")
}

fn eval_first_form_after_marker(eval: &mut Context, source: &str, marker: &str) {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing GNU subr.el marker: {marker}"));
    let (form, _) = crate::emacs_core::value_reader::read_one(&source[start..], 0, &test_ob())
        .unwrap_or_else(|err| panic!("parse GNU subr.el from {marker} failed: {:?}", err))
        .unwrap_or_else(|| panic!("no GNU subr.el form found after marker: {marker}"));
    eval.eval_form(form)
        .unwrap_or_else(|err| panic!("evaluate GNU subr.el form {marker} failed: {:?}", err));
}

/// Install minimal `defun`/`defmacro`/`when`/`unless` shims so a bare
/// evaluator can evaluate forms extracted from GNU `.el` source files.
/// Also installs a minimal `with-temp-buffer` that uses
/// get-buffer-create with a fixed unique name (sufficient for tests
/// that don't nest with-temp-buffer).
fn install_bare_elisp_shims(ev: &mut Context) {
    let shims = r#"
(defalias 'defun (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name) (cons 'function (list (cons 'lambda (cons arglist body))))))))
(defalias 'defmacro (cons 'macro #'(lambda (name arglist &rest body)
  (list 'defalias (list 'quote name)
        (list 'cons ''macro (cons 'function (list (cons 'lambda (cons arglist body)))))))))
(defalias 'when (cons 'macro #'(lambda (cond &rest body)
  (list 'if cond (cons 'progn body)))))
(defalias 'unless (cons 'macro #'(lambda (cond &rest body)
  (cons 'if (cons cond (cons nil body))))))
(defalias 'with-temp-buffer (cons 'macro #'(lambda (&rest body)
  (list 'let
        (list (list 'vm-temp-buf
                    (list 'get-buffer-create " *vm-shim-temp*" t)))
        (list 'unwind-protect
              (list 'save-current-buffer
                    (list 'set-buffer 'vm-temp-buf)
                    (list 'erase-buffer)
                    (cons 'progn body))
              (list 'kill-buffer 'vm-temp-buf))))))
"#;
    ev.eval_str(shims).expect("install bare elisp shims");
}

fn gnu_subr_keymap_eval_all(src: &str) -> Vec<String> {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let subr_path = project_root.join("lisp/subr.el");
    let subr_source = fs::read_to_string(&subr_path).expect("read GNU subr.el");

    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    ev.set_lexical_binding(true);
    for marker in [
        "(defun global-key-binding",
        "(defvar esc-map",
        "(fset 'ESC-prefix esc-map)",
        "(defvar ctl-x-4-map",
        "(defalias 'ctl-x-4-prefix ctl-x-4-map)",
        "(defvar ctl-x-5-map",
        "(defalias 'ctl-x-5-prefix ctl-x-5-map)",
        "(defvar tab-prefix-map",
        "(defvar ctl-x-map",
        "(fset 'Control-X-prefix ctl-x-map)",
        "(defvar global-map",
        "(use-global-map global-map)",
    ] {
        eval_first_form_after_marker(&mut ev, &subr_source, marker);
    }
    eval_all_with(&mut ev, src)
}

fn gnu_simple_command_execute_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");
    let subr_path = project_root.join("lisp/subr.el");
    let subr_source = fs::read_to_string(&subr_path).expect("read GNU subr.el");

    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    ev.eval_str(
        r#"
        (defalias 'when-let* (cons 'macro #'(lambda (bindings &rest body)
          (let ((binding (car bindings)))
            (if (consp binding)
                (list 'let
                      (list (list (car binding) (car (cdr binding))))
                      (list 'if (car binding) (cons 'progn body)))
              (cons 'progn body))))))
        (defalias 'autoloadp #'(lambda (object) (eq 'autoload (car-safe object))))
        (fset 'prefix-command-update (lambda () nil))
        (fset 'add-to-history (lambda (&rest _args) nil))
        (fset 'macroexp--obsolete-warning (lambda (&rest _args) ""))
        (fset 'help--key-description-fontified (lambda (&rest _args) ""))
        (fset 'where-is-internal (lambda (&rest _args) nil))
        (fset 'mouse-drag-region (lambda (&rest _args) nil))
        (fset 'mouse-set-point (lambda (&rest _args) nil))
        "#,
    )
    .expect("eval command-execute test stubs");
    eval_first_form_after_marker(&mut ev, &subr_source, "(defun error (string &rest args)");
    eval_first_form_after_marker(
        &mut ev,
        &simple_source,
        "(defun command-execute (cmd &optional record-flag keys special)",
    );
    eval_first_form_after_marker(
        &mut ev,
        &simple_source,
        "(defun command-execute--query (command)",
    );
    // Load set-mark and its dependencies (needed by interactive region commands).
    // set-mark calls activate-mark, which calls region-active-p and mark.
    eval_first_form_after_marker(&mut ev, &simple_source, "(defun mark (&optional force)");
    eval_first_form_after_marker(&mut ev, &simple_source, "(defun activate-mark");
    eval_first_form_after_marker(&mut ev, &simple_source, "(defun set-mark (pos)");
    crate::test_utils::load_gnu_undo_auto_runtime(&mut ev);
    // `global-set-key' is Lisp -- lisp/subr.el:1545, DIVERGENCES.md 152 -- so
    // this prelude supplies it the way it supplies `command-execute': by
    // evaluating GNU's own `defun'.
    eval_first_form_after_marker(&mut ev, &subr_source, "(defun global-set-key (key command)");
    // Likewise `read-number' (lisp/subr.el:3725): the "n" and "N" code letters
    // reach it through the function cell, as GNU does at src/callint.c:645.
    eval_first_form_after_marker(
        &mut ev,
        &subr_source,
        "(defun read-number (prompt &optional default hist)",
    );
    ev.eval_str(
        r#"
        (use-global-map (make-sparse-keymap))
        (global-set-key [down-mouse-1] #'mouse-drag-region)
        (global-set-key [mouse-1] #'mouse-set-point)
        "#,
    )
    .expect("select command-execute test global map");
    ev
}

fn gnu_simple_command_execute_eval_all(src: &str) -> Vec<String> {
    // Use bootstrap evaluator — all Elisp functions (set-mark,
    // activate-mark, region-active-p, command-execute, etc.) are loaded.
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    eval_all_with(&mut ev, src)
}

fn gnu_simple_execute_extended_command_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");

    let mut ev = gnu_simple_command_execute_eval();
    ev.eval_str(
        r#"
        (setq suggest-key-bindings nil)
        (setq extended-command-suggest-shorter nil)
        (setq execute-extended-command--binding-timer nil)
        (setq executing-kbd-macro nil)
        (fset 'read-extended-command
              (lambda (&rest _args)
                (signal 'end-of-file '("Error reading from stdin"))))
        "#,
    )
    .expect("eval setup");
    eval_first_form_after_marker(
        &mut ev,
        &simple_source,
        "(defun execute-extended-command (prefixarg &optional command-name typed)",
    );
    ev
}

fn gnu_files_command_eval() -> Context {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let files_path = project_root.join("lisp/files.el");
    let files_source = fs::read_to_string(&files_path).expect("read GNU files.el");

    let mut ev = gnu_simple_command_execute_eval();
    for marker in [
        "(defun find-file-read-args (prompt mustmatch)",
        "(defun find-file (filename &optional wildcards)",
        "(defun save-buffer (&optional arg)",
    ] {
        eval_first_form_after_marker(&mut ev, &files_source, marker);
    }
    ev
}

fn gnu_files_command_eval_all(src: &str) -> Vec<String> {
    let mut ev = gnu_files_command_eval();
    eval_all_with(&mut ev, src)
}

fn gnu_simple_execute_extended_command_eval_all(src: &str) -> Vec<String> {
    let mut ev = gnu_simple_execute_extended_command_eval();
    eval_all_with(&mut ev, src)
}

fn load_gnu_eval_expression_into(ev: &mut Context) {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");

    ev.eval_str(
        r#"
        (defalias 'read--expression #'(lambda (&rest _args)
          (signal 'end-of-file '("Error reading from stdin"))))
        (defalias 'eval-expression-get-print-arguments #'(lambda (&rest _args) nil))
        "#,
    )
    .expect("eval setup");
    eval_first_form_after_marker(
        ev,
        &simple_source,
        "(defun eval-expression (exp &optional insert-value no-truncate char-print-limit)",
    );
}

fn gnu_simple_eval_expression_eval() -> Context {
    let mut ev = Context::new();
    install_bare_elisp_shims(&mut ev);
    load_gnu_eval_expression_into(&mut ev);
    ev
}

fn read_first_object(ev: &mut Context, src: &str) -> Value {
    let result = crate::emacs_core::reader::builtin_read_from_string(ev, vec![Value::string(src)])
        .unwrap_or_else(|err| panic!("read-from-string failed for {src:?}: {err:?}"));
    if !result.is_cons() {
        panic!("expected cons from read-from-string, got {result:?}");
    };
    result.cons_car()
}

fn gnu_simple_command_execute_with_eval_expression_eval() -> Context {
    let mut ev = gnu_simple_command_execute_eval();
    load_gnu_eval_expression_into(&mut ev);
    ev
}

fn gnu_simple_universal_argument_eval_all(src: &str) -> Vec<String> {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");

    let mut ev = gnu_simple_command_execute_eval();
    ev.eval_str(
        r#"
        (fset 'prefix-command-preserve-state (lambda () nil))
        (fset 'universal-argument--mode (lambda () nil))
        "#,
    )
    .expect("eval setup");
    eval_first_form_after_marker(&mut ev, &simple_source, "(defun universal-argument ()");
    eval_all_with(&mut ev, src)
}

fn gnu_simple_quoted_insert_eval_all(src: &str) -> Vec<String> {
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source = fs::read_to_string(&simple_path).expect("read GNU simple.el");

    let mut ev = gnu_simple_command_execute_eval();
    ev.eval_str(
        r#"
        (defun cadr (x) (car (cdr x)))
        (defmacro with-no-warnings (&rest body) (cons 'progn body))
        (setq overwrite-mode nil)
        (fset 'read-quoted-char
              (lambda (&rest _args)
                (signal 'end-of-file '("Error reading from stdin"))))
        (fset 'read-char
              (lambda (&rest _args)
                (signal 'end-of-file '("Error reading from stdin"))))
        "#,
    )
    .expect("eval setup");
    eval_first_form_after_marker(&mut ev, &simple_source, "(defun quoted-insert (arg)");
    eval_all_with(&mut ev, src)
}

// -------------------------------------------------------------------
// InteractiveSpec
// -------------------------------------------------------------------

#[test]
fn interactive_spec_no_args() {
    crate::test_utils::init_test_tracing();
    let spec = InteractiveSpec::no_args();
    assert_eq!(spec.string_code_runtime_owned(), Some(String::new()));
}

#[test]
fn interactive_spec_with_code() {
    crate::test_utils::init_test_tracing();
    let spec = InteractiveSpec::new("p");
    assert_eq!(spec.string_code_runtime_owned(), Some("p".to_string()));
}

#[test]
fn interactive_spec_with_prompt() {
    crate::test_utils::init_test_tracing();
    let spec = InteractiveSpec::new("sEnter name: ");
    assert_eq!(
        spec.string_code_runtime_owned(),
        Some("sEnter name: ".to_string())
    );
}

// -------------------------------------------------------------------
// InteractiveRegistry
// -------------------------------------------------------------------

#[test]
fn registry_register_and_query() {
    crate::test_utils::init_test_tracing();
    let mut reg = InteractiveRegistry::new();
    let forward_char = crate::emacs_core::intern::intern("forward-char");
    reg.register_interactive(forward_char, InteractiveSpec::new("p"));
    assert!(reg.is_interactive(forward_char));
    assert!(!reg.is_interactive(crate::emacs_core::intern::intern("nonexistent")));
}

#[test]
fn registry_get_spec() {
    crate::test_utils::init_test_tracing();
    let mut reg = InteractiveRegistry::new();
    let find_file = crate::emacs_core::intern::intern("find-file");
    reg.register_interactive(find_file, InteractiveSpec::new("FFind file: "));
    let spec = reg.get_spec(find_file).unwrap();
    assert_eq!(
        spec.string_code_runtime_owned(),
        Some("FFind file: ".to_string())
    );
}

#[test]
fn registry_interactive_form_preserves_lisp_form_specs() {
    crate::test_utils::init_test_tracing();
    let mut reg = InteractiveRegistry::new();
    let sym = crate::emacs_core::intern::intern("registry-form-command");
    let spec_form = Value::list(vec![Value::symbol("list"), Value::fixnum(7)]);
    reg.register_interactive(sym, InteractiveSpec::from_value(spec_form));

    let form = registry_interactive_form(&reg, sym).expect("interactive form");
    let items = crate::emacs_core::value::list_to_vec(&form).expect("interactive form list");
    assert_eq!(items[0], Value::symbol("interactive"));
    assert_eq!(items[1], spec_form);
}

#[test]
fn call_interactively_evaluates_registry_form_specs() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_interactive_shims();
    ev.eval_str(r#"(fset 'registry-form-command (lambda (x) x))"#)
        .expect("install registry-form-command");

    let sym = crate::emacs_core::intern::intern("registry-form-command");
    ev.interactive.register_interactive(
        sym,
        InteractiveSpec::from_value(Value::list(vec![Value::symbol("list"), Value::fixnum(7)])),
    );

    let result = builtin_call_interactively(&mut ev, vec![Value::symbol("registry-form-command")])
        .expect("call-interactively should evaluate registry form spec");
    assert_eq!(result, Value::fixnum(7));
}

#[test]
fn call_interactively_restores_command_identity_after_form_spec_evaluation() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq this-command 'outer-this
                       this-original-command 'outer-original
                       real-this-command 'outer-real
                       last-command 'outer-last)
                 (defun neovm-ci-command-state (argument)
                   (interactive
                    (progn
                      (setq this-command 'inner-this
                            this-original-command 'inner-original
                            real-this-command 'inner-real
                            last-command 'inner-last)
                      (list 7)))
                   (list argument
                         this-command
                         this-original-command
                         real-this-command
                         last-command))
                 (list (call-interactively 'neovm-ci-command-state)
                       this-command
                       this-original-command
                       real-this-command
                       last-command))"#
        ),
        "OK ((7 outer-this outer-original outer-real outer-last) outer-this outer-original outer-real outer-last)"
    );
}

#[test]
fn call_interactively_honors_function_cell_mutation_during_interactive_spec_eval() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (fset 'neovm-ci-mutate
                       (lambda ()
                         (interactive
                          (progn
                            (fset 'neovm-ci-mutate (lambda () 42))
                            (list)))
                         7))
                 (unwind-protect
                     (call-interactively 'neovm-ci-mutate)
                   (fmakunbound 'neovm-ci-mutate)))"#
        ),
        "OK 42"
    );
}

#[test]
fn call_interactively_form_spec_uses_interpreted_closure_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(eval
                 '(let ((command
                         (let ((captured 37))
                           (lambda (value)
                             (interactive (list captured))
                             value))))
                    (call-interactively command))
                 t)"#
        ),
        "OK 37"
    );
}

#[test]
fn call_interactively_quoted_lambda_form_spec_uses_dynamic_environment() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(eval
                 '(let ((ambient-only 99))
                    (call-interactively
                     '(lambda (value)
                        (interactive
                         (list
                          (condition-case nil
                              ambient-only
                            (void-variable 'dynamic))))
                        value)))
                 t)"#
        ),
        "OK dynamic"
    );
}

#[test]
fn defalias_replaces_stale_noarg_registry_spec_with_live_interactive_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_interactive_shims();
    let sym = crate::emacs_core::intern::intern("vm-stale-interactive-form");
    ev.interactive
        .register_interactive(sym, InteractiveSpec::no_args());

    ev.eval_str(
        r#"(defalias 'vm-stale-interactive-form
                      (lambda (x)
                        (interactive (list 7))
                        x))"#,
    )
    .expect("install live interactive form");

    let result =
        builtin_call_interactively(&mut ev, vec![Value::symbol("vm-stale-interactive-form")])
            .expect("call-interactively should use the live interactive form");
    assert_eq!(result, Value::fixnum(7));
}

#[test]
fn registry_interactive_call_stack() {
    crate::test_utils::init_test_tracing();
    let mut reg = InteractiveRegistry::new();
    assert!(!reg.is_called_interactively());

    reg.push_interactive_call(true);
    assert!(reg.is_called_interactively());

    reg.push_interactive_call(false);
    assert!(!reg.is_called_interactively());

    reg.pop_interactive_call();
    assert!(reg.is_called_interactively());

    reg.pop_interactive_call();
    assert!(!reg.is_called_interactively());
}

#[test]
fn registry_default() {
    crate::test_utils::init_test_tracing();
    let reg = InteractiveRegistry::default();
    assert!(!reg.is_called_interactively());
}

// -------------------------------------------------------------------
// GNU mode-definition macro ownership
// -------------------------------------------------------------------

#[test]
fn mode_definition_macros_start_as_gnu_autoloads() {
    crate::test_utils::init_test_tracing();
    assert!(!crate::emacs_core::subr_info::is_special_form(
        "define-minor-mode"
    ));
    assert!(!crate::emacs_core::subr_info::is_special_form(
        "define-derived-mode"
    ));
    assert!(!crate::emacs_core::subr_info::is_special_form(
        "define-generic-mode"
    ));

    let mut ev = eval_with_ldefs_boot_autoloads(&[
        "define-minor-mode",
        "define-derived-mode",
        "define-generic-mode",
    ]);
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (and (consp (symbol-function 'define-minor-mode))
                  (eq (car (symbol-function 'define-minor-mode)) 'autoload)
                  (eq (get 'define-minor-mode 'autoload-macro) 'expand))
             (and (consp (symbol-function 'define-derived-mode))
                  (eq (car (symbol-function 'define-derived-mode)) 'autoload)
                  (eq (get 'define-derived-mode 'autoload-macro) 'expand))
             (and (consp (symbol-function 'define-generic-mode))
                  (eq (car (symbol-function 'define-generic-mode)) 'autoload)
                  (eq (get 'define-generic-mode 'autoload-macro) 'expand)))"#,
    );
    assert_eq!(results[0], "OK (t t t)");
}

// -------------------------------------------------------------------
// commandp (interactive-aware version)
// -------------------------------------------------------------------

#[test]
fn commandp_borrows_subr_arguments_like_gnu() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let entry = crate::emacs_core::eval::lookup_global_subr_entry(intern("commandp"))
        .expect("commandp should be registered as a builtin subr");

    assert!(
        matches!(
            entry.function,
            Some(crate::tagged::header::SubrFn::ManySlice(_))
        ),
        "GNU Fcommandp receives Lisp_Object arguments directly; Neomacs must not allocate an owned Vec for each command candidate"
    );
}

#[test]
fn commandp_non_interactive() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    eval_all_with(&mut ev, r#"(defalias 'my-plain-fn #'(lambda () 42))"#);
    let result = builtin_commandp_interactive(&mut ev, &[Value::symbol("my-plain-fn")]);
    assert!(result.unwrap().is_nil());
}

/// GNU `indirect_function` (`src/eval.c:Fcommandp`) reads `SYMBOL_FUNC` once
/// per symbol alias hop. A function-cell lookup is one logical operation in
/// Neomacs as well; splitting its unbound and value states into independent
/// `Obarray::slot` probes multiplies the dominant M-x completion loop.
#[test]
fn commandp_reads_each_function_cell_once() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let command = intern("forward-char");

    ev.obarray.reset_symbol_slot_read_count();
    assert_eq!(
        classify_command_designator_in_state(&ev.obarray, &Value::from_sym_id(command), false),
        CommandpClassification::Interactive
    );
    assert_eq!(
        ev.obarray.symbol_slot_read_count(),
        1,
        "one symbol alias hop must take one typed function-cell snapshot"
    );

    let alias = intern("neo-commandp-function-cell-snapshot-alias");
    ev.obarray
        .set_symbol_function_id(alias, Value::from_sym_id(command));
    ev.obarray.reset_symbol_slot_read_count();
    assert_eq!(
        classify_command_designator_in_state(&ev.obarray, &Value::from_sym_id(alias), false),
        CommandpClassification::Interactive
    );
    assert_eq!(
        ev.obarray.symbol_slot_read_count(),
        2,
        "a two-symbol alias chain must take exactly two typed snapshots"
    );
}

/// GNU `Fcommandp` reads `XSUBR (fun)->intspec.string` directly from the
/// primitive object.  Re-entering Neomacs's global subr registry for every
/// primitive candidate turns the M-x obarray scan into thousands of TLS
/// `RefCell` borrows.
#[test]
fn commandp_reads_interactivity_from_the_subr_object() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    let function = ev
        .obarray
        .symbol_function_id(intern("forward-char"))
        .expect("forward-char should have a primitive function cell");
    assert_eq!(function.veclike_type(), Some(VecLikeType::Subr));

    crate::emacs_core::eval::reset_global_subr_lookup_count();
    assert_eq!(
        classify_command_object_in_state(&function, false),
        CommandpClassification::Interactive
    );
    assert_eq!(
        crate::emacs_core::eval::global_subr_lookup_count(),
        0,
        "primitive interactivity must be an intrinsic SubrObj field"
    );
}

#[test]
fn subr_registration_refreshes_an_existing_objects_interactivity() {
    crate::test_utils::init_test_tracing();
    let _ev = Context::new();
    let symbol = intern("neo-commandp-late-interactive-subr");
    let function = Value::subr_from_sym_id(symbol);
    assert_eq!(
        function.subr_interactivity(),
        Some(crate::tagged::header::SubrInteractivity::NonInteractive)
    );

    let mut entry = crate::emacs_core::eval::lookup_global_subr_entry(intern("forward-char"))
        .expect("forward-char should have registered primitive metadata");
    entry.name_id = crate::emacs_core::intern::symbol_name_id(symbol);
    crate::emacs_core::eval::register_global_subr_entry(symbol, entry);

    assert_eq!(
        function.subr_interactivity(),
        Some(crate::tagged::header::SubrInteractivity::Interactive),
        "native subr registration must refresh an object allocated before its entry"
    );
}

#[test]
fn commandp_uses_closure_slot_shape_instead_of_scanning_body() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r##"(let ((f #[nil ((interactive)) nil]))
                       (list (length f) (commandp f) (interactive-form f)))"##
        ),
        "OK (3 nil nil)"
    );
}

#[test]
fn commandp_distinguishes_gnu_docstring_references_from_oclosure_tags() {
    crate::test_utils::init_test_tracing();
    let classify_doc = |doc| {
        let closure = Value::make_lambda_with_slots(vec![
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            doc,
        ]);
        classify_command_object_in_state(&closure, false)
    };

    for valid_doc in [
        Value::fixnum(7),
        Value::string("inline documentation"),
        Value::cons(Value::string("etc/DOC"), Value::fixnum(42)),
    ] {
        assert_eq!(
            classify_doc(valid_doc),
            CommandpClassification::CheckInteractiveFormProperty(
                InteractiveFormFallback::PropertyOnly
            )
        );
    }
    let oclosure_tag = Value::symbol("neo-test-oclosure-tag");
    assert!(matches!(
        classify_doc(oclosure_tag),
        CommandpClassification::CheckInteractiveFormProperty(
            InteractiveFormFallback::GenericDispatch(closure)
        ) if closure.closure_doc_value() == Some(oclosure_tag)
    ));
}

#[test]
fn commandp_returns_nil_for_raw_unibyte_symbol_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let raw_name = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let raw_symbol = ev
        .apply(Value::symbol("intern"), vec![raw_name])
        .expect("intern raw unibyte symbol");

    let result = builtin_commandp_interactive(&mut ev, &[raw_symbol]).unwrap();

    assert_eq!(result, Value::NIL);
}

#[test]
fn commandp_errors_on_uninterned_interactive_form_property_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((s (make-symbol "neo-cmd-prop")))
                 (fset s (lambda () 1))
                 (put s 'interactive-form '(interactive "p"))
                 (list (condition-case e
                           (commandp s)
                         (error (list (car e)
                                      (equal (car (cdr e))
                                             "Found an 'interactive-form' property!"))))
                       (interactive-form s)
                       (commandp (symbol-function s))))"#
        ),
        r#"OK ((error t) (interactive "p") nil)"#
    );
}

#[test]
fn commandp_only_checks_interactive_form_property_for_gnu_fallback_classes() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r##"(let ((scalar (make-symbol "neo-commandp-scalar"))
                       (macro (make-symbol "neo-commandp-macro"))
                       (non-lambda (make-symbol "neo-commandp-non-lambda")))
                   (defmacro neo-commandp-macro-form () nil)
                   (fset scalar 42)
                   (fset macro "x")
                   (fset non-lambda '(not-lambda))
                   (put scalar 'interactive-form '(interactive))
                   (put macro 'interactive-form '(interactive))
                   (put non-lambda 'interactive-form '(interactive))
                   (put 'neo-commandp-macro-form 'interactive-form '(interactive))
                   (list (condition-case nil (commandp scalar) (error 'error))
                         (condition-case nil (commandp macro t) (error 'error))
                         (condition-case nil (commandp non-lambda) (error 'error))
                         (condition-case nil
                             (commandp 'neo-commandp-macro-form)
                           (error 'error))))"##
        ),
        "OK (nil nil nil nil)"
    );
}

#[test]
fn commandp_true_for_an_interactive_c_subr() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // GNU 31.0.90: (commandp 'make-local-variable) => t, its intspec being
    // "vMake Local Variable: " (src/data.c).  This used to ask about `ignore',
    // which is lisp/subr.el:501 and not a subr at all (DIVERGENCES.md 152).
    let result = builtin_commandp_interactive(&mut ev, &[Value::symbol("make-local-variable")]);
    assert!(result.unwrap().is_truthy());
}

#[test]
fn commandp_true_for_execute_extended_command_from_simple_el() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        gnu_simple_execute_extended_command_eval_all(
            r#"(list (commandp 'execute-extended-command)
                     (subrp (symbol-function 'execute-extended-command)))"#
        ),
        vec!["OK (t nil)".to_string()]
    );
}

#[test]
fn eval_expression_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("eval-expression")
        .expect("missing eval-expression bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
    let result = builtin_commandp_interactive(&mut ev, &[Value::symbol("eval-expression")])
        .expect("commandp should accept eval-expression");
    assert!(result.is_truthy());
}

#[test]
fn commandp_true_for_defun_with_declare_before_interactive() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_all(
            r#"(progn
                 (defun neo-declare-interactive ()
                   (declare (interactive-only t))
                   (interactive)
                   'ok)
                 (commandp 'neo-declare-interactive))"#
        ),
        vec!["OK t".to_string()]
    );
}

#[test]
fn commandp_true_for_builtin_forward_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_commandp_interactive(&mut ev, &[Value::symbol("forward-char")]);
    assert!(result.unwrap().is_truthy());
}

#[test]
fn commandp_handles_keyboard_macros_and_bytecode_interactive_slots() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let bytecode = crate::emacs_core::builtins::symbols::make_byte_code_from_parts(
        &Value::NIL,
        &Value::string(""),
        &Value::vector(vec![]),
        &Value::fixnum(0),
        None,
        Some(&Value::vector(vec![Value::NIL, Value::NIL])),
    )
    .expect("make-byte-code should build commandp fixture");

    assert!(
        builtin_commandp_interactive(&mut ev, &[Value::string("abc")])
            .expect("string keyboard macro should be accepted")
            .is_truthy()
    );
    assert!(
        builtin_commandp_interactive(&mut ev, &[Value::vector(vec![Value::fixnum(1)])])
            .expect("vector keyboard macro should be accepted")
            .is_truthy()
    );
    assert!(
        builtin_commandp_interactive(&mut ev, &[Value::string("abc"), Value::T])
            .expect("FOR-CALL-INTERACTIVELY should reject strings")
            .is_nil()
    );
    assert!(
        builtin_commandp_interactive(&mut ev, &[Value::vector(vec![Value::fixnum(1)]), Value::T])
            .expect("FOR-CALL-INTERACTIVELY should reject vectors")
            .is_nil()
    );
    assert!(
        builtin_commandp_interactive(&mut ev, &[bytecode])
            .expect("bytecode with interactive slot should be a command")
            .is_truthy()
    );
}

#[test]
fn commandp_true_for_registered_rust_editing_commands() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    for name in [
        "backward-char",
        "delete-char",
        "insert-char",
        "upcase-word",
        "downcase-word",
        "capitalize-word",
        "upcase-region",
        "downcase-region",
        "capitalize-region",
        "upcase-initials-region",
        "scroll-up",
        "scroll-down",
        "scroll-left",
        "scroll-right",
        "recenter",
    ] {
        assert!(
            ev.obarray
                .symbol_function(name)
                .is_some_and(|function| function.is_subr()),
            "{name} must be a registered Rust subr"
        );
        let result =
            builtin_commandp_interactive(&mut ev, &[Value::symbol(name)]).expect("commandp call");
        assert!(result.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn abbrev_mode_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("abbrev-mode")
        .expect("missing abbrev-mode bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
    let command = builtin_commandp_interactive(&mut ev, &[Value::symbol("abbrev-mode")])
        .expect("commandp should accept abbrev-mode");
    assert!(command.is_truthy());
}

#[test]
fn bookmark_commands_startup_are_autoloaded() {
    crate::test_utils::init_test_tracing();
    let names = [
        "bookmark-delete",
        "bookmark-jump",
        "bookmark-load",
        "bookmark-rename",
        "bookmark-save",
        "bookmark-set",
    ];
    let ev = eval_with_ldefs_boot_autoloads(&names);
    for name in names {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} startup function cell to be a GNU autoload"
        );
    }
}

#[test]
fn rectangle_commands_startup_are_autoloaded() {
    crate::test_utils::init_test_tracing();
    let names = [
        "clear-rectangle",
        "delete-rectangle",
        "kill-rectangle",
        "open-rectangle",
        "string-rectangle",
        "yank-rectangle",
    ];
    let ev = eval_with_ldefs_boot_autoloads(&names);
    for name in names {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} startup function cell to be a GNU autoload"
        );
    }
}

#[test]
fn simple_commands_are_real_lisp_functions_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    for name in [
        "exchange-point-and-mark",
        "list-processes",
        "process-menu-delete-process",
        "process-menu-mode",
    ] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            !crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} startup function cell to be loaded, not an autoload"
        );
        let command = builtin_commandp_interactive(&mut ev, &[Value::symbol(name)])
            .unwrap_or_else(|err| panic!("commandp should accept {name}: {err:?}"));
        assert!(command.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn replace_commands_are_real_lisp_functions_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    for name in [
        "flush-lines",
        "how-many",
        "keep-lines",
        "query-replace",
        "query-replace-regexp",
        "replace-regexp",
        "replace-string",
    ] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            !crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} startup function cell to be loaded, not an autoload"
        );
        let command = builtin_commandp_interactive(&mut ev, &[Value::symbol(name)])
            .unwrap_or_else(|err| panic!("commandp should accept {name}: {err:?}"));
        assert!(command.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn subr_key_binding_commands_are_real_lisp_functions_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    for name in ["global-set-key", "local-set-key"] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            !crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} startup function cell to be loaded, not an autoload"
        );
        assert!(
            !function.is_subr(),
            "expected {name} startup function cell to be a GNU Lisp function, not a Rust subr"
        );
        let command = builtin_commandp_interactive(&mut ev, &[Value::symbol(name)])
            .unwrap_or_else(|err| panic!("commandp should accept {name}: {err:?}"));
        assert!(command.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn env_command_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("setenv")
        .expect("missing setenv bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
    let command = builtin_commandp_interactive(&mut ev, &[Value::symbol("setenv")])
        .expect("commandp should accept setenv");
    assert!(command.is_truthy());
}

#[test]
fn files_command_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("load-file")
        .expect("missing load-file bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
    let command = builtin_commandp_interactive(&mut ev, &[Value::symbol("load-file")])
        .expect("commandp should accept load-file");
    assert!(command.is_truthy());
}

#[test]
fn regexp_search_aliases_are_available_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    // search-forward-regexp and search-backward-regexp are defalias'd to
    // re-search-forward / re-search-backward in subr.el (not autoloaded).
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    for name in ["search-forward-regexp", "search-backward-regexp"] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            !crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} to be a resolved function (defalias), not an autoload"
        );
    }
}

#[test]
fn upcase_char_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let ev = eval_with_ldefs_boot_autoloads(&["upcase-char"]);
    let function = ev
        .obarray
        .symbol_function("upcase-char")
        .expect("missing upcase-char startup function cell");
    assert!(crate::emacs_core::autoload::is_autoload_value(&function));
}

#[test]
fn mode_and_mark_commands_are_real_lisp_functions_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    for name in ["auto-composition-mode", "set-mark-command"] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            !crate::emacs_core::autoload::is_autoload_value(&function),
            "expected {name} startup function cell to be loaded, not an autoload"
        );
        let command = builtin_commandp_interactive(&mut ev, &[Value::symbol(name)])
            .unwrap_or_else(|err| panic!("commandp should accept {name}: {err:?}"));
        assert!(command.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn count_matches_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("count-matches")
        .expect("missing count-matches bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
    let command = builtin_commandp_interactive(&mut ev, &[Value::symbol("count-matches")])
        .expect("commandp call");
    assert!(command.is_truthy());
}

#[test]
fn kmacro_name_last_macro_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    // kmacro-name-last-macro has a real autoload entry in ldefs-boot.el.
    // name-last-kbd-macro is a defalias to it (not an autoload form), so it
    // is only available after the defalias in ldefs-boot.el is evaluated
    // during bootstrap -- not via the autoload-only loader.
    let mut ev = eval_with_ldefs_boot_autoloads(&["kmacro-name-last-macro"]);
    let function = ev
        .obarray
        .symbol_function("kmacro-name-last-macro")
        .expect("missing kmacro-name-last-macro startup function cell");
    assert!(
        crate::emacs_core::autoload::is_autoload_value(&function),
        "expected kmacro-name-last-macro startup function cell to be a GNU autoload"
    );
    let command = builtin_commandp_interactive(&mut ev, &[Value::symbol("kmacro-name-last-macro")])
        .expect("commandp should accept kmacro-name-last-macro");
    assert!(
        command.is_truthy(),
        "expected commandp true for kmacro-name-last-macro"
    );
}

#[test]
fn raw_context_does_not_prebind_kmacro_name_aliases() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new();
    for name in ["kmacro-name-last-macro", "name-last-kbd-macro"] {
        assert!(
            ev.obarray.symbol_function_id(intern(name)).is_none(),
            "{name} should come from GNU ldefs-boot/kmacro Lisp, not Context::new"
        );
    }
}

#[test]
fn ldefs_boot_aliases_name_last_kbd_macro_to_kmacro_name_last_macro() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let ldefs_source =
        fs::read_to_string(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("lisp/ldefs-boot.el"))
            .expect("read ldefs-boot.el");
    eval_first_form_after_marker(
        &mut ev,
        &ldefs_source,
        "(autoload 'kmacro-name-last-macro \"kmacro\"",
    );
    eval_first_form_after_marker(
        &mut ev,
        &ldefs_source,
        "(defalias 'name-last-kbd-macro #'kmacro-name-last-macro)",
    );

    let kmacro = ev
        .obarray
        .symbol_function("kmacro-name-last-macro")
        .expect("kmacro-name-last-macro autoload");
    assert!(
        crate::emacs_core::autoload::is_autoload_value(&kmacro),
        "GNU ldefs-boot should install kmacro-name-last-macro as an autoload"
    );
    assert_eq!(
        ev.obarray.symbol_function("name-last-kbd-macro"),
        Some(Value::symbol("kmacro-name-last-macro"))
    );
}

#[test]
fn remove_hook_is_available_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    // remove-hook is a defun in subr.el, not autoloaded in GNU Emacs.
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("remove-hook")
        .expect("missing remove-hook startup function cell");
    assert!(
        !crate::emacs_core::autoload::is_autoload_value(&function),
        "expected remove-hook to be a resolved function, not an autoload"
    );
}

#[test]
fn commandp_true_for_additional_registered_rust_commands() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    for name in [
        "base64-decode-region",
        "base64-encode-region",
        "base64url-encode-region",
        "decode-coding-region",
        "encode-coding-region",
        "eval-buffer",
        "goto-char",
        "iconify-frame",
        "kill-emacs",
        "lower-frame",
        "make-frame-invisible",
        "make-frame-visible",
        "make-indirect-buffer",
        "open-dribble-file",
        "raise-frame",
        "re-search-forward",
        "redirect-debugging-output",
        "rename-buffer",
        "select-frame",
        "transpose-regions",
        "kill-process",
        "signal-process",
        "suspend-emacs",
        "top-level",
        "unix-sync",
        "write-region",
        "x-menu-bar-open-internal",
    ] {
        assert!(
            ev.obarray
                .symbol_function(name)
                .is_some_and(|function| function.is_subr()),
            "{name} must be a registered Rust subr"
        );
        let result =
            builtin_commandp_interactive(&mut ev, &[Value::symbol(name)]).expect("commandp call");
        assert!(result.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn commandp_true_for_loaded_lisp_commands_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    for name in [
        "backward-kill-word",
        "copy-region-as-kill",
        "delete-horizontal-space",
        "delete-indentation",
        "display-buffer",
        "forward-sexp",
        "gui-set-selection",
        "isearch-forward",
        "just-one-space",
        "kill-line",
        "kill-region",
        "kill-ring-save",
        "kill-whole-line",
        "kill-word",
        "make-directory",
        "move-beginning-of-line",
        "move-end-of-line",
        "newline-and-indent",
        "open-line",
        "recenter-top-bottom",
        "replace-buffer-contents",
        "scroll-down-command",
        "scroll-up-command",
        "set-buffer-process-coding-system",
        "tab-to-tab-stop",
        "transient-mark-mode",
        "transpose-chars",
        "transpose-lines",
        "transpose-paragraphs",
        "transpose-sentences",
        "transpose-sexps",
        "transpose-words",
        "yank",
        "yank-pop",
    ] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing {name} startup function cell"));
        assert!(
            !function.is_subr(),
            "expected {name} startup function cell to remain a GNU Lisp command"
        );
        let result =
            builtin_commandp_interactive(&mut ev, &[Value::symbol(name)]).expect("commandp call");
        assert!(result.is_truthy(), "expected commandp true for {name}");
    }
}

#[test]
fn commandp_false_for_noninteractive_builtin() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_commandp_interactive(&mut ev, &[Value::symbol("car")]);
    assert!(result.unwrap().is_nil());
}

/// GNU `Fcommandp` compares symbol properties against its predeclared
/// `Qinteractive_form`; it never re-interns the property name while filtering
/// an obarray. Empty-prefix M-x completion calls this once per candidate.
#[test]
fn commandp_uses_predeclared_interactive_form_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let function = Value::symbol("car");
    let _ = InteractiveFormSymbol::id();

    crate::emacs_core::intern::reset_intern_calls();
    let result = builtin_commandp_interactive(&mut ev, &[function]).expect("commandp car");

    assert!(result.is_nil());
    assert_eq!(
        crate::emacs_core::intern::intern_calls(),
        0,
        "commandp must use the cached Qinteractive_form identity"
    );
}

#[test]
fn commandp_rejects_overflow_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_commandp_interactive(&mut ev, &[Value::symbol("ignore"), Value::NIL, Value::NIL])
            .expect_err("commandp should reject more than two arguments");
    match result {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn commandp_resolves_aliases_and_symbol_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_function("t", Value::symbol("forward-char"));
    ev.obarray
        .set_symbol_function(":vm-command-alias-keyword", Value::symbol("forward-char"));
    ev.obarray.set_symbol_function("vm-command-alias", Value::T);
    ev.obarray.set_symbol_function(
        "vm-command-alias-keyword",
        Value::keyword(":vm-command-alias-keyword"),
    );

    let t_result = builtin_commandp_interactive(&mut ev, &[Value::T]);
    assert!(t_result.unwrap().is_truthy());
    let keyword_result =
        builtin_commandp_interactive(&mut ev, &[Value::keyword(":vm-command-alias-keyword")]);
    assert!(keyword_result.unwrap().is_truthy());
    let alias_result = builtin_commandp_interactive(&mut ev, &[Value::symbol("vm-command-alias")]);
    assert!(alias_result.unwrap().is_truthy());
    let keyword_alias_result =
        builtin_commandp_interactive(&mut ev, &[Value::symbol("vm-command-alias-keyword")]);
    assert!(keyword_alias_result.unwrap().is_truthy());
}

/// GNU `eval.c:Fcommandp` derives command identity from the resolved function
/// object.  Mutable auxiliary metadata must not make an unbound symbol a
/// command: otherwise a stale registry entry can survive a definition change
/// and disagree with the Lisp-visible function cell.
#[test]
fn commandp_ignores_registry_metadata_for_an_unbound_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let symbol = crate::emacs_core::intern::intern("neovm--stale-command-metadata");
    ev.interactive
        .register_interactive(symbol, InteractiveSpec::no_args());

    let result = builtin_commandp_interactive(&mut ev, &[Value::from_sym_id(symbol)])
        .expect("commandp should classify an unbound symbol");

    assert!(
        result.is_nil(),
        "only the GNU-shaped function cell may establish command identity"
    );
}

#[test]
fn commandp_true_for_lambda_with_interactive_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let lambda = eval_all_with(&mut ev, "(lambda () (interactive) 1)");
    let value = ev
        .eval_str("(lambda () (interactive) 1)")
        .expect("lambda form should evaluate");
    assert_eq!(lambda[0], "OK #[nil (1) nil nil nil nil]");
    let result = builtin_commandp_interactive(&mut ev, &[value]);
    assert!(result.unwrap().is_truthy());
}

#[test]
fn commandp_true_for_quoted_lambda_with_interactive_form() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let quoted_lambda = ev
        .eval_str("'(lambda () \"doc\" (interactive) 1)")
        .expect("quoted lambda should evaluate");
    let result = builtin_commandp_interactive(&mut ev, &[quoted_lambda]);
    assert!(result.unwrap().is_truthy());
}

#[test]
fn call_interactively_state_resolution_handles_default_and_noarg_cases() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("current-prefix-arg", Value::list(vec![Value::fixnum(4)]));

    let mut builtin_plan =
        plan_call_interactively_for_test(&mut ev, &[Value::symbol("forward-char")])
            .expect("plan builtin default interactive command");
    let (_, builtin_args) = resolve_call_interactively_target_and_args_in_state(
        &mut ev.obarray,
        &mut Vec::new(),
        &mut ev.buffers,
        &ev.custom,
        ev.specpdl.as_slice(),
        &mut builtin_plan,
    )
    .expect("resolve builtin default args")
    .expect("shared-state builtin default path");
    assert_eq!(builtin_args, vec![Value::fixnum(4)]);

    let lambda = ev
        .eval_str("(lambda () (interactive) 1)")
        .expect("eval lambda");
    let mut lambda_plan =
        plan_call_interactively_for_test(&mut ev, &[lambda]).expect("plan interactive lambda");
    let (_, lambda_args) = resolve_call_interactively_target_and_args_in_state(
        &mut ev.obarray,
        &mut Vec::new(),
        &mut ev.buffers,
        &ev.custom,
        ev.specpdl.as_slice(),
        &mut lambda_plan,
    )
    .expect("resolve lambda args")
    .expect("shared-state no-arg lambda path");
    assert!(lambda_args.is_empty());
}

#[test]
fn call_interactively_state_resolution_defers_prompting_specs_to_eval() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let lambda = ev
        .eval_str("(lambda (x) (interactive \"sPrompt: \") x)")
        .expect("eval prompting lambda");
    let mut plan =
        plan_call_interactively_for_test(&mut ev, &[lambda]).expect("plan prompting lambda");
    let resolved = resolve_call_interactively_target_and_args_in_state(
        &mut ev.obarray,
        &mut Vec::new(),
        &mut ev.buffers,
        &ev.custom,
        ev.specpdl.as_slice(),
        &mut plan,
    )
    .expect("resolve prompting lambda");
    assert!(resolved.is_none());
}

#[test]
fn call_interactively_state_resolution_handles_simple_string_codes_without_eval() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("current-prefix-arg", Value::list(vec![Value::fixnum(4)]));
    let current = ev.buffers.current_buffer_id().expect("current buffer");
    let _ = ev.buffers.replace_buffer_contents(current, "abcd");
    let _ = ev
        .buffers
        .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
    let _ = ev
        .buffers
        .set_buffer_mark_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(1));

    let event = ev
        .eval_str("(list 'mouse-1 (list (list (selected-window) (point) '(0 . 0) 0)))")
        .expect("eval event");
    let lambda = ev
        .eval_str(
            "(lambda (raw num pt mk beg end evt up ignored)
           (interactive \"P\np\nd\nm\nr\ne\nU\ni\")
           (list raw num pt mk beg end evt up ignored))",
        )
        .expect("eval lambda");
    let mut plan = plan_call_interactively_for_test(
        &mut ev,
        &[lambda, Value::NIL, Value::vector(vec![event])],
    )
    .expect("plan simple string-code lambda");
    let (_, args) = resolve_call_interactively_target_and_args_in_state(
        &mut ev.obarray,
        &mut Vec::new(),
        &mut ev.buffers,
        &ev.custom,
        ev.specpdl.as_slice(),
        &mut plan,
    )
    .expect("resolve simple string-code args")
    .expect("shared-state simple string-code path");
    assert_eq!(
        args,
        vec![
            Value::list(vec![Value::fixnum(4)]),
            Value::fixnum(4),
            Value::fixnum(3),
            Value::fixnum(2),
            Value::fixnum(2),
            Value::fixnum(3),
            event,
            Value::NIL,
            Value::NIL,
        ]
    );
}

#[test]
fn call_interactively_state_resolution_applies_shift_selection_prefix_in_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let current = ev.buffers.current_buffer_id().expect("current buffer");
    let _ = ev.buffers.replace_buffer_contents(current, "abcd");
    let _ = ev
        .buffers
        .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
    ev.obarray
        .set_symbol_value("this-command-keys-shift-translated", Value::T);
    ev.obarray.set_symbol_value("shift-select-mode", Value::T);

    let lambda = ev
        .eval_str("(lambda (pt) (interactive \"^d\") pt)")
        .expect("eval lambda");
    let mut plan =
        plan_call_interactively_for_test(&mut ev, &[lambda]).expect("plan shift-selection lambda");
    let (_, args) = resolve_call_interactively_target_and_args_in_state(
        &mut ev.obarray,
        &mut Vec::new(),
        &mut ev.buffers,
        &ev.custom,
        ev.specpdl.as_slice(),
        &mut plan,
    )
    .expect("resolve shift-selection args")
    .expect("shared-state shift-selection path");
    assert_eq!(args, vec![Value::fixnum(3)]);

    let buf = ev.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(2));
    assert_eq!(buf.get_buffer_local("mark-active"), Some(Value::T));
}

#[test]
fn call_interactively_state_resolution_handles_optional_coding_without_prefix() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let lambda = ev
        .eval_str("(lambda (coding) (interactive \"ZCoding: \") coding)")
        .expect("eval lambda");
    let mut plan =
        plan_call_interactively_for_test(&mut ev, &[lambda]).expect("plan optional coding lambda");
    let (_, args) = resolve_call_interactively_target_and_args_in_state(
        &mut ev.obarray,
        &mut Vec::new(),
        &mut ev.buffers,
        &ev.custom,
        ev.specpdl.as_slice(),
        &mut plan,
    )
    .expect("resolve optional coding args")
    .expect("shared-state optional coding path");
    assert_eq!(args, vec![Value::NIL]);
}

#[test]
fn interactive_lambda_r_capital_spec_uses_use_region_p_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let current = ev.buffers.current_buffer_id().expect("current buffer");
    let _ = ev.buffers.replace_buffer_contents(current, "abcd");
    let _ = ev
        .buffers
        .goto_buffer_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(2));
    let _ = ev
        .buffers
        .set_buffer_mark_emacs_byte_pos(current, crate::buffer::EmacsBytePos::new(1));

    let mut context = InteractiveInvocationContext::default();
    let _ = ev.eval_str("(fset 'use-region-p (lambda () nil))");
    let region_spec = crate::heap_types::LispString::from_utf8("R");
    let args = interactive_args_from_string_code(
        &mut ev,
        &region_spec,
        CommandInvocationKind::CallInteractively,
        &mut context,
    )
    .expect("resolve inactive R")
    .expect("R should produce args");
    assert_eq!(args, vec![Value::NIL, Value::NIL]);

    let _ = ev.eval_str("(fset 'use-region-p (lambda () t))");
    let args = interactive_args_from_string_code(
        &mut ev,
        &region_spec,
        CommandInvocationKind::CallInteractively,
        &mut context,
    )
    .expect("resolve active R")
    .expect("R should produce args");
    assert_eq!(args, vec![Value::fixnum(2), Value::fixnum(3)]);
}

// -------------------------------------------------------------------
// interactive-p / called-interactively-p
// -------------------------------------------------------------------

#[test]
fn interactive_p_false_by_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_interactive_p(&mut ev, vec![]);
    assert!(result.unwrap().is_nil());
}

#[test]
fn interactive_p_nil_when_interactive() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.interactive.push_interactive_call(true);
    let result = builtin_interactive_p(&mut ev, vec![]);
    ev.interactive.pop_interactive_call();
    assert!(result.unwrap().is_nil());
}

#[test]
fn called_interactively_p_false_by_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_called_interactively_p(&mut ev, vec![]);
    assert!(result.unwrap().is_nil());
}

#[test]
fn called_interactively_p_with_kind() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("any")]);
    assert!(result.unwrap().is_nil());
}

#[test]
fn called_interactively_p_kind_interactive_is_nil_when_interactive() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.interactive.push_interactive_call(true);
    let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("interactive")]);
    ev.interactive.pop_interactive_call();
    assert!(result.unwrap().is_nil());
}

#[test]
fn called_interactively_p_kind_any_is_t_when_interactive() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.interactive.push_interactive_call(true);
    let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("any")]);
    ev.interactive.pop_interactive_call();
    assert!(result.unwrap().is_truthy());
}

#[test]
fn called_interactively_p_unknown_kind_is_t_when_interactive() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.interactive.push_interactive_call(true);
    let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("foo")]);
    ev.interactive.pop_interactive_call();
    assert!(result.unwrap().is_truthy());
}

#[test]
fn called_interactively_p_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_called_interactively_p(&mut ev, vec![Value::symbol("any"), Value::symbol("extra")]);
    assert!(result.is_err());
}

// -------------------------------------------------------------------
// this-command-keys / this-command-keys-vector
// -------------------------------------------------------------------

#[test]
fn this_command_keys_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_this_command_keys(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_utf8_str(), Some(""));
}

#[test]
fn this_command_keys_after_set() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_this_command_keys_from_string(&crate::heap_types::LispString::from_utf8("ab"))
        .unwrap();
    let result = builtin_this_command_keys(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("ab"));
}

#[test]
fn this_command_keys_vector_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
    assert!(result.is_vector());
}

#[test]
fn this_command_keys_vector_after_set() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_this_command_keys_from_string(&crate::heap_types::LispString::from_utf8("x"))
        .unwrap();
    let result = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
    if result.is_vector() {
        let v = result.as_vector_data().unwrap().clone();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], Value::fixnum('x' as i64));
    } else {
        panic!("expected vector");
    }
}

#[test]
fn this_command_keys_uses_read_command_key_chars() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);

    let text = builtin_this_command_keys(&mut ev, vec![]).unwrap();
    assert_eq!(text.as_utf8_str(), Some("a"));

    let vec_result = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
    match vec_result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = vec_result.as_vector_data().unwrap().clone();
            assert_eq!(items.as_slice(), &[Value::fixnum(97)]);
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn this_command_keys_returns_vector_for_non_char_read_command_keys() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::list(vec![Value::symbol("mouse-1")])]);

    let result = builtin_this_command_keys(&mut ev, vec![]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert!(items[0].is_cons());
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn this_single_command_keys_prefers_read_command_key_vector() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);

    let result = builtin_this_single_command_keys(&mut ev, vec![]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.as_slice(), &[Value::fixnum(97)]);
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn this_single_command_raw_keys_tracks_raw_sequence() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_command_key_sequences(
        vec![Value::fixnum('b' as i64)],
        vec![Value::fixnum('a' as i64)],
    );

    let result = builtin_this_single_command_raw_keys(&mut ev, vec![]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.as_slice(), &[Value::fixnum('a' as i64)]);
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn clear_this_command_keys_clears_read_key_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);

    let result = builtin_clear_this_command_keys(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
    assert_eq!(ev.read_command_keys(), &[]);

    let vec_result = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
    match vec_result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = vec_result.as_vector_data().unwrap().clone();
            assert!(items.is_empty());
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn set_this_command_keys_clears_raw_sequence_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_command_key_sequences(
        vec![Value::fixnum('q' as i64)],
        vec![Value::fixnum('z' as i64)],
    );
    ev.set_this_command_keys_from_string(&crate::heap_types::LispString::from_utf8(
        "\u{00f8}foo\r",
    ))
    .unwrap();

    let translated = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
    let raw = builtin_this_single_command_raw_keys(&mut ev, vec![]).unwrap();
    match translated.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = translated.as_vector_data().unwrap().clone();
            assert_eq!(
                items[0],
                Value::fixnum(('x' as i64) | ((1u32 << 27) as i64))
            );
            assert_eq!(items[1], Value::fixnum('f' as i64));
            assert_eq!(items[2], Value::fixnum('o' as i64));
            assert_eq!(items[3], Value::fixnum('o' as i64));
            assert_eq!(items[4], Value::fixnum('\r' as i64));
        }
        other => panic!("expected vector, got {other:?}"),
    }
    match raw.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = raw.as_vector_data().unwrap().clone();
            assert!(items.is_empty());
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn clear_this_command_keys_without_keep_record_clears_recent_input_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.record_input_event(Value::fixnum(97));
    assert_eq!(ev.recent_input_events(), &[Value::fixnum(97)]);

    let result = builtin_clear_this_command_keys(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn clear_this_command_keys_with_nil_keep_record_clears_recent_input_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.record_input_event(Value::fixnum(98));
    assert_eq!(ev.recent_input_events(), &[Value::fixnum(98)]);

    let result = builtin_clear_this_command_keys(&mut ev, vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn clear_this_command_keys_with_keep_record_preserves_recent_input_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.record_input_event(Value::fixnum(99));
    assert_eq!(ev.recent_input_events(), &[Value::fixnum(99)]);

    let result = builtin_clear_this_command_keys(&mut ev, vec![Value::symbol("t")]).unwrap();
    assert!(result.is_nil());
    assert_eq!(ev.recent_input_events(), &[Value::fixnum(99)]);
}

#[test]
fn clear_this_command_keys_rejects_more_than_one_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_clear_this_command_keys(&mut ev, vec![Value::fixnum(1), Value::fixnum(2)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "wrong-number-of-arguments"
                && sig.data
                    == vec![Value::symbol("clear-this-command-keys"), Value::fixnum(2)]
    ));
}

// -------------------------------------------------------------------
// key-binding / local-key-binding / global-key-binding
// -------------------------------------------------------------------

#[test]
fn key_binding_global() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let km = make_list_keymap();
    ev.select_global_map(km);
    // ctrl-f = char 6
    let ctrl_f = Value::fixnum(6);
    crate::emacs_core::keymap::list_keymap_define(km, ctrl_f, Value::symbol("forward-char"));

    let result = builtin_key_binding(&mut ev, vec![Value::string("\x06")]).unwrap();
    assert_eq!(result.as_symbol_name(), Some("forward-char"));
}

#[test]
fn key_binding_prefers_minor_and_emulation_mode_maps() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap))
                     (m (make-sparse-keymap))
                     (minor-mode-map-alist nil)
                 (demo-mode t))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key m "\x01" 'forward-char)
                 (define-key l "\x01" 'self-insert-command)
                 (setq minor-mode-map-alist (list (cons 'demo-mode m)))
                 (key-binding "\x01"))"#
        ),
        "OK forward-char"
    );
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (m-minor (make-sparse-keymap))
                     (m-emu (make-sparse-keymap))
                     (minor-mode-map-alist nil)
                 (emulation-mode-map-alists nil)
                 (minor-mode t)
                 (emu-mode t))
                 (use-global-map g)
                 (define-key m-minor "\x01" 'self-insert-command)
                 (define-key m-emu "\x01" 'forward-char)
                 (setq minor-mode-map-alist (list (cons 'minor-mode m-minor)))
                 (setq emulation-mode-map-alists (list (list (cons 'emu-mode m-emu))))
                 (key-binding "\x01"))"#
        ),
        "OK forward-char"
    );
}

#[test]
fn key_binding_activates_minor_mode_maps_keyed_by_uninterned_symbols_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((mode (make-symbol "private-minor-mode"))
                       (map (make-sparse-keymap))
                       (minor-mode-map-alist (list (cons mode map))))
                  (make-variable-buffer-local mode)
                  (set mode t)
                  (define-key map "\x01" 'forward-char)
                  (list (key-binding "\x01")
                        (if (memq map (current-active-maps)) t)))"#
        ),
        "OK (forward-char t)"
    );
}

#[test]
fn key_binding_ignores_invalid_active_minor_emulation_entries() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (minor-mode-map-alist '((demo-mode . 999999)))
                     (demo-mode t))
                 (use-global-map g)
                 (define-key g "\x01" 'self-insert-command)
                 (key-binding "\x01"))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (emulation-mode-map-alists (list (list (cons 'demo-mode 999999))))
                     (demo-mode t))
                 (use-global-map g)
                 (define-key g "\x01" 'self-insert-command)
                 (key-binding "\x01"))"#
        ),
        "OK self-insert-command"
    );
}

#[test]
fn key_binding_applies_command_remapping_unless_no_remap() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key g "a" 'self-insert-command)
                 (define-key g [remap self-insert-command] 'forward-char)
                 (list (key-binding "a")
                       (key-binding "a" t nil)
                       (key-binding "a" t t)))"#
        ),
        "OK (forward-char forward-char self-insert-command)"
    );
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key g "a" 'self-insert-command)
                 (define-key g [remap self-insert-command] t)
                 (key-binding "a"))"#
        ),
        "OK self-insert-command"
    );
}

#[test]
fn key_binding_applies_menu_item_filter_prefix_keymap() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key g [tool-bar]
                   '(menu-item "tool bar" ignore
                               :filter
                               (lambda (_ignore)
                                 (let ((m (make-sparse-keymap)))
                                   (define-key m [open-file]
                                     '(menu-item "Open" forward-char))
                                   m))))
                 (key-binding [tool-bar open-file]))"#
        ),
        "OK forward-char"
    );
}

#[test]
fn key_binding_unbound() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_key_binding(&mut ev, vec![Value::string("\x1a")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn key_binding_empty_returns_keymap_list() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap)))
                 (use-global-map g)
                 (let ((m (key-binding "")))
                   (and (consp m) (keymapp (car m)))))"#
        ),
        "OK t"
    );
}

#[test]
fn key_binding_empty_vector_is_nil() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one(r#"(key-binding [])"#), "OK nil");
}

#[test]
fn key_binding_default_plain_char_self_insert() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_key_binding(&mut ev, vec![Value::string("a")]).unwrap();
    assert_eq!(result.as_symbol_name(), Some("self-insert-command"));
}

#[test]
fn key_binding_too_many_args_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_key_binding(
        &mut ev,
        vec![
            Value::string("\x03"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn key_binding_integer_position_out_of_range_signals_args_out_of_range() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((g (make-sparse-keymap)))
                   (use-global-map g)
                   (define-key g "a" 'forward-char)
                   (let ((err (condition-case e
                                  (key-binding "a" t nil 0)
                                (error e))))
                     (list (car err) (bufferp (nth 1 err)) (nth 2 err)))))"#
        ),
        "OK (args-out-of-range t 0)"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((g (make-sparse-keymap)))
                   (use-global-map g)
                   (define-key g "a" 'forward-char)
                   (let ((err (condition-case e
                                  (key-binding "a" t nil 2)
                                (error e))))
                     (list (car err) (bufferp (nth 1 err)) (nth 2 err)))))"#
        ),
        "OK (args-out-of-range t 2)"
    );
}

#[test]
fn key_binding_non_position_designators_default_to_point() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((g (make-sparse-keymap)))
                   (use-global-map g)
                   (define-key g "a" 'forward-char)
                   (list
                    (key-binding "a" t nil 1)
                    (key-binding "a" t nil t)
                    (key-binding "a" t nil 'foo)
                    (key-binding "a" t nil "x")
                    (key-binding "a" t nil [1])
                    (key-binding "a" t nil '(1))
                    (key-binding "a" t nil 1.5))))"#
        ),
        "OK (forward-char forward-char forward-char forward-char forward-char forward-char forward-char)"
    );
}

#[test]
fn current_active_maps_and_key_binding_use_live_window_position_buffer() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((w1 (selected-window))
                      (b1 (current-buffer))
                      (b2 (get-buffer-create " *phase8-window-pos*"))
                      (w2 (split-window-internal w1 nil 'right nil))
                      (g (make-sparse-keymap))
                      (l1 (make-sparse-keymap))
                      (l2 (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key g "a" 'ignore)
                 (use-local-map l1)
                 (define-key l1 "a" 'forward-char)
                 (set-window-buffer w2 b2)
                 (set-buffer b2)
                 (use-local-map l2)
                 (define-key l2 "a" 'backward-char)
                 (set-buffer b1)
                 (list
                  (key-binding "a" t nil w2)
                  (key-binding "a" t nil w1)
                  (let ((maps2 (current-active-maps t w2))
                        (maps1 (current-active-maps t w1)))
                    (list (eq (car maps2) l2)
                          (eq (car maps1) l1)))))"#
        ),
        "OK (backward-char forward-char (t t))"
    );
}

#[test]
fn global_key_binding_bootstrap_matches_subr_el() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        gnu_subr_keymap_eval_all(
            r#"(list (subrp (symbol-function 'global-key-binding))
                     (keymapp (global-key-binding ""))
                     (global-key-binding "\ex")
                     (global-key-binding "a")
                     (global-key-binding "ab"))"#
        ),
        vec!["OK (nil t execute-extended-command self-insert-command 1)".to_string()]
    );
}

#[test]
fn global_key_binding_bootstrap_wrong_arity_matches_lisp() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        gnu_subr_keymap_eval_all(
            r#"(let ((err (condition-case e
                             (global-key-binding "\x03" nil nil)
                           (error e))))
                 (car err))"#
        ),
        vec!["OK wrong-number-of-arguments".to_string()]
    );
}

#[test]
fn key_binding_and_lookup_key_follow_meta_prefix_char() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (esc (make-sparse-keymap)))
                 (define-key esc "x" 'execute-extended-command)
                 (define-key g "\e" esc)
                 (use-global-map g)
                 (list (key-binding [134217848])
                       (lookup-key g [134217848])))"#
        ),
        "OK (execute-extended-command execute-extended-command)"
    );
}

#[test]
fn key_binding_and_lookup_key_follow_ctl_x_prefix_map() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (ctlx (make-sparse-keymap)))
                 (define-key ctlx "2" 'split-window-below)
                 (define-key ctlx "3" 'split-window-right)
                 (define-key g "\C-x" ctlx)
                 (use-global-map g)
                 (list (key-binding [24 50])
                       (lookup-key g [24 50])
                       (key-binding [24 51])
                       (lookup-key g [24 51])))"#
        ),
        "OK (split-window-below split-window-below split-window-right split-window-right)"
    );
}

#[test]
fn lookup_key_accepts_legacy_menu_symbol_case_and_spaces() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [menu-bar text] (cons "Text" 'ignore))
                 (define-key m [menu-bar some-menu] (cons "Some" 'ignore))
                 (list (lookup-key m [menu-bar Text])
                       (lookup-key m (vector 'menu-bar (intern "Some Menu")))))"#
        ),
        "OK (ignore ignore)"
    );
}

#[test]
fn menu_bar_menu_at_x_y_uses_recursive_display_order_for_final_items() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((g (make-sparse-keymap))
                      (embedded (make-sparse-keymap))
                      (file-menu (make-sparse-keymap))
                      (edit-menu (make-sparse-keymap))
                      (help-menu (make-sparse-keymap)))
                 (define-key embedded [file] (cons "File" file-menu))
                 (define-key embedded [edit] (cons "Edit" edit-menu))
                 (define-key embedded [help-menu] (cons "Help" help-menu))
                 (define-key g [menu-bar] (list 'keymap embedded))
                 (use-global-map g)
                 (setq menu-bar-final-items '(help-menu))
                 (list (menu-bar-menu-at-x-y 0 0)
                       (menu-bar-menu-at-x-y 5 0)
                       (menu-bar-menu-at-x-y 10 0)))"#
        ),
        "OK (edit file help-menu)"
    );
}

#[test]
fn menu_bar_menu_at_x_y_prefers_pending_native_click_key() {
    crate::test_utils::init_test_tracing();
    let mut ev = eval_with_interactive_shims();
    let frame_id = ev
        .frames
        .create_frame("F1", 960, 640, crate::buffer::BufferId(1));
    ev.frames.select_frame(frame_id);
    ev.pending_menu_bar_popup_anchor = Some(crate::emacs_core::MenuBarPopupAnchor {
        frame_id,
        menu_key: Some("help-menu".to_string()),
        menu_x: 49,
        x: 439,
        y: 0,
        width: 47,
        height: 18,
    });

    assert_eq!(
        format_eval_result(&ev.eval_str(
            r#"(let* ((g (make-sparse-keymap))
                          (file-menu (make-sparse-keymap))
                          (help-menu (make-sparse-keymap)))
                     (define-key g [menu-bar file] (cons "File" file-menu))
                     (define-key g [menu-bar help-menu] (cons "Help" help-menu))
                     (setq global-map g)
                     (menu-bar-menu-at-x-y 0 0))"#
        )),
        "OK help-menu"
    );
}

#[test]
fn lookup_key_matches_parameterized_mouse_event_on_event_type() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [mouse-movement] 'move)
                 (lookup-key m (vector (list 'mouse-movement (list 1 2)))))"#
        ),
        "OK move"
    );
}

#[test]
fn define_key_normalizes_lucid_events_in_keyboard_macro_definitions() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [kp-6] (vector (list ?6)))
                 (lookup-key m [kp-6]))"#,
        ),
        "OK [54]"
    );
}

#[test]
fn define_key_sequence_preserves_gnu_prefix_symbol_bindings() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((esc (make-keymap))
                     (ctlx (make-keymap))
                     (g (make-keymap)))
                 (fset 'ESC-prefix esc)
                 (fset 'Control-X-prefix ctlx)
                 (define-key esc "x" 'execute-extended-command)
                 (define-key ctlx "2" 'split-window-below)
                 (define-key ctlx "3" 'split-window-right)
                 (define-key g "\e" 'ESC-prefix)
                 (define-key g "\C-x" 'Control-X-prefix)
                 (define-key g "\e\e\e" 'keyboard-escape-quit)
                 (define-key g "\C-x\C-z" 'suspend-emacs)
                 (use-global-map g)
                 (list (lookup-key g "\e")
                       (lookup-key esc "x")
                       (lookup-key g "\C-x")
                       (lookup-key ctlx "2")
                       (lookup-key ctlx "3")
                       (lookup-key g "\e\e\e")
                       (lookup-key g "\C-x\C-z")
                       (key-binding [134217848])
                       (key-binding [24 50])
                       (key-binding [24 51])))"#
        ),
        "OK (ESC-prefix execute-extended-command Control-X-prefix split-window-below split-window-right keyboard-escape-quit suspend-emacs execute-extended-command split-window-below split-window-right)"
    );
}

#[test]
fn local_key_binding_nil_when_no_local_map() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_local_key_binding(&mut ev, vec![Value::string("\x03")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn local_key_binding_too_many_args_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_local_key_binding(&mut ev, vec![Value::string("\x03"), Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn minor_mode_key_binding_returns_nil_when_no_modes_are_active() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_minor_mode_key_binding(&mut ev, vec![Value::string("\x03")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn minor_mode_key_binding_returns_first_matching_mode_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((m1 (make-sparse-keymap))
                      (m2 (make-sparse-keymap)))
                 (define-key m1 "\x01" 'ignore)
                 (define-key m2 "\x01" 'forward-char)
                 (let ((minor-mode-map-alist (list (cons 'mode1 m1)
                                                   (cons 'mode2 m2)))
                       (mode1 t)
                       (mode2 t))
                   (minor-mode-key-binding "\x01")))"#
        ),
        "OK ((mode1 . ignore))"
    );
}

#[test]
fn minor_mode_key_binding_invalid_keymap_id_errors_for_active_mode() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((minor-mode-map-alist '((demo-mode . 999999)))
                     (demo-mode t))
                 (condition-case err
                     (minor-mode-key-binding "\x01")
                   (error err)))"#
        ),
        "OK (wrong-type-argument keymapp 999999)"
    );
}

#[test]
fn minor_mode_key_binding_prefers_emulation_mode_maps() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((m-minor (make-sparse-keymap))
                      (m-emu (make-sparse-keymap)))
                 (define-key m-minor "\x01" 'ignore)
                 (define-key m-emu "\x01" 'forward-char)
                 (let ((emulation-mode-map-alists (list (list (cons 'emu-mode m-emu))))
                       (minor-mode-map-alist (list (cons 'minor-mode m-minor)))
                       (emu-mode t)
                       (minor-mode t))
                   (minor-mode-key-binding "\x01")))"#
        ),
        "OK ((emu-mode . forward-char))"
    );
}

#[test]
fn minor_mode_key_binding_prefers_overriding_mode_maps() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((m-over (make-sparse-keymap))
                      (m-minor (make-sparse-keymap)))
                 (define-key m-over "\x01" 'forward-char)
                 (define-key m-minor "\x01" 'ignore)
                 (let ((minor-mode-overriding-map-alist (list (cons 'minor-mode m-over)))
                       (minor-mode-map-alist (list (cons 'minor-mode m-minor)))
                       (minor-mode t))
                   (minor-mode-key-binding "\x01")))"#
        ),
        "OK ((minor-mode . forward-char))"
    );
}

#[test]
fn minor_mode_key_binding_resolves_symbol_emulation_alists() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((m (make-sparse-keymap)))
                 (define-key m "\x01" 'ignore)
                 (let ((emu-alist (list (cons 'emu-mode m)))
                       (emulation-mode-map-alists '(emu-alist))
                       (emu-mode t))
                   (minor-mode-key-binding "\x01")))"#
        ),
        "OK ((emu-mode . ignore))"
    );
}

#[test]
fn minor_mode_key_binding_invalid_emulation_keymap_id_errors() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((emulation-mode-map-alists (list (list (cons 'emu-mode 999999))))
                     (emu-mode t))
                 (condition-case err
                     (minor-mode-key-binding "\x01")
                   (error err)))"#
        ),
        "OK (wrong-type-argument keymapp 999999)"
    );
}

#[test]
fn minor_mode_key_binding_too_many_args_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_minor_mode_key_binding(
        &mut ev,
        vec![Value::string("\x03"), Value::T, Value::symbol("extra")],
    );
    assert!(result.is_err());
}

// -------------------------------------------------------------------
// describe-key-briefly
// -------------------------------------------------------------------

#[test]
fn describe_key_briefly_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let function = ev
        .obarray
        .symbol_function("describe-key-briefly")
        .expect("missing describe-key-briefly bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
}

#[test]
fn describe_key_briefly_loads_from_gnu_help_el() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((g (make-sparse-keymap)))
             (define-key g (kbd "C-f") #'forward-char)
             (use-global-map g)
             (describe-key-briefly (kbd "C-f")))"#,
    );
    assert_eq!(
        result[0],
        r#"OK #("C-f runs the command forward-char" 0 3 (font-lock-face help-key-binding face help-key-binding))"#
    );
}

#[test]
fn describe_key_briefly_loaded_insert_writes_message() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (let ((g (make-sparse-keymap)))
               (define-key g (kbd "C-f") #'forward-char)
               (use-global-map g)
               (list (describe-key-briefly (kbd "C-f") t)
                     (buffer-string)
                     (subrp (symbol-function 'describe-key-briefly)))))"#,
    );
    assert_eq!(
        result[0],
        r#"OK (nil #("C-f runs the command forward-char" 0 3 (font-lock-face help-key-binding face help-key-binding)) nil)"#
    );
}

#[test]
fn describe_key_briefly_loaded_wrong_type_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(condition-case err
               (describe-key-briefly 1)
             (error (list 'err (car err))))"#,
    );
    assert_eq!(result[0], r#"OK (err wrong-type-argument)"#);
}

// -------------------------------------------------------------------
// thing-at-point / symbol-at-point / word-at-point
// -------------------------------------------------------------------

#[test]
fn thingatpt_startup_functions_are_autoloaded() {
    crate::test_utils::init_test_tracing();
    let ev = eval_with_ldefs_boot_autoloads(&[
        "bounds-of-thing-at-point",
        "thing-at-point",
        "symbol-at-point",
    ]);

    for name in [
        "bounds-of-thing-at-point",
        "thing-at-point",
        "symbol-at-point",
    ] {
        let function = ev
            .obarray
            .symbol_function(name)
            .unwrap_or_else(|| panic!("missing startup function cell for {name}"));
        assert!(
            crate::emacs_core::autoload::is_autoload_value(&function),
            "{name} should come from GNU thingatpt.el"
        );
    }

    assert!(!ev.obarray.fboundp("word-at-point"));
}

#[test]
fn word_at_point_starts_unbound_before_thingatpt_load() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = eval_all_with(
        &mut ev,
        r#"(progn
             (get-buffer-create "word-at-point-startup")
             (set-buffer "word-at-point-startup")
             (insert "alpha beta")
             (goto-char 3)
             (word-at-point))"#,
    );
    assert_eq!(result[0], "ERR (void-function (word-at-point))");
}

#[test]
fn thingatpt_functions_load_from_gnu_elisp() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "foo bar")
             (goto-char 2)
             (list (thing-at-point 'word)
                   (symbol-name (symbol-at-point))
                   (fboundp 'word-at-point)
                   (bounds-of-thing-at-point 'word)))"#,
    );
    assert_eq!(result[0], r#"OK ("foo" "foo" t (1 . 4))"#);
}

// -------------------------------------------------------------------
// command-execute
// -------------------------------------------------------------------

#[test]
fn command_execute_lisp_ignore() {
    crate::test_utils::init_test_tracing();
    // `ignore' is lisp/subr.el:501 and has no Rust subr (DIVERGENCES.md 152),
    // so this asks the runtime that loaded `subr.el'.  GNU 31.0.90 answers
    // nil for both.
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            "(command-execute 'ignore)
             (commandp 'ignore)",
        ),
        vec!["OK nil", "OK t"],
    );
}

#[test]
fn command_execute_rejects_non_vector_keys_argument() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![Value::symbol("ignore"), Value::NIL, Value::string("a")],
        )
        .expect_err("command-execute should reject non-vector keys argument");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("vectorp"), Value::string("a")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn command_execute_accepts_vector_keys_argument() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![
                Value::symbol("ignore"),
                Value::NIL,
                Value::vector(vec![Value::fixnum(97)]),
            ],
        )
        .expect("command-execute should accept vector keys argument");
    assert!(result.is_nil());
}

#[test]
fn command_execute_does_not_record_keys_argument_in_recent_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![
                Value::symbol("ignore"),
                Value::NIL,
                Value::vector(vec![Value::fixnum(97), Value::symbol("mouse-1")]),
            ],
        )
        .expect("command-execute should accept vector keys argument");
    assert!(result.is_nil());
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn command_execute_keys_vector_keeps_this_command_keys_empty_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let _ = eval_all_with(
        &mut ev,
        "(fset 'neo-rk-loop-probe
                (lambda ()
                  (interactive)
                  (list (this-command-keys) (this-command-keys-vector))))",
    );

    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![
                Value::symbol("neo-rk-loop-probe"),
                Value::NIL,
                Value::vector(vec![Value::fixnum(97), Value::fixnum(98)]),
            ],
        )
        .expect("command-execute should accept vector keys argument");
    let output = list_to_vec(&result).expect("probe result should be list");
    assert_eq!(output, vec![Value::string(""), Value::vector(vec![])]);
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn command_execute_rejects_list_keys_argument_without_recording_recent_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let keys = Value::list(vec![Value::fixnum(97), Value::fixnum(98)]);
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![Value::symbol("ignore"), Value::NIL, keys],
        )
        .expect_err("command-execute should reject list keys argument");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("vectorp"), keys]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn command_execute_rejects_too_many_arguments() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![
                Value::symbol("ignore"),
                Value::NIL,
                Value::NIL,
                Value::NIL,
                Value::NIL,
            ],
        )
        .expect_err("command-execute should reject too many arguments");
    match result {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn command_execute_builtin_eval_expression_reads_stdin_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_with_eval_expression_eval();
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![Value::symbol("eval-expression")],
        )
        .expect_err("command-execute eval-expression should signal end-of-file in batch");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "end-of-file");
            assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn command_execute_builtin_self_insert_command_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![Value::symbol("self-insert-command")],
        )
        .expect("self-insert-command should execute");
    assert!(result.is_nil());
}

#[test]
fn command_execute_builtin_delete_char_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (command-execute 'delete-char)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"bc\"");
}

#[test]
fn call_interactively_builtin_delete_char_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (call-interactively 'delete-char)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"bc\"");
}

#[test]
fn call_interactively_rejects_non_vector_keys_argument() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_call_interactively(
        &mut ev,
        vec![Value::symbol("ignore"), Value::NIL, Value::string("b")],
    )
    .expect_err("call-interactively should reject non-vector keys argument");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("vectorp"), Value::string("b")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn call_interactively_accepts_vector_keys_argument() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let command = ev
        .eval_str("'(lambda () (interactive) nil)")
        .expect("quoted lambda command");
    let result = builtin_call_interactively(
        &mut ev,
        vec![command, Value::NIL, Value::vector(vec![Value::fixnum(98)])],
    )
    .expect("call-interactively should accept vector keys argument");
    assert!(result.is_nil());
}

#[test]
fn call_interactively_does_not_record_keys_argument_in_recent_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let command = ev
        .eval_str("'(lambda () (interactive) nil)")
        .expect("quoted lambda command");
    let result = builtin_call_interactively(
        &mut ev,
        vec![command, Value::NIL, Value::vector(vec![Value::fixnum(98)])],
    )
    .expect("call-interactively should accept vector keys argument");
    assert!(result.is_nil());
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn call_interactively_keys_vector_keeps_this_command_keys_empty_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let _ = eval_all_with(
        &mut ev,
        "(fset 'neo-rk-loop-probe
                (lambda ()
                  (interactive)
                  (list (this-command-keys) (this-command-keys-vector))))",
    );

    let result = builtin_call_interactively(
        &mut ev,
        vec![
            Value::symbol("neo-rk-loop-probe"),
            Value::NIL,
            Value::vector(vec![Value::symbol("foo")]),
        ],
    )
    .expect("call-interactively should accept vector keys argument");
    let output = list_to_vec(&result).expect("probe result should be list");
    assert_eq!(output, vec![Value::string(""), Value::vector(vec![])]);
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn call_interactively_rejects_list_keys_argument_without_recording_recent_history() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let keys = Value::list(vec![Value::fixnum(97), Value::fixnum(98)]);
    let result =
        builtin_call_interactively(&mut ev, vec![Value::symbol("ignore"), Value::NIL, keys])
            .expect_err("call-interactively should reject list keys argument");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("vectorp"), keys]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
    assert!(ev.recent_input_events().is_empty());
}

#[test]
fn call_interactively_rejects_too_many_arguments() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_call_interactively(
        &mut ev,
        vec![Value::symbol("ignore"), Value::NIL, Value::NIL, Value::NIL],
    )
    .expect_err("call-interactively should reject too many arguments");
    match result {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn command_execute_builtin_upcase_word_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc def")
             (goto-char 1)
             (command-execute 'upcase-word)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"ABC def\"");
}

#[test]
fn call_interactively_builtin_capitalize_word_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (insert "abc def")
             (goto-char 1)
             (call-interactively 'capitalize-word)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Abc def\"");
}

#[test]
fn command_execute_builtin_transpose_words_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "aa bb")
             (goto-char 1)
             (command-execute 'transpose-words)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"bb aa\"");
}

#[test]
fn command_execute_builtin_other_window_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((w1 (selected-window)))
             (split-window)
             (command-execute 'other-window)
             (not (eq (selected-window) w1)))"#,
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn call_interactively_builtin_transpose_words_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "aa bb")
             (goto-char 1)
             (call-interactively 'transpose-words)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"bb aa\"");
}

#[test]
fn call_interactively_builtin_other_window_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((w1 (selected-window)))
             (split-window)
             (call-interactively 'other-window)
             (not (eq (selected-window) w1)))"#,
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn command_execute_builtin_transpose_sexps_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "(aa) (bb)")
             (goto-char 5)
             (command-execute 'transpose-sexps)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"(bb) (aa)\"");
}

#[test]
fn call_interactively_builtin_transpose_sexps_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "(aa) (bb)")
             (goto-char 5)
             (call-interactively 'transpose-sexps)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"(bb) (aa)\"");
}

#[test]
fn command_execute_builtin_transpose_sentences_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "One.  Two.")
             (goto-char 1)
             (command-execute 'transpose-sentences)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Two.  One.\"");
}

#[test]
fn call_interactively_builtin_transpose_sentences_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "One.  Two.")
             (goto-char 1)
             (call-interactively 'transpose-sentences)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Two.  One.\"");
}

#[test]
fn command_execute_builtin_transpose_paragraphs_swaps_paragraphs() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "A\n\nB")
             (goto-char 1)
             (command-execute 'transpose-paragraphs)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"\nBA\n\"");
}

#[test]
fn command_execute_builtin_kill_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (command-execute 'kill-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"c\"");
}

#[test]
fn call_interactively_builtin_delete_region_uses_its_registered_region_spec() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abcdef")
             (goto-char 2)
             (set-mark 5)
             (call-interactively 'delete-region)
             (list (interactive-form 'delete-region)
                   (buffer-string)
                   (point)))"#,
    );
    assert_eq!(results[0], "OK ((interactive \"r\") \"aef\" 2)");
}

#[test]
fn call_interactively_eval_buffer_uses_its_registered_gnu_no_arg_spec() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq neovm--eval-buffer-interactive-result nil)
             (unwind-protect
                 (with-temp-buffer
                   (insert ";;; -*- lexical-binding: t; -*-\n"
                           "(setq neovm--eval-buffer-interactive-result 42)")
                   (list (commandp 'eval-buffer t)
                         (interactive-form 'eval-buffer)
                         (progn
                           (call-interactively 'eval-buffer)
                           neovm--eval-buffer-interactive-result)))
               (makunbound 'neovm--eval-buffer-interactive-result)))"#,
    );
    assert_eq!(results[0], "OK (t (interactive \"\") 42)");
}

#[test]
fn call_interactively_eval_region_uses_its_registered_gnu_region_spec() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
             (setq neovm--eval-region-interactive-result nil)
             (unwind-protect
                 (with-temp-buffer
                   (insert "(setq neovm--eval-region-interactive-result 73)")
                   (goto-char (point-min))
                   (set-mark (point-max))
                   (list (commandp 'eval-region t)
                         (interactive-form 'eval-region)
                         (progn
                           (call-interactively 'eval-region)
                           neovm--eval-region-interactive-result)))
               (makunbound 'neovm--eval-region-interactive-result)))"#,
    );
    assert_eq!(results[0], "OK (t (interactive \"r\") 73)");
}

#[test]
fn every_builtin_subr_command_exposes_the_interactive_form_that_defines_it() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let (missing)
             (mapatoms
              (lambda (symbol)
                (when (and (fboundp symbol)
                           (subrp (symbol-function symbol))
                           (commandp symbol t)
                           (null (interactive-form symbol)))
                  (push symbol missing))))
             (sort missing
                   (lambda (left right)
                     (string-lessp (symbol-name left)
                                   (symbol-name right)))))"#,
    );
    assert_eq!(results[0], "OK nil");
}

#[test]
fn registered_builtin_interactive_spec_is_the_command_identity_source() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.register_subr(
        SubrSpec::new(
            "neovm--typed-interactive-registration-test",
            NativeFn::ContextVec(|_ctx, _args| Ok(Value::NIL)),
            SubrArity::new(0, Some(0)),
        )
        .interactive(BuiltinInteractiveSpec::String("")),
    );
    let command = crate::emacs_core::intern::intern("neovm--typed-interactive-registration-test");
    assert!(
        ev.interactive.get_spec(command).is_none(),
        "immutable Rust subr metadata must not leak into the mutable Lisp registry"
    );

    let results = eval_all_with(
        &mut ev,
        r#"(list
             (commandp 'neovm--typed-interactive-registration-test)
             (interactive-form 'neovm--typed-interactive-registration-test))"#,
    );
    // An empty control string is what a GNU DEFUN with intspec "" answers:
    // measured, (interactive-form 'recursive-edit) => (interactive "").
    assert_eq!(results[0], "OK (t (interactive \"\"))");
}

#[test]
fn registered_builtin_alias_resolves_command_identity_without_registry_copy() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.register_subr(
        SubrSpec::new(
            "neovm--typed-interactive-target",
            NativeFn::ContextVec(|_ctx, _args| Ok(Value::NIL)),
            SubrArity::new(0, Some(0)),
        )
        .interactive(BuiltinInteractiveSpec::String("P")),
    );

    let results = eval_all_with(
        &mut ev,
        r#"(progn
             (defalias 'neovm--typed-interactive-alias
                       (symbol-function 'neovm--typed-interactive-target))
             (list
              (commandp 'neovm--typed-interactive-alias)
              (interactive-form 'neovm--typed-interactive-alias)))"#,
    );
    let alias = crate::emacs_core::intern::intern("neovm--typed-interactive-alias");
    assert!(
        ev.interactive.get_spec(alias).is_none(),
        "subr aliases must resolve immutable metadata through the function object"
    );
    assert_eq!(results[0], "OK (t (interactive \"P\"))");
}

#[test]
fn replacing_registered_builtin_with_noninteractive_lambda_removes_command_identity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let results = eval_all_with(
        &mut ev,
        r#"(progn
             (fset 'eval-buffer (lambda () nil))
             (list
              (commandp 'eval-buffer t)
              (interactive-form 'eval-buffer)))"#,
    );
    assert_eq!(results[0], "OK (nil nil)");
}

#[test]
fn captured_noninteractive_subr_does_not_inherit_replacement_symbol_command_identity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let results = eval_all_with(
        &mut ev,
        r#"(let ((old (symbol-function 'car)))
             (fset 'car '(autoload "never" nil t))
             (list
              (commandp old t)
              (interactive-form old)))"#,
    );
    assert_eq!(results[0], "OK (nil nil)");
}

#[test]
fn builtin_subr_commands_carry_their_gnu_interactive_specs() {
    crate::test_utils::init_test_tracing();

    let results = bootstrap_eval_all(
        r#"(mapcar (lambda (command)
                     (list command
                           (commandp command t)
                           (interactive-form command)))
                   '(kill-buffer widen abort-minibuffers delete-frame))"#,
    );
    assert_eq!(
        results[0],
        "OK ((kill-buffer t (interactive \"bKill buffer: \")) (widen t (interactive \"\")) (abort-minibuffers t (interactive \"\")) (delete-frame t (interactive \"\")))"
    );
}

#[test]
fn call_interactively_builtin_kill_ring_save_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (call-interactively 'kill-ring-save)
             (current-kill 0 t))"#,
    );
    assert_eq!(results[0], "OK \"ab\"");
}

#[test]
fn command_execute_builtin_copy_region_as_kill_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(let ((kill-ring nil))
             (with-temp-buffer
               (insert "abc")
               (goto-char 1)
               (set-mark 3)
               (command-execute 'copy-region-as-kill)
               (current-kill 0 t)))"#,
    );
    assert_eq!(results[0], "OK \"ab\"");
}

#[test]
fn call_interactively_builtin_copy_region_as_kill_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((kill-ring nil))
             (with-temp-buffer
               (insert "abc")
               (goto-char 1)
               (set-mark 3)
               (call-interactively 'copy-region-as-kill)
               (current-kill 0 t)))"#,
    );
    assert_eq!(results[0], "OK \"ab\"");
}

#[test]
fn call_interactively_builtin_upcase_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (call-interactively 'upcase-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"ABc\"");
}

#[test]
fn call_interactively_builtin_downcase_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "ABC")
             (goto-char 1)
             (set-mark 3)
             (call-interactively 'downcase-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"abC\"");
}

#[test]
fn call_interactively_builtin_capitalize_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (call-interactively 'capitalize-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Abc\"");
}

#[test]
fn command_execute_builtin_upcase_region_signals_args_out_of_range() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (condition-case err
                 (command-execute 'upcase-region)
               (error err)))"#,
    );
    assert_eq!(results[0], "OK (args-out-of-range \"\" 0)");
}

#[test]
fn command_execute_builtin_downcase_region_signals_args_out_of_range() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "ABC")
             (goto-char 1)
             (set-mark 3)
             (condition-case err
                 (command-execute 'downcase-region)
               (error err)))"#,
    );
    assert_eq!(results[0], "OK (args-out-of-range \"\" 0)");
}

#[test]
fn command_execute_builtin_capitalize_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (command-execute 'capitalize-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Abc\"");
}

#[test]
fn command_execute_builtin_capitalize_region_without_mark_signals_error() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (command-execute 'capitalize-region)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn call_interactively_builtin_upcase_initials_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (call-interactively 'upcase-initials-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Abc\"");
}

#[test]
fn command_execute_builtin_upcase_initials_region_uses_marked_region() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (set-mark 3)
             (command-execute 'upcase-initials-region)
             (buffer-string))"#,
    );
    assert_eq!(results[0], "OK \"Abc\"");
}

#[test]
fn command_execute_builtin_upcase_initials_region_without_mark_signals_error() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (command-execute 'upcase-initials-region)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn command_execute_builtin_kill_region_without_mark_signals_user_error() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (command-execute 'kill-region)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (user-error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn command_execute_builtin_kill_ring_save_without_mark_signals_error() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (command-execute 'kill-ring-save)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn call_interactively_builtin_kill_ring_save_without_mark_signals_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (call-interactively 'kill-ring-save)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn command_execute_builtin_copy_region_as_kill_without_mark_signals_error() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_command_execute_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (command-execute 'copy-region-as-kill)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn call_interactively_builtin_copy_region_as_kill_without_mark_signals_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 1)
             (condition-case err
                 (call-interactively 'copy-region-as-kill)
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn find_file_is_owned_by_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = gnu_files_command_eval_all(
        r#"(list
             (commandp 'find-file)
             (subrp (symbol-function 'find-file)))"#,
    );
    assert_eq!(results[0], "OK (t nil)");
}

#[test]
fn save_buffer_is_owned_by_gnu_files_el() {
    crate::test_utils::init_test_tracing();
    let results = gnu_files_command_eval_all(
        r#"(list
             (commandp 'save-buffer)
             (subrp (symbol-function 'save-buffer)))"#,
    );
    assert_eq!(results[0], "OK (t nil)");
}

#[test]
fn command_execute_builtin_set_mark_command_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");
    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![Value::symbol("set-mark-command")],
        )
        .expect("set-mark-command should execute");
    assert!(result.is_nil());
}

#[test]
fn bootstrap_command_execute_quoted_insert_uses_simple_el() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_quoted_insert_eval_all(
        r#"(list (commandp 'quoted-insert)
                 (subrp (symbol-function 'quoted-insert))
                 (condition-case err
                     (progn (command-execute 'quoted-insert) 'no-error)
                   (error (list (car err) (cadr err)))))"#,
    );
    assert_eq!(
        results[0],
        r#"OK (t nil (end-of-file "Error reading from stdin"))"#
    );
}

#[test]
fn bootstrap_command_execute_universal_argument_sets_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_universal_argument_eval_all(
        r#"(progn
             (setq prefix-arg nil)
             (command-execute 'universal-argument)
             (list (consp prefix-arg)
                   (equal prefix-arg '(4))
                   (subrp (symbol-function 'universal-argument))))"#,
    );
    assert_eq!(results[0], "OK (t t nil)");
}

#[test]
fn command_execute_calls_function() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    eval_all_with(
        &mut ev,
        r#"(defvar exec-ran nil)
           (defun test-cmd () (setq exec-ran t))"#,
    );
    ev.interactive.register_interactive(
        crate::emacs_core::intern::intern("test-cmd"),
        InteractiveSpec::no_args(),
    );

    let result = ev
        .apply(
            Value::symbol("command-execute"),
            vec![Value::symbol("test-cmd")],
        )
        .unwrap();
    assert!(result.is_truthy());

    let ran = *ev.obarray.symbol_value("exec-ran").unwrap();
    assert!(ran.is_truthy());
}

#[test]
fn interactive_form_ignores_command_modes_tail_in_closure_slot() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(let ((cmd (lambda ()
                       (interactive nil command-mode-tail-mode)
                       'ok)))
             (interactive-form cmd))"#,
    );
    assert_eq!(results[0], "OK (interactive nil)");
}

#[test]
fn command_execute_ignores_command_modes_tail_in_interactive_spec() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(progn
           (defvar command-mode-tail-ran nil)
           (defun command-mode-tail-cmd ()
             (interactive nil command-mode-tail-mode)
             (setq command-mode-tail-ran t))
           (list
            (commandp 'command-mode-tail-cmd)
            (command-execute 'command-mode-tail-cmd)
            command-mode-tail-ran))"#,
    );
    assert_eq!(results[0], "OK (t t t)");
}

#[test]
fn command_execute_non_command_signals_commandp_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let result = ev
        .apply(Value::symbol("command-execute"), vec![Value::symbol("car")])
        .expect_err("command-execute should reject non-command symbols");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("commandp"), Value::symbol("car")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn call_interactively_lisp_ignore() {
    crate::test_utils::init_test_tracing();
    // Same move as `command_execute_lisp_ignore': `ignore' is Lisp
    // (lisp/subr.el:501), so the runtime is where it can be called at all.
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            "(call-interactively 'ignore)
             (interactive-form 'ignore)",
        ),
        vec!["OK nil", "OK (interactive nil)"],
    );
}

#[test]
fn call_interactively_lambda_interactive_p_uses_current_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(let ((f (lambda (n) (interactive "p") n))
                 (current-prefix-arg '(4)))
             (call-interactively f))"#,
    );
    assert_eq!(results[0], "OK 4");
}

#[test]
fn command_execute_lambda_interactive_p_uses_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((f (lambda (n) (interactive "p") n))
                   (current-prefix-arg '(4)))
               (command-execute f))
             (let ((f (lambda (n) (interactive "p") n))
                   (current-prefix-arg '(4))
                   (prefix-arg '(5)))
               (command-execute f)))"#,
    );
    assert_eq!(results[0], "OK (1 5)");
}

#[test]
fn call_interactively_lambda_interactive_p_prefers_current_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(let ((f (lambda (n) (interactive "p") n))
                 (current-prefix-arg '(4))
                 (prefix-arg '(5)))
             (call-interactively f))"#,
    );
    assert_eq!(results[0], "OK 4");
}

#[test]
fn interactive_lambda_forms_support_p_p_and_expression_specs() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((f (lambda (arg) (interactive "P") arg))
                   (current-prefix-arg '(4)))
               (call-interactively f))
             (let ((f (lambda (arg) (interactive "P") arg))
                   (prefix-arg '(5)))
               (command-execute f))
             (call-interactively (lambda (x) (interactive (list 7)) x))
             (command-execute (lambda (x) (interactive (list 8)) x))
             (condition-case err
                 (call-interactively (lambda (x) (interactive 7) x))
               (error err)))"#,
    );
    assert_eq!(results[0], "OK ((4) (5) 7 8 (wrong-type-argument listp 7))");
}

#[test]
fn call_interactively_accepts_quoted_lambda_commands() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(let ((current-prefix-arg 3))
             (call-interactively '(lambda (n) (interactive "p") n)))"#,
    );
    assert_eq!(results[0], "OK 3");
}

#[test]
fn interactive_lambda_r_spec_reads_region_for_call_interactively() {
    crate::test_utils::init_test_tracing();
    // set-mark is defined in simple.el — need bootstrap evaluator.
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 2)
             (set-mark 3)
             (let ((f (lambda (b e) (interactive "r") (list b e))))
               (call-interactively f)))"#,
    );
    assert_eq!(results[0], "OK (2 3)");

    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (insert "abc")
             (goto-char 2)
             (condition-case err
                 (let ((f (lambda (b e) (interactive "r") (list b e))))
                   (call-interactively f))
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (error \"The mark is not set now, so there is no region\")"
    );
}

#[test]
fn interactive_lambda_s_spec_reads_prompt_and_signals_eof_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (condition-case err
                 (call-interactively (lambda (s) (interactive "sPrompt: ") s))
               (error err))
             (condition-case err
                 (command-execute (lambda (s) (interactive "sPrompt: ") s))
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK ((end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\"))"
    );
}

#[test]
fn interactive_lambda_extended_string_codes_cover_point_mark_ignored_and_key_readers() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list
             (let ((unread-command-events (list 97 98 99)))
               (with-temp-buffer
                 (insert "abcd")
                 (goto-char 3)
                 (set-mark 2)
                 (call-interactively
                  (lambda (pt mk ignored ch keys keyvec)
                    (interactive "d
m
i
c
k
K")
                    (list pt mk ignored ch keys keyvec)))))
             (let ((unread-command-events (list 97 98 99)))
               (with-temp-buffer
                 (insert "abcd")
                 (goto-char 3)
                 (set-mark 2)
                 (command-execute
                  (lambda (pt mk ignored ch keys keyvec)
                    (interactive "d
m
i
c
k
K")
                    (list pt mk ignored ch keys keyvec)))))
             (with-temp-buffer
               (insert "abc")
               (goto-char 2)
               (condition-case err
                   (call-interactively (lambda (mk) (interactive "m") mk))
                 (error err))))"#,
    );
    assert_eq!(
        results[0],
        "OK ((3 2 nil 97 \"b\" [99]) (3 2 nil 97 \"b\" [99]) (error \"The mark is not set now\"))"
    );
}

#[test]
fn interactive_lambda_extended_reader_prompt_codes_signal_eof_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (condition-case err
                 (call-interactively (lambda (x) (interactive "aFunction: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "bBuffer: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "BBuffer: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "CCommand: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "DDirectory: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "fFind file: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "FFind file: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "vVariable: ") x))
               (error err))
             (condition-case err
                 (command-execute (lambda (x) (interactive "bBuffer: ") x))
               (error err))
             (condition-case err
                 (command-execute (lambda (x) (interactive "fFind file: ") x))
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK ((end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\"))"
    );
}

#[test]
fn interactive_lambda_n_and_optional_coding_specs_follow_prefix_and_batch_behavior() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((current-prefix-arg '(4))
                   (prefix-arg nil))
               (call-interactively (lambda (n) (interactive "NNumber: ") n)))
             (let ((current-prefix-arg nil)
                   (prefix-arg '(5)))
               (command-execute (lambda (n) (interactive "NNumber: ") n)))
             (let ((current-prefix-arg nil)
                   (prefix-arg nil))
               (condition-case err
                   (call-interactively (lambda (n) (interactive "NNumber: ") n))
                 (error err)))
             (let ((current-prefix-arg nil)
                   (prefix-arg nil))
               (condition-case err
                   (command-execute (lambda (n) (interactive "NNumber: ") n))
                 (error err)))
             (let ((unread-command-events (list 97)))
               (list
                (call-interactively (lambda (c) (interactive "ZCoding: ") c))
                unread-command-events))
             (let ((unread-command-events (list 97)))
               (list
                (command-execute (lambda (c) (interactive "ZCoding: ") c))
                unread-command-events)))"#,
    );
    assert_eq!(
        results[0],
        "OK (4 5 (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (nil (97)) (nil (97)))"
    );
}

#[test]
fn interactive_lambda_m_s_x_x_and_z_specs_signal_eof_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (condition-case err
                 (call-interactively (lambda (s) (interactive "MString: ") s))
               (error err))
             (condition-case err
                 (call-interactively (lambda (s) (interactive "SSymbol: ") s))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "xExpr: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "XExpr: ") x))
               (error err))
             (condition-case err
                 (call-interactively (lambda (c) (interactive "zCoding: ") c))
               (error err))
             (condition-case err
                 (command-execute (lambda (s) (interactive "MString: ") s))
               (error err))
             (condition-case err
                 (command-execute (lambda (s) (interactive "SSymbol: ") s))
               (error err))
             (condition-case err
                 (command-execute (lambda (x) (interactive "xExpr: ") x))
               (error err))
             (condition-case err
                 (command-execute (lambda (x) (interactive "XExpr: ") x))
               (error err))
             (condition-case err
                 (command-execute (lambda (c) (interactive "zCoding: ") c))
               (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK ((end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\"))"
    );
}

#[test]
fn interactive_lambda_g_e_and_u_specs_follow_batch_behavior() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (condition-case err
                 (call-interactively (lambda (x) (interactive "GFind file: ") x))
               (error err))
             (condition-case err
                 (command-execute (lambda (x) (interactive "GFind file: ") x))
               (error err))
             (let ((unread-command-events (list 97)))
               (list
                (call-interactively (lambda (x) (interactive "U") x))
                unread-command-events))
             (let ((unread-command-events (list 97)))
               (list
                (command-execute (lambda (x) (interactive "U") x))
                unread-command-events))
             (let ((unread-command-events (list 97)))
               (condition-case err
                   (call-interactively (lambda (x) (interactive "e") x))
                 (error (list err unread-command-events))))
             (let ((unread-command-events (list 97)))
               (condition-case err
                   (command-execute (lambda (x) (interactive "e") x))
                 (error (list err unread-command-events)))))"#,
    );
    assert_eq!(
        results[0],
        "OK ((end-of-file \"Error reading from stdin\") (end-of-file \"Error reading from stdin\") (nil (97)) (nil (97)) ((error \"command must be bound to an event with parameters\") (97)) ((error \"command must be bound to an event with parameters\") (97)))"
    );
}

#[test]
fn interactive_lambda_k_k_capital_and_u_specs_match_gnu_batch_mouse_up_event_behavior() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
               (call-interactively
                (lambda (keys up) (interactive "k
U") (list keys up))))
             (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
               (call-interactively
                (lambda (keys up) (interactive "K
U") (list keys up))))
             (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
               (command-execute
                (lambda (keys up) (interactive "k
U") (list keys up))))
             (let ((unread-command-events (list '(down-mouse-1) '(mouse-1))))
               (command-execute
                (lambda (keys up) (interactive "K
U") (list keys up)))))"#,
    );
    assert_eq!(
        results[0],
        "OK (([(down-mouse-1)] [(mouse-1)]) ([(down-mouse-1)] [(mouse-1)]) ([(down-mouse-1)] [(mouse-1)]) ([(down-mouse-1)] [(mouse-1)]))"
    );
}

#[test]
fn interactive_lambda_invalid_control_letter_signals_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((r (condition-case err
                          (call-interactively (lambda (x) (interactive "q") x))
                        (error err))))
               (list (if (consp r) (car r) 'non-error)
                     (and (consp r)
                          (stringp (nth 1 r))
                          (>= (length (nth 1 r)) 22)
                          (equal (substring (nth 1 r) 0 22) "Invalid control letter"))))
             (let ((called nil)
                   (r nil))
               (setq r (condition-case err
                           (call-interactively (lambda () (interactive "q") (setq called t)))
                         (error err)))
               (list (if (consp r) (car r) 'non-error)
                     (and (consp r)
                          (stringp (nth 1 r))
                          (>= (length (nth 1 r)) 22)
                          (equal (substring (nth 1 r) 0 22) "Invalid control letter"))
                     called))
             (let ((r (condition-case err
                          (call-interactively (lambda (x) (interactive "*q") x))
                        (error err))))
               (list (if (consp r) (car r) 'non-error)
                     (and (consp r)
                          (stringp (nth 1 r))
                          (>= (length (nth 1 r)) 22)
                          (equal (substring (nth 1 r) 0 22) "Invalid control letter")))))"#,
    );
    assert_eq!(results[0], "OK ((error t) (error t nil) (error t))");
}

#[test]
fn interactive_shift_selection_prefix_sets_mark_and_mark_active() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    {
        let buf = ev.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("abcd");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(2));
    }
    ev.obarray
        .set_symbol_value("this-command-keys-shift-translated", Value::T);
    ev.obarray.set_symbol_value("shift-select-mode", Value::T);

    interactive_apply_shift_selection_prefix(&mut ev);

    let buf = ev.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.mark_emacs_byte_pos().map(|pos| pos.get()), Some(2));
    assert_eq!(buf.get_buffer_local("mark-active"), Some(Value::T));
}

#[test]
fn interactive_lambda_prefix_flags_star_hat_and_at_follow_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list
             (with-temp-buffer
               (let ((buffer-read-only nil))
                 (call-interactively (lambda () (interactive "*") 'ok))))
             (with-temp-buffer
               (let ((buffer-read-only t))
                 (condition-case err
                     (call-interactively (lambda () (interactive "*") 'ok))
                   (error (car err)))))
             (with-temp-buffer
               (let ((buffer-read-only t)
                     (inhibit-read-only t))
                 (call-interactively (lambda () (interactive "*") 'ok))))
             (with-temp-buffer
               (insert "abcd")
               (goto-char 3)
               (setq mark-active nil)
               (let ((this-command-keys-shift-translated t)
                     (shift-select-mode t))
                 (list
                  (call-interactively (lambda (pt) (interactive "^d") pt))
                  mark-active
                  (mark t))))
             (with-temp-buffer
               (insert "abcd")
               (goto-char 3)
               (setq mark-active nil)
               (let ((this-command-keys-shift-translated t)
                     (shift-select-mode t))
                 (list
                  (command-execute (lambda (pt) (interactive "^d") pt))
                  mark-active
                  (mark t))))
             (with-temp-buffer
               (insert "abcd")
               (goto-char 3)
               (set-mark 1)
               (setq mark-active nil)
               (let ((this-command-keys-shift-translated nil)
                     (shift-select-mode t))
                 (list
                  (call-interactively (lambda (pt) (interactive "^d") pt))
                  mark-active
                  (mark t))))
             (with-temp-buffer
               (insert "ab")
               (goto-char 2)
               (call-interactively (lambda (pt) (interactive "@d") pt)))
             (with-temp-buffer
               (insert "ab")
               (goto-char 2)
               (command-execute (lambda (pt) (interactive "@d") pt)))
             (with-temp-buffer
               (insert "abcd")
               (goto-char 3)
               (setq buffer-read-only t)
               (setq mark-active nil)
               (let ((this-command-keys-shift-translated t)
                     (shift-select-mode t))
                 (condition-case err
                     (progn
                       (call-interactively (lambda (x) (interactive "*^d") x))
                       (list 'ok mark-active (mark t)))
                   (error (list (car err) mark-active (mark t))))))
             (with-temp-buffer
               (insert "abcd")
               (goto-char 3)
               (setq buffer-read-only t)
               (setq mark-active nil)
               (let ((this-command-keys-shift-translated t)
                     (shift-select-mode t))
                 (condition-case err
                     (progn
                       (call-interactively (lambda (x) (interactive "^*d") x))
                       (list 'ok mark-active (mark t)))
                   (error (list (car err) mark-active (mark t)))))))"#,
    );
    // With SymbolValue::BufferLocal, setq on buffer-read-only correctly
    // creates a buffer-local binding. The "*" interactive prefix detects
    // the buffer is read-only and signals buffer-read-only, matching GNU.
    assert_eq!(
        results[0],
        "OK (ok buffer-read-only ok (3 t 3) (3 t 3) (3 nil 1) 2 2 (buffer-read-only nil nil) (buffer-read-only t 3))"
    );
}

#[test]
fn interactive_lambda_e_spec_reads_parameterized_events_from_keys_vector() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((evt (list 'mouse-1 (list (list (selected-window) (point) '(0 . 0) 0))))
                   (r nil))
               (setq r (call-interactively (lambda (x) (interactive "e") x) nil (vector evt)))
               (and (consp r) (eq (car r) 'mouse-1)))
             (let ((evt (list 'mouse-1 (list (list (selected-window) (point) '(0 . 0) 0))))
                   (r nil))
               (setq r (command-execute (lambda (x) (interactive "e") x) nil (vector evt)))
               (and (consp r) (eq (car r) 'mouse-1)))
             (let ((evt (list 'mouse-1 (list (list (selected-window) (point) '(0 . 0) 0))))
                   (r nil))
               (setq r (call-interactively (lambda (x) (interactive "e") x) nil (vector 97 evt)))
               (and (consp r) (eq (car r) 'mouse-1)))
             (equal
              (call-interactively (lambda (x) (interactive "e") x) nil (vector '(mouse-1)))
              '(mouse-1))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "e") x) nil [mouse-1])
               (error (car err)))
             (condition-case err
                 (command-execute (lambda (x) (interactive "e") x) nil [mouse-1])
               (error (car err)))
             (condition-case err
                 (call-interactively (lambda (x) (interactive "e") x) nil (vector [mouse-1]))
               (error (car err))))"#,
    );
    assert_eq!(results[0], "OK (t t t t error error error)");
}

#[test]
fn interactive_at_prefix_selects_mouse_window_from_explicit_command_keys() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let* ((w1 (selected-window))
                    (w2 (split-window-internal w1 nil 'right nil))
                    (b1 (get-buffer-create "iat-prefix-b1"))
                    (b2 (get-buffer-create "iat-prefix-b2"))
                    (evt (list 'mouse-1 (list (list w2 2 '(0 . 0) 0)))))
               (set-window-buffer w1 b1)
               (set-window-buffer w2 b2)
               (select-window w1)
               (erase-buffer)
               (insert "aaaa")
               (select-window w2)
               (erase-buffer)
               (insert "bbbbbb")
               (set-window-point w1 2)
               (set-window-point w2 5)
               (select-window w1)
               (list
                (call-interactively
                 (lambda (pt)
                   (interactive "@d")
                   (list (eq (selected-window) w2) pt (eq (current-buffer) b2)))
                 nil
                 (vector evt))
                (eq (selected-window) w2)
                (eq (current-buffer) b2)))
             (let* ((w1 (selected-window))
                    (w2 (split-window-internal w1 nil 'right nil))
                    (b1 (get-buffer-create "iat-prefix-ce-b1"))
                    (b2 (get-buffer-create "iat-prefix-ce-b2"))
                    (evt (list 'mouse-1 (list (list w2 2 '(0 . 0) 0)))))
               (set-window-buffer w1 b1)
               (set-window-buffer w2 b2)
               (select-window w1)
               (erase-buffer)
               (insert "aaaa")
               (select-window w2)
               (erase-buffer)
               (insert "bbbbbb")
               (set-window-point w1 2)
               (set-window-point w2 5)
               (select-window w1)
               (list
                (command-execute
                 (lambda (pt)
                   (interactive "@d")
                   (list (eq (selected-window) w2) pt (eq (current-buffer) b2)))
                 nil
                 (vector evt))
                (eq (selected-window) w2)
                (eq (current-buffer) b2))))"#,
    );
    assert_eq!(results[0], "OK (((t 5 t) t t) ((t 5 t) t t))");
}

#[test]
fn interactive_lambda_e_spec_uses_command_key_context_for_event_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((unread-command-events (list '(mouse-1))))
               (list
                (read-event)
                (call-interactively (lambda (x) (interactive "e") x))))
             (let ((unread-command-events (list '(mouse-1))))
               (list
                (read-event)
                (command-execute (lambda (x) (interactive "e") x))))
             (let ((unread-command-events (list 97 '(mouse-1))))
               (list
                (read-event)
                (call-interactively (lambda (x) (interactive "e") x))
                unread-command-events))
             (let ((unread-command-events (list 97 '(mouse-1))))
               (list
                (read-event)
                (command-execute (lambda (x) (interactive "e") x))
                unread-command-events)))"#,
    );
    assert_eq!(
        results[0],
        "OK (((mouse-1) (mouse-1)) ((mouse-1) (mouse-1)) (97 (mouse-1) ((mouse-1))) (97 (mouse-1) ((mouse-1))))"
    );
}

#[test]
fn interactive_lambda_e_spec_does_not_use_unread_queue_without_command_key_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((start (length (recent-keys))))
               (let ((unread-command-events (list '(mouse-1))))
                 (condition-case err
                     (call-interactively (lambda (x) (interactive "e") x))
                   (error (list (car err)
                                unread-command-events
                                (append (nthcdr start (append (recent-keys) nil)) nil))))))
             (let ((start (length (recent-keys))))
               (let ((unread-command-events (list '(mouse-1))))
                 (condition-case err
                     (command-execute (lambda (x) (interactive "e") x))
                   (error (list (car err)
                                unread-command-events
                                (append (nthcdr start (append (recent-keys) nil)) nil))))))
             (let ((start (length (recent-keys))))
               (let ((unread-command-events (list 97 '(mouse-1))))
                 (condition-case err
                     (call-interactively (lambda (x) (interactive "e") x))
                   (error (list (car err)
                                unread-command-events
                                (append (nthcdr start (append (recent-keys) nil)) nil))))))
             (let ((start (length (recent-keys))))
               (let ((unread-command-events (list 97 '(mouse-1))))
                 (condition-case err
                     (command-execute (lambda (x) (interactive "e") x))
                   (error (list (car err)
                                unread-command-events
                                (append (nthcdr start (append (recent-keys) nil)) nil)))))))"#,
    );
    assert_eq!(
        results[0],
        "OK ((error ((mouse-1)) nil) (error ((mouse-1)) nil) (error (97 (mouse-1)) nil) (error (97 (mouse-1)) nil))"
    );
}

#[test]
fn interactive_lambda_e_spec_prefers_existing_command_keys_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(list
             (let ((unread-command-events (list 97)))
               (read-key-sequence ""))
             (this-command-keys-vector)
             (let ((unread-command-events (list '(mouse-1))))
               (condition-case err
                   (call-interactively (lambda (x) (interactive "e") x))
                 (error (car err))))
             (let ((unread-command-events (list '(mouse-1))))
               (condition-case err
                   (call-interactively (lambda (x) (interactive "e") x) nil [])
                 (error (car err))))
             (call-interactively
              (lambda (x) (interactive "e") x)
              nil
              (vector '(mouse-1))))"#,
    );
    assert_eq!(results[0], "OK (\"a\" [97] error error (mouse-1))");
}

#[test]
fn call_interactively_non_command_signals_commandp_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_call_interactively(&mut ev, vec![Value::symbol("car")])
        .expect_err("call-interactively should reject non-command symbols");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("commandp"), Value::symbol("car")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn call_interactively_propagates_noninteractive_autoload_failure_before_commandp() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        "(autoload 'neovm-ci-missing-plain
                   \"neovm-ci-missing-plain-library\"
                   nil nil)",
    )
    .expect("install non-interactive autoload");

    let result = builtin_call_interactively(&mut ev, vec![Value::symbol("neovm-ci-missing-plain")])
        .expect_err("call-interactively should propagate the autoload failure");
    match result {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "file-missing"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn call_interactively_eval_expression_reads_stdin_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_eval_expression_eval();
    let result = builtin_call_interactively(&mut ev, vec![Value::symbol("eval-expression")])
        .expect_err("call-interactively eval-expression should signal end-of-file in batch");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "end-of-file");
            assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn bootstrap_read_expression_internal_map_installs_try_read_binding() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(list (lookup-key read-expression-map (kbd "RET"))
                 (lookup-key read--expression-map (kbd "RET"))
                 (lookup-key read--expression-map (kbd "C-j")))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap read--expression-map result");
    assert_eq!(
        result,
        "OK (exit-minibuffer read--expression-try-read read--expression-try-read)"
    );
}

#[test]
fn eval_expression_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_eval_expression_eval();
    let result = ev
        .apply(
            Value::symbol("eval-expression"),
            vec![
                Value::fixnum(1),
                Value::NIL,
                Value::NIL,
                Value::NIL,
                Value::NIL,
            ],
        )
        .expect_err("eval-expression should reject more than four args");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data.len(), 2);
            assert_eq!(sig.data[1], Value::fixnum(5));
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn eval_expression_apply_executes_form_argument() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");

    let expr = read_first_object(&mut ev, r#"(message "NEO-OK")"#);
    let result = ev
        .apply(
            Value::symbol("eval-expression"),
            vec![expr, Value::NIL, Value::NIL, Value::fixnum(127)],
        )
        .expect("eval-expression should evaluate message form");
    let rendered = crate::emacs_core::print::print_value(&result);
    let current_message = ev
        .apply(Value::symbol("current-message"), vec![])
        .expect("current-message should be readable after eval-expression");

    assert_eq!(
        result.as_utf8_str(),
        Some("NEO-OK"),
        "unexpected eval-expression result={rendered} current-message={:?}",
        current_message.as_utf8_str()
    );
}

#[test]
fn call_interactively_eval_expression_executes_read_expression_result() {
    crate::test_utils::init_test_tracing();
    let mut ev = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut ev).expect("runtime startup state");

    eval_all_with(
        &mut ev,
        r#"
        (defun read--expression (&rest _args) '(message "NEO-OK"))
        (defun eval-expression-get-print-arguments (&rest _args) nil)
        "#,
    );

    let result = builtin_call_interactively(&mut ev, vec![Value::symbol("eval-expression")])
        .expect("call-interactively eval-expression should succeed");
    let rendered = crate::emacs_core::print::print_value(&result);
    let current_message = ev
        .apply(Value::symbol("current-message"), vec![])
        .expect("current-message should be readable after call-interactively");

    assert_eq!(
        result.as_utf8_str(),
        Some("NEO-OK"),
        "unexpected interactive eval-expression result={rendered} current-message={:?}",
        current_message.as_utf8_str()
    );
}

#[test]
fn self_insert_command_argument_validation() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let missing = builtin_self_insert_command(&mut ev, vec![])
        .expect_err("self-insert-command should require one arg");
    match missing {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("self-insert-command"), Value::fixnum(0)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let too_many =
        builtin_self_insert_command(&mut ev, vec![Value::fixnum(1), Value::NIL, Value::NIL])
            .expect_err("self-insert-command should reject too many args");
    match too_many {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![Value::symbol("self-insert-command"), Value::fixnum(3)]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let wrong_type = builtin_self_insert_command(&mut ev, vec![Value::symbol("x")])
        .expect_err("self-insert-command should type check arg");
    match wrong_type {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::symbol("x")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let negative = builtin_self_insert_command(&mut ev, vec![Value::fixnum(-1)])
        .expect_err("self-insert-command should reject negative repetition");
    match negative {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Negative repetition argument -1")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn self_insert_command_uses_last_command_event_character() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (let ((last-command-event 97))
               (self-insert-command 2)
               (buffer-string)))"#,
    );
    assert_eq!(results[0], "OK \"aa\"");
}

#[test]
fn self_insert_command_expands_abbrev_at_word_boundary() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(define-abbrev-table 'probe-abbrev-table '(("se" "SEQUENCE")))
           (with-temp-buffer
             (setq local-abbrev-table probe-abbrev-table)
             (abbrev-mode 1)
             (insert "se")
             (let ((last-command-event ?\s))
               (self-insert-command 1))
             (buffer-string))"#,
    );
    assert_eq!(results[1], "OK \"SEQUENCE \"");
}

#[test]
fn self_insert_command_signals_read_only_without_inserting() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (insert "alpha")
             (goto-char 1)
             (setq buffer-read-only t)
             (let ((last-command-event 88))
               (list
                (condition-case err
                    (self-insert-command 1)
                  (error (car err)))
                (buffer-string))))"#,
    );
    assert_eq!(results[0], r#"OK (buffer-read-only "alpha")"#);
}

#[test]
fn self_insert_command_non_character_second_arg_beeps() {
    // GNU Emacs cmds.c: second arg C is the character to insert.
    // If C is not a character (like `t`), GNU calls bitch_at_user() (beep)
    // and does not insert anything.  Buffer remains empty.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (let ((last-command-event 97))
               (self-insert-command 2 t)
               (buffer-string)))"#,
    );
    assert_eq!(results[0], "OK \"\"");
}

#[test]
fn command_execute_self_insert_uses_last_command_event_when_available() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_command_execute_eval();
    let results = eval_all_with(
        &mut ev,
        r#"(with-temp-buffer
             (let ((last-command-event 98))
               (command-execute 'self-insert-command nil [98])
               (buffer-string)))"#,
    );
    assert_eq!(results[0], "OK \"b\"");
}

#[test]
fn keyboard_quit_signals_quit() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(condition-case err
               (command-execute 'keyboard-quit)
             (quit (cons (car err) (cdr err))))"#,
    );
    assert_eq!(results[0], "OK (quit)");
}

// -------------------------------------------------------------------
// execute-extended-command
// -------------------------------------------------------------------

#[test]
fn execute_extended_command_with_command_name() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_execute_extended_command_eval_all(
        r#"(progn
             (defun neo-eec-noargs ()
               (interactive)
               'neo-eec-noargs-ran)
             (execute-extended-command nil "neo-eec-noargs"))"#,
    );
    assert_eq!(results[0], "OK nil");
}

#[test]
fn execute_extended_command_returns_nil_and_seeds_current_prefix_arg() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_execute_extended_command_eval_all(
        r#"(progn
             (setq neo-eec-seen-vars :unset)
             (defun neo-eec-vars ()
               (interactive)
               (setq neo-eec-seen-vars (list current-prefix-arg prefix-arg))
               'neo-eec-vars-ret)
             (list
              (execute-extended-command nil "neo-eec-vars")
              neo-eec-seen-vars
              (let ((prefix-arg '(5))
                    (current-prefix-arg '(6)))
                (execute-extended-command 7 "neo-eec-vars")
                neo-eec-seen-vars)))"#,
    );
    assert_eq!(results[0], "OK (nil (nil nil) (7 nil))");
}

#[test]
fn execute_extended_command_applies_prefix_arg_for_p_and_p_specs() {
    crate::test_utils::init_test_tracing();
    let results = gnu_simple_execute_extended_command_eval_all(
        r#"(progn
             (setq neo-eec-seen-p :unset)
             (setq neo-eec-seen-P :unset)
             (defun neo-eec-p (arg)
               (interactive "p")
               (setq neo-eec-seen-p arg)
               'neo-eec-p-ret)
             (defun neo-eec-P (arg)
               (interactive "P")
               (setq neo-eec-seen-P arg)
               'neo-eec-P-ret)
             (list
              (list (execute-extended-command 7 "neo-eec-p") neo-eec-seen-p)
              (list (execute-extended-command '(4) "neo-eec-p") neo-eec-seen-p)
              (list (execute-extended-command '- "neo-eec-p") neo-eec-seen-p)
              (list (execute-extended-command 7 "neo-eec-P") neo-eec-seen-P)
              (list (execute-extended-command '(4) "neo-eec-P") neo-eec-seen-P)
              (list (execute-extended-command '- "neo-eec-P") neo-eec-seen-P)))"#,
    );
    assert_eq!(
        results[0],
        "OK ((nil 7) (nil 4) (nil -1) (nil 7) (nil (4)) (nil -))"
    );
}

#[test]
fn execute_extended_command_no_name_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_execute_extended_command_eval();
    let result = ev
        .apply(Value::symbol("execute-extended-command"), vec![Value::NIL])
        .expect_err("execute-extended-command should signal end-of-file in batch");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "end-of-file");
            assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn execute_extended_command_rejects_symbol_name_payload() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_execute_extended_command_eval();
    let result = ev
        .apply(
            Value::symbol("execute-extended-command"),
            vec![Value::NIL, Value::symbol("ignore")],
        )
        .expect_err("symbol payload should not be accepted as a command name");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "\u{2018}ignore\u{2019} is not a valid command name"
                )]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn execute_extended_command_rejects_non_command_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_execute_extended_command_eval();
    let result = ev
        .apply(
            Value::symbol("execute-extended-command"),
            vec![Value::NIL, Value::string("car")],
        )
        .expect_err("non-command names should be rejected");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "\u{2018}car\u{2019} is not a valid command name"
                )]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn execute_extended_command_rejects_non_string_name_payload() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_execute_extended_command_eval();
    let result = ev
        .apply(
            Value::symbol("execute-extended-command"),
            vec![Value::NIL, Value::fixnum(1)],
        )
        .expect_err("non-string command names should be rejected");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "\u{2018}1\u{2019} is not a valid command name"
                )]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn execute_extended_command_rejects_overflow_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = gnu_simple_execute_extended_command_eval();
    let result = ev
        .apply(
            Value::symbol("execute-extended-command"),
            vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL],
        )
        .expect_err("execute-extended-command should reject more than three arguments");
    match result {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn define_key_accepts_optional_remove_arg() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (eq (define-key m "a" 'ignore t) 'ignore))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn map_keymap_make_keymap_reports_char_table_bindings() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-keymap))
                 (seen nil))
             (define-key m "\C-x" 'Control-X-prefix)
             (define-key m "\C-z" 'suspend-emacs)
             (map-keymap
              (lambda (key binding)
                (when (memq key '(24 26))
                  (setq seen (cons (list key binding) seen))))
              m)
             (nreverse seen))"#,
    );
    assert_eq!(result, "OK ((24 Control-X-prefix) (26 suspend-emacs))");
}

// -------------------------------------------------------------------
// where-is-internal
// -------------------------------------------------------------------

#[test]
fn where_is_internal_finds_binding_in_explicit_map() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "a" 'ignore)
             (equal (car (where-is-internal 'ignore m)) [97]))"#,
    );
    assert_eq!(result, "OK t");
}

// -------------------------------------------------------------------
// where-is-internal: GNU's `:advertised-binding' short circuit
//
// GNU `Fwhere_is_internal' (src/keymap.c:2669-2684) consults the symbol's
// `:advertised-binding' property BEFORE it runs the reverse search, whenever
// FIRSTONLY is non-nil, and returns the advertised sequence VERBATIM if that
// sequence still resolves to DEFINITION in the same keymaps.
//
// This is not a curiosity: `lisp/bindings.el:1331-1334' binds
// `set-mark-command' to BOTH `C-@' (ASCII NUL, 0) and `C-SPC' (32 with the
// 2**26 control bit, 67108896) and advertises the second, which is the only
// reason GNU renders `\\[set-mark-command]' as `C-SPC' instead of `C-@'.
//
// Every expectation below is the answer measured from GNU Emacs 31.0.90
// (tmp/l185-adv-probe.el), not a reading of the C.
// -------------------------------------------------------------------

#[test]
fn where_is_internal_advertised_binding_outranks_the_reverse_search_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU: [67108896] -- the advertised C-SPC, not the C-@ the search finds.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "\C-@" 'l185-adv-a)
             (define-key m [67108896] 'l185-adv-a)
             (put 'l185-adv-a :advertised-binding [67108896])
             (where-is-internal 'l185-adv-a m t))"#,
    );
    assert_eq!(result, "OK [67108896]");
}

#[test]
fn where_is_internal_advertised_binding_applies_to_non_ascii_firstonly_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's guard is `!NILP (firstonly)', so the `non-ascii' spelling takes
    // the short circuit too.  GNU: [67108896].
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "\C-@" 'l185-adv-na)
             (define-key m [67108896] 'l185-adv-na)
             (put 'l185-adv-na :advertised-binding [67108896])
             (where-is-internal 'l185-adv-na m 'non-ascii))"#,
    );
    assert_eq!(result, "OK [67108896]");
}

#[test]
fn where_is_internal_advertised_binding_is_ignored_when_it_no_longer_resolves_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU verifies the advertised sequence through `shadow_lookup' and falls
    // back to the reverse search when it does not answer DEFINITION.
    // GNU: [120].
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "x" 'l185-adv-b)
             (put 'l185-adv-b :advertised-binding [?y])
             (where-is-internal 'l185-adv-b m t))"#,
    );
    assert_eq!(result, "OK [120]");
}

#[test]
fn where_is_internal_advertised_binding_pointing_at_another_command_is_ignored_like_gnu() {
    crate::test_utils::init_test_tracing();
    // The advertised key is bound, but to a DIFFERENT command; GNU's `EQ'
    // check rejects it.  GNU: [102].
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "f" 'l185-adv-f)
             (define-key m "g" 'l185-adv-g)
             (put 'l185-adv-f :advertised-binding [?g])
             (where-is-internal 'l185-adv-f m t))"#,
    );
    assert_eq!(result, "OK [102]");
}

#[test]
fn where_is_internal_advertised_binding_list_takes_the_first_that_resolves_like_gnu() {
    crate::test_utils::init_test_tracing();
    // A LIST property is walked in order and the first element that resolves
    // to DEFINITION wins -- [?z] is unbound, so [?q] answers.  GNU: [113].
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "p" 'l185-adv-c)
             (define-key m "q" 'l185-adv-c)
             (put 'l185-adv-c :advertised-binding (list [?z] [?q] [?p]))
             (where-is-internal 'l185-adv-c m t))"#,
    );
    assert_eq!(result, "OK [113]");
}

#[test]
fn where_is_internal_advertised_binding_list_that_matches_nothing_signals_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's walk offers the exhausted list's nil TAIL to `shadow_lookup' as
    // one more candidate, and `Flookup_key' rejects nil as a key sequence.
    // That is GNU's real, measured answer -- not a fall-back to the reverse
    // search.  GNU: (wrong-type-argument arrayp nil).
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "d" 'l185-adv-d)
             (put 'l185-adv-d :advertised-binding (list [?z] [?y]))
             (condition-case e (where-is-internal 'l185-adv-d m t) (error e)))"#,
    );
    assert_eq!(result, "OK (wrong-type-argument arrayp nil)");
}

#[test]
fn where_is_internal_advertised_binding_is_returned_verbatim_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU returns the property object itself, so a STRING property yields a
    // string where the reverse search would have yielded a vector.
    // GNU: "m" and t.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "m" 'l185-adv-e)
             (put 'l185-adv-e :advertised-binding "m")
             (list (where-is-internal 'l185-adv-e m t)
                   (stringp (where-is-internal 'l185-adv-e m t))))"#,
    );
    assert_eq!(result, "OK (\"m\" t)");
}

#[test]
fn where_is_internal_advertised_binding_is_read_from_the_remap_target_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU substitutes the remap target into DEFINITION first, so the property
    // consulted belongs to the target and not to the asked-for command.
    // GNU: [67108896].
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "i" 'l185-adv-i)
             (define-key m [67108896] 'l185-adv-i)
             (define-key m [remap l185-adv-h] 'l185-adv-i)
             (put 'l185-adv-i :advertised-binding [67108896])
             (where-is-internal 'l185-adv-h m t))"#,
    );
    assert_eq!(result, "OK [67108896]");
}

#[test]
fn where_is_internal_without_firstonly_still_lists_every_binding_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's guard is `!NILP (firstonly)': with FIRSTONLY nil the property is
    // not consulted at all and the full list is returned.
    // GNU: ([67108896] [0]).
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "\C-@" 'l185-adv-all)
             (define-key m [67108896] 'l185-adv-all)
             (put 'l185-adv-all :advertised-binding [67108896])
             (where-is-internal 'l185-adv-all m))"#,
    );
    assert_eq!(result, "OK ([67108896] [0])");
}

#[test]
fn set_mark_command_advertises_c_spc_like_gnu() {
    crate::test_utils::init_test_tracing();
    // The name entry 178 handed over, replayed at unit scale: this is exactly
    // what `lisp/bindings.el:1331-1334' does, and that file is byte-identical
    // in this tree, so the only missing piece was `Fwhere_is_internal'
    // reading the property.  (The bootstrapped end of it -- the real
    // `global-map' and the `\\[set-mark-command]' docstring -- is pinned in
    // the oracle suite, which runs GNU beside this port.)  GNU: [67108896].
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "\C-@" 'set-mark-command)
             (define-key m [67108896] 'set-mark-command)
             (put 'set-mark-command :advertised-binding [67108896])
             (where-is-internal 'set-mark-command m t))"#,
    );
    assert_eq!(result, "OK [67108896]");
}

#[test]
fn where_is_internal_filters_non_key_events_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU keymap.c:2745-2750 drops a one-element sequence naming a symbol with
    // a non-nil `non-key-event' property -- `lisp/keymap.el:798's
    // `make-non-key-event', used by `term/ns-win.el' for `ns-power-off' and
    // friends.  It is delivered through a keymap but cannot be typed, so it is
    // not a way to run the command.  GNU: (([107]) [107]).
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (put 'l185-nk-sig 'non-key-event t)
             (define-key m [l185-nk-sig] 'l185-nk-cmd)
             (define-key m "k" 'l185-nk-cmd)
             (list (where-is-internal 'l185-nk-cmd m)
                   (where-is-internal 'l185-nk-cmd m t)))"#,
    );
    assert_eq!(result, "OK (([107]) [107])");
}

#[test]
fn where_is_internal_reports_nothing_when_only_a_non_key_event_is_bound_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU: (nil nil) -- the filtered sequence is not a fallback either.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (put 'l185-nk-only 'non-key-event t)
             (define-key m [l185-nk-only] 'l185-nk-cmd2)
             (list (where-is-internal 'l185-nk-cmd2 m)
                   (where-is-internal 'l185-nk-cmd2 m t)))"#,
    );
    assert_eq!(result, "OK (nil nil)");
}

#[test]
fn where_is_internal_non_ascii_bypasses_the_non_key_event_filter_like_gnu() {
    crate::test_utils::init_test_tracing();
    // The asymmetry is GNU's, and it is structural rather than intentional:
    // the `firstonly = non-ascii' early return (keymap.c:2756-2757) sits
    // OUTSIDE the filter, so it hands back the sequence the filter had just
    // rejected.  Measured, because no reading of the docstring predicts it.
    // GNU: (nil nil [l185-nk-na]).
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (put 'l185-nk-na 'non-key-event t)
             (define-key m [l185-nk-na] 'l185-nk-cmd3)
             (list (where-is-internal 'l185-nk-cmd3 m)
                   (where-is-internal 'l185-nk-cmd3 m t)
                   (where-is-internal 'l185-nk-cmd3 m 'non-ascii)))"#,
    );
    assert_eq!(result, "OK (nil nil [l185-nk-na])");
}

#[test]
fn where_is_internal_reuses_one_reverse_index_for_many_definitions() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    let results = eval_all_with(
        &mut eval,
        r#"(let ((m (make-sparse-keymap)))
              (define-key m "a" 'first-command)
              (define-key m "b" 'second-command)
              (list (where-is-internal 'first-command (list m) t)
                    (where-is-internal 'second-command (list m) t)))"#,
    );

    assert_eq!(results, ["OK ([97] [98])"]);
    assert_eq!(eval.interactive.where_is_reverse_index_build_count(), 1);
}

/// GNU's `Fwhere_is_internal` reads C globals such as
/// `Vminor_mode_map_alist` and predeclared property identities such as
/// `Qkeymap` directly while collecting the active maps.  M-x affixation calls
/// this path once per command candidate, so a steady-state lookup must not
/// translate those fixed identities from Rust strings on every call.
#[test]
fn where_is_internal_active_map_collection_uses_predeclared_symbols() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    eval.eval_str(
        r#"(progn
              (defalias 'test-predeclared-where-is-command (lambda () (interactive)))
              (let ((map (make-sparse-keymap)))
                (define-key map "a" 'test-predeclared-where-is-command)
                (use-global-map map)))"#,
    )
    .expect("install active global keymap");

    let command = Value::symbol("test-predeclared-where-is-command");
    let args = vec![command, Value::NIL, Value::T];
    let warm = builtin_where_is_internal(&mut eval, args.clone()).expect("warm where-is");
    assert_eq!(warm, Value::vector(vec![Value::fixnum(b'a'.into())]));

    crate::emacs_core::intern::reset_intern_calls();
    let repeated = builtin_where_is_internal(&mut eval, args).expect("repeat where-is");
    assert_eq!(repeated, warm);

    const GNU_PREDECLARED_KEYMAP_IDENTITIES: &[&str] = &[
        "emulation-mode-map-alists",
        "minor-mode-overriding-map-alist",
        "minor-mode-map-alist",
        "overriding-local-map",
        "overriding-terminal-local-map",
        "keymap",
        "local-map",
        "meta-prefix-char",
    ];
    let interned = crate::emacs_core::intern::intern_call_names();
    let repeated_fixed_names = interned
        .iter()
        .filter(|name| GNU_PREDECLARED_KEYMAP_IDENTITIES.contains(&name.as_str()))
        .collect::<Vec<_>>();
    assert!(
        repeated_fixed_names.is_empty(),
        "steady-state where-is must use GNU-shaped predeclared identities; interned {repeated_fixed_names:?}"
    );
}

#[test]
fn where_is_internal_answers_many_command_remappings_from_reverse_index() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    let results = eval_all_with(
        &mut eval,
        r##"(let ((m (make-sparse-keymap)))
              (define-key m [remap first-command] 'first-target)
              (define-key m [remap second-command] 'second-target)
              (define-key m "a" 'first-target)
              (define-key m "b" 'second-target)
              (list (where-is-internal 'first-command (list m) t)
                    (where-is-internal 'second-command (list m) t)))"##,
    );

    assert_eq!(results, ["OK ([97] [98])"]);
    assert_eq!(eval.interactive.where_is_reverse_index_build_count(), 1);
    assert_eq!(
        eval.interactive
            .where_is_reverse_index_remapping_lookup_count(),
        2
    );
}

#[test]
fn where_is_internal_rebuilds_reverse_index_after_keymap_mutation() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    let results = eval_all_with(
        &mut eval,
        r#"(let ((m (make-sparse-keymap)))
              (define-key m "a" 'first-command)
              (where-is-internal 'first-command (list m) t)
              (define-key m "b" 'second-command)
              (list (where-is-internal 'first-command (list m) t)
                    (where-is-internal 'second-command (list m) t)))"#,
    );

    assert_eq!(results, ["OK ([97] [98])"]);
    assert_eq!(eval.interactive.where_is_reverse_index_build_count(), 2);
}

#[test]
fn where_is_internal_rebuilds_indexed_remapping_after_keymap_mutation() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    let results = eval_all_with(
        &mut eval,
        r##"(let ((m (make-sparse-keymap)))
              (define-key m [remap source-command] 'first-target)
              (define-key m "a" 'first-target)
              (where-is-internal 'source-command (list m) t)
              (define-key m [remap source-command] 'second-target)
              (define-key m "b" 'second-target)
              (where-is-internal 'source-command (list m) t))"##,
    );

    assert_eq!(results, ["OK [98]"]);
    assert_eq!(eval.interactive.where_is_reverse_index_build_count(), 2);
}

#[test]
fn where_is_internal_does_not_reuse_reverse_index_for_different_keymaps() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    let results = eval_all_with(
        &mut eval,
        r#"(let ((first-map (make-sparse-keymap))
                  (second-map (make-sparse-keymap)))
              (define-key first-map "a" 'shared-command)
              (define-key second-map "b" 'shared-command)
              (list (where-is-internal 'shared-command (list first-map) t)
                    (where-is-internal 'shared-command (list second-map) t)))"#,
    );

    assert_eq!(results, ["OK ([97] [98])"]);
    assert_eq!(eval.interactive.where_is_reverse_index_build_count(), 2);
}

#[test]
fn where_is_internal_uncached_mode_clears_reverse_index_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_interactive_shims();
    let results = eval_all_with(
        &mut eval,
        r#"(let ((m (make-sparse-keymap)))
              (define-key m "a" 'shared-command)
              (where-is-internal 'shared-command (list m) t)
              (where-is-internal 'shared-command (list m))
              (where-is-internal 'shared-command (list m) t))"#,
    );

    assert_eq!(results, ["OK [97]"]);
    assert_eq!(eval.interactive.where_is_reverse_index_build_count(), 2);
}

#[test]
fn where_is_internal_does_not_duplicate_nested_sparse_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "\C-x\C-f" 'find-file)
             (define-key m "\C-x\C-s" 'save-buffer)
             (list (where-is-internal 'find-file m)
                   (where-is-internal 'save-buffer m)))"#,
    );
    assert_eq!(result, "OK (([24 6]) ([24 19]))");
}

#[test]
fn where_is_internal_first_only_returns_vector() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "a" 'ignore)
             (equal (where-is-internal 'ignore m t) [97]))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_accepts_list_of_keymaps() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m1 (make-sparse-keymap))
                 (m2 (make-sparse-keymap)))
             (define-key m1 "a" 'ignore)
             (define-key m2 "b" 'ignore)
             (list (equal (where-is-internal 'ignore (list m2 m1) t) [98])
                   (equal (where-is-internal 'ignore (list m2 m1))
                          '([98] [97]))))"#,
    );
    assert_eq!(result, "OK (t t)");
}

#[test]
fn where_is_internal_skips_menu_item_disabled_by_filter() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "\r"
               '(menu-item "" ignore :filter (lambda (_command) nil)))
             (define-key m "\M-\r" 'ignore)
             (key-description (where-is-internal 'ignore (list m) t)))"#,
    );
    assert_eq!(result, r#"OK "M-RET""#);
}

#[test]
fn where_is_internal_keymap_type_errors() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err
               (where-is-internal 'ignore 'not-a-map)
             (error err))"#,
    );
    assert_eq!(result, "OK (wrong-type-argument keymapp not-a-map)");
}

#[test]
fn where_is_internal_non_definition_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = eval_one("(where-is-internal 1)");
    assert_eq!(result, "OK nil");
}

#[test]
fn where_is_internal_follows_symbol_function_prefix_maps_like_gnu_help_command() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-keymap))
                 (prefix (make-sparse-keymap)))
             (define-key prefix [1] 'about-emacs)
             (fset 'test-where-is-prefix prefix)
             (define-key m [8] 'test-where-is-prefix)
             (list (keymapp (symbol-function 'test-where-is-prefix))
                   (equal (where-is-internal 'about-emacs prefix t) [1])
                   (equal (where-is-internal 'about-emacs m t) [8 1])
                   (equal (key-description (where-is-internal 'about-emacs m t))
                          "C-h C-a")))"#,
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn where_is_internal_follows_menu_item_commands_and_prefix_keymaps() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap))
                 (file-menu (make-sparse-keymap)))
             (define-key file-menu [new-file]
                         '(menu-item "Visit New File..." find-file))
             (define-key m [file]
                         (list 'menu-item "File" file-menu))
             (equal (where-is-internal 'find-file m t) [file new-file]))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_follows_menu_bar_label_prefix_keymaps() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap))
                 (file-menu (make-sparse-keymap)))
             (define-key file-menu [new-file]
                         '(menu-item "Visit New File..." find-file))
             (define-key m [file] (cons "File" file-menu))
             (equal (where-is-internal 'find-file m t) [file new-file]))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_reduces_old_style_string_menu_item_leaf() {
    // Doom's leader binds commands as `(MENU-STRING . COMMAND)` leaves, e.g.
    // `("Recent files" . recentf-open-files)` (produced by general.el `:desc`).
    // Forward `lookup-key` strips the label via `get_keyelt`; the reverse
    // `where-is-internal` scan must reduce it too, or the dashboard leader hint
    // `SPC f r` never resolves (issue #164).
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "r" '("Recent files" . find-file))
             (list (eq (lookup-key m "r") 'find-file)
                   (equal (where-is-internal 'find-file m t) [?r])))"#,
    );
    assert_eq!(result, "OK (t t)");
}

#[test]
fn where_is_internal_reduces_string_menu_item_under_symbol_prefix_like_doom_leader() {
    // Faithful reproduction of the #164 keymap shape: SPC -> a prefix-command
    // SYMBOL (like `doom/leader`, whose function cell is a keymap) whose nested
    // map holds an old-style `(STRING . COMMAND)` leaf. Both indirections must
    // be followed. (`fset` stands in for `define-prefix-command`, which is a
    // lisp-level function absent from the bare unit-test evaluator.)
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((host (make-sparse-keymap))
                 (leader-map (make-sparse-keymap))
                 (file-map (make-sparse-keymap)))
             (define-key file-map "r" '("Recent files" . recentf-open-files))
             (define-key leader-map "f" file-map)
             (fset 'test-doom-leader leader-map)
             (define-key host " " 'test-doom-leader)
             (equal (where-is-internal 'recentf-open-files (list host) t)
                    [?\s ?f ?r]))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_finds_bindings_stored_in_an_inline_vector() {
    // A keymap element may be an inline vector indexing bindings by char code
    // (GNU `map_keymap_internal`). The reverse where-is scan lacked that arm, so
    // such a command was unreachable by `where-is-internal` although
    // `lookup-key` found it -- found by probing the element taxonomy, not by a
    // bug report.
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let* ((v (make-vector 128 nil))
                  (sub (list 'keymap v))
                  (m (make-sparse-keymap)))
             (aset v ?a 'vec-cmd)
             (define-key m "p" sub)
             (list (eq (lookup-key m "pa") 'vec-cmd)
                   (equal (where-is-internal 'vec-cmd (list sub) t) [?a])
                   (equal (where-is-internal 'vec-cmd (list m) t) [?p ?a])))"#,
    );
    assert_eq!(result, "OK (t t t)");
}

#[test]
fn where_is_internal_descends_into_composed_keymaps() {
    // evil/general build active state keymaps with `make-composed-keymap`. The
    // reverse where-is scan must descend into a composed keymap's inline submaps
    // (not merely its parent), or a leader binding reachable only through a
    // composed submap stays invisible -- the real cause of the missing Doom
    // dashboard `SPC f r` hints (issue #164). Combines with the `(STRING . cmd)`
    // leaf reduction and the symbol-prefix descent above.
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((leader (make-sparse-keymap))
                 (file-map (make-sparse-keymap))
                 (inner (make-sparse-keymap)))
             (define-key file-map "r" '("Recent files" . recentf-open-files))
             (define-key leader "f" file-map)
             (fset 'test-doom-leader-2 leader)
             (define-key inner " " 'test-doom-leader-2)
             ;; `(keymap SUBMAP...)` is exactly what `make-composed-keymap`
             ;; builds (that helper is a lisp-level function absent here).
             (let ((host (list 'keymap inner)))
               (equal (where-is-internal 'recentf-open-files (list host) t)
                      [?\s ?f ?r])))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_descends_string_labelled_symbol_prefix_like_evil_leader() {
    // The exact shape evil stores for Doom's leader in its state aux keymaps:
    // `(?\s "<leader>" . doom/leader)`, i.e. SPC's binding is
    // `("<leader>" . doom/leader)` -- a `(STRING . SYMBOL)` label wrapping a
    // symbol prefix command. Descent must reduce the label (get_keyelt) THEN
    // resolve the symbol to its keymap, mirroring GNU `accessible_keymaps_1`.
    // This is the actual #164 dashboard blocker.
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((leader (make-sparse-keymap))
                 (file-map (make-sparse-keymap))
                 (aux (make-sparse-keymap)))
             (define-key file-map "r" '("Recent files" . recentf-open-files))
             (define-key leader "f" file-map)
             (fset 'test-evil-leader leader)
             (define-key aux " " '("<leader>" . test-evil-leader))
             (list (equal (where-is-internal 'recentf-open-files (list aux) t)
                          [?\s ?f ?r])
                   ;; and through a composed wrapper, as the active maps arrive
                   (equal (where-is-internal 'recentf-open-files (list (list 'keymap aux)) t)
                          [?\s ?f ?r])))"#,
    );
    assert_eq!(result, "OK (t t)");
}

#[test]
fn where_is_internal_noindirect_does_not_reduce_string_menu_item() {
    // GNU 4th arg NOINDIRECT: with it non-nil, the `(STRING . DEFN)` wrapper is
    // NOT stripped, so a search for the underlying command finds nothing.
    // Search a `(list m)` (only these maps, no global fallback) with a command
    // bound solely through the label wrapper, so the result isolates NOINDIRECT.
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(progn
             (defun test-where-is-noindirect-cmd () (interactive))
             (let ((m (make-sparse-keymap)))
               (define-key m "x" '("Some label" . test-where-is-noindirect-cmd))
               (list (equal (where-is-internal 'test-where-is-noindirect-cmd (list m) t) [?x])
                     (where-is-internal 'test-where-is-noindirect-cmd (list m) t t))))"#,
    );
    assert_eq!(result, "OK (t nil)");
}

#[test]
fn where_is_internal_firstonly_rejects_menu_bar_bindings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap))
                 (file-menu (make-sparse-keymap)))
             (defun test-where-is-menu-command ())
             (define-key file-menu [new-file]
                         '(menu-item "Visit New File..." test-where-is-menu-command))
             (define-key m [menu-bar file]
                         (list 'menu-item "File" file-menu))
             (list (where-is-internal 'test-where-is-menu-command m t)
                   (where-is-internal 'test-where-is-menu-command m)
                   (where-is-internal 'test-where-is-menu-command m 'non-ascii)))"#,
    );
    assert_eq!(
        result,
        "OK (nil ([menu-bar file new-file]) [menu-bar file new-file])"
    );
}

#[test]
fn where_is_internal_returns_prefix_key_for_prefix_command_symbol() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-keymap))
                 (prefix (make-sparse-keymap)))
             (fset 'test-where-is-prefix-command prefix)
             (define-key m [8] 'test-where-is-prefix-command)
             (equal (where-is-internal 'test-where-is-prefix-command m t) [8]))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_preserves_map_iteration_order_for_first_match() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "H" 'ignore)
             (define-key m "?" 'ignore)
             (define-key m "h" 'ignore)
             (equal (where-is-internal 'ignore m t) [104]))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_firstonly_prefers_unmodified_printable_key() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((m (define-keymap
                      "q" #'ignore
                      "C-c C-k" #'ignore)))
             (equal (where-is-internal 'ignore m t) [113]))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap where-is preferred key result");
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_firstonly_non_ascii_uses_gnu_accessible_order() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((m (define-keymap
                      "q" #'ignore
                      "C-c C-k" #'ignore)))
             (equal (where-is-internal 'ignore m 'non-ascii) [113]))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap where-is GNU accessible-map order result");
    assert_eq!(result, "OK t");
}

#[test]
fn where_is_internal_firstonly_uses_where_is_preferred_modifier() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((m (define-keymap
                      "M-q" #'ignore
                      "C-c C-k" #'ignore))
                 (where-is-preferred-modifier 'control))
             (equal (key-description (where-is-internal 'ignore m t)) "C-c C-k"))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap where-is preferred modifier result");
    assert_eq!(result, "OK t");
}

/// GNU snapshots `Vwhere_is_preferred_modifier` once at
/// `Fwhere_is_internal` entry, then passes the parsed C modifier mask through
/// every candidate comparison.  Candidate selection must not return to the
/// Lisp variable name (and therefore to the global interner) per sequence.
#[test]
fn where_is_preference_reuses_one_predeclared_snapshot() {
    crate::test_utils::init_test_tracing();
    let mut obarray = test_ob();
    obarray.set_symbol_value("where-is-preferred-modifier", Value::symbol("control"));
    let sequences = vec![
        vec![Value::fixnum(KEY_CHAR_META | i64::from(b'q'))],
        vec![Value::fixnum(KEY_CHAR_CTRL | i64::from(b'c'))],
    ];

    let preferred_modifier = WhereIsPreferredModifier::snapshot(&obarray);
    crate::emacs_core::intern::reset_intern_calls();
    let selected = select_where_is_preferred_sequence(preferred_modifier, &sequences);

    assert_eq!(selected, &sequences[1]);
    assert_eq!(
        crate::emacs_core::intern::intern_calls(),
        0,
        "candidate selection must use one predeclared preferred-modifier snapshot"
    );
}

/// `where-is-preferred-modifier` uses GNU `parse_solitary_modifier`, whose
/// symbol domain includes the customary one-letter spellings as well as the
/// long names.  Keep that accepted domain explicit when parsing the snapshot.
#[test]
fn where_is_preferred_modifier_accepts_gnu_solitary_names() {
    crate::test_utils::init_test_tracing();
    let mut obarray = test_ob();
    for (name, expected) in [
        ("C", KEY_CHAR_CTRL),
        ("ctrl", KEY_CHAR_CTRL),
        ("control", KEY_CHAR_CTRL),
        ("M", KEY_CHAR_META),
        ("meta", KEY_CHAR_META),
        ("S", KEY_CHAR_SHIFT),
        ("shift", KEY_CHAR_SHIFT),
        ("s", KEY_CHAR_SUPER),
        ("super", KEY_CHAR_SUPER),
        ("H", KEY_CHAR_HYPER),
        ("hyper", KEY_CHAR_HYPER),
        ("A", KEY_CHAR_ALT),
        ("alt", KEY_CHAR_ALT),
    ] {
        obarray.set_symbol_value("where-is-preferred-modifier", Value::symbol(name));
        assert_eq!(
            WhereIsPreferredModifier::snapshot(&obarray).mask(),
            expected,
            "GNU solitary modifier spelling {name}"
        );
    }
}

// `where-is-internal` command-remapping handling, mirroring GNU keymap.c
// `Fwhere_is_internal`: (a) a DEFINITION that is itself remapped resolves to its
// remap target's keys, (b) keys for commands remapped TO DEFINITION are folded
// in via `[remap COMMAND]` expansion, and (c) the raw `[remap ...]` pseudo-key
// never leaks into the returned key list.

#[test]
fn where_is_internal_resolves_command_remapping_to_target_keys() {
    crate::test_utils::init_test_tracing();
    // forward-char is remapped to next-line, which is bound to "n" (110).
    // GNU: forward-char's keys are next-line's keys, and next-line itself also
    // reports the same keys; the raw [remap forward-char] is never returned.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m [remap forward-char] 'next-line)
             (define-key m "n" 'next-line)
             (list (where-is-internal 'forward-char (list m) t)
                   (where-is-internal 'next-line (list m))
                   (where-is-internal 'forward-char (list m))))"#,
    );
    assert_eq!(result, "OK ([110] ([110]) ([110]))");
}

#[test]
fn where_is_internal_remap_target_key_sequence_is_resolved() {
    crate::test_utils::init_test_tracing();
    // forward-char remapped to `my`, which is bound to the multi-event key
    // sequence C-c z ([3 122]).  where-is for forward-char must report [3 122],
    // not the raw [remap forward-char] pseudo-key.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m [3 122] 'my)
             (define-key m [remap forward-char] 'my)
             (list (where-is-internal 'forward-char (list m) t)
                   (where-is-internal 'forward-char (list m))))"#,
    );
    assert_eq!(result, "OK ([3 122] ([3 122]))");
}

#[test]
fn where_is_internal_remapped_current_active_maps_preserve_active_map_order() {
    crate::test_utils::init_test_tracing();
    // Doom's dashboard asks `(where-is-internal 'bookmark-jump nil t)`.
    // `bookmark-jump` is remapped to `consult-bookmark`, but the visible
    // binding still comes from the earlier active minor/emulation map.  GNU
    // preserves that active-map order when expanding remapped sequences.
    let result = eval_one(
        r#"(let ((global-map (make-sparse-keymap))
                 (minor-map (make-sparse-keymap))
                 (minor-mode-map-alist nil)
                 (demo-mode t))
             (use-global-map global-map)
             (define-key minor-map [32 13] 'bookmark-jump)
             (define-key global-map [24 114 98] 'bookmark-jump)
             (define-key global-map [remap bookmark-jump] 'consult-bookmark)
             (setq minor-mode-map-alist (list (cons 'demo-mode minor-map)))
             (list (key-description (where-is-internal 'bookmark-jump nil t))
                   (key-description (where-is-internal 'consult-bookmark nil t))
                   (key-description (where-is-internal 'bookmark-jump nil t nil t))))"#,
    );
    assert_eq!(result, r#"OK ("SPC RET" "SPC RET" "SPC RET")"#);
}

#[test]
fn where_is_internal_filters_raw_remap_pseudo_key_from_results() {
    crate::test_utils::init_test_tracing();
    // next-line is bound to "n", and forward-char is remapped to next-line.
    // where-is for next-line must list only the real key [110]; the raw
    // [remap forward-char] pseudo-key must be filtered out.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m [remap forward-char] 'next-line)
             (define-key m "n" 'next-line)
             (where-is-internal 'next-line (list m)))"#,
    );
    assert_eq!(result, "OK ([110])");
}

#[test]
fn where_is_internal_no_remap_keeps_raw_remap_pseudo_key() {
    crate::test_utils::init_test_tracing();
    // With NO-REMAP non-nil GNU does NOT expand or resolve remapping:
    // - next-line keeps its own key [110] plus the raw [remap forward-char].
    // - forward-char (only reachable via remapping) returns nil.
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m [remap forward-char] 'next-line)
             (define-key m "n" 'next-line)
             (list (where-is-internal 'next-line (list m) nil nil t)
                   (where-is-internal 'forward-char (list m) nil nil t)))"#,
    );
    assert_eq!(result, "OK (([110] [remap forward-char]) nil)");
}

#[test]
fn bootstrap_define_keymap_populates_help_style_bindings() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((m (define-keymap
                      "C-a" #'about-emacs
                      "a" #'describe-bindings
                      "RET" #'view-order-manuals)))
             (list (lookup-key m [1])
                   (lookup-key m [97])
                   (lookup-key m [13])))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap define-keymap result");
    assert_eq!(
        result,
        "OK (about-emacs describe-bindings view-order-manuals)"
    );
}

#[test]
fn bootstrap_defvar_keymap_and_fset_share_populated_keymap_object() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (defvar-keymap test-help-map
               "C-a" #'about-emacs
               "a" #'describe-bindings)
             (fset 'test-help-command test-help-map)
             (list (lookup-key test-help-map [1])
                   (lookup-key (symbol-function 'test-help-command) [1])
                   (eq test-help-map (symbol-function 'test-help-command))))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap defvar-keymap result");
    assert_eq!(result, "OK (about-emacs about-emacs t)");
}

#[test]
fn lookup_key_applies_menu_item_filters_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((m (make-sparse-keymap))
                 (tool (make-sparse-keymap))
                 (mouse (make-sparse-keymap)))
             (define-key tool [open] 'ignore)
             (define-key mouse [minor] 'ignore)
             (define-key m [tool-bar]
               `(menu-item "tool bar" ignore
                           :filter ,(lambda (_) tool)))
             (define-key m [C-down-mouse-3]
               `(menu-item "Menu Bar" ignore
                           :filter ,(lambda (_) mouse)))
             (list (keymapp (lookup-key m [tool-bar]))
                   (lookup-key (lookup-key m [tool-bar]) [open])
                   (keymapp (lookup-key m [C-down-mouse-3]))
                   (lookup-key (lookup-key m [C-down-mouse-3]) [minor])))"#,
    )
    .into_iter()
    .next()
    .expect("filtered menu-item lookup result");
    assert_eq!(result, "OK (t ignore t ignore)");
}

#[test]
fn bootstrap_runtime_help_map_and_help_command_are_populated_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(list (lookup-key help-map [1])
                 (lookup-key help-map [97])
                 (lookup-key (current-global-map) [8])
                 (lookup-key (current-global-map) [8 1])
                 (lookup-key (symbol-function 'help-command) [1]))"#,
    )
    .into_iter()
    .next()
    .expect("bootstrap help-map result");
    assert_eq!(
        result,
        "OK (about-emacs apropos-command help-command about-emacs about-emacs)"
    );
}

#[test]
fn command_modes_extracts_modes_and_preserves_arity_checks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(command-modes 'ignore)"), "OK nil");
    assert_eq!(eval_one("(command-modes nil)"), "OK nil");
    assert_eq!(eval_one("(command-modes 0)"), "OK nil");
    assert_eq!(eval_one("(command-modes \"ignore\")"), "OK nil");
    assert_eq!(
        eval_one("(command-modes '(lambda () (interactive)))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(command-modes '(lambda () (interactive \"p\" text-mode prog-mode) t))"),
        "OK (text-mode prog-mode)"
    );
    assert_eq!(eval_one("(command-modes '(lambda (x) x))"), "OK nil");
    assert_eq!(
        eval_one(
            "(progn
               (fset 'vm-command-modes-target '(lambda () t))
               (fset 'vm-command-modes-alias 'vm-command-modes-target)
               (put 'vm-command-modes-alias 'command-modes '(foo-mode bar-mode))
               (command-modes 'vm-command-modes-alias))"
        ),
        "OK (foo-mode bar-mode)"
    );
    assert_eq!(
        eval_one(
            "(let ((f (make-byte-code '() \"\" [] 0 nil [nil '(rust-ts-mode c-mode)])))
               (fset 'vm-command-modes-bytecode f)
               (command-modes 'vm-command-modes-bytecode))"
        ),
        "OK (rust-ts-mode c-mode)"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-modes)
                 (wrong-number-of-arguments (car err)))"#
        ),
        "OK wrong-number-of-arguments"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-modes 'ignore nil)
                 (wrong-number-of-arguments (car err)))"#
        ),
        "OK wrong-number-of-arguments"
    );
}

#[test]
fn command_modes_preserves_modes_on_interpreted_defun() {
    assert_eq!(
        eval_one(
            "(progn
               (defun command-modes-interpreted-probe ()
                 (interactive \"p\" text-mode prog-mode)
                 t)
               (list
                 (command-modes 'command-modes-interpreted-probe)
                 (aref (symbol-function 'command-modes-interpreted-probe) 5)))",
        ),
        "OK ((text-mode prog-mode) [\"p\" (text-mode prog-mode)])"
    );
}

#[test]
fn command_remapping_nil_and_keymap_type_checks() {
    crate::test_utils::init_test_tracing();
    assert_eq!(eval_one("(command-remapping 'ignore)"), "OK nil");
    assert_eq!(eval_one("(command-remapping 'ignore nil)"), "OK nil");
    assert_eq!(eval_one("(command-remapping 'ignore nil nil)"), "OK nil");
    assert_eq!(
        eval_one("(command-remapping 'ignore nil (list 'keymap))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(1 2 3))"),
        "OK nil"
    );
    assert_eq!(eval_one("(command-remapping 'ignore nil '(foo))"), "OK nil");
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(foo bar))"),
        "OK nil"
    );
    assert_eq!(eval_one("(command-remapping nil)"), "OK nil");
    assert_eq!(eval_one("(command-remapping 0)"), "OK nil");
    assert_eq!(eval_one("(command-remapping \"ignore\")"), "OK nil");
    assert_eq!(
        eval_one("(command-remapping '(lambda () (interactive)))"),
        "OK nil"
    );
    assert_eq!(eval_one("(command-remapping '(lambda (x) x))"), "OK nil");
    assert_eq!(eval_one("(command-remapping 'ignore '(x) nil)"), "OK nil");
    assert_eq!(
        eval_one("(command-remapping 'ignore '(x) (make-sparse-keymap))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore [x] (make-sparse-keymap))"),
        "OK nil"
    );

    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping 'ignore nil t)
                 (wrong-type-argument (list (car err) (cdr err))))"#
        ),
        "OK (wrong-type-argument (keymapp t))"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping 'ignore nil [1])
                 (wrong-type-argument (list (car err) (cdr err))))"#
        ),
        "OK (wrong-type-argument (keymapp [1]))"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping 'ignore nil "x")
                 (wrong-type-argument (list (car err) (cdr err))))"#
        ),
        "OK (wrong-type-argument (keymapp \"x\"))"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping 'ignore nil 1)
                 (wrong-type-argument (list (car err) (cdr err))))"#
        ),
        "OK (wrong-type-argument (keymapp 1))"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping 'ignore nil 'foo)
                 (wrong-type-argument (list (car err) (cdr err))))"#
        ),
        "OK (wrong-type-argument (keymapp foo))"
    );

    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping)
                 (wrong-number-of-arguments (car err)))"#
        ),
        "OK wrong-number-of-arguments"
    );
    assert_eq!(
        eval_one(
            r#"(condition-case err
                   (command-remapping 'ignore nil nil nil)
                 (wrong-number-of-arguments (car err)))"#
        ),
        "OK wrong-number-of-arguments"
    );
}

#[test]
fn command_remapping_integer_position_range_and_ordering_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((err (condition-case e
                                (command-remapping 'ignore 0)
                              (error e))))
                   (list (car err) (bufferp (nth 1 err)) (nth 2 err))))"#
        ),
        "OK (args-out-of-range t 0)"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((err (condition-case e
                                (command-remapping 'ignore -1)
                              (error e))))
                   (list (car err) (bufferp (nth 1 err)) (nth 2 err))))"#
        ),
        "OK (args-out-of-range t -1)"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((err (condition-case e
                                (command-remapping 'ignore 2)
                              (error e))))
                   (list (car err) (bufferp (nth 1 err)) (nth 2 err))))"#
        ),
        "OK (args-out-of-range t 2)"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((m (make-sparse-keymap)))
                   (define-key m [remap ignore] 'self-insert-command)
                   (list
                    (command-remapping 'ignore 1 m)
                    (command-remapping 'ignore t m)
                    (command-remapping 'ignore 'foo m)
                    (command-remapping 'ignore "x" m)
                    (command-remapping 'ignore [1] m)
                    (command-remapping 'ignore '(1) m)
                    (command-remapping 'ignore 1.5 m)
                    (command-remapping 'ignore (copy-marker (point)) m))))"#
        ),
        "OK (self-insert-command self-insert-command self-insert-command self-insert-command self-insert-command self-insert-command self-insert-command self-insert-command)"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((err (condition-case e
                                (command-remapping nil 0)
                              (error e))))
                   (list (car err) (bufferp (nth 1 err)) (nth 2 err))))"#
        ),
        "OK (args-out-of-range t 0)"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (condition-case e
                     (command-remapping 'ignore 0 t)
                   (error e)))"#
        ),
        "OK (wrong-type-argument keymapp t)"
    );
    assert_eq!(
        bootstrap_eval_one("(with-temp-buffer (command-remapping 0 0))"),
        "OK nil"
    );
}

#[test]
fn command_remapping_resolves_remap_bindings_on_keymap_handles() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] 'self-insert-command)
                 (command-remapping 'ignore nil m))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] [x])
                 (command-remapping 'ignore nil m))"#
        ),
        "OK [x]"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] 1)
                 (command-remapping 'ignore nil m))"#
        ),
        "OK nil"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] t)
                 (command-remapping 'ignore nil m))"#
        ),
        "OK nil"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] '(menu-item "x" ignore))
                 (command-remapping 'ignore nil m))"#
        ),
        "OK ignore"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] '(menu-item "x" 1))
                 (command-remapping 'ignore nil m))"#
        ),
        "OK nil"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] '(menu-item))
                 (command-remapping 'ignore nil m))"#
        ),
        "OK (menu-item)"
    );
    assert_eq!(
        eval_one(
            r#"(let ((m (make-sparse-keymap)))
                 (define-key m [remap ignore] 'self-insert-command)
                 (command-remapping 0 nil m))"#
        ),
        "OK nil"
    );
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap)))
                 (use-global-map g)
                 (define-key (current-global-map) [remap ignore] 'self-insert-command)
                 (command-remapping 'ignore))"#
        ),
        "OK self-insert-command"
    );
}

#[test]
fn command_remapping_global_map_remap_binding() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key g [remap ignore] 'self-insert-command)
                 (list (keymapp (current-global-map))
                       (lookup-key (current-global-map) [remap ignore])
                       (command-remapping 'ignore nil (current-global-map))
                       (command-remapping 'ignore)))"#
        ),
        "OK (t self-insert-command self-insert-command self-insert-command)"
    );
}

#[test]
fn command_remapping_prefers_local_map_when_keymap_omitted_or_nil() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        bootstrap_eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key l [remap ignore] 'self-insert-command)
                 (command-remapping 'ignore))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key l [remap ignore] 'self-insert-command)
                 (command-remapping 'ignore nil nil))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key g [remap ignore] 'forward-char)
                 (define-key l [remap ignore] 'self-insert-command)
                 (command-remapping 'ignore))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap)))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key g [remap ignore] 'self-insert-command)
                 (command-remapping 'ignore))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        bootstrap_eval_one(
            r#"(with-temp-buffer
                 (let ((g (make-sparse-keymap))
                       (l (make-sparse-keymap)))
                   (use-global-map g)
                   (use-local-map l)
                   (define-key l [remap ignore] 'self-insert-command)
                   (command-remapping 'ignore (point-min))))"#
        ),
        "OK self-insert-command"
    );
}

#[test]
fn switch_to_buffer_restores_buffer_local_map_for_key_lookup() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let* ((b1 (get-buffer-create "*km-a*"))
                      (b2 (get-buffer-create "*km-b*"))
                      (m1 (make-sparse-keymap))
                      (m2 (make-sparse-keymap)))
                 (define-key m1 "a" 'forward-char)
                 (define-key m2 "h" 'describe-mode)
                 (set-buffer b1)
                 (use-local-map m1)
                 (set-buffer b2)
                 (use-local-map m2)
                 (set-buffer b1)
                 (list (eq (current-local-map) m1)
                       (key-binding "a")
                       (key-binding "h")))"#
        ),
        "OK (t forward-char self-insert-command)"
    );
}

#[test]
fn command_remapping_checks_minor_mode_maps_before_local_and_global() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (m (make-sparse-keymap))
                     (minor-mode-map-alist nil)
                     (demo-mode t))
                 (use-global-map g)
                 (define-key m [remap ignore] 'self-insert-command)
                 (setq minor-mode-map-alist (list (cons 'demo-mode m)))
                 (command-remapping 'ignore))"#
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap))
                     (m (make-sparse-keymap))
                     (minor-mode-map-alist nil)
                     (demo-mode t))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key m [remap ignore] 'forward-char)
                 (define-key l [remap ignore] 'self-insert-command)
                 (setq minor-mode-map-alist (list (cons 'demo-mode m)))
                 (command-remapping 'ignore))"#
        ),
        "OK forward-char"
    );
    assert_eq!(
        eval_one(
            r#"(let ((g (make-sparse-keymap))
                     (l (make-sparse-keymap))
                     (m (make-sparse-keymap))
                     (minor-mode-overriding-map-alist nil)
                     (minor-mode-map-alist nil)
                     (demo-mode t))
                 (use-global-map g)
                 (use-local-map l)
                 (define-key m [remap ignore] 'forward-char)
                 (define-key l [remap ignore] 'self-insert-command)
                 (setq minor-mode-overriding-map-alist (list (cons 'demo-mode m)))
                 (setq minor-mode-map-alist (list (cons 'demo-mode l)))
                 (command-remapping 'ignore))"#
        ),
        "OK forward-char"
    );
    assert_eq!(
        eval_one(
            r#"(let ((minor-mode-map-alist '((demo-mode . 999999)))
                     (demo-mode t))
                 (command-remapping 'ignore))"#
        ),
        "OK nil"
    );
}

#[test]
fn command_remapping_resolves_remap_bindings_on_lisp_keymaps() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(command-remapping 'ignore nil '(keymap (remap keymap (ignore . self-insert-command))))"
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        eval_one(
            "(let ((m '(keymap (remap keymap (ignore . self-insert-command))))) (command-remapping 'ignore nil (list m)))"
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        eval_one(
            "(command-remapping 'ignore nil '(keymap (remap keymap (ignore menu-item \"x\" ignore))))"
        ),
        "OK ignore"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(keymap (remap keymap (ignore . 1))))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(keymap (remap keymap (ignore . t))))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(keymap (remap keymap (ignore))))"),
        "OK nil"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(keymap (remap keymap (ignore . [x]))))"),
        "OK [x]"
    );
    assert_eq!(
        eval_one("(command-remapping 'ignore nil '(keymap (remap keymap (ignore . \"x\"))))"),
        "OK \"x\""
    );
    assert_eq!(
        eval_one(
            "(command-remapping 'not-bound nil '(keymap (remap keymap (not-bound . self-insert-command))))"
        ),
        "OK self-insert-command"
    );
    assert_eq!(
        eval_one(
            "(command-remapping 'ignore nil '(keymap (remap keymap (foo . self-insert-command))))"
        ),
        "OK nil"
    );
}

#[test]
fn command_remapping_resolves_remap_bindings_across_composed_keymap_members() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            "(let ((map '(keymap
                          (keymap (remap keymap (act . y-or-n-p-insert-y)))
                          keymap
                          (121 . act))))
               (list (lookup-key map \"y\")
                     (command-remapping 'act nil map)))"
        ),
        "OK (act y-or-n-p-insert-y)"
    );
}

#[test]
fn a_symbol_keymap_tail_is_followed_by_both_lookup_and_where_is() {
    // GNU's spine loops retry `get_keymap` whenever the tail stops being a cons
    // -- `access_keymap_1`: `(CONSP (tail) || (tail = get_keymap (tail, 0,
    // autoload), CONSP (tail)))`, and the same in `map_keymap` -- so a tail that
    // NAMES a keymap continues the spine. neomacs resolved it nowhere, so both
    // directions missed such a binding.
    //
    // `set-keymap-parent` rejects a non-keymap, so the shape is built with
    // `setcdr`, which is how it arises in hand-built spines.
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap))
                 (tail-map (make-sparse-keymap)))
             (define-key m "a" 'own-cmd)
             (define-key tail-map "t" 'tail-cmd)
             (fset 'tail-map-symbol tail-map)
             ;; `m' is `(keymap (?a . own-cmd))'; its last cons is `(cdr m)'.
             (setcdr (cdr m) 'tail-map-symbol)
             (list (eq (lookup-key m "t") 'tail-cmd)
                   (eq (lookup-key m "a") 'own-cmd)
                   (equal (where-is-internal 'tail-cmd (list m) t) [?t])))"#,
    );
    assert_eq!(result, "OK (t t t)");
}

#[test]
fn a_spine_tail_naming_no_keymap_ends_the_spine() {
    // GNU's loop condition fails when `get_keymap' returns nil, so the walk just
    // ends -- no binding, no error.
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(let ((m (make-sparse-keymap)))
             (define-key m "a" 'own-cmd)
             (setcdr (cdr m) 'not-a-keymap-symbol)
             (list (eq (lookup-key m "a") 'own-cmd)
                   (null (lookup-key m "z"))
                   (null (where-is-internal 'nowhere-cmd (list m) t))))"#,
    );
    assert_eq!(result, "OK (t t t)");
}

/// GNU's `quotify_arg' (callint.c:127) wraps an argument in `quote' when it is
/// a cons, or a symbol other than nil and t -- exactly the values that would
/// not evaluate to themselves when `repeat-complex-command' re-reads the entry.
/// Numbers, strings and vectors are recorded bare.
#[test]
fn command_history_quotes_arguments_that_do_not_evaluate_to_themselves() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(progn
             (defun p9-hist-raw (arg) (interactive "P") arg)
             (defun p9-hist-sym (s) (interactive (list 'a-symbol)) s)
             (defun p9-hist-lit (s v n) (interactive (list "txt" [1 2] 7)) (list s v n))
             (defun p9-hist-bool (x y) (interactive (list t nil)) (list x y))
             (setq command-history nil)
             (let ((current-prefix-arg '(4))) (call-interactively 'p9-hist-raw t))
             (call-interactively 'p9-hist-sym t)
             (call-interactively 'p9-hist-lit t)
             (call-interactively 'p9-hist-bool t)
             (mapcar #'prin1-to-string (reverse command-history)))"#,
    );
    assert_eq!(
        result,
        r#"OK ("(p9-hist-raw '(4))" "(p9-hist-sym 'a-symbol)" "(p9-hist-lit \"txt\" [1 2] 7)" "(p9-hist-bool t nil)")"#
    );
}

/// GNU's `varies' array (callint.c:781) records an argument that came from
/// point, the mark or the region as a call to the function that produced it,
/// so replaying the entry re-reads the *current* position instead of the one
/// that happened to be current when the command ran.
#[test]
fn command_history_records_position_arguments_as_the_calls_that_produced_them() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (defun p9-hist-d (p) (interactive "d") p)
             (defun p9-hist-m (m) (interactive "m") m)
             (defun p9-hist-r (b e) (interactive "r") (list b e))
             (defun p9-hist-cap-r (b e) (interactive "R") (list b e))
             (insert "hello world")
             (set-mark 3)
             (goto-char 8)
             (setq command-history nil)
             (call-interactively 'p9-hist-d t)
             (call-interactively 'p9-hist-m t)
             (call-interactively 'p9-hist-r t)
             (call-interactively 'p9-hist-cap-r t)
             (mapcar #'prin1-to-string (reverse command-history)))"#,
    );
    assert_eq!(
        result,
        r#"OK ("(p9-hist-d (point))" "(p9-hist-m (mark))" "(p9-hist-r (region-beginning) (region-end))" "(p9-hist-cap-r (use-region-beginning) (use-region-end))")"#
    );
}

/// GNU's `fix_command' (callint.c:175) rewrites the recorded arguments of a
/// command carrying an `interactive-args' property, then drops trailing nil
/// optional arguments so the history entry reads as the user would type it.
/// The trim is skipped for a `&rest' function, whose arity has no fixed max.
#[test]
fn command_history_applies_interactive_args_and_trims_trailing_optional_nils() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(progn
             (defun p9-fix-reps (a b) (interactive (list 'val 'other)) (list a b))
             (put 'p9-fix-reps 'interactive-args '((1 . (some-form))))
             (defun p9-fix-trim (a b &optional c d) (interactive (list 'x 'y nil nil))
               (list a b c d))
             (defun p9-fix-keep (a &optional b) (interactive (list nil nil)) (list a b))
             (defun p9-fix-rest (a &rest r) (interactive (list 'x nil nil)) (list a r))
             (setq command-history nil)
             (call-interactively 'p9-fix-reps t)
             (call-interactively 'p9-fix-trim t)
             (call-interactively 'p9-fix-keep t)
             (call-interactively 'p9-fix-rest t)
             (mapcar #'prin1-to-string (reverse command-history)))"#,
    );
    assert_eq!(
        result,
        r#"OK ("(p9-fix-reps 'val (some-form))" "(p9-fix-trim 'x 'y)" "(p9-fix-keep nil nil)" "(p9-fix-rest 'x nil nil)")"#
    );
}

/// The `history_forms` slice length is a letter's argument count, so the table
/// must agree with the arguments the spec walk actually pushes.  Checked here
/// for every letter that reads no input, which covers both letters GNU gives a
/// two-argument entry.
#[test]
fn every_letter_reports_one_history_form_per_argument_it_pushes() {
    crate::test_utils::init_test_tracing();
    for letter in InteractiveControlLetter::iter() {
        let forms = letter.history_forms().len();
        let expected = match letter {
            InteractiveControlLetter::Region | InteractiveControlLetter::ActiveRegion => 2,
            _ => 1,
        };
        assert_eq!(
            forms,
            expected,
            "history_forms arity for spec letter {}",
            letter.letter()
        );
    }

    // The letters that resolve without reading input, walked for real.
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (defun p9-arity (&rest args) (interactive "d\nm\nr\nR\ni\nP\np\nU") (length args))
             (insert "hello world")
             (set-mark 3)
             (goto-char 8)
             (call-interactively 'p9-arity t))"#,
    );
    let walked: usize = result
        .strip_prefix("OK ")
        .expect("spec walk should succeed")
        .parse()
        .expect("argument count");
    let tabled: usize = "dmrRiPpU"
        .chars()
        .map(|letter| {
            InteractiveControlLetter::from_char(letter)
                .expect("known spec letter")
                .history_forms()
                .len()
        })
        .sum();
    assert_eq!(walked, tabled, "spec walk arity vs history_forms arity");
}

/// GNU `internal_self_insert' (src/cmds.c:484-492) fills the line a
/// self-inserted newline just terminated: point steps back over the newline so
/// `internal-auto-fill' sees the finished line's column, and steps forward
/// again afterwards.  Without that step a newline never wraps anything, because
/// point sits in column 0 of the fresh line when the filler runs.
#[test]
fn self_inserted_newline_fills_the_line_it_terminated() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (text-mode)
             (setq fill-column 24)
             (auto-fill-mode 1)
             (dolist (char (string-to-list
                            "Summary words continue beyond configured boundary\n\n"))
               (let ((last-command-event char))
                 (self-insert-command 1 char)))
             (buffer-string))"#,
    );
    assert_eq!(
        result,
        "OK \"Summary words continue\nbeyond configured\nboundary\n\n\""
    );
}

/// A self-inserted space fills the current line and leaves point after the
/// space, so the newline step above must stay confined to the newline case.
#[test]
fn self_inserted_space_fills_without_shifting_point() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(with-temp-buffer
             (text-mode)
             (setq fill-column 24)
             (auto-fill-mode 1)
             (dolist (char (string-to-list "Summary words continue beyond "))
               (let ((last-command-event char))
                 (self-insert-command 1 char)))
             (list (buffer-string) (point)))"#,
    );
    assert_eq!(result, "OK (\"Summary words continue\nbeyond \" 31)");
}
