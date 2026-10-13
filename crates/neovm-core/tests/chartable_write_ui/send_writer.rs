use neovm_core::tagged::mutate::CharTableWrite;

fn require_send<T: Send>() {}

fn main() {
    require_send::<CharTableWrite<'static>>();
}
