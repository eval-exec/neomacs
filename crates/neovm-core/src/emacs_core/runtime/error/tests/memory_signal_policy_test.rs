//! OOM descriptor validation follows GNU Fget/CHECK_SYMBOL with owning policy.
use super::{
    MemorySignalBinding, MemorySignalBindingError, SignalDelivery, collect_in_flight_flow_gc_roots,
    flow_from_eval_error, make_signal_binding_value, memory_signal_from_binding_value,
};
use crate::emacs_core::print::map_flow;
use crate::emacs_core::{Context, Value, alloc::AllocationFailure};
use crate::tagged::SymbolMode;

#[test]
fn issue554_memory_signal_binding_requires_explicit_symbol_policy() {
    let mut context = Context::new();
    let original = context
        .eval_str("(cons (position-symbol 'error 17) '(policy-payload . 99))")
        .unwrap();
    assert!(matches!(
        MemorySignalBinding::try_from((original, SymbolMode::Bare)),
        Err(MemorySignalBindingError::NotSymbol)
    ));
    let binding = MemorySignalBinding::try_from((original, SymbolMode::Positioned)).unwrap();
    assert_eq!(binding.original().bits(), original.bits());
    for mode in [SymbolMode::Bare, SymbolMode::Positioned] {
        assert!(matches!(
            MemorySignalBinding::try_from((Value::fixnum(42), mode)),
            Err(MemorySignalBindingError::NotCons)
        ));
        let canonical = Value::cons(Value::symbol("error"), Value::fixnum(99));
        assert!(MemorySignalBinding::try_from((canonical, mode)).is_ok());
    }
}

#[test]
fn issue554_positioned_memory_descriptor_roundtrip_keeps_original_and_roots() {
    let mut context = Context::new();
    let original = context
        .eval_str("(cons (position-symbol 'error 17) (cons (vector 7) 99))")
        .unwrap();
    let condition = original.cons_car();
    let tail = original.cons_cdr();
    let vector = tail.cons_car();
    let flow = memory_signal_from_binding_value(original, SymbolMode::Positioned);
    let original_error = map_flow(flow);
    let error = original_error.clone();
    drop(original_error);
    let mut roots = Vec::new();
    collect_in_flight_flow_gc_roots(&mut roots, context.tagged_heap.identity());
    assert!(roots.iter().any(|root| root.bits() == original.bits()));
    context.gc_collect_exact();
    for value in [original, condition, tail, vector] {
        assert!(context.tagged_heap.owns_heap_value_for_test(value));
    }
    let back = flow_from_eval_error(error);
    let signal = back.as_signal().unwrap();
    assert_eq!(signal.symbol_name(), "error");
    assert_eq!(signal.raw_data.map(|value| value.bits()), Some(tail.bits()));
    assert_eq!(make_signal_binding_value(signal).bits(), original.bits());
    let SignalDelivery::MemoryExhausted(binding) = signal.delivery() else {
        panic!("positioned condition must retain OOM delivery");
    };
    assert_eq!(binding.original().bits(), original.bits());
}

#[test]
fn issue554_allocation_failure_reads_live_context_symbol_policy() {
    for enabled in [false, true] {
        let mut context = Context::new();
        let original = context
            .eval_str("(setq memory-signal-data (cons (position-symbol 'error 17) '(policy-payload . 99)))")
            .unwrap();
        context
            .eval_str(if enabled {
                "(setq symbols-with-pos-enabled t)"
            } else {
                "(setq symbols-with-pos-enabled nil)"
            })
            .unwrap();
        let flow = AllocationFailure::NullAllocation.into_flow_in_context(&context);
        let signal = flow.as_signal().unwrap();
        if enabled {
            assert_eq!(signal.symbol_name(), "error");
            assert_eq!(
                signal.raw_data.map(|value| value.bits()),
                Some(original.cons_cdr().bits())
            );
            assert_eq!(make_signal_binding_value(signal).bits(), original.bits());
            assert!(matches!(
                signal.delivery(),
                SignalDelivery::MemoryExhausted(_)
            ));
        } else {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data[0].bits(), Value::symbol("symbolp").bits());
            assert_eq!(signal.data[1].bits(), original.cons_car().bits());
            assert!(matches!(signal.delivery(), SignalDelivery::Ordinary));
        }
    }
}

#[test]
fn issue554_noncons_memory_descriptor_keeps_ordinary_raw_payload_in_both_modes() {
    let _context = Context::new();
    for mode in [SymbolMode::Bare, SymbolMode::Positioned] {
        let value = Value::fixnum(42);
        let flow = memory_signal_from_binding_value(value, mode);
        let signal = flow.as_signal().unwrap();
        assert_eq!(signal.symbol_name(), "error");
        assert_eq!(
            signal.raw_data.map(|value| value.bits()),
            Some(value.bits())
        );
        assert!(matches!(signal.delivery(), SignalDelivery::Ordinary));
    }
}
