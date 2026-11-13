#[test]
fn every_bcall_leaf_is_attached_to_its_registered_subr() {
    neovm_eval::test_support::authority::run_every_bcall_leaf_is_attached_to_its_registered_subr_for_test(crate::test_utils::new_context);
}
