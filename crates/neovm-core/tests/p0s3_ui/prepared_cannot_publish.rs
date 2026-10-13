use neovm_core::tagged::transport::PreparedRootBatch;

fn publish_before_finalize(prepared: PreparedRootBatch) {
    let _ = prepared.publish();
}

fn main() {}
