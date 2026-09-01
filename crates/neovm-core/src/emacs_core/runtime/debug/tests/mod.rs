use super::*;
use crate::emacs_core::intern::{intern, resolve_sym};
use crate::emacs_core::value::{LambdaData, LambdaParams};

/// Placeholder retained for old test setup naming — tagged heap is auto-created for tests.
fn init_test_heap() {}

fn debug_text(text: &str) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_utf8(text)
}

// -- Backtrace tests --

#[test]
fn backtrace_push_pop() {
    crate::test_utils::init_test_tracing();
    let mut bt = Backtrace::new();
    assert_eq!(bt.depth(), 0);

    bt.push(BacktraceFrame {
        function: Value::symbol("foo"),
        args: vec![Value::fixnum(1)],
        file: None,
        line: None,
        is_special_form: false,
    });
    assert_eq!(bt.depth(), 1);

    bt.push(BacktraceFrame {
        function: Value::symbol("bar"),
        args: vec![Value::fixnum(2), Value::fixnum(3)],
        file: Some(debug_text("test.el")),
        line: Some(42),
        is_special_form: false,
    });
    assert_eq!(bt.depth(), 2);

    let top = bt.pop().unwrap();
    assert_eq!(top.function, Value::symbol("bar"));
    assert_eq!(bt.depth(), 1);

    let bottom = bt.pop().unwrap();
    assert_eq!(bottom.function, Value::symbol("foo"));
    assert_eq!(bt.depth(), 0);

    assert!(bt.pop().is_none());
}

#[test]
fn backtrace_max_depth() {
    crate::test_utils::init_test_tracing();
    let mut bt = Backtrace::with_max_depth(3);
    for i in 0..10 {
        bt.push(BacktraceFrame {
            function: Value::symbol(format!("fn{}", i)),
            args: vec![],
            file: None,
            line: None,
            is_special_form: false,
        });
    }
    assert_eq!(bt.depth(), 3);
}

#[test]
fn backtrace_format_nonempty() {
    crate::test_utils::init_test_tracing();
    let mut bt = Backtrace::new();
    bt.push(BacktraceFrame {
        function: Value::symbol("+"),
        args: vec![Value::fixnum(1), Value::fixnum(2)],
        file: None,
        line: None,
        is_special_form: false,
    });
    bt.push(BacktraceFrame {
        function: Value::symbol("my-add"),
        args: vec![Value::fixnum(1), Value::fixnum(2)],
        file: Some(debug_text("test.el")),
        line: Some(10),
        is_special_form: false,
    });
    let formatted = bt.format();
    assert!(formatted.contains("my-add"));
    assert!(formatted.contains("+"));
    assert!(formatted.contains("test.el"));
}

#[test]
fn backtrace_format_empty() {
    crate::test_utils::init_test_tracing();
    let bt = Backtrace::new();
    let formatted = bt.format();
    assert!(formatted.contains("no backtrace"));
}

#[test]
fn backtrace_format_special_form() {
    crate::test_utils::init_test_tracing();
    let mut bt = Backtrace::new();
    bt.push(BacktraceFrame {
        function: Value::symbol("if"),
        args: vec![Value::T],
        file: None,
        line: None,
        is_special_form: true,
    });
    let formatted = bt.format();
    assert!(formatted.contains("*"));
    assert!(formatted.contains("if"));
}

#[test]
fn backtrace_clear() {
    crate::test_utils::init_test_tracing();
    let mut bt = Backtrace::new();
    bt.push(BacktraceFrame {
        function: Value::symbol("foo"),
        args: vec![],
        file: None,
        line: None,
        is_special_form: false,
    });
    assert_eq!(bt.depth(), 1);
    bt.clear();
    assert_eq!(bt.depth(), 0);
}

// -- DebugState tests --

#[test]
fn debug_on_entry_add_remove() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();
    assert!(!ds.should_debug_on_entry("foo"));

    ds.add_debug_on_entry("foo");
    assert!(ds.should_debug_on_entry("foo"));
    assert!(!ds.should_debug_on_entry("bar"));

    ds.remove_debug_on_entry("foo");
    assert!(!ds.should_debug_on_entry("foo"));
}

#[test]
fn debug_on_entry_via_breakpoint() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();
    let bp_id = ds.add_breakpoint("my-fn");
    assert!(ds.should_debug_on_entry("my-fn"));
    assert!(!ds.should_debug_on_entry("other-fn"));

    // Disable the breakpoint
    ds.toggle_breakpoint(bp_id);
    assert!(!ds.should_debug_on_entry("my-fn"));

    // Re-enable
    ds.toggle_breakpoint(bp_id);
    assert!(ds.should_debug_on_entry("my-fn"));
}

