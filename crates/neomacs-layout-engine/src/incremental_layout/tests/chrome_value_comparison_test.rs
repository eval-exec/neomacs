use super::super::{MAX_DEPTH, NODE_BUDGET, NotPlain, PlainEncoder, PropertyScope, tag};
use super::*;
use neovm_core::buffer::{CharLen, CharPos0};
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::emacs_char::{EmacsChar, MAX_5_BYTE_CHAR};
use neovm_core::emacs_core::value::{StringTextPropertyRun, Value, ValueKind, VecLikeType};
use neovm_core::heap_types::{LispMarker, LispString};
use strum::IntoEnumIterator;

fn detached_marker() -> Value {
    Value::make_marker(LispMarker::new(
        neovm_core::heap_types::MarkerBuffer::DETACHED,
        CharPos0::new(0),
        neovm_core::buffer::EmacsBytePos::new(0),
        false,
        false,
    ))
}

fn property_string(plist: Value) -> Value {
    let value = Value::string_with_text_properties(
        "same",
        vec![StringTextPropertyRun {
            start: 0,
            end: 4,
            plist: Value::list(vec![Value::symbol("fixture-property"), Value::T]),
        }],
    );
    // Normal fixture construction canonicalizes duplicate and malformed names.
    // Install the live raw plist through the existing interval cons cells so
    // the encoder itself must apply first-binding and refusal rules.
    let installed = {
        let string = value.as_lisp_string().expect("fixture string");
        string
            .intervals()
            .as_table()
            .expect("fixture interval table")
            .interval_plist_at_char_pos(CharPos0::ZERO)
            .expect("fixture interval plist")
    };
    assert!(plist.is_cons());
    installed.set_car(plist.cons_car());
    installed.set_cdr(plist.cons_cdr());
    value
}

#[test]
fn chrome_captured_memo_keys_match_core_equality_for_all_fixture_pairs() {
    let mut eval = Context::new();
    let positioned = neovm_core::tagged::gc::with_tagged_heap(|heap| {
        heap.alloc_symbol_with_pos(Value::symbol("chrome-key"), Value::fixnum(3))
    });
    let float = Value::make_float(f64::from_bits(0x7ff8_0000_0000_0001));
    let plain = Value::string("same");
    let mut values = vec![
        Value::NIL,
        Value::T,
        Value::fixnum(-17),
        Value::fixnum(0),
        Value::fixnum(17),
        Value::symbol("chrome-key"),
        Value::symbol("other-key"),
        positioned,
        Value::make_float(0.0),
        Value::make_float(-0.0),
        float,
        float,
        Value::make_float(f64::from_bits(0x7ff8_0000_0000_0001)),
        Value::make_float(f64::from_bits(0x7ff8_0000_0000_0002)),
        plain,
        plain,
        Value::string("same"),
        Value::multibyte_string("same"),
        Value::string("different"),
        Value::string("éλ文"),
        Value::string("éλ文"),
        Value::heap_string(LispString::from_unibyte(vec![0xc3, 0xa9])),
        Value::string("é"),
        property_string(Value::list(vec![
            Value::symbol("face"),
            Value::symbol("bold"),
        ])),
        property_string(Value::list(vec![
            Value::symbol("face"),
            Value::symbol("italic"),
        ])),
        Value::list(vec![Value::fixnum(1), Value::string("nested")]),
        Value::list(vec![Value::fixnum(1), Value::string("nested")]),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::vector(vec![Value::fixnum(1), Value::string("nested")]),
        Value::vector(vec![Value::fixnum(1), Value::string("nested")]),
        detached_marker(),
        detached_marker(),
        Value::make_surface_handle(5),
        Value::make_surface_handle(6),
    ];
    for character in [
        EmacsChar::from_byte8(0x80),
        EmacsChar::from_code(0x11_0000).expect("four-byte Emacs code"),
        EmacsChar::from_code(MAX_5_BYTE_CHAR).expect("five-byte Emacs code"),
    ] {
        values.push(Value::heap_string(LispString::from_emacs_bytes(
            character.to_emacs_bytes(),
        )));
        values.push(Value::heap_string(LispString::from_emacs_bytes(
            character.to_emacs_bytes(),
        )));
    }
    // Native allocation invokes no Lisp. Root every fixture before evaluator
    // operations; capture and equality perform no Lisp allocation or callback.
    eval.set_variable(
        "chrome-captured-key-fixtures",
        Value::vector(values.clone()),
    );
    let scope = MemoKeyScope::for_values(&values);
    let keys: Vec<_> = values.iter().map(|value| scope.capture(*value)).collect();
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(key.original().bits(), values[index].bits());
    }
    for (left_index, left) in values.iter().enumerate() {
        for (right_index, right) in values.iter().enumerate() {
            assert_eq!(
                keys[left_index].equivalent(&keys[right_index]),
                left == right,
                "pair ({left_index}, {right_index})",
            );
        }
    }
}

