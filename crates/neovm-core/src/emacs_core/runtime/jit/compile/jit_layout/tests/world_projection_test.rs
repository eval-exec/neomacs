use super::*;
use crate::emacs_core::forward::alloc_boolfwd;
use crate::emacs_core::intern::intern;

fn admission_words(context: &Context) -> (u64, usize) {
    let base = std::ptr::from_ref(context).cast::<u8>();
    // SAFETY: these repr-pinned offsets name initialized inline owner header
    // words. This shared Context phase excludes retirement; Cell's transparent
    // storage is read without forming a mutable reference or calling Lisp/GC.
    let words = unsafe {
        let header = base.add(CONTEXT_DISPATCH_CELLS_OFFSET);
        let epoch = header
            .add(DISPATCH_FUNCTION_EPOCH_OFFSET)
            .cast::<u64>()
            .read();
        let descriptor = header
            .add(DISPATCH_DEBUG_FWD_OFFSET)
            .cast::<*const crate::emacs_core::forward::LispBoolFwd>()
            .read();
        assert!(!descriptor.is_null());
        (epoch, descriptor as usize)
    };
    assert_eq!(words.0, context.obarray().function_epoch());
    assert!(context.obarray.admission_matches_canonical());
    words
}

#[test]
fn world_projection_goldens_follow_canonical_writer_retirement() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new_minimal_vm_harness();
    let before = admission_words(&context).0;
    context
        .obarray_mut()
        .set_symbol_function("projection-golden-target", Value::fixnum(17));
    assert!(admission_words(&context).0 > before);
    assert_eq!(
        admission_words(&context).0,
        context.obarray().function_epoch()
    );

    let descriptor = alloc_boolfwd(false);
    context
        .obarray_mut()
        .install_boolfwd(intern("debug-on-next-call"), descriptor);
    assert_eq!(
        admission_words(&context).1,
        std::ptr::from_ref(descriptor) as usize
    );
    context.gc_collect_exact();
    descriptor.set(true);
    assert!(context.debug_on_next_call_is_armed());
    descriptor.set(false);
    assert!(!context.debug_on_next_call_is_armed());
}

#[test]
fn world_projection_goldens_construct_replacement_before_admission() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new_minimal_vm_harness();
    context
        .obarray_mut()
        .set_symbol_function("projection-golden-replaced", Value::fixnum(23));
    let _ = admission_words(&context);
    drop(context);

    // A fresh minimal constructor chooses its raw manager before World sealing.
    // An admitted view exposes no safe mutable manager/replacement operation.
    let mut replacement = Context::new_minimal_vm_harness();
    assert!(
        replacement
            .obarray()
            .symbol_function("projection-golden-replaced")
            .is_none()
    );
    let _ = admission_words(&replacement);
    replacement.resolve_debug_on_next_call_cell();
    assert!(
        !replacement.debug_on_next_call_is_armed(),
        "absent sentinel"
    );
    let _ = admission_words(&replacement);
}
