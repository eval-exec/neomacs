use neovm_core::tagged::gc::{Gen1Admission, MutatorPermit};

fn needs_admission(_admission: Gen1Admission<'_>) {}

fn substitute(permit: MutatorPermit<'_>) {
    needs_admission(permit);
}

fn main() {}
