use super::*;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::cache;
use crate::emacs_core::value::LambdaParams;
use std::sync::atomic::Ordering;

// The header writes an observable variable BEFORE its first numeric operation.
// Entry rejection must happen before that write, not at the old per-op guard.
fn effect_before_numeric_loop() -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("payload"), intern("n")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![
        Value::make_int(0),
        Value::make_int(3),
        Value::symbol("osr-entry-held"),
        Value::symbol("osr-entry-effect"),
        Value::make_int(42),
    ]
    .into();
    f.ops = vec![
        Op::Constant(1),
        Op::VarBind(2),
        Op::Constant(0), // payload n i
        Op::Goto(4),
        Op::Constant(4), // header
        Op::VarSet(3),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(14),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(4),
        Op::Pop,
        Op::Pop,
        Op::Unbind(1),
        Op::Return,
    ];
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f
}

fn install_bindings(ctx: &mut Context) -> (usize, usize) {
    ctx.eval_str("(setq osr-entry-held 17 osr-entry-outer 23 osr-entry-effect 0)")
        .unwrap();
    let outer = ctx.specpdl.len();
    ctx.try_specbind(intern("osr-entry-outer"), Value::make_int(29))
        .unwrap();
    ctx.jit_bind_stack.push(outer);
    let borrowed = ctx.specpdl.len();
    ctx.try_specbind(intern("osr-entry-held"), Value::make_int(3))
        .unwrap();
    (outer, borrowed)
}

#[test]
fn osr_entry_guard_checks_each_snapshot_before_effects_and_keeps_unknown_heap_slots() {
    let mut ctx = Context::new();
    let (outer, borrowed) = install_bindings(&mut ctx);
    let spec_depth = ctx.specpdl.len();
    let f = effect_before_numeric_loop();
    let payload = Value::list(vec![Value::string("live payload")]);
    // Model a published interpreter snapshot with a caller-owned prefix.
    ctx.bc_buf.push(Value::make_int(123));
    ctx.bc_buf.push(payload);
    for seed in [
        Value::make_float(0.5),
        Value::string("not an integer"),
        Value::NIL,
    ] {
        let snapshot = [payload, Value::make_int(30), seed];
        ctx.bc_buf.truncate(1);
        ctx.bc_buf.extend_from_slice(&snapshot);
        let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
        let Some(NativeRun::DeoptAt(resume)) =
            cache::try_run_osr(&mut ctx, &f, 4, &snapshot, &[borrowed])
        else {
            panic!("incompatible seed must precisely deopt at entry");
        };
        assert_eq!(
            cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed),
            before + 1
        );
        assert_eq!(resume.pc, 4);
        assert_eq!(resume.stack, snapshot);
        assert_eq!(resume.binds, [borrowed]);
        assert_eq!(resume.handlers, 0);
        assert_eq!(resume.spec_base, spec_depth);
        assert_eq!(resume.cond_base, ctx.condition_stack_len());
        assert_eq!(ctx.specpdl.len(), spec_depth);
        assert_eq!(ctx.jit_bind_stack, [outer]);
        assert_eq!(ctx.jit_root_stack_top, 0);
        assert_eq!(
            ctx.eval_str("osr-entry-effect").unwrap(),
            Value::make_int(0)
        );
    }
    // Reuse the SAME cached OSR leaf. Only i is proven integer; the payload
    // argument remains untyped and must accept a heap value unchanged.
    let snapshot = [payload, Value::make_int(30), Value::make_int(10)];
    ctx.bc_buf.truncate(1);
    ctx.bc_buf.extend_from_slice(&snapshot);
    let Some(NativeRun::Ok(bits)) = cache::try_run_osr(&mut ctx, &f, 4, &snapshot, &[borrowed])
    else {
        panic!("compatible seed must run the native body");
    };
    assert_eq!(Value::from_bits(bits), payload);
    assert_eq!(ctx.specpdl.len(), borrowed);
    assert_eq!(ctx.jit_bind_stack, [outer]);
    assert_eq!(ctx.jit_root_stack_top, 0);
    assert_eq!(
        ctx.eval_str("osr-entry-effect").unwrap(),
        Value::make_int(42)
    );
    assert_eq!(ctx.eval_str("osr-entry-held").unwrap(), Value::make_int(17));
    ctx.bc_buf.clear();
    ctx.jit_bind_stack.clear();
    ctx.unbind_to(outer);
}

