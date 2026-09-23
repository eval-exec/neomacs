//! Scratch allocation reuse for the baseline and MIR frontend builders.

use cranelift_frontend::FunctionBuilderContext;
use std::cell::RefCell;

thread_local! {
    // At most one finalized context is retained per compiling thread.
    static AVAILABLE: RefCell<Option<Box<FunctionBuilderContext>>> = const { RefCell::new(None) };
}

pub(super) fn take() -> Box<FunctionBuilderContext> {
    // Release the borrow before lowering: another compilation can take its own
    // context while this one is in use. During TLS teardown, allocate normally.
    AVAILABLE
        .try_with(|slot| slot.take())
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Return a context immediately after its FunctionBuilder::finalize succeeded.
/// A context abandoned by an error or panic must be dropped instead: Cranelift
/// only clears frontend state during finalize, and its clear method is private.
pub(super) fn recycle(context: Box<FunctionBuilderContext>) {
    // A nested compilation may already have returned a context. Keep only the
    // latest one and drop any displaced allocation outside the RefCell borrow.
    let _ = AVAILABLE.try_with(|slot| slot.replace(Some(context)));
}

#[cfg(test)]
mod tests {
    use super::super::{CompileError, Op, Value, lower_nullary_leaf};
    use super::*;

    fn compile_constant(value: i64) {
        let expected = Value::make_int(value);
        let leaf = lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[expected]).unwrap();
        assert_eq!(leaf.call_for_test(&[]), Some(expected.bits()));
    }

    fn available() -> bool {
        AVAILABLE.with(|slot| slot.borrow().is_some())
    }

    #[test]
    fn partial_lowering_failure_discards_frontend_state() {
        drop(take());
        compile_constant(17);
        assert!(available());
        // CFG analysis accepts this shape. The invalid symbol operand fails
        // after the baseline builder has created its blocks and variables.
        assert!(matches!(
            lower_nullary_leaf(&[Op::VarRef(0), Op::Return], &[Value::make_int(9)]),
            Err(CompileError::BadOperand)
        ));
        assert!(!available());
        compile_constant(23);
        assert!(available());
    }

    #[test]
    fn owned_context_does_not_hold_a_pool_borrow() {
        compile_constant(31);
        let outer = take();
        assert!(!available());
        compile_constant(37);
        assert!(available());
        // The outer context was finalized before it was taken, and never used
        // by another builder. Returning it can safely displace the inner one.
        recycle(outer);
        compile_constant(41);
    }

    #[test]
    fn panic_drops_a_partially_built_context() {
        use cranelift_codegen::ir::{Function, InstBuilder, types};
        use cranelift_frontend::FunctionBuilder;
        compile_constant(43);
        let result = std::panic::catch_unwind(|| {
            let mut context = take();
            let mut function = Function::new();
            let mut builder = FunctionBuilder::new(&mut function, &mut context);
            let block = builder.create_block();
            builder.switch_to_block(block);
            builder.ins().iconst(types::I64, 1);
            panic!("abandon partially built frontend state");
        });
        assert!(result.is_err());
        assert!(!available());
        compile_constant(47);
    }

    #[test]
    fn compiling_threads_have_independent_frontend_storage() {
        compile_constant(53);
        let outer = take();
        std::thread::spawn(|| {
            assert!(!available());
            compile_constant(59);
            assert!(available());
        })
        .join()
        .unwrap();
        assert!(!available());
        recycle(outer);
        compile_constant(61);
    }

    #[test]
    fn scratch_access_tolerates_an_already_destroyed_thread_local_pool() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        struct Probe(Arc<AtomicBool>);
        impl Drop for Probe {
            fn drop(&mut self) {
                let destroyed = AVAILABLE.try_with(|_| ()).is_err();
                let context = take();
                recycle(context);
                self.0.store(destroyed, Ordering::Relaxed);
            }
        }
        thread_local! {
            static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
        }
        let observed = Arc::new(AtomicBool::new(false));
        let in_thread = observed.clone();
        std::thread::spawn(move || {
            // Initialize this destructor first so it runs after AVAILABLE.
            PROBE.with(|slot| *slot.borrow_mut() = Some(Probe(in_thread)));
            compile_constant(71);
        })
        .join()
        .unwrap();
        assert!(observed.load(Ordering::Relaxed));
    }

    #[test]
    fn baseline_mir_and_object_builders_share_only_empty_state() {
        use super::super::{lower_leaf, lowering::lower_mir_pure};
        use crate::emacs_core::jit::{aot, mir};
        let ops = [Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return];
        let argument = Value::make_int(7);
        for increment in [1, 11, 101] {
            let constants = [Value::make_int(increment)];
            let function = mir::build_mir(&ops, &constants, 1).unwrap();
            drop(take());
            let object = aot::build_object_for_leaf(&function, "frontend_scratch_mir").unwrap();
            let baseline_object = aot::build_baseline_object_for_leaf(&ops, &constants, 1, None)
                .unwrap()
                .unwrap();
            let expected = Value::make_int(7 + increment).bits();
            assert_eq!(
                lower_leaf(&ops, &constants, 1)
                    .unwrap()
                    .call_for_test(&[argument]),
                Some(expected)
            );
            assert_eq!(
                lower_mir_pure(&function)
                    .unwrap()
                    .call_for_test(&[argument]),
                Some(expected)
            );
            compile_constant(67);
            assert_eq!(
                aot::build_object_for_leaf(&function, "frontend_scratch_mir").unwrap(),
                object
            );
            assert_eq!(
                aot::build_baseline_object_for_leaf(&ops, &constants, 1, None)
                    .unwrap()
                    .unwrap(),
                baseline_object
            );
        }
    }
}
