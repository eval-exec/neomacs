//! Restricted-lisp overlap shells.
//! The bare evaluator, planted-word roots, era admission, and the baton
//! World walk live here. Assertions stay in the value bodies. Without the
//! JIT feature the switch callbacks are inert; the JIT test itself is
//! cfg-gated so those stand-ins cannot satisfy it.
use std::cell::RefCell;

use neovm_eval::test_support::authority::{
    admit_owned_proof_parallel_for_test, baton_collect_parallel_world_roots,
};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::run_restricted_lisp_test_for_test;

fn run(name: &str) {
    let context = crate::test_utils::new_context();
    let generational = context.tagged_heap().tagged_heap_generational_enabled();
    let cell = RefCell::new(context);

    let mut eval = |source: &str| {
        cell.borrow_mut()
            .eval_str(source)
            .map_err(|error| format!("{error:?}"))
    };
    let mut share = |value: TaggedValue| {
        let context = cell.borrow();
        // SAFETY: nil, fixnum, and planted words come from this context's heap.
        // The caller shares them before admission and does not collect in between.
        unsafe { SharedRoot::new(context.tagged_heap(), value) }
    };
    let mut admit = |admit_label: &str,
                     finish_label: &str,
                     body: &mut dyn for<'e> FnMut(
        &neovm_eval::test_support::authority::WorldProofEra<'e>,
    )| {
        let mut context = cell.borrow_mut();
        admit_owned_proof_parallel_for_test(&mut context, |projection| {
            body(projection.era());
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("{finish_label}: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("{admit_label}: {error:?}"));
    };
    let mut bytes_since_gc = || cell.borrow().tagged_heap().bytes_since_gc();

    #[cfg(feature = "jit")]
    let switch_on = crate::emacs_core::jit::compile::jit_inline_switch_on();
    #[cfg(not(feature = "jit"))]
    let switch_on = false;

    #[cfg(feature = "jit")]
    let mut switch_sites = || crate::emacs_core::jit::compile::inline_switch_sites_for_test();
    #[cfg(not(feature = "jit"))]
    let mut switch_sites = || 0usize;

    #[cfg(feature = "jit")]
    let leaf_cell = RefCell::new(None::<crate::emacs_core::jit::compile::CompiledLeaf>);
    #[cfg(feature = "jit")]
    let mut switch_compile =
        |ops: &[neovm_value::tagged::bytecode::opcode::Op],
         constants: &[TaggedValue],
         map: &[neovm_value::tagged::bytecode::chunk::GnuByteOffsetMapEntry]| {
            let leaf =
                crate::emacs_core::jit::compile::lower_leaf_with_map(ops, constants, 1, Some(map))
                    .expect("the switch body compiles");
            *leaf_cell.borrow_mut() = Some(leaf);
        };
    #[cfg(not(feature = "jit"))]
    let mut switch_compile =
        |_ops: &[neovm_value::tagged::bytecode::opcode::Op],
         _constants: &[TaggedValue],
         _map: &[neovm_value::tagged::bytecode::chunk::GnuByteOffsetMapEntry]| {};

    #[cfg(feature = "jit")]
    let mut switch_call = |args: &[TaggedValue]| {
        let leaf_borrow = leaf_cell.borrow();
        let leaf = leaf_borrow.as_ref().expect("the switch body compiles");
        let mut context = cell.borrow_mut();
        let ctx_ptr = &mut *context as *mut crate::emacs_core::eval::Context as *mut u8;
        match leaf.call(ctx_ptr, args) {
            crate::emacs_core::jit::compile::NativeRun::Ok(bits) => (0, bits, String::new()),
            crate::emacs_core::jit::compile::NativeRun::Signal => (1, 0, String::new()),
            other => (2, 0, format!("{other:?}")),
        }
    };
    #[cfg(not(feature = "jit"))]
    let mut switch_call = |_args: &[TaggedValue]| (2u8, 0usize, String::new());

    #[cfg(feature = "jit")]
    let mut switch_flow = || crate::emacs_core::jit::compile::take_pending_flow();
    #[cfg(not(feature = "jit"))]
    let mut switch_flow = || None;

    #[cfg(feature = "jit")]
    let mut switch_print = |value: &TaggedValue| crate::emacs_core::print::print_value(value);
    #[cfg(not(feature = "jit"))]
    let mut switch_print = |_value: &TaggedValue| String::new();

    run_restricted_lisp_test_for_test(
        name,
        &mut eval,
        &mut share,
        &mut admit,
        &mut bytes_since_gc,
        generational,
        baton_collect_parallel_world_roots,
        switch_on,
        &mut switch_sites,
        &mut switch_compile,
        &mut switch_call,
        &mut switch_flow,
        &mut switch_print,
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

shell!(a6_runtime_refuses_foreign_cons_without_writing);
shell!(a6_arithmetic_matches_gnu_oracle);
shell!(a6_ht_ops_match_gnu_oracle);
shell!(a6_ht_user_test_refuses_mutation_inside_the_test);
shell!(a6_ht_user_test_revalidates_after_callback_window);
shell!(a6_ht_lease_mints_from_a_live_run_access);
shell!(a6_nonallocating_sum_loop);
shell!(a6_overlap_lists_two_owners);
shell!(a6_overlap_lists_four_owners);
shell!(a6_ht_overlap_shared_readers_two_owners);
shell!(a6_ht_overlap_shared_readers_three_owners);
shell!(a6_ht_overlap_shared_readers_four_owners);
shell!(a6_ht_overlap_writer_drains_four_readers);
shell!(a6_ht_overlap_final_state_in_legal_serial_interleavings);
shell!(a6_ht_string_keys_match_gnu_oracle);
shell!(a6_ht_user_test_revalidates_under_another_owner);
shell!(a6_ht_reader_scaling_row);
shell!(a6_ht_growth_races_readers);
shell!(a6_ht_bounded_growth_many_owners);
shell!(a6_ht_growth_memory_overhead);
shell!(a6_ht_growth_serial_twin_matches_gnu_oracle);
shell!(a6_ht_make_copy_serial_twins_match_gnu_oracle);
shell!(a6_ht_gc_shared_roots_survive_and_clrhash_reclaims);
shell!(a6_ht_gc_old_tables_remember_young_values);
shell!(a6_ht_b7_make_copy_collect_resume_and_reclaim);
shell!(a6_ht_b7_copy_while_another_owner_mutates);
shell!(a6_ht_b7_copy_active_test_starts_quiescent_and_bad_home_is_atomic);

#[cfg(feature = "perf-probes")]
#[test]
fn a8_throughput_rows() {
    run("a8_throughput_rows");
}

#[cfg(feature = "jit")]
#[test]
fn a6_ht_sealed_edit_triggers_jit_switch_stale_shim() {
    run("a6_ht_sealed_edit_triggers_jit_switch_stale_shim");
}
