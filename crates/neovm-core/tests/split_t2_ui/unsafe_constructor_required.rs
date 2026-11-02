use neovm_core::tagged::header::{SubrMetadata, SubrObj};
fn forged(metadata: SubrMetadata) {
    let _ = SubrObj::<fn()>::new_unchecked(metadata, None);
}
fn main() {}
