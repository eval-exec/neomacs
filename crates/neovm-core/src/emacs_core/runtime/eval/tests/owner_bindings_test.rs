//! Phase 0 step 2, round 3a: dynamic bindings of a projected variable live
//! in the binding owner's slab, and canonical storage -- what every other
//! owner sees -- does not change under them.
//!
//! "Another owner's write" is simulated with the canonical store an owner
//! without a binding makes (`owner_canonical_store`): no second Context
//! exists yet.

use super::owner_bindings::Channel;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

fn context(projected: bool) -> Context {
    let mut ev = Context::new();
    eval(
        &mut ev,
        "(progn (defvar tob-v 5) (defvar tob-log nil)
                (defvar tob-auto 5) (make-variable-buffer-local 'tob-auto)
                (get-buffer-create \" tob-other\")
                (fset 'tob-watch
                      (lambda (sym new op where)
                        (setq tob-log (cons (list sym new op (bufferp where)) tob-log))))
                (fset 'tob-unlet-error
                      (lambda (_sym _new op _where)
                        (if (eq op 'unlet) (error \"tob unlet\")))))",
    );
    if projected {
        for name in ["tob-v", "tob-auto"] {
            ev.project_thread_variable_for_test(intern(name))
                .unwrap_or_else(|| panic!("{name} projects"));
        }
    }
    ev
}

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn v() -> SymId {
    intern("tob-v")
}

fn records(ev: &Context) -> usize {
    ev.owner.slab().map_or(0, |slab| {
        slab.records_of(v()).count() + slab.records_of(intern("tob-auto")).count()
    })
}

fn canonical_default(ev: &mut Context, id: SymId) -> String {
    print_value(&ev.owner_canonical_value(id, Channel::Default))
}

#[test]
fn a_binding_lives_in_the_owner_slab_not_canonical_storage() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    ev.try_specbind(v(), Value::fixnum(7)).expect("bind");
    assert_eq!(records(&ev), 1);
    assert_eq!(eval(&mut ev, "tob-v"), "7");
    assert_eq!(eval(&mut ev, "(symbol-value 'tob-v)"), "7");
    assert_eq!(eval(&mut ev, "(default-value 'tob-v)"), "7");
    assert_eq!(canonical_default(&mut ev, v()), "5");
    ev.unbind_to(count);
    assert_eq!(records(&ev), 0);
    assert_eq!(eval(&mut ev, "tob-v"), "5");
}

#[test]
fn a_canonical_write_under_a_binding_survives_its_pop() {
    // GNU: thread B's `setq` of a variable B does not bind outlives thread
    // A's `let` of it, because A's pop restores the value its entry saved at
    // A's last switch-in, which is B's write.
    let mut ev = context(true);
    let count = ev.specpdl.len();
    ev.try_specbind(v(), Value::fixnum(7)).expect("bind");
    ev.owner_canonical_store(v(), Channel::Default, Value::fixnum(9));
    assert_eq!(eval(&mut ev, "tob-v"), "7");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "tob-v"), "9");
}

#[test]
fn nested_bindings_and_setq_stay_in_the_slab() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    ev.try_specbind(v(), Value::fixnum(7)).expect("outer");
    let inner = ev.specpdl.len();
    ev.try_specbind(v(), Value::fixnum(8)).expect("inner");
    assert_eq!(eval(&mut ev, "(setq tob-v 11)"), "11");
    assert_eq!(canonical_default(&mut ev, v()), "5");
    ev.unbind_to(inner);
    assert_eq!(eval(&mut ev, "tob-v"), "7");
    assert_eq!(eval(&mut ev, "(setq tob-v 12)"), "12");
    assert_eq!(canonical_default(&mut ev, v()), "5");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "tob-v"), "5");
    assert_eq!(records(&ev), 0);
}

#[test]
fn toplevel_value_is_what_the_outermost_binding_hides() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    ev.try_specbind(v(), Value::fixnum(7)).expect("bind");
    ev.try_specbind(v(), Value::fixnum(8)).expect("bind");
    assert_eq!(eval(&mut ev, "(default-toplevel-value 'tob-v)"), "5");
    assert_eq!(
        eval(&mut ev, "(set-default-toplevel-value 'tob-v 6)"),
        "nil"
    );
    assert_eq!(
        eval(&mut ev, "(list tob-v (default-toplevel-value 'tob-v))"),
        "(8 6)"
    );
    assert_eq!(canonical_default(&mut ev, v()), "6");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "tob-v"), "6");
}

