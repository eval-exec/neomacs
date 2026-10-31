use super::*;
use crate::emacs_core::error::FlowRef;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::LambdaParams;
use crate::heap_types::LispString;

#[derive(Debug)]
#[must_use]
struct InlineStringGuard(std::marker::PhantomData<*const ()>);

impl InlineStringGuard {
    fn new() -> Self {
        crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
        force_leaf_knob_for_test(Some(LeafKnob {
            string: true,
            ..LeafKnob::OFF
        }));
        Self(std::marker::PhantomData)
    }
}

impl Drop for InlineStringGuard {
    fn drop(&mut self) {
        force_leaf_knob_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
    }
}

#[derive(Clone, Copy, Debug)]
enum ArrayOp {
    Aref,
    Aset,
}

fn array_fn(op: ArrayOp) -> ByteCodeFunction {
    let nargs = match op {
        ArrayOp::Aref => 2,
        ArrayOp::Aset => 3,
    };
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(crate::emacs_core::intern::SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.ops = match op {
        ArrayOp::Aset => vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        ArrayOp::Aref => vec![Op::StackRef(1), Op::StackRef(1), Op::Aref, Op::Return],
    };
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    function
}

fn declared_name() -> Value {
    Value::heap_string(LispString::from_dump(vec![0xe9, 0xe9, 0xe9], 3, 3))
}

#[test]
fn declared_multibyte_high_bytes_miss_jit_raw_byte_aref_and_aset_guards() {
    let mut context = Context::new();
    let _guard = InlineStringGuard::new();
    let aref = compile_bytecode_function(&array_fn(ArrayOp::Aref)).expect("aref compiles");
    let aset = compile_bytecode_function(&array_fn(ArrayOp::Aset)).expect("aset compiles");
    let context_ptr = &mut context as *mut Context as *mut u8;
    let name = declared_name();
    let args = [name, Value::fixnum(0)];
    assert_eq!(
        Value::from_bits(
            // SAFETY: the test frame supplies the live `ctx` and argument words; the shim contract holds.
            unsafe { neovm_jit_aref(context_ptr, args[0].bits() as i64, args[1].bits() as i64) }
                as usize
        ),
        Value::fixnum(0xaaa9),
    );
    match aref.call(context_ptr, &args) {
        NativeRun::Ok(bits) => assert_eq!(Value::from_bits(bits), Value::fixnum(0xaaa9)),
        other => panic!("bounded decoder should return a character: {other:?}"),
    }
    let replacement = Value::fixnum(65);
    let args = [name, Value::fixnum(0), replacement];
    // SAFETY: the test frame supplies the live `ctx` and argument words; the shim contract holds.
    let status = unsafe {
        neovm_jit_aset(
            context_ptr,
            name.bits() as i64,
            args[1].bits() as i64,
            replacement.bits() as i64,
        )
    };
    assert_eq!(status, VALUE_SHIM_SIGNAL);
    assert!(matches!(
        take_pending_flow().unwrap().kind(),
        FlowRef::Signal(_)
    ));
    assert!(matches!(aset.call(context_ptr, &args), NativeRun::Signal));
    assert!(matches!(
        take_pending_flow().unwrap().kind(),
        FlowRef::Signal(_)
    ));
    assert_eq!(
        name.as_lisp_string().unwrap().as_bytes(),
        &[0xe9, 0xe9, 0xe9]
    );
    // Ordinary MB ASCII and unibyte high bytes still take their byte paths.
    for (string, expected) in [
        (LispString::from_utf8("a"), 97),
        (LispString::from_unibyte(vec![0xe9]), 233),
    ] {
        let array = Value::heap_string(string);
        match aref.call(context_ptr, &[array, Value::fixnum(0)]) {
            NativeRun::Ok(bits) => assert_eq!(Value::from_bits(bits), Value::fixnum(expected)),
            other => panic!("ordinary string access changed: {other:?}"),
        }
    }
}
