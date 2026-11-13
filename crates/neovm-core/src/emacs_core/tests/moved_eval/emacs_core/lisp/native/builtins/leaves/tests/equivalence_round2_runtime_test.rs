#[test]
#[cfg(feature = "vm-profile")]
fn buffer_local_value_leaf_matches_the_builtin_on_every_variable_shape() {
    neovm_eval::test_support::authority::run_buffer_local_value_leaf_matches_the_builtin_on_every_variable_shape_for_test(crate::test_utils::new_context);
}
