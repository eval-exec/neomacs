//! Generational major shells.
//! The eval scratch-root adapters, the JIT cons stores, and the edit
//! marker-chain host live here. Assertions stay in the value bodies.
use crate::buffer::buffer_text::BufferText;
use neovm_edit::test_support::authority::chain_splice_at_head_for_test;
use neovm_eval::emacs_core::eval::{
    push_scratch_gc_root_slot, restore_scratch_gc_roots, save_scratch_gc_roots, set_scratch_gc_root,
};
use neovm_value::tagged::header::MarkerObj;
use neovm_value::test_support::authority::run_generational_major_test_for_test;

type JitStore = unsafe extern "C" fn(*mut u8, i64, i64) -> i64;

fn new_marker_state() -> *mut () {
    Box::into_raw(Box::new(BufferText::new())) as *mut ()
}

unsafe fn marker_splice(state: *mut (), marker: *mut MarkerObj) {
    let text = unsafe { &*(state as *const BufferText) };
    // SAFETY: `marker` is a live unchained marker in this serial fixture, and
    // `state` is the text this shell allocated.
    unsafe { chain_splice_at_head_for_test(text, marker) };
}

unsafe fn marker_head(state: *mut ()) -> *mut *mut MarkerObj {
    let text = unsafe { &*(state as *const BufferText) };
    // SAFETY: the value body calls this from the same exclusive fixture that
    // used to call `BufferText::markers_head_slot_raw`.
    unsafe { text.markers_head_slot_raw() }
}

unsafe fn marker_walk(state: *mut ()) -> Vec<*mut MarkerObj> {
    let text = unsafe { &*(state as *const BufferText) };
    text.chain_walk_collect()
}

unsafe fn marker_drop(state: *mut ()) {
    drop(unsafe { Box::from_raw(state as *mut BufferText) });
}

#[cfg(feature = "jit")]
fn jit_stores() -> (JitStore, JitStore) {
    (
        neovm_eval::emacs_core::jit::compile::neovm_jit_setcar,
        neovm_eval::emacs_core::jit::compile::neovm_jit_setcdr,
    )
}

#[cfg(not(feature = "jit"))]
fn jit_stores() -> (JitStore, JitStore) {
    (jit_unavailable, jit_unavailable)
}

#[cfg(not(feature = "jit"))]
unsafe extern "C" fn jit_unavailable(_ctx: *mut u8, _cell: i64, _new: i64) -> i64 {
    panic!("the JIT fixture is only selected with feature=jit");
}

fn run(name: &str) {
    let (jit_setcar, jit_setcdr) = jit_stores();
    run_generational_major_test_for_test(
        name,
        save_scratch_gc_roots,
        push_scratch_gc_root_slot,
        set_scratch_gc_root,
        restore_scratch_gc_roots,
        jit_setcar,
        jit_setcdr,
        new_marker_state,
        marker_splice,
        marker_head,
        marker_walk,
        marker_drop,
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

shell!(major_reclaims_dead_old_cons_box_and_bytecode_payload);
shell!(major_keeps_permanent_payload_and_its_ordinary_old_child);
shell!(major_drops_an_unrooted_old_weak_key_after_minor_retention);
shell!(major_queues_an_unrooted_old_finalizer_once);
shell!(major_unchains_an_unrooted_old_marker);
shell!(major_retraces_old_children_when_mark_parity_repeats);
shell!(major_retraces_old_uninterned_symbol_owner_without_recent_r);
shell!(late_dump_partition_visits_old_objects_and_consumes_stale_r_safely);
shell!(concurrent_major_promotes_worker_claims_black_births_before_deferred_sweep);
shell!(major_and_minor_do_not_change_exact_allocation_counter_snapshots);
shell!(major_explicit_first_partition_disarm_drops_completed_black_birth_logs);
shell!(concurrent_major_keeps_fresh_symbol_inserted_into_claimed_old_cons);
shell!(concurrent_major_keeps_existing_weak_symbol_inserted_into_claimed_old_cons);
shell!(concurrent_major_keeps_existing_weak_cons_inserted_into_claimed_old_cons);
shell!(concurrent_major_keeps_existing_weak_vector_inserted_into_claimed_old_cons);
shell!(concurrent_major_keeps_existing_weak_string_inserted_into_claimed_old_cons);
shell!(concurrent_major_generic_cons_hook_keeps_existing_weak_heap_objects);
shell!(major_remembered_membership_requires_owner_identity);
