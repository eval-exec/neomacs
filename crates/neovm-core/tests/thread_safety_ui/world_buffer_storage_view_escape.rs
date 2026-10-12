use neovm_core::buffer::buffer::{BufferManager, BufferStorage};

fn escaping_storage() -> &'static BufferStorage {
    let manager = BufferManager::new();
    &manager
}

fn main() {}
