//! value-gc C13 positive compiling control (G1): implementing the `unsafe trait`
//! `PublicationPayload` with an explicit `unsafe impl` compiles cleanly.
#[path = "../../../neovm-value/src/tagged/gc/publication/mod.rs"]
mod publication;

use publication::{PayloadPlan, PublicationPayload, RetainedBytes};

struct PinnedPayload(Box<u8>);

// SAFETY: `Box<u8>` keeps its pointee at a stable heap address across moves
// into the predecessor vector until drop.
unsafe impl PublicationPayload for PinnedPayload {
    type Error = ();
    type Generation = u64;

    fn published_address(&self) -> *mut u8 {
        (&raw const *self.0).cast_mut()
    }

    fn retained_bytes(&self) -> RetainedBytes {
        RetainedBytes::new(1)
    }

    fn generation(&self) -> Self::Generation {
        0
    }

    fn prepare(_current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
        Ok(PayloadPlan::Unchanged)
    }
}

fn main() {
    let payload = PinnedPayload(Box::new(7));
    let _ = payload.published_address();
}
