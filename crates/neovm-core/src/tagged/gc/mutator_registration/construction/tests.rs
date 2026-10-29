//! Actual minimal World/collector inputs; no bootstrap or setup collection.
use super::*;
use crate::emacs_core::eval::Context;

#[derive(Debug)]
enum ActualMode {
    Gen0,
    Gen1,
}

#[test]
fn nmjit_n2_actual_constructor_checks_mode_and_returns_complete_ownership() {
    Context::with_test_symbol_era(|era, _| {
        let (mut collector, keeper) = era
            .finish()
            .expect("empty fixture ends before admission probe");
        let heap = collector.heap_identity();
        let expected = match collector.registration_heap_mut().generational_enabled() {
            false => ActualMode::Gen0,
            true => ActualMode::Gen1,
        };
        let (collector, keeper) = match (expected, Gen1Admission::check(collector, keeper)) {
            (ActualMode::Gen0, Err(failure)) => {
                assert_eq!(failure.reason, Gen1AdmissionError::GenerationalRequired);
                eprintln!("NMJIT_N2_GEN0_TYPED_REFUSAL_OWNERSHIP_RETURNED");
                (failure.collector, failure.keeper)
            }
            (ActualMode::Gen1, Ok(admission)) => {
                let (checked, _witness) = admission.into_checked();
                eprintln!("NMJIT_N2_GEN1_ACTUAL_COLLECTOR_CAPABILITY");
                checked.into_parts()
            }
            (mode, result) => panic!("actual heap mode mismatch: {mode:?}: {result:?}"),
        };
        assert_eq!(collector.heap_identity(), heap);
        assert_eq!(keeper.parallel_heap_identity(), heap);
        assert!(keeper.held_here());
        // Inputs remain usable, not merely pointer-identical. This legacy
        // fixture route will become the explicit ProofEra route in N2.b.
        let era = super::super::ParallelHeapEra::admit(collector, keeper)
            .expect("complete ownership permits fixture re-admission");
        let (collector, keeper) = era.finish().expect("re-admitted fixture ends");
        drop(collector);
        drop(keeper);
    });
}

#[test]
fn nmjit_n2_proof_inputs_preserve_collector_mode_without_production_capability() {
    Context::with_test_symbol_era(|era, _| {
        let (mut collector, keeper) = era.finish().expect("empty proof fixture ends");
        let heap = collector.heap_identity();
        let mode = collector.registration_heap_mut().generational_enabled();
        let proof = ProofAdmission::check(collector, keeper).expect("explicit proof admission");
        let (checked, _proof_witness) = proof.into_checked();
        let (mut collector, keeper) = checked.into_parts();
        assert_eq!(collector.heap_identity(), heap);
        assert_eq!(
            collector.registration_heap_mut().generational_enabled(),
            mode
        );
        drop(collector);
        drop(keeper);
    });
}
