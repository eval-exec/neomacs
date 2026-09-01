use super::*;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::intern::intern;
use crate::emacs_core::{Context, format_eval_result};
use crate::heap_types::LispString;
use crate::test_utils::load_minimal_gnu_backquote_runtime;
use std::fs;
use std::path::PathBuf;

fn eval_one(src: &str) -> String {
    let mut ev = Context::new();
    let result = ev.eval_str(src);
    format_eval_result(&result)
}

fn eval_all(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    // Root all parsed forms across the eval loop. The Vec<Value>
    // lives on the malloc heap and is invisible to conservative
    // stack scanning; without rooting, any intervening GC reclaims
    // the cons cells in the unrooted forms vec.
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

fn eval_first_gnu_form_after_marker(eval: &mut Context, source: &str, marker: &str) {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing GNU source marker: {marker}"));
    let (form, _) = crate::emacs_core::value_reader::read_one(&source[start..], 0, &test_ob())
        .unwrap_or_else(|err| panic!("parse GNU source from {marker} failed: {:?}", err))
        .unwrap_or_else(|| panic!("no GNU form found after marker: {marker}"));
    eval.eval_form(form)
        .unwrap_or_else(|err| panic!("evaluate GNU form {marker} failed: {:?}", err));
}

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
"#;
    ev.eval_str(shims).expect("install bare elisp shims");
}

fn load_minimal_autoload_runtime(ev: &mut Context) {
    install_bare_elisp_shims(ev);

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));

    let subr_source =
        fs::read_to_string(project_root.join("lisp/subr.el")).expect("read GNU subr.el");
    eval_first_gnu_form_after_marker(ev, &subr_source, "(defun special-form-p (object)");

    let byte_run_source = fs::read_to_string(project_root.join("lisp/emacs-lisp/byte-run.el"))
        .expect("read GNU byte-run.el");
    eval_first_gnu_form_after_marker(
        ev,
        &byte_run_source,
        "(defmacro eval-when-compile (&rest body)",
    );
    eval_first_gnu_form_after_marker(
        ev,
        &byte_run_source,
        "(defmacro eval-and-compile (&rest body)",
    );
}

fn minimal_autoload_eval_all(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    load_minimal_autoload_runtime(&mut ev);
    eval_all_with(&mut ev, src)
}

fn minimal_autoload_eval_one(src: &str) -> String {
    minimal_autoload_eval_all(src)
        .into_iter()
        .last()
        .expect("minimal autoload eval result")
}

fn minimal_backquote_runtime_eval_all(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    load_minimal_gnu_backquote_runtime(&mut ev);
    eval_all_with(&mut ev, src)
}

// -----------------------------------------------------------------------
// AutoloadManager unit tests
// -----------------------------------------------------------------------

#[test]
fn autoload_manager_register_and_lookup() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    assert!(!mgr.is_autoloaded("foo"));

    mgr.register(
        "foo",
        AutoloadEntry {
            file: LispString::from_utf8("foo-lib"),
            docstring: Some(LispString::from_utf8("Do foo things.")),
            interactive: false,
            autoload_type: AutoloadType::Function,
        },
    );

    assert!(mgr.is_autoloaded("foo"));
    let entry = mgr.get_entry("foo").unwrap();
    assert_eq!(entry.file.as_utf8_str(), Some("foo-lib"));
    assert_eq!(
        entry.docstring.as_ref().and_then(LispString::as_utf8_str),
        Some("Do foo things.")
    );
    assert!(!entry.interactive);
    assert_eq!(entry.autoload_type, AutoloadType::Function);
}

#[test]
fn autoload_manager_keeps_live_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    let name = intern("autoload-live-symbol");

    mgr.register_symbol(
        name,
        AutoloadEntry {
            file: LispString::from_utf8("autoload-live-symbol-file"),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Function,
        },
    );

    assert!(mgr.entries.contains_key(&name));
    assert_eq!(
        mgr.get_entry_symbol(name)
            .map(|entry| entry.file.as_utf8_str()),
        Some(Some("autoload-live-symbol-file"))
    );
}

#[test]
fn autoload_manager_remove() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.register(
        "bar",
        AutoloadEntry {
            file: LispString::from_utf8("bar-lib"),
            docstring: None,
            interactive: true,
            autoload_type: AutoloadType::Macro,
        },
    );
    assert!(mgr.is_autoloaded("bar"));
    mgr.remove("bar");
    assert!(!mgr.is_autoloaded("bar"));
}

