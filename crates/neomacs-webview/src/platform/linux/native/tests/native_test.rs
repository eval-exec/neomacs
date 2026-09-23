#[test]
fn native_vfunc_panics_are_contained_at_the_abi_boundary() {
    assert_eq!(
        super::guard_native_vfunc("test", 41_u32, || panic!("contained")),
        41
    );
}
