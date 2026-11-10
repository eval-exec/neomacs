use neovm_eval::test_support::authority::RetainedOsrLeafForTest;
use neovm_value::value::Value;

fn invoke(leaf: &RetainedOsrLeafForTest) {
    let args: &[Value] = &[];
    leaf.call(core::ptr::null_mut::<u8>(), args);
}

fn main() {}
