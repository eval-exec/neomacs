use neovm_core::tagged::gc::{Gen1Admission, TaggedHeap};

// Another heap loan cannot extend this admission's retained collector loan.
// Actual heap identity is separately checked by the runtime constructor.
fn rebind<'source, 'other>(
    admission: Gen1Admission<'source>,
    _other_heap: &'other mut TaggedHeap,
) -> Gen1Admission<'other> {
    admission
}

fn main() {}
