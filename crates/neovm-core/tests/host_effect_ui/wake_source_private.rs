//! External callers cannot construct a bare backend control source. The
//! constructor stays `pub(crate)` at its DEFINING crate path; the other
//! fixtures import real source to test the inner owner constraints separately.
use neovm_eval::host_effect::wake::ControlWake;

fn main() {
    let _unpaired_public_source = ControlWake::paired();
}
