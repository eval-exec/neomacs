use super::*;
use crate::intern::intern;

// =======================================================================
// ErrorRegistry (standalone HashMap-based) tests
// =======================================================================

#[test]
fn registry_new_has_standard_errors() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(reg.parents.contains_key(&intern("error")));
    assert!(reg.parents.contains_key(&intern("void-variable")));
    assert!(reg.parents.contains_key(&intern("file-missing")));
    assert!(reg.parents.contains_key(&intern("overflow-error")));
}

#[test]
fn registry_direct_match() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(reg.signal_matches_condition_sym(intern("void-variable"), intern("void-variable")));
    assert!(reg.signal_matches_condition("void-variable", "void-variable"));
}

#[test]
fn registry_parent_match() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(reg.signal_matches_condition("void-variable", "error"));
}

#[test]
fn registry_grandparent_match() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    // overflow-error -> arith-error -> error
    assert!(reg.signal_matches_condition("overflow-error", "arith-error"));
    assert!(reg.signal_matches_condition("overflow-error", "error"));
}

#[test]
fn registry_no_match() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(!reg.signal_matches_condition("void-variable", "void-function"));
    assert!(!reg.signal_matches_condition("void-variable", "arith-error"));
}

#[test]
fn registry_t_catches_all() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(reg.signal_matches_condition("void-variable", "t"));
    assert!(reg.signal_matches_condition("file-missing", "t"));
    assert!(reg.signal_matches_condition("error", "t"));
}

#[test]
fn registry_define_error_custom() {
    crate::test_utils::init_test_tracing();
    let mut reg = ErrorRegistry::new();
    reg.define_error("my-error", "My custom error", &["user-error"]);
    assert!(reg.signal_matches_condition("my-error", "user-error"));
    assert!(reg.signal_matches_condition("my-error", "error"));
    assert!(!reg.signal_matches_condition("my-error", "file-error"));
}

#[test]
fn registry_define_error_multiple_parents() {
    crate::test_utils::init_test_tracing();
    let mut reg = ErrorRegistry::new();
    reg.define_error("hybrid-error", "Hybrid", &["file-error", "arith-error"]);
    assert!(reg.signal_matches_condition("hybrid-error", "file-error"));
    assert!(reg.signal_matches_condition("hybrid-error", "arith-error"));
    assert!(reg.signal_matches_condition("hybrid-error", "error"));
}

#[test]
fn registry_conditions_for() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    let cond_syms = reg.conditions_for_sym(intern("file-missing"));
    assert!(cond_syms.contains(&intern("file-missing")));
    assert!(cond_syms.contains(&intern("file-error")));
    assert!(cond_syms.contains(&intern("error")));
    let conds = reg.conditions_for("file-missing");
    assert!(conds.contains(&"file-missing".to_string()));
    assert!(conds.contains(&"file-error".to_string()));
    assert!(conds.contains(&"error".to_string()));
}

#[test]
fn registry_file_error_family() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    for child in &[
        "file-already-exists",
        "file-date-error",
        "file-locked",
        "file-missing",
        "file-notify-error",
    ] {
        assert!(
            reg.signal_matches_condition(child, "file-error"),
            "{} should match file-error",
            child
        );
        assert!(
            reg.signal_matches_condition(child, "error"),
            "{} should match error",
            child
        );
    }
}

#[test]
fn registry_json_error_family() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(reg.signal_matches_condition("json-parse-error", "json-error"));
    assert!(reg.signal_matches_condition("json-serialize-error", "json-error"));
    assert!(reg.signal_matches_condition("json-parse-error", "error"));

    // Full GNU json.c hierarchy: every condition is a json-error and an error.
    for cond in &[
        "json-out-of-memory",
        "json-object-too-deep",
        "json-utf8-decode-error",
        "json-invalid-surrogate-error",
        "json-number-out-of-range-error",
        "json-end-of-file",
        "json-trailing-content",
        "json-escape-sequence-error",
    ] {
        assert!(
            reg.signal_matches_condition(cond, "json-error"),
            "{cond} should match json-error"
        );
        assert!(
            reg.signal_matches_condition(cond, "error"),
            "{cond} should match error"
        );
    }

    // These three are children of json-parse-error in GNU, not direct
    // children of json-error.
    for cond in &[
        "json-end-of-file",
        "json-trailing-content",
        "json-escape-sequence-error",
    ] {
        assert!(
            reg.signal_matches_condition(cond, "json-parse-error"),
            "{cond} should match json-parse-error"
        );
    }
}

