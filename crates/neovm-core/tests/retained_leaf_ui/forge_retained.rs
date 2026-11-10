use neovm_eval::test_support::authority::RetainedLeafForTest;

fn forge(leaf: RetainedLeafForTest) -> RetainedLeafForTest {
    RetainedLeafForTest { ..leaf }
}

fn main() {}
