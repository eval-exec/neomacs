//! A compiled named call checks the executing World's actual function cell.
use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use crate::emacs_core::jit::compile::{CompiledLeaf, NativeRun};
use crate::emacs_core::jit::reopt::DeoptCause;
use neovm_value::symbol::FunctionEpochBump;

struct Named;
impl Named {
    #[doc(hidden)]
    pub fn enter() -> Self {
        compile::force_inline2_for_test(Some(compile::Inline2Mode::Named));
        Self
    }
}
impl Drop for Named {
    fn drop(&mut self) {
        compile::force_inline2_for_test(None);
    }
}

fn current_receiver(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    symbol: SymId,
) -> String {
    ctx.setup_thread_locals();
    let run = leaf.call_consts(
        ctx as *mut Context as *mut u8,
        source.constants.as_ptr(),
        &[],
    );
    let NativeRun::DeoptAt(resume) = run else {
        panic!("different current binding must refuse the compiled inline entry: {run:?}")
    };
    assert_eq!(resume.cause, Some(DeoptCause::InlineIdentity));
    assert_eq!(resume.pc, 1, "replay the original nullary call");
    assert_eq!(resume.stack, vec![Value::from_sym_id(symbol)]);
    assert!(resume.inlined.is_none(), "no old virtual callee entered");
    let value = compile::resumed_chain::resume_deopt(ctx, source, Value::NIL, leaf, *resume)
        .expect("current receiver resumes the original GNU call");
    coherent(ctx, ctx.obarray().function_epoch());
    crate::emacs_core::print_value(&value)
}

#[test]
fn p3sym_c4_compiled_a_current_b_identity_across_owned_era() {
    if !super::super::inline::jit_inline_on() {
        eprintln!("P3SYM_C4_RECEIVER_INAPPLICABLE NEOVM_JIT_INLINE=off");
        return;
    }
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Legacy);
    let _named = Named::enter();
    let (mut a, ids, a_targets, a_callers, a_replacement) = fixture("p3sym-c4-receiver");
    let _a_roots = a.retain_gc_roots(
        a_targets
            .into_iter()
            .chain(a_callers)
            .chain([a_replacement]),
    );
    let (mut b, b_ids, b_targets, b_callers, b_replacement) = fixture("p3sym-c4-receiver");
    let _b_roots = b.retain_gc_roots(
        b_targets
            .into_iter()
            .chain(b_callers)
            .chain([b_replacement]),
    );
    assert_eq!(ids, b_ids);
    assert_ne!(a.obarray.generation(), b.obarray.generation());
    assert_ne!(a.tagged_heap.heap_identity(), b.tagged_heap.heap_identity());
    assert_ne!(a_targets[0].bits(), b_targets[0].bits());
    let clock = a.obarray.function_epoch().max(b.obarray.function_epoch());
    for ctx in [&mut a, &mut b] {
        while ctx.obarray.function_epoch() < clock {
            ctx.obarray
                .invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
        }
        coherent(ctx, clock);
    }

    // This host-owned source contains only an immediate symbol. No value from
    // A's heap is supplied as a constant, operand or root in B's World.
    let mut source = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    source.lexical = true;
    source.ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
    source.constants = vec![Value::from_sym_id(ids[0])].into();
    source.max_stack = StackDepth::for_test(4);
    source.seal_hand_assembled_ops_for_test();
    source.jit_runtime().set_hot_for_test();
    a.setup_thread_locals();
    let leaf = compile::compile_bytecode_function_requested(
        &source,
        Some(&a.obarray),
        compile::CompileRequest {
            regalloc: compile::lowering::RegallocPolicy::Full,
            bypass_profit_gate: true,
            origin: super::super::stats::CompileOrigin::Retier,
            tier: super::super::tier2::CompileTier::Plain,
        },
    )
    .expect("actual named inline compilation under A");
    assert!(leaf.inline_deps().contains(&ids[0]));
    // A constant callee needs no interior replay chain. Its original object
    // in the leaf's relocations distinguishes named v2 from scalar MIR
    // inlining; the exact entry refusal below proves the emitted guard.
    assert!(
        leaf.reloc_values().contains(&a_targets[0]),
        "actual named region retains its selected definition identity"
    );
    let reloc_roots = a.retain_gc_roots(leaf.reloc_values().iter().copied());
    let before_a = match leaf.call_consts(
        &mut a as *mut Context as *mut u8,
        source.constants.as_ptr(),
        &[],
    ) {
        NativeRun::Ok(bits) => printed(bits),
        other => panic!("original World must enter the actual native body: {other:?}"),
    };
    coherent(&a, clock);
    let before_b = current_receiver(&mut b, &source, &leaf, ids[0]);
    assert_eq!(b.obarray.function_epoch(), clock, "equal clocks at refusal");
    assert_eq!(format!("({before_a} {before_b})"), oracle()[0]);
    eprintln!("P3SYM_C4_RECEIVER_ROW row=1 world=A clock={clock} actual={before_a}");
    eprintln!("P3SYM_C4_RECEIVER_ROW row=2 world=B clock={clock} actual={before_b}");

    test_support::settle(&mut b);
    let replacement = input(&mut b, b_replacement);
    let (receipt, _) = b.c4_function_era_for_test(
        ids,
        FunctionEraEdit::Replace { symbol: 0 },
        replacement,
        |_| {},
    );
    assert_eq!(receipt.before().facts()[0].0, clock);
    assert_eq!(receipt.after().facts()[0].0, clock + 1);
    coherent(&b, clock + 1);
    let after_b = current_receiver(&mut b, &source, &leaf, ids[0]);
    assert_eq!(format!("({before_b} {after_b})"), oracle()[2]);
    assert_eq!(a.obarray.symbol_function_id(ids[0]), Some(a_targets[0]));
    coherent(&a, clock);
    a.setup_thread_locals();
    let after_a = match leaf.call_consts(
        &mut a as *mut Context as *mut u8,
        source.constants.as_ptr(),
        &[],
    ) {
        NativeRun::Ok(bits) => printed(bits),
        other => panic!("A's independent binding remains a valid native entry: {other:?}"),
    };
    assert_eq!(after_a, before_a, "B's edit cannot change A's World");
    coherent(&a, clock);
    eprintln!(
        "P3SYM_C4_RECEIVER_ROW row=3 world=B clock={} actual={after_b}",
        clock + 1
    );
    eprintln!("P3SYM_C4_RECEIVER_ROW row=4 world=A clock={clock} actual={after_a}");
    // The owned leaf outlives every observation but is released before its
    // original-heap relocation roots. No evicted TLS leaf pointer is borrowed.
    drop(leaf);
    drop(reloc_roots);
}
