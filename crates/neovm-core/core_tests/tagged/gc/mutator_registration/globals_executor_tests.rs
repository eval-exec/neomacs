//! Global-executor shells.
//! The symbol-era boot, one owned-proof admission, and the baton World walk
//! live here. Assertions stay in the value bodies. Three multi-owner handoff
//! rows stay `not(miri)`.
use neovm_eval::test_support::authority::{
    admit_owned_proof_parallel_for_test, baton_collect_parallel_world_roots,
    with_test_symbol_backing_era, with_test_symbol_backings_era, with_test_symbol_era,
};
use neovm_value::intern::SymId;
use neovm_value::tagged::gc::PlainSymbolBacking;
use neovm_value::test_support::authority::run_globals_executor_test_for_test;

fn run(name: &str) {
    crate::test_utils::init_test_tracing();
    let mut symbol_era = |body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        SymId,
    )| {
        with_test_symbol_era(|era, id| {
            body(era, id);
        });
    };
    let mut symbol_backing = |kind: PlainSymbolBacking,
                              body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        SymId,
    )| {
        with_test_symbol_backing_era(kind, |era, id| {
            body(era, id);
        });
    };
    let mut symbol_backings = |kinds: &[PlainSymbolBacking],
                               body: &mut dyn for<'e> FnMut(
        neovm_eval::test_support::authority::WorldProofEra<'e>,
        Vec<SymId>,
    )| {
        with_test_symbol_backings_era(kinds, |era, ids| {
            body(era, ids);
        });
    };
    let mut eval_then_admit = |source: &str,
                               body: &mut dyn for<'e> FnMut(
        &neovm_eval::test_support::authority::WorldProofEra<'e>,
    )| {
        let mut context = crate::test_utils::new_context();
        context.eval_str(source).unwrap();
        admit_owned_proof_parallel_for_test(&mut context, |projection| {
            body(projection.era());
            let (collector, keeper) = projection.finish().unwrap();
            (collector, keeper, ())
        })
        .unwrap();
    };
    run_globals_executor_test_for_test(
        name,
        &mut symbol_era,
        &mut symbol_backing,
        &mut symbol_backings,
        &mut eval_then_admit,
        baton_collect_parallel_world_roots,
    );
}

macro_rules! shell {
    ($name:ident) => {
        #[test]
        fn $name() {
            run(stringify!($name));
        }
    };
}

shell!(p3sym_c5_global_executor_cannot_admit_intern_even_if_unreachable);
shell!(p3sym_c3_executor_primitives_match_gnu);
shell!(p3sym_c3_executor_nil_void_and_graph_reads);
shell!(p3sym_c3_executor_gate_checks_all_participants_and_handles);
shell!(p3sym_c3_executor_rejects_transitive_mutable_global_before_activation);
shell!(p3sym_c3_executor_refusal_preserves_pc_and_retries);
shell!(p3sym_c3_plain_global_graph_four_stops);
shell!(p3sym_c3_projected_global_graph_four_stops);
shell!(p3sym_c3_void_global_graph_four_stops);
shell!(p3sym_c3_void_siblings_are_independent_and_page_growth_is_charged);
shell!(p3sym_c3_two_owner_handoff_gc_small);

#[cfg(not(miri))]
#[test]
fn p3sym_c3_plain_multi_owner_handoff_gc() {
    run("p3sym_c3_plain_multi_owner_handoff_gc");
}

#[cfg(not(miri))]
#[test]
fn p3sym_c3_projected_multi_owner_handoff_gc() {
    run("p3sym_c3_projected_multi_owner_handoff_gc");
}

#[cfg(not(miri))]
#[test]
fn p3sym_c3_void_multi_owner_handoff_gc() {
    run("p3sym_c3_void_multi_owner_handoff_gc");
}