#[test]
fn autoload_manager_multiple_entries() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.register(
        "a",
        AutoloadEntry {
            file: LispString::from_utf8("file-a"),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Function,
        },
    );
    mgr.register(
        "b",
        AutoloadEntry {
            file: LispString::from_utf8("file-b"),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Keymap,
        },
    );
    assert!(mgr.is_autoloaded("a"));
    assert!(mgr.is_autoloaded("b"));
    assert!(!mgr.is_autoloaded("c"));
}

#[test]
fn autoload_type_from_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        AutoloadType::from_value(&Value::NIL),
        AutoloadType::Function
    );
    assert_eq!(
        AutoloadType::from_value(&Value::symbol("macro")),
        AutoloadType::Macro
    );
    assert_eq!(
        AutoloadType::from_value(&Value::symbol("keymap")),
        AutoloadType::Keymap
    );
    assert_eq!(
        AutoloadType::from_value(&Value::symbol("unknown")),
        AutoloadType::Function
    );
}

#[test]
fn autoload_type_roundtrip() {
    crate::test_utils::init_test_tracing();
    let types = [
        AutoloadType::Function,
        AutoloadType::Macro,
        AutoloadType::Keymap,
    ];
    for ty in &types {
        let val = ty.to_value();
        let back = AutoloadType::from_value(&val);
        assert_eq!(&back, ty);
    }
}

#[test]
fn after_load_add_and_take() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.add_after_load("my-file", Value::fixnum(1));
    mgr.add_after_load("my-file", Value::fixnum(2));
    mgr.add_after_load("other-file", Value::fixnum(3));

    let forms = mgr.take_after_load_forms("my-file");
    assert_eq!(forms.len(), 2);

    // After taking, should be empty
    let forms2 = mgr.take_after_load_forms("my-file");
    assert!(forms2.is_empty());

    // Other file still has its form
    let forms3 = mgr.take_after_load_forms("other-file");
    assert_eq!(forms3.len(), 1);
}

#[test]
fn after_load_keys_canonicalize_ascii_storage_variants() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.add_after_load_key(
        AfterLoadKey::from_lisp_string(&crate::emacs_core::builtins::plain_str_to_lisp_string(
            "same-file",
            false,
        )),
        Value::fixnum(7),
    );
    let forms = mgr.take_after_load_forms("same-file");
    assert_eq!(forms, vec![Value::fixnum(7)]);
}

#[test]
fn autoload_manager_pdump_uses_symbol_and_lisp_identity() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    let autoload_name = intern("autoload-pdump-symbol");
    let obsolete_name = intern("autoload-obsolete-symbol");

    mgr.register_symbol(
        autoload_name,
        AutoloadEntry {
            file: LispString::from_utf8("autoload-pdump-file"),
            docstring: Some(LispString::from_utf8("autoload doc")),
            interactive: true,
            autoload_type: AutoloadType::Macro,
        },
    );
    mgr.make_obsolete_symbol(
        obsolete_name,
        LispString::from_utf8("replacement-symbol"),
        LispString::from_utf8("31.1"),
    );

    assert!(mgr.dump_entries().contains_key(&autoload_name));
    assert_eq!(
        mgr.dump_obsolete_functions()
            .get(&obsolete_name)
            .map(|(new_name, when)| (new_name.as_utf8_str(), when.as_utf8_str())),
        Some((Some("replacement-symbol"), Some("31.1")))
    );

    let legacy_dump = crate::emacs_core::pdump::types::DumpAutoloadManager {
        entries_syms: Vec::new(),
        entries: vec![(
            "legacy-autoload".to_string(),
            crate::emacs_core::pdump::types::DumpAutoloadEntry {
                file: crate::emacs_core::pdump::types::DumpLispString {
                    data: b"legacy-file".to_vec(),
                    size: "legacy-file".chars().count(),
                    size_byte: "legacy-file".len() as i64,
                },
                docstring: Some(crate::emacs_core::pdump::types::DumpLispString {
                    data: b"legacy-doc".to_vec(),
                    size: "legacy-doc".chars().count(),
                    size_byte: "legacy-doc".len() as i64,
                }),
                interactive: false,
                autoload_type: crate::emacs_core::pdump::types::DumpAutoloadType::Function,
            },
        )],
        after_load_lisp: Vec::new(),
        after_load: Vec::new(),
        loaded_files: Vec::new(),
        obsolete_functions_syms: Vec::new(),
        obsolete_functions: vec![(
            "legacy-obsolete".to_string(),
            ("legacy-new".to_string(), "30.1".to_string()),
        )],
        obsolete_variables_syms: Vec::new(),
        obsolete_variables: Vec::new(),
    };
    let empty_heap = crate::emacs_core::pdump::types::DumpTaggedHeap {
        objects: Vec::new(),
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut decoder = crate::emacs_core::pdump::convert::LoadDecoder::new(&empty_heap);
    let restored =
        crate::emacs_core::pdump::convert::load_autoload_manager(&mut decoder, &legacy_dump);
    assert!(restored.is_autoloaded_symbol(intern("legacy-autoload")));
    assert!(restored.is_function_obsolete_symbol(intern("legacy-obsolete")));
}

