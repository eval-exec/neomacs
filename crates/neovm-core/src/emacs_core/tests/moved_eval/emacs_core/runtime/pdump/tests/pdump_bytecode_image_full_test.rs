//! The bytecode-image pdump oracles: reader-constructed bytecode is
//! arena-resident before the dump and image-resident after it, GNU bytecode
//! stays cold until first execution, a corrupted stub-layout witness is
//! rejected before any baked struct bytes are trusted, and a lazy stub
//! survives a full GC and materializes on its first call.
//! (These are the bytecode-image FULL tests of eval's pdump_test.rs; the
//! CONTEXT-FREE stub-template determinism test stays in eval.)
//
// API-drift shims (post-split re-home): the pre-split tests reached
// `Context`, `dump_to_file` and `load_from_dump` through the pdump test
// module; the evaluator is now built through the core test facade,
// `load_from_dump` lives in core's load_boot, the header-size probe rides
// the mmap test-support facade, and tagged-heap probes go through the
// `tagged_heap()` accessor.
use crate::emacs_core::bytecode::chunk::{ByteCodeFunction, GnuByteOffsetMapEntry};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::load_boot::load_from_dump;
use crate::emacs_core::pdump::DumpError;
use crate::emacs_core::pdump::dump_to_file;
use neovm_value::value::{LambdaParams, Value};

