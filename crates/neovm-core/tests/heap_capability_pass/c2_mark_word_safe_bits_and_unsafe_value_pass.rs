//! value-gc C2 positive compiling control (G1): `MarkWord::NIL` and
//! `MarkWord::bits` stay safe while `MarkWord::of`, `MarkWord::value`,
//! `MarkStack::from_values`, `MarkStack::push`, and `MarkStack::pop` compile
//! under `unsafe`.
mod tagged {
    pub mod value {
        pub use neovm_value::tagged::value::TaggedValue;
    }
}

#[path = "../../../neovm-value/src/tagged/gc/mark_word.rs"]
mod mark_word;

fn main() {
    let nil_word = mark_word::MarkWord::nil();
    let _nil_bits: usize = nil_word.bits();
    // SAFETY: immediate `NIL` has no heap backing or owner lifetime.
    let word = unsafe { mark_word::MarkWord::of(tagged::value::TaggedValue::NIL) };
    let _bits: usize = word.bits();
    // SAFETY: immediate `NIL` has no heap backing or owner lifetime.
    let mut stack =
        unsafe { mark_word::MarkStack::from_values(vec![tagged::value::TaggedValue::NIL]) };
    // SAFETY: immediate `NIL` has no heap backing or owner lifetime.
    unsafe { stack.push(tagged::value::TaggedValue::NIL) };
    // SAFETY: immediate `NIL` on the constructing thread.
    let _val = unsafe { word.value() };
    // SAFETY: immediate `NIL` on the constructing thread.
    let _popped = unsafe { stack.pop() };
}
