use super::*;

use self::command::CclCommand;
use self::expr::{eval_expr_self, eval_set_expr};
use self::extension::{ExtensionStep, MapMultipleState, execute_extension};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};
use neovm_value::intern::SymId;
use neovm_value::tagged::value::SymbolMode;
use std::cell::Cell;
use std::collections::HashMap;

use neovm_text::emacs_core::ccl::*;
use neovm_value::value::Value;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::intern::intern;

use neovm_value::value::ValueKind;

#[test]
fn ccl_execute_call_runs_the_registered_program_and_returns() {
    crate::test_utils::init_test_tracing();
    let callee = Value::vector([0, 3, 1793, 22].into_iter().map(Value::fixnum).collect());
    let id = builtin_register_ccl_program_impl(vec![Value::symbol("ccl-call-callee"), callee])
        .expect("callee should register")
        .as_int()
        .unwrap();
    // Opcode 0x13 with register field 1: the following word is the program id.
    // Then (r1 = 3), which is the word 801 GNU emits after `call`.
    let caller = Value::vector(
        [0, 5, 51, id, 801, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![Value::NIL; 8]);
    builtin_ccl_execute_impl(vec![caller, registers]).expect("call should return");
    assert_eq!(registers.as_vector_data().unwrap()[0], Value::fixnum(7));
    assert_eq!(registers.as_vector_data().unwrap()[1], Value::fixnum(3));
}

#[test]
fn ccl_execute_iterate_multiple_map_calls_a_program_then_continues() {
    crate::test_utils::init_test_tracing();
    // GNU `(iterate-multiple-map r1 r0 mi)` then `(r2 = 5)`. The map slot is
    // a CCL program that sets r0 to 9 and r1 to 4. Execution resumes after
    // the map list, so r2 becomes 5.
    let callee = Value::vector(
        [0, 4, 2305, 1057, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    builtin_register_ccl_program_impl(vec![Value::symbol("ccl-iter-mapper"), callee])
        .expect("mapper should register");
    let map = Value::vector(vec![Value::fixnum(0), Value::symbol("ccl-iter-mapper")]);
    let map_id =
        builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-iter-map"), map])
            .expect("map should register")
            .as_int()
            .unwrap();
    let program = Value::vector(
        [0, 6, 262_431, 1, map_id, 1345, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![Value::NIL; 8]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("iterate should call and continue");
    let slots = registers.as_vector_data().unwrap();
    assert_eq!(slots[0], Value::fixnum(9));
    assert_eq!(slots[1], Value::fixnum(4));
    assert_eq!(slots[2], Value::fixnum(5));
}

#[test]
fn ccl_execute_lookup_integer_reads_the_lisp_hash_table_vector() {
    crate::test_utils::init_test_tracing();
    // Same registers as `ccl-hash-table` in test/lisp/international/ccl-tests.el:
    // key 17 maps to character 16, r0 becomes the unicode charset id, r7 is 1.
    let mut eval = crate::test_utils::runtime_startup_context();
    let registers = eval
        .eval_str(
            r#"(let ((table (make-hash-table :test 'eq))
                     (reg (vector 17 0 0 0 0 0 0 0)))
                 (puthash 16 17 table)
                 (puthash 17 16 table)
                 (setq translation-hash-table-vector (vector (cons 'th table)))
                 (ccl-execute [0 4 311359 0 22] reg)
                 reg)"#,
        )
        .expect("lookup-integer through the Lisp hash table");
    let registers = registers.as_vector_data().expect("register vector");
    assert_eq!(registers[0], Value::fixnum(2));
    assert_eq!(registers[1], Value::fixnum(16));
    assert_eq!(registers[7], Value::fixnum(1));
}

#[test]
fn ccl_execute_translate_character_rewrites_a_char_table_entry() {
    crate::test_utils::init_test_tracing();
    // GNU `define-translation-table` of (?A . ?B), then
    // (r1 = 0) (r0 = ?A) (translate-character tr r1 r0)
    // leaves (66, 0). ?Z is not in the table and stays 90.
    let mut eval = crate::test_utils::runtime_startup_context();
    let hit = eval
        .eval_str(
            r#"(let* ((tbl (make-char-table nil nil))
                      (reg (vector 65 0 0 0 0 0 0 0)))
                 (aset tbl 65 66)
                 (setq translation-table-vector (vector (cons 'tr tbl)))
                 (ccl-execute [0 6 33 16641 49439 0 22] reg)
                 reg)"#,
        )
        .expect("translate A to B");
    let hit = hit.as_vector_data().expect("register vector");
    assert_eq!(hit[0], Value::fixnum(66));
    assert_eq!(hit[1], Value::fixnum(0));

    let miss = eval
        .eval_str(
            r#"(let ((reg (vector 90 0 0 0 0 0 0 0)))
                 (ccl-execute [0 6 33 23041 49439 0 22] reg)
                 reg)"#,
        )
        .expect("untranslated Z stays Z");
    let miss = miss.as_vector_data().expect("register vector");
    assert_eq!(miss[0], Value::fixnum(90));
    assert_eq!(miss[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_translate_character_keeps_everything_for_a_nil_table() {
    crate::test_utils::init_test_tracing();
    // GNU `translate_char` returns the character untouched when the table is
    // nil: (r0 = ?A) (r1 = 0) (translate-const-table 0) leaves [65, 0].
    // The old code signalled `Error in CCL program` here.
    let mut eval = crate::test_utils::runtime_startup_context();
    let registers = eval
        .eval_str(
            r#"(let ((reg (vector 65 0 0 0 0 0 0 0)))
                 (setq translation-table-vector (vector (cons 'tt nil)))
                 (ccl-execute [0 4 49439 0 22] reg)
                 reg)"#,
        )
        .expect("nil table maps identity");
    let registers = registers.as_vector_data().expect("register vector");
    assert_eq!(registers[0], Value::fixnum(65));
    assert_eq!(registers[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_translate_character_skips_a_mapping_that_is_not_a_character() {
    crate::test_utils::init_test_tracing();
    // GNU guards the lookup result with `CHARACTERP (ch)` in
    // `translate_char`. A -1 entry is not a character, so ?A is untouched.
    let mut eval = crate::test_utils::runtime_startup_context();
    let registers = eval
        .eval_str(
            r#"(let* ((tbl (make-char-table nil nil))
                      (reg (vector 65 0 0 0 0 0 0 0)))
                 (aset tbl 65 -1)
                 (setq translation-table-vector (vector (cons 'tt tbl)))
                 (ccl-execute [0 4 49439 0 22] reg)
                 reg)"#,
        )
        .expect("non-character mapping keeps the input");
    let registers = registers.as_vector_data().expect("register vector");
    assert_eq!(registers[0], Value::fixnum(65));
    assert_eq!(registers[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_translate_character_walks_a_list_of_tables() {
    crate::test_utils::init_test_tracing();
    // GNU `translate_char` recurses through a cons list of char tables
    // (`for (; CONSP (table); table = XCDR (table))`).
    let mut eval = crate::test_utils::runtime_startup_context();
    let registers = eval
        .eval_str(
            r#"(let* ((tbl (make-char-table nil nil))
                      (reg (vector 65 0 0 0 0 0 0 0)))
                 (aset tbl 65 66)
                 (setq translation-table-vector (vector (cons 'tt (list tbl))))
                 (ccl-execute [0 4 49439 0 22] reg)
                 reg)"#,
        )
        .expect("list table translates recursively");
    let registers = registers.as_vector_data().expect("register vector");
    assert_eq!(registers[0], Value::fixnum(66));
    assert_eq!(registers[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_map_single_rejects_a_missing_called_program() {
    crate::test_utils::init_test_tracing();
    // GNU `CCL_CALL_FOR_MAP_INSTRUCTION` signals an invalid command when
    // `setup_ccl_program` fails on the map entry's symbol. It does not fall
    // back to a miss. The instruction counter is past both words.
    let mut eval = crate::test_utils::runtime_startup_context();
    let err = eval
        .eval_str(
            r#"(let ((reg (vector 0 0 0 0 0 0 0 0)))
                 (register-code-conversion-map 'm1 (vector 0 'nope-program))
                 (ccl-execute [0 4 294943 0 22] reg))"#,
        )
        .expect_err("map-single with a missing program is an invalid command");
    assert_ccl_error_at_index(err, 4);
}

#[test]
fn ccl_execute_iterate_multiple_map_rejects_a_missing_called_program() {
    crate::test_utils::init_test_tracing();
    // GNU consumes the map-id word before the call, so the error names the
    // 5th code. The Rust code treated the failure as a miss and moved on.
    let mut eval = crate::test_utils::runtime_startup_context();
    let err = eval
        .eval_str(
            r#"(let ((reg (vector 0 0 0 0 0 0 0 0)))
                 (register-code-conversion-map 'm1 (vector 0 'nope-program))
                 (ccl-execute [0 4 262431 1 0 22] reg))"#,
        )
        .expect_err("iterate-multiple-map with a missing program is an invalid command");
    assert_ccl_error_at_index(err, 5);
}

#[test]
fn ccl_execute_map_multiple_rejects_a_missing_called_program() {
    crate::test_utils::init_test_tracing();
    // GNU reports the 4th code: the point word is not consumed before the
    // symbol call. The old code pushed map stack entries and kept going.
    let mut eval = crate::test_utils::runtime_startup_context();
    let err = eval
        .eval_str(
            r#"(let ((reg (vector 0 0 0 0 0 0 0 0)))
                 (register-code-conversion-map 'm1 (vector 0 'nope-program))
                 (ccl-execute [0 4 278815 1 0 22] reg))"#,
        )
        .expect_err("map-multiple with a missing program is an invalid command");
    assert_ccl_error_at_index(err, 4);
}

#[test]
fn ccl_execute_lookup_rejects_a_table_id_outside_the_hash_vector() {
    crate::test_utils::init_test_tracing();
    // GNU bounds the table id with `GET_CCL_RANGE` against
    // `ASIZE (Vtranslation_hash_table_vector)`. A nil vector bounds to -1, so
    // every id is out of range; an id beyond the vector likewise. Emacs 31.1
    // reads one past the end for id == ASIZE (UB), which we reject.
    let mut eval = crate::test_utils::runtime_startup_context();
    let err = eval
        .eval_str(
            r#"(let ((reg (vector 1 0 0 0 0 0 0 0)))
                 (ccl-execute [0 4 311359 0 22] reg))"#,
        )
        .expect_err("a nil translation-hash-table-vector invalidates lookup-integer");
    assert_lookup_invalid(err);

    let err = eval
        .eval_str(
            r#"(let ((reg (vector 1 0 0 0 0 0 0 0)))
                 (setq translation-hash-table-vector (vector (cons 'h (make-hash-table :test 'eq))))
                 (ccl-execute [0 4 311359 1 22] reg))"#,
        )
        .expect_err("an id beyond the hash vector invalidates lookup-integer");
    assert_lookup_invalid(err);

    let err = eval
        .eval_str(
            r#"(let ((reg (vector 65 0 0 0 0 0 0 0)))
                 (setq translation-hash-table-vector (vector (cons 'h (make-hash-table :test 'eq))))
                 (ccl-execute [0 4 327967 -1 22] reg))"#,
        )
        .expect_err("a negative id invalidates lookup-character");
    assert_lookup_invalid(err);
}

#[test]
fn ccl_execute_lookup_character_stores_an_int_including_negative() {
    crate::test_utils::init_test_tracing();
    // GNU: (lookup-character TABLE r1 r0) decodes charset r1 and code r0.
    // A hit writes the integer into r1 and sets r7. -1 is a valid C int.
    // A missing key clears r7 and leaves the code register alone.
    // Opcode 327967 is that instruction.
    let mut entries = std::collections::HashMap::new();
    entries.insert(65, 99);
    entries.insert(66, -1);
    let id = super::install_translation_hash(entries);

    let hit = lookup_character_registers(id, 65, 0);
    let hit = hit.as_vector_data().unwrap();
    assert_eq!(hit[0], Value::fixnum(65));
    assert_eq!(hit[1], Value::fixnum(99));
    assert_eq!(hit[7], Value::fixnum(1));

    let negative = lookup_character_registers(id, 66, 0);
    let negative = negative.as_vector_data().unwrap();
    assert_eq!(negative[0], Value::fixnum(66));
    assert_eq!(negative[1], Value::fixnum(-1));
    assert_eq!(negative[7], Value::fixnum(1));

    let miss = lookup_character_registers(id, 69, 5);
    let miss = miss.as_vector_data().unwrap();
    assert_eq!(miss[0], Value::fixnum(69));
    assert_eq!(miss[1], Value::fixnum(0));
    assert_eq!(miss[7], Value::fixnum(0));
}

#[test]
fn ccl_execute_lookup_character_rejects_a_non_integer_hash_value() {
    crate::test_utils::init_test_tracing();
    // GNU Emacs: hash key 67 maps to the symbol not-int. lookup-character
    // requires a fixnum in the C int range. The table id has already been
    // read, so the error is "at 4th code". lookup-integer of a symbol is
    // the same invalid command, not a miss.
    let mut eval = crate::test_utils::runtime_startup_context();
    let character = eval
        .eval_str(
            r#"(let ((table (make-hash-table :test 'eq)))
                 (puthash 67 'not-int table)
                 (setq translation-hash-table-vector (vector (cons 'th table)))
                 (ccl-execute [0 4 327967 0 22] (vector 67 0 0 0 0 0 0 0)))"#,
        )
        .expect_err("a non-integer hash value is an invalid CCL command");
    assert_ccl_error_at_fourth(character);

    let integer = eval
        .eval_str(
            r#"(let ((table (make-hash-table :test 'eq)))
                 (puthash 1 'not-int table)
                 (setq translation-hash-table-vector (vector (cons 'th table)))
                 (ccl-execute [0 4 311359 0 22] (vector 1 0 0 0 0 0 0 0)))"#,
        )
        .expect_err("lookup-integer of a symbol is an invalid CCL command");
    assert_ccl_error_at_fourth(integer);
}

#[test]
fn ccl_execute_map_single_treats_an_out_of_int_range_value_as_a_miss() {
    crate::test_utils::init_test_tracing();
    // GNU map-single content guard: content that is not a C int falls
    // through to the final `reg[RRR] = -1`. The value register keeps its
    // original store because the failure path writes only the status.
    let mut eval = crate::test_utils::runtime_startup_context();
    let registers = eval
        .eval_str(
            r#"(let ((reg (vector 0 0 0 0 0 0 0 0)))
                 (register-code-conversion-map 'm2 (vector 0 4294967296))
                 (ccl-execute [0 4 294943 0 22] reg)
                 reg)"#,
        )
        .expect("out-of-int-range map content is a miss, not an error");
    let regs = registers.as_vector_data().expect("register vector");
    assert_eq!(regs[0], Value::fixnum(-1));
}

#[test]
fn ccl_execute_iterate_rejects_an_out_of_int_range_value() {
    crate::test_utils::init_test_tracing();
    // GNU consumed the map-id word (counter at 5) and signals invalid.
    let mut eval = crate::test_utils::runtime_startup_context();
    let err = eval
        .eval_str(
            r#"(let ((reg (vector 0 0 0 0 0 0 0 0)))
                 (register-code-conversion-map 'm2 (vector 0 4294967296))
                 (ccl-execute [0 4 262431 1 0 22] reg))"#,
        )
        .expect_err("iterate with an out-of-int-range content is invalid");
    assert_ccl_error_at_index(err, 5);
}

#[test]
fn ccl_execute_map_multiple_rejects_an_out_of_int_range_value() {
    crate::test_utils::init_test_tracing();
    // The point word is not consumed at map-multiple: error at 4th code.
    let mut eval = crate::test_utils::runtime_startup_context();
    let err = eval
        .eval_str(
            r#"(let ((reg (vector 0 0 0 0 0 0 0 0)))
                 (register-code-conversion-map 'm2 (vector 0 4294967296))
                 (ccl-execute [0 4 278815 1 0 22] reg))"#,
        )
        .expect_err("map-multiple with an out-of-int-range content is invalid");
    assert_ccl_error_at_index(err, 4);
}

fn assert_ccl_error_at_index(err: neovm_value::error::EvalError, nth: usize) {
    match err {
        neovm_value::error::EvalError::Signal { data, .. } => {
            assert_eq!(
                data[0],
                Value::string(format!("Error in CCL program at {nth}th code"))
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

fn assert_lookup_invalid(err: neovm_value::error::EvalError) {
    assert_ccl_error_at_index(err, 4);
}

fn assert_ccl_error_at_fourth(err: neovm_value::error::EvalError) {
    assert_ccl_error_at_index(err, 4);
}

fn lookup_character_registers(id: i64, code: i64, r7: i64) -> Value {
    let program = Value::vector(
        [0, 4, 327_967, id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![
        Value::fixnum(code),
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::fixnum(r7),
    ]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("lookup-character should run");
    registers
}