#[test]
fn collection_keeps_owner_binding_values() {
    let mut ev = context(true);
    let count = ev.specpdl.len();
    assert_eq!(
        eval(
            &mut ev,
            "(progn (setq tob-v (concat \"outer-\" \"value\")) nil)"
        ),
        "nil"
    );
    let outer = ev
        .eval_str("(concat \"bound-\" \"outer\")")
        .expect("string");
    ev.try_specbind(v(), outer).expect("outer");
    let inner = ev
        .eval_str("(concat \"bound-\" \"inner\")")
        .expect("string");
    ev.try_specbind(v(), inner).expect("inner");
    ev.owner_canonical_store(v(), Channel::Default, Value::fixnum(1));
    for _ in 0..3 {
        ev.gc_collect_exact();
        let _ = ev.eval_str("(make-list 10000 (concat \"x\" \"y\"))");
    }
    assert_eq!(eval(&mut ev, "tob-v"), "\"bound-inner\"");
    ev.unbind_to(count + 1);
    ev.gc_collect_exact();
    assert_eq!(eval(&mut ev, "tob-v"), "\"bound-outer\"");
    ev.unbind_to(count);
    assert_eq!(eval(&mut ev, "tob-v"), "1");
}

#[test]
fn panic_recovery_restores_owner_bindings_without_lisp() {
    let mut ev = context(true);
    eval(&mut ev, "(add-variable-watcher 'tob-v 'tob-watch)");
    let count = ev.specpdl.len();
    ev.try_specbind(v(), Value::fixnum(7)).expect("outer");
    ev.try_specbind(v(), Value::fixnum(8)).expect("inner");
    eval(&mut ev, "(setq tob-log nil)");
    ev.discard_specpdl_to(count);
    assert_eq!(records(&ev), 0);
    assert_eq!(eval(&mut ev, "(list tob-v tob-log)"), "(5 nil)");
}

/// The same forms, projected and not, give the same answers.
fn same_answers(forms: &[&str]) {
    let mut plain = context(false);
    let mut projected = context(true);
    for form in forms {
        let want = eval(&mut plain, form);
        assert!(
            !want.contains("void-function"),
            "the bare harness lacks a function {form} needs: {want}"
        );
        let got = eval(&mut projected, form);
        assert_eq!(got, want, "{form}");
    }
    assert_eq!(records(&projected), 0, "every binding popped");
}

#[test]
fn watched_bindings_notify_as_unprojected() {
    same_answers(&[
        "(progn (add-variable-watcher 'tob-v 'tob-watch) nil)",
        "(let ((tob-v 7)) (let ((tob-v 8)) (setq tob-v 9)) tob-v)",
        "(prog1 (reverse tob-log) (setq tob-log nil))",
        "(save-current-buffer (set-buffer \" tob-other\") (let ((tob-v 7)) tob-v))",
        "(prog1 (reverse tob-log) (setq tob-log nil))",
        "(progn (add-variable-watcher 'tob-auto 'tob-watch) nil)",
        "(save-current-buffer (set-buffer \" tob-other\") (setq tob-auto 6) (let ((tob-auto 7)) tob-auto))",
        "(let ((tob-auto 8)) (list tob-auto (save-current-buffer (set-buffer \" tob-other\") tob-auto)))",
        "(prog1 (reverse tob-log) (setq tob-log nil))",
    ]);
}

#[test]
fn an_unlet_watcher_error_leaves_the_binding_value() {
    // GNU notifies `unlet` with the entry already popped and the binding
    // still in the cell: an error there keeps the binding's value.
    same_answers(&[
        "(progn (add-variable-watcher 'tob-v 'tob-unlet-error) nil)",
        "(condition-case e (let ((tob-v 70)) tob-v) (error (car (cdr e))))",
        "tob-v",
        "(condition-case e (let ((tob-v 71)) (let ((tob-v 72)) tob-v)) (error (car (cdr e))))",
        "tob-v",
        "(progn (remove-variable-watcher 'tob-v 'tob-unlet-error) (setq tob-v 5))",
    ]);
}

