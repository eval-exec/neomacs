use super::*;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::{Context, format_eval_result};
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_all};

fn eval_all(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    // Root all parsed forms across the eval loop. Same GC hazard
    // as eval_test::eval_all: the Vec<Value> lives on the malloc
    // heap and is invisible to conservative stack scanning.
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

fn bootstrap_context() -> Context {
    runtime_startup_context()
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

// -- CustomManager unit tests ------------------------------------------

#[test]
fn custom_manager_new_is_empty() {
    crate::test_utils::init_test_tracing();
    // Phase D: auto_buffer_local mirror deleted. CustomManager is now empty.
    let _cm = CustomManager::new();
}

#[test]
fn custom_manager_pdump_round_trip() {
    crate::test_utils::init_test_tracing();
    // Phase D: dump_custom_manager emits empty vecs; load_custom_manager
    // ignores the payload. Round-trip is a no-op.
    let cm = CustomManager::new();
    let dump = crate::emacs_core::pdump::convert::dump_custom_manager(&cm);
    assert!(dump.auto_buffer_local.is_empty());
    assert!(dump.auto_buffer_local_syms.is_empty());
    // Loading a legacy dump with entries should succeed and return an
    // empty CustomManager (entries are now irrelevant).
    let legacy_dump = crate::emacs_core::pdump::types::DumpCustomManager {
        auto_buffer_local_syms: Vec::new(),
        auto_buffer_local: vec!["tab-width".to_string(), "fill-column".to_string()],
    };
    let _restored = crate::emacs_core::pdump::convert::load_custom_manager(&legacy_dump);
}

// -- GNU custom.el runtime tests ----------------------------------------

#[test]
fn defcustom_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defcustom my-var 42 "My variable.")"#);
    assert_eq!(results[0], "OK my-var");
}

#[test]
fn defcustom_sets_value() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defcustom my-var 42 "My variable.") my-var"#);
    assert_eq!(results[1], "OK 42");
}

#[test]
fn defcustom_with_type() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defcustom my-var 42 "Docs." :type 'integer) my-var"#);
    assert_eq!(results[1], "OK 42");
}

#[test]
fn defcustom_with_group() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defcustom my-var 10 "Docs." :group 'my-group) my-var"#);
    assert_eq!(results[1], "OK 10");
}

#[test]
fn custom_declare_variable_version_string_survives_gc_stress() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    ev.gc_stress = true;

    let result = ev
        .eval_str(
            r#"(progn
                 (custom-declare-variable
                   'vm-custom-version-gc
                   nil
                   "Docs."
                   :type 'boolean
                   :version "29.1")
                 (get 'vm-custom-version-gc 'custom-version))"#,
        )
        .expect("custom-declare-variable should preserve :version string");

    assert_eq!(result, Value::string("29.1"));
}

#[test]
fn defcustom_does_not_override_existing() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(setq my-var 99) (defcustom my-var 42 "Docs.") my-var"#);
    // defcustom should not override an existing value, like defvar
    assert_eq!(results[2], "OK 99");
}

#[test]
fn defcustom_marks_special() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    let _result = ev.eval_str(r#"(defcustom my-var 42 "Docs.")"#);
    assert!(ev.obarray().is_special("my-var"));
}

#[test]
fn defcustom_custom_variable_p() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defcustom my-var 42 "Docs.") (custom-variable-p 'my-var) (custom-variable-p 'other)"#,
    );
    // custom-variable-p returns the standard-value property (truthy), not t.
    // GNU: ((funcall #'#[nil (42) (t)]))
    assert!(
        results[1].starts_with("OK ("),
        "custom-variable-p should return truthy standard-value, got: {}",
        results[1]
    );
    assert_eq!(results[2], "OK nil");
}

/// Reproduces the `help-macro.el` defcustom-then-reference pattern:
/// after the macroexpand-all-toplevel pass that the loader's eager
/// expansion runs, custom-declare-variable should still set the
/// variable. We verify by directly calling the *expanded* form so
/// the test exercises exactly what the loader sees post-expansion.
#[test]
fn custom_declare_variable_with_funcall_lambda_default_sets_variable() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    ev.set_lexical_binding(true);
    // Mirror the form that the byte-compiler produces for
    // `\`(funcall #',(lambda () nil))` per GNU macroexp:
    //   (list 'funcall (list 'function #'(lambda nil nil)))
    // — `function` wraps the lambda as its single argument inside a
    // list, NOT a cons. Earlier scratch tests used (cons 'function
    // (list 'lambda nil nil)) which yields `(function lambda nil
    // nil)` and signals wrong-number-of-arguments on the function
    // special form.
    let result = ev
        .eval_str(
            r#"(progn
                 (custom-declare-variable
                   'test-three-step-help
                   (list 'funcall (list 'function (list 'lambda nil 'nil)))
                   "docstring"
                   :type 'boolean
                   :group 'help)
                 (boundp 'test-three-step-help))"#,
        )
        .expect("eval");
    assert_eq!(
        result,
        crate::emacs_core::value::Value::T,
        "test-three-step-help should be bound after custom-declare-variable"
    );
}

