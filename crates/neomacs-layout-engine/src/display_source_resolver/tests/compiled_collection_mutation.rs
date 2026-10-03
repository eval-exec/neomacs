//! The real source-face cache must re-resolve mutable face plists after a
//! compiled store, just as it does after an interpreted store.

use super::*;
use neovm_core::emacs_core::bytecode::Op;
use neovm_core::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use neovm_core::emacs_core::jit::compile::{NativeRun, lower_leaf};

#[test]
fn gen0_compiled_face_plist_mutation_rebuilds_cached_realization() {
    // Nextest runs this regression in its own process. Select GEN0 before
    // constructing its heap, including when the outer suite selects GEN1.
    let previous_journal = std::env::var_os("NEOVM_JIT_GEN0_COLLECTION_JOURNAL");
    let previous_generational = std::env::var_os("NEOVM_GC_GENERATIONAL");
    unsafe {
        std::env::set_var("NEOVM_JIT_GEN0_COLLECTION_JOURNAL", "on");
        std::env::remove_var("NEOVM_GC_GENERATIONAL");
    }
    let mut context = Context::new();
    unsafe {
        if let Some(previous) = previous_generational {
            std::env::set_var("NEOVM_GC_GENERATIONAL", previous);
        }
    }
    let value = Value::list(vec![Value::symbol(":height"), Value::fixnum(120)]);
    let roots = save_scratch_gc_roots();
    push_scratch_gc_root(value);
    let cell = value.cons_cdr();
    let leaf = lower_leaf(
        &[Op::StackRef(1), Op::StackRef(1), Op::Setcar, Op::Return],
        &[],
        2,
    )
    .expect("native face-plist setter compiles");
    unsafe {
        if let Some(previous) = previous_journal {
            std::env::set_var("NEOVM_JIT_GEN0_COLLECTION_JOURNAL", previous);
        } else {
            std::env::remove_var("NEOVM_JIT_GEN0_COLLECTION_JOURNAL");
        }
    }

    let table = FaceTable::new();
    let face_resolver = test_face_resolver(&table);
    let params = DisplaySourceResolveParams::new(
        DisplaySourceFaceBasis::new(
            &face_resolver,
            FaceId::new(0),
            face_resolver.default_face(),
            DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
        ),
        None,
        ImageScaleEnvironment::default(),
    );
    let mut state = DisplaySourceResolveState::default();
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(20);
    let mut pending = Vec::new();
    let resolve = |state: &mut DisplaySourceResolveState,
                   face_ids: &mut FrameFaceAttempt,
                   pending: &mut Vec<PendingDisplaySourceFace>| {
        DisplaySourcePropertyResolver::frame_local(params, state, face_ids, pending)
            .resolve_face_ref(RenderFaceRef::Inherit, value)
    };
    let first = resolve(&mut state, &mut face_ids, &mut pending);
    // Cache insertion precedes the first structural hit. The second hit
    // captures its dependencies, and the third exercises certificate reuse.
    assert_eq!(resolve(&mut state, &mut face_ids, &mut pending), first);
    assert_eq!(resolve(&mut state, &mut face_ids, &mut pending), first);
    assert_eq!(state.structural_face_lookups, 2);
    let first_size = state.resolved_face(face_id(first)).unwrap().font_size;

    let replacement = Value::fixnum(180);
    match leaf.call(
        &mut context as *mut Context as *mut u8,
        &[cell, replacement],
    ) {
        NativeRun::Ok(bits) => assert_eq!(bits, replacement.bits()),
        other => panic!("the face-plist setter must execute natively: {other:?}"),
    }
    assert_eq!(cell.cons_car(), replacement);
    let cached = resolve(&mut state, &mut face_ids, &mut pending);
    let cached_size = state.resolved_face(face_id(cached)).unwrap().font_size;

    let mut fresh_state = DisplaySourceResolveState::default();
    let mut fresh_ids = FrameFaceAttempt::for_test_with_next_id(40);
    let mut fresh_pending = Vec::new();
    let fresh = resolve(&mut fresh_state, &mut fresh_ids, &mut fresh_pending);
    let fresh_size = fresh_state.resolved_face(face_id(fresh)).unwrap().font_size;
    assert_ne!(
        fresh_size, first_size,
        "the changed plist changes the face height"
    );
    assert_eq!(
        cached_size, fresh_size,
        "the real source-face cache reused a stale realization after compiled setcar"
    );
    assert_eq!(
        state.structural_face_lookups, 3,
        "the mutated certificate retries structural resolution"
    );
    restore_scratch_gc_roots(roots);
}
