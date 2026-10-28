use crate::emacs_core::eval::Context;
use std::time::{Duration, Instant};

fn assert_full_timeout(context: &mut Context, form: &str) {
    context
        .eval_str("(setq noninteractive t timer-list nil timer-idle-list nil)")
        .expect("batch mode with no timers");
    let expression = context
        .eval_str(&format!("'{form}"))
        .expect("read wait form");
    let start = Instant::now();
    context.eval_form(expression).expect("timed wait");
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(50),
        "{form} returned early after {elapsed:?}"
    );
}

#[test]
fn accept_process_output_never_returns_before_50ms() {
    assert_full_timeout(&mut Context::new(), "(accept-process-output nil 0.05)");
}

#[test]
fn sleep_for_never_returns_before_50ms() {
    assert_full_timeout(&mut Context::new(), "(sleep-for 0.05)");
}

#[test]
fn sit_for_in_batch_never_returns_before_50ms() {
    // Exercise GNU's actual batch sit-for definition (which calls sleep-for).
    let mut context = super::gnu_subr_sit_for_eval();
    assert_full_timeout(&mut context, "(sit-for 0.05)");
}

#[test]
fn sleep_for_fine_elapsed_matches_gnu() {
    const FORM: &str = "(let ((t0 (float-time))) (sleep-for 0.05) (>= (- (float-time) t0) 0.05))";
    let fixture = crate::test_utils::workspace_root().join(
        "crates/neovm-core/src/emacs_core/system/timer/tests/fixtures/sleep_fine_elapsed.txt",
    );
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        let emacs = crate::test_utils::gnu_oracle().executable_path();
        let output = std::process::Command::new(emacs)
            .args(["--quick", "--batch", "--eval", &format!("(prin1 {FORM})")])
            .output()
            .expect("run GNU reference inside the campaign sandbox");
        assert!(output.status.success(), "{output:?}");
        std::fs::write(&fixture, &output.stdout).expect("save GNU expectation");
    }
    let expected = std::fs::read_to_string(fixture).expect("refresh from GNU with UPDATE_EXPECT=1");
    let mut context = Context::new();
    let result = context.eval_str(FORM).expect("measure sleep-for");
    assert_eq!(crate::emacs_core::print::print_value(&result), expected);
}

fn green_timed_wait_context(sit_for: bool) -> Context {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut context = Context::new_with_threading(crate::emacs_core::eval::ThreadingMode::Green);
    if sit_for {
        let source =
            std::fs::read_to_string(crate::test_utils::workspace_root().join("lisp/subr.el"))
                .expect("read GNU subr.el");
        super::install_bare_elisp_shims(&mut context);
        context.set_lexical_binding(true);
        super::eval_first_form_after_marker(
            &mut context,
            &source,
            "(defun sit-for (seconds &optional nodisp)",
        );
    }
    context
}

fn assert_green_full_timeout(form: &str, sit_for: bool) {
    let mut context = green_timed_wait_context(sit_for);
    context
        .eval_str(
            "(progn
           (setq noninteractive t timer-list nil timer-idle-list nil g3f-clock-stop nil)
           (setq g3f-clock-peer
             (make-thread (lambda ()
               (while (null g3f-clock-stop) (thread-yield)))))
           (thread-yield))",
        )
        .expect("start a peer that remains runnable for the timed wait");
    let expression = context
        .eval_str(&format!("'{form}"))
        .expect("read timed wait");
    let start = Instant::now();
    context
        .eval_form(expression)
        .expect("wait while the peer runs");
    let elapsed = start.elapsed();
    context
        .eval_str("(progn (setq g3f-clock-stop t) (thread-join g3f-clock-peer))")
        .expect("join the yielding peer");
    assert!(
        elapsed >= Duration::from_millis(50),
        "Green {form} returned early after {elapsed:?}"
    );
}

#[test]
fn green_accept_process_output_never_returns_before_50ms_with_a_runnable_peer() {
    assert_green_full_timeout("(accept-process-output nil 0.05)", false);
}

#[test]
fn green_sleep_for_never_returns_before_50ms_with_a_runnable_peer() {
    assert_green_full_timeout("(sleep-for 0.05)", false);
}

#[test]
fn green_sit_for_in_batch_never_returns_before_50ms_with_a_runnable_peer() {
    assert_green_full_timeout("(sit-for 0.05)", true);
}
