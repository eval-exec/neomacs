//! Common test utilities for neovm-core.
//!
//! Provides shared helpers used across all test modules.

use crate::emacs_core::error::map_flow;
fn test_ob() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}
use crate::emacs_core::load::{
    apply_ldefs_boot_autoloads_for_names, bootstrap_load_path_entries,
    create_runtime_startup_evaluator_cached, find_file_in_load_path, get_load_path, load_file,
};
use crate::emacs_core::value::Value;
use crate::emacs_core::{Context, format_eval_result};
use std::path::PathBuf;

/// Initialize the tracing subscriber for test output.
///
/// Thin wrapper around [`crate::logging::init_for_tests`] kept for the
/// many existing call sites in this crate's test files. Tests never
/// write to a log file regardless of `NEOMACS_LOG_TO_FILE` — output is
/// always routed through the test harness's writer.
///
/// # Usage
/// Call at the start of any test that needs tracing:
/// ```rust,ignore
/// #[test]
/// fn my_test() {
///     crate::test_utils::init_test_tracing();
///     // ... test code ...
/// }
/// ```
pub fn init_test_tracing() {
    crate::logging::init_for_tests();
}

/// Load a small GNU Lisp runtime that is sufficient for tests that need
/// `byte-run`, backquote expansion, and the basic `subr.el` support layer,
/// without paying for full `loadup.el` startup.
pub fn load_minimal_gnu_backquote_runtime(eval: &mut Context) {
    eval.set_lexical_binding(true);
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let lisp_dir = project_root.join("lisp");
    eval.set_variable(
        "load-path",
        Value::list(bootstrap_load_path_entries(&lisp_dir)),
    );
    let load_path = get_load_path(&eval.obarray(), eval.buffers.current_buffer());
    for name in &[
        "emacs-lisp/debug-early",
        "emacs-lisp/byte-run",
        "emacs-lisp/backquote",
        "subr",
    ] {
        let path = find_file_in_load_path(name, &load_path)
            .unwrap_or_else(|| panic!("cannot find {name}"));
        load_file(eval, &path).unwrap_or_else(|err| panic!("load {name}: {err:?}"));
    }
}

/// Load GNU `macroexp.el` after the early `subr.el` layer, mirroring the
/// loadup phase before later Lisp files such as `simple.el` are evaluated.
pub fn load_gnu_macroexp_runtime(eval: &mut Context) {
    if eval.obarray().symbol_function("macroexp-progn").is_some() {
        return;
    }
    let load_path = get_load_path(&eval.obarray(), eval.buffers.current_buffer());
    for name in &["emacs-lisp/macroexp", "emacs-lisp/pcase"] {
        let path = find_file_in_load_path(name, &load_path)
            .unwrap_or_else(|| panic!("cannot find {name}"));
        load_file(eval, &path).unwrap_or_else(|err| panic!("load {name}: {err:?}"));
    }
}

/// Load the GNU `simple.el` undo auto-amalgamation surface needed by
/// primitives such as `delete-char` and `self-insert-command`.
///
/// GNU cmds.c calls `undo-auto-amalgamate` unconditionally; that function is
/// Lisp-defined by `simple.el` during loadup. Some focused unit tests do not
/// run loadup, so this evaluates only the real source forms that provide that
/// function and its helper variables instead of guarding the primitive. It does
/// not install `undo-auto--undoable-change`, which also requires `timer.el`.
pub fn load_gnu_undo_auto_runtime(eval: &mut Context) {
    if eval
        .obarray()
        .symbol_function("undo-auto-amalgamate")
        .is_some()
    {
        return;
    }
    if eval.obarray().symbol_function("defvar-local").is_none() {
        load_minimal_gnu_backquote_runtime(eval);
    }
    load_gnu_macroexp_runtime(eval);

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source =
        std::fs::read_to_string(&simple_path).unwrap_or_else(|err| panic!("read simple.el: {err}"));
    let limit_start = simple_source
        .find("(defvar amalgamating-undo-limit ")
        .expect("simple.el amalgamating-undo-limit form");
    let limit_form =
        crate::emacs_core::value_reader::read_one(&simple_source, limit_start, &test_ob())
            .expect("parse simple.el amalgamating-undo-limit")
            .map(|(form, _)| form)
            .expect("read simple.el amalgamating-undo-limit");

    let start = simple_source
        .find("(defvar-local undo-auto--last-boundary-cause ")
        .expect("simple.el undo auto section start");
    let end = simple_source[start..]
        .find("(defun undo-auto--undoable-change ")
        .map(|offset| start + offset)
        .expect("simple.el undo auto section end");
    let mut forms = vec![limit_form];
    forms.extend(
        crate::emacs_core::value_reader::read_all(&simple_source[start..end], &test_ob())
            .expect("parse simple.el undo auto section"),
    );

    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    for (index, form) in forms.into_iter().enumerate() {
        eval.eval_sub(form).map_err(map_flow).unwrap_or_else(|err| {
            panic!(
                "eval simple.el undo auto section form #{index} {}: {err}",
                crate::emacs_core::print::print_value(&form)
            )
        });
    }
    eval.restore_specpdl_roots(roots);
}

