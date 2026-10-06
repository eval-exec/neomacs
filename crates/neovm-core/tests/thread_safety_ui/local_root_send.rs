use neovm_core::tagged::transport::LocalRoot;

fn require_send<T: Send>() {}

fn main() {
    require_send::<LocalRoot<'static>>();
}
