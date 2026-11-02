use neovm_core::tagged::header::SubrObj;
use neovm_core::tagged::value::{CanonicalSubrFactory, TaggedValue};
use neovm_core::emacs_core::intern::SymId;
unsafe impl CanonicalSubrFactory for SubrObj<fn()> {
    fn allocate(_: SymId) -> TaggedValue { TaggedValue::NIL }
}
fn main() {}
