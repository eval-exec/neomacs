use neovm_core::emacs_core::{Context, Obarray};

fn replace_owner_storage(context: &mut Context) {
    let mut writer = context.obarray_mut();
    let _old = std::mem::replace(&mut *writer, Obarray::new());
}

fn main() {}
