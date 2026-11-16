//! Heap-free values can cross between independently booted owner threads.

use crate::transfer::{self, Call, Transfer, TransferError};
use neovm_core::Value;
use neovm_core::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::load::apply_runtime_startup_state;
use neovm_core::emacs_core::load_boot::create_bootstrap_evaluator_cached;
use neovm_core::emacs_core::print::print_value_with_eval;
use neovm_core::emacs_core::value::{
    HashTableTest, HashTableWeakness, get_string_text_properties_for_value, list_length,
};
use neovm_core::heap_types::LispString;

fn context() -> Box<Context> {
    let mut ctx = Box::new(create_bootstrap_evaluator_cached().expect("bootstrap evaluator"));
    ctx.setup_thread_locals();
    apply_runtime_startup_state(&mut ctx).expect("runtime startup state");
    ctx
}

fn eval(ctx: &mut Context, form: &str) -> Value {
    ctx.eval_str(form)
        .unwrap_or_else(|error| panic!("{form}: {error:?}"))
}

fn printed(ctx: &Context, value: &Value) -> String {
    print_value_with_eval(ctx, value)
}

fn add_value(ctx: &Context, cases: &mut Vec<(String, String, Transfer)>, name: &str, value: Value) {
    let expected = if name == "uninterned-symbol" {
        "@interned-symbol".to_string()
    } else {
        printed(ctx, &value)
    };
    let transfer = transfer::from_value(ctx, value).expect(name);
    let wire = transfer::encode(&transfer);
    let decoded = transfer::decode(&wire).expect(name);
    assert_eq!(decoded, transfer, "wire row {name}");
    cases.push((name.to_string(), expected, decoded));
}

fn add_form(
    ctx: &mut Context,
    cases: &mut Vec<(String, String, Transfer)>,
    name: &str,
    form: &str,
) {
    let value = eval(ctx, form);
    add_value(ctx, cases, name, value);
}

fn cross_thread(cases: Vec<(String, String, Transfer)>) -> Vec<(String, String, String)> {
    std::thread::spawn(move || {
        let mut ctx = context();
        cases
            .into_iter()
            .map(|(name, expected, transfer)| {
                let value = transfer::to_value(&mut ctx, &transfer).unwrap_or_else(|error| {
                    panic!("{name}: {error:?}; encoded value: {transfer:?}")
                });
                let actual = if expected == "@million-list" {
                    format!("{} elements", list_length(&value).unwrap_or(0))
                } else if expected == "@interned-symbol" {
                    printed(&ctx, &value)
                } else if expected == "@bytecode-constants" {
                    let constants = &value
                        .get_bytecode_data()
                        .expect("rebuilt byte-code function")
                        .constants;
                    format!(
                        "closure={} string={}",
                        constants.iter().any(|value| value.is_lambda()),
                        constants.iter().any(|value| value.is_string())
                    )
                } else {
                    printed(&ctx, &value)
                };
                (name, expected, actual)
            })
            .collect()
    })
    .join()
    .expect("receiving owner thread")
}