// -- Breakpoint tests --

#[test]
fn breakpoint_add_remove() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();
    let id1 = ds.add_breakpoint("foo");
    let id2 = ds.add_breakpoint("bar");
    assert_eq!(ds.list_breakpoints().len(), 2);

    assert!(ds.remove_breakpoint(id1));
    assert_eq!(ds.list_breakpoints().len(), 1);
    assert_eq!(resolve_sym(ds.list_breakpoints()[0].function), "bar");

    // Removing non-existent returns false
    assert!(!ds.remove_breakpoint(999));

    assert!(ds.remove_breakpoint(id2));
    assert!(ds.list_breakpoints().is_empty());
}

#[test]
fn breakpoint_toggle() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();
    let id = ds.add_breakpoint("test-fn");
    assert!(ds.list_breakpoints()[0].enabled);

    assert!(ds.toggle_breakpoint(id));
    assert!(!ds.list_breakpoints()[0].enabled);

    assert!(ds.toggle_breakpoint(id));
    assert!(ds.list_breakpoints()[0].enabled);

    // Toggle non-existent
    assert!(!ds.toggle_breakpoint(999));
}

#[test]
fn breakpoint_hit_count() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();
    let _id = ds.add_breakpoint("count-me");
    assert_eq!(ds.list_breakpoints()[0].hit_count, 0);

    ds.record_breakpoint_hit("count-me");
    assert_eq!(ds.list_breakpoints()[0].hit_count, 1);

    ds.record_breakpoint_hit("count-me");
    ds.record_breakpoint_hit("count-me");
    assert_eq!(ds.list_breakpoints()[0].hit_count, 3);

    // Hitting a non-existent function is a no-op
    ds.record_breakpoint_hit("other");
    assert_eq!(ds.list_breakpoints()[0].hit_count, 3);
}

#[test]
fn breakpoint_conditional() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();
    let id = ds.add_conditional_breakpoint("my-fn", "(> x 5)");
    let bp = &ds.list_breakpoints()[0];
    assert_eq!(bp.id, id);
    assert_eq!(bp.condition.as_ref(), Some(&debug_text("(> x 5)")));
}

// -- DocStore tests --

#[test]
fn docstore_set_get_function() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    assert!(store.get_function_doc("car").is_none());

    store.set_function_doc("car", "Return the car of LIST.");
    assert_eq!(
        store.get_function_doc("car"),
        Some("Return the car of LIST.".to_string())
    );

    // Overwrite
    store.set_function_doc("car", "Updated doc.");
    assert_eq!(
        store.get_function_doc("car"),
        Some("Updated doc.".to_string())
    );
}

#[test]
fn docstore_set_get_variable() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    assert!(store.get_variable_doc("load-path").is_none());

    store.set_variable_doc("load-path", "List of directories to search.");
    assert_eq!(
        store.get_variable_doc("load-path"),
        Some("List of directories to search.".to_string())
    );
}

#[test]
fn docstore_apropos_basic() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    store.set_function_doc("car", "Return the car.");
    store.set_function_doc("cdr", "Return the cdr.");
    store.set_function_doc("cons", "Create a cons cell.");
    store.set_variable_doc("car-mode", "Mode for cars.");

    let results = store.apropos("car");
    // Should match "car" (func), "car-mode" (var)
    assert_eq!(results.len(), 2);
    // Results are sorted
    assert_eq!(results[0].0, "car");
    assert!(results[0].1); // has func
    assert!(!results[0].2); // no var

    assert_eq!(results[1].0, "car-mode");
    assert!(!results[1].1); // no func
    assert!(results[1].2); // has var
}

#[test]
fn docstore_apropos_case_insensitive() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    store.set_function_doc("Buffer-Name", "Return buffer name.");
    store.set_function_doc("buffer-size", "Return buffer size.");

    let results = store.apropos("buffer");
    assert_eq!(results.len(), 2);
}

#[test]
fn docstore_apropos_both() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    store.set_function_doc("fill-column", "Get fill column.");
    store.set_variable_doc("fill-column", "Column for fill.");

    let results = store.apropos("fill");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "fill-column");
    assert!(results[0].1); // has func
    assert!(results[0].2); // has var
}

#[test]
fn docstore_apropos_no_match() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    store.set_function_doc("car", "car doc");
    let results = store.apropos("zzz-nonexistent");
    assert!(results.is_empty());
}

#[test]
fn docstore_all_documented() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    store.set_function_doc("cdr", "cdr doc");
    store.set_function_doc("car", "car doc");
    store.set_variable_doc("x", "x doc");
    store.set_variable_doc("a", "a doc");

    let fns = store.all_documented_functions();
    assert_eq!(fns, vec!["car".to_string(), "cdr".to_string()]);

    let vars = store.all_documented_variables();
    assert_eq!(vars, vec!["a".to_string(), "x".to_string()]);
}

