//! A publication view is confined to its owner and to one thread.
//!
//! The fixture compiles the generic owner by path. The view stays
//! crate-private in `neovm-value`; this file does not re-export it.

#[path = "../../../neovm-value/src/tagged/gc/publication/mod.rs"]
mod publication;

#[path = "support/thread_bounds.rs"]
mod thread_bounds;

use publication::{BoundWitness, PublicationLedger, PublicationView};
use thread_bounds::ThreadTransfer;

fn view_outlives(store: &PublicationLedger<BoundWitness>) -> PublicationView<'static, BoundWitness> {
    store.view()
}

fn main() {
    ThreadTransfer::<PublicationView<'static, BoundWitness>>::check();
    let _prove_the_view_cannot_outlive_its_owner = view_outlives;
}
