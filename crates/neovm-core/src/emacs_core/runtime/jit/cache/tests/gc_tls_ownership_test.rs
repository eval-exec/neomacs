use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::value::LambdaParams;

fn cache_payload() -> Value {
    let payload = Value::list(vec![Value::string("gc-tls-jit-reloc")]);
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    function.ops = vec![Op::Constant(0), Op::Car, Op::Return];
    function.constants = vec![payload].into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    function.jit_runtime().set_hot_for_test();
    let object = Value::make_bytecode(function.clone());
    assert!(
        try_run_compiled(std::ptr::null_mut(), &function, object, &[])
            .unwrap()
            .is_some()
    );
    payload
}

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_jit_reloc_gc_roots_for_heap(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_jit_reloc_excludes_another_live_heap() {
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let mut first = Context::new();
    let mut second = Context::new();
    let payload = cache_payload();
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == payload.bits())
    );
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| root.bits() != payload.bits())
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_jit_reloc_reset_excludes_a_dropped_heap() {
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let payload_bits = {
        let first = Context::new();
        let payload = cache_payload();
        assert!(
            roots_for(&first)
                .iter()
                .any(|root| root.bits() == payload.bits())
        );
        payload.bits()
    };
    let mut next = Context::new();
    assert!(
        roots_for(&next)
            .iter()
            .all(|root| root.bits() != payload_bits)
    );
    next.gc_collect_exact();
}
