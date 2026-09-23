//! Owned catalog identity, shared by image requests and layout freshness.
//! Host-side machinery; this is not a port of GNU's Lisp hash-table keys.
use crate::emacs_core::value::{HashKey, HashTableTest, Value, list_to_vec};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Structural image-spec identity, including decoder-specific properties.
///
/// Lists, vectors and strings are captured explicitly: binary strings must not
/// fall back to pointer identity, and long property lists must not hit the
/// generic equal-key recursion limit. Flat tokens also make hashing, comparison
/// and destruction independent of the Lisp nesting depth.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageSpecIdentity(Arc<[Token]>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Token {
    Cons,
    Vector(usize),
    String { chars: usize, bytes: Arc<[u8]> },
    Cycle(usize),
    Other(HashKey),
}

enum Work {
    Visit(Value),
    Leave(usize),
}

#[cfg(test)]
mod tests;

impl ImageSpecIdentity {
    /// Snapshot a proper `(image ...)` specification. String contents follow
    /// GNU `equal`: character count and raw bytes, excluding text properties.
    /// Other Lisp object types keep their existing equal-key semantics.
    #[must_use]
    pub fn from_lisp_spec(spec: &Value) -> Option<Self> {
        let items = list_to_vec(spec)?;
        if items.first()?.as_symbol_name() != Some("image") {
            return None;
        }
        let mut pending = vec![Work::Visit(*spec)];
        let mut active = FxHashMap::default();
        let mut tokens = Vec::new();
        while let Some(work) = pending.pop() {
            let value = match work {
                Work::Leave(identity) => {
                    active.remove(&identity);
                    continue;
                }
                Work::Visit(value) => value,
            };
            if let Some(string) = value.as_lisp_string() {
                tokens.push(Token::String {
                    chars: string.schars(),
                    bytes: string.as_bytes().into(),
                });
            } else if value.is_cons() || value.is_vector() {
                if let Some(&ancestor) = active.get(&value.bits()) {
                    tokens.push(Token::Cycle(ancestor));
                    continue;
                }
                active.insert(value.bits(), active.len());
                pending.push(Work::Leave(value.bits()));
                if value.is_cons() {
                    tokens.push(Token::Cons);
                    pending.push(Work::Visit(value.cons_cdr()));
                    pending.push(Work::Visit(value.cons_car()));
                } else if let Some(elements) = value.as_vector_data() {
                    tokens.push(Token::Vector(elements.len()));
                    pending.extend(elements.iter().rev().copied().map(Work::Visit));
                }
            } else {
                tokens.push(Token::Other(value.to_hash_key(&HashTableTest::Equal)));
            }
        }
        Some(Self(tokens.into()))
    }
}
