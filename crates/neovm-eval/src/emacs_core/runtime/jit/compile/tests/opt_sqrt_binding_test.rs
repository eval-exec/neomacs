//! Primitive selected binding/publication tests; not intrinsic-profit evidence.

use super::sqrt_binding::{binding_valid, is_actual_sqrt};
use super::*;
#[doc(hidden)]
pub use crate::emacs_core::subr::{FixedMin1, SubrSpec};
use neovm_value::symbol::FunctionEpochBump;

fn align_real_owner_clocks(ctx: &mut Context, target: u64) {
    while ctx.obarray.function_epoch() != target {
        ctx.obarray
            .invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
    }
}

#[test]
fn opt_sqrt_binding_equal_owner_clocks_still_read_cell_and_inplace_entry() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let symbol = intern("sqrt");
    let symbol_bits = Value::from_sym_id(symbol).bits() as u64;
    let original = first.obarray.symbol_function_id(symbol).unwrap();
    let first_epoch = first.obarray.function_epoch();
    let parked_first = first.park_for_test();
    let mut second = Context::new(crate::test_utils::unit_runtime());
    second
        .eval_str("(fset 'sqrt (lambda (x) (list 'changed x)))")
        .unwrap();
    let epoch = first_epoch.max(second.obarray.function_epoch());
    align_real_owner_clocks(&mut second, epoch);
    let second_generation = second.obarray.generation();
    let second_epoch = second.obarray.function_epoch();
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first owner");
    align_real_owner_clocks(&mut first, epoch);
    assert_ne!(first.obarray.generation(), second_generation);
    assert_eq!(first.obarray.function_epoch(), second_epoch);
    assert!(is_actual_sqrt(original));
    assert!(unsafe { binding_valid(&first, symbol_bits, original.bits() as u64, epoch) });
    let parked_first = first.park_for_test();
    let mut second = parked_second
        .resume_for_test()
        .expect("resume second owner");
    assert!(
        !unsafe { binding_valid(&second, symbol_bits, original.bits() as u64, epoch) },
        "same numeric epoch cannot replace CURRENT owner function-cell proof"
    );

    second.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sin_1,
        FixedMin1::One,
    ));
    let parked_second = second.park_for_test();
    let first = parked_first.resume_for_test().expect("resume first owner");
    assert_eq!(
        first.obarray.function_epoch(),
        epoch,
        "another Context's registration does not move this owner clock"
    );
    assert_eq!(
        first.obarray.symbol_function_id(symbol).unwrap().bits(),
        original.bits()
    );
    assert!(
        !unsafe { binding_valid(&first, symbol_bits, original.bits() as u64, epoch) },
        "same static pointer now holds a different actual A1 function"
    );
    let parked_first = first.park_for_test();
    let mut second = parked_second
        .resume_for_test()
        .expect("resume second owner");
    second.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sqrt_1,
        FixedMin1::One,
    ));
    drop(second);
    let mut first = parked_first.resume_for_test().expect("resume first owner");
    assert!(unsafe { binding_valid(&first, symbol_bits, original.bits() as u64, epoch) });
    first.setup_thread_locals();
}

#[test]
fn opt_sqrt_binding_shared_static_reader_declines_busy_writer_and_observes_rewrite() {
    let mut writer = Context::new(crate::test_utils::unit_runtime());
    let symbol = intern("sqrt");
    let symbol_bits = Value::from_sym_id(symbol).bits() as u64;
    let original_bits = writer.obarray.symbol_function_id(symbol).unwrap().bits() as u64;
    // Only opaque words cross threads. The retained Subr object is production
    // leaked-static storage, not a GC object or a Context borrow. The selected
    // helper's actual-field read and production registration writer use the
    // same process-wide lock; no Lisp TLS cache is introduced by this test.
    enum Probe {
        Check,
        Stop,
    }
    let (send, receive) = std::sync::mpsc::channel::<Probe>();
    let (reply, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut reader = Context::new(crate::test_utils::unit_runtime());
        reader.obarray.set_symbol_function_id(symbol, unsafe {
            /* SAFETY: inverse of Value::bits on the symbol's function value. */
            Value::from_bits(original_bits as usize)
        });
        let epoch = reader.obarray.function_epoch();
        reply.send(None).unwrap();
        while let Ok(Probe::Check) = receive.recv() {
            let answer = unsafe { binding_valid(&reader, symbol_bits, original_bits, epoch) };
            reply.send(Some(answer)).unwrap();
        }
    });
    assert_eq!(result.recv().unwrap(), None);
    neovm_value::tagged::value::with_static_subr_entry_write(|| {
        send.send(Probe::Check).unwrap();
        assert_eq!(
            result.recv().unwrap(),
            Some(false),
            "try_read misses before any plain metadata fields are inspected"
        );
    });
    send.send(Probe::Check).unwrap();
    assert_eq!(result.recv().unwrap(), Some(true));
    writer.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sin_1,
        FixedMin1::One,
    ));
    send.send(Probe::Check).unwrap();
    assert_eq!(
        result.recv().unwrap(),
        Some(false),
        "writer changes actual fields, while reader's owner clock stays fixed"
    );
    writer.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sqrt_1,
        FixedMin1::One,
    ));
    send.send(Probe::Check).unwrap();
    assert_eq!(result.recv().unwrap(), Some(true));
    send.send(Probe::Stop).unwrap();
    worker.join().unwrap();
    writer.setup_thread_locals();
}
