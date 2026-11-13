use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::intern::{SymId, intern};

fn context(projected: bool) -> Context {
    let mut ev = crate::test_utils::new_context();
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
    neovm_eval::test_support::authority::owner_records_for_test(ev, v())
        .map_or(0, |records| records.len())
        + neovm_eval::test_support::authority::owner_records_for_test(ev, intern("tob-auto"))
            .map_or(0, |records| records.len())
}

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
