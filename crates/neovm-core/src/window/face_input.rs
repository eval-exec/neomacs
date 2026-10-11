//! Owned syntactic dependencies of faces on prefix/replacement strings.
//! This does not resolve faces, evaluate filters or copy arbitrary Lisp graphs.
use crate::buffer::{CharPos0, text_props::TextPropertiesRef};
use crate::emacs_core::{
    plist::plist_get,
    value::{Value, list_to_vec},
};
use crate::face::{DecorationProperty, LFaceAttr};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StringFaceInputs(Arc<[FaceRun]>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct FaceRun {
    start: CharPos0,
    end: CharPos0,
    property: FaceProperty,
    input: Arc<[Token]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaceProperty {
    Face,
    FontLockFace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Atom(usize),
    String { bytes: Arc<[u8]>, multibyte: bool },
    List,
    Attributes,
    Filtered,
    Filter(Option<Arc<[usize]>>),
    Attribute(LFaceAttr),
    Decoration,
    DecorationProperty(DecorationProperty),
    WidthPair,
    InlineBitmap,
    End,
    Reference(usize),
}

enum Work {
    Face(Value),
    Attribute(LFaceAttr, Value),
    End,
}

impl StringFaceInputs {
    pub(super) fn capture(properties: TextPropertiesRef<'_>) -> Self {
        let mut runs = Vec::new();
        let keys = [
            (FaceProperty::Face, Value::symbol("face")),
            (FaceProperty::FontLockFace, Value::symbol("font-lock-face")),
        ];
        properties.for_each_interval_from_char_pos(CharPos0::new(0), |start, end, plist| {
            for (property, key) in keys {
                if let Some(value) = plist_get(plist, &key) {
                    runs.push(FaceRun {
                        start,
                        end,
                        property,
                        input: capture_face(value),
                    });
                }
            }
            true
        });
        Self(runs.into())
    }
}

fn atom(value: Value) -> Token {
    match value.as_lisp_string() {
        Some(string) => Token::String {
            bytes: string.as_bytes().into(),
            multibyte: string.is_multibyte(),
        },
        None => Token::Atom(value.bits()),
    }
}

fn capture_face(value: Value) -> Arc<[Token]> {
    let mut pending = vec![Work::Face(value)];
    let mut seen = FxHashMap::default();
    let mut tokens = Vec::new();
    while let Some(work) = pending.pop() {
        match work {
            Work::End => tokens.push(Token::End),
            Work::Attribute(key, value) => {
                tokens.push(Token::Attribute(key));
                if key == LFaceAttr::Inherit {
                    pending.push(Work::Face(value));
                } else if matches!(key, LFaceAttr::Box | LFaceAttr::Underline) {
                    capture_decoration(key, value, &mut tokens);
                } else if key == LFaceAttr::Stipple {
                    capture_stipple(value, &mut tokens);
                } else {
                    // Scalar/string operands are owned; compound font/resource
                    // payloads retain identity pending their audit.
                    tokens.push(atom(value));
                }
            }
            Work::Face(value) => {
                if !value.is_cons() {
                    tokens.push(atom(value));
                    continue;
                }
                if let Some(previous) = seen.insert(value.bits(), tokens.len()) {
                    tokens.push(Token::Reference(previous));
                    continue;
                }
                let Some(items) = list_to_vec(&value) else {
                    // The current resolver rejects improper/cyclic face lists.
                    tokens.push(Token::Atom(value.bits()));
                    continue;
                };
                if items[0]
                    .as_symbol_name()
                    .is_some_and(|name| matches!(name, "filtered" | ":filtered"))
                {
                    tokens.push(Token::Filtered);
                    if items.len() >= 3 {
                        // Filters inspect a flat predicate list. Window operands
                        // are compared by identity, not by their contents.
                        let filter = list_to_vec(&items[1])
                            .map(|items| items.into_iter().map(|value| value.bits()).collect());
                        tokens.push(Token::Filter(filter));
                        pending.push(Work::End);
                        // The resolver interprets the remaining tail as a face
                        // list/plist, including Neomacs' flattened spec syntax.
                        pending.push(Work::Face(value.cons_cdr().cons_cdr()));
                    }
                    continue;
                }
                if value
                    .cons_car()
                    .as_symbol_name()
                    .is_some_and(|name| name.starts_with(':'))
                {
                    tokens.push(Token::Attributes);
                    pending.push(Work::End);
                    for pair in items.chunks_exact(2).rev() {
                        if let Some(key) =
                            pair[0].as_symbol_name().and_then(LFaceAttr::from_keyword)
                        {
                            pending.push(Work::Attribute(key, pair[1]));
                        }
                    }
                } else {
                    tokens.push(Token::List);
                    pending.push(Work::End);
                    pending.extend(items.into_iter().rev().map(Work::Face));
                }
            }
        }
    }
    tokens.into()
}

fn capture_decoration(attribute: LFaceAttr, value: Value, tokens: &mut Vec<Token>) {
    if !value.is_cons() {
        tokens.push(atom(value));
        return;
    }
    let Some(items) = list_to_vec(&value) else {
        // The parser rejects improper/cyclic decoration plists.
        tokens.push(atom(value));
        return;
    };
    tokens.push(Token::Decoration);
    for pair in items.chunks_exact(2) {
        if let Some(property) = DecorationProperty::from_value(pair[0])
            && property.applies_to(attribute)
        {
            tokens.push(Token::DecorationProperty(property));
            let value = pair[1];
            if property == DecorationProperty::LineWidth && value.is_cons() {
                tokens.push(Token::WidthPair);
                tokens.push(atom(value.cons_car()));
                tokens.push(atom(value.cons_cdr()));
            } else {
                tokens.push(atom(value));
            }
        }
    }
    tokens.push(Token::End);
}

fn capture_stipple(value: Value, tokens: &mut Vec<Token>) {
    if let Some(items) = list_to_vec(&value)
        && let [width, height, data] = items.as_slice()
    {
        // The resolver consumes exactly (WIDTH HEIGHT DATA). Capture even
        // invalid operands so a mutation that makes them valid is observable.
        // Bitmap bytes are owned, not the string's text-property graph.
        tokens.push(Token::InlineBitmap);
        tokens.push(atom(*width));
        tokens.push(atom(*height));
        tokens.push(atom(*data));
        tokens.push(Token::End);
    } else {
        // File/builtin names use string contents; unsupported compound forms
        // retain identity. External file changes need resource invalidation.
        tokens.push(atom(value));
    }
}