#[test]
fn docstore_remove() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();
    store.set_function_doc("foo", "doc");
    assert!(store.remove_function_doc("foo"));
    assert!(store.get_function_doc("foo").is_none());
    assert!(!store.remove_function_doc("foo")); // already gone

    store.set_variable_doc("bar", "doc");
    assert!(store.remove_variable_doc("bar"));
    assert!(store.get_variable_doc("bar").is_none());
}

// -- HelpFormatter tests --

#[test]
fn help_describe_function_lambda() {
    crate::test_utils::init_test_tracing();
    init_test_heap();
    let lam = Value::make_lambda(LambdaData {
        params: LambdaParams {
            required: vec![intern("x"), intern("y")],
            optional: vec![],
            rest: None,
        },
        body: vec![].into(),
        env: None,
        docstring: Some(crate::heap_types::LispString::from_utf8("Add X and Y.")),
        doc_form: None,
        interactive: None,
    });
    let output = HelpFormatter::describe_function("my-add", &lam, None);
    assert!(output.contains("my-add is a Lisp function."));
    assert!(output.contains("(my-add X Y)"));
    assert!(output.contains("Add X and Y."));
}

#[test]
fn help_describe_function_with_docstore() {
    crate::test_utils::init_test_tracing();
    init_test_heap();
    let lam = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: vec![].into(),
        env: None,
        docstring: Some(crate::heap_types::LispString::from_utf8("Inline doc.")),
        doc_form: None,
        interactive: None,
    });
    // Docstore doc overrides inline
    let output = HelpFormatter::describe_function("my-fn", &lam, Some("Docstore doc."));
    assert!(output.contains("Docstore doc."));
    assert!(!output.contains("Inline doc."));
}

#[test]
fn help_describe_function_subr() {
    crate::test_utils::init_test_tracing();
    let subr = Value::subr(intern("car"));
    let output = HelpFormatter::describe_function("car", &subr, Some("Return the car of LIST."));
    assert!(output.contains("car is a built-in function."));
    assert!(output.contains("Return the car of LIST."));
}

#[test]
fn help_describe_function_no_doc() {
    crate::test_utils::init_test_tracing();
    let subr = Value::subr(intern("mystery"));
    let output = HelpFormatter::describe_function("mystery", &subr, None);
    assert!(output.contains("Not documented."));
}

#[test]
fn help_describe_function_closure() {
    crate::test_utils::init_test_tracing();
    init_test_heap();
    let lam = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: vec![].into(),
        env: Some(Value::NIL),
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    let output = HelpFormatter::describe_function("my-closure", &lam, None);
    assert!(output.contains("a Lisp closure"));
}

#[test]
fn help_describe_variable() {
    crate::test_utils::init_test_tracing();
    let output = HelpFormatter::describe_variable(
        "fill-column",
        &Value::fixnum(70),
        Some("Column beyond which automatic line-filling takes place."),
    );
    assert!(output.contains("fill-column's value is 70"));
    assert!(output.contains("Column beyond which"));
}

#[test]
fn help_describe_variable_no_doc() {
    crate::test_utils::init_test_tracing();
    let output = HelpFormatter::describe_variable("x", &Value::NIL, None);
    assert!(output.contains("x's value is nil"));
    assert!(output.contains("Not documented."));
}

#[test]
fn help_describe_key() {
    crate::test_utils::init_test_tracing();
    let output = HelpFormatter::describe_key("C-x C-f", "find-file", Some("Visit a file."));
    assert!(output.contains("C-x C-f runs the command find-file"));
    assert!(output.contains("Visit a file."));
}

#[test]
fn help_describe_key_no_doc() {
    crate::test_utils::init_test_tracing();
    let output = HelpFormatter::describe_key("C-c a", "my-cmd", None);
    assert!(output.contains("C-c a runs the command my-cmd"));
    assert!(!output.contains("documented"));
}

#[test]
fn help_format_apropos_entries() {
    crate::test_utils::init_test_tracing();
    let entries = vec![
        ("car".to_string(), true, false),
        ("car-mode".to_string(), false, true),
        ("cons".to_string(), true, true),
    ];
    let output = HelpFormatter::format_apropos(&entries);
    assert!(output.contains("car\n  Function\n"));
    assert!(output.contains("car-mode\n  Variable\n"));
    assert!(output.contains("cons\n  Function, Variable\n"));
}

#[test]
fn help_format_apropos_empty() {
    crate::test_utils::init_test_tracing();
    let output = HelpFormatter::format_apropos(&[]);
    assert!(output.contains("No matches."));
}

