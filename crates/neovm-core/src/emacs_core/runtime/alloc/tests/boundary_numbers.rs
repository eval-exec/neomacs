//! Expectations captured from GNU Emacs 31.1 through standalone sandbox runs.
//! Ordinary tests read saved fixtures; they never launch a nested GNU sandbox.
use crate::test_utils::runtime_startup_eval_one;

fn oracle(_form: &str, fixture: &str) -> String {
    format!("OK {}", fixture.trim_end())
}

#[test]
fn record_slots_include_type_and_obey_gnu_limit() {
    let form = r#"(list
      (length (make-record 'foo 4094 nil))
      (condition-case e (length (make-record 'foo 4095 nil)) (error e))
      (condition-case e (length (make-record 'foo 5000 nil)) (error e))
      (condition-case e (length (make-record 'foo most-positive-fixnum nil)) (error e)))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/record_slots_include_type_and_obey_gnu_limit.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn obarray_hint_rejects_unrepresentable_bucket_bits() {
    let form = r#"(list (obarrayp (obarray-make 0))
      (condition-case e (obarray-make most-positive-fixnum) (error e))
      (condition-case e (obarray-make -1) (error e)))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/obarray_hint_rejects_unrepresentable_bucket_bits.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn expt_rejects_gnu_limb_limit_before_computation() {
    let form = r#"(list (expt 7 50)
      (condition-case e (expt 7 (ash 1 34)) (error e))
      (expt -1 (ash 1 34)) (expt 0 (ash 1 34)))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/expt_rejects_gnu_limb_limit_before_computation.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn repeated_insertion_rejects_buffer_length_overflow() {
    let form = r#"(with-temp-buffer
      (list (condition-case e (insert-char ?a most-positive-fixnum) (error e))
            (condition-case e (insert-byte 65 most-positive-fixnum) (error e))
            (condition-case e (let ((indent-tabs-mode nil))
                                (indent-to most-positive-fixnum)) (error e))))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/repeated_insertion_rejects_buffer_length_overflow.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn hash_table_size_overflow_is_a_lisp_condition() {
    let form = r#"(list (hash-table-count (make-hash-table :size 0))
      (condition-case e (make-hash-table :size most-positive-fixnum)
        (error (list (car e) (substring-no-properties (cadr e))))))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/hash_table_size_overflow_is_a_lisp_condition.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn self_insert_length_overflow_is_a_lisp_condition() {
    let form = r#"(with-temp-buffer
      (condition-case e (self-insert-command most-positive-fixnum ?a)
        (error (list (car e) (substring-no-properties (cadr e))))))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/self_insert_length_overflow_is_a_lisp_condition.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn char_table_extras_narrow_at_the_gnu_c_int_boundary() {
    let form = r#"(let ((purpose 'tsb-char-table))
      (list (progn (put purpose 'char-table-extra-slots 10)
                   (let ((table (make-char-table purpose 42)))
                     (list (char-table-p table) (char-table-extra-slot table 9))))
            (progn (put purpose 'char-table-extra-slots most-positive-fixnum)
                   (char-table-p (make-char-table purpose)))))"#;
    let expected = oracle(
        form,
        include_str!("boundary_numbers/char_table_extras_narrow_at_the_gnu_c_int_boundary.expect"),
    );
    assert_eq!(runtime_startup_eval_one(form), expected);
}

#[test]
fn hash_table_invalid_weakness_precedes_capacity_failure() {
    let form = r#"(condition-case e
      (make-hash-table :size most-positive-fixnum :weakness 'tsb-invalid-weakness)
      (error e))"#;
    let expected = include_str!(
        "boundary_numbers/hash_table_invalid_weakness_precedes_capacity_failure.expect"
    );
    assert_eq!(
        runtime_startup_eval_one(form),
        format!("OK {}", expected.trim_end())
    );
}

#[test]
fn hash_table_invalid_size_precedes_invalid_weakness() {
    let form = r#"(condition-case e
      (make-hash-table :size -1 :weakness 'tsb-invalid-weakness)
      (error e))"#;
    let expected =
        include_str!("boundary_numbers/hash_table_invalid_size_precedes_invalid_weakness.expect");
    assert_eq!(
        runtime_startup_eval_one(form),
        format!("OK {}", expected.trim_end())
    );
}
