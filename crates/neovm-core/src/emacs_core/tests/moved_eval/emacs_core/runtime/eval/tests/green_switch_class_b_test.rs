use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::value::Value;
fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    crate::test_utils::new_green_context()
}

#[test]
fn a_sim_worlds_waits_never_slice() {
    neovm_eval::test_support::authority::run_a_sim_worlds_waits_never_slice_for_test(
        crate::test_utils::new_context,
    );
}

#[test]
fn d1_a_thread_sees_the_time_zone_rule_set_by_another() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (set-time-zone-rule \"XXX-5\")
               (list (current-time-zone)
                     (thread-join (make-thread (lambda () (current-time-zone))))))"
        ),
        "((18000 \"XXX\") (18000 \"XXX\"))"
    );
}

#[test]
fn d1_a_thread_sees_a_process_made_by_another() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgd1-p (make-pipe-process :name \"tgp\"))
               (prog1 (thread-join (make-thread
                                    (lambda ()
                                      (list (format \"%S\" tgd1-p) (process-name tgd1-p)
                                            (and (memq tgd1-p (process-list)) t)))))
                 (delete-process tgd1-p)))"
        ),
        "(\"#<process tgp>\" \"tgp\" t)"
    );
}

#[test]
fn fd_readiness_wakes_a_green_select_while_a_peer_yields() {
    neovm_eval::test_support::authority::run_fd_readiness_wakes_a_green_select_while_a_peer_yields_for_test(
        crate::test_utils::new_green_context,
    );
}
