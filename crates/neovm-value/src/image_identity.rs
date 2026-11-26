//! Owned catalog identity, shared by image requests and layout freshness.
//! Host-side machinery; this is not a port of GNU's Lisp hash-table keys.
use crate::value::{HashKey, HashTableTest, Value, list_to_vec};
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

/// Animation bookkeeping properties that GNU `filter_image_spec`
/// (`src/image.c:2259-2288`) excludes from image cache identity.
///
/// `image-animate-timeout` mutates `:animate-tardiness` on every tick and
/// `image-animate` stores `:animate-buffer` / `:animate-position`; none of
/// them affect display, and hashing them breaks the image cache on every
/// animation frame (#823).
fn is_animation_bookkeeping_key(key: Value) -> bool {
    matches!(
        key.as_bare_symbol_name(),
        Some(":animate-buffer" | ":animate-tardiness" | ":animate-position")
    )
}

fn push_value_tokens(root: Value, tokens: &mut Vec<Token>) {
    let mut pending = vec![Work::Visit(root)];
    let mut active = FxHashMap::default();
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
                pending.extend(elements.iter().rev().map(Work::Visit));
            }
        } else {
            tokens.push(Token::Other(value.to_hash_key(&HashTableTest::Equal)));
        }
    }
}

impl ImageSpecIdentity {
    /// Snapshot a proper `(image ...)` specification. String contents follow
    /// GNU `equal`: character count and raw bytes, excluding text properties.
    /// Other Lisp object types keep their existing equal-key semantics.
    ///
    /// Top-level animation bookkeeping pairs (`:animate-buffer`,
    /// `:animate-tardiness`, `:animate-position`) are omitted, matching GNU's
    /// `filter_image_spec` (`src/image.c:2259-2288`).
    #[must_use]
    pub fn from_lisp_spec(spec: &Value) -> Option<Self> {
        let items = list_to_vec(spec)?;
        if items.first()?.as_bare_symbol_name() != Some("image") {
            return None;
        }
        let mut tokens = Vec::new();
        tokens.push(Token::Cons);
        push_value_tokens(items[0], &mut tokens);
        let mut index = 1;
        while index + 1 < items.len() {
            let key = items[index];
            let value = items[index + 1];
            index += 2;
            if is_animation_bookkeeping_key(key) {
                continue;
            }
            tokens.push(Token::Cons);
            push_value_tokens(key, &mut tokens);
            tokens.push(Token::Cons);
            push_value_tokens(value, &mut tokens);
        }
        if index < items.len() {
            tokens.push(Token::Cons);
            push_value_tokens(items[index], &mut tokens);
        }
        tokens.push(Token::Other(Value::NIL.to_hash_key(&HashTableTest::Equal)));
        Some(Self(tokens.into()))
    }
}
