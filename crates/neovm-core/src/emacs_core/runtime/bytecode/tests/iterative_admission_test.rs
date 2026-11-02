use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;

fn admission_function(argdesc: i64, max_stack: usize, bytes: Vec<u8>) -> ByteCodeFunction {
    let params = FunctionParams::try_from(Value::fixnum(argdesc)).unwrap();
    let mut func = ByteCodeFunction::new(params);
    func.max_stack = StackDepth::for_test(max_stack);
    func.constants = vec![Value::fixnum(42)].into();
    func.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(bytes));
    func.defer_gnu_decode();
    assert!(func.lazy_gnu_code.is_some());
    func
}

#[test]
fn bytecode_lazy_iterative_admission_decodes_real_gnu_code_and_keeps_arity_bounds() {
    let func = admission_function((2 << 8) | 1, 3, vec![192, 135]);
    let lazy = func.lazy_gnu_code.as_ref().unwrap();
    assert!(lazy.decoded.get().is_none());
    assert!(lazy.iterative_admission.get().is_none());
    assert_eq!(func.lazy_iterative_admission(0), Some(false));
    assert!(
        lazy.decoded.get().is_none(),
        "wrong arity must keep GNU verification/decode cold"
    );
    assert!(
        lazy.iterative_admission.get().is_none(),
        "wrong arity cannot poison a later valid optional-arity call"
    );
    for (nargs, admitted) in [
        (1, true),
        (2, true),
        (0, false),
        (3, false),
        (usize::MAX, false),
        (1, true),
    ] {
        assert_eq!(func.lazy_iterative_admission(nargs), Some(admitted));
    }
    assert!(lazy.decoded.get().unwrap().stack_verified);
    assert_eq!(func.executable_ops(), &[Op::Constant(0), Op::Return]);
    let range = lazy.iterative_admission.get().unwrap().unwrap();
    assert_eq!((range.required, range.nonrest), (1, 2));
}

#[test]
fn bytecode_lazy_iterative_admission_checks_each_clones_environment() {
    let proto = admission_function(0, 1, vec![192, 135]);
    let mut captured = proto.clone();
    captured.env = Some(Value::NIL);
    assert!(std::sync::Arc::ptr_eq(
        proto.lazy_gnu_code.as_ref().unwrap(),
        captured.lazy_gnu_code.as_ref().unwrap()
    ));
    // A cold captured clone must not store a source-wide rejection.
    assert_eq!(captured.lazy_iterative_admission(0), Some(false));
    assert!(
        proto
            .lazy_gnu_code
            .as_ref()
            .unwrap()
            .iterative_admission
            .get()
            .is_none()
    );
    assert_eq!(proto.lazy_iterative_admission(0), Some(true));
    // Nor may a captured clone consume its env-less prototype's proof.
    assert_eq!(captured.lazy_iterative_admission(0), Some(false));
    let envless = proto.clone();
    assert_eq!(envless.lazy_iterative_admission(0), Some(true));
    assert!(std::ptr::eq(
        proto
            .lazy_gnu_code
            .as_ref()
            .unwrap()
            .iterative_admission
            .get()
            .unwrap(),
        envless
            .lazy_gnu_code
            .as_ref()
            .unwrap()
            .iterative_admission
            .get()
            .unwrap()
    ));
}