#[test]
fn registry_remote_file_error_inherits_file_error() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    assert!(reg.signal_matches_condition("remote-file-error", "file-error"));
    assert!(reg.signal_matches_condition("remote-file-error", "error"));
}

#[test]
fn registry_module_load_failed_family() {
    crate::test_utils::init_test_tracing();
    let reg = ErrorRegistry::new();
    for cond in [
        "module-open-failed",
        "module-not-gpl-compatible",
        "missing-module-init-function",
        "module-init-failed",
    ] {
        assert!(
            reg.signal_matches_condition(cond, "module-load-failed"),
            "{cond} should match module-load-failed"
        );
        assert!(reg.signal_matches_condition(cond, "error"));
    }
    for cond in [
        "module-load-failed",
        "module-out-of-memory",
        "invalid-arity",
        "memory-buffer-too-small",
    ] {
        assert_eq!(reg.conditions_for(cond), vec![cond, "error"]);
    }
}

// =======================================================================
// Obarray-based hierarchy tests
// =======================================================================

#[test]
fn obarray_init_standard_errors() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    // Check that error-conditions is set for 'error' itself.
    let conds = ob.get_property("error", "error-conditions").unwrap();
    let items = iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare);
    assert_eq!(items, vec!["error"]);
}

#[test]
fn obarray_void_variable_conditions() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let conds = ob
        .get_property("void-variable", "error-conditions")
        .unwrap();
    let items = iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare);
    assert!(items.contains(&"void-variable".to_string()));
    assert!(items.contains(&"error".to_string()));
}

#[test]
fn obarray_quit_conditions_match_gnu_non_error_hierarchy() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    let quit_conds = ob.get_property("quit", "error-conditions").unwrap();
    assert_eq!(
        iter_symbol_list(&quit_conds, crate::tagged::SymbolMode::Bare),
        vec!["quit"]
    );

    let minibuffer_quit_conds = ob
        .get_property("minibuffer-quit", "error-conditions")
        .unwrap();
    assert_eq!(
        iter_symbol_list(&minibuffer_quit_conds, crate::tagged::SymbolMode::Bare),
        vec!["minibuffer-quit", "quit"]
    );

    assert!(signal_matches_condition_value(
        &ob,
        "quit",
        &Value::symbol("quit"),
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_condition_value(
        &ob,
        "quit",
        &Value::symbol("error"),
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_condition_value(
        &ob,
        "minibuffer-quit",
        &Value::symbol("quit"),
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_condition_value(
        &ob,
        "minibuffer-quit",
        &Value::symbol("error"),
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn condition_handlers_use_canonical_t_identity_without_reading_symbol_names() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let signal = intern("error");
    let raw_condition =
        crate::intern::intern_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![0xff]));
    let uninterned_t = crate::intern::intern_uninterned("t");

    assert!(!signal_matches_condition_value_sym(
        &ob,
        signal,
        &Value::from_sym_id(raw_condition),
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_condition_value_sym(
        &ob,
        signal,
        &Value::from_sym_id(uninterned_t),
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_condition_value_sym(
        &ob,
        signal,
        &Value::T,
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_overflow_error_conditions() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let conds = ob
        .get_property("overflow-error", "error-conditions")
        .unwrap();
    let items = iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare);
    assert!(items.contains(&"overflow-error".to_string()));
    assert!(items.contains(&"arith-error".to_string()));
    assert!(items.contains(&"error".to_string()));
}

#[test]
fn obarray_file_missing_conditions() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let conds = ob.get_property("file-missing", "error-conditions").unwrap();
    let items = iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare);
    assert!(items.contains(&"file-missing".to_string()));
    assert!(items.contains(&"file-error".to_string()));
    assert!(items.contains(&"error".to_string()));
}

/// `dbus-error` is NOT a bootstrap condition here.
///
/// GNU puts `Fput (Qdbus_error, Qerror_conditions, ...)` in `syms_of_dbusbind`
/// (`src/dbusbind.c:2013-2017`), inside the `#ifdef HAVE_DBUS` that covers the
/// whole file, and this build links no libdbus.  GNU's own `lisp/net/dbus.el`
/// supplies it at load for exactly this build (`:50-51`, with the comment "The
/// following symbols are defined in dbusbind.c.  We need them also when Emacs
/// is compiled without D-Bus support"), so nothing loses the condition -- it
/// arrives when `dbus.el` does, as it does in GNU without the option.
/// Ledger 192.
#[test]
fn obarray_dbus_error_condition_follows_the_transport() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    std::cfg_select! {
        neomacs_have_dbus => {
            let conds = ob
                .get_property("dbus-error", "error-conditions")
                .expect("dbus-error is registered with the transport");
            assert!(iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare).contains(&"dbus-error".to_string()));
            assert!(iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare).contains(&"error".to_string()));
        }
        _ => {
            assert_eq!(ob.get_property("dbus-error", "error-conditions"), None);
            assert_eq!(ob.get_property("dbus-error", "error-message"), None);
        }
    }
    // Anti-vacuity: a neighbour registered by the same function is still
    // there, so the two `None`s above are about `dbus-error` and not about a
    // table that never got built.
    let conds = ob
        .get_property("file-notify-error", "error-conditions")
        .expect("file-notify-error keeps its conditions");
    assert!(
        iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare)
            .contains(&"file-error".to_string())
    );
}

