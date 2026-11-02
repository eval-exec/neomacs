use neovm_core::emacs_core::jit::contract_fixture::{fixture_runtime, set_compiled_id_word};

fn main() {
    let runtime = fixture_runtime();
    set_compiled_id_word(&*runtime, 1);
}