/// Loads a real .elc fixture compiled by GNU Emacs from a tiny
/// `(defcustom test-mle-edge 'window ...)` source file. Verifies
/// that the .elc bytecode path correctly invokes
/// custom-declare-variable and that the variable is set afterwards.
/// This is the smallest possible repro for the bindings.elc bootstrap
/// failure (which signals void-variable on the same defcustom shape).
#[test]
fn defcustom_loaded_from_real_elc_sets_variable() {
    crate::test_utils::init_test_tracing();
    let dir = tempfile::tempdir().expect("tempdir");
    let src_path = dir.path().join("test_dc.el");
    std::fs::write(
        &src_path,
        r#";;; -*- lexical-binding: t -*-
(defvar-local test-mle-process nil "doc1")
;;;###autoload
(put 'test-mle-process 'risky-local-variable t)

(defcustom test-mle-edge 'window
  "Where function `test-mle-fn' should align to.
Multiple lines, just like the bindings.el version."
  :type '(choice (const right-margin)
                 (const right-fringe)
                 (const window))
  :group 'help
  :version "30.1")
(defun test-mle-fn () test-mle-edge)
"#,
    )
    .expect("write source");
    // Byte-compile via GNU Emacs.
    let gnu_emacs = "/home/exec/.local/bin/emacs";
    if !std::path::Path::new(gnu_emacs).exists() {
        eprintln!("skipping: GNU Emacs not available at {gnu_emacs}");
        return;
    }
    let status = std::process::Command::new(gnu_emacs)
        .args(["--batch", "--eval"])
        .arg(format!("(byte-compile-file \"{}\")", src_path.display()))
        .status()
        .expect("byte-compile");
    assert!(status.success(), "byte-compile failed");
    let elc_path = dir.path().join("test_dc.elc");
    assert!(elc_path.exists(), ".elc file should exist");

    let mut ev = bootstrap_context();
    super::super::load::load_file(&mut ev, &elc_path).expect("load .elc fixture");
    let result = ev.eval_str("(boundp 'test-mle-edge)").expect("boundp eval");
    assert_eq!(
        result,
        crate::emacs_core::value::Value::T,
        "test-mle-edge should be bound after loading .elc"
    );
    let value = ev.eval_str("test-mle-edge").expect("read test-mle-edge");
    assert_eq!(value, crate::emacs_core::value::Value::symbol("window"));
}

/// Reproduces the actual `.elc` form for `(defcustom mode-line-right-align-edge
/// 'window ...)` — with a *bytecode* lambda as the default's lambda
/// (not a regular cons-cell lambda). The .elc bytecode for the
/// defcustom default constructs `(list 'funcall (list 'function #[0
/// "\300\207" [window] 1]))` where #[...] is a real bytecode object.
#[test]
fn custom_declare_variable_with_bytecode_lambda_default_sets_variable() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    ev.set_lexical_binding(true);
    let result = ev
        .eval_str(
            r##"(progn
                  (custom-declare-variable
                    'test-bc-edge
                    (list 'funcall (list 'function (read "#[0 \"\\300\\207\" [window] 1]")))
                    "docstring"
                    :type 'symbol
                    :group 'help)
                  test-bc-edge)"##,
        )
        .expect("eval");
    assert_eq!(
        result,
        crate::emacs_core::value::Value::symbol("window"),
        "test-bc-edge should be set to 'window via bytecode lambda default"
    );
}

/// Reproduces the `bindings.elc` failure pattern: defcustom with a
/// quoted-symbol default followed by a defun that references the
/// variable. GNU verified: this should yield (window window).
#[test]
fn defcustom_quoted_symbol_default_followed_by_defun_using_var() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    ev.set_lexical_binding(true);
    let results: Vec<_> = ev
        .eval_str_each(
            r#"(defcustom mode-line-test-edge 'window
                  "Where function `mode-line-format-right-align' should align to."
                  :type '(choice (const right-margin)
                                 (const right-fringe)
                                 (const window)))
               (defun mode--line-test ()
                 (let ((edge mode-line-test-edge))
                   edge))
               (list mode-line-test-edge (mode--line-test))"#,
        )
        .iter()
        .map(crate::emacs_core::error::format_eval_result)
        .collect();
    // GNU verified via:
    //   emacs --batch --eval '(progn (defcustom ...) (defun ...) (list ...))'
    //   => (window window)
    assert_eq!(results[2], "OK (window window)", "results: {results:?}");
}

// -- GNU custom.el group tests -----------------------------------------

#[test]
fn defgroup_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defgroup my-group nil "My group.")"#);
    assert_eq!(results[0], "OK my-group");
}

