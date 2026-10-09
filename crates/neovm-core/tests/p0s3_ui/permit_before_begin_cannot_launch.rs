use neovm_core::tagged::gc::ConcurrentMarkPermit;

fn launch_before_begin(permit: ConcurrentMarkPermit<'_>) {
    let _ = permit.launch();
}

fn main() {}
