
fn main() {
    let context = Box::new(neovm_core::emacs_core::eval_hook_installers::new_context());
    context.shutdown().unwrap();
    context.shutdown().unwrap();
}
