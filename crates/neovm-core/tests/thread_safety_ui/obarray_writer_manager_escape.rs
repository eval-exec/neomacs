use neovm_core::emacs_core::{Context, Obarray};

fn escape_manager(context: &mut Context) -> &Obarray {
    context.with_obarray_mut(|writer| &*writer)
}

fn main() {}
