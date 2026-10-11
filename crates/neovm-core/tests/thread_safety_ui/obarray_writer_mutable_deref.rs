use neovm_core::emacs_core::{Context, Obarray};

fn mutable_manager(context: &mut Context) {
    let mut writer = context.obarray_mut();
    let _: &mut Obarray = std::ops::DerefMut::deref_mut(&mut writer);
}

fn main() {}