/// Load the real GNU `simple.el` special-mode surface required by help buffers.
pub fn load_gnu_special_mode_runtime(eval: &mut Context) {
    if eval.obarray().symbol_value("special-mode-map").is_some()
        && eval.obarray().symbol_function("special-mode").is_some()
    {
        return;
    }

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source =
        std::fs::read_to_string(&simple_path).unwrap_or_else(|err| panic!("read simple.el: {err}"));
    let start = simple_source
        .find("(defun fundamental-mode ()")
        .expect("simple.el fundamental-mode form");
    let end = simple_source[start..]
        .find(";; Making and deleting lines.")
        .map(|offset| start + offset)
        .expect("simple.el special-mode section end");
    let forms = crate::emacs_core::value_reader::read_all(&simple_source[start..end], &test_ob())
        .expect("parse simple.el special-mode section");

    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    for (index, form) in forms.into_iter().enumerate() {
        eval.eval_sub(form).map_err(map_flow).unwrap_or_else(|err| {
            panic!(
                "eval simple.el special-mode section form #{index} {}: {err}",
                crate::emacs_core::print::print_value(&form)
            )
        });
    }
    eval.restore_specpdl_roots(roots);
}

/// Load the real GNU display predicate used by help separators.
pub fn load_gnu_display_graphic_runtime(eval: &mut Context) {
    if eval
        .obarray()
        .symbol_function("display-graphic-p")
        .is_some()
    {
        return;
    }

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let frame_path = project_root.join("lisp/frame.el");
    let frame_source =
        std::fs::read_to_string(&frame_path).unwrap_or_else(|err| panic!("read frame.el: {err}"));
    let framep_start = frame_source
        .find("(defun framep-on-display ")
        .expect("frame.el framep-on-display form");
    let framep_form =
        crate::emacs_core::value_reader::read_one(&frame_source, framep_start, &test_ob())
            .expect("parse frame.el framep-on-display")
            .map(|(form, _)| form)
            .expect("read frame.el framep-on-display");
    let display_start = frame_source
        .find("(defun display-graphic-p ")
        .expect("frame.el display-graphic-p form");
    let display_form =
        crate::emacs_core::value_reader::read_one(&frame_source, display_start, &test_ob())
            .expect("parse frame.el display-graphic-p")
            .map(|(form, _)| form)
            .expect("read frame.el display-graphic-p");
    let forms = vec![framep_form, display_form];

    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    for (index, form) in forms.into_iter().enumerate() {
        eval.eval_sub(form).map_err(map_flow).unwrap_or_else(|err| {
            panic!(
                "eval frame.el display predicate form #{index} {}: {err}",
                crate::emacs_core::print::print_value(&form)
            )
        });
    }
    eval.restore_specpdl_roots(roots);
}

/// Load GNU aliases from `window.el` for C-defined window primitives.
pub fn load_gnu_window_alias_runtime(eval: &mut Context) {
    if eval.obarray().symbol_function("window-width").is_some() {
        return;
    }

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let window_path = project_root.join("lisp/window.el");
    let window_source =
        std::fs::read_to_string(&window_path).unwrap_or_else(|err| panic!("read window.el: {err}"));
    let start = window_source
        .find("(defalias 'window-height ")
        .expect("window.el window primitive alias block");
    let end = window_source[start..]
        .find("(defun window-full-height-p ")
        .map(|offset| start + offset)
        .expect("window.el window primitive alias block end");
    let forms = crate::emacs_core::value_reader::read_all(&window_source[start..end], &test_ob())
        .expect("parse window.el window primitive alias block");

    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    for (index, form) in forms.into_iter().enumerate() {
        eval.eval_sub(form).map_err(map_flow).unwrap_or_else(|err| {
            panic!(
                "eval window.el primitive alias form #{index} {}: {err}",
                crate::emacs_core::print::print_value(&form)
            )
        });
    }
    eval.restore_specpdl_roots(roots);
}

