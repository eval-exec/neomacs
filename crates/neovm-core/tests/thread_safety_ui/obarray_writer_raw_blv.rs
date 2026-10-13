use neovm_core::emacs_core::{Context, SymId};

fn raw_blv(context: &mut Context, id: SymId) {
    let mut writer = context.obarray_mut();
    let _localized = writer.blv_mut(id);
}

fn main() {}
