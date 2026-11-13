//! S-1: the EvalHooksInstalled boot proof is crate authority, not public API.
//! The defining path is named directly; a safe call must stay E0624.
use neovm_eval::emacs_core::eval::hooks_proof::EvalHooksInstalled;

fn main() {
    let _ = EvalHooksInstalled::mint();
}
