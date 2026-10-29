use neovm_core::tagged::gc::Gen1Admission;

fn escape<'heap>(admission: Gen1Admission<'heap>) -> Gen1Admission<'static> {
    admission
}

fn main() {}
