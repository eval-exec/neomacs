//! External callers cannot construct a bare backend control source. This is
//! the aggregate module privacy barrier; the other fixtures import real source
//! to test the inner owner constraints separately.
use neovm_core::host_effect::wake::ControlWake;

fn main() {
    let _unpaired_public_source = ControlWake::paired();
}
