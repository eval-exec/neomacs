use neovm_core::tagged::gc::Gen1Admission;

fn duplicate<'heap>(admission: Gen1Admission<'heap>) -> Gen1Admission<'heap> {
    <Gen1Admission<'heap> as Clone>::clone(&admission)
}

fn main() {}