/// Load the real GNU separator-line helper used by help buffers.
pub fn load_gnu_separator_line_runtime(eval: &mut Context) {
    if eval
        .obarray()
        .symbol_function("make-separator-line")
        .is_some()
    {
        return;
    }

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let simple_path = project_root.join("lisp/simple.el");
    let simple_source =
        std::fs::read_to_string(&simple_path).unwrap_or_else(|err| panic!("read simple.el: {err}"));
    let start = simple_source
        .find("(defface separator-line")
        .expect("simple.el separator-line face form");
    let end = simple_source[start..]
        .find("(defun delete-indentation ")
        .map(|offset| start + offset)
        .expect("simple.el make-separator-line section end");
    let forms = crate::emacs_core::value_reader::read_all(&simple_source[start..end], &test_ob())
        .expect("parse simple.el make-separator-line section");

    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    for (index, form) in forms.into_iter().enumerate() {
        eval.eval_sub(form).map_err(map_flow).unwrap_or_else(|err| {
            panic!(
                "eval simple.el make-separator-line section form #{index} {}: {err}",
                crate::emacs_core::print::print_value(&form)
            )
        });
    }
    eval.restore_specpdl_roots(roots);
}

/// Load the real GNU Emacs Lisp syntax table used by help-mode.
pub fn load_gnu_elisp_syntax_table_runtime(eval: &mut Context) {
    if eval
        .obarray()
        .symbol_value("emacs-lisp-mode-syntax-table")
        .is_some()
    {
        return;
    }

    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let elisp_mode_path = project_root.join("lisp/progmodes/elisp-mode.el");
    let elisp_mode_source = std::fs::read_to_string(&elisp_mode_path)
        .unwrap_or_else(|err| panic!("read elisp-mode.el: {err}"));
    let start = elisp_mode_source
        .find("(defvar emacs-lisp-mode-syntax-table")
        .expect("elisp-mode.el emacs-lisp-mode-syntax-table form");
    let form = crate::emacs_core::value_reader::read_one(&elisp_mode_source, start, &test_ob())
        .expect("parse elisp-mode.el emacs-lisp-mode-syntax-table")
        .map(|(form, _)| form)
        .expect("read elisp-mode.el emacs-lisp-mode-syntax-table");

    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(form);
    eval.eval_sub(form).map_err(map_flow).unwrap_or_else(|err| {
        panic!(
            "eval elisp-mode.el emacs-lisp-mode-syntax-table {}: {err}",
            crate::emacs_core::print::print_value(&form)
        )
    });
    eval.restore_specpdl_roots(roots);
}

/// Load a small GNU Lisp runtime that is sufficient for `help.el`
/// semantics such as `substitute-command-keys`, without paying for
/// full `loadup.el` startup.
pub fn load_minimal_gnu_help_runtime(eval: &mut Context) {
    load_minimal_gnu_backquote_runtime(eval);
    let load_path = get_load_path(&eval.obarray(), eval.buffers.current_buffer());
    for name in &[
        "keymap",
        "widget",
        "custom",
        "cus-face",
        "faces",
        "bindings",
        "emacs-lisp/macroexp",
        "emacs-lisp/pcase",
        "emacs-lisp/gv",
    ] {
        let path = find_file_in_load_path(name, &load_path)
            .unwrap_or_else(|| panic!("cannot find {name}"));
        load_file(eval, &path).unwrap_or_else(|err| panic!("load {name}: {err:?}"));
    }
    apply_ldefs_boot_autoloads_for_names(
        eval,
        &[
            "define-derived-mode",
            "define-inline",
            "define-minor-mode",
            "help-fns-function-name",
            "regexp-opt",
            "rx",
        ],
    )
    .expect("ldefs-boot help runtime autoloads");
    for name in &[
        "emacs-lisp/cl-preloaded",
        "emacs-lisp/oclosure",
        "obarray",
        "abbrev",
        "emacs-lisp/cl-generic",
        "emacs-lisp/seq",
        "emacs-lisp/easy-mmode",
        "emacs-lisp/derived",
        "emacs-lisp/easymenu",
        "button",
        "help-macro",
    ] {
        let path = find_file_in_load_path(name, &load_path)
            .unwrap_or_else(|| panic!("cannot find {name}"));
        load_file(eval, &path).unwrap_or_else(|err| panic!("load {name}: {err:?}"));
    }
    load_gnu_special_mode_runtime(eval);
    load_gnu_display_graphic_runtime(eval);
    load_gnu_window_alias_runtime(eval);
    load_gnu_separator_line_runtime(eval);
    for name in &["progmodes/prog-mode", "emacs-lisp/lisp-mode", "tool-bar"] {
        let path = find_file_in_load_path(name, &load_path)
            .unwrap_or_else(|| panic!("cannot find {name}"));
        load_file(eval, &path).unwrap_or_else(|err| panic!("load {name}: {err:?}"));
    }
    load_gnu_elisp_syntax_table_runtime(eval);
    // Force the .el source — `find_file_in_load_path("help", ...)`
    // returns help.elc when both exist, but `read_to_string` then
    // mis-parses .elc binary data and emits `(nil . OFFSET)` doc
    // refs that downstream `defface` rejects. Passing "help.el"
    // explicitly bypasses the suffix preference loop.
    let help_path = find_file_in_load_path("help.el", &load_path).expect("cannot find help.el");
    let help_source =
        std::fs::read_to_string(&help_path).unwrap_or_else(|err| panic!("read help.el: {err}"));
    let help_forms =
        crate::emacs_core::value_reader::read_all(&help_source, &test_ob()).expect("parse help.el");
    // Root every parsed form upfront. Without this, forms still
    // sitting in the `help_forms` Vec aren't visible to the GC and
    // can be reclaimed when an `eval_sub` of an earlier form
    // triggers a collection. Mirrors the rooting pattern in
    // `Context::eval_str_each` (eval.rs:6170-6183).
    let roots = eval.save_specpdl_roots();
    for form in &help_forms {
        eval.push_specpdl_root(*form);
    }
    let mut found_substitute_command_keys = false;
    let mut found_describe_map_fill_columns = false;
    for form in &help_forms {
        let is_substitute_command_keys = is_named_defun_value(form, "substitute-command-keys");
        let is_describe_map_fill_columns = is_named_defun_value(form, "describe-map--fill-columns");
        eval.eval_sub(*form)
            .map_err(map_flow)
            .unwrap_or_else(|err| panic!("eval help.el prefix: {err:?}"));
        if is_substitute_command_keys {
            found_substitute_command_keys = true;
        }
        if is_describe_map_fill_columns {
            found_describe_map_fill_columns = true;
            break;
        }
    }
    eval.restore_specpdl_roots(roots);
    assert!(
        found_substitute_command_keys,
        "help.el should define substitute-command-keys"
    );
    assert!(
        found_describe_map_fill_columns,
        "help.el should define describe-map--fill-columns"
    );
    load_gnu_undo_auto_runtime(eval);
}

