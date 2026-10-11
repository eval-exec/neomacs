use neovm_core::tagged::transport::FinalizedRootBatch;
use neovm_core::tagged::value::TaggedValue;

fn replace_finalized_words(finalized: &mut FinalizedRootBatch) {
    let words: &mut [TaggedValue] = finalized.as_mut();
    words[0] = TaggedValue::NIL;
}

fn main() {}