#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum ComparisonShape {
    DistinctStrings,
    IdenticalString,
    StringFixnum,
    FixnumString,
    OtherString,
    StringOther,
}

#[derive(Clone, Copy, Debug, strum::EnumIter)]
enum MutatedOwner {
    String,
    OtherString,
}

#[test]
fn chrome_captured_memo_keys_preserve_core_collection_read_dependencies() {
    let mut eval = Context::new();
    for shape in ComparisonShape::iter() {
        for mutated_owner in MutatedOwner::iter() {
            let string = Value::string("mutable");
            let other = Value::string("mutable");
            let (left, right) = match shape {
                ComparisonShape::DistinctStrings => (string, other),
                ComparisonShape::IdenticalString => (string, string),
                ComparisonShape::StringFixnum => (string, Value::fixnum(1)),
                ComparisonShape::FixnumString => (Value::fixnum(1), string),
                ComparisonShape::OtherString => (Value::make_float(1.0), string),
                ComparisonShape::StringOther => (string, Value::make_float(1.0)),
            };
            let values = [string, other, left, right];
            eval.set_variable(
                "chrome-captured-key-mutable",
                Value::vector(values.to_vec()),
            );
            let expected = left == right;
            let (_, structural) = neovm_core::tagged::collection_reads::capture(|| {
                assert_eq!(left == right, expected);
            });
            let (_, captured) = neovm_core::tagged::collection_reads::capture(|| {
                // Include scope creation and capture: eager string reads at
                // either boundary would change identity/mixed-pair freshness.
                let scope = MemoKeyScope::for_values(&values);
                let left = scope.capture(left);
                let right = scope.capture(right);
                assert_eq!(left.equivalent(&right), expected);
            });
            let structural = structural.expect("read-only core equality capture");
            let captured = captured.expect("read-only memo equality capture");
            let target = match mutated_owner {
                MutatedOwner::String => string,
                MutatedOwner::OtherString => other,
            };
            assert!(neovm_core::tagged::mutate::set_string_byte_same_char_count(
                target, 0, b'M'
            ));
            assert_eq!(
                captured.unchanged(),
                structural.unchanged(),
                "shape {shape:?}, mutated owner {mutated_owner:?}",
            );
        }
    }
}

/// Independent pre-change comparison oracle. Share byte/budget primitives,
/// never the production value/string_props recursion or memo-key comparison.
#[derive(Default)]
struct CoreEqualityEncoder {
    inner: PlainEncoder,
}

impl std::fmt::Debug for CoreEqualityEncoder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CoreEqualityEncoder")
            .field("output", &self.inner.out)
            .field("nodes", &self.inner.nodes)
            .finish()
    }
}

