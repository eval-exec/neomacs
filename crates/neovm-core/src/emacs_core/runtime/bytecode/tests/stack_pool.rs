use super::*;

struct StackReturnOverride(Option<bool>);

impl StackReturnOverride {
    fn new(enabled: bool) -> Self {
        Self(VM_STACK_RETURN_TEST_OVERRIDE.replace(Some(enabled)))
    }
}

impl Drop for StackReturnOverride {
    fn drop(&mut self) {
        VM_STACK_RETURN_TEST_OVERRIDE.set(self.0);
    }
}

fn populated_stacks(function: &ByteCodeFunction) -> InterpreterStacks {
    let mut frames = Vec::with_capacity(8);
    frames.push(InterpreterFrame {
        function: InterpreterFunction::new(function),
        code: ActiveCodeView::of(function),
        frame_base: 0,
        #[cfg(feature = "jit")]
        resume: InterpreterResumePoint::new(0, false),
        #[cfg(not(feature = "jit"))]
        pc: 0,
        cleanup: InterpreterFrameCleanup {
            condition_stack_base: 0,
            specpdl_base: 0,
        },
        caller_return: InterpreterCallerReturn::ENTRY,
        #[cfg(debug_assertions)]
        entry_lexenv: Value::NIL,
    });
    let mut suspended = Vec::with_capacity(4);
    suspended.push(SuspendedInterpreterFrameAux {
        depth: InterpreterDriverDepth::ROOT,
        state: InterpreterFrameAux::new(HandlerStack::new(), (0..16).collect()),
    });
    InterpreterStacks { frames, suspended }
}

#[test]
fn stack_return_knob_defaults_off_and_accepts_explicit_on() {
    assert!(!parse_vm_stack_return_knob(None));
    for value in ["", "off", "0", "false", "no", "invalid"] {
        assert!(!parse_vm_stack_return_knob(Some(value)), "{value}");
    }
    for value in ["1", "on", "true", "yes", " ON "] {
        assert!(parse_vm_stack_return_knob(Some(value)), "{value}");
    }
}

#[test]
fn stack_return_empties_and_reuses_both_backing_stores() {
    let function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    for enabled in [false, true] {
        let _override = StackReturnOverride::new(enabled);
        let mut pool = InterpreterStackPool::new();
        let stacks = populated_stacks(&function);
        let frame_storage = stacks.frames.as_ptr();
        let suspended_storage = stacks.suspended.as_ptr();
        let frame_capacity = stacks.frames.capacity();
        let suspended_capacity = stacks.suspended.capacity();

        pool.give_back(stacks);
        assert_eq!(pool.free.len(), 1);
        let mut reused = pool.take();
        assert!(reused.frames.is_empty());
        assert!(reused.suspended.is_empty());
        assert_eq!(reused.frames.as_ptr(), frame_storage);
        assert_eq!(reused.suspended.as_ptr(), suspended_storage);
        assert_eq!(reused.frames.capacity(), frame_capacity);
        assert_eq!(reused.suspended.capacity(), suspended_capacity);
        assert!(pool.free.is_empty());

        let mut next = populated_stacks(&function);
        reused.frames.push(next.frames.pop().unwrap());
        reused.suspended.push(next.suspended.pop().unwrap());
        pool.give_back(reused);
        let reused = pool.take();
        assert!(reused.frames.is_empty());
        assert!(reused.suspended.is_empty());
        assert_eq!(reused.frames.as_ptr(), frame_storage);
        assert_eq!(reused.suspended.as_ptr(), suspended_storage);
    }
}

#[test]
fn stack_return_overflow_preserves_retained_stores() {
    let function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    for enabled in [false, true] {
        let _override = StackReturnOverride::new(enabled);
        let mut pool = InterpreterStackPool::new();
        for _ in 0..InterpreterStackPool::MAX_FREE {
            pool.give_back(populated_stacks(&function));
        }
        let retained: Vec<_> = pool
            .free
            .iter()
            .map(|stacks| (stacks.frames.as_ptr(), stacks.suspended.as_ptr()))
            .collect();

        pool.give_back(populated_stacks(&function));

        assert_eq!(pool.free.len(), InterpreterStackPool::MAX_FREE);
        let after_overflow: Vec<_> = pool
            .free
            .iter()
            .map(|stacks| {
                assert!(stacks.frames.is_empty());
                assert!(stacks.suspended.is_empty());
                (stacks.frames.as_ptr(), stacks.suspended.as_ptr())
            })
            .collect();
        assert_eq!(after_overflow, retained);
        for &(frames, suspended) in retained.iter().rev() {
            let reused = pool.take();
            assert_eq!(reused.frames.as_ptr(), frames);
            assert_eq!(reused.suspended.as_ptr(), suspended);
        }
        assert!(pool.free.is_empty());
    }
}

#[test]
fn stack_return_reuses_storage_after_an_unhandled_signal() {
    crate::test_utils::init_test_tracing();
    for enabled in [false, true] {
        let _override = StackReturnOverride::new(enabled);
        let mut context = crate::emacs_core::eval::Context::new_minimal_vm_harness();
        let mut failing = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        failing.constants = vec![Value::symbol("vm-stack-return-undefined-function")].into();
        failing.ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
        failing.max_stack = 1;
        let mut succeeding = ByteCodeFunction::new(LambdaParams::simple(vec![]));
        succeeding.constants = vec![Value::fixnum(42)].into();
        succeeding.ops = vec![Op::Constant(0), Op::Return];
        succeeding.max_stack = 1;

        let mut vm = Vm::from_context(&mut context);
        #[cfg(feature = "jit")]
        vm.force_interpreter_only_for_test();
        assert!(matches!(vm.execute(&failing, vec![]), Err(Flow::Signal(_))));
        let returned = &vm.ctx.interpreter_stacks.free;
        assert_eq!(returned.len(), 1);
        assert!(returned[0].frames.is_empty());
        assert!(returned[0].suspended.is_empty());
        let frame_storage = returned[0].frames.as_ptr();

        assert_eq!(
            vm.execute(&succeeding, vec![])
                .expect("the next entry must reuse clean storage"),
            Value::fixnum(42)
        );
        let returned = &vm.ctx.interpreter_stacks.free;
        assert_eq!(returned.len(), 1);
        assert!(returned[0].frames.is_empty());
        assert!(returned[0].suspended.is_empty());
        assert_eq!(returned[0].frames.as_ptr(), frame_storage);
        assert!(vm.ctx.bc_buf.is_empty());
        assert!(vm.ctx.bc_frames.is_empty());
    }
}
