use neovm_core::tagged::header::{SubrMetadata, SubrObj};
fn forged(metadata: SubrMetadata) {
    let _ = SubrObj::<fn()> { metadata, function: None };
}
fn main() {}
