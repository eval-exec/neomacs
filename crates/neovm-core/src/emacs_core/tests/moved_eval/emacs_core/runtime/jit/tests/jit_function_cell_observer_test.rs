/// The function-cell observer is process state; every path that installs the
/// byte-code semantics hook must install it too. The runtime-startup harness
/// restores a pdump image (`pdump_reconstruct`), not `Context::new_inner`: the
/// observer first landed only in the constructor, and every image-restored
/// process silently lost redefinition pinning and inline eviction.
#[test]
fn a_restored_bootstrap_context_installs_the_function_cell_observer() {
    let _ev = crate::test_utils::runtime_startup_context();
    assert!(
        neovm_value::symbol::function_cell_observer().is_some(),
        "the pdump restore path must install the function-cell observer"
    );
}
