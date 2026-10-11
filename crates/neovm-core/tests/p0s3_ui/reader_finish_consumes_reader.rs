use neovm_core::tagged::transport::RootBatchReader;

fn acknowledge_twice(reader: RootBatchReader<'_>) {
    let _ = reader.finish();
    let _ = reader.finish();
}

fn main() {}
