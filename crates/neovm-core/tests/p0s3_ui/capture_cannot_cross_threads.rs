use neovm_core::tagged::gc::ConcurrentMarkCapture;

fn transfer(capture: ConcurrentMarkCapture<'static>) {
    std::thread::spawn(move || {
        let _ = capture.launch();
    });
}

fn main() {}