#[test]
fn buffer_local_bindings_follow_their_local() {
    same_answers(&[
        "(save-current-buffer (set-buffer \" tob-other\") (setq tob-auto 6))",
        "(save-current-buffer (set-buffer \" tob-other\") (let ((tob-auto 17)) (list tob-auto (default-value 'tob-auto))))",
        "(save-current-buffer (set-buffer \" tob-other\") tob-auto)",
        "(save-current-buffer (set-buffer \" tob-other\")
           (let ((tob-auto 18))
             (kill-local-variable 'tob-auto)
             (list tob-auto (local-variable-p 'tob-auto))))",
        "(save-current-buffer (set-buffer \" tob-other\") (list tob-auto (local-variable-p 'tob-auto)))",
        "(save-current-buffer (set-buffer \" tob-other\")
           (setq tob-auto 6)
           (let ((tob-auto 19))
             (kill-local-variable 'tob-auto)
             (setq tob-auto 20)
             (list tob-auto (local-variable-p 'tob-auto))))",
        "(save-current-buffer (set-buffer \" tob-other\") (list tob-auto (local-variable-p 'tob-auto)))",
        "(let ((tob-auto 21)) (list tob-auto (default-value 'tob-auto)
                                    (save-current-buffer (set-buffer \" tob-other\") tob-auto)))",
        "(let ((tob-auto 22)) (setq tob-auto 23) (list tob-auto (default-value 'tob-auto)))",
        "(list tob-auto (default-value 'tob-auto) (local-variable-p 'tob-auto))",
        "(let ((tob-v 30)) (save-current-buffer (set-buffer (get-buffer-create \" tob-tmp\")) (set (make-local-variable 'tob-v) tob-v)
                             (list tob-v (local-variable-p 'tob-v))))",
    ]);
}

#[test]
fn buffer_local_value_answers_the_owner_local_binding() {
    same_answers(&[
        "(save-current-buffer (set-buffer \" tob-other\") (setq tob-auto 6)
           (let ((tob-auto 31))
             (list (buffer-local-value 'tob-auto (current-buffer)) tob-auto
                   (default-value 'tob-auto))))",
        "(let ((tob-auto 36))
           (list (buffer-local-value 'tob-auto (get-buffer \" tob-other\"))
                 (buffer-local-value 'tob-auto (current-buffer))))",
        "(buffer-local-value 'tob-auto (get-buffer \" tob-other\"))",
    ]);
}

#[test]
fn kill_all_local_variables_captures_owner_local_bindings() {
    same_answers(&[
        "(save-current-buffer (set-buffer \" tob-other\") (setq tob-auto 6)
           (let ((tob-auto 32))
             (kill-all-local-variables)
             (list tob-auto (local-variable-p 'tob-auto))))",
        "(save-current-buffer (set-buffer \" tob-other\") (list tob-auto (local-variable-p 'tob-auto)))",
        "(save-current-buffer (set-buffer \" tob-other\") (setq tob-auto 6)
           (let ((tob-auto 33))
             (kill-all-local-variables)
             (setq tob-auto 34)
             (list tob-auto (local-variable-p 'tob-auto))))",
        "(save-current-buffer (set-buffer \" tob-other\") (list tob-auto (local-variable-p 'tob-auto)))",
    ]);
}

#[test]
fn killing_a_buffer_drops_owner_bindings_of_its_locals() {
    same_answers(&[
        "(let ((b (get-buffer-create \" tob-k\")))
           (save-current-buffer (set-buffer b)
             (setq tob-auto 7)
             (let ((tob-auto 35))
               (kill-buffer b)
               (list tob-auto (default-value 'tob-auto) (buffer-live-p b)))))",
        "(list tob-auto (default-value 'tob-auto))",
    ]);
}

#[test]
fn defvaralias_refuses_an_owner_bound_variable() {
    same_answers(&[
        "(let ((tob-v 1)) (condition-case e (defvaralias 'tob-v 'tob-log) (error (car e))))",
        "(list tob-v (indirect-variable 'tob-v))",
    ]);
}

/// Callgrind micro: `TOB_MICRO_N` `let`/unbind pairs of `tob-v` through the
/// VM's bind and unbind entry points. `TOB_MICRO` picks the shape: `plain`
/// (a dormant `Let`), `plain-watched`, `owner` or `owner-watched` (an owner
/// binding). Run with `--ignored` at N = 0 and N under callgrind; the
/// per-pair cost is the difference over N.
#[test]
#[cfg(feature = "perf-probes")]
fn owner_let_pop_micro() {
    let mode = std::env::var("TOB_MICRO").unwrap_or_else(|_| "owner".into());
    let n: i64 = std::env::var("TOB_MICRO_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    let mut ev = context(mode.starts_with("owner"));
    if mode.ends_with("watched") {
        eval(
            &mut ev,
            "(progn (fset 'tob-ignore (lambda (&rest _) nil))
                    (add-variable-watcher 'tob-v 'tob-ignore))",
        );
    }
    let (var, count) = (v(), ev.specpdl.len());
    for i in 0..n {
        ev.try_specbind(var, Value::fixnum(i)).expect("bind");
        let _ = ev.unbind_to_with_result(count, Ok(Value::NIL));
    }
    assert_eq!(ev.specpdl.len(), count);
}
