//! These are the product types, not the standalone N1 sketches.
use neovm_core::tagged::gc::{
    Gen1Admission, Gen1AdmissionError, MutatorPermit, ParallelHeapEra, RegistrationError,
};

fn preserve<'heap>(admission: Gen1Admission<'heap>) -> Gen1Admission<'heap> {
    admission
}

fn borrow<'loan, 'heap>(admission: &'loan Gen1Admission<'heap>) -> &'loan Gen1Admission<'heap> {
    admission
}

fn register<'era, 'heap>(
    era: &'era ParallelHeapEra<'heap>,
) -> Result<MutatorPermit<'era>, RegistrationError> {
    era.register(1, 1)
}

fn refusal(error: Gen1AdmissionError) -> Option<RegistrationError> {
    match error {
        Gen1AdmissionError::GenerationalRequired => None,
        Gen1AdmissionError::Registration(reason) => Some(reason),
    }
}

fn main() {
    let _ = preserve;
    let _ = borrow;
    let _ = register;
    let _ = refusal;
}
