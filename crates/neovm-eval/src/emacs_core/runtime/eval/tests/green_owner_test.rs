//! Phase 0 step 2, round 3d: further Green owners over one World, each on
//! its own OS thread, built only by the thread holding the World's Baton.
//!
//! These tests hand the Baton over by hand; the scheduler (group 3) does it
//! at thread switches. The first owner publishes no parked roots here yet,
//! so the further owner runs with collection inhibited.

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use std::sync::mpsc;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    Context::new_with_threading(ThreadingMode::Green)
}

#[test]
fn a_further_owner_shares_the_world_and_keeps_its_own_bindings() {
    let mut main = green();
    eval(
        &mut main,
        "(progn (defvar tgo-v 1)
                (set-buffer (get-buffer-create \"tgo-buf\"))
                (insert \"abc\"))",
    );
    let baton = main.start_green_runtime().expect("start the Green runtime");
    let (to_main, from_worker) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        baton.enter();
        let mut owner = Context::new_green_owner(&baton);
        owner.gc_inhibit_depth += 1;
        let seen = [
            eval(&mut owner, "tgo-v"),
            eval(&mut owner, "(buffer-name)"),
            eval(&mut owner, "(buffer-string)"),
            eval(&mut owner, "(progn (setq tgo-v 2) (let ((tgo-v 3)) tgo-v))"),
            eval(
                &mut owner,
                "(list (+ 1 2) (car '(a b)) (symbol-function 'car))",
            ),
            eval(
                &mut owner,
                "(eq (symbol-function 'car) (symbol-function 'car))",
            ),
        ];
        owner.gc_inhibit_depth -= 1;
        owner.retire_green_owner();
        to_main
            .send((baton, seen))
            .expect("the first owner waits for the Baton");
    });
    let (baton, seen) = from_worker.recv().expect("the Baton comes back");
    baton.enter();
    worker.join().expect("the further owner finished");
    assert_eq!(
        seen,
        [
            "1".to_string(),
            "\"tgo-buf\"".to_string(),
            "\"abc\"".to_string(),
            "3".to_string(),
            "(3 a #<subr car>)".to_string(),
            "t".to_string(),
        ]
    );
    // The further owner's global write is the World's; its binding was its own.
    assert_eq!(eval(&mut main, "tgo-v"), "2");
    assert_eq!(eval(&mut main, "(buffer-string)"), "\"abc\"");
}

#[test]
fn the_canonical_subr_object_is_the_same_on_every_owner() {
    let mut main = green();
    let baton = main.start_green_runtime().expect("start the Green runtime");
    eval(&mut main, "(defvar tgo-subr (symbol-function 'car))");
    let (to_main, from_worker) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        baton.enter();
        let mut owner = Context::new_green_owner(&baton);
        owner.gc_inhibit_depth += 1;
        let same = eval(&mut owner, "(eq tgo-subr (symbol-function 'car))");
        let funcall = eval(&mut owner, "(funcall tgo-subr '(1 2))");
        owner.gc_inhibit_depth -= 1;
        owner.retire_green_owner();
        to_main.send((baton, same, funcall)).expect("send back");
    });
    let (baton, same, funcall) = from_worker.recv().expect("the Baton comes back");
    baton.enter();
    worker.join().expect("the further owner finished");
    assert_eq!(same, "t");
    assert_eq!(funcall, "1");
}

#[cfg(debug_assertions)]
#[test]
fn a_view_made_off_the_baton_holder_panics() {
    let mut main = green();
    let baton = main.start_green_runtime().expect("start the Green runtime");
    let outcome = std::thread::spawn(move || {
        // This thread has the Baton object but never entered it: it is not
        // the holder, so it may not build views.
        let _views = baton.owner_views(None);
    })
    .join();
    assert!(outcome.is_err(), "building views off the holder panics");
}

#[test]
fn a_sim_world_refuses_the_green_runtime() {
    crate::test_utils::init_test_tracing();
    let mut sim = Context::new();
    assert!(matches!(
        sim.start_green_runtime(),
        Err(super::GreenStartError::NotGreen)
    ));
}
