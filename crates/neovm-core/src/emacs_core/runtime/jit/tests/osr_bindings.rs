use super::*;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::{ConditionFrame, Context, ResumeTarget};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::{cache, force_osr_for_test};
use crate::emacs_core::value::LambdaParams;
use std::sync::atomic::Ordering;

fn binding_sum(nested: bool) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("n")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![
        Value::make_int(0),
        Value::make_int(3),
        Value::symbol("osr-bound"),
        Value::make_int(7),
    ]
    .into();
    f.ops = vec![
        Op::Constant(1),
        Op::VarBind(2),
        Op::Constant(0),
        Op::Constant(0), // n sum i
        Op::StackRef(0),
        Op::StackRef(3),
        Op::Lss,
        Op::GotoIfNil(0),
    ];
    if nested {
        f.ops.extend([Op::Constant(3), Op::VarBind(2)]);
    }
    f.ops
        .extend([Op::StackRef(1), Op::VarRef(2), Op::Add, Op::StackSet(2)]);
    if nested {
        f.ops.extend([
            Op::Unbind(1),
            Op::StackRef(1),
            Op::VarRef(2),
            Op::Add,
            Op::StackSet(2),
        ]);
    }
    f.ops
        .extend([Op::StackRef(0), Op::Add1, Op::StackSet(1), Op::Goto(4)]);
    f.ops[7] = Op::GotoIfNil(f.ops.len() as u32);
    f.ops.extend([Op::StackRef(1), Op::Unbind(1), Op::Return]);
    f.max_stack = 16;
    f
}

#[test]
fn osr_binding_transfer_preserves_nested_binds_and_outer_native_segment() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq osr-bound 17 osr-outer 23)").unwrap();
    let outer_base = ctx.specpdl.len();
    ctx.try_specbind(intern("osr-outer"), Value::make_int(29))
        .unwrap();
    ctx.jit_bind_stack.push(outer_base);
    let spec_depth = ctx.specpdl.len();
    for nested in [false, true] {
        for osr in [false, true] {
            force_osr_for_test(osr);
            let mut f = binding_sum(nested);
            f.seal_hand_assembled_ops();
            f.jit_runtime().set_hot_for_test();
            let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
            let value = Vm::from_context(&mut ctx)
                .execute(&f, vec![Value::make_int(2000)])
                .unwrap();
            assert_eq!(value, Value::make_int(if nested { 20000 } else { 6000 }));
            assert_eq!(
                cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) - before,
                u64::from(osr)
            );
            assert_eq!(ctx.specpdl.len(), spec_depth);
            assert_eq!(ctx.jit_bind_stack, [outer_base]);
            assert_eq!(
                ctx.eval_str("(list osr-bound osr-outer)")
                    .unwrap()
                    .to_string(),
                "(17 29)"
            );
        }
    }
    force_osr_for_test(false);
    ctx.jit_bind_stack.clear();
    ctx.unbind_to(outer_base);
    assert_eq!(ctx.eval_str("osr-outer").unwrap(), Value::make_int(23));
}

