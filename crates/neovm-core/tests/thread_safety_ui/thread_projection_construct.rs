use neovm_core::emacs_core::forward::{CanonicalStorage, ThreadProjectionDescriptor, TlsIndex};

fn forge(index: TlsIndex, canonical: CanonicalStorage) -> ThreadProjectionDescriptor {
    ThreadProjectionDescriptor { index, canonical }
}

fn main() {
    let _ = forge;
}