#[test]
fn test_pdump_bytecode_round_trips_image_resident() {
    crate::test_utils::init_test_tracing();
    // task 03/3a, amended by the mapped-bytecode arc: the `#[...]` reader
    // literal (`closure_from_reader_literal_slots` -> `Value::make_bytecode`)
    // allocates from the BYTECODE ARENA PAGES pre-dump, while the pdump
    // restore installs the function INTO ITS IMAGE-RESERVED SPAN (the
    // `ByteCodeObj` reservation written by the dump; see
    // `reserve_typed_object_with_extras`). A restored function must be
    // image-resident, still callable, and survive a full GC in place.
    let mut eval = crate::test_utils::new_context();
    // Classic constant-returning GNU bytecode: constants[0]=42, Breturn.
    eval.eval_str("(defvar bcarena-pdump-fn #[0 \"\\300\\207\" [42] 1])")
        .expect("defvar should evaluate");
    let pre = eval
        .obarray()
        .symbol_value_copied("bcarena-pdump-fn")
        .expect("pre-dump value");
    assert_eq!(
        pre.veclike_type(),
        Some(neovm_value::tagged::header::VecLikeType::ByteCode)
    );
    assert!(
        eval.tagged_heap()
            .bytecode_arena_owns_for_test(pre.as_veclike_ptr().unwrap() as *const u8),
        "reader-constructed bytecode must live on the arena pages",
    );

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("bcarena.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut restored = load_from_dump(&dump_path).expect("load should succeed");

    let post = restored
        .obarray()
        .symbol_value_copied("bcarena-pdump-fn")
        .expect("restored value");
    assert_eq!(
        post.veclike_type(),
        Some(neovm_value::tagged::header::VecLikeType::ByteCode)
    );
    assert!(
        restored.tagged_heap().mapped_image_owns_for_test(post),
        "pdump-restored bytecode must live in its image-reserved span",
    );
    assert!(
        !restored
            .tagged_heap()
            .bytecode_arena_owns_for_test(post.as_veclike_ptr().unwrap() as *const u8),
        "image-resident bytecode must not also claim an arena page",
    );
    assert!(
        post.get_bytecode_data()
            .expect("restored bytecode data")
            .resident_ops()
            .is_empty(),
        "pdump-restored GNU bytecode should stay cold until execution",
    );

    // The restored function executes (payload round-tripped intact), and
    // still does after a full GC in the restored evaluator.
    let out = restored
        .eval_str("(funcall bcarena-pdump-fn)")
        .expect("restored bytecode runs");
    assert_eq!(out, Value::fixnum(42));
    assert!(
        !post
            .get_bytecode_data()
            .expect("restored bytecode data")
            .resident_ops()
            .is_empty(),
        "executing restored GNU bytecode should initialize its decoded IR",
    );
    restored.gc_collect();
    let out2 = restored
        .eval_str("(funcall bcarena-pdump-fn)")
        .expect("bytecode runs after GC");
    assert_eq!(out2, Value::fixnum(42));
    let after_gc = restored
        .obarray()
        .symbol_value_copied("bcarena-pdump-fn")
        .expect("value after GC");
    assert!(
        restored.tagged_heap().mapped_image_owns_for_test(after_gc),
        "image-resident bytecode survives GC in place",
    );
}

#[test]
fn test_pdump_rejects_stub_layout_witness_mismatch() {
    crate::test_utils::init_test_tracing();
    let eval = crate::test_utils::new_context();
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("witness-mismatch.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    // The witness is the LAST u64 of the header; corrupting it models an
    // image dumped by a binary whose ByteCodeFunction layout differs. The
    // loader must reject cleanly BEFORE trusting any baked struct bytes.
    let mut bytes = std::fs::read(&dump_path).expect("read dump bytes");
    let witness_start =
        crate::emacs_core::pdump::mmap_image_test_support::header_size_for_test() - 8;
    bytes[witness_start] ^= 0x01;
    std::fs::write(&dump_path, bytes).expect("rewrite dump bytes");

    match load_from_dump(&dump_path) {
        Err(DumpError::ImageFormatError(message)) => {
            assert!(
                message.contains("stub layout witness"),
                "unexpected rejection message: {message}"
            );
        }
        Ok(_) => panic!("expected witness mismatch rejection, but load succeeded"),
        Err(other) => panic!("expected witness mismatch rejection, got {other}"),
    }
}

#[test]
fn lazy_stub_survives_gc_and_materializes_on_first_call() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // Hand-assembled GNU function with every extras facet: params (required
    // + optional + rest), docstring, interactive spec, a heap-valued constant
    // and a heap-valued extra slot. Both values are hash tables: the loader
    // allocates them OUTSIDE the dump image, so after load the only thing
    // that keeps them alive is the stub walker's constant / extra-slot
    // emission during the collector cycle (an image-resident cons would be
    // permanent-black under the dump partition and could not witness the
    // walker at all).
    // Body: constant0; return  ->  returns the constant hash table.
    let secret_hash = eval
        .eval_str(
            "(let ((h (make-hash-table :test 'eq))) \
             (puthash 'pdump-lazy-key 'pdump-lazy-value h) h)",
        )
        .expect("constant hash-table fixture");
    let extra_hash = eval
        .eval_str(
            "(let ((h (make-hash-table :test 'eq))) \
             (puthash 'pdump-lazy-extra-key 'pdump-lazy-extra-value h) h)",
        )
        .expect("extra-slot hash-table fixture");
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![neovm_value::intern::intern("pdump-lazy-a")],
        optional: vec![neovm_value::intern::intern("pdump-lazy-b")],
        rest: Some(neovm_value::intern::intern("pdump-lazy-c")),
    });
    function.ops = vec![Op::Constant(0), Op::Return];
    function.constants = vec![secret_hash].into();
    function.extra_slots = vec![extra_hash];
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    function.lexical = true;
    function.docstring = Some(neovm_value::heap_types::LispString::from_unibyte(
        b"Lazy stub docstring.".to_vec(),
    ));
    function.interactive = Some(Value::string("p"));
    function.gnu_byte_offset_map = Some(vec![
        GnuByteOffsetMapEntry::new(0, 0),
        GnuByteOffsetMapEntry::new(1, 1),
    ]);
    // Bconstant0 (0xC0), Breturn (0x87).
    function.gnu_bytecode_bytes = Some(neovm_value::tagged::header::LispByteVec::owned(vec![
        0xC0, 0x87,
    ]));
    let func_val = Value::make_bytecode(function);
    eval.obarray_mut()
        .set_symbol_value("pdump-lazy-probe", func_val);

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("lazy.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");

    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let func = loaded
        .obarray()
        .symbol_value_copied("pdump-lazy-probe")
        .expect("value cell should be restored");
    assert!(func.is_bytecode(), "restored value must be bytecode");
    let was_stub = func.bytecode_data_if_materialized().is_none();

    // The interactive probe must answer WITHOUT materializing.
    let probe = func
        .bytecode_interactive_probe()
        .expect("bytecode probe should answer");
    assert!(
        probe.slot_count() >= 6,
        "interactive fn must classify as command (got {})",
        probe.slot_count()
    );
    if was_stub {
        assert!(
            func.bytecode_data_if_materialized().is_none(),
            "the interactive probe must not materialize a stub"
        );
    }

    // A full GC cycle with the stub un-materialized: the hash tables live
    // only in the mapped image words — the stub tracing legs must emit the
    // constant and extra-slot words so both heap objects survive.
    loaded
        .eval_str("(garbage-collect)")
        .expect("gc with live stubs should succeed");

    // First call materializes and runs; both hash tables must have survived
    // (UAF check for the constant and extra-slots emissions), and the params
    // must have round-tripped as runtime symbols.
    let result = loaded
        .eval_str(
            "(let ((r (funcall (symbol-value 'pdump-lazy-probe) 5))) \
                 (list (gethash 'pdump-lazy-key r) \
                       (gethash 'pdump-lazy-extra-key \
                                (aref (symbol-value 'pdump-lazy-probe) 6))))",
        )
        .expect("first call of a lazy stub should materialize and run");
    assert_eq!(
        format!("{result}"),
        "(pdump-lazy-value pdump-lazy-extra-value)"
    );
    let materialized = func
        .bytecode_data_if_materialized()
        .expect("the call must have materialized the stub");
    assert_eq!(materialized.extra_slots.len(), 1);
    if was_stub {
        let data = func
            .bytecode_data_if_materialized()
            .expect("the call must have materialized the stub");
        assert_eq!(
            neovm_value::intern::resolve_sym(data.params.named().unwrap().required[0]),
            "pdump-lazy-a",
            "param symbols must resolve after the fallback-path id rewrite"
        );
        assert_eq!(
            data.params
                .named()
                .unwrap()
                .rest
                .map(neovm_value::intern::resolve_sym),
            Some("pdump-lazy-c"),
        );
    }
}
