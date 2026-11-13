//! `#[test]` home for the four B8 weak-table bodies.
//!
//! The bodies stay in neovm-value. This module implements [`WeakTableHost`]
//! for [`Context`] and constructs that host with the unit runtime.

use neovm_value::error::EvalError;
use neovm_value::tagged::gc::{AllStopped, QuiescentCollector, RendezvousError, TaggedHeap};
use neovm_value::tagged::header::MarkerObj;
use neovm_value::test_support::authority::ProofEra;
use neovm_value::test_support::{
    WeakAdmit, WeakProjection, WeakTableHost,
    a6_ht_b8_cross_table_fixpoint_and_unrooted_cycles as cross_table_body,
    a6_ht_b8_four_weaknesses_shared_root_union_and_reclamation as four_weaknesses_body,
    a6_ht_b8_removal_heavy_custom_hash_memos_and_slot_reuse as removal_body,
    a6_ht_b8_weak_copy_refuses_before_allocation_or_root_publication as weak_copy_body,
};
use neovm_value::value::Value;

use super::Context;
use super::world::{Baton, OwnedParallelError, OwnedProofProjection};

type BatonRoots = for<'k, 's, 'e, 'o> fn(
    &'k Baton,
    &'s mut AllStopped<'e>,
    &'o mut Vec<Value>,
) -> Result<Vec<*mut *mut MarkerObj>, RendezvousError>;

#[allow(private_interfaces, private_bounds)]
impl<'heap> WeakProjection<'heap> for OwnedProofProjection<'heap> {
    type Keeper = Baton;
    type FinishError = OwnedParallelError;

    fn era(&self) -> &ProofEra<'heap, Baton> {
        OwnedProofProjection::era(self)
    }

    fn finish(self) -> Result<(QuiescentCollector<'heap>, Baton), OwnedParallelError> {
        OwnedProofProjection::finish(self)
    }
}

/// Private host. `Context` is public, so a direct `WeakTableHost` impl would
/// publish `OwnedParallelError` and `OwnedProofProjection`.
struct UnitHost(Context);

#[allow(private_interfaces, private_bounds)]
impl WeakTableHost for UnitHost {
    type EvalError = EvalError;
    type AdmitError = OwnedParallelError;
    type Keeper = Baton;
    type Roots = BatonRoots;
    type Proj<'heap>
        = OwnedProofProjection<'heap>
    where
        Self: 'heap;

    fn eval_str(&mut self, source: &str) -> Result<Value, EvalError> {
        self.0.eval_str(source)
    }

    fn tagged_heap(&self) -> &TaggedHeap {
        &*self.0.tagged_heap
    }

    fn world_roots(&self) -> Self::Roots {
        Baton::collect_parallel_world_roots
    }

    fn admit_owned_parallel_with_weak_tables_for_test<R>(
        &mut self,
        tables: &[Value],
        body: impl WeakAdmit<Self, Out = R>,
    ) -> Result<R, OwnedParallelError>
    where
        Self: Sized,
    {
        self.0
            .admit_owned_parallel_with_weak_tables_for_test(tables, |projection| {
                WeakAdmit::call(body, projection)
            })
    }
}

fn unit_host() -> UnitHost {
    UnitHost(Context::new(crate::test_utils::unit_runtime()))
}

#[test]
fn a6_ht_b8_weak_copy_refuses_before_allocation_or_root_publication() {
    let mut context = unit_host();
    weak_copy_body(&mut context);
}

#[test]
fn a6_ht_b8_four_weaknesses_shared_root_union_and_reclamation() {
    let mut context = unit_host();
    four_weaknesses_body(&mut context);
}

#[test]
fn a6_ht_b8_cross_table_fixpoint_and_unrooted_cycles() {
    let mut context = unit_host();
    cross_table_body(&mut context);
}

#[test]
fn a6_ht_b8_removal_heavy_custom_hash_memos_and_slot_reuse() {
    let mut context = unit_host();
    removal_body(&mut context);
}
