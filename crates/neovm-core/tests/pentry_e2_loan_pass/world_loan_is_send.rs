use neovm_value::tagged::gc::WorldLoan;

fn assert_send<T: Send>() {}

fn main() {
    assert_send::<WorldLoan>();
}
