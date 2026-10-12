use neovm_core::emacs_core::{Context, SymId};

fn raw_symbol(context: &mut Context, id: SymId) {
    let mut writer = context.obarray_mut();
    let _symbol = writer.get_mut_by_id(id);
}

fn main() {}
