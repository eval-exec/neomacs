//! Actual minimal World/collector inputs; no bootstrap or setup collection.
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::gc::{Gen1Admission, Gen1AdmissionError};

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
        let (collector, keeper) = match (expected, Gen1Admission::check(collector)) {
            (ActualMode::Gen0, Err(failure)) => {
                assert_eq!(failure.reason, Gen1AdmissionError::GenerationalRequired);
                eprintln!("NMJIT_N2_GEN0_TYPED_REFUSAL_OWNERSHIP_RETURNED");
                (failure.collector, keeper)
            }
            (ActualMode::Gen1, Ok(admission)) => {
                let era = super::super::ParallelHeapEra::admit(admission, keeper)
                    .expect("production era consumes actual GEN1 admission");
                assert_eq!(era.heap_identity(), heap);
                let permit = era
                    .register(1, 4)
                    .expect("real production registration requires retained GEN1 evidence");
                drop(permit);
                let inputs = era
                    .finish()
                    .expect("cancelled permit returns production inputs");
                eprintln!("NMJIT_N2_GEN1_ACTUAL_COLLECTOR_CAPABILITY");
                inputs
            }
            (mode, result) => panic!("actual heap mode mismatch: {mode:?}: {result:?}"),
        };
        assert_eq!(collector.heap_identity(), heap);
        assert_eq!(keeper.parallel_heap_identity(), heap);
        assert!(keeper.held_here());
        // Inputs remain usable, not merely pointer-identical. Re-admission
        // uses the nominal proof-only route, including under GEN0.
        let era = crate::tagged::gc::ProofEra::admit(collector, keeper)
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
