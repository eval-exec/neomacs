//! value-gc C2 regression guard (G1): `MarkWord::of`, `MarkStack::from_values`,
//! and `MarkStack::push` must require `unsafe` so safe code cannot detach a
//! `!Send` `TaggedValue` into a `Send + Sync + Copy` word or `Send` stack.
mod tagged {
    pub mod value {
        pub use neovm_value::tagged::value::TaggedValue;
    }
}

#[path = "../../../neovm-value/src/tagged/gc/mark_word.rs"]
mod mark_word;

fn main() {
    let _ = mark_word::MarkWord::of(tagged::value::TaggedValue::NIL);
    let mut stack = mark_word::MarkStack::from_values(vec![tagged::value::TaggedValue::NIL]);
    stack.push(tagged::value::TaggedValue::NIL);
}
