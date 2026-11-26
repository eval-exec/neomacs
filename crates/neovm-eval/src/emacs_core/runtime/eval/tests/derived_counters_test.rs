//! v0.0.21 #778: the `*-consed` DEFVAR_INTs read the live counters.
//!
//! GNU binds the seven variables to the same `intmax_t` slots that
//! `memory-use-counts` reports (`src/alloc.c:7429-7448`), so a read sees the
//! running allocation count and the variable always agrees with its
//! `(memory-use-counts)` field. Neomacs derives the value from its allocation
//! accounting at read time; these rows drive that through Lisp.
//!
//! The unit runtime is pre-loadup, so the probes use special forms and subrs
//! only (`while`, `let`, `setq`, `progn`); `dotimes`/`dolist` live in subr.el.

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

fn context() -> Context {
    Context::new(crate::test_utils::unit_runtime())
}

/// Direct references take the ordinary symbol-read fast path. Each counter
/// must be live (nonzero after an allocation of that kind), and the added
/// symbol/interval allocation accounting must have advanced them.
#[test]
fn consed_variables_read_live_counters_on_direct_references() {
    let mut context = context();
    let value = context
        .eval_str(
            r#"(progn
                 (make-list 100 nil)
                 (let ((i 0)) (while (< i 100) (* 1.5 i) (setq i (1+ i))))
                 (make-vector 100 nil)
                 (let ((i 0)) (while (< i 100) (make-symbol "derived-counter-s") (setq i (1+ i))))
                 (make-string 100 ?x)
                 (let ((i 0)) (while (< i 100) (propertize "x" 'face 'bold) (setq i (1+ i))))
                 (let ((i 0)) (while (< i 100) (make-string 10 ?x) (setq i (1+ i))))
                 (list (> cons-cells-consed 0)
                       (> floats-consed 0)
                       (> vector-cells-consed 0)
                       (> symbols-consed 0)
                       (> string-chars-consed 0)
                       (> intervals-consed 0)
                       (> strings-consed 0)))"#,
        )
        .expect("direct reads of the consed counters");
    assert_eq!(
        value,
        Value::list(vec![Value::T; 7]),
        "every *-consed variable must read a live counter"
    );
}

/// One delta row: around one allocation of that kind, the variable's delta
/// equals the matching `memory-use-counts` delta and is positive.
fn delta_row(counter_index: usize, symbol: &str, allocate: &str) -> String {
    format!(
        r#"(let ((v0 (symbol-value '{symbol}))
                   (u0 (nth {counter_index} (memory-use-counts))))
             {allocate}
             (let ((v1 (symbol-value '{symbol}))
                   (u1 (nth {counter_index} (memory-use-counts))))
               (if (and (= (- v1 v0) (- u1 u0)) (> (- v1 v0) 0))
                   t
                 (setq ok nil))))"#
    )
}

#[test]
fn consed_variable_deltas_match_memory_use_counts_fields() {
    let mut context = context();
    let mut form = String::from("(let ((ok t))\n");
    form.push_str(&delta_row(0, "cons-cells-consed", "(make-list 1000 nil)"));
    form.push_str(&delta_row(
        1,
        "floats-consed",
        "(let ((i 0)) (while (< i 1000) (* 1.5 i) (setq i (1+ i))))",
    ));
    form.push_str(&delta_row(
        2,
        "vector-cells-consed",
        "(make-vector 1000 nil)",
    ));
    form.push_str(&delta_row(
        3,
        "symbols-consed",
        "(let ((i 0)) (while (< i 1000) (make-symbol \"s\") (setq i (1+ i))))",
    ));
    form.push_str(&delta_row(
        4,
        "string-chars-consed",
        "(make-string 1000 ?x)",
    ));
    form.push_str(&delta_row(
        5,
        "intervals-consed",
        "(let ((i 0)) (while (< i 1000) (propertize \"x\" 'face 'bold) (setq i (1+ i))))",
    ));
    form.push_str(&delta_row(
        6,
        "strings-consed",
        "(let ((i 0)) (while (< i 1000) (make-string 10 ?x) (setq i (1+ i))))",
    ));
    form.push_str("ok)");

    let value = context
        .eval_str(&form)
        .expect("delta protocol over the consed counters");
    assert_eq!(
        value,
        Value::T,
        "every counter delta must match and advance"
    );
}
