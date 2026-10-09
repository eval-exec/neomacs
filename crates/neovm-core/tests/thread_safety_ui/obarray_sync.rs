use neovm_core::emacs_core::Obarray;

fn require_sync<T: Sync>() {}

fn main() {
    // Mutable BLVs and local Values are not made shareable by atomic cells.
    require_sync::<Obarray>();
}
