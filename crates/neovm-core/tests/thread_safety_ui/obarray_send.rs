use neovm_core::emacs_core::Obarray;

fn require_send<T: Send>() {}

fn main() {
    // The owner stays on its mutator; only its admitted scan snapshot moves.
    require_send::<Obarray>();
}
