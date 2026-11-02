use neovm_core::tagged::value::{CanonicalSubrFactory, TaggedValue};
use neovm_core::emacs_core::intern::SymId;
struct Fake;
impl CanonicalSubrFactory for Fake { fn allocate(_: SymId) -> TaggedValue { TaggedValue::NIL } }
fn main() {}
