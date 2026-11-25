//! value-gc C13 regression guard (G1): `PublicationPayload` must be an `unsafe trait`
//! because callers dereference `published_address()` across moves into the
//! predecessor vector until drop.
#[path = "../../../neovm-value/src/tagged/gc/publication/mod.rs"]
mod publication;

use publication::{PayloadPlan, PublicationPayload, RetainedBytes};

struct SafePayload(u8);

impl PublicationPayload for SafePayload {
    type Error = ();
    type Generation = u64;

    fn published_address(&self) -> *mut u8 {
        std::ptr::null_mut()
    }

    fn retained_bytes(&self) -> RetainedBytes {
        RetainedBytes::new(0)
    }

    fn generation(&self) -> Self::Generation {
        0
    }

    fn prepare(_current: &Self) -> Result<PayloadPlan<Self>, Self::Error> {
        Ok(PayloadPlan::Unchanged)
    }
}

fn main() {
    let _ = SafePayload(0).published_address();
}