#[test]
fn osr_binding_entry_rejects_wrong_snapshot_without_mutating_either_stack() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq osr-bound 17 osr-outer 23)").unwrap();
    let outer_base = ctx.specpdl.len();
    ctx.try_specbind(intern("osr-outer"), Value::make_int(29))
        .unwrap();
    ctx.jit_bind_stack.push(outer_base);
    let bind_base = ctx.specpdl.len();
    ctx.try_specbind(intern("osr-bound"), Value::make_int(3))
        .unwrap();
    let spec_depth = ctx.specpdl.len();
    let mut f = binding_sum(false);
    f.seal_hand_assembled_ops();
    let stack = [
        Value::make_int(2000),
        Value::make_int(768),
        Value::make_int(256),
    ];
    for (operands, binds) in [
        (&stack[..], &[][..]),
        (&stack[..2], &[bind_base][..]),
        (&stack[..], &[spec_depth][..]),
    ] {
        let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
        assert!(cache::try_run_osr(&mut ctx, &f, 4, operands, binds).is_none());
        assert_eq!(cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed), before);
        assert_eq!(ctx.jit_bind_stack, [outer_base]);
        assert_eq!(ctx.specpdl.len(), spec_depth);
        assert_eq!(ctx.eval_str("osr-bound").unwrap(), Value::make_int(3));
    }
    // Binding support does not admit nonlexical frames or save records.
    for save_record in [false, true] {
        let mut ineligible = binding_sum(false);
        if save_record {
            let at = ineligible.ops.len() - 1;
            ineligible.ops.insert(at, Op::SaveExcursion);
            ineligible.ops.insert(at + 1, Op::Unbind(1));
        } else {
            ineligible.lexical = false;
        }
        ineligible.seal_hand_assembled_ops();
        assert!(cache::try_run_osr(&mut ctx, &ineligible, 4, &stack, &[bind_base]).is_none());
        assert_eq!(ctx.jit_bind_stack, [outer_base]);
        assert_eq!(ctx.specpdl.len(), spec_depth);
    }
    ctx.jit_bind_stack.clear();
    ctx.unbind_to(outer_base);
}

#[test]
fn osr_binding_deopt_adopts_new_inner_binding_without_replaying_effects() {
    // The OSR header has ONE live bind. At overflow the native body has TWO.
    // Resuming with the old VM binding stack would unbind the wrong variable.
    let mk = || {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![intern("start"), intern("n")],
            optional: vec![],
            rest: None,
        });
        f.lexical = true;
        f.constants = vec![
            Value::make_int(0),
            Value::symbol("osr-effects"),
            Value::symbol("osr-bound"),
            Value::make_int(3),
            Value::make_int(7),
        ]
        .into();
        f.ops = vec![
            Op::Constant(3),
            Op::VarBind(2),
            Op::Constant(0),
            Op::StackRef(2), // start n i x
            Op::StackRef(1),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(25),
            Op::VarRef(1),
            Op::Add1,
            Op::VarSet(1),
            Op::Constant(4),
            Op::VarBind(2),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1), // overflow with inner binding live
            Op::Unbind(1),
            Op::VarRef(2),
            Op::Constant(3),
            Op::Eqlsign,
            Op::Pop, // outer binding restored
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::Goto(4),
            Op::StackRef(1),
            Op::VarRef(2),
            Op::Add,
            Op::Unbind(1),
            Op::Return,
        ];
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    };
    let mut ctx = Context::new();
    ctx.eval_str("(setq osr-outer 23)").unwrap();
    let outer_base = ctx.specpdl.len();
    ctx.try_specbind(intern("osr-outer"), Value::make_int(29))
        .unwrap();
    ctx.jit_bind_stack.push(outer_base);
    for osr in [false, true] {
        ctx.eval_str("(setq osr-bound 17 osr-effects 0)").unwrap();
        force_osr_for_test(osr);
        let f = mk();
        f.jit_runtime().set_hot_for_test();
        let spec_depth = ctx.specpdl.len();
        let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
        let result = Vm::from_context(&mut ctx)
            .execute(
                &f,
                vec![
                    Value::make_int(Value::MOST_POSITIVE_FIXNUM - 1500),
                    Value::make_int(2000),
                ],
            )
            .unwrap();
        assert_eq!(result, Value::make_int(2003));
        assert_eq!(ctx.eval_str("osr-effects").unwrap(), Value::make_int(2000));
        assert_eq!(ctx.eval_str("osr-bound").unwrap(), Value::make_int(17));
        assert_eq!(ctx.specpdl.len(), spec_depth);
        assert_eq!(ctx.jit_bind_stack, [outer_base]);
        assert_eq!(
            cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) - before,
            u64::from(osr),
            "precise resume must latch against another transfer"
        );
    }
    force_osr_for_test(false);
    ctx.jit_bind_stack.clear();
    ctx.unbind_to(outer_base);
}