#[test]
fn value_matrix_round_trips_across_contexts() {
    let mut main = context();
    let mut cases = Vec::new();
    for (name, form) in [
        ("fixnum-max", "most-positive-fixnum"),
        ("fixnum-min", "most-negative-fixnum"),
        ("bignum-max-plus-one", "(1+ most-positive-fixnum)"),
        ("bignum-min-minus-one", "(1- most-negative-fixnum)"),
        ("nil", "nil"),
        ("t", "t"),
        ("keyword", ":transfer-keyword"),
        ("interned-symbol", "'transfer-symbol"),
        ("uninterned-symbol", "(make-symbol \"transfer-uninterned\")"),
        ("empty-string", "\"\""),
        ("ascii-string", "\"worker\""),
        ("multibyte-string", "\"café 中\""),
        ("unibyte-string", "(string-to-unibyte \"ascii\")"),
        ("dotted-pair", "'(1 2 . 3)"),
        ("vector-of-vectors", "[[1 2] [3 [4]]]"),
        ("record", "(record 'point 1 2)"),
        ("eieio-shaped-record", "(record 'outer (record 'inner 1) 2)"),
        (
            "interpreted-closure",
            "(eval '(let ((base 40)) (lambda (n) (+ base n))) t)",
        ),
        (
            "nested-lambda-body",
            "(lambda (x) (funcall (lambda (y) (+ x y)) 2))",
        ),
        ("subr-value", "(symbol-function 'car)"),
    ] {
        add_form(&mut main, &mut cases, name, form);
    }

    let bytecode = eval(
        &mut main,
        "(progn (defun transfer-test-bytecode () (list \"constant\" 7)) (let* ((compiled (byte-compile 'transfer-test-bytecode)) (closure (eval '(let ((base 4)) (lambda (x) (+ base x))) t)) (constants (vconcat (aref compiled 2) (vector closure)))) (make-byte-code (aref compiled 0) (aref compiled 1) constants (aref compiled 3))))",
    );
    let bytecode_transfer = transfer::from_value(&main, bytecode).expect("byte-code constants");
    assert!(matches!(bytecode_transfer, Transfer::ByteCode(_)));
    let wire = transfer::encode(&bytecode_transfer);
    let decoded = transfer::decode(&wire).expect("byte-code wire");
    assert_eq!(decoded, bytecode_transfer);
    cases.push((
        "byte-code-closure-and-string-constants".into(),
        "@bytecode-constants".into(),
        decoded,
    ));

    let inner = Value::make_record(vec![Value::symbol("inner"), Value::fixnum(1)]);
    let eieio_record = Value::make_record(vec![inner, Value::fixnum(2)]);
    add_value(&main, &mut cases, "eieio-slot-zero-record", eieio_record);

    for (name, value) in [
        ("nan", Value::make_float(f64::NAN)),
        ("positive-infinity", Value::make_float(f64::INFINITY)),
        ("negative-infinity", Value::make_float(f64::NEG_INFINITY)),
        ("negative-zero", Value::make_float(-0.0)),
        (
            "raw-unibyte-string",
            Value::heap_string(LispString::from_unibyte((0x80..=0xff).collect())),
        ),
    ] {
        add_value(&main, &mut cases, name, value);
    }

    for (nbits, label) in [
        (0, "bool-vector-0"),
        (1, "bool-vector-1"),
        (64, "bool-vector-64"),
        (65, "bool-vector-65"),
    ] {
        add_form(
            &mut main,
            &mut cases,
            label,
            &format!("(make-bool-vector {nbits} t)"),
        );
    }

    for test in ["eq", "eql", "equal"] {
        for weakness in [
            None,
            Some("key"),
            Some("value"),
            Some("key-or-value"),
            Some("key-and-value"),
        ] {
            let weakness_form = weakness.map_or(String::new(), |w| format!(" :weakness '{w}"));
            let form = format!(
                "(let ((table (make-hash-table :test '{test}{weakness_form}))) (puthash 'key '(value) table) table)"
            );
            let row = format!("hash-{test}-{}", weakness.unwrap_or("strong"));
            add_form(&mut main, &mut cases, &row, &form);
        }
    }

    let mut list = Value::fixnum(1);
    for _ in 0..1_000 {
        list = Value::cons(list, Value::NIL);
    }
    add_value(&main, &mut cases, "nested-list-depth-1000", list);
    let long_list = eval(&mut main, "(make-list 1000000 7)");
    let transfer = transfer::from_value(&main, long_list).expect("million-element list");
    assert!(matches!(&transfer, Transfer::List { items, tail: None } if items.len() == 1_000_000));
    let wire = transfer::encode(&transfer);
    let decoded = transfer::decode(&wire).expect("million-element list wire");
    assert_eq!(decoded, transfer);
    cases.push((
        "million-element-list".into(),
        "@million-list".into(),
        decoded,
    ));

    for (name, expected, actual) in cross_thread(cases) {
        match expected.as_str() {
            "@million-list" => assert_eq!(actual, "1000000 elements", "{name}"),
            "@interned-symbol" => assert_eq!(actual, "transfer-uninterned", "{name}"),
            "@bytecode-constants" => {
                assert_eq!(actual, "closure=true string=true", "{name}")
            }
            _ => assert_eq!(actual, expected, "cross-context row {name}"),
        }
    }
}

#[test]
fn transfer_rejects_cycles_and_preserves_shared_dags() {
    let mut ctx = context();
    for form in [
        "(let ((x (list nil))) (setcar x x) x)",
        "(let ((x (list 1))) (setcdr x x) x)",
        "(let ((x (vector nil))) (aset x 0 x) x)",
        "(let ((x (record 'cycle nil))) (aset x 1 x) x)",
        "(let ((x (make-hash-table :test 'eq))) (puthash 'self x x) x)",
    ] {
        let value = eval(&mut ctx, form);
        assert_eq!(
            transfer::from_value(&ctx, value),
            Err(TransferError::Circular),
            "{form}"
        );
    }
    let dag = eval(
        &mut ctx,
        "(let ((shared (list 1 2))) (vector shared shared))",
    );
    assert!(matches!(
        transfer::from_value(&ctx, dag),
        Ok(Transfer::Vector(_))
    ));
}

