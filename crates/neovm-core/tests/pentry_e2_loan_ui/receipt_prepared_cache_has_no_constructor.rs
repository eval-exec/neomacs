use neovm_value::heap_registry::text_retirement::PreparedValueCacheRetirement;

fn main() {
    let _prepared = PreparedValueCacheRetirement::<'_>::new();
}
