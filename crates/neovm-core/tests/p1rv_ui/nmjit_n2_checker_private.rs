use neovm_core::tagged::gc::Gen1Admission;

fn unchecked_checker() {
    let _ = Gen1Admission::check;
}

fn main() {}
