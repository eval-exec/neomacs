use neovm_value::tagged::gc::SymbolHandle;
fn forge(key: &mut SymbolHandle) {
    key.world = 7;
}
fn main() {}
