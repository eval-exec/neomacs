use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::eval::ObarrayWriteAccess;

fn escape_capability(context: &mut Context) -> ObarrayWriteAccess<'_> {
    context.with_obarray_mut(|writer| writer)
}

fn main() {}
