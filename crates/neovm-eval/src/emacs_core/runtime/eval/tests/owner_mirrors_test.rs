//! Phase 0 step 2, round 3b: an owner's host mirrors and its
//! `debug-on-next-call` cell follow what the owner sees.
//!
//! "Another owner's write" is the canonical store an owner without a
//! binding makes (`owner_canonical_store`), as in `owner_bindings_test`.

use super::super::owner_bindings::Channel;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::intern::{SymId, intern};
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn projected(names: &[&str]) -> Context {
    let mut ev = Context::new();
    eval(
        &mut ev,
        "(progn (defvar tom-auto 5) (make-variable-buffer-local 'tom-auto)
                (get-buffer-create \" tom-other\"))",
    );
    for name in names {
        ev.project_thread_variable_for_test(intern(name))
            .unwrap_or_else(|| panic!("{name} projects"));
    }
    ev
}

fn armed(ev: &Context) -> bool {
    ev.debug_on_next_call_is_armed()
}

#[test]
fn refresh_republishes_a_mirror_another_owner_wrote() {
    let mut ev = projected(&["inhibit-quit"]);
    let id = intern("inhibit-quit");
    ev.owner_canonical_store(id, Channel::Default, Value::T);
    // The other owner's write does not reach this Context's mirror.
    ev.inhibit_quit = Value::NIL;
    ev.refresh_owner_mirrors();
    assert_eq!(ev.inhibit_quit, Value::T);
}

#[test]
fn refresh_keeps_the_owner_binding_over_canonical() {
    let mut ev = projected(&["inhibit-quit"]);
    let id = intern("inhibit-quit");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::T).expect("bind");
    ev.owner_canonical_store(id, Channel::Default, Value::NIL);
    ev.inhibit_quit = Value::NIL;
    ev.refresh_owner_mirrors();
    assert_eq!(ev.inhibit_quit, Value::T, "the owner's binding is its view");
    ev.unbind_to(count);
    assert_eq!(ev.inhibit_quit, Value::NIL);
}

#[test]
fn refresh_installs_the_owner_gc_threshold() {
    let mut ev = projected(&["gc-cons-threshold"]);
    let id = intern("gc-cons-threshold");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::fixnum(123_456_789))
        .expect("bind");
    ev.owner_canonical_store(id, Channel::Default, Value::fixnum(800_000));
    ev.refresh_owner_mirrors();
    assert_eq!(
        ev.gc_runtime_settings_cache.gc_cons_threshold_bytes,
        123_456_789
    );
    ev.unbind_to(count);
    ev.refresh_owner_mirrors();
    assert_eq!(
        ev.gc_runtime_settings_cache.gc_cons_threshold_bytes,
        800_000
    );
}

#[test]
fn resume_retires_a_binding_whose_buffer_died() {
    let mut ev = projected(&["tom-auto"]);
    let id: SymId = intern("tom-auto");
    let other = ev
        .buffers
        .find_buffer_by_name(" tom-other")
        .expect("tom-other");
    eval(
        &mut ev,
        "(save-current-buffer (set-buffer \" tom-other\") (setq tom-auto 6))",
    );
    let home = eval(&mut ev, "(buffer-name)");
    eval(&mut ev, "(set-buffer \" tom-other\")");
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::fixnum(9)).expect("bind");
    assert_eq!(eval(&mut ev, "tom-auto"), "9");
    let slab = ev.owner.slab().expect("slab");
    assert!(
        slab.records_of(id)
            .all(|record| record.channel == Channel::BufferLocal(other)),
        "a let of the local binds its channel"
    );
    eval(&mut ev, &format!("(set-buffer {home})"));
    let parked = ev.park_owner();
    assert!(
        ev.buffers.kill_buffer(other),
        "another owner kills the buffer"
    );
    // The returning continuation validates the owner's local bindings.
    ev.resume_owner(parked);
    let slab = ev.owner.slab().expect("slab");
    assert!(
        slab.records_of(id).all(|record| matches!(
            record.undo,
            super::super::owner_bindings::UndoTarget::Retired
        )),
        "the binding of a dead buffer's local is zapped"
    );
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "(default-value 'tom-auto)"), "5");
}

#[test]
fn an_owner_binding_of_debug_on_next_call_arms_only_this_owner() {
    let mut ev = projected(&["debug-on-next-call"]);
    let id = intern("debug-on-next-call");
    assert!(!armed(&ev));
    let count = ev.specpdl.len();
    ev.try_specbind(id, Value::T).expect("bind");
    assert!(armed(&ev), "the header names the owner's cell");
    assert_eq!(
        print_value(&ev.owner_canonical_value(id, Channel::Default)),
        "nil",
        "canonical storage, which other owners read, is unchanged"
    );
    // A canonical function-epoch write retires the header but keeps the
    // cell. (No Lisp runs here: an armed debug-on-next-call would take the
    // arm at the first form.)
    ev.obarray.invalidate_all_function_bindings(
        neovm_value::symbol::FunctionEpochBump::CompilerOverrides,
    );
    assert!(armed(&ev));
    // So does a function-cell write, which retires the function clock only.
    ev.obarray
        .set_symbol_function_id(intern("tom-f"), Value::NIL);
    assert!(armed(&ev));
    // Assignments under the binding move the owner's cell.
    ev.store_variable(id, Value::NIL);
    assert!(!armed(&ev));
    ev.store_variable(id, Value::T);
    assert!(armed(&ev));
    // The debugger's disarm clears the binding too.
    ev.obarray
        .debug_on_next_call_bool_fwd_cached()
        .expect("owner cell")
        .set(false);
    ev.owner_debugger_disarmed();
    assert_eq!(eval(&mut ev, "debug-on-next-call"), "nil");
    ev.unbind_to(count);
    assert!(!armed(&ev));
    assert!(ev.obarray.admission_matches_canonical());
}

