//! Conditional absence-only regression; explicit inputs have separate controls.
//! The pure None input reads no Lisp state and changes no process policy.
//! Independent test threads own their inputs, with no shared mutator state.
use super::parse_lazy_proof;

#[test]
fn lazy_proof_defaults_on_only_when_absent() {
    assert!(
        parse_lazy_proof(None),
        "absence alone selects the promoted Lazy default"
    );
}
