//! value-gc C2 regression guard (G1): `EntryImageSlot` seals `EntryState.image`
//! with a private inner field so sibling modules in `mutator_registration`
//! cannot directly assign or replace the stored `PublishedGcImage`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeapIdentity(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct YoungRegistrationId(u64);
impl YoungRegistrationId {
    fn heap_identity(self) -> HeapIdentity {
        HeapIdentity(self.0)
    }
}
struct YoungBlockLog;
#[derive(Debug)]
enum YoungRegistryError {
    WrongHeap,
    UnknownRegistration,
}
#[derive(Debug)]
enum ImageError {
    DestinationOccupied,
    Young(YoungRegistryError),
}
#[derive(Debug)]
struct RestoreImageFailure {
    error: ImageError,
    image: PublishedGcImage,
}
#[derive(Debug)]
struct PublishedGcImage(YoungRegistrationId);
impl PublishedGcImage {
    fn young_id(&self) -> YoungRegistrationId {
        self.0
    }
    fn young_mut(&mut self) -> &mut YoungBlockLog {
        unreachable!()
    }
}

#[path = "../../../neovm-value/src/tagged/gc/mutator_registration/entry_image.rs"]
mod entry_image;

fn main() {
    let mut slot = entry_image::EntryImageSlot::empty();
    slot.0 = Some(PublishedGcImage(YoungRegistrationId(1)));
}
