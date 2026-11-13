//! SQLite-feature runtime tests re-homed from eval's VM and Green harnesses.
//!
//! Positive SQL semantics are UNATTESTED: the pinned GNU oracle available
//! for this work was built without sqlite3. These existing success assertions
//! are preserved as Neomacs regression coverage; moving them does not supply
//! an attested GNU comparison. They require the `sqlite` Cargo feature.
//!
//! Core's bare and Green constructors install the canonical runtime hooks.

#![cfg(feature = "sqlite")]

use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::error::EvalError;
use neovm_value::value::Value;

fn with_vm_eval_in_context<R>(
    mut eval: Context,
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    eval.set_lexical_binding(lexical);
    let result = eval.eval_str(src);
    f(result, &eval)
}

fn with_vm_eval<R>(src: &str, lexical: bool, f: impl FnOnce(Result<Value, EvalError>) -> R) -> R {
    with_vm_eval_state(src, lexical, |result, _| f(result))
}

fn with_vm_eval_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(crate::test_utils::new_context(), src, lexical, f)
}

fn vm_eval_str(src: &str) -> String {
    with_vm_eval(src, false, |result| {
        crate::emacs_core::format_eval_result(&result)
    })
}

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
fn vm_sqlite_runtime_uses_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let ((db (sqlite-open)))
                  (list
                   (sqlite-available-p)
                   (fboundp 'sqlitep)
                   (fboundp 'sqlite-open)
                   (sqlitep db)
                   (equal (sqlite-select db "pragma busy_timeout") '((0)))
                   (sqlite-close db)))"##
        ),
        r#"OK (t t t t t t)"#
    );
}

#[test]
fn vm_sqlite_preserves_i64_errors_and_result_set_lifetime() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r##"(let* ((db (sqlite-open))
                       (set (sqlite-select
                             db
                             "select 9223372036854775807 union all select 2"
                             nil
                             'set))
                       (prepare-error
                        (condition-case error-data
                            (sqlite-execute db "select * from sqlite_missing")
                          (sqlite-error error-data))))
                  (sqlite-close db)
                  (list
                   (sqlite-next set)
                   (sqlite-next set)
                   (sqlite-next set)
                   prepare-error
                   (sqlite-close db)))"##
        ),
        r#"OK ((9223372036854775807) (2) nil (sqlite-error ("SQL logic error" "no such table: sqlite_missing" 1 1)) t)"#
    );
}

#[test]
fn d1_a_thread_uses_a_sqlite_handle_opened_by_another() {
    let mut ev = green();
    // GNU 31.1 here was built without sqlite; any GNU with sqlite answers
    // the row the other thread inserted.
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgd1-db (sqlite-open))
               (sqlite-execute tgd1-db \"create table t (x)\")
               (sqlite-execute tgd1-db \"insert into t values (42)\")
               (thread-join (make-thread
                             (lambda () (sqlite-select tgd1-db \"select x from t\")))))"
        ),
        "((42))"
    );
}