#[test]
fn loaded_files_tracking() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    assert!(!mgr.is_loaded("foo.el"));
    mgr.mark_loaded("foo.el");
    assert!(mgr.is_loaded("foo.el"));
    // Duplicate mark is harmless
    mgr.mark_loaded("foo.el");
    assert!(mgr.is_loaded("foo.el"));
}

#[test]
fn obsolete_function_tracking() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    assert!(!mgr.is_function_obsolete("old-fn"));
    mgr.make_obsolete("old-fn", "new-fn", "28.1");
    assert!(mgr.is_function_obsolete("old-fn"));
    let info = mgr.get_obsolete_function("old-fn").unwrap();
    assert_eq!(info.0.as_utf8_str(), Some("new-fn"));
    assert_eq!(info.1.as_utf8_str(), Some("28.1"));
}

#[test]
fn obsolete_variable_tracking() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    assert!(!mgr.is_variable_obsolete("old-var"));
    mgr.make_variable_obsolete("old-var", "new-var", "27.1");
    assert!(mgr.is_variable_obsolete("old-var"));
    let info = mgr.get_obsolete_variable("old-var").unwrap();
    assert_eq!(info.0.as_utf8_str(), Some("new-var"));
    assert_eq!(info.1.as_utf8_str(), Some("27.1"));
}

// -----------------------------------------------------------------------
// is_autoload_value tests
// -----------------------------------------------------------------------

#[test]
fn is_autoload_value_positive() {
    crate::test_utils::init_test_tracing();
    let val = Value::list(vec![Value::symbol("autoload"), Value::string("my-file")]);
    assert!(is_autoload_value(&val));
}

#[test]
fn is_autoload_value_matches_gnu_first_cell_check() {
    crate::test_utils::init_test_tracing();
    let dotted = Value::cons(Value::symbol("autoload"), Value::symbol("not-a-list"));
    assert!(is_autoload_value(&dotted));
}

#[test]
fn is_autoload_value_requires_canonical_autoload_symbol() {
    crate::test_utils::init_test_tracing();
    let uninterned_autoload =
        Value::symbol(crate::emacs_core::intern::intern_uninterned("autoload"));
    let val = Value::cons(uninterned_autoload, Value::symbol("not-a-list"));
    assert!(!is_autoload_value(&val));
}

#[test]
fn is_autoload_value_negative() {
    crate::test_utils::init_test_tracing();
    assert!(!is_autoload_value(&Value::NIL));
    assert!(!is_autoload_value(&Value::fixnum(42)));
    assert!(!is_autoload_value(&Value::list(vec![
        Value::symbol("lambda"),
        Value::NIL,
    ])));
}

// -----------------------------------------------------------------------
// Special form tests (eval-level)
// -----------------------------------------------------------------------

