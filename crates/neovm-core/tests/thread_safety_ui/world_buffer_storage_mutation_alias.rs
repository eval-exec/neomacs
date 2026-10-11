use neovm_core::buffer::buffer::BufferManager;

fn main() {
    let mut manager = BufferManager::new();
    let storage = &*manager;
    let _defaults = manager.buffer_defaults_mut();
    let _retained_storage = storage.buffer_defaults.len();
}