#[test]
fn unsupported_lisp_objects_report_their_cl_type_names() {
    let mut ctx = context();
    for (form, expected) in [
        ("(current-buffer)", "buffer"),
        ("(obarray-make)", "obarray"),
        ("(make-marker)", "marker"),
        ("(make-overlay 1 2)", "overlay"),
        ("(selected-window)", "window"),
        ("(selected-frame)", "frame"),
        ("(frame-terminal (selected-frame))", "terminal"),
        ("(current-window-configuration)", "window-configuration"),
        ("(make-mutex \"transfer-test\")", "mutex"),
        (
            "(make-condition-variable (make-mutex \"transfer-test-cv\"))",
            "condition-variable",
        ),
        ("(make-finalizer #'ignore)", "finalizer"),
        ("(make-char-table 'transfer-test)", "char-table"),
        (
            "(make-process :name \"transfer-test\" :command '(\"true\") :noquery t)",
            "process",
        ),
    ] {
        let value = eval(&mut ctx, form);
        assert_eq!(
            transfer::from_value(&ctx, value),
            Err(TransferError::Unsupported(expected)),
            "{form}"
        );
    }
    let user_test = eval(
        &mut ctx,
        "(progn (define-hash-table-test 'transfer-custom (lambda (a b) (eq a b)) (lambda (a) 1)) (make-hash-table :test 'transfer-custom))",
    );
    assert_eq!(
        transfer::from_value(&ctx, user_test),
        Err(TransferError::UserHashTableTest)
    );
}

#[test]
fn propertized_strings_are_refused_by_name() {
    let mut ctx = context();
    let value = eval(&mut ctx, "(propertize \"text\" 'face 'bold)");
    assert!(get_string_text_properties_for_value(value).is_some());
    assert_eq!(
        transfer::from_value(&ctx, value),
        Err(TransferError::Unsupported("propertized-string"))
    );
}

#[test]
fn decoder_rejects_random_and_truncated_wire_without_panicking() {
    let value = Transfer::List {
        items: vec![Transfer::Symbol("fuzz".into()), Transfer::Fixnum(42)],
        tail: Some(Box::new(Transfer::Nil)),
    };
    let valid = transfer::encode(&value);
    let call = transfer::encode_call(&Call {
        function: Transfer::Subr("car".into()),
        args: vec![value],
    });
    for end in 0..valid.len() {
        assert!(matches!(
            transfer::decode(&valid[..end]),
            Err(TransferError::Decode(_))
        ));
    }
    for end in 0..call.len() {
        assert!(matches!(
            transfer::decode_call(&call[..end]),
            Err(TransferError::Decode(_))
        ));
    }
    let mut state = 0x9e37_79b9_u32;
    for _ in 0..10_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let len = (state as usize) % 48;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            bytes.push(state as u8);
        }
        if let Some(tag) = bytes.first_mut() {
            *tag = u8::MAX;
        }
        assert!(matches!(
            transfer::decode(&bytes),
            Err(TransferError::Decode(_))
        ));
        assert!(matches!(
            transfer::decode_call(&bytes),
            Err(TransferError::Decode(_))
        ));
    }
    let mut oversized = vec![5, 0];
    oversized.extend_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        transfer::decode(&oversized),
        Err(TransferError::Decode(_))
    ));
    let mut oversized_call = b"NWC1".to_vec();
    oversized_call.extend([0, 255]);
    oversized_call.extend_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        transfer::decode_call(&oversized_call),
        Err(TransferError::Decode(_))
    ));
}

#[test]
fn hash_table_wire_variants_are_exhaustive() {
    assert_eq!(
        [HashTableTest::Eq, HashTableTest::Eql, HashTableTest::Equal].len(),
        3
    );
    assert_eq!(
        [
            None,
            Some(HashTableWeakness::Key),
            Some(HashTableWeakness::Value),
            Some(HashTableWeakness::KeyOrValue),
            Some(HashTableWeakness::KeyAndValue),
        ]
        .len(),
        5
    );
}

#[test]
fn wire_decoder_rejects_bool_vectors_with_inconsistent_word_counts() {
    let mut bytes = vec![10]; // TAG_BOOL_VECTOR
    bytes.extend_from_slice(&65_u64.to_le_bytes()); // nbits requires two words
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // but the wire has none

    assert!(matches!(transfer::decode(&bytes), Err(TransferError::Decode(_))));
}

#[test]
fn wire_decoder_rejects_nested_values_beyond_the_transfer_depth_limit() {
    let mut bytes = Vec::new();
    for _ in 0..=4096 {
        bytes.push(8); // TAG_VECTOR
        bytes.extend_from_slice(&1_u64.to_le_bytes());
    }
    bytes.push(0); // TAG_NIL

    assert!(matches!(
        transfer::decode(&bytes),
        Err(TransferError::TooDeep)
    ));
}
