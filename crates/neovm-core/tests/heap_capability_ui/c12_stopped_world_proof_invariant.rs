//! value-gc C12 regression guard: `StoppedWorldProof<'stop>` is invariant in
//! `'stop`, so two distinct stop lifetimes cannot unify via subtyping.

use neovm_value::tagged::gc::StoppedWorldProof;

fn coerce_lifetime<'short, 'long: 'short>(
    proof: &'short StoppedWorldProof<'long>,
) -> &'short StoppedWorldProof<'short> {
    proof
}

fn main() {
    let _ = coerce_lifetime;
}
