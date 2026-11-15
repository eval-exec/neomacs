//! A green thread's face and display changes must reach the main thread.
//!
//! `internal-make-lisp-face` and `internal-set-lisp-face-attribute` are
//! display-owned subrs (absent-layer stand-ins in eval's unit runtime), so
//! the row moved here runs against core's canonical face install. The
//! assertions are unchanged; the counter accessors are the `_for_test`
//! spellings this crate's moved_eval fixtures already use.

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

#[test]
fn a_threads_face_and_display_changes_reach_main() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ev = crate::test_utils::new_green_context();
    let faces = ev.face_change_count_for_test();
    let display = ev.display_var_change_count_for_test();
    assert_eq!(
        eval(
            &mut ev,
            "(thread-join (make-thread (lambda ()
               (condition-case err
                   (progn
                     (internal-make-lisp-face 'tg3w-face)
                     (internal-set-lisp-face-attribute 'tg3w-face :foreground \"green\")
                     (set (make-local-variable (quote truncate-lines)) t)
                     'done)
                 (error err)))))"
        ),
        "done"
    );
    assert!(ev.face_change_count_for_test() > faces);
    assert!(ev.display_var_change_count_for_test() > display);
}
