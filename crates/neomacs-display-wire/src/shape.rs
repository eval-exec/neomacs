//! The serde shape of a wire type (DESIGN §6.6 item 5).
//!
//! Bincode is positional: how a value's bytes are read is decided by the
//! shape of its type alone. That shape is every struct and field, every enum
//! and variant in declaration order, down to the primitives. The golden
//! corpus pins the bytes of sampled values. It cannot see a nested variant
//! that no sample takes: swapping two `CursorStyle` variants changes no
//! fixture byte. [`shape_of`] reads the shape itself, and the schema id
//! hashes it together with the corpus.
//!
//! The type is deserialized from a tracing `Deserializer`. The tracer records
//! what each `Deserialize` impl asks for, at its position in the value, and
//! answers with a placeholder: `false`, `1`, `1.0`, `'a'`, `"a"`, no bytes,
//! one element per sequence and map, `Some` for options. Each pass takes, at
//! every enum position, a variant not taken there yet, or one whose subtree
//! still has such a position. Passes repeat until every variant at every
//! position has been taken. A position that repeats an enclosing type (a menu
//! item's submenu) is cut: later passes give it no element, and the text
//! names the type it repeats.
//!
//! The tracer answers `is_human_readable` with `false`, as bincode does, so a
//! type that encodes differently for humans shows its binary shape.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::ops::Bound;

use serde::de::value::U32Deserializer;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

use crate::payload::PAYLOAD_NEWTYPE;

/// Why a type's shape could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ShapeError {
    /// A `Deserialize` impl refused the placeholder values, or asked for a
    /// self-describing read (`deserialize_any`) that bincode refuses too.
    #[error("at `{path}`: {message}")]
    Refused { path: String, message: String },
    /// A type repeats inside itself with no sequence, option or map between
    /// the two occurrences, so no placeholder value can end it.
    #[error("at `{path}`: `{name}` repeats inside itself with no sequence, option or map between")]
    UnboundedRecursion { path: String, name: &'static str },
    /// A pass took no new variant (a tracer defect, not a type's).
    #[error("no new variant after {passes} passes")]
    Stuck { passes: usize },
}

/// The read shape of `T`, as indented text.
pub fn shape_of<T: DeserializeOwned>() -> Result<String, ShapeError> {
    const MAX_PASSES: usize = 100_000;
    let mut tracer = Tracer::default();
    for passes in 1..=MAX_PASSES {
        tracer.begin_pass();
        let result = T::deserialize(TraceDe {
            tracer: &mut tracer,
        });
        match result {
            Ok(_) if tracer.open.is_empty() => return tracer.text(),
            Ok(_) if !tracer.progressed => return Err(ShapeError::Stuck { passes }),
            Ok(_) => {}
            Err(error) => match tracer.outcome.take() {
                Some(PassOutcome::Cut { path, name }) => {
                    if tracer.cuts.contains_key(&path) {
                        return Err(ShapeError::Stuck { passes });
                    }
                    tracer.cut(path, name);
                }
                Some(PassOutcome::Unbounded { path, name }) => {
                    return Err(ShapeError::UnboundedRecursion { path, name });
                }
                None => {
                    return Err(ShapeError::Refused {
                        path: tracer.failure.take().unwrap_or_default(),
                        message: error.0,
                    });
                }
            },
        }
    }
    Err(ShapeError::Stuck { passes: MAX_PASSES })
}

/// The error of one pass, as serde carries it.
#[derive(Debug)]
struct PassError(String);

impl fmt::Display for PassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PassError {}

impl de::Error for PassError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

/// A position that can hold no element, so a repeating type can stop there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cuttable {
    Seq,
    Option,
    Map,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VariantKind {
    Unit,
    Newtype,
    Tuple(usize),
    Struct(&'static [&'static str]),
}