// -- DebugAction clone/debug --

#[test]
fn debug_action_variants() {
    crate::test_utils::init_test_tracing();
    let actions = vec![
        DebugAction::Continue,
        DebugAction::Step,
        DebugAction::Next,
        DebugAction::Finish,
        DebugAction::Quit,
        DebugAction::Eval(debug_text("(+ 1 2)")),
    ];
    // Just verify they can be cloned and debug-printed
    for action in &actions {
        let _cloned = action.clone();
        let _debug = format!("{:?}", action);
    }
}

// -- Integration-style tests --

#[test]
fn debug_state_full_workflow() {
    crate::test_utils::init_test_tracing();
    let mut ds = DebugState::new();

    // Initially nothing triggers
    assert!(!ds.should_debug_on_entry("my-fn"));
    assert!(!ds.active);
    assert!(!ds.stepping);

    // Set up debug-on-entry
    ds.add_debug_on_entry("my-fn");
    assert!(ds.should_debug_on_entry("my-fn"));

    // Set up a breakpoint
    let bp_id = ds.add_breakpoint("other-fn");
    assert!(ds.should_debug_on_entry("other-fn"));

    // Use the backtrace
    ds.current_backtrace.push(BacktraceFrame {
        function: Value::symbol("my-fn"),
        args: vec![Value::fixnum(1)],
        file: None,
        line: None,
        is_special_form: false,
    });
    assert_eq!(ds.current_backtrace.depth(), 1);

    // Record a hit
    ds.record_breakpoint_hit("other-fn");
    assert_eq!(ds.list_breakpoints()[0].hit_count, 1);

    // Clean up
    ds.remove_debug_on_entry("my-fn");
    assert!(!ds.should_debug_on_entry("my-fn"));
    ds.remove_breakpoint(bp_id);
    assert!(!ds.should_debug_on_entry("other-fn"));
}

#[test]
fn docstore_full_workflow() {
    crate::test_utils::init_test_tracing();
    let mut store = DocStore::new();

    // Populate
    store.set_function_doc(
        "car",
        "Return the car of LIST.\nThe car is the first element.",
    );
    store.set_function_doc("cdr", "Return the cdr of LIST.");
    store.set_function_doc("cons", "Create a new cons cell from CAR and CDR.");
    store.set_variable_doc(
        "load-path",
        "List of directories to search for files to load.",
    );
    store.set_variable_doc("debug-on-error", "Non-nil means enter debugger on error.");

    // Lookup
    assert!(
        store
            .get_function_doc("car")
            .unwrap()
            .contains("first element")
    );
    assert!(
        store
            .get_variable_doc("load-path")
            .unwrap()
            .contains("directories")
    );

    // Apropos
    let results = store.apropos("c");
    // "car", "cdr", "cons" match
    assert_eq!(results.len(), 3);

    let results = store.apropos("on-error");
    // "debug-on-error" matches
    assert_eq!(results.len(), 1);
    assert!(!results[0].1); // no function doc for debug-on-error
    assert!(results[0].2); // has variable doc

    // Format apropos
    let formatted = HelpFormatter::format_apropos(&results);
    assert!(formatted.contains("debug-on-error"));
    assert!(formatted.contains("Variable"));

    // All documented
    let fns = store.all_documented_functions();
    assert_eq!(fns.len(), 3);
    let vars = store.all_documented_variables();
    assert_eq!(vars.len(), 2);
}

#[test]
fn help_formatter_with_optional_and_rest() {
    crate::test_utils::init_test_tracing();
    init_test_heap();
    let lam = Value::make_lambda(LambdaData {
        params: LambdaParams {
            required: vec![intern("x")],
            optional: vec![intern("y")],
            rest: Some(intern("args")),
        },
        body: vec![].into(),
        env: None,
        docstring: Some(crate::heap_types::LispString::from_utf8(
            "A function with complex params.",
        )),
        doc_form: None,
        interactive: None,
    });
    let output = HelpFormatter::describe_function("complex-fn", &lam, None);
    assert!(output.contains("(complex-fn X &optional Y &rest ARGS)"));
    assert!(output.contains("A function with complex params."));
}

#[test]
fn help_formatter_macro() {
    crate::test_utils::init_test_tracing();
    init_test_heap();
    let mac = Value::make_macro(LambdaData {
        params: LambdaParams::simple(vec![intern("body")]),
        body: vec![].into(),
        env: None,
        docstring: Some(crate::heap_types::LispString::from_utf8("A test macro.")),
        doc_form: None,
        interactive: None,
    });
    let output = HelpFormatter::describe_function("my-macro", &mac, None);
    assert!(output.contains("a Lisp macro"));
    assert!(output.contains("(my-macro BODY)"));
}
