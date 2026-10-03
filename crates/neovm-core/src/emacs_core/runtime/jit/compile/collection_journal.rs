//! Collection history for accepted native string byte stores.
//!
//! Threading: this shim retains no state. It updates the executing mutator's
//! existing private revision/journal, exactly like the interpreter helpers.
//! Certificates remain confined to their capturing mutator (`!Send`); this
//! does not introduce or claim cross-mutator certificate coherence.

/// No Lisp allocation, callback or collector safe point occurs here. Native
/// sites have already proved a live string and all store guards, and call
/// before changing its contents. String bytes need no GC write barrier, but
/// require the same revision and pointer observation as the string setter.
#[unsafe(no_mangle)]
pub extern "C" fn neovm_jit_string_collection_write(bits: i64) {
    let owner = crate::tagged::value::TaggedValue::from_bits(bits as usize);
    crate::tagged::mutate::LispCollectionRevision::changed(owner);
    let projected = owner.as_string_ptr();
    debug_assert!(projected.is_some());
}
