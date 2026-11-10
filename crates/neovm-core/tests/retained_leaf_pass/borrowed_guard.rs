use neovm_eval::test_support::authority::{
    LeafIdentity, NativeEntryIdentityForTest, RetainedLeafForTest,
};

fn observe(leaf: &RetainedLeafForTest) -> (bool, LeafIdentity, NativeEntryIdentityForTest) {
    (leaf.retired(), leaf.identity(), leaf.entry_identity())
}

fn main() {}

fn observe_osr(
    leaf: &neovm_eval::test_support::authority::RetainedOsrLeafForTest,
) -> (bool, LeafIdentity) {
    (leaf.retired(), leaf.identity())
}
