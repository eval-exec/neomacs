//! GNU's `*-consed` variables are `DEFVAR_INT`s over the C globals
//! `Fmemory_use_counts` reads (`src/alloc.c:7190-7196`, `:7429-7449`), so each
//! variable and the matching `memory-use-counts` field are one number.

use crate::emacs_core::eval::Context;
use neovm_value::value::{Value, buffer_vocab::BufferId};

fn holds(eval: &mut Context, form: &str) -> bool {
    eval.eval_str(form)
        .unwrap_or_else(|error| panic!("{form}: {error:?}"))
        .is_truthy()
}

/// Evaluate FORM and compare its printed value with EXPECTED.
fn assert_returns(eval: &mut Context, form: &str, expected: &str) {
    assert!(
        holds(
            eval,
            &format!("(progn (setq consed-counter-result {form}) t)")
        ),
        "{form}"
    );
    let printed = eval
        .eval_str("(prin1-to-string consed-counter-result)")
        .unwrap_or_else(|error| panic!("{form}: {error:?}"));
    assert_eq!(printed.as_utf8_str(), Some(expected), "{form}");
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

/// GNU's forwarded variable holds the loaded binding (`swap_in_symval_forwarding`,
/// `src/data.c:1574-1604`), so while a buffer's local binding is loaded the
/// allocator counts into it and the default keeps the value it was unloaded
/// with (`default_value`, `src/data.c:1962-1983`).
#[test]
fn localized_consed_loaded_local_binding_takes_the_count() {
    let _context = Context::new(crate::test_utils::unit_runtime());
    let mut obarray = neovm_value::symbol::Obarray::new();
    let symbol = neovm_value::intern::intern("cons-cells-consed");
    obarray.seed_value("cons-cells-consed", Value::fixnum(0));
    let _blv = obarray.make_symbol_localized(symbol, Value::fixnum(0));
    obarray.define_counter_variable(
        "cons-cells-consed",
        neovm_value::tagged::gc::MemoryUseCountSlot::ConsCells,
    );

    obarray.set_blv_local_if_set(symbol, true);
    let buffer_id = BufferId(7001);
    let local_alist = obarray
        .set_internal_localized(
            symbol,
            Value::fixnum(777),
            Value::make_buffer(buffer_id),
            Value::NIL,
            neovm_value::symbol::SetInternalBind::Set,
            false,
        )
        .into_value();

    let before_default = obarray.default_value_id_copied(symbol).unwrap();
    let _allocated = Value::cons(Value::fixnum(1), Value::NIL);
    let after_count = Value::memory_use_counts_snapshot()[0];
    let after_default = obarray.default_value_id_copied(symbol).unwrap();
    let local_value = obarray
        .read_localized_for_buffer(symbol, buffer_id, local_alist)
        .unwrap();

    assert!(after_count > 777);
    assert_eq!(local_value, Value::fixnum(after_count as i64));
    assert_eq!(after_default, before_default);
}

/// A counter declaration that runs after Lisp has already localized the
/// symbol must replace the BLV's missing descriptor with the live counter.
#[test]
fn localized_consed_counter_declared_after_localization_uses_live_slot() {
    let _context = Context::new(crate::test_utils::unit_runtime());
    let mut obarray = neovm_value::symbol::Obarray::new();
    let symbol = neovm_value::intern::intern("cons-cells-consed");
    obarray.seed_value("cons-cells-consed", Value::fixnum(0));
    assert!(
        obarray
            .make_symbol_localized(symbol, Value::fixnum(0))
            .is_some()
    );

    obarray.define_counter_variable(
        "cons-cells-consed",
        neovm_value::tagged::gc::MemoryUseCountSlot::ConsCells,
    );
    assert!(
        obarray
            .blv(symbol)
            .and_then(|blv| blv.fwd)
            .is_some_and(|fwd| fwd.reads_allocation_counter())
    );
}

/// The current-value fast path reads the forwarded default while the current
/// buffer has no local binding, but leaves an explicit local cell unchanged.
#[test]
fn localized_consed_read_without_local_binding_follows_counter() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    assert!(holds(
        &mut eval,
        "(progn
           (setq consed-counter-buffer-a (get-buffer-create \" *consed-counter-a*\")
                 consed-counter-buffer-b (get-buffer-create \" *consed-counter-b*\"))
           (set-buffer consed-counter-buffer-a)
           (make-local-variable 'cons-cells-consed)
           (set-buffer consed-counter-buffer-b)
           (setq consed-counter-before cons-cells-consed
                 consed-counter-local-before
                   (buffer-local-value 'cons-cells-consed consed-counter-buffer-a))
           (make-list 12000 nil)
           (setq consed-counter-after cons-cells-consed
                 consed-counter-memory-after (nth 0 (memory-use-counts))
                 consed-counter-local-after
                   (buffer-local-value 'cons-cells-consed consed-counter-buffer-a))
           t)"
    ));
    assert!(holds(
        &mut eval,
        "(> consed-counter-after consed-counter-before)"
    ));
    assert!(holds(
        &mut eval,
        "(= consed-counter-after consed-counter-memory-after)"
    ));
    assert!(holds(
        &mut eval,
        "(= consed-counter-local-after consed-counter-local-before)"
    ));
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

/// Allocation counts into the binding that is loaded: a buffer's local
/// binding while that buffer is current, the default while a buffer without
/// one is, and an unloaded local binding keeps its value until it is loaded
/// again. `make-local-variable` loads the new binding at once
/// (`src/data.c:2334-2342`; `swap_in_symval_forwarding`, `src/data.c:1574-1604`;
/// `buffer_local_value` reads an unloaded cell, `src/buffer.c:1376-1399`).
const LOCAL_BINDING_COUNTS_FORM: &str = "
(let ((a (get-buffer-create \" *consed-local-a*\"))
      (b (get-buffer-create \" *consed-local-b*\"))
      created local-before local-low local-field local-high
      default-before default-after saved-1 saved-2 back)
  (set-buffer a)
  (setq created cons-cells-consed)
  (make-local-variable 'cons-cells-consed)
  (make-list 1000 nil)
  (setq local-before cons-cells-consed)
  (make-list 1000 nil)
  (setq local-low cons-cells-consed
        local-field (car (memory-use-counts))
        local-high cons-cells-consed)
  (set-buffer b)
  (setq default-before cons-cells-consed
        saved-1 (buffer-local-value 'cons-cells-consed a))
  (make-list 1000 nil)
  (setq default-after cons-cells-consed
        saved-2 (buffer-local-value 'cons-cells-consed a))
  (set-buffer a)
  (setq back cons-cells-consed)
  (list (>= (- local-before created) 1000)
        (>= (- local-low local-before) 1000)
        (<= local-low local-field local-high)
        (>= (- default-after default-before) 1000)
        (= saved-1 saved-2)
        (<= saved-1 back)
        (< (- back saved-1) 1000)))";

#[test]
fn localized_consed_local_binding_counts_while_its_buffer_is_current() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    assert_returns(&mut eval, LOCAL_BINDING_COUNTS_FORM, "(t t t t t t t)");
}
