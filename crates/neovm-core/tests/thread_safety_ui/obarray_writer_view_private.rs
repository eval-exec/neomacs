use neovm_eval::emacs_core::Context;

fn extract_view(context: &mut Context) {
    let writer = context.obarray_mut();
    let _view = writer.view;
}

fn main() {}