#[test]
fn osr_binding_unlet_watchers_collect_and_preserve_cleanup_exit_semantics() {
    for mode in ["collect", "signal", "throw"] {
        for osr in [false, true] {
            let mut ctx = Context::new();
            ctx.eval_str(&format!(
                r#"(progn
                (setq osr-bound 17 osr-watch-log nil osr-watch-mode '{mode})
                (fset 'osr-binding-watcher
                    (lambda (_symbol value operation _where)
                        (setq osr-watch-log (cons (list operation value) osr-watch-log))
                        (if (eq operation 'unlet)
                            (progn (garbage-collect)
                                (if (eq osr-watch-mode 'signal)
                                    (signal 'error '("restore"))
                                  (if (eq osr-watch-mode 'throw)
                                      (throw 'osr-watcher 'cleanup-throw)))))))
                (add-variable-watcher 'osr-bound 'osr-binding-watcher))"#
            ))
            .unwrap();
            let mut f = binding_sum(false);
            f.ops.truncate(f.ops.len() - 3);
            let make_string = f.add_constant(Value::symbol("make-string"));
            let length = f.add_constant(Value::make_int(40));
            let ch = f.add_constant(Value::make_int(120));
            f.ops.extend([
                Op::Constant(make_string),
                Op::Constant(length),
                Op::Constant(ch),
                Op::Call(2),
                Op::Unbind(1),
                Op::Return,
            ]);
            f.seal_hand_assembled_ops();
            f.jit_runtime().set_hot_for_test();
            force_osr_for_test(osr);
            let spec_depth = ctx.specpdl.len();
            let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
            let gc_before = ctx.tagged_heap.gc_collections();
            if mode == "throw" {
                ctx.push_condition_frame(ConditionFrame::Catch {
                    tag: Value::symbol("osr-watcher"),
                    resume: ResumeTarget::InterpreterCatch,
                });
            }
            let conditions = ctx.condition_stack_len();
            let result = Vm::from_context(&mut ctx).execute(&f, vec![Value::make_int(2000)]);
            match (mode, result) {
                ("collect", Ok(value)) => {
                    assert_eq!(value.as_utf8_str(), Some("x".repeat(40).as_str()))
                }
                ("signal", Err(Flow::Signal(signal))) => assert_eq!(signal.symbol, intern("error")),
                ("throw", Err(Flow::Throw(flow))) => {
                    assert_eq!(flow.value, Value::symbol("cleanup-throw"))
                }
                (_, result) => panic!("mode={mode}, osr={osr}: {result:?}"),
            }
            assert!(ctx.tagged_heap.gc_collections() > gc_before);
            assert_eq!(
                cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) - before,
                u64::from(osr)
            );
            assert_eq!(ctx.specpdl.len(), spec_depth);
            assert_eq!(ctx.condition_stack_len(), conditions);
            assert!(ctx.jit_bind_stack.is_empty());
            // GNU leaves the inner value installed if the unlet watcher exits,
            // though it has already seen the proposed outer value 17.
            assert_eq!(
                ctx.eval_str("osr-bound").unwrap(),
                Value::make_int(if mode == "collect" { 17 } else { 3 })
            );
            assert_eq!(
                crate::emacs_core::print::print_value(&ctx.eval_str("osr-watch-log").unwrap()),
                "((unlet 17) (let 3))"
            );
        }
    }
    force_osr_for_test(false);
}

#[test]
fn osr_binding_frame_exit_unwinds_inherited_and_new_binds_for_each_outcome() {
    for exit in ["return", "signal", "throw"] {
        for osr in [false, true] {
            let mut ctx = Context::new();
            ctx.eval_str("(setq osr-bound 17 osr-outer 23)").unwrap();
            let outer_base = ctx.specpdl.len();
            ctx.try_specbind(intern("osr-outer"), Value::make_int(29))
                .unwrap();
            ctx.jit_bind_stack.push(outer_base);
            let spec_depth = ctx.specpdl.len();
            let mut f = binding_sum(false);
            f.ops.truncate(f.ops.len() - 3);
            f.ops.extend([Op::Constant(3), Op::VarBind(2)]);
            if exit == "return" {
                f.ops.push(Op::StackRef(1));
            } else {
                let callee = f.add_constant(Value::symbol(exit));
                let tag = f.add_constant(Value::symbol(if exit == "signal" {
                    "error"
                } else {
                    "osr-exit"
                }));
                let payload = f.add_constant(Value::list(vec![Value::string("osr-exit-payload")]));
                f.ops.extend([
                    Op::Constant(callee),
                    Op::Constant(tag),
                    Op::Constant(payload),
                    Op::Call(2),
                ]);
            }
            // No explicit Unbind: the native exit owns the new inner binding,
            // and the suspended VM still owns the inherited outer binding.
            f.ops.push(Op::Return);
            f.seal_hand_assembled_ops();
            f.jit_runtime().set_hot_for_test();
            force_osr_for_test(osr);
            let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
            if exit == "throw" {
                ctx.push_condition_frame(ConditionFrame::Catch {
                    tag: Value::symbol("osr-exit"),
                    resume: ResumeTarget::InterpreterCatch,
                });
            }
            let conditions = ctx.condition_stack_len();
            let result = Vm::from_context(&mut ctx).execute(&f, vec![Value::make_int(2000)]);
            match (exit, result) {
                ("return", Ok(value)) => assert_eq!(value, Value::make_int(6000)),
                ("signal", Err(Flow::Signal(signal))) => assert_eq!(signal.symbol, intern("error")),
                ("throw", Err(Flow::Throw(flow))) => {
                    assert_eq!(flow.tag, Value::symbol("osr-exit"))
                }
                (_, result) => panic!("exit={exit}, osr={osr}: {result:?}"),
            }
            assert_eq!(
                cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) - before,
                u64::from(osr)
            );
            assert_eq!(ctx.specpdl.len(), spec_depth);
            assert_eq!(ctx.condition_stack_len(), conditions);
            assert_eq!(ctx.jit_bind_stack, [outer_base]);
            assert_eq!(ctx.eval_str("osr-bound").unwrap(), Value::make_int(17));
            assert_eq!(ctx.eval_str("osr-outer").unwrap(), Value::make_int(29));
            ctx.jit_bind_stack.clear();
            ctx.unbind_to(outer_base);
        }
    }
    force_osr_for_test(false);
}

#[test]
fn osr_binding_transfer_from_iterative_child_preserves_callers_binding() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq osr-bound 17 osr-outer 23)").unwrap();
    let child = Value::make_bytecode(binding_sum(true));
    child
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_hot_for_test();
    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    caller.lexical = true;
    caller.constants = vec![
        child,
        Value::make_int(2000),
        Value::symbol("osr-outer"),
        Value::make_int(99),
    ]
    .into();
    caller.ops = vec![
        Op::Constant(3),
        Op::VarBind(2),
        Op::Constant(0),
        Op::Constant(1),
        Op::Call(1),
        Op::VarRef(2),
        Op::Add,
        Op::Unbind(1),
        Op::Return,
    ];
    caller.max_stack = 8;
    caller.seal_hand_assembled_ops();
    force_osr_for_test(true);
    let before = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
    let spec_depth = ctx.specpdl.len();
    let result = {
        let mut vm = Vm::from_context(&mut ctx);
        // Force Bcall to install a bytecode child, while its hot loop remains
        // eligible for OSR. There is no loop in the entry/caller frame.
        vm.force_interpreter_only_for_test();
        vm.execute(&caller, vec![]).unwrap()
    };
    assert_eq!(result, Value::make_int(20099));
    assert_eq!(
        cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed) - before,
        1
    );
    assert_eq!(ctx.specpdl.len(), spec_depth);
    assert!(ctx.jit_bind_stack.is_empty());
    assert_eq!(ctx.eval_str("osr-bound").unwrap(), Value::make_int(17));
    assert_eq!(ctx.eval_str("osr-outer").unwrap(), Value::make_int(23));
    force_osr_for_test(false);
}
