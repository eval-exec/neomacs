use neovm_core::emacs_core::Context;

fn main() {
    let context = Box::new(Context::new());
    context.shutdown().unwrap();
    context.shutdown().unwrap();
}
