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
                Value::string("Error in CCL program at 6th code")
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
