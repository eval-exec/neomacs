//! Phase 0 step 2, round 3b: the owner-aware variable API answers the
//! running owner's binding of a projected variable, and canonical storage
//! (what every other owner sees) otherwise.

use super::super::owner_bindings::Channel;
use super::VariableView;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::intern::intern;
use neovm_value::symbol::CanonicalAccess;
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn context(projected: bool) -> Context {
    let mut ev = Context::new();
    eval(
        &mut ev,
        "(progn (defvar tva-v 5)
                (defvar tva-auto 5) (make-variable-buffer-local 'tva-auto))",
    );
    if projected {
        for name in ["tva-v", "tva-auto"] {
            ev.project_thread_variable_for_test(intern(name))
                .unwrap_or_else(|| panic!("{name} projects"));
        }
    }
    ev
}

fn show(value: Option<Value>) -> String {
    value.map_or_else(|| "void".to_owned(), |value| print_value(&value))
}

fn canonical(ev: &mut Context, name: &str) -> String {
    print_value(&ev.owner_canonical_value(intern(name), Channel::Default))
}

#[test]
fn a_dormant_context_reads_and_writes_canonical_storage() {
    for projected in [false, true] {
        let mut ev = context(projected);
        assert_eq!(show(ev.default_value_only_by_name("tva-v")), "5");
        ev.store_variable_by_name("tva-v", Value::fixnum(6));
        assert_eq!(eval(&mut ev, "tva-v"), "6");
        assert_eq!(canonical(&mut ev, "tva-v"), "6");
        assert!(ev.variables().boundp_by_name("tva-v"));
        ev.makunbound_variable(intern("tva-v"));
        assert_eq!(eval(&mut ev, "(boundp 'tva-v)"), "nil");
        assert_eq!(show(ev.default_value_only_by_name("tva-v")), "void");
    }
}

#[test]
fn reads_answer_the_owner_binding() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    ev.try_specbind(intern("tva-v"), Value::fixnum(7))
        .expect("bind");
    assert_eq!(show(ev.default_value_only_by_name("tva-v")), "7");
    assert_eq!(print_value(&ev.variable_value_or_nil(intern("tva-v"))), "7");
    assert_eq!(show(ev.variables().default_value(intern("tva-v"))), "7");
    assert!(ev.variables().boundp(intern("tva-v")));
    let canonical_view = VariableView::canonical(CanonicalAccess::TEST, &ev.obarray);
    assert_eq!(
        show(canonical_view.value(intern("tva-v"))),
        "void",
        "a projected cell's canonical half answers void; the owner answers"
    );
    assert_eq!(canonical(&mut ev, "tva-v"), "5");
    ev.unbind_to(count);
    assert_eq!(show(ev.default_value_only_by_name("tva-v")), "5");
}

#[test]
fn writes_under_a_binding_stay_the_owner_s() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    ev.try_specbind(intern("tva-v"), Value::fixnum(7))
        .expect("bind");
    ev.store_variable_by_name("tva-v", Value::fixnum(8));
    assert_eq!(eval(&mut ev, "tva-v"), "8");
    assert_eq!(canonical(&mut ev, "tva-v"), "5");
    ev.makunbound_variable(intern("tva-v"));
    assert_eq!(eval(&mut ev, "(boundp 'tva-v)"), "nil");
    assert_eq!(canonical(&mut ev, "tva-v"), "5");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "tva-v"), "5");
}

#[test]
fn a_write_with_no_binding_reaches_canonical_storage() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    // A slab exists, but this owner binds nothing of tva-v.
    ev.try_specbind(intern("tva-auto"), Value::fixnum(1))
        .expect("bind another variable");
    ev.store_variable_by_name("tva-v", Value::fixnum(9));
    assert_eq!(canonical(&mut ev, "tva-v"), "9");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "tva-v"), "9");
}

#[test]
fn buffer_reads_answer_the_owner_local_binding() {
    let mut ev = context(true);
    eval(
        &mut ev,
        "(progn (set-buffer (get-buffer-create \" tva-b\")) (setq tva-auto 6))",
    );
    let count = ev.specpdl.len();
    ev.try_specbind(intern("tva-auto"), Value::fixnum(11))
        .expect("bind the local");
    let buffer = ev.buffers.current_buffer();
    assert_eq!(
        show(ev.variables().value_in_buffer(buffer, intern("tva-auto"))),
        "11"
    );
    assert_eq!(
        show(ev.variable_value_in_current_buffer(intern("tva-auto"))),
        "11"
    );
    assert_eq!(show(ev.variables().default_value(intern("tva-auto"))), "5");
    ev.unbind_to(count);
    let buffer = ev.buffers.current_buffer();
    assert_eq!(
        show(ev.variables().value_in_buffer(buffer, intern("tva-auto"))),
        "6"
    );
}

#[test]
fn the_reader_shorthand_cell_read_falls_through_to_the_owner() {
    // `read_symbol_shorthands_value` reads the variable's cell directly; a
    // projected cell answers void there, so the read takes the general,
    // owner-aware lookup.
    let mut ev = context(false);
    let id = intern("read-symbol-shorthands");
    ev.project_thread_variable_for_test(id)
        .expect("read-symbol-shorthands projects");
    let shorthands = |ev: &Context| {
        print_value(&crate::emacs_core::builtins::symbols::read_symbol_shorthands_value(ev))
    };
    assert_eq!(shorthands(&ev), "nil");
    let count = ev.specpdl.len();
    let binding = ev
        .eval_str("'((\"tva-\" . \"tvb-\"))")
        .expect("shorthand alist");
    ev.try_specbind(id, binding).expect("bind");
    assert_eq!(shorthands(&ev), "((\"tva-\" . \"tvb-\"))");
    ev.unbind_to(count);
    assert_eq!(shorthands(&ev), "nil");
}
