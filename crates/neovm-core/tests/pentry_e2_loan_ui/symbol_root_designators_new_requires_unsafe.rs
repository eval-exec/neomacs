use neovm_value::tagged::gc::{SymbolRootDesignators, WorldIdentity};
use std::ptr::NonNull;

fn main() {
    let world = unsafe { WorldIdentity::new(0) };
    // The designator constructor itself must be unsafe.
    let _roots =
        SymbolRootDesignators::new(world, NonNull::<()>::dangling(), NonNull::<()>::dangling());
}