fn is_named_defun_value(form: &Value, name: &str) -> bool {
    if !form.is_cons() {
        return false;
    }
    let car = form.cons_car();
    if !car.is_symbol_named("defun") {
        return false;
    }
    let cdr = form.cons_cdr();
    if !cdr.is_cons() {
        return false;
    }
    cdr.cons_car().is_symbol_named(name)
}

/// Create a bare evaluator with GNU `ldefs-boot.el` autoload cells restored
/// for the named symbols and a bootstrap-compatible `load-path`.
pub fn eval_with_ldefs_boot_autoloads(names: &[&str]) -> Context {
    let mut eval = Context::new();
    let project_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let lisp_dir = project_root.join("lisp");
    eval.set_variable(
        "load-path",
        Value::list(bootstrap_load_path_entries(&lisp_dir)),
    );
    for name in names {
        eval.obarray_mut().fmakunbound(name);
    }
    apply_ldefs_boot_autoloads_for_names(&mut eval, names).expect("ldefs-boot autoload restore");
    eval
}

/// Create a cached runtime-startup evaluator for tests that need the full
/// GNU bootstrap surface.
pub fn runtime_startup_context() -> Context {
    create_runtime_startup_evaluator_cached().expect("bootstrap")
}

/// Evaluate FORMS in a cached runtime-startup evaluator and return formatted
/// results, matching the common bootstrap test pattern.
pub fn runtime_startup_eval_all(src: &str) -> Vec<String> {
    let mut eval = runtime_startup_context();
    let forms = crate::emacs_core::value_reader::read_all(src, &test_ob()).expect("parse");
    // Root parsed forms across the eval loop. Heap literals like bignums,
    // strings, and cons cells are otherwise invisible to the GC until the
    // evaluator reaches them, which can corrupt bootstrap tests that call
    // into bytecode and trigger collection mid-eval.
    let roots = eval.save_specpdl_roots();
    for form in &forms {
        eval.push_specpdl_root(*form);
    }
    let results = forms
        .into_iter()
        .map(|form| {
            let result = eval.eval_form(form);
            format_eval_result(&result)
        })
        .collect();
    eval.restore_specpdl_roots(roots);
    results
}

/// Evaluate the first form from SRC in a cached runtime-startup evaluator and
/// return the formatted result.
pub fn runtime_startup_eval_one(src: &str) -> String {
    let mut eval = runtime_startup_context();
    let result = eval.eval_str(src);
    format_eval_result(&result)
}
