use super::*;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::ValueKind;

#[test]
fn ccl_programp_validates_shape_and_type() {
    crate::test_utils::init_test_tracing();
    let program = Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]);
    let invalid_program = Value::vector(vec![Value::fixnum(0), Value::fixnum(0)]);
    let invalid_negative =
        Value::vector(vec![Value::fixnum(-1), Value::fixnum(0), Value::fixnum(0)]);
    let invalid_header_mode =
        Value::vector(vec![Value::fixnum(10), Value::fixnum(4), Value::fixnum(0)]);
    let valid_real_eof = Value::vector(vec![
        Value::fixnum(10),
        Value::fixnum(4),
        Value::fixnum(0),
        Value::fixnum(0),
    ]);
    assert_eq!(
        builtin_ccl_program_p_impl(vec![program]).expect("valid program"),
        Value::T
    );
    assert_eq!(
        builtin_ccl_program_p_impl(vec![invalid_program]).expect("invalid program"),
        Value::NIL
    );
    assert_eq!(
        builtin_ccl_program_p_impl(vec![invalid_negative]).expect("invalid program"),
        Value::NIL
    );
    assert_eq!(
        builtin_ccl_program_p_impl(vec![invalid_header_mode]).expect("invalid program"),
        Value::NIL
    );
    assert_eq!(
        builtin_ccl_program_p_impl(vec![valid_real_eof]).expect("valid GNU CCL EOF index"),
        Value::T
    );
}