impl CoreEqualityEncoder {
    fn reference_value(&mut self, value: Value, depth: u32) -> Result<(), NotPlain> {
        self.inner.visit(depth)?;
        match value.kind() {
            ValueKind::Nil
            | ValueKind::T
            | ValueKind::Fixnum(_)
            | ValueKind::Symbol(_)
            | ValueKind::Subr(_) => {
                self.inner.out.push(tag::IMMEDIATE);
                self.inner.u64(value.bits() as u64);
                Ok(())
            }
            ValueKind::Float => {
                self.inner.out.push(tag::FLOAT);
                self.inner.u64(value.xfloat().to_bits());
                Ok(())
            }
            ValueKind::String => {
                let string = value.as_lisp_string().ok_or(NotPlain)?;
                self.inner.out.push(tag::STRING);
                self.inner.out.push(u8::from(string.is_multibyte()));
                self.inner.bytes(string.as_bytes());
                self.reference_string_props(string, PropertyScope::Nested, depth + 1)
            }
            ValueKind::Cons => {
                let mut cursor = value;
                while cursor.is_cons() {
                    self.inner.visit(depth)?;
                    self.inner.out.push(tag::CONS);
                    self.reference_value(cursor.cons_car(), depth + 1)?;
                    cursor = cursor.cons_cdr();
                }
                self.inner.out.push(tag::LIST_TAIL);
                self.reference_value(cursor, depth + 1)
            }
            ValueKind::Veclike(kind) => match kind {
                VecLikeType::Vector => {
                    let elements = value.as_vector_data().ok_or(NotPlain)?;
                    self.inner.out.push(tag::VECTOR);
                    self.inner.u64(elements.len() as u64);
                    for element in elements.iter() {
                        self.reference_value(*element, depth + 1)?;
                    }
                    Ok(())
                }
                VecLikeType::Bignum
                | VecLikeType::Marker
                | VecLikeType::Overlay
                | VecLikeType::Finalizer
                | VecLikeType::SymbolWithPos
                | VecLikeType::UserPtr
                | VecLikeType::Process
                | VecLikeType::Frame
                | VecLikeType::Window
                | VecLikeType::BoolVector
                | VecLikeType::Buffer
                | VecLikeType::HashTable
                | VecLikeType::Obarray
                | VecLikeType::Terminal
                | VecLikeType::WindowConfiguration
                | VecLikeType::Subr
                | VecLikeType::Xwidget
                | VecLikeType::XwidgetView
                | VecLikeType::Thread
                | VecLikeType::Mutex
                | VecLikeType::CondVar
                | VecLikeType::ModuleFunction
                | VecLikeType::Sqlite
                | VecLikeType::Lambda
                | VecLikeType::CharTable
                | VecLikeType::SubCharTable
                | VecLikeType::Record
                | VecLikeType::Font
                | VecLikeType::Macro
                | VecLikeType::ByteCode
                | VecLikeType::Timer
                | VecLikeType::SurfaceHandle
                | VecLikeType::VideoHandle => Err(NotPlain),
            },
            ValueKind::Unbound | ValueKind::Unknown => Err(NotPlain),
        }
    }

    fn reference_string_props(
        &mut self,
        string: &LispString,
        scope: PropertyScope,
        depth: u32,
    ) -> Result<(), NotPlain> {
        let intervals = string.intervals();
        if intervals.is_empty() {
            return Ok(());
        }
        let display = Value::symbol("display");
        let name_only = [
            Value::symbol("help-echo"),
            Value::symbol("local-map"),
            Value::symbol("keymap"),
        ];
        let runs = intervals.object_interval_plist_runs_for_char_len(CharLen::new(string.schars()));
        let mut properties: Vec<(Value, Value)> = Vec::new();
        for run in runs {
            let mut plist = run.plist();
            if plist.is_nil() {
                continue;
            }
            self.inner.visit(depth)?;
            self.inner.out.push(tag::RUN);
            self.inner.u64(run.start().get() as u64);
            self.inner.u64(run.end().get() as u64);
            properties.clear();
            while plist.is_cons() {
                self.inner.visit(depth)?;
                let name = plist.cons_car();
                let rest = plist.cons_cdr();
                let value = if rest.is_cons() {
                    rest.cons_car()
                } else {
                    Value::NIL
                };
                if !properties.iter().any(|(seen, _)| *seen == name) {
                    properties.push((name, value));
                }
                plist = if rest.is_cons() {
                    rest.cons_cdr()
                } else {
                    Value::NIL
                };
            }
            properties.sort_by_key(|(name, _)| name.bits());
            for &(name, value) in &properties {
                self.reference_value(name, depth + 1)?;
                match scope {
                    PropertyScope::Root => {
                        if name == display && !value.is_nil() {
                            return Err(NotPlain);
                        }
                        if name_only.contains(&name) {
                            self.inner.out.push(tag::NAME_ONLY);
                            continue;
                        }
                    }
                    PropertyScope::Nested => {}
                }
                self.reference_value(value, depth + 1)?;
            }
        }
        Ok(())
    }
}