#[test]
fn defgroup_registers_group() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    let _result = ev.eval_str(r#"(defgroup my-group nil "Docs.")"#);
    let doc = ev
        .obarray
        .get_property("my-group", "group-documentation")
        .expect("group-documentation");
    assert_eq!(doc.as_utf8_str(), Some("Docs."));
}

#[test]
fn custom_group_p_unavailable_without_custom_library() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defgroup my-group nil "Docs.")
           (fboundp 'custom-group-p)
           (custom-group-p 'my-group)
           (custom-group-p 'other)"#,
    );
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "ERR (void-function (custom-group-p))");
    assert_eq!(results[3], "ERR (void-function (custom-group-p))");
}

#[test]
fn defgroup_with_parent_records_parent_group() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defgroup parent-group nil "Parent.")
           (defgroup child-group nil "Child." :group 'parent-group)
           (get 'parent-group 'custom-group)"#,
    );
    assert_eq!(results[2], "OK ((child-group custom-group))");
}

// -- defvar-local GNU macro tests ---------------------------------------

#[test]
fn defvar_local_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defvar-local my-local 42) my-local"#);
    assert_eq!(results[0], "OK my-local");
    assert_eq!(results[1], "OK 42");
}

#[test]
fn defvar_local_marks_special() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    let _result = ev.eval_str(r#"(defvar-local my-local 42)"#);
    assert!(ev.obarray().is_special("my-local"));
}

#[test]
fn defvar_local_marks_buffer_local() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    let _result = ev.eval_str(r#"(defvar-local my-local 42)"#);
    assert!(ev.obarray().is_buffer_local("my-local"));
    // Phase D: is_auto_buffer_local mirror removed; verify via BLV local_if_set.
    let id = crate::emacs_core::intern::intern("my-local");
    assert!(ev.obarray().blv(id).map_or(false, |b| b.local_if_set));
}

#[test]
fn defvar_local_does_not_override() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(setq my-local 99) (defvar-local my-local 42) my-local"#);
    assert_eq!(results[2], "OK 99");
}

#[test]
fn defvar_local_with_docstring() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defvar-local my-local 42 "Documentation.") my-local"#);
    assert_eq!(results[1], "OK 42");
}

// -- setq-default macro tests ------------------------------------------

#[test]
fn setq_default_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defvar x 10) (setq-default x 42) x"#);
    assert_eq!(results[2], "OK 42");
}

#[test]
fn setq_default_multiple_pairs() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(defvar a 1) (defvar b 2) (setq-default a 10 b 20) a"#);
    assert_eq!(results[3], "OK 10");
}

#[test]
fn setq_default_returns_last_value() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(setq-default x 42)"#);
    assert_eq!(results[0], "OK 42");
}

#[test]
fn setq_default_follows_alias_resolution() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defvaralias 'vm-setq-default-alias 'vm-setq-default-base)
           (setq-default vm-setq-default-alias 3)
           (list (default-value 'vm-setq-default-base)
                 (default-value 'vm-setq-default-alias))"#,
    );
    assert_eq!(results[2], "OK (3 3)");
}

#[test]
fn setq_default_rejects_constant_symbols() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list
             (condition-case err (setq-default nil 1) (error err))
             (condition-case err (setq-default :foo 1) (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK ((setting-constant nil) (setting-constant :foo))"
    );
}

#[test]
fn setq_default_alias_triggers_variable_watchers_twice() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(setq vm-setq-default-watch-events nil)
           (fset 'vm-setq-default-watch-rec
                 (lambda (symbol newval operation where)
                   (setq vm-setq-default-watch-events
                         (cons (list symbol newval operation where)
                               vm-setq-default-watch-events))))
           (defvaralias 'vm-setq-default-watch 'vm-setq-default-watch-base)
           (add-variable-watcher 'vm-setq-default-watch-base 'vm-setq-default-watch-rec)
           (setq-default vm-setq-default-watch 7)
           (length vm-setq-default-watch-events)"#,
    );
    assert_eq!(results[5], "OK 2");
}

// -- make-variable-buffer-local builtin --------------------------------

#[test]
fn make_variable_buffer_local_works() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(r#"(make-variable-buffer-local 'my-var)"#);
    assert_eq!(results[0], "OK my-var");
}

