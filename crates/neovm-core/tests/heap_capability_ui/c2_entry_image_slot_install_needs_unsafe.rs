//! value-gc C2 regression guard (G1): `EntryImageSlot::install` requires
//! `unsafe` so safe code in `mutator_registration` cannot install an arbitrary
//! `PublishedGcImage` into `EntryState`.

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
    let _ = slot.install(
        HeapIdentity(1),
        YoungRegistrationId(1),
        PublishedGcImage(YoungRegistrationId(1)),
    );
}