#[test]
fn ccl_programp_accepts_registered_symbol_designator() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_ccl_program_p_impl(vec![Value::symbol("ccl-program-p-unregistered")])
            .expect("unregistered symbol should be nil"),
        Value::NIL
    );
    let _ = builtin_register_ccl_program_impl(vec![
        Value::symbol("ccl-program-p-registered"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("registration should succeed");
    assert_eq!(
        builtin_ccl_program_p_impl(vec![Value::symbol("ccl-program-p-registered")])
            .expect("registered symbol should be accepted"),
        Value::T
    );
}

#[test]
fn ccl_execute_requires_registers_vector_length_eight() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ccl_execute_impl(vec![
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::vector(vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect_err("registers length should be checked");
    match err {
        Flow::Signal(sig) => assert_eq!(
            sig.data[0],
            Value::string("Length of vector REGISTERS is not 8")
        ),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn ccl_execute_reports_invalid_program_before_success() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ccl_execute_impl(vec![
        Value::fixnum(1),
        Value::vector(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
    ])
    .expect_err("non-vector program must be rejected");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.data[0], Value::string("Invalid CCL program")),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn ccl_execute_on_string_requires_status_vector_length_nine() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ccl_execute_on_string_impl(vec![
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::vector(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
        Value::string("abc"),
    ])
    .expect_err("status length should be checked");
    match err {
        Flow::Signal(sig) => assert_eq!(
            sig.data[0],
            Value::string("Length of vector STATUS is not 9")
        ),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn ccl_execute_on_string_rejects_non_vector_status() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ccl_execute_on_string_impl(vec![
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::fixnum(1),
        Value::string("abc"),
    ])
    .expect_err("status must be a vector");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn ccl_execute_on_string_rejects_non_string_payload() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ccl_execute_on_string_impl(vec![
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::vector(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
        Value::fixnum(1),
    ])
    .expect_err("non-string payload must be rejected");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn ccl_execute_on_string_rejects_over_arity() {
    crate::test_utils::init_test_tracing();
    let err = builtin_ccl_execute_on_string_impl(vec![
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::vector(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
        Value::string("abc"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ])
    .expect_err("over-arity should signal");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn register_ccl_program_requires_symbol_name() {
    crate::test_utils::init_test_tracing();
    let err = builtin_register_ccl_program_impl(vec![
        Value::fixnum(1),
        Value::vector(vec![Value::fixnum(10)]),
    ])
    .expect_err("register-ccl-program name must be symbol");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn register_ccl_program_requires_vector_when_program_non_nil() {
    crate::test_utils::init_test_tracing();
    let err = builtin_register_ccl_program_impl(vec![Value::symbol("foo"), Value::fixnum(1)])
        .expect_err("register-ccl-program program must be vector when non-nil");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("vectorp"));
            assert_eq!(sig.data[1], Value::fixnum(1));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn register_ccl_program_accepts_nil_program() {
    crate::test_utils::init_test_tracing();
    let result = builtin_register_ccl_program_impl(vec![Value::symbol("foo-nil"), Value::NIL])
        .expect("register-ccl-program should accept nil");
    match result.kind() {
        ValueKind::Fixnum(id) => assert!(id > 0),
        other => panic!("expected integer id, got {other:?}"),
    }
    let programp = builtin_ccl_program_p_impl(vec![Value::symbol("foo-nil")])
        .expect("registered nil program should resolve as valid");
    assert_eq!(programp, Value::T);
}

#[test]
fn register_ccl_program_rejects_invalid_program_shape() {
    crate::test_utils::init_test_tracing();
    let err = builtin_register_ccl_program_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(1)]),
    ])
    .expect_err("invalid program must be rejected");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.data[0], Value::string("Error in CCL program"));
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn register_ccl_program_accepts_eof_header_within_vector_length() {
    crate::test_utils::init_test_tracing();
    let result = builtin_register_ccl_program_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![
            Value::fixnum(10),
            Value::fixnum(4),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
    ])
    .expect("EOF instruction counter may point to vector length");
    assert!(result.as_int().is_some_and(|id| id > 0));
}

#[test]
fn register_ccl_program_returns_success_code() {
    crate::test_utils::init_test_tracing();
    let first = builtin_register_ccl_program_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("valid registration should succeed");
    let second = builtin_register_ccl_program_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("repeat registration should keep id");
    assert_eq!(first, second);
    match first.kind() {
        ValueKind::Fixnum(id) => assert!(id > 0),
        other => panic!("expected integer id, got {other:?}"),
    }
}

#[test]
fn register_ccl_program_keeps_symbol_identity_in_registry() {
    crate::test_utils::init_test_tracing();
    let symbol = intern("ccl-symbol-registry-live-key");
    let program = Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]);
    builtin_register_ccl_program_impl(vec![Value::from_sym_id(symbol), program])
        .expect("registration should succeed");
    with_ccl_registry(|registry| {
        assert!(registry.programs.contains_key(&symbol));
        assert_eq!(registry.lookup_program(symbol), Some(program));
    });
}

#[test]
fn register_code_conversion_map_requires_symbol_name() {
    crate::test_utils::init_test_tracing();
    let err = builtin_register_code_conversion_map_impl(vec![
        Value::fixnum(1),
        Value::vector(vec![Value::fixnum(0)]),
    ])
    .expect_err("register-code-conversion-map name must be symbol");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn register_code_conversion_map_requires_vector_map() {
    crate::test_utils::init_test_tracing();
    let err =
        builtin_register_code_conversion_map_impl(vec![Value::symbol("foo"), Value::fixnum(1)])
            .expect_err("register-code-conversion-map map must be vector");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("vectorp"));
            assert_eq!(sig.data[1], Value::fixnum(1));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn register_code_conversion_map_returns_success_code() {
    crate::test_utils::init_test_tracing();
    let first = builtin_register_code_conversion_map_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("valid registration should succeed");
    let second = builtin_register_code_conversion_map_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
    ])
    .expect("repeat registration should keep id");
    assert_eq!(first, second);
    match first.kind() {
        ValueKind::Fixnum(id) => assert!(id >= 0),
        other => panic!("expected integer id, got {other:?}"),
    }
}

#[test]
fn register_code_conversion_map_keeps_symbol_identity_in_registry() {
    crate::test_utils::init_test_tracing();
    let symbol = intern("ccl-map-symbol-registry-live-key");
    let map = Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]);
    builtin_register_code_conversion_map_impl(vec![Value::from_sym_id(symbol), map])
        .expect("registration should succeed");
    with_ccl_registry(|registry| {
        assert!(registry.code_conversion_maps.contains_key(&symbol));
    });
}

#[test]
fn register_ccl_program_assigns_new_ids_for_new_symbols() {
    crate::test_utils::init_test_tracing();
    let a = builtin_register_ccl_program_impl(vec![
        Value::symbol("ccl-id-a"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("registration a should succeed");
    let b = builtin_register_ccl_program_impl(vec![
        Value::symbol("ccl-id-b"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("registration b should succeed");
    match (a.kind(), b.kind()) {
        (ValueKind::Fixnum(aid), ValueKind::Fixnum(bid)) => assert!(bid > aid),
        other => panic!("expected integer ids, got {other:?}"),
    }
}

#[test]
fn register_code_conversion_map_assigns_new_ids_for_new_symbols() {
    crate::test_utils::init_test_tracing();
    let a = builtin_register_code_conversion_map_impl(vec![
        Value::symbol("ccl-map-id-a"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("registration a should succeed");
    let b = builtin_register_code_conversion_map_impl(vec![
        Value::symbol("ccl-map-id-b"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
    ])
    .expect("registration b should succeed");
    match (a.kind(), b.kind()) {
        (ValueKind::Fixnum(aid), ValueKind::Fixnum(bid)) => assert!(bid > aid),
        other => panic!("expected integer ids, got {other:?}"),
    }
}

#[test]
fn ccl_execute_accepts_registered_symbol_program_designator() {
    crate::test_utils::init_test_tracing();
    let _ = builtin_register_ccl_program_impl(vec![
        Value::symbol("ccl-designator-probe"),
        Value::vector(vec![
            Value::fixnum(10),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
    ])
    .expect("registration should succeed");
    let err = builtin_ccl_execute_impl(vec![
        Value::symbol("ccl-designator-probe"),
        Value::vector(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
        ]),
    ])
    .expect_err("symbol designator should resolve to registered program");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(
                sig.data[0],
                Value::string("Error in CCL program at 5th code")
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn ccl_execute_on_string_accepts_registered_symbol_program_designator() {
    crate::test_utils::init_test_tracing();
    let _ = builtin_register_ccl_program_impl(vec![
        Value::symbol("ccl-designator-probe-on-string"),
        Value::vector(vec![
            Value::fixnum(1),
            Value::fixnum(5),
            Value::fixnum(14),
            Value::fixnum(-249),
            Value::fixnum(-500),
            Value::fixnum(22),
        ]),
    ])
    .expect("registration should succeed");
    let status = Value::vector(vec![Value::NIL; 9]);
    let output = builtin_ccl_execute_on_string_impl(vec![
        Value::symbol("ccl-designator-probe-on-string"),
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
            0, 1, 65, 127, 128, 255,
        ])),
        Value::NIL,
        Value::T,
    ])
    .expect("registered identity program should execute");

    assert_eq!(
        output.as_lisp_string().unwrap().as_bytes(),
        &[0, 1, 65, 127, 128, 255]
    );
    assert!(!output.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(
        status.as_vector_data().unwrap().as_slice(),
        &[
            Value::fixnum(-1),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(5),
        ]
    );
}

#[test]
fn ccl_execute_on_string_resumes_identity_program_from_status_instruction() {
    crate::test_utils::init_test_tracing();
    let program = Value::vector(vec![
        Value::fixnum(1),
        Value::fixnum(5),
        Value::fixnum(14),
        Value::fixnum(-249),
        Value::fixnum(-500),
        Value::fixnum(22),
    ]);
    let status = Value::vector(vec![Value::NIL; 9]);

    let first = builtin_ccl_execute_on_string_impl(vec![
        program,
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![65, 128])),
        Value::T,
        Value::T,
    ])
    .expect("continued execution should suspend at the read instruction");
    assert_eq!(first.as_lisp_string().unwrap().as_bytes(), &[65, 128]);
    assert_eq!(status.as_vector_data().unwrap()[0], Value::fixnum(128));
    assert_eq!(status.as_vector_data().unwrap()[8], Value::fixnum(4));

    let second = builtin_ccl_execute_on_string_impl(vec![
        program,
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![66, 255])),
        Value::NIL,
        Value::T,
    ])
    .expect("final execution should run the EOF block");
    assert_eq!(second.as_lisp_string().unwrap().as_bytes(), &[66, 255]);
    assert_eq!(status.as_vector_data().unwrap()[0], Value::fixnum(-1));
    assert_eq!(status.as_vector_data().unwrap()[8], Value::fixnum(5));
}

fn execute_ccl_on_string(
    words: &[i64],
    registers: [i64; 8],
    input: &[u8],
    last_block: bool,
) -> (Vec<u8>, Vec<Value>) {
    let program = Value::vector(words.iter().copied().map(Value::fixnum).collect());
    let mut status_slots = registers.map(Value::fixnum).to_vec();
    status_slots.push(Value::NIL);
    let status = Value::vector(status_slots);
    let output = builtin_ccl_execute_on_string_impl(vec![
        program,
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(input.to_vec())),
        Value::bool_val(!last_block),
        Value::T,
    ])
    .expect("CCL program should execute");
    let bytes = output.as_lisp_string().unwrap().as_bytes().to_vec();
    assert!(!output.as_lisp_string().unwrap().is_multibyte());
    let status = status.as_vector_data().unwrap().to_vec();
    (bytes, status)
}

#[test]
fn ccl_execute_on_string_runs_branch_to_the_selected_block() {
    crate::test_utils::init_test_tracing();
    // GNU Emacs `ccl-compile` of (1 ((branch r0 (write "A")))), then
    // `ccl-execute-on-string` with a zeroed status vector and an empty input.
    // r0 is 0, so the jump table selects the block that writes "A" and leaves
    // the instruction counter on the trailing End word.
    let program = Value::vector(
        [1, 7, 269, 2, 4, 308, 4_259_840, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let status = Value::vector(vec![Value::NIL; 9]);
    let output = builtin_ccl_execute_on_string_impl(vec![
        program,
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(Vec::new())),
        Value::NIL,
        Value::T,
    ])
    .expect("branch on r0 selects the write block");
    assert_eq!(output.as_lisp_string().unwrap().as_bytes(), b"A");
    assert!(!output.as_lisp_string().unwrap().is_multibyte());
    assert_eq!(
        status.as_vector_data().unwrap().as_slice(),
        &[
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(7),
        ]
    );
}

#[test]
fn ccl_execute_on_string_branch_uses_the_out_of_range_slot() {
    crate::test_utils::init_test_tracing();
    // Same GNU program as the r0 == 0 case. Register 1 and -1 both take the
    // extra jump-table slot, which lands on End and writes nothing.
    let program = [1, 7, 269, 2, 4, 308, 4_259_840, 22];
    for selector in [1, -1] {
        let mut registers = [0; 8];
        registers[0] = selector;
        let (output, status) = execute_ccl_on_string(&program, registers, b"", true);
        assert_eq!(output, b"");
        assert_eq!(status[0], Value::fixnum(selector));
        assert_eq!(status[8], Value::fixnum(7));
    }
}

#[test]
fn ccl_execute_on_string_branch_selects_a_later_register_block() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of (1 ((branch r1 (write "A") (write "B")))) with r1 = 1.
    let mut registers = [0; 8];
    registers[1] = 1;
    let (output, status) = execute_ccl_on_string(
        &[1, 11, 557, 3, 6, 8, 308, 4_259_840, 516, 308, 4_325_376, 22],
        registers,
        b"",
        true,
    );
    assert_eq!(output, b"B");
    assert_eq!(status[1], Value::fixnum(1));
    assert_eq!(status[8], Value::fixnum(11));
}

#[test]
fn ccl_execute_on_string_read_branch_selects_from_the_input_byte() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of (1 ((read-branch r0 (write "A") (write "B")))).
    // Byte 0 selects "A", byte 1 selects "B", byte 2 takes the out-of-range
    // slot. An empty final block stores EOF in r0 and skips the table. An
    // empty non-final block suspends on the ReadBranch word itself.
    let program = [1, 11, 528, 3, 6, 8, 308, 4_259_840, 516, 308, 4_325_376, 22];
    let (zero, status) = execute_ccl_on_string(&program, [0; 8], &[0], true);
    assert_eq!(zero, b"A");
    assert_eq!(status[0], Value::fixnum(0));
    assert_eq!(status[8], Value::fixnum(11));

    let (one, status) = execute_ccl_on_string(&program, [0; 8], &[1], true);
    assert_eq!(one, b"B");
    assert_eq!(status[0], Value::fixnum(1));
    assert_eq!(status[8], Value::fixnum(11));

    let (two, status) = execute_ccl_on_string(&program, [0; 8], &[2], true);
    assert_eq!(two, b"");
    assert_eq!(status[0], Value::fixnum(2));
    assert_eq!(status[8], Value::fixnum(11));

    let (eof, status) = execute_ccl_on_string(&program, [0; 8], b"", true);
    assert_eq!(eof, b"");
    assert_eq!(status[0], Value::fixnum(-1));
    assert_eq!(status[8], Value::fixnum(11));

    let (suspended, status) = execute_ccl_on_string(&program, [0; 8], b"", false);
    assert_eq!(suspended, b"");
    assert_eq!(status[0], Value::fixnum(0));
    assert_eq!(status[8], Value::fixnum(2));
}

#[test]
fn ccl_execute_runs_assignment_and_comparison() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` / `ccl-execute` of
    // (r0 = 7) (r1 = (r0 + 1)) (r2 = (r0 << 1)) (if (r1 < 9) (r3 = 1) (r3 = 2))
    let program = Value::vector(
        [
            1, 13, 1793, 57, 1, 131161, 1, 1083, 16, 9, 353, 260, 609, 22,
        ]
        .into_iter()
        .map(Value::fixnum)
        .collect(),
    );
    let registers = Value::vector(vec![
        Value::fixnum(3),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("arithmetic program should run");
    assert_eq!(
        registers.as_vector_data().unwrap().as_slice(),
        &[
            Value::fixnum(7),
            Value::fixnum(8),
            Value::fixnum(14),
            Value::fixnum(1),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(1),
        ]
    );
}

#[test]
fn ccl_execute_on_string_resumes_a_multiregister_read() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of (1 ((read r0 r1) (write r0) (write r1))).
    // One input byte suspends on the second read operand. The next call
    // reads that byte into r1 and writes both registers.
    let program = [1, 6, 270, 46, 17, 49, 22];
    let (output, status) = execute_ccl_on_string(&program, [0; 8], &[65], false);
    assert_eq!(output, b"");
    assert_eq!(status[0], Value::fixnum(65));
    assert_eq!(status[1], Value::fixnum(0));
    assert_eq!(status[8], Value::fixnum(3));

    let mut registers = [0; 8];
    registers[0] = 65;
    let program_value = Value::vector(program.into_iter().map(Value::fixnum).collect());
    let mut slots = registers.map(Value::fixnum).to_vec();
    slots.push(Value::fixnum(3));
    let status = Value::vector(slots);
    let output = builtin_ccl_execute_on_string_impl(vec![
        program_value,
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![66])),
        Value::NIL,
        Value::T,
    ])
    .expect("the second read should resume at r1");
    assert_eq!(output.as_lisp_string().unwrap().as_bytes(), b"AB");
    assert_eq!(status.as_vector_data().unwrap()[1], Value::fixnum(66));
    assert_eq!(status.as_vector_data().unwrap()[8], Value::fixnum(6));
}

#[test]
fn ccl_execute_on_string_writes_a_multibyte_constant_character() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of (1 ((write "あ"))). The data word has the
    // multibyte flag set and U+3042 in the low 24 bits.
    let program = Value::vector(
        [1, 4, 308, 16_789_570, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let status = Value::vector(vec![Value::NIL; 9]);
    let output = builtin_ccl_execute_on_string_impl(vec![
        program,
        status,
        Value::heap_string(crate::heap_types::LispString::from_unibyte(Vec::new())),
    ])
    .expect("a multibyte constant character should be written");
    let string = output.as_lisp_string().unwrap();
    assert!(string.is_multibyte());
    let (character, length) = crate::emacs_core::emacs_char::string_char(string.as_bytes());
    assert_eq!(character, 0x3042);
    assert_eq!(length, string.as_bytes().len());
}

#[test]
fn ccl_execute_on_string_decodes_midi_running_status() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of `midikbd-decoder` from midi-kbd-0.2, the vector
    // checked in as `prog-midi-code` in test/lisp/international/ccl-tests.el.
    // Note-on writes 0, channel, note, velocity. Velocity 0 becomes note-off
    // (leading 1). A second note without a status byte uses running status.
    let program = [
        2, 72, 4893, 16, 128, 1133, 5, 6, 9, 12, 16, -2556, 32, 1024, 6660, 32, 865, -4092, 64,
        609, 1024, 4868, 795, 20, 248, 3844, 3099, 16, 240, 128, 82169, 224, 1275, 18, 192, 353,
        260, 609, -9468, 97, -9980, 82169, 240, 4091, 18, 144, 1371, 18, 0, 16407, 16, 1796, 81943,
        15, 20, 529, 305, 81, -14588, 82169, 240, 2555, 18, 128, 81943, 15, 276, 529, 305, 81,
        -17660, -17916, 22,
    ];
    for (input, expected) in [
        (&[144, 60, 100][..], &[0, 0, 60, 100][..]),
        (&[128, 60, 0][..], &[1, 0, 60, 0][..]),
        (
            &[144, 60, 100, 62, 80][..],
            &[0, 0, 60, 100, 0, 0, 62, 80][..],
        ),
        (&[144, 60, 0][..], &[1, 0, 60, 0][..]),
    ] {
        let (output, _) = execute_ccl_on_string(&program, [0; 8], input, true);
        assert_eq!(output, expected);
    }
}

#[test]
fn ccl_execute_set_array_reads_the_indexed_element() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of (1 ((r0 = r1 [65 66 67]))) with r1 = 1.
    let program = Value::vector(
        [1, 6, 6403, 65, 66, 67, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![
        Value::fixnum(0),
        Value::fixnum(1),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("set-array should run");
    assert_eq!(registers.as_vector_data().unwrap()[0], Value::fixnum(66));
    assert_eq!(registers.as_vector_data().unwrap()[1], Value::fixnum(1));
}

#[test]
fn ccl_execute_on_string_write_array_uses_the_register_or_skips() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` of (1 ((write r0 [65 66 67]))).
    let program = [1, 6, 789, 65, 66, 67, 22];
    let mut selected = [0; 8];
    selected[0] = 2;
    let (output, _) = execute_ccl_on_string(&program, selected, b"", true);
    assert_eq!(output, &[67]);
    let mut out_of_range = [0; 8];
    out_of_range[0] = 9;
    let (output, _) = execute_ccl_on_string(&program, out_of_range, b"", true);
    assert_eq!(output, b"");
}

#[test]
fn ccl_execute_on_string_write_const_read_jump_writes_then_reads() {
    crate::test_utils::init_test_tracing();
    // Hand-built `CCL_WriteConstReadJump` checked on GNU Emacs: write 65,
    // read the next input byte into r0, then land on End.
    let (output, status) = execute_ccl_on_string(&[1, 5, 521, 65, 12, 22], [0; 8], b"ab", true);
    assert_eq!(output, &[65]);
    assert_eq!(status[0], Value::fixnum(97));
    assert_eq!(status[8], Value::fixnum(5));
}

#[test]
fn ccl_execute_on_string_write_array_read_jump_writes_the_indexed_element() {
    crate::test_utils::init_test_tracing();
    // Hand-built `CCL_WriteArrayReadJump` checked on GNU Emacs. r0 = 1
    // selects 66, then the next input byte is read into r0.
    let mut registers = [0; 8];
    registers[0] = 1;
    let (output, status) =
        execute_ccl_on_string(&[1, 8, 1291, 3, 65, 66, 67, 12, 22], registers, b"ab", true);
    assert_eq!(output, &[66]);
    assert_eq!(status[0], Value::fixnum(97));
    assert_eq!(status[8], Value::fixnum(8));
}

#[test]
fn ccl_execute_on_string_write_string_jump_writes_the_embedded_text() {
    crate::test_utils::init_test_tracing();
    // One `CCL_WriteStringJump` of "A" whose relative address lands on End.
    let (output, status) = execute_ccl_on_string(&[1, 5, 522, 1, 4_259_840, 22], [0; 8], b"", true);
    assert_eq!(output, b"A");
    assert_eq!(status[8], Value::fixnum(5));
}

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
fn ccl_execute_call_resolves_an_embedded_program_symbol() {
    crate::test_utils::init_test_tracing();
    let callee = Value::vector([0, 3, 1793, 22].into_iter().map(Value::fixnum).collect());
    builtin_register_ccl_program_impl(vec![Value::symbol("ccl-call-named"), callee])
        .expect("named callee should register");
    let caller = Value::vector(vec![
        Value::fixnum(0),
        Value::fixnum(4),
        Value::fixnum(51),
        Value::cons(
            Value::symbol("ccl-call-named"),
            Value::symbol("ccl-program-idx"),
        ),
        Value::fixnum(22),
    ]);
    let registers = Value::vector(vec![Value::NIL; 8]);
    builtin_ccl_execute_impl(vec![caller, registers]).expect("symbol call should resolve");
    assert_eq!(registers.as_vector_data().unwrap()[0], Value::fixnum(7));
}

#[test]
fn ccl_execute_map_single_reads_the_code_conversion_map() {
    crate::test_utils::init_test_tracing();
    let map = Value::vector([0, 10, 20, 30].into_iter().map(Value::fixnum).collect());
    let id = builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-map-single"), map])
        .expect("map should register")
        .as_int()
        .unwrap();
    // `map-single` with the value in r0 and the status in r1.
    let program = Value::vector(
        [0, 4, 295_199, id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![
        Value::fixnum(2),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("map-single should run");
    assert_eq!(registers.as_vector_data().unwrap()[0], Value::fixnum(30));
    assert_eq!(registers.as_vector_data().unwrap()[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_map_multiple_restores_the_value_when_the_called_program_returns_minus_one() {
    crate::test_utils::init_test_tracing();
    // GNU: a map element that is a CCL program is called. If that program
    // leaves the value register at -1, map-multiple treats it as nil and
    // restores the value from before the call. r1 becomes -1.
    let callee = Value::vector([0, 3, -255, 22].into_iter().map(Value::fixnum).collect());
    builtin_register_ccl_program_impl(vec![Value::symbol("ccl-map-nil"), callee])
        .expect("mapper should register");
    let map = Value::vector(vec![Value::fixnum(0), Value::symbol("ccl-map-nil")]);
    let map_id =
        builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-map-nil-table"), map])
            .expect("map should register")
            .as_int()
            .unwrap();
    let program = Value::vector(
        [0, 5, 278_815, 1, map_id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![
        Value::fixnum(4),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("map-multiple should resume");
    let slots = registers.as_vector_data().unwrap();
    assert_eq!(slots[0], Value::fixnum(4));
    assert_eq!(slots[1], Value::fixnum(-1));
}

#[test]
fn ccl_execute_map_multiple_keeps_a_normal_call_result_and_skips_the_rest() {
    crate::test_utils::init_test_tracing();
    // GNU skips the maps after a called program that returns an ordinary
    // value. The following map would turn 0 into 3, but it does not run.
    let callee = Value::vector([0, 3, 1_793, 22].into_iter().map(Value::fixnum).collect());
    builtin_register_ccl_program_impl(vec![Value::symbol("ccl-map-seven"), callee])
        .expect("mapper should register");
    let calling = Value::vector(vec![Value::fixnum(0), Value::symbol("ccl-map-seven")]);
    let calling_id =
        builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-map-call"), calling])
            .expect("calling map should register")
            .as_int()
            .unwrap();
    let after = Value::vector([0, 3].into_iter().map(Value::fixnum).collect());
    let after_id =
        builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-map-after"), after])
            .expect("following map should register")
            .as_int()
            .unwrap();
    let program = Value::vector(
        [0, 6, 278_815, 2, calling_id, after_id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![Value::NIL; 8]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("map-multiple should resume");
    let slots = registers.as_vector_data().unwrap();
    assert_eq!(slots[0], Value::fixnum(7));
    assert_eq!(slots[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_map_multiple_chains_nested_separator_sets() {
    crate::test_utils::init_test_tracing();
    // GNU `(map-multiple r1 r0 ((ma) (mb)))` with ma `[0 10]` and mb
    // `[10 20]`. 0 maps to 10, then 10 maps to 20. Status register is 3.
    let ma = Value::vector([0, 10].into_iter().map(Value::fixnum).collect());
    let mb = Value::vector([10, 20].into_iter().map(Value::fixnum).collect());
    let ma_id = builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-nest-ma"), ma])
        .expect("ma")
        .as_int()
        .unwrap();
    let mb_id = builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-nest-mb"), mb])
        .expect("mb")
        .as_int()
        .unwrap();
    let program = Value::vector(
        [0, 8, 278_815, 4, -1, ma_id, -1, mb_id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![Value::NIL; 8]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("nested map-multiple");
    let slots = registers.as_vector_data().unwrap();
    assert_eq!(slots[0], Value::fixnum(20));
    assert_eq!(slots[1], Value::fixnum(3));
}

#[test]
fn ccl_execute_map_single_reads_a_pair_value() {
    crate::test_utils::init_test_tracing();
    let map = Value::vector(vec![Value::fixnum(0), Value::cons(Value::fixnum(1), Value::fixnum(55))]);
    let id = builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-pair-map"), map])
        .expect("pair map")
        .as_int()
        .unwrap();
    let program = Value::vector(
        [0, 4, 295_199, id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![Value::NIL; 8]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("pair map-single");
    let slots = registers.as_vector_data().unwrap();
    assert_eq!(slots[0], Value::fixnum(55));
    assert_eq!(slots[1], Value::fixnum(0));
}

#[test]
fn ccl_execute_map_multiple_applies_a_closed_open_range() {
    crate::test_utils::init_test_tracing();
    // `[t 77 5 9]` maps `5 <= value < 9` to 77. 9 is outside the range.
    let map = Value::vector(vec![
        Value::T,
        Value::fixnum(77),
        Value::fixnum(5),
        Value::fixnum(9),
    ]);
    let id = builtin_register_code_conversion_map_impl(vec![Value::symbol("ccl-range-map"), map])
        .expect("range map")
        .as_int()
        .unwrap();
    let program = Value::vector(
        [0, 5, 278_815, 1, id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let inside = Value::vector(vec![
        Value::fixnum(6),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, inside]).expect("range hit");
    assert_eq!(inside.as_vector_data().unwrap()[0], Value::fixnum(77));
    assert_eq!(inside.as_vector_data().unwrap()[1], Value::fixnum(0));

    let program = Value::vector(
        [0, 5, 278_815, 1, id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let outside = Value::vector(vec![
        Value::fixnum(9),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, outside]).expect("range miss");
    assert_eq!(outside.as_vector_data().unwrap()[0], Value::fixnum(9));
    assert_eq!(outside.as_vector_data().unwrap()[1], Value::fixnum(-1));
}

#[test]
fn ccl_execute_on_string_rejects_io_when_magnification_is_zero() {
    crate::test_utils::init_test_tracing();
    // GNU sets the output pointer to null when buffer magnification is 0,
    // so a write is an invalid command. 16660 is `(write 65)`.
    let err = builtin_ccl_execute_on_string_impl(vec![
        Value::vector(
            [0, 3, 16_660, 22]
                .into_iter()
                .map(Value::fixnum)
                .collect(),
        ),
        Value::vector(vec![Value::NIL; 9]),
        Value::heap_string(crate::heap_types::LispString::from_unibyte(Vec::new())),
    ])
    .expect_err("magnification 0 cannot write");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.data[0], Value::string("Error in CCL program at 3th code"));
        }
        other => panic!("expected error signal, got {other:?}"),
    }
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
    let map = Value::vector(vec![
        Value::fixnum(0),
        Value::symbol("ccl-iter-mapper"),
    ]);
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
fn ccl_execute_lookup_integer_sets_unicode_and_the_value() {
    crate::test_utils::init_test_tracing();
    let mut entries = std::collections::HashMap::new();
    entries.insert(16, 17);
    entries.insert(17, 16);
    let id = super::install_translation_hash(entries);
    // `lookup-integer` key in r0, value in r1. GNU stores charset id 2
    // (`unicode`) and sets r7 on success.
    let program = Value::vector(
        [0, 4, 311_359, id, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );
    let registers = Value::vector(vec![
        Value::fixnum(17),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    builtin_ccl_execute_impl(vec![program, registers]).expect("lookup-integer should run");
    let slots = registers.as_vector_data().unwrap();
    assert_eq!(slots[0], Value::fixnum(2));
    assert_eq!(slots[1], Value::fixnum(16));
    assert_eq!(slots[7], Value::fixnum(1));
}

#[test]
fn ccl_execute_on_string_runs_pgg_crc24() {
    crate::test_utils::init_test_tracing();
    // GNU `pgg-parse-crc24` vector and initial registers from
    // `pgg-parse-crc24-string`. The checksum is the three bytes
    // (r1 & 255), (r2 >> 8) & 255, (r2 & 255).
    let program = [
        1, 30, 14, 114744, 114775, 0, 161, 131127, 1, 148217, 15, 82167, 1, 1848, 131159, 1, 1595,
        5, 256, 114743, 390, 114775, 19707, 1467, 16, 7, 183, 1, -5628, -7164, 22,
    ];
    let mut registers = [0; 8];
    registers[1] = 183;
    registers[2] = 1230;
    for (input, expected) in [
        (b"foo".as_slice(), [0x4f, 0xc2, 0x55]),
        (b"bar", [0x51, 0xd9, 0x53]),
        (b"baz", [0xf0, 0x58, 0x6a]),
    ] {
        let (_output, status) = execute_ccl_on_string(&program, registers, input, true);
        let r1 = status[1].as_int().unwrap();
        let r2 = status[2].as_int().unwrap();
        assert_eq!(
            [r1 & 255, (r2 >> 8) & 255, r2 & 255],
            expected.map(i64::from)
        );
    }
}

#[test]
fn ccl_execute_on_string_runs_packed_constant_string_in_eof_block() {
    crate::test_utils::init_test_tracing();
    // GNU `ccl-compile` output for:
    //   (1 ((read r0) (write r0) (read r0)) (write "[EOF]"))
    let program = Value::vector(
        [1, 5, 14, 17, 14, 1332, 5_981_519, 4_611_328, 22]
            .into_iter()
            .map(Value::fixnum)
            .collect(),
    );

    for (input, expected) in [(Vec::new(), b"[EOF]".as_slice()), (vec![b'a'], b"a[EOF]")] {
        let status = Value::vector(vec![Value::NIL; 9]);
        let output = builtin_ccl_execute_on_string_impl(vec![
            program,
            status,
            Value::heap_string(crate::heap_types::LispString::from_unibyte(input)),
            Value::NIL,
            Value::T,
        ])
        .expect("the final block should write its packed ASCII constant");
        assert_eq!(output.as_lisp_string().unwrap().as_bytes(), expected);
        assert_eq!(status.as_vector_data().unwrap()[0], Value::fixnum(-1));
        assert_eq!(status.as_vector_data().unwrap()[8], Value::fixnum(8));
    }
}

#[test]
fn register_ccl_program_rejects_over_arity() {
    crate::test_utils::init_test_tracing();
    let err = builtin_register_ccl_program_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::NIL,
    ])
    .expect_err("over-arity should signal");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn register_code_conversion_map_rejects_over_arity() {
    crate::test_utils::init_test_tracing();
    let err = builtin_register_code_conversion_map_impl(vec![
        Value::symbol("foo"),
        Value::vector(vec![Value::fixnum(10), Value::fixnum(0), Value::fixnum(0)]),
        Value::NIL,
    ])
    .expect_err("over-arity should signal");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}
