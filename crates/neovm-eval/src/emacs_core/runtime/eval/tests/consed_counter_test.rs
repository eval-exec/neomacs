//! GNU's `*-consed` variables are `DEFVAR_INT`s over the C globals
//! `Fmemory_use_counts` reads (`src/alloc.c:7190-7196`, `:7429-7449`), so each
//! variable and the matching `memory-use-counts` field are one number.

use crate::emacs_core::eval::Context;

fn holds(eval: &mut Context, form: &str) -> bool {
    eval.eval_str(form)
        .unwrap_or_else(|error| panic!("{form}: {error:?}"))
        .is_truthy()
}

/// Every variable with its `memory-use-counts` field.
const FIELDS: [(&str, usize); 7] = [
    ("cons-cells-consed", 0),
    ("floats-consed", 1),
    ("vector-cells-consed", 2),
    ("symbols-consed", 3),
    ("string-chars-consed", 4),
    ("intervals-consed", 5),
    ("strings-consed", 6),
];

/// A variable with a form that allocates at least 1000 of its unit.
const ALLOCATIONS: [(&str, &str); 5] = [
    ("cons-cells-consed", "(make-list 1000 nil)"),
    (
        "floats-consed",
        "(let ((i 0)) (while (< i 1000) (setq i (1+ i)) (* 1.5 i)))",
    ),
    ("vector-cells-consed", "(make-vector 1000 nil)"),
    ("string-chars-consed", "(make-string 1000 ?x)"),
    (
        "strings-consed",
        "(let ((i 0)) (while (< i 1000) (setq i (1+ i)) (make-string 1 ?x)))",
    ),
];

#[test]
fn consed_variables_advance_with_allocation() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    for (variable, allocate) in ALLOCATIONS {
        assert!(
            holds(
                &mut eval,
                &format!("(let ((before {variable})) {allocate} (>= (- {variable} before) 1000))"),
            ),
            "{variable} counts {allocate}"
        );
        assert!(
            holds(
                &mut eval,
                &format!("(let ((before {variable})) (garbage-collect) (>= {variable} before))"),
            ),
            "{variable} is cumulative across a collection"
        );
    }
}

/// The field is read between two reads of the variable, so the comparison
/// does not depend on what evaluating the form itself allocates.
#[test]
fn consed_variables_read_the_memory_use_counts_fields() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    for (_, allocate) in ALLOCATIONS {
        assert!(holds(&mut eval, &format!("(progn {allocate} t)")));
    }
    for (variable, field) in FIELDS {
        assert!(
            holds(
                &mut eval,
                &format!(
                    "(let* ((low {variable})
                            (field (nth {field} (memory-use-counts)))
                            (high {variable}))
                       (and (<= low field) (<= field high)))"
                ),
            ),
            "{variable} is field {field} of memory-use-counts"
        );
    }
}

/// A Lisp assignment assigns the C global in GNU, so the function reports the
/// assigned value and the allocator counts on from it.
#[test]
fn assigning_a_consed_variable_assigns_the_counter() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    assert!(
        holds(
            &mut eval,
            "(let ((target (+ cons-cells-consed 1000000)))
               (setq cons-cells-consed target)
               (and (>= cons-cells-consed target)
                    (>= (nth 0 (memory-use-counts)) target)
                    (progn (make-list 1000 nil)
                           (>= cons-cells-consed (+ target 1000)))))",
        ),
        "setq moves the counter memory-use-counts reports"
    );
    assert!(
        holds(
            &mut eval,
            "(eq (condition-case nil (setq cons-cells-consed \"x\")
                   (wrong-type-argument 'refused))
                 'refused)",
        ),
        "the DEFVAR_INT type rule still applies"
    );
    assert!(
        holds(
            &mut eval,
            "(let ((inside (let ((cons-cells-consed 5)) cons-cells-consed)))
               (and (>= inside 5) (< inside 1000)
                    (>= cons-cells-consed 1000000)))",
        ),
        "a let binding assigns the counter and restores it"
    );
}
