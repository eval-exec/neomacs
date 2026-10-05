use neovm_core::tagged::header::VectorScanEntry;

fn main() {
    let _entry = VectorScanEntry {
        base: std::ptr::null(),
        len: 1,
        is_mapped: false,
    };
}
