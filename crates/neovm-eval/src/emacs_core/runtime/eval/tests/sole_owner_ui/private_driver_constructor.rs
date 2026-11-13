// Name the real crate. A #[path] include would copy the module under this
// fixture's crate root, where pub(crate) means something else; the contract is
// that the constructor is private *across* the crate boundary.
use neovm_eval::emacs_core::eval::SoleOwnerDriver;

fn main() {
    let _forged = SoleOwnerDriver::new();
}