/// What the `Deserialize` impl asked for at one position.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Primitive(&'static str),
    Option,
    Seq,
    Map,
    Tuple(usize),
    UnitStruct(&'static str),
    Newtype(&'static str),
    TupleStruct(&'static str, usize),
    Struct(&'static str, &'static [&'static str]),
    Enum {
        name: &'static str,
        variants: &'static [&'static str],
        taken: Vec<Option<VariantKind>>,
    },
    /// A cut position: it holds the named enclosing type again.
    Repeats(Cuttable, &'static str),
}

/// How a pass that failed on purpose ended.
enum PassOutcome {
    /// A type repeats inside itself; cut the position at `path`.
    Cut {
        path: String,
        name: &'static str,
    },
    Unbounded {
        path: String,
        name: &'static str,
    },
}

#[derive(Default)]
struct Tracer {
    /// Every position read so far, by path.
    nodes: BTreeMap<String, Node>,
    /// Enum positions with a variant not taken yet.
    open: BTreeSet<String>,
    /// Cut positions and the type each repeats.
    cuts: BTreeMap<String, &'static str>,
    path: String,
    /// Named types being read, with the path length where each begins.
    stack: Vec<(&'static str, usize)>,
    /// Cuttable positions being read, by path length.
    cuttable: Vec<usize>,
    outcome: Option<PassOutcome>,
    /// Where the first refusal of a pass arose.
    failure: Option<String>,
    /// Whether this pass took a variant not taken before.
    progressed: bool,
}

/// Whether `path` lies at or below `prefix` (a whole path segment).
fn under(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

impl Tracer {
    fn begin_pass(&mut self) {
        self.path.clear();
        self.stack.clear();
        self.cuttable.clear();
        self.outcome = None;
        self.failure = None;
        self.progressed = false;
    }

    fn push(&mut self, segment: &str) -> usize {
        let mark = self.path.len();
        self.path.push_str(segment);
        mark
    }

    /// Leave a position: note where a refusal arose, then restore the path.
    fn finish<T>(&mut self, mark: usize, result: Result<T, PassError>) -> Result<T, PassError> {
        if result.is_err() && self.failure.is_none() {
            self.failure = Some(self.path.clone());
        }
        self.path.truncate(mark);
        result
    }

    fn refuse(&mut self, message: &str) -> PassError {
        if self.failure.is_none() {
            self.failure = Some(self.path.clone());
        }
        PassError(message.to_owned())
    }

    /// Record what the current position reads; a position read again must
    /// read the same (an enum position keeps the variants taken so far).
    fn record(&mut self, node: Node) -> Result<(), PassError> {
        let Some(existing) = self.nodes.get(&self.path) else {
            if let Node::Enum { .. } = node {
                self.open.insert(self.path.clone());
            }
            self.nodes.insert(self.path.clone(), node);
            return Ok(());
        };
        let same = match (existing, &node) {
            (
                Node::Enum { name, variants, .. },
                Node::Enum {
                    name: again,
                    variants: again_variants,
                    ..
                },
            ) => name == again && variants == again_variants,
            (existing, node) => existing == node,
        };
        if same {
            Ok(())
        } else {
            let message = format!("the position reads {node:?} after {existing:?}");
            Err(self.refuse(&message))
        }
    }

    /// Begin reading the named type `name`; a type already being read
    /// repeats, and the innermost cuttable position since it began is cut.
    fn enter(&mut self, name: &'static str) -> Result<(), PassError> {
        if let Some(&(_, start)) = self.stack.iter().find(|(open, _)| *open == name) {
            let path = self.path.clone();
            self.outcome = Some(match self.cuttable.iter().rev().find(|len| **len > start) {
                Some(&len) => PassOutcome::Cut {
                    path: self.path[..len].to_owned(),
                    name,
                },
                None => PassOutcome::Unbounded { path, name },
            });
            return Err(PassError(format!("{name} repeats")));
        }
        self.stack.push((name, self.path.len()));
        Ok(())
    }

    fn leave(&mut self) {
        self.stack.pop();
    }

    /// Forget what a cut position held: later passes give it no element.
    fn cut(&mut self, path: String, name: &'static str) {
        let below: Vec<String> = self
            .nodes
            .range::<str, _>((Bound::Included(path.as_str()), Bound::Unbounded))
            .map(|(key, _)| key)
            .take_while(|key| key.starts_with(path.as_str()))
            .filter(|key| under(key, &path))
            .cloned()
            .collect();
        for key in below {
            self.nodes.remove(&key);
            self.open.remove(&key);
        }
        self.cuts.insert(path, name);
    }

    /// Whether an enum position at or below `prefix` has a variant not taken.
    fn open_under(&self, prefix: &str) -> bool {
        self.open
            .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
            .take_while(|path| path.starts_with(prefix))
            .any(|path| under(path, prefix))
    }

    /// The variant to take at the current enum position.
    fn choose(&self, variants: &'static [&'static str]) -> usize {
        if let Some(Node::Enum { taken, .. }) = self.nodes.get(&self.path)
            && let Some(index) = taken.iter().position(Option::is_none)
        {
            return index;
        }
        variants
            .iter()
            .position(|variant| self.open_under(&format!("{}::{variant}", self.path)))
            .unwrap_or(0)
    }

    /// The variant `index` of the enum at the current position was read.
    fn take(&mut self, index: usize, kind: VariantKind) {
        if let Some(Node::Enum { taken, .. }) = self.nodes.get_mut(&self.path)
            && taken[index].is_none()
        {
            taken[index] = Some(kind);
            self.progressed = true;
            if taken.iter().all(Option::is_some) {
                self.open.remove(&self.path);
            }
        }
    }

    /// The shape as indented text. A named type whose whole shape was
    /// already written is written again as its name only: the text elides
    /// exact repeats and nothing else.
    fn text(&self) -> Result<String, ShapeError> {
        let mut out = String::new();
        self.write("", "", 0, &mut out, &mut Some(BTreeSet::new()))?;
        Ok(out)
    }

    fn node(&self, path: &str) -> Result<&Node, ShapeError> {
        self.nodes.get(path).ok_or_else(|| ShapeError::Refused {
            path: path.to_owned(),
            message: "the position was never read".to_owned(),
        })
    }

    /// Write the shape at `path`. With `written`, a named type whose shape
    /// is in the set is written as its name only.
    fn write(
        &self,
        path: &str,
        label: &str,
        depth: usize,
        out: &mut String,
        written: &mut Option<BTreeSet<String>>,
    ) -> Result<(), ShapeError> {
        let node = self.node(path)?;
        let _ = write!(out, "{:width$}", "", width = depth * 2);
        if !label.is_empty() {
            let _ = write!(out, "{label}: ");
        }
        let named = match node {
            Node::Newtype(name) => Some(("newtype", *name)),
            Node::TupleStruct(name, _) => Some(("tuple struct", *name)),
            Node::Struct(name, _) => Some(("struct", *name)),
            Node::Enum { name, .. } => Some(("enum", *name)),
            _ => None,
        };
        if let (Some((kind, name)), Some(set)) = (named, written.as_ref()) {
            let mut whole = String::new();
            self.write(path, "", 0, &mut whole, &mut None)?;
            if set.contains(&whole) {
                let _ = writeln!(out, "{kind} {name} (as above)");
                return Ok(());
            }
            written.as_mut().map(|set| set.insert(whole));
        }
        let child = |segment: &str| format!("{path}{segment}");
        match node {
            Node::Primitive(name) => {
                let _ = writeln!(out, "{name}");
            }
            Node::Option => {
                let _ = writeln!(out, "option");
                self.write(&child("?"), "some", depth + 1, out, written)?;
            }
            Node::Seq => {
                let _ = writeln!(out, "seq");
                self.write(&child("[]"), "element", depth + 1, out, written)?;
            }
            Node::Map => {
                let _ = writeln!(out, "map");
                self.write(&child("{key}"), "key", depth + 1, out, written)?;
                self.write(&child("{value}"), "value", depth + 1, out, written)?;
            }
            Node::Tuple(len) => {
                let _ = writeln!(out, "tuple");
                for index in 0..*len {
                    let label = index.to_string();
                    self.write(
                        &child(&format!(".{index}")),
                        &label,
                        depth + 1,
                        out,
                        written,
                    )?;
                }
            }
            Node::UnitStruct(name) => {
                let _ = writeln!(out, "unit struct {name}");
            }
            Node::Newtype(name) => {
                let _ = writeln!(out, "newtype {name}");
                self.write(&child(".0"), "0", depth + 1, out, written)?;
            }
            Node::TupleStruct(name, len) => {
                let _ = writeln!(out, "tuple struct {name}");
                for index in 0..*len {
                    let label = index.to_string();
                    self.write(
                        &child(&format!(".{index}")),
                        &label,
                        depth + 1,
                        out,
                        written,
                    )?;
                }
            }
            Node::Struct(name, fields) => {
                let _ = writeln!(out, "struct {name}");
                for field in *fields {
                    self.write(&child(&format!(".{field}")), field, depth + 1, out, written)?;
                }
            }
            Node::Enum {
                name,
                variants,
                taken,
            } => {
                let _ = writeln!(out, "enum {name}");
                let indent = depth * 2 + 2;
                for (variant, kind) in variants.iter().zip(taken) {
                    let variant_path = child(&format!("::{variant}"));
                    match kind {
                        None => {
                            return Err(ShapeError::Refused {
                                path: variant_path,
                                message: "the variant was never taken".to_owned(),
                            });
                        }
                        Some(VariantKind::Unit) => {
                            let _ = writeln!(out, "{:indent$}{variant}: unit", "");
                        }
                        Some(VariantKind::Newtype) => {
                            self.write(&variant_path, variant, depth + 1, out, written)?;
                        }
                        Some(VariantKind::Tuple(len)) => {
                            let _ = writeln!(out, "{:indent$}{variant}: tuple", "");
                            for index in 0..*len {
                                let label = index.to_string();
                                let path = format!("{variant_path}.{index}");
                                self.write(&path, &label, depth + 2, out, written)?;
                            }
                        }
                        Some(VariantKind::Struct(fields)) => {
                            let _ = writeln!(out, "{:indent$}{variant}: struct", "");
                            for field in *fields {
                                let path = format!("{variant_path}.{field}");
                                self.write(&path, field, depth + 2, out, written)?;
                            }
                        }
                    }
                }
            }
            Node::Repeats(kind, name) => {
                let kind = match kind {
                    Cuttable::Seq => "seq",
                    Cuttable::Option => "option",
                    Cuttable::Map => "map",
                };
                let _ = writeln!(out, "{kind} of {name} (repeats)");
            }
        }
        Ok(())
    }
}

/// The tracing deserializer at the tracer's current position.
struct TraceDe<'t> {
    tracer: &'t mut Tracer,
}

macro_rules! primitive {
    ($($method:ident => $visit:ident($value:expr), $name:literal;)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, PassError> {
            self.tracer.record(Node::Primitive($name))?;
            let result = visitor.$visit($value);
            let mark = self.tracer.path.len();
            self.tracer.finish(mark, result)
        }
    )*};
}

impl<'de> de::Deserializer<'de> for TraceDe<'_> {
    type Error = PassError;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, PassError> {
        Err(self
            .tracer
            .refuse("a self-describing read (`deserialize_any`), which bincode refuses"))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, PassError> {
        Err(self
            .tracer
            .refuse("a self-describing read (`deserialize_ignored_any`), which bincode refuses"))
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, PassError> {
        Err(self
            .tracer
            .refuse("an identifier read (`deserialize_identifier`), which bincode refuses"))
    }

    primitive! {
        deserialize_bool => visit_bool(false), "bool";
        deserialize_i8 => visit_i8(1), "i8";
        deserialize_i16 => visit_i16(1), "i16";
        deserialize_i32 => visit_i32(1), "i32";
        deserialize_i64 => visit_i64(1), "i64";
        deserialize_i128 => visit_i128(1), "i128";
        deserialize_u8 => visit_u8(1), "u8";
        deserialize_u16 => visit_u16(1), "u16";
        deserialize_u32 => visit_u32(1), "u32";
        deserialize_u64 => visit_u64(1), "u64";
        deserialize_u128 => visit_u128(1), "u128";
        deserialize_f32 => visit_f32(1.0), "f32";
        deserialize_f64 => visit_f64(1.0), "f64";
        deserialize_char => visit_char('a'), "char";
        deserialize_str => visit_str("a"), "str";
        deserialize_string => visit_string("a".to_owned()), "str";
        deserialize_bytes => visit_bytes(&[]), "bytes";
        deserialize_byte_buf => visit_byte_buf(Vec::new()), "bytes";
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, PassError> {
        self.tracer.record(Node::Primitive("unit"))?;
        visitor.visit_unit()
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        if let Some(name) = tracer.cuts.get(&tracer.path).copied() {
            tracer.record(Node::Repeats(Cuttable::Option, name))?;
            return visitor.visit_none();
        }
        tracer.record(Node::Option)?;
        tracer.cuttable.push(tracer.path.len());
        let mark = tracer.push("?");
        let result = visitor.visit_some(TraceDe {
            tracer: &mut *tracer,
        });
        let result = tracer.finish(mark, result);
        tracer.cuttable.pop();
        result
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, PassError> {
        self.tracer.record(Node::UnitStruct(name))?;
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        tracer.record(Node::Newtype(name))?;
        tracer.enter(name)?;
        let result = if name == PAYLOAD_NEWTYPE {
            // A payload's wire form is opaque bytes; read as a one-element
            // sequence it holds the value, so the shape is the value's.
            visitor.visit_seq(Elements {
                tracer: &mut *tracer,
                labels: Labels::Indices,
                len: 1,
                index: 0,
            })
        } else {
            let mark = tracer.push(".0");
            let result = visitor.visit_newtype_struct(TraceDe {
                tracer: &mut *tracer,
            });
            tracer.finish(mark, result)
        };
        tracer.leave();
        result
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        if let Some(name) = tracer.cuts.get(&tracer.path).copied() {
            tracer.record(Node::Repeats(Cuttable::Seq, name))?;
            return visitor.visit_seq(Elements {
                tracer,
                labels: Labels::Element,
                len: 0,
                index: 0,
            });
        }
        tracer.record(Node::Seq)?;
        tracer.cuttable.push(tracer.path.len());
        let result = visitor.visit_seq(Elements {
            tracer: &mut *tracer,
            labels: Labels::Element,
            len: 1,
            index: 0,
        });
        tracer.cuttable.pop();
        result
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, PassError> {
        self.tracer.record(Node::Tuple(len))?;
        visitor.visit_seq(Elements {
            tracer: self.tracer,
            labels: Labels::Indices,
            len,
            index: 0,
        })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        tracer.record(Node::TupleStruct(name, len))?;
        tracer.enter(name)?;
        let result = visitor.visit_seq(Elements {
            tracer: &mut *tracer,
            labels: Labels::Indices,
            len,
            index: 0,
        });
        tracer.leave();
        result
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        if let Some(name) = tracer.cuts.get(&tracer.path).copied() {
            tracer.record(Node::Repeats(Cuttable::Map, name))?;
            return visitor.visit_map(Entries {
                tracer,
                remaining: 0,
            });
        }
        tracer.record(Node::Map)?;
        tracer.cuttable.push(tracer.path.len());
        let result = visitor.visit_map(Entries {
            tracer: &mut *tracer,
            remaining: 1,
        });
        tracer.cuttable.pop();
        result
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        tracer.record(Node::Struct(name, fields))?;
        tracer.enter(name)?;
        let result = visitor.visit_seq(Elements {
            tracer: &mut *tracer,
            labels: Labels::Fields(fields),
            len: fields.len(),
            index: 0,
        });
        tracer.leave();
        result
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, PassError> {
        let tracer = self.tracer;
        tracer.record(Node::Enum {
            name,
            variants,
            taken: vec![None; variants.len()],
        })?;
        if variants.is_empty() {
            return Err(tracer.refuse("an enum without variants has no value"));
        }
        tracer.enter(name)?;
        let index = tracer.choose(variants);
        let result = visitor.visit_enum(Variant {
            tracer: &mut *tracer,
            variants,
            index,
        });
        tracer.leave();
        result
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

/// How the elements of a sequence-shaped read are labelled.
#[derive(Clone, Copy)]
enum Labels {
    Element,
    Indices,
    Fields(&'static [&'static str]),
}

struct Elements<'t> {
    tracer: &'t mut Tracer,
    labels: Labels,
    len: usize,
    index: usize,
}

impl<'de> SeqAccess<'de> for Elements<'_> {
    type Error = PassError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, PassError> {
        if self.index == self.len {
            return Ok(None);
        }
        let segment = match self.labels {
            Labels::Element => "[]".to_owned(),
            Labels::Indices => format!(".{}", self.index),
            Labels::Fields(fields) => format!(".{}", fields[self.index]),
        };
        self.index += 1;
        let mark = self.tracer.push(&segment);
        let result = seed.deserialize(TraceDe {
            tracer: &mut *self.tracer,
        });
        self.tracer.finish(mark, result).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.len - self.index)
    }
}

struct Entries<'t> {
    tracer: &'t mut Tracer,
    remaining: usize,
}

impl<'de> MapAccess<'de> for Entries<'_> {
    type Error = PassError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, PassError> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let mark = self.tracer.push("{key}");
        let result = seed.deserialize(TraceDe {
            tracer: &mut *self.tracer,
        });
        self.tracer.finish(mark, result).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, PassError> {
        self.remaining -= 1;
        let mark = self.tracer.push("{value}");
        let result = seed.deserialize(TraceDe {
            tracer: &mut *self.tracer,
        });
        self.tracer.finish(mark, result)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

/// The variant a pass takes at one enum position.
struct Variant<'t> {
    tracer: &'t mut Tracer,
    variants: &'static [&'static str],
    index: usize,
}

impl<'de> EnumAccess<'de> for Variant<'_> {
    type Error = PassError;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self), PassError> {
        let index = u32::try_from(self.index).map_err(de::Error::custom)?;
        let identifier: U32Deserializer<PassError> = index.into_deserializer();
        let value = seed.deserialize(identifier)?;
        Ok((value, self))
    }
}

impl Variant<'_> {
    /// Read the variant's payload below `::Variant`, then mark it taken.
    fn read<T>(
        self,
        kind: VariantKind,
        payload: impl FnOnce(&mut Tracer) -> Result<T, PassError>,
    ) -> Result<T, PassError> {
        let mark = self
            .tracer
            .push(&format!("::{}", self.variants[self.index]));
        let result = payload(&mut *self.tracer);
        let value = self.tracer.finish(mark, result)?;
        self.tracer.take(self.index, kind);
        Ok(value)
    }
}

impl<'de> VariantAccess<'de> for Variant<'_> {
    type Error = PassError;

    fn unit_variant(self) -> Result<(), PassError> {
        self.tracer.take(self.index, VariantKind::Unit);
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, PassError> {
        self.read(VariantKind::Newtype, |tracer| {
            seed.deserialize(TraceDe { tracer })
        })
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, PassError> {
        self.read(VariantKind::Tuple(len), |tracer| {
            visitor.visit_seq(Elements {
                tracer,
                labels: Labels::Indices,
                len,
                index: 0,
            })
        })
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, PassError> {
        self.read(VariantKind::Struct(fields), |tracer| {
            visitor.visit_seq(Elements {
                tracer,
                labels: Labels::Fields(fields),
                len: fields.len(),
                index: 0,
            })
        })
    }
}