#[test]
fn bytecode_lazy_iterative_admission_refuses_unverified_oversized_and_rest_code() {
    // An empty Return intentionally returns nil; Pop at entry really underflows.
    let unverified = admission_function(0, 1, vec![136, 135]);
    assert_eq!(unverified.lazy_iterative_admission(0), Some(false));
    assert!(!unverified.executes_verified_ops());
    let oversized = admission_function(
        0,
        StackDepth::ITERATIVE_FRAME_LIMIT.get() + 1,
        vec![192, 135],
    );
    assert_eq!(oversized.lazy_iterative_admission(0), Some(false));
    let rest = admission_function(128, 2, vec![192, 135]);
    assert_eq!(rest.lazy_iterative_admission(0), Some(false));
    for func in [&unverified, &oversized, &rest] {
        assert!(
            func.lazy_gnu_code
                .as_ref()
                .unwrap()
                .iterative_admission
                .get()
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn bytecode_lazy_iterative_admission_restarts_when_decode_is_reinstalled() {
    let mut func = admission_function(0, 1, vec![192, 135]);
    assert_eq!(func.lazy_iterative_admission(0), Some(true));
    let old = func.lazy_gnu_code.as_ref().unwrap().clone();
    // Shape changes are finalized before reinstalling the GNU decode cell.
    func.params = FunctionParams::try_from(Value::fixnum((2 << 8) | 1)).unwrap();
    func.arglist = Value::fixnum((2 << 8) | 1);
    func.max_stack = StackDepth::for_test(3);
    func.restore_gnu_decode_policy().unwrap();
    let restored = func.lazy_gnu_code.as_ref().unwrap().clone();
    assert!(!std::sync::Arc::ptr_eq(&old, &restored));
    assert!(restored.iterative_admission.get().is_none());
    assert_eq!(func.lazy_iterative_admission(0), Some(false));
    assert_eq!(func.lazy_iterative_admission(1), Some(true));
    let range = restored.iterative_admission.get().unwrap().unwrap();
    assert_eq!((range.required, range.nonrest), (1, 2));
    // Defer is another installer and must also start with a fresh cell.
    func.defer_gnu_decode();
    let deferred = func.lazy_gnu_code.as_ref().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&restored, deferred));
    assert!(deferred.iterative_admission.get().is_none());
    assert_eq!(func.lazy_iterative_admission(2), Some(true));
}

#[test]
fn bytecode_lazy_iterative_admission_preserves_shared_layout_and_eager_fallback() {
    #[repr(C)]
    struct OriginalPrefix {
        decoded: OnceLock<DecodedGnuCode>,
        proof: OnceLock<super::super::decode::StackProof>,
    }
    assert_eq!(
        std::mem::offset_of!(LazyGnuCode, decoded),
        std::mem::offset_of!(OriginalPrefix, decoded)
    );
    assert_eq!(
        std::mem::offset_of!(LazyGnuCode, proof),
        std::mem::offset_of!(OriginalPrefix, proof)
    );
    // The decoded payload the hot projection returns: run_loop inlines these
    // three field projections, so freeze their layout beside LazyGnuCode's.
    struct OriginalDecoded {
        ops: Vec<Op>,
        byte_offset_map: Vec<crate::tagged::bytecode::chunk::GnuByteOffsetMapEntry>,
        stack_verified: bool,
    }
    assert_eq!(
        std::mem::offset_of!(DecodedGnuCode, ops),
        std::mem::offset_of!(OriginalDecoded, ops)
    );
    assert_eq!(
        std::mem::offset_of!(DecodedGnuCode, byte_offset_map),
        std::mem::offset_of!(OriginalDecoded, byte_offset_map)
    );
    assert_eq!(
        std::mem::offset_of!(DecodedGnuCode, stack_verified),
        std::mem::offset_of!(OriginalDecoded, stack_verified)
    );
    // The enclosing function projection: the VM admission/decode path reads
    // these ByteCodeFunction fields before it reaches the lazy cell. Pin the
    // same field order/types in a frozen mirror; adding, reordering or
    // retyping a field moves the real offsets and fails here.
    struct OriginalFunction {
        source_id: u64,
        ops: Vec<Op>,
        ops_sealed: bool,
        stack_verified: bool,
        constants: crate::tagged::header::LispValueVec,
        max_stack: StackDepth,
        params: FunctionParams,
        arglist: Value,
        lexical: bool,
        env: Option<Value>,
        gnu_byte_offset_map: Option<Vec<crate::tagged::bytecode::chunk::GnuByteOffsetMapEntry>>,
        gnu_bytecode_bytes: Option<crate::tagged::header::LispByteVec>,
        docstring: Option<crate::heap_types::LispString>,
        doc_form: Option<Value>,
        interactive: Option<Value>,
        closure_slot_count: usize,
        extra_slots: Vec<Value>,
        #[cfg(feature = "jit")]
        runtime: Option<crate::emacs_core::jit::Runtime>,
        lazy_gnu_code: Option<std::sync::Arc<LazyGnuCode>>,
    }
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, ops),
        std::mem::offset_of!(OriginalFunction, ops)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, constants),
        std::mem::offset_of!(OriginalFunction, constants)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, max_stack),
        std::mem::offset_of!(OriginalFunction, max_stack)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, params),
        std::mem::offset_of!(OriginalFunction, params)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, arglist),
        std::mem::offset_of!(OriginalFunction, arglist)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, lexical),
        std::mem::offset_of!(OriginalFunction, lexical)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, env),
        std::mem::offset_of!(OriginalFunction, env)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, gnu_bytecode_bytes),
        std::mem::offset_of!(OriginalFunction, gnu_bytecode_bytes)
    );
    assert_eq!(
        std::mem::offset_of!(ByteCodeFunction, lazy_gnu_code),
        std::mem::offset_of!(OriginalFunction, lazy_gnu_code)
    );
    fn require_shared<T: Send + Sync>() {}
    require_shared::<LazyGnuCode>();
    let eager = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    assert_eq!(eager.lazy_iterative_admission(0), None);
}
