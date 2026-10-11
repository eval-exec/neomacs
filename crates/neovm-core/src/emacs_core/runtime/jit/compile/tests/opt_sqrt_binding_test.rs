//! Primitive selected binding/publication tests; not intrinsic-profit evidence.

use super::sqrt_binding::{binding_valid, is_actual_sqrt};
use super::*;
use crate::emacs_core::subr::{FixedMin1, SubrSpec};
use crate::emacs_core::symbol::FunctionEpochBump;

fn align_real_owner_clocks(a: &mut Context, b: &mut Context) -> u64 {
    let target = a.obarray.function_epoch().max(b.obarray.function_epoch());
    for ctx in [a, b] {
        while ctx.obarray.function_epoch() != target {
            ctx.obarray
                .invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
        }
    }
    target
}

#[test]
fn opt_sqrt_binding_equal_owner_clocks_still_read_cell_and_inplace_entry() {
    let mut first = Context::new();
    let symbol = intern("sqrt");
    let symbol_bits = Value::from_sym_id(symbol).bits() as u64;
    let original = first.obarray.symbol_function_id(symbol).unwrap();
    let mut second = Context::new();
    second
        .eval_str("(fset 'sqrt (lambda (x) (list 'changed x)))")
        .unwrap();
    let epoch = align_real_owner_clocks(&mut first, &mut second);
    assert_ne!(first.obarray.generation(), second.obarray.generation());
    assert_eq!(
        first.obarray.function_epoch(),
        second.obarray.function_epoch()
    );
    assert!(is_actual_sqrt(original));
    assert!(binding_valid(
        &first,
        symbol_bits,
        original.bits() as u64,
        epoch
    ));
    assert!(
        !binding_valid(&second, symbol_bits, original.bits() as u64, epoch),
        "same numeric epoch cannot replace CURRENT owner function-cell proof"
    );

    second.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sin_1,
        FixedMin1::One,
    ));
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
        !binding_valid(&first, symbol_bits, original.bits() as u64, epoch),
        "same static pointer now holds a different actual A1 function"
    );
    second.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sqrt_1,
        FixedMin1::One,
    ));
    assert!(binding_valid(
        &first,
        symbol_bits,
        original.bits() as u64,
        epoch
    ));
    first.setup_thread_locals();
}

#[test]
fn opt_sqrt_binding_shared_static_reader_declines_busy_writer_and_observes_rewrite() {
    let mut writer = Context::new();
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
        let mut reader = Context::new();
        reader
            .obarray
            .set_symbol_function_id(symbol, Value::from_bits(original_bits as usize));
        let epoch = reader.obarray.function_epoch();
        reply.send(None).unwrap();
        while let Ok(Probe::Check) = receive.recv() {
            let answer = binding_valid(&reader, symbol_bits, original_bits, epoch);
            reply.send(Some(answer)).unwrap();
        }
    });
    assert_eq!(result.recv().unwrap(), None);
    crate::tagged::value::with_static_subr_entry_write(|| {
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