#[test]
fn make_variable_buffer_local_binds_unbound_symbol_to_nil_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (makunbound 'vm-mvbl-unbound)
             (make-variable-buffer-local 'vm-mvbl-unbound)
             (list (boundp 'vm-mvbl-unbound)
                   (default-value 'vm-mvbl-unbound)
                   (with-temp-buffer
                     (local-variable-p 'vm-mvbl-unbound))))"#,
    );
    assert_eq!(result[0], "OK (t nil nil)");
}

#[test]
fn make_variable_buffer_local_resolves_alias_for_auto_local_assignment() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(setq vm-mvbl-base 1)
           (defvaralias 'vm-mvbl-alias 'vm-mvbl-base)
           (make-variable-buffer-local 'vm-mvbl-alias)
           (with-temp-buffer
             (setq vm-mvbl-alias 7)
             (list (local-variable-p 'vm-mvbl-alias)
                   (local-variable-p 'vm-mvbl-base)
                   vm-mvbl-alias
                   vm-mvbl-base
                   (default-value 'vm-mvbl-base)))"#,
    );
    assert_eq!(result[3], "OK (t t 7 7 1)");
}

#[test]
fn make_variable_buffer_local_constant_and_keyword_payloads_match_oracle() {
    crate::test_utils::init_test_tracing();
    let result = eval_all(
        r#"(list
             (condition-case err (make-variable-buffer-local nil) (error err))
             (condition-case err (make-variable-buffer-local t) (error err))
             (condition-case err (make-variable-buffer-local :vm-mvbl-k) (error err))
             (condition-case err (make-variable-buffer-local 1) (error err)))"#,
    );
    assert_eq!(
        result[0],
        "OK ((setting-constant nil) (setting-constant t) (setting-constant :vm-mvbl-k) (wrong-type-argument symbolp 1))"
    );
}

// -- make-local-variable builtin ---------------------------------------

#[test]
fn make_local_variable_in_buffer() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(defvar my-var 42)
           (get-buffer-create "test-buf")
           (set-buffer "test-buf")
           (make-local-variable 'my-var)
           (local-variable-p 'my-var)"#,
    );
    assert_eq!(results[4], "OK t");
}

#[test]
fn make_local_variable_resolves_alias_bindings() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(setq vm-mlv-base 4)
           (defvaralias 'vm-mlv-alias 'vm-mlv-base)
           (with-temp-buffer
             (make-local-variable 'vm-mlv-alias)
             (list (local-variable-p 'vm-mlv-alias)
                   (local-variable-p 'vm-mlv-base)
                   (symbol-value 'vm-mlv-alias)
                   (symbol-value 'vm-mlv-base)
                   (default-value 'vm-mlv-base)))"#,
    );
    assert_eq!(result[2], "OK (t t 4 4 4)");
}

#[test]
fn make_local_variable_preserves_existing_buffer_local_binding() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (setq vm-mlv-preserve-global 1)
             (with-temp-buffer
               (set (make-local-variable 'vm-mlv-preserve-global) 9)
               (make-local-variable 'vm-mlv-preserve-global)
               (list vm-mlv-preserve-global
                     (default-value 'vm-mlv-preserve-global))))"#,
    );
    assert_eq!(result[0], "OK (9 1)");
}

#[test]
fn make_local_variable_captures_dynamic_value_in_new_local_binding() {
    crate::test_utils::init_test_tracing();
    let result = eval_all(
        r#"(let ((buf (get-buffer-create "vm-mlv-buf")))
             (let ((vm-mlv-cross 5))
               (set-buffer buf)
               (make-local-variable 'vm-mlv-cross))
             (set-buffer buf)
             (condition-case err vm-mlv-cross (error err)))"#,
    );
    assert_eq!(result[0], "OK 5");
}

#[test]
fn make_local_variable_on_void_symbol_creates_local_void_binding() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(with-temp-buffer
             (makunbound 'vm-mlv-void)
             (make-local-variable 'vm-mlv-void)
             (list (local-variable-p 'vm-mlv-void (current-buffer))
                   (buffer-local-boundp 'vm-mlv-void (current-buffer))
                   (condition-case err (symbol-value 'vm-mlv-void) (error (car err)))
                   (condition-case err
                       (buffer-local-value 'vm-mlv-void (current-buffer))
                     (error (car err)))
                   (not (null (memq 'vm-mlv-void (buffer-local-variables))))
                   (assoc 'vm-mlv-void (buffer-local-variables))))"#,
    );
    assert_eq!(result[0], "OK (t nil void-variable void-variable t nil)");
}

#[test]
fn make_local_variable_ignores_lexical_bindings_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((lexical-binding t))
             (eval
              '(progn
                 (setq vm-mlv-lex-global 'global)
                 (with-temp-buffer
                   (let ((vm-mlv-lex-global 'lex))
                     (make-local-variable 'vm-mlv-lex-global)
                     (list vm-mlv-lex-global
                           (symbol-value 'vm-mlv-lex-global)
                           (buffer-local-value 'vm-mlv-lex-global (current-buffer))
                           (local-variable-p 'vm-mlv-lex-global (current-buffer))
                           (buffer-local-boundp 'vm-mlv-lex-global (current-buffer))
                           (default-value 'vm-mlv-lex-global)))))
              t))"#,
    );
    assert_eq!(result[0], "OK (lex global global t t global)");
}

#[test]
fn make_local_variable_after_compiled_preread_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let* ((fn (byte-compile
                      '(lambda ()
                         (unless delay-mode-hooks nil)
                         (make-local-variable 'delay-mode-hooks)
                         (let ((delay-mode-hooks t)) nil)
                         (list (local-variable-p 'delay-mode-hooks)
                               (assq 'delay-mode-hooks (buffer-local-variables))
                               delay-mode-hooks
                               (default-value 'delay-mode-hooks))))))
             (with-temp-buffer
               (funcall fn)))"#,
    );
    assert_eq!(result[0], "OK (t (delay-mode-hooks) nil nil)");
}

#[test]
fn derived_mode_grandparent_hook_fires() {
    crate::test_utils::init_test_tracing();
    // 2-level derived chain: vm-mode-a -> vm-mode-b -> text-mode.
    // text-mode is the GRANDPARENT of vm-mode-a. Its hook must fire when
    // vm-mode-a is invoked (org->outline->text symptom). GNU runs the
    // accumulated hooks innermost-ancestor-first: (text b a).
    let result = bootstrap_eval_all(
        r#"(progn
             (setq vm-grand-log nil)
             (define-derived-mode vm-mode-b text-mode "VM-B")
             (define-derived-mode vm-mode-a vm-mode-b "VM-A")
             (with-temp-buffer
               (let ((text-mode-hook (list (lambda () (push 'text vm-grand-log))))
                     (vm-mode-b-hook (list (lambda () (push 'b vm-grand-log))))
                     (vm-mode-a-hook (list (lambda () (push 'a vm-grand-log)))))
                 (vm-mode-a)
                 (list major-mode (nreverse vm-grand-log)))))"#,
    );
    assert_eq!(result[0], "OK (vm-mode-a (text b a))");
}

#[test]
fn derived_mode_grandparent_hook_fires_real_outline_chain() {
    crate::test_utils::init_test_tracing();
    // Real bundled outline-mode (outline -> text), then a 2-level derived
    // mode built on the REAL outline-mode so text-mode is the grandparent.
    let real = bootstrap_eval_all(
        r#"(progn
             (require 'outline)
             (setq vm-real-log nil)
             (define-derived-mode vm-real-child outline-mode "VM-Real")
             (with-temp-buffer
               (let ((text-mode-hook (list (lambda () (push 'text vm-real-log))))
                     (outline-mode-hook (list (lambda () (push 'outline vm-real-log))))
                     (vm-real-child-hook (list (lambda () (push 'child vm-real-log)))))
                 (vm-real-child)
                 (list major-mode (nreverse vm-real-log)))))"#,
    );
    assert_eq!(real[0], "OK (vm-real-child (text outline child))");

    // 1-level baseline (outline -> text): both hooks fire.
    let real1 = bootstrap_eval_all(
        r#"(progn
             (require 'outline)
             (setq vm-real1-log nil)
             (with-temp-buffer
               (let ((text-mode-hook (list (lambda () (push 'text vm-real1-log))))
                     (outline-mode-hook (list (lambda () (push 'outline vm-real1-log)))))
                 (outline-mode)
                 (list major-mode (nreverse vm-real1-log)))))"#,
    );
    assert_eq!(real1[0], "OK (outline-mode (text outline))");
}

#[test]
fn let_local_unbind_skips_restore_when_local_killed_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // ROOT-CAUSE unit test. A var made buffer-local, then `let`-bound,
    // whose local binding is KILLED by `kill-all-local-variables` inside
    // the let body. GNU `do_one_unbind` SPECPDL_LET_LOCAL only restores
    // the old local value `if (!NILP (Flocal_variable_p (symbol, where)))`
    // -- so a non-permanent var killed by KALV is left non-local with the
    // default value; the kill wins. neomacs previously restored the old
    // local value unconditionally, leaking stale buffer-local state.
    //
    // GNU 31.0.90 (emacs -Q --batch): (nil nil)
    let div = bootstrap_eval_all(
        r#"(progn
             (defvar vm-div-var nil)
             (with-temp-buffer
               (make-local-variable 'vm-div-var)
               (setq vm-div-var 'pre)
               (let ((vm-div-var 'bound))
                 (kill-all-local-variables))
               (list (local-variable-p 'vm-div-var) vm-div-var)))"#,
    );
    assert_eq!(div[0], "OK (nil nil)");
}

#[test]
fn let_local_unbind_restores_permanent_local_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // Counterpart to the above: for a PERMANENT-LOCAL var (the
    // `delay-mode-hooks` shape), KALV preserves the local binding, so
    // `Flocal_variable_p` stays true and the let-unbind DOES restore the
    // pre-let local value. This must remain unchanged by the fix.
    //
    // GNU 31.0.90 (emacs -Q --batch): (t pre)
    let divp = bootstrap_eval_all(
        r#"(progn
             (defvar vm-div-perm nil)
             (put 'vm-div-perm 'permanent-local t)
             (with-temp-buffer
               (make-local-variable 'vm-div-perm)
               (setq vm-div-perm 'pre)
               (let ((vm-div-perm 'bound))
                 (kill-all-local-variables))
               (list (local-variable-p 'vm-div-perm) vm-div-perm)))"#,
    );
    assert_eq!(divp[0], "OK (t pre)");
}

#[test]
fn delayed_mode_hooks_accumulator_not_resurrected_after_kill_matches_gnu() {
    crate::test_utils::init_test_tracing();
    // The exact org/derived-mode hook-loss mechanism. The non-permanent
    // buffer-local accumulator `delayed-mode-hooks` is let-bound (as can
    // happen on a deferred-org / mode-restart path), then a major-mode
    // switch runs `kill-all-local-variables` which clears it. GNU leaves
    // it non-local and nil; the buggy let-unbind resurrected a stale value
    // which then corrupted the next `run-mode-hooks` flush (dropping the
    // grandparent text-mode-hook).
    //
    // GNU 31.0.90 (emacs -Q --batch): (:after-let nil nil)
    let res = bootstrap_eval_all(
        r#"(with-temp-buffer
             (make-local-variable 'delayed-mode-hooks)
             (setq delayed-mode-hooks '(stale-hook))
             (let ((delayed-mode-hooks '(let-bound)))
               (kill-all-local-variables))
             (list :after-let (local-variable-p 'delayed-mode-hooks) delayed-mode-hooks))"#,
    );
    assert_eq!(res[0], "OK (:after-let nil nil)");
}

#[test]
fn make_local_variable_constant_and_keyword_payloads_match_oracle() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(list
             (condition-case err (with-temp-buffer (make-local-variable nil)) (error err))
             (condition-case err (with-temp-buffer (make-local-variable t)) (error err))
             (condition-case err (with-temp-buffer (make-local-variable :vm-k)) (error err))
             (condition-case err (with-temp-buffer (make-local-variable 1)) (error err)))"#,
    );
    assert_eq!(
        result[0],
        "OK ((setting-constant nil) (setting-constant t) (setting-constant :vm-k) (wrong-type-argument symbolp 1))"
    );
}

#[test]
fn make_local_variable_preserves_buffer_undo_list_value() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(let ((a (generate-new-buffer " undo-a"))
                 (b (generate-new-buffer " undo-b")))
             (unwind-protect
                 (progn
                   (with-current-buffer a
                     (insert "x"))
                   (list
                    (with-current-buffer a buffer-undo-list)
                    (with-current-buffer b buffer-undo-list)
                    (with-current-buffer b
                      (make-local-variable 'buffer-undo-list))
                    (with-current-buffer a buffer-undo-list)
                    (with-current-buffer b buffer-undo-list)))
               (kill-buffer a)
               (kill-buffer b)))"#,
    );
    assert_eq!(result[0], "OK (t t buffer-undo-list t t)");
}

// -- local-variable-p builtin ------------------------------------------

#[test]
fn local_variable_p_returns_nil_when_not_local() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(get-buffer-create "test-buf")
           (set-buffer "test-buf")
           (local-variable-p 'nonexistent)"#,
    );
    assert_eq!(results[2], "OK nil");
}

#[test]
fn local_variable_p_reports_builtin_buffer_locals() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(with-temp-buffer
             (list (local-variable-p 'major-mode)
                   (local-variable-p 'mode-name)
                   (local-variable-p 'buffer-undo-list)))"#,
    );
    assert_eq!(results[0], "OK (t t t)");
}

#[test]
fn local_variable_p_enforces_buffer_and_symbol_contracts() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(list
             (condition-case err (local-variable-p 'x) (error err))
             (condition-case err (local-variable-p 'x nil) (error err))
             (condition-case err (local-variable-p 'x (current-buffer)) (error err))
             (condition-case err (local-variable-p 'x 1) (error err))
             (condition-case err (local-variable-p 1 (current-buffer)) (error err))
             (condition-case err (local-variable-p :vm-k (current-buffer)) (error err))
             (condition-case err (local-variable-p nil (current-buffer)) (error err))
             (condition-case err (local-variable-p t (current-buffer)) (error err))
             (condition-case err (local-variable-p 'x (current-buffer) nil) (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK (nil nil nil (wrong-type-argument bufferp 1) (wrong-type-argument symbolp 1) nil nil nil (wrong-number-of-arguments local-variable-p 3))"
    );
}

#[test]
fn local_and_buffer_local_predicates_follow_alias_resolution() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defvaralias 'vm-local-p-alias 'vm-local-p-base)
           (let ((buf (get-buffer-create "vm-local-p-buf")))
             (set-buffer buf)
             (setq-local vm-local-p-alias 8)
             (list (local-variable-p 'vm-local-p-alias buf)
                   (local-variable-p 'vm-local-p-base buf)
                   (buffer-local-boundp 'vm-local-p-alias buf)
                   (buffer-local-boundp 'vm-local-p-base buf)))"#,
    );
    assert_eq!(results[1], "OK (t t t t)");
}

#[test]
fn buffer_local_bound_p_matches_emacs_shape() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defvar neomacs-buffer-local-boundp-global 1)
           (let ((buf (get-buffer-create "test-buf")))
             (buffer-local-boundp 'neomacs-buffer-local-boundp-global buf))
           (let ((buf (get-buffer-create "test-buf")))
             (buffer-local-boundp 'neomacs-buffer-local-boundp-missing buf))
           (let ((buf (get-buffer-create "test-buf-local")))
             (set-buffer buf)
             (make-local-variable 'neomacs-buffer-local-boundp-local)
             (setq neomacs-buffer-local-boundp-local 7)
             (buffer-local-boundp 'neomacs-buffer-local-boundp-local buf))
           (let ((buf (get-buffer-create "dead-buf")))
             (kill-buffer buf)
             (buffer-local-boundp 'neomacs-buffer-local-boundp-global buf))
           (condition-case err (buffer-local-boundp 1 (current-buffer)) (error (car err)))
           (condition-case err (buffer-local-boundp 'x nil) (error (car err)))
           (condition-case err (buffer-local-boundp 'x (current-buffer) nil)
             (error (car err)))"#,
    );
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK t");
    assert_eq!(results[4], "OK t");
    assert_eq!(results[5], "OK wrong-type-argument");
    assert_eq!(results[6], "OK wrong-type-argument");
    assert_eq!(results[7], "OK wrong-number-of-arguments");
}

// -- buffer-local-variables builtin ------------------------------------

#[test]
fn buffer_local_variables_include_default_entries() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(get-buffer-create "test-buf")
           (set-buffer "test-buf")
           (let ((locals (buffer-local-variables)))
             (and (listp locals)
                  (assq 'buffer-read-only locals)))"#,
    );
    assert_eq!(results[2], "OK (buffer-read-only)");
}

#[test]
fn buffer_local_variables_argument_validation() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(condition-case err (buffer-local-variables 1) (error err))
           (condition-case err (buffer-local-variables "test-buf") (error err))
           (condition-case err (buffer-local-variables nil nil) (error err))"#,
    );
    assert_eq!(results[0], "OK (wrong-type-argument bufferp 1)");
    assert_eq!(results[1], "OK (wrong-type-argument bufferp \"test-buf\")");
    assert_eq!(
        results[2],
        "OK (wrong-number-of-arguments buffer-local-variables 2)"
    );
}

// -- kill-local-variable builtin ----------------------------------------

#[test]
fn kill_local_variable_removes_binding() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(defvar my-var 42)
           (get-buffer-create "test-buf")
           (set-buffer "test-buf")
           (make-local-variable 'my-var)
           (local-variable-p 'my-var)
           (kill-local-variable 'my-var)
           (local-variable-p 'my-var)"#,
    );
    assert_eq!(results[4], "OK t");
    assert_eq!(results[6], "OK nil");
}

#[test]
fn kill_local_variable_resolves_alias_bindings() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defvaralias 'vm-klv-alias 'vm-klv-base)
           (with-temp-buffer
             (set (make-local-variable 'vm-klv-alias) 3)
             (kill-local-variable 'vm-klv-alias)
             (list (local-variable-p 'vm-klv-alias)
                   (local-variable-p 'vm-klv-base)
                   (condition-case err
                       (symbol-value 'vm-klv-alias)
                     (error (car err)))))"#,
    );
    assert_eq!(results[1], "OK (nil nil void-variable)");
}

#[test]
fn kill_local_variable_resets_forwarded_buffer_slot_to_default() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((orig (default-value 'fill-column)))
             (unwind-protect
                 (with-temp-buffer
                   (setq-default fill-column 71)
                   (setq-local fill-column 33)
                   (list fill-column
                         (default-value 'fill-column)
                         (progn
                           (kill-local-variable 'fill-column)
                           fill-column)
                         (local-variable-p 'fill-column)))
               (setq-default fill-column orig)))"#,
    );
    assert_eq!(results[0], "OK (33 71 71 nil)");
}

#[test]
fn kill_local_variable_accepts_keywords_like_oracle() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(list
             (condition-case err (with-temp-buffer (kill-local-variable nil)) (error err))
             (condition-case err (with-temp-buffer (kill-local-variable t)) (error err))
             (condition-case err (with-temp-buffer (kill-local-variable :vm-k)) (error err))
             (condition-case err (with-temp-buffer (kill-local-variable 1)) (error err)))"#,
    );
    assert_eq!(
        result[0],
        "OK (nil t :vm-k (wrong-type-argument symbolp 1))"
    );
}

#[test]
fn kill_local_variable_triggers_makunbound_watcher_with_buffer_where() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_all(
        r#"(progn
             (setq vm-klv-a-events nil)
             (fset 'vm-klv-a-rec
                   (lambda (symbol newval operation where)
                     (setq vm-klv-a-events
                           (cons (list symbol newval operation (bufferp where) (buffer-live-p where))
                                 vm-klv-a-events))))
             (defvaralias 'vm-klv-a-alias 'vm-klv-a-base)
             (add-variable-watcher 'vm-klv-a-base 'vm-klv-a-rec)
             (with-temp-buffer
               (set (make-local-variable 'vm-klv-a-alias) 7)
               (kill-local-variable 'vm-klv-a-alias))
             vm-klv-a-events)"#,
    );
    assert_eq!(
        result[0],
        "OK ((vm-klv-a-base nil makunbound t t) (vm-klv-a-base 7 set t t))"
    );
}

// -- custom-set-variables builtin --------------------------------------

#[test]
fn custom_set_variables_basic() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defvar my-var 1)
           (custom-set-variables '(my-var 42))
           (default-value 'my-var)"#,
    );
    assert_eq!(results[2], "OK 42");
}

#[test]
fn custom_set_variables_ignores_unknown_variable() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(custom-set-variables '(my-var 42))
           (condition-case err (default-value 'my-var) (error err))"#,
    );
    assert_eq!(results[1], "OK (void-variable my-var)");
}

// -- custom-set-faces --------------------------------------------------

#[test]
fn custom_set_faces_returns_nil() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(custom-set-faces '(default ((t (:height 120)))))"#);
    assert_eq!(results[0], "OK nil");
}

#[test]
fn custom_set_faces_non_list_spec_errors() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(condition-case err (custom-set-faces 1) (error err))"#);
    assert_eq!(results[0], r#"OK (error "Incompatible Custom theme spec")"#);
}

#[test]
fn custom_set_faces_requires_symbol_face_name() {
    crate::test_utils::init_test_tracing();
    let results =
        bootstrap_eval_all(r#"(condition-case err (custom-set-faces '(1 2)) (error err))"#);
    assert_eq!(results[0], "OK (wrong-type-argument symbolp 1)");
}

#[test]
fn custom_set_variables_errors_for_non_list_spec() {
    crate::test_utils::init_test_tracing();
    let results =
        bootstrap_eval_all(r#"(condition-case err (custom-set-variables 1) (error err))"#);
    assert_eq!(results[0], "OK (wrong-type-argument listp 1)");
}

#[test]
fn custom_set_variables_errors_for_non_symbol_variable_name() {
    crate::test_utils::init_test_tracing();
    let results =
        bootstrap_eval_all(r#"(condition-case err (custom-set-variables '(1 2)) (error err))"#);
    assert_eq!(results[0], "OK (wrong-type-argument symbolp 1)");
}

// -- Integration tests -------------------------------------------------

#[test]
fn defcustom_then_setq_default() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defcustom my-opt 10 "Opt." :type 'integer)
           (setq-default my-opt 20)
           my-opt"#,
    );
    assert_eq!(results[2], "OK 20");
}

#[test]
fn defvar_local_then_buffer_local_check() {
    crate::test_utils::init_test_tracing();
    let mut ev = bootstrap_context();
    let _ = ev.eval_str(
        r#"(defvar-local my-local-var 99)
           (make-variable-buffer-local 'other-var)"#,
    );
    assert!(ev.obarray().is_buffer_local("my-local-var"));
    // Phase D: is_auto_buffer_local mirror removed; verify via BLV local_if_set.
    let id_local = crate::emacs_core::intern::intern("my-local-var");
    let id_other = crate::emacs_core::intern::intern("other-var");
    assert!(ev.obarray().blv(id_local).map_or(false, |b| b.local_if_set));
    assert!(ev.obarray().blv(id_other).map_or(false, |b| b.local_if_set));
}

#[test]
fn defcustom_keyword_args_ignored_gracefully() {
    crate::test_utils::init_test_tracing();
    // Extra keywords like :initialize should not cause errors
    let results = bootstrap_eval_all(
        r#"(defcustom my-var 5 "Docs." :type 'integer :group 'editing :initialize 'custom-initialize-default) my-var"#,
    );
    assert_eq!(results[1], "OK 5");
}

#[test]
fn defgroup_multiple_groups() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(defgroup g1 nil "Group 1.")
           (defgroup g2 nil "Group 2.")
           (list (get 'g1 'group-documentation)
                 (get 'g2 'group-documentation))"#,
    );
    assert_eq!(results[2], "OK (\"Group 1.\" \"Group 2.\")");
}

#[test]
fn setq_default_works_on_new_variable() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(setq-default new-var 100) new-var"#);
    assert_eq!(results[1], "OK 100");
}
