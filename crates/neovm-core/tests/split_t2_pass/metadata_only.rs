use neovm_core::tagged::header::{SubrMetadata, SubrStorageLayout};
fn inspect(metadata: &SubrMetadata) {
    let _ = metadata.symbol_id();
    let _ = metadata.name_id();
    let _ = (metadata.min_args(), metadata.max_args());
    let _ = (metadata.dispatch_kind(), metadata.interactivity());
    let _ = SubrStorageLayout::layout();
}
fn main() { let _ = inspect as fn(&SubrMetadata); }