#[test]
fn osr_entry_guard_keeps_heap_operands_through_native_and_resumed_watched_store_gc() {
    for guard_fails in [true, false] {
        let mut ctx = Context::new();
        let (outer, borrowed) = install_bindings(&mut ctx);
        ctx.eval_str(
            "(progn
           (setq osr-entry-writes 0)
           (fset 'osr-entry-watch
             (lambda (_symbol _value operation _where)
               (if (eq operation 'set)
                 (progn
                   (setq osr-entry-writes (1+ osr-entry-writes))
                   (garbage-collect)))))
           (add-variable-watcher 'osr-entry-effect 'osr-entry-watch))",
        )
        .unwrap();
        let f = effect_before_numeric_loop();
        let payload = Value::string("x".repeat(40));
        let seed = if guard_fails {
            Value::make_float(0.5)
        } else {
            Value::make_int(0)
        };
        let snapshot = [payload, Value::make_int(2), seed];
        ctx.bc_buf.extend_from_slice(&snapshot);
        let gc_before = ctx.tagged_heap.gc_collections();
        let result = match cache::try_run_osr(&mut ctx, &f, 4, &snapshot, &[borrowed]) {
            Some(NativeRun::DeoptAt(resume)) if guard_fails => {
                assert_eq!(resume.pc, 4);
                assert_eq!(ctx.tagged_heap.gc_collections(), gc_before);
                assert_eq!(ctx.jit_root_stack_top, 0);
                let DeoptResume {
                    pc,
                    stack,
                    handlers,
                    binds,
                    spec_base,
                    cond_base,
                    ..
                } = *resume;
                // Only the captured operands can root payload during resumption.
                // Its first store invokes the collecting watcher.
                ctx.bc_buf.clear();
                Vm::from_context(&mut ctx)
                    .run_resumed_frame_latched(
                        &f,
                        Value::NIL,
                        pc,
                        &stack,
                        handlers,
                        &binds,
                        spec_base,
                        cond_base,
                        true,
                    )
                    .unwrap()
            }
            Some(NativeRun::Ok(bits)) if !guard_fails => Value::from_bits(bits),
            other => panic!("unexpected native outcome, guard_fails={guard_fails}: {other:?}"),
        };
        assert_eq!(
            result.as_utf8_str(),
            Some("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")
        );
        assert!(ctx.tagged_heap.gc_collections() >= gc_before + 3);
        if guard_fails {
            assert!(ctx.bc_buf.is_empty());
        } else {
            assert_eq!(ctx.bc_buf, snapshot);
            ctx.bc_buf.clear();
        }
        assert_eq!(ctx.jit_root_stack_top, 0);
        assert_eq!(ctx.specpdl.len(), borrowed);
        assert_eq!(ctx.jit_bind_stack, [outer]);
        assert_eq!(
            ctx.eval_str("osr-entry-writes").unwrap(),
            Value::make_int(3)
        );
        assert_eq!(ctx.eval_str("osr-entry-held").unwrap(), Value::make_int(17));
        ctx.jit_bind_stack.clear();
        ctx.unbind_to(outer);
    }
}

#[test]
fn osr_entry_guard_does_not_infer_a_type_from_a_patched_constant_prefix() {
    for patched in [false, true] {
        let mut ctx = Context::new();
        let (outer, borrowed) = install_bindings(&mut ctx);
        let f = effect_before_numeric_loop();
        if patched {
            f.jit_runtime().note_patched_prefix(1);
        }
        let snapshot = [Value::NIL, Value::make_int(2), Value::make_float(0.5)];
        ctx.bc_buf.extend_from_slice(&snapshot);
        let Some(NativeRun::DeoptAt(resume)) =
            cache::try_run_osr(&mut ctx, &f, 4, &snapshot, &[borrowed])
        else {
            panic!("float seed must reach a precise guard");
        };
        // An unpatched constant proves the entry type. The patched prefix is
        // masked for analysis, so its live value instead reaches the existing
        // numeric guard AFTER the header's observable store.
        assert_eq!(resume.pc, if patched { 8 } else { 4 });
        assert_eq!(
            ctx.eval_str("osr-entry-effect").unwrap(),
            Value::make_int(if patched { 42 } else { 0 }),
        );
        assert_eq!(ctx.jit_bind_stack, [outer]);
        assert_eq!(ctx.jit_root_stack_top, 0);
        ctx.bc_buf.clear();
        ctx.jit_bind_stack.clear();
        ctx.unbind_to(outer);
    }
}
