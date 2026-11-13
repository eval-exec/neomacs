//! GEN1 native stores use GEN0's observed-owner gate for collection history.
//!
//! The remembered-set barrier is separate: an unobserved old owner records no
//! history, yet its young child must survive the next minor collection.

use super::*;

#[test]
fn gen1_observed_setcar_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::Setcar, true);
}

#[test]
fn gen1_observed_setcdr_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::Setcdr, true);
}

#[test]
fn gen1_observed_constant_store_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::ConstantSetcar, true);
}

#[test]
fn gen1_observed_vector_aset_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::VectorAset, true);
}

#[test]
fn gen1_observed_record_aset_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::RecordAset, true);
}

#[test]
fn gen1_observed_string_aset_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::UnibyteStringAset, true);
    check_first_store_in(StoreKind::MultibyteStringAset, true);
}

#[test]
fn gen1_observed_blv_store_skips_until_the_first_owner_read() {
    check_first_store_in(StoreKind::BlvDefault, true);
    check_first_store_in(StoreKind::BlvLocal, true);
}

/// Old, unobserved owners inside and outside the observed hull store young
/// children through native code without collection history. Each store must
/// still remember its owner, so the child survives a minor collection.
#[test]
fn gen1_unobserved_old_owners_remember_young_children_without_history() {
    let _journal = JournalMode::observed();
    let mut context = context(true);
    for kind in [
        StoreKind::Setcar,
        StoreKind::Setcdr,
        StoreKind::VectorAset,
        StoreKind::RecordAset,
    ] {
        let leaf = match kind {
            StoreKind::Setcar => compile_bytecode_function(&store_function(Op::Setcar)),
            StoreKind::Setcdr => compile_bytecode_function(&store_function(Op::Setcdr)),
            _ => Ok(aset_leaf()),
        }
        .expect("GEN1 store compiles");
        let mut owners =
            std::array::from_fn::<_, 4, _>(|_| kind.allocate(&mut context.tagged_heap));
        owners.sort_unstable_by_key(|owner| owner.bits());
        // The middle owner lies inside the observed hull, the last outside it.
        let [first, inside, last, outside] = owners;
        let roots = context.save_specpdl_roots();
        for owner in owners {
            context.push_specpdl_root(owner);
        }
        context.gc_collect_exact();
        for owner in owners {
            assert!(context.tagged_heap.value_is_old_for_test(owner), "{kind:?}");
        }
        let (_, endpoints) = capture(|| (kind.read(first), kind.read(last)));
        let endpoints = endpoints.expect("the endpoints are retained observations");
        for (tag, owner) in [(1, inside), (2, outside)] {
            assert!(!is_observed(owner.bits()), "{kind:?}");
            let child_roots = context.save_specpdl_roots();
            let child = Value::cons(Value::make_int(90 + tag), Value::NIL);
            context.push_specpdl_root(child);
            assert!(!context.tagged_heap.value_is_old_for_test(child));
            let before = LispCollectionRevision::current();
            assert_eq!(native(&mut context, &leaf, &kind.args(owner, child)), child);
            assert_eq!(
                LispCollectionRevision::current().steps_since_for_test(before),
                0,
                "an unobserved GEN1 owner records no history: {kind:?}"
            );
            // Conses dirty their card; other owners log in the remembered set.
            assert!(
                if owner.is_cons() {
                    context.tagged_heap.owner_card_is_dirty_for_test(owner)
                } else {
                    context.tagged_heap.is_remembered_for_test(owner)
                },
                "the generational barrier still runs: {kind:?}"
            );
            assert!(!is_observed(owner.bits()), "the store is not a read");
            context.restore_specpdl_roots(child_roots);

            minor(&mut context);
            let stored = kind.read(owner);
            assert_eq!(stored, child, "{kind:?}");
            assert_eq!(stored.cons_car(), Value::make_int(90 + tag), "{kind:?}");
            assert!(context.tagged_heap.value_is_old_for_test(stored));
        }
        assert!(endpoints.unchanged(), "{kind:?}");
        context.restore_specpdl_roots(roots);
    }
}