#[test]
fn autoload_special_form_registers() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(autoload 'my-func "my-file" "A function." t)
           (let ((f (symbol-function 'my-func)))
             (and (consp f) (eq (car f) 'autoload)))"#,
    );
    // autoload should return the function name as a symbol
    assert_eq!(results[0], "OK my-func");
    // The registered definition should be an autoload form.
    assert_eq!(results[1], "OK t");
}

#[test]
fn autoload_minimal_form() {
    crate::test_utils::init_test_tracing();
    // Minimal autoload: just function name and file
    let results = eval_all(
        r#"(autoload 'minimal-fn "min-file")
           (let ((f (symbol-function 'minimal-fn)))
             (and (consp f) (eq (car f) 'autoload)))"#,
    );
    assert_eq!(results[0], "OK minimal-fn");
    assert_eq!(results[1], "OK t");
}

#[test]
fn autoload_with_type() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(autoload 'my-macro "macro-file" nil nil 'macro)
           (let ((f (symbol-function 'my-macro)))
             (and (consp f) (eq (car f) 'autoload)))"#,
    );
    assert_eq!(results[0], "OK my-macro");
    assert_eq!(results[1], "OK t");
}

#[test]
fn autoload_do_load_passes_nomessage_to_load_source_file_function() {
    crate::test_utils::init_test_tracing();

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "neovm-autoload-nomessage-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create autoload test directory");
    fs::write(
        dir.join("vm-auto-file.el"),
        ";; loaded via load-source-file-function\n",
    )
    .expect("write autoload source file");

    let mut ev = Context::new();
    ev.set_variable(
        "load-path",
        Value::list(vec![Value::string(dir.to_string_lossy().as_ref())]),
    );
    ev.eval_str(
        r#"(progn
             (setq load-source-file-function
                   (lambda (fullname file noerror nomessage)
                     (setq vm-autoload-nomessage nomessage)
                     (fset 'vm-auto-target (lambda () 'ok))
                     t))
             (autoload 'vm-auto-target "vm-auto-file" nil t))"#,
    )
    .expect("install autoload test setup");

    let result = ev.eval_str(
        r#"(progn
             (autoload-do-load (symbol-function 'vm-auto-target) 'vm-auto-target)
             (list vm-autoload-nomessage (vm-auto-target)))"#,
    );

    assert_eq!(format_eval_result(&result), "OK (t ok)");
}

#[test]
fn named_call_follows_chained_autoloads() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("autoload chain tempdir");
    fs::write(
        dir.path().join("vm-autoload-chain-outer.el"),
        "(autoload 'vm-autoload-chain-run \"vm-autoload-chain-target\" nil t)\n",
    )
    .expect("write outer autoload fixture");
    fs::write(
        dir.path().join("vm-autoload-chain-target.el"),
        "(defalias 'vm-autoload-chain-run #'(lambda (value) (+ value 7)))\n",
    )
    .expect("write chained autoload target fixture");

    let mut eval = Context::new();
    eval.set_variable(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    let result = eval.eval_str(
        r#"(progn
             (autoload 'vm-autoload-chain-run "vm-autoload-chain-outer" nil t)
             (funcall 'vm-autoload-chain-run 5))"#,
    );

    assert_eq!(format_eval_result(&result), "OK 12");
}

#[test]
fn named_call_retries_to_void_after_autoload_removes_definition() {
    crate::test_utils::init_test_tracing();

    let dir = tempfile::tempdir().expect("autoload removal tempdir");
    fs::write(
        dir.path().join("vm-autoload-removes-definition.el"),
        "(fmakunbound 'vm-autoload-removed)\n",
    )
    .expect("write autoload removal fixture");

    let mut eval = Context::new();
    eval.set_variable(
        "load-path",
        Value::list(vec![Value::string(dir.path().to_string_lossy())]),
    );
    let result = eval.eval_str(
        r#"(progn
             (autoload 'vm-autoload-removed
                       "vm-autoload-removes-definition" nil t)
             (condition-case error
                 (funcall 'vm-autoload-removed)
               (error error)))"#,
    );

    assert_eq!(
        format_eval_result(&result),
        "OK (void-function vm-autoload-removed)"
    );
}

#[test]
fn require_passes_nomessage_to_load_source_file_function() {
    crate::test_utils::init_test_tracing();

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "neovm-require-nomessage-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create require test directory");
    fs::write(
        dir.join("vm-require-file.el"),
        ";; loaded via load-source-file-function\n",
    )
    .expect("write require source file");

    let mut ev = Context::new();
    ev.set_variable(
        "load-path",
        Value::list(vec![Value::string(dir.to_string_lossy().as_ref())]),
    );
    let result = ev.eval_str(
        r#"(progn
             (setq load-source-file-function
                   (lambda (fullname file noerror nomessage)
                     (setq vm-require-nomessage nomessage)
                     (provide 'vm-require-feature)
                     t))
             (list (require 'vm-require-feature "vm-require-file")
                   vm-require-nomessage))"#,
    );

    assert_eq!(format_eval_result(&result), "OK (vm-require-feature t)");
}

#[test]
fn autoload_do_load_checks_funname_before_loading_macro_autoload() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((macro-autoload '(autoload "missing-macro-file" nil nil macro))
                 (function-autoload '(autoload "missing-function-file" nil nil nil)))
             (list
              (eq (autoload-do-load function-autoload 42 'macro)
                  function-autoload)
              (condition-case err
                  (autoload-do-load macro-autoload 42 'macro)
                (error (list (car err) (cdr err))))))"#,
    );

    assert_eq!(results[0], "OK (t (wrong-type-argument (symbolp 42)))");
}

#[test]
fn autoload_is_callable_subr_surface() {
    crate::test_utils::init_test_tracing();
    let results = minimal_autoload_eval_all(
        r#"(fboundp 'autoload)
           (special-form-p 'autoload)
           (subrp (symbol-function 'autoload))
           (subr-arity (symbol-function 'autoload))
           (func-arity 'autoload)
           (funcall 'autoload 'my-funcall-fn "my-funcall-file")
           (let ((f (symbol-function 'my-funcall-fn)))
             (and (consp f) (eq (car f) 'autoload)))"#,
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK (2 . 5)");
    assert_eq!(results[4], "OK (2 . 5)");
    assert_eq!(results[5], "OK my-funcall-fn");
    assert_eq!(results[6], "OK t");
}

#[test]
fn autoload_rejects_too_many_arguments() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err
              (autoload 'too-many "x" nil nil nil nil)
            (error (list (car err) (cdr err))))"#,
    );
    assert_eq!(result, "OK (wrong-number-of-arguments (autoload 6))");
}

#[test]
fn autoload_funcall_type_checks_first_argument() {
    crate::test_utils::init_test_tracing();
    let result = eval_one(
        r#"(condition-case err
              (funcall 'autoload 1 "x")
            (error (list (car err) (cdr err))))"#,
    );
    assert_eq!(result, "OK (wrong-type-argument (symbolp 1))");
}

#[test]
fn eval_when_compile_evaluates_body() {
    crate::test_utils::init_test_tracing();
    let result = minimal_autoload_eval_one("(eval-when-compile (+ 1 2))");
    assert_eq!(result, "OK 3");
}

#[test]
fn eval_when_compile_multiple_forms() {
    crate::test_utils::init_test_tracing();
    let result = minimal_autoload_eval_one("(eval-when-compile 1 2 (+ 3 4))");
    assert_eq!(result, "OK 7");
}

#[test]
fn eval_when_compile_propagates_errors() {
    crate::test_utils::init_test_tracing();
    let result = minimal_autoload_eval_one(
        r#"(condition-case err
              (eval-when-compile (signal 'error '("boom")))
            (error (list (car err) (cdr err))))"#,
    );
    assert_eq!(result, r#"OK (error ("boom"))"#);
}

#[test]
fn eval_and_compile_evaluates_body() {
    crate::test_utils::init_test_tracing();
    let result = minimal_autoload_eval_one("(eval-and-compile (+ 10 20))");
    assert_eq!(result, "OK 30");
}

#[test]
fn eval_and_compile_multiple_forms() {
    crate::test_utils::init_test_tracing();
    // Should return the last form's value
    let result = minimal_autoload_eval_one("(eval-and-compile (setq x 1) (setq y 2) (+ x y))");
    assert_eq!(result, "OK 3");
}

/// `symbol-file' is Lisp -- lisp/subr.el:3351, DIVERGENCES.md 152 -- so a
/// bare evaluator has nothing to ask.  What THIS module owns is the function
/// cell `autoload' writes, which is also what GNU's `symbol-file' reads:
/// `(nth 1 (symbol-function symbol))'.
///
/// Measured on GNU 31.0.90 `-Q --batch' first.
#[test]
fn autoload_writes_the_file_into_the_function_cell_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(autoload 'sym-file-probe "sym-file-probe-file")
           (symbol-function 'sym-file-probe)
           (nth 1 (symbol-function 'sym-file-probe))
           (fboundp 'symbol-file)"#,
    );
    assert_eq!(
        results[1],
        r#"OK (autoload "sym-file-probe-file" nil nil nil)"#
    );
    assert_eq!(results[2], r#"OK "sym-file-probe-file""#);
    // GNU's src/ has no DEFUN of this name, so a bare evaluator has none.
    assert_eq!(results[3], "OK nil");
}

#[test]
fn autoload_entry_interactive_flag() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.register(
        "cmd",
        AutoloadEntry {
            file: LispString::from_utf8("cmd-file"),
            docstring: None,
            interactive: true,
            autoload_type: AutoloadType::Function,
        },
    );
    let entry = mgr.get_entry("cmd").unwrap();
    assert!(entry.interactive);
}

#[test]
fn autoload_entry_keymap_type() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.register(
        "my-map",
        AutoloadEntry {
            file: LispString::from_utf8("map-file"),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Keymap,
        },
    );
    let entry = mgr.get_entry("my-map").unwrap();
    assert_eq!(entry.autoload_type, AutoloadType::Keymap);
}

#[test]
fn autoload_overwrites_previous() {
    crate::test_utils::init_test_tracing();
    let mut mgr = AutoloadManager::new();
    mgr.register(
        "f",
        AutoloadEntry {
            file: LispString::from_utf8("old-file"),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Function,
        },
    );
    mgr.register(
        "f",
        AutoloadEntry {
            file: LispString::from_utf8("new-file"),
            docstring: None,
            interactive: true,
            autoload_type: AutoloadType::Macro,
        },
    );
    let entry = mgr.get_entry("f").unwrap();
    assert_eq!(entry.file.as_utf8_str(), Some("new-file"));
    assert!(entry.interactive);
    assert_eq!(entry.autoload_type, AutoloadType::Macro);
}

/// GNU Emacs: "If FUNCTION is already defined other than as an autoload,
/// this does nothing and returns nil."
#[test]
fn autoload_does_not_override_real_definition() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(defalias 'already-defined #'(lambda () 42))
           (autoload 'already-defined "some-file")
           ;; autoload should return nil (skipped)
           ;; and the real definition should still be in place
           (already-defined)"#,
    );
    // autoload on an already-defined function returns nil
    assert_eq!(results[1], "OK nil");
    // Real definition still works
    assert_eq!(results[2], "OK 42");
}

#[test]
fn autoload_registers_in_autoload_manager() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(autoload 'test-auto-fn "test-auto-file" "Test doc" t 'macro)"#,
    );
    assert_eq!(results[0], "OK test-auto-fn");
    assert!(ev.autoloads.is_autoloaded("test-auto-fn"));
    let entry = ev.autoloads.get_entry("test-auto-fn").unwrap();
    assert_eq!(entry.file.as_utf8_str(), Some("test-auto-file"));
    assert_eq!(
        entry.docstring.as_ref().and_then(LispString::as_utf8_str),
        Some("Test doc")
    );
    assert!(entry.interactive);
    assert_eq!(entry.autoload_type, AutoloadType::Macro);
}

#[test]
fn autoload_uses_defalias_hook_and_indexes_the_resulting_function_cell() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = eval_all_with(
        &mut ev,
        r#"(setq vm-autoload-hook-log nil)
           (put 'vm-hooked-autoload 'defalias-fset-function
                (lambda (symbol definition)
                  (setq vm-autoload-hook-log (list symbol definition))
                  (fset symbol definition)))
           (autoload 'vm-hooked-autoload "vm-hooked-file" nil t)
           vm-autoload-hook-log"#,
    );

    assert_eq!(results[2], "OK vm-hooked-autoload");
    assert_eq!(
        results[3],
        "OK (vm-hooked-autoload (autoload \"vm-hooked-file\" nil t nil))"
    );
    assert!(ev.autoloads.is_autoloaded("vm-hooked-autoload"));
}

#[test]
fn autoload_load_history_distinguishes_runtime_from_dump_definitions() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((current-load-list (list "runtime-autoloads.el"))
                 (dump-mode nil))
             (autoload 'vm-runtime-autoload "vm-runtime-file")
             current-load-list)
           (let ((current-load-list (list "bootstrap-autoloads.el"))
                 (dump-mode "pdump"))
             (autoload 'vm-bootstrap-autoload "vm-bootstrap-file")
             current-load-list)"#,
    );

    assert_eq!(
        results[0],
        "OK ((defun . vm-runtime-autoload) \"runtime-autoloads.el\")"
    );
    assert_eq!(results[1], "OK (\"bootstrap-autoloads.el\")");
}

#[test]
fn autoload_sets_uninterned_symbol_function_cell_by_identity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let sym = ev
        .eval_str(r#"(make-symbol "neo-auto")"#)
        .expect("make uninterned symbol");

    builtin_autoload(
        &mut ev,
        vec![
            sym,
            Value::string("nofile"),
            Value::string("doc"),
            Value::T,
            Value::NIL,
        ],
    )
    .expect("autoload uninterned symbol");

    let function = ev
        .obarray
        .symbol_function_of_value(&sym)
        .expect("uninterned symbol function cell");
    assert!(is_autoload_value(&function));
    let cell = list_to_vec(&function).expect("autoload cell should be a list");
    assert_eq!(cell[1], Value::string("nofile"));
}

#[test]
fn autoload_accepts_symbol_with_pos_only_when_enabled() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let positioned = ev
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("test-auto-pos-fn"), Value::fixnum(17));

    let disabled = builtin_autoload(
        &mut ev,
        vec![positioned, Value::string("test-auto-pos-file")],
    );
    assert!(
        disabled.is_err(),
        "autoload should reject symbol-with-pos while GNU transparency is disabled"
    );

    ev.set_variable("symbols-with-pos-enabled", Value::T);
    let enabled = builtin_autoload(
        &mut ev,
        vec![positioned, Value::string("test-auto-pos-file")],
    )
    .expect("autoload should accept symbol-with-pos when GNU transparency is enabled");
    assert_eq!(enabled, Value::symbol("test-auto-pos-fn"));
    assert!(ev.autoloads.is_autoloaded("test-auto-pos-fn"));
}

#[test]
fn autoload_preserves_a_raw_unibyte_file_in_the_function_cell() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let raw_file = Value::heap_string(LispString::from_unibyte(vec![0xFF]));

    builtin_autoload(
        &mut ev,
        vec![
            Value::symbol("raw-autoload"),
            raw_file,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("register raw autoload");

    let function = ev
        .obarray
        .symbol_function("raw-autoload")
        .expect("raw-autoload function cell");
    let cell = list_to_vec(&function).expect("autoload cell should be a list");
    let text = cell[1].as_lisp_string().expect("raw autoload file string");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

// -----------------------------------------------------------------------
// eval-after-load / provide integration
// -----------------------------------------------------------------------

#[test]
fn eval_after_load_deferred_fires_on_provide() {
    crate::test_utils::init_test_tracing();
    // Register eval-after-load BEFORE providing the feature.
    // When provide is called, the deferred callback should fire.
    let results = minimal_backquote_runtime_eval_all(
        r#"(defvar neovm--eal-test-log nil)
           (eval-after-load 'neovm--eal-test-feat
             '(setq neovm--eal-test-log (cons 'deferred neovm--eal-test-log)))
           neovm--eal-test-log
           (provide 'neovm--eal-test-feat)
           neovm--eal-test-log"#,
    );
    // Before provide: log should be nil
    assert_eq!(results[2], "OK nil");
    // After provide: callback should have fired
    assert_eq!(results[4], "OK (deferred)");
}

#[test]
fn eval_after_load_immediate_fires_when_already_provided() {
    crate::test_utils::init_test_tracing();
    // When eval-after-load is called for an already-provided feature,
    // the callback should fire immediately.
    let results = minimal_backquote_runtime_eval_all(
        r#"(defvar neovm--eal-imm-log nil)
           (provide 'neovm--eal-imm-feat)
           (eval-after-load 'neovm--eal-imm-feat
             '(setq neovm--eal-imm-log (cons 'immediate neovm--eal-imm-log)))
           neovm--eal-imm-log"#,
    );
    // Should have fired immediately
    assert_eq!(results[3], "OK (immediate)");
}

#[test]
fn with_eval_after_load_fires_when_already_provided() {
    crate::test_utils::init_test_tracing();
    // with-eval-after-load macro wraps body in a lambda and calls eval-after-load.
    let results = minimal_backquote_runtime_eval_all(
        r#"(defvar neovm--weal-test-result nil)
           (provide 'neovm--weal-test-feat)
           (with-eval-after-load 'neovm--weal-test-feat
             (setq neovm--weal-test-result 'executed))
           neovm--weal-test-result"#,
    );
    assert_eq!(results[3], "OK executed");
}
