use neovm_core::tagged::transport::RootBatchReader;

fn transfer(reader: RootBatchReader<'static>) {
    std::thread::spawn(move || {
        let _ = reader.finish();
    });
}

fn main() {}