#[test]
fn obarray_cyclic_indirection_conditions() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    let function_conds = ob
        .get_property("cyclic-function-indirection", "error-conditions")
        .unwrap();
    let function_items = iter_symbol_list(&function_conds, crate::tagged::SymbolMode::Bare);
    assert!(function_items.contains(&"cyclic-function-indirection".to_string()));
    assert!(function_items.contains(&"error".to_string()));

    let variable_conds = ob
        .get_property("cyclic-variable-indirection", "error-conditions")
        .unwrap();
    let variable_items = iter_symbol_list(&variable_conds, crate::tagged::SymbolMode::Bare);
    assert!(variable_items.contains(&"cyclic-variable-indirection".to_string()));
    assert!(variable_items.contains(&"error".to_string()));
}

#[test]
fn obarray_hierarchical_match() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    assert!(signal_matches_hierarchical(
        &ob,
        "void-variable",
        "error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "overflow-error",
        "arith-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "file-missing",
        "file-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_hierarchical(
        &ob,
        "void-variable",
        "arith-error",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_hierarchical_match_exact() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    assert!(signal_matches_hierarchical(
        &ob,
        "void-variable",
        "void-variable",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_hierarchical_match_t() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    assert!(signal_matches_hierarchical(
        &ob,
        "void-variable",
        "t",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_error_message_property() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let msg = ob.get_property("void-variable", "error-message").unwrap();
    assert_eq!(
        msg.as_utf8_str(),
        Some("Symbol’s value as variable is void")
    );
}

#[test]
fn obarray_condition_pattern_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let pat = Value::symbol("error");
    assert!(signal_matches_condition_value(
        &ob,
        "void-variable",
        &pat,
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_condition_pattern_list() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    let pat = Value::list(vec![
        Value::symbol("arith-error"),
        Value::symbol("file-error"),
    ]);
    assert!(signal_matches_condition_value(
        &ob,
        "overflow-error",
        &pat,
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_condition_value(
        &ob,
        "file-missing",
        &pat,
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_condition_value(
        &ob,
        "void-variable",
        &pat,
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_unknown_signal_no_conditions() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);
    // A signal that was never registered — only exact match works.
    assert!(!signal_matches_hierarchical(
        &ob,
        "unknown-error",
        "error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "unknown-error",
        "unknown-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "unknown-error",
        "t",
        crate::tagged::SymbolMode::Bare
    ));
}

// =======================================================================
// define-error runtime tests (via bootstrapped GNU Lisp)
// =======================================================================

// =======================================================================
// Builtin tests
// =======================================================================

// =======================================================================
// Edge case / integration tests
// =======================================================================

#[test]
fn obarray_define_then_match() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    // Manually define a custom error.
    let conds = build_conditions_from_obarray(&ob, "my-error", &["user-error"]);
    let cond_refs: Vec<&str> = conds.iter().map(|s| s.as_str()).collect();
    put_error_properties(&mut ob, "my-error", "My custom error", cond_refs);

    assert!(signal_matches_hierarchical(
        &ob,
        "my-error",
        "user-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "my-error",
        "error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_hierarchical(
        &ob,
        "my-error",
        "file-error",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_deep_hierarchy() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    // level1 -> file-error -> error
    register_simple(&mut ob, "level1", "L1", &["file-error"]);
    // level2 -> level1 -> file-error -> error
    register_simple(&mut ob, "level2", "L2", &["level1"]);
    // level3 -> level2 -> level1 -> file-error -> error
    register_simple(&mut ob, "level3", "L3", &["level2"]);

    assert!(signal_matches_hierarchical(
        &ob,
        "level3",
        "level2",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "level3",
        "level1",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "level3",
        "file-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &ob,
        "level3",
        "error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(!signal_matches_hierarchical(
        &ob,
        "level3",
        "arith-error",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn obarray_all_standard_errors_have_message() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    let standard = [
        "error",
        "quit",
        "user-error",
        "args-out-of-range",
        "arith-error",
        "overflow-error",
        "range-error",
        "domain-error",
        "underflow-error",
        "beginning-of-buffer",
        "end-of-buffer",
        "buffer-read-only",
        "coding-system-error",
        "file-error",
        "file-already-exists",
        "file-date-error",
        "file-locked",
        "file-missing",
        "file-notify-error",
        "invalid-function",
        "invalid-read-syntax",
        "invalid-regexp",
        "mark-inactive",
        "no-catch",
        "scan-error",
        "search-failed",
        "setting-constant",
        "text-read-only",
        "void-function",
        "void-variable",
        "wrong-number-of-arguments",
        "wrong-type-argument",
        "cl-assertion-failed",
        "circular-list",
        "json-error",
        "json-parse-error",
        "json-serialize-error",
        "treesit-error",
        "treesit-query-error",
        "treesit-parse-error",
        "treesit-range-invalid",
        "treesit-buffer-too-large",
        "treesit-load-language-error",
        "treesit-node-outdated",
        "treesit-buffer_changed",
        "treesit-node-buffer-killed",
        "treesit-parser-deleted",
        "treesit-invalid-predicate",
        "permission-denied",
        "remote-file-error",
        "recursion-error",
    ];

    for name in &standard {
        assert!(
            ob.get_property(name, "error-message").is_some(),
            "{} should have error-message",
            name
        );
        assert!(
            ob.get_property(name, "error-conditions").is_some(),
            "{} should have error-conditions",
            name
        );
    }
}

#[test]
fn obarray_all_standard_errors_include_self_in_conditions() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    let standard = [
        "error",
        "void-variable",
        "overflow-error",
        "file-missing",
        "json-parse-error",
        "treesit-query-error",
    ];

    for name in &standard {
        let conds = ob.get_property(name, "error-conditions").unwrap();
        let items = iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare);
        assert!(
            items.contains(&name.to_string()),
            "{} should contain itself in error-conditions",
            name
        );
    }
}

#[test]
fn obarray_treesit_query_error_matches_gnu_c_defined_hierarchy() {
    crate::test_utils::init_test_tracing();
    let mut ob = Obarray::new();
    init_standard_errors(&mut ob);

    let message = ob
        .get_property("treesit-query-error", "error-message")
        .expect("treesit-query-error should have error-message");
    assert_eq!(message.as_utf8_str(), Some("Query pattern is malformed"));

    let conds = ob
        .get_property("treesit-query-error", "error-conditions")
        .expect("treesit-query-error should have error-conditions");
    assert_eq!(
        iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare),
        vec!["treesit-query-error", "treesit-error", "error"]
    );
}

// =======================================================================
// `signal` re-signal idiom: a cons error symbol with nil DATA is the
// whole error object, re-raised as-is (GNU `signal_or_quit`, eval.c:1930).
//
// These use a bare `Context` (so they avoid the bootstrap pdump) and
// `define-error` the `scan-error` symbol they re-raise.
// =======================================================================