fn encode_string(value: Value, scope: PropertyScope) -> Option<Box<[u8]>> {
    let mut encoder = PlainEncoder::default();
    encoder
        .string_props(value.as_lisp_string().expect("fixture string"), scope, 0)
        .ok()?;
    Some(encoder.finish())
}

fn encode_string_with_core_equality(value: Value, scope: PropertyScope) -> Option<Box<[u8]>> {
    let mut encoder = CoreEqualityEncoder::default();
    encoder
        .reference_string_props(value.as_lisp_string().expect("fixture string"), scope, 0)
        .ok()?;
    Some(encoder.inner.finish())
}

#[test]
fn chrome_captured_memo_keys_preserve_recursive_property_encoding_and_admission() {
    let mut eval = Context::new();
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");
    let nested = property_string(Value::list(vec![face, bold]));
    let nested_duplicates = property_string(Value::list(vec![
        Value::string("nested-key"),
        Value::vector(vec![nested]),
        Value::string("nested-key"),
        detached_marker(),
        face,
        bold,
        face,
        detached_marker(),
    ]));
    let circular = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    circular.cons_cdr().set_cdr(circular);
    let cyclic_plist = Value::list(vec![face, bold]);
    cyclic_plist.cons_cdr().set_cdr(cyclic_plist);
    let mut deep = Value::T;
    for _ in 0..=MAX_DEPTH {
        deep = Value::vector(vec![deep]);
    }
    let plain_lists = [
        Value::list(vec![face, bold, Value::symbol("mouse-face"), italic]),
        Value::list(vec![Value::symbol("mouse-face"), italic, face, bold]),
        Value::list(vec![face, bold, face, italic]),
        Value::list(vec![
            Value::string("key"),
            nested,
            Value::string("key"),
            Value::T,
        ]),
        Value::list(vec![
            Value::fixnum(-1),
            bold,
            Value::vector(vec![Value::T]),
            italic,
        ]),
        Value::cons(face, Value::fixnum(1)),
        Value::list(vec![Value::symbol("my-prop"), nested]),
        Value::list(vec![
            Value::symbol("my-prop"),
            Value::list(vec![Value::vector(vec![nested_duplicates])]),
        ]),
        Value::list(vec![Value::symbol("display"), Value::NIL]),
        Value::list(vec![
            Value::symbol("display"),
            Value::list(vec![Value::symbol("space")]),
        ]),
        Value::list(vec![Value::symbol("help-echo"), detached_marker()]),
        Value::list(vec![face, detached_marker()]),
        Value::list(vec![face, circular]),
        Value::list(vec![face, deep]),
        Value::list(vec![face, Value::vector(vec![Value::T; NODE_BUDGET])]),
        cyclic_plist,
    ];
    let strings: Vec<_> = plain_lists.into_iter().map(property_string).collect();
    eval.set_variable(
        "chrome-property-comparison-fixtures",
        Value::vector(strings.clone()),
    );
    let nested_expected =
        encode_string_with_core_equality(nested_duplicates, PropertyScope::Nested)
            .expect("the independent core reference skips shadowed opaque property values");
    assert_eq!(
        encode_string(nested_duplicates, PropertyScope::Nested).as_deref(),
        Some(nested_expected.as_ref()),
    );
    let mut accepted = 0;
    let mut refused = 0;
    for (index, value) in strings.iter().enumerate() {
        for scope in [PropertyScope::Root, PropertyScope::Nested] {
            let baseline = encode_string_with_core_equality(*value, scope);
            let captured = encode_string(*value, scope);
            assert_eq!(captured, baseline, "fixture {index}, scope {scope:?}");
            if baseline.is_some() {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    assert!(accepted > 0);
    assert!(refused > 0);
}