#[test]
fn a_canonical_arm_under_no_owner_binding_stays_canonical() {
    let mut ev = projected(&["debug-on-next-call"]);
    let id = intern("debug-on-next-call");
    ev.owner_canonical_store(id, Channel::Default, Value::T);
    ev.refresh_owner_mirrors();
    assert!(
        armed(&ev),
        "with no binding the owner sees canonical storage"
    );
    ev.owner_canonical_store(id, Channel::Default, Value::NIL);
    assert!(!armed(&ev));
}

#[test]
fn undo_list_bindings_swap_with_their_owner() {
    let mut ev = projected(&[]);
    assert!(
        ev.project_thread_variable_for_test(intern("buffer-undo-list"))
            .is_none(),
        "buffer-undo-list's storage is the buffer's undo state, never a cell"
    );
    eval(
        &mut ev,
        "(progn (set-buffer (get-buffer-create \" tom-u\"))
                (setq buffer-undo-list nil) (insert \"a\"))",
    );
    let outer = eval(&mut ev, "buffer-undo-list");
    let count = ev.specpdl.len();
    ev.try_specbind(intern("buffer-undo-list"), Value::T)
        .expect("bind");
    assert_eq!(eval(&mut ev, "buffer-undo-list"), "t");
    // Another owner runs: it sees, and records into, what the binding hides.
    let parked = ev.park_owner();
    assert_eq!(eval(&mut ev, "buffer-undo-list"), outer);
    eval(&mut ev, "(insert \"b\")");
    let recorded = eval(&mut ev, "buffer-undo-list");
    assert_ne!(recorded, outer, "the other owner's edit is recorded");
    // This owner returns: its binding is back, so its edit is not recorded.
    ev.resume_owner(parked);
    assert_eq!(eval(&mut ev, "buffer-undo-list"), "t");
    eval(&mut ev, "(insert \"c\")");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "buffer-undo-list"), recorded);
}

#[test]
fn a_buffer_slot_binding_lives_in_the_slot_and_parks_in_the_slab() {
    let mut ev = projected(&["fill-column", "mode-name"]);
    eval(
        &mut ev,
        "(progn (set-buffer (get-buffer-create \" tom-slot\"))
                (setq fill-column 50) (setq mode-name \"outer\"))",
    );
    let buffer = ev.buffers.current_buffer_id().expect("current buffer");
    let fill = intern("fill-column");
    let mode = intern("mode-name");
    let count = ev.specpdl.len();
    ev.try_specbind(fill, Value::fixnum(60))
        .expect("bind fill-column");
    eval(
        &mut ev,
        "(setq tom-name (concat \"inner-\" (number-to-string 7)))",
    );
    let name = ev.default_value_only_by_name("tom-name").expect("tom-name");
    ev.try_specbind(mode, name).expect("bind mode-name");
    // In place: the slot holds the binding, and the slab records nothing.
    assert_eq!(
        print_value(&ev.owner_canonical_value(fill, Channel::BufferLocal(buffer))),
        "60"
    );
    assert!(
        ev.owner
            .slab()
            .is_none_or(|slab| slab.records_of(fill).next().is_none()),
        "a buffer's own slot binding is not a slab binding"
    );
    assert_eq!(
        eval(&mut ev, "(list fill-column mode-name)"),
        "(60 \"inner-7\")"
    );
    // Another owner runs: it sees what the bindings hide, and writes it.
    let parked = ev.park_owner();
    eval(&mut ev, "(makunbound 'tom-name)");
    assert_eq!(
        eval(&mut ev, "(list fill-column mode-name)"),
        "(50 \"outer\")"
    );
    eval(&mut ev, "(setq fill-column 55)");
    // The parked binding values stay rooted through the slab.
    ev.gc_collect_exact();
    ev.resume_owner(parked);
    assert_eq!(
        eval(&mut ev, "(list fill-column mode-name)"),
        "(60 \"inner-7\")"
    );
    ev.unbind_to(count);
    // The other owner's write is what the binding hid when it popped.
    assert_eq!(
        eval(&mut ev, "(list fill-column mode-name)"),
        "(55 \"outer\")"
    );
}

#[test]
fn undo_list_reads_answer_the_shared_state() {
    let mut ev = projected(&["fill-column"]);
    let undo = intern("buffer-undo-list");
    eval(
        &mut ev,
        "(progn (set-buffer (get-buffer-create \" tom-u2\"))
                (setq buffer-undo-list nil) (insert \"a\"))",
    );
    let shared = eval(&mut ev, "buffer-undo-list");
    let read = |ev: &Context| match ev.find_symbol_value_by_id(undo) {
        Ok(super::super::SymbolValueLookup::Bound(value)) => print_value(&value),
        other => format!("{other:?}"),
    };
    assert_eq!(read(&ev), shared, "the shared undo state, with a slab");
    let count = ev.specpdl.len();
    ev.try_specbind(undo, Value::T).expect("bind");
    assert_eq!(read(&ev), "t");
    ev.unbind_to(count);
    assert_eq!(read(&ev), shared);
}
