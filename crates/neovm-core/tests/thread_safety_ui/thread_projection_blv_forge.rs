use neovm_value::forward::CanonicalArm;

// A canonical localized arm names a record of an obarray's BLV pool; only
// the crate can make one.
fn forge(blv: neovm_value::symbol::BlvPtr) -> CanonicalArm {
    CanonicalArm::Localized(neovm_value::forward::CanonicalBlv::new(blv))
}

fn main() {
    let _ = forge;
}
