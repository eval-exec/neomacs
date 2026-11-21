//! GNU's `*-consed` variables are `DEFVAR_INT`s over the C globals
//! `Fmemory_use_counts` reads (`src/alloc.c:7190-7196`, `:7429-7449`).
//! A read, an assignment and a `let` binding act on that one number; once
//! the variable is buffer-local, the number belongs to the loaded binding
//! (`swap_in_symval_forwarding`, `src/data.c:1574-1604`). Allocation counts
//! differ between editors, so each case prints comparisons, not counts.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The variable is read on both sides of the `memory-use-counts` call.
#[test]
fn consed_counter_reads_the_memory_use_counts_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (make-list 1000 nil)
  (let* ((low cons-cells-consed)
         (field (car (memory-use-counts)))
         (high cons-cells-consed))
    (<= low field high)))
"##;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// An assignment sets the counter, and a `let` binding sets it and restores
/// the outer value on exit.
#[test]
fn consed_counter_assignment_and_let_binding_set_the_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((target (+ cons-cells-consed 100000000)))
  (setq cons-cells-consed target)
  (let ((after-set (car (memory-use-counts))))
    (make-list 1000 nil)
    (list (<= target after-set)
          (<= (+ target 1000) cons-cells-consed)
          (let ((inside (let ((cons-cells-consed 5)) cons-cells-consed)))
            (and (<= 5 inside) (< inside 1000)))
          (<= (+ target 1000) cons-cells-consed))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The local binding counts while its buffer is current, the default counts
/// in a buffer without one, and an unloaded binding keeps its value
/// (`buffer_local_value`, `src/buffer.c:1376-1399`) until it is loaded again.
#[test]
fn consed_counter_counts_into_the_loaded_buffer_local_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((a (get-buffer-create " *consed-local-a*"))
      (b (get-buffer-create " *consed-local-b*"))
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
        (< (- back saved-1) 1000)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `set-default` under a loaded local binding writes only the default cell
/// (`set_default_internal`, `src/data.c:2068-2079`); the cached default is
/// restored into the counter when a buffer without a local binding is
/// current.
#[test]
fn consed_counter_default_written_under_a_local_binding_is_restored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((a (get-buffer-create " *consed-default-a*"))
      (b (get-buffer-create " *consed-default-b*"))
      target in-a default-in-a seen)
  (set-buffer a)
  (make-local-variable 'cons-cells-consed)
  (setq target (+ cons-cells-consed 100000000))
  (set-default 'cons-cells-consed target)
  (setq in-a cons-cells-consed
        default-in-a (default-value 'cons-cells-consed))
  (set-buffer b)
  (setq seen cons-cells-consed)
  (list (< in-a target)
        (= default-in-a target)
        (<= target seen)
        (< (- seen target) 1000)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
