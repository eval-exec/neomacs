//! value-gc C2 regression guard (G1): `MarkStack::pop` must require `unsafe`
//! because `MarkStack: Send` would otherwise launder `!Send` `TaggedValue`.
mod tagged {
    pub mod value {
        pub use neovm_value::tagged::value::TaggedValue;
    }
}

#[path = "../../../neovm-value/src/tagged/gc/mark_word.rs"]
mod mark_word;

fn main() {
    let mut stack = mark_word::MarkStack::default();
    let _ = stack.pop();
}
