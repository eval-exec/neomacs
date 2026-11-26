//! value-gc C2 regression guard (G1): `MarkWord::value` must require `unsafe`
//! so safe code cannot launder a `!Send` `TaggedValue` across threads.
mod tagged {
    pub mod value {
        pub use neovm_value::tagged::value::TaggedValue;
    }
}

#[path = "../../../neovm-value/src/tagged/gc/mark_word.rs"]
mod mark_word;

fn main() {
    let word = mark_word::MarkWord::nil();
    let _ = word.value();
}
