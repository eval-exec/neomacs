//! P11 soak finding #2 (scheduler half): a finished Lisp thread must stop
//! charging the scheduler registry.
//!
//! `finish` publishes `Wait::Exited` so joiners and the native main observe
//! the end, but the wait entry and the thread's wake sender used to stay in
//! `waits`/`senders` forever: every later scheduler scan walked one dead
//! record per thread the World had ever run. The end stays typed as the
//! `Wait::Exited` arm published before retirement, and `exited` reads an
//! absent wait as the same fact, so `thread-join` keeps working. The
//! registry-root half of the finding (the Lisp thread objects themselves)
//! belongs to P15 THREADOBJ and is not touched here.

use std::sync::Arc;

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;

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
fn a_spawn_join_loop_leaves_the_registry_at_its_pre_loop_size() {
    let mut main = green();
    main.start_green_threads().expect("start the Green runtime");
    let control = main.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let before = control.registry_sizes_for_test();
    assert_eq!(
        before,
        (1, 1),
        "the Green runtime registers exactly main before any worker exists"
    );

    const ROUNDS: usize = 32;
    for round in 0..ROUNDS {
        let form = format!(
            "(let ((worker (make-thread (lambda () {round}) \"retirement-{round}\"))) \
               (thread-join worker))"
        );
        assert_eq!(
            eval(&mut main, &form),
            round.to_string(),
            "round {round} must join its worker"
        );
    }

    assert_eq!(
        control.registry_sizes_for_test(),
        before,
        "{ROUNDS} finished workers must leave no wait entry and no sender behind; \
         dead records would otherwise grow the scheduler registry and charge every scan"
    );
}
