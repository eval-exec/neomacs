use neovm_core::emacs_core::jit::contract_fixture::{fixture_runtime, set_leaf_slot_word};

fn main() {
    let runtime = fixture_runtime();
    set_leaf_slot_word(&*runtime, 0, 0);
}
