use neovm_core::tagged::gc::FacadeMarkExclusion;

fn transfer(exclusion: FacadeMarkExclusion) {
    std::thread::spawn(move || drop(exclusion));
}

fn main() {}
