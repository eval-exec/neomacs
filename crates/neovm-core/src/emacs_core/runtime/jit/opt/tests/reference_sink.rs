//! Selected recipe reference execution. All state is invocation-owned; physical
//! fields/cache live in the ordinary SSA Cell array. Tokens distinguish fresh
//! dynamic instances and never key a persistent payload-to-box cache. Source
//! observations print/freeze scalars without allocating Lisp boxes; actual cold
//! snapshots materialize once per identity in a local reconstruction graph.

use super::{Cell, EvalError, Evaluator, Snapshot, TypedStep, invalid};
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::compile::lowering::UNBOXED_FLOAT_TAG_WORD;
use crate::emacs_core::jit::opt::{ir::*, sink_recipes::*};
use crate::emacs_core::value::Value as LispValue;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Token {
    Fresh(u64),
    Borrowed(ValueBits),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecipeTrace {
    pub(crate) pc: u32,
    pub(crate) printed_stack: String,
    pub(crate) handlers: u16,
    pub(crate) binds: u16,
    /// Tokens/raw bits can be compared but never dereferenced after the point.
    pub(crate) identities: Vec<Option<Token>>,
}

#[derive(Clone, Copy)]
struct Numeric {
    payload: u64,
    word: ValueBits,
    ready: bool,
    real_box: ValueBits,
}
#[derive(Clone, Copy)]
enum Pending {
    Number(Numeric),
    Cons(ValueBits),
}

#[derive(Default)]
pub(super) struct State {
    next: u64,
    /// Compiler-op-local tuple, only between producer and contiguous fields.
    /// Removed when RealBox (the final projection) is written to real SSA.
    pending: HashMap<Value, Pending>,
}

impl State {
    fn fresh(&mut self) -> Token {
        let token = Token::Fresh(self.next);
        self.next += 1;
        token
    }
}

fn nil() -> ValueBits {
    ValueBits::from_value(LispValue::NIL)
}
fn marker() -> ValueBits {
    ValueBits(UNBOXED_FLOAT_TAG_WORD as u64)
}

impl Evaluator<'_, '_> {
    fn recipe_version(&self, owner: Value) -> Result<&RecipeVersion, EvalError> {
        let owner = self
            .func
            .resolve(owner)
            .ok_or_else(|| invalid("recipe alias cycle"))?;
        let verified = self
            .recipe_verified
            .as_ref()
            .ok_or_else(|| invalid("missing recipe capability"))?;
        let id = verified
            .version_at(self.current_point, owner)
            .ok_or_else(|| {
                invalid(format!(
                    "no recipe version at {:?} for {owner:?}",
                    self.current_point
                ))
            })?;
        verified
            .version(id)
            .ok_or_else(|| invalid("recipe version ID"))
    }

    fn recipe_number(&self, owner: Value) -> Result<Numeric, EvalError> {
        let RecipeFields::Number(fields) = self.recipe_version(owner)?.fields else {
            return Err(invalid("number requested from Cons recipe"));
        };
        let payload = match self.read(fields.payload)? {
            Cell::F64 { bits, .. } => bits,
            _ => return Err(invalid("recipe payload field")),
        };
        let word = match self.read(fields.word)? {
            Cell::RawWord(bits) => bits,
            _ => return Err(invalid("recipe opaque word field")),
        };
        let ready = match self.read(fields.ready)? {
            Cell::Bool(value) => value,
            _ => return Err(invalid("recipe readiness field")),
        };
        let real_box = match self.read(fields.real_box)? {
            Cell::Lisp(bits) => bits,
            _ => return Err(invalid("recipe real-box field")),
        };
        Ok(Numeric {
            payload,
            word,
            ready,
            real_box,
        })
    }

    /// Borrowed values are guarded here, before any Float pointer load. Unsupported
    /// numbers/markers/bignums deopt to the exact original operation's frame.
    fn resolved_number(&self, owner: Value) -> Result<Option<(bool, i64, f64)>, EvalError> {
        let number = self.recipe_number(owner)?;
        if number.ready {
            if number.word == marker() {
                return Ok(Some((true, 0, f64::from_bits(number.payload))));
            }
            return Ok(number
                .word
                .to_value()
                .as_fixnum()
                .map(|fix| (false, fix, fix as f64)));
        }
        if number.word != number.real_box {
            return Err(invalid("borrow word/box mismatch"));
        }
        let original = number.real_box.to_value();
        if let Some(fix) = original.as_fixnum() {
            return Ok(Some((false, fix, fix as f64)));
        }
        if original.is_float() {
            return Ok(Some((true, 0, original.xfloat())));
        }
        Ok(None)
    }

    pub(super) fn recipe_inst(&mut self, id: Inst) -> Result<Option<TypedStep>, EvalError> {
        if self.func.sink_recipes.owners.is_empty() {
            return Ok(None);
        }
        let inst = &self.func.insts[id.index()];
        let result = inst.result;
        let cell = match &inst.op {
            Opcode::Sink(SinkOp::BorrowNum) => {
                let original = ValueBits::from_value(self.lisp(inst.args[0])?);
                let owner = result.ok_or_else(|| invalid("Borrow without result"))?;
                self.recipe_state.pending.insert(
                    owner,
                    Pending::Number(Numeric {
                        payload: 0.0f64.to_bits(),
                        word: original,
                        ready: false,
                        real_box: original,
                    }),
                );
                Some(Cell::Recipe(Token::Borrowed(original)))
            }
            Opcode::Sink(SinkOp::SourceNum(op)) => {
                // Match shared native guard order. Guard failure replays original
                // source, so GNU determines actual wrong-type/error precedence.
                let Some(right) = self.resolved_number(inst.args[1])? else {
                    return self.deopt(inst).map(Some);
                };
                let Some(left) = self.resolved_number(inst.args[0])? else {
                    return self.deopt(inst).map(Some);
                };
                let number = if !left.0 && !right.0 {
                    let a = i128::from(left.1);
                    let b = i128::from(right.1);
                    let value = match op {
                        Op::Add => a + b,
                        Op::Sub => a - b,
                        Op::Mul => a * b,
                        Op::Div if b != 0 => a / b,
                        Op::Div => return self.deopt(inst).map(Some),
                        _ => return Err(invalid("unsupported numeric source")),
                    };
                    if !(i128::from(LispValue::MOST_NEGATIVE_FIXNUM)
                        ..=i128::from(LispValue::MOST_POSITIVE_FIXNUM))
                        .contains(&value)
                    {
                        return self.deopt(inst).map(Some);
                    }
                    let word = ValueBits::from_value(LispValue::fixnum(value as i64));
                    Numeric {
                        payload: (value as f64).to_bits(),
                        word,
                        ready: true,
                        real_box: word,
                    }
                } else {
                    let a = left.2;
                    let b = right.2;
                    let value = match op {
                        Op::Add => a + b,
                        Op::Sub => a - b,
                        Op::Mul => a * b,
                        Op::Div => {
                            let q = a / b;
                            if q.is_nan() && !a.is_nan() && !b.is_nan() {
                                f64::NAN.copysign(-1.0)
                            } else {
                                q
                            }
                        }
                        _ => return Err(invalid("unsupported Float source")),
                    };
                    Numeric {
                        payload: value.to_bits(),
                        word: marker(),
                        ready: true,
                        real_box: nil(),
                    }
                };
                let owner = result.ok_or_else(|| invalid("source without result"))?;
                self.recipe_state
                    .pending
                    .insert(owner, Pending::Number(number));
                Some(Cell::Recipe(self.recipe_state.fresh()))
            }
            Opcode::Sink(SinkOp::SourceSqrt) => {
                // Func carries admission PCs, not the actual callee/builtin
                // witness. Always replay original Call(1) with exact frozen
                // recipe stack; native independently validates its real witness.
                return self.deopt(inst).map(Some);
            }
            Opcode::Sink(SinkOp::SourceCons(_)) => {
                let owner = result.ok_or_else(|| invalid("Cons source without result"))?;
                self.recipe_state
                    .pending
                    .insert(owner, Pending::Cons(nil()));
                Some(Cell::Recipe(self.recipe_state.fresh()))
            }
            Opcode::Sink(SinkOp::RecipeField(component)) => {
                let owner = inst.args[0];
                let pending = *self
                    .recipe_state
                    .pending
                    .get(&owner)
                    .ok_or_else(|| invalid("projection missing contiguous producer tuple"))?;
                let cell = match (pending, component) {
                    (Pending::Number(number), RecipeField::Payload) => Cell::F64 {
                        bits: number.payload,
                        identity: 0,
                    },
                    (Pending::Number(number), RecipeField::Word) => Cell::RawWord(number.word),
                    (Pending::Number(number), RecipeField::Ready) => Cell::Bool(number.ready),
                    (Pending::Number(number), RecipeField::RealBox) => Cell::Lisp(number.real_box),
                    (Pending::Cons(boxed), RecipeField::RealBox) => Cell::Lisp(boxed),
                    _ => return Err(invalid("projection kind/field mismatch")),
                };
                if *component == RecipeField::RealBox {
                    self.recipe_state.pending.remove(&owner);
                }
                Some(cell)
            }
            Opcode::Sink(SinkOp::MaterializeNum | SinkOp::MaterializeCons) => {
                let mut graph = HashMap::new();
                let boxed = self.recipe_materialize(inst.args[0], &mut graph, 0)?;
                Some(Cell::Lisp(ValueBits::from_value(boxed)))
            }
            Opcode::Sink(SinkOp::CacheBoxAfter) => Some(self.read(inst.args[2])?),
            Opcode::Refine(_)
                if result
                    .is_some_and(|owner| self.func.sink_recipes.owners.contains_key(&owner)) =>
            {
                let owner = result.unwrap();
                let input = inst.args[0];
                let cell = self.read(input)?;
                if !matches!(cell, Cell::Recipe(_)) {
                    return Err(invalid("logical view input"));
                }
                let definition = self.func.sink_recipes.owners[&owner].definition_version;
                let fields = self.func.sink_recipes.versions[definition.0 as usize].fields;
                if let RecipeFields::Number(fields) = fields {
                    if matches!(self.func.values[fields.payload.index()].def, ValueDef::Inst(projection)
                        if self.func.insts[projection.index()].args == [owner])
                    {
                        let tuple = self.recipe_number(input)?;
                        self.recipe_state
                            .pending
                            .insert(owner, Pending::Number(tuple));
                    }
                }
                Some(cell)
            }
            _ => return Ok(None),
        };
        Ok(Some(TypedStep::Cell(cell)))
    }

    fn recipe_materialize(
        &self,
        value: Value,
        graph: &mut HashMap<Token, LispValue>,
        depth: usize,
    ) -> Result<LispValue, EvalError> {
        if depth > self.func.values.len() {
            return Err(invalid("cyclic cold recipe graph"));
        }
        let value = self
            .func
            .resolve(value)
            .ok_or_else(|| invalid("cold alias cycle"))?;
        let Cell::Recipe(token) = self.read(value)? else {
            return self.lisp(value);
        };
        if let Some(&word) = graph.get(&token) {
            return Ok(word);
        }
        let fields = self.recipe_version(value)?.fields;
        let boxed = match fields {
            RecipeFields::Number(_) => {
                let number = self.recipe_number(value)?;
                if !number.ready || number.word != marker() {
                    number.real_box.to_value()
                } else if !number.real_box.to_value().is_nil() {
                    number.real_box.to_value()
                } else {
                    LispValue::make_float(f64::from_bits(number.payload))
                }
            }
            RecipeFields::Cons(fields) => {
                let Cell::Lisp(boxed) = self.read(fields.real_box)? else {
                    return Err(invalid("Cons box field"));
                };
                if !boxed.to_value().is_nil() {
                    boxed.to_value()
                } else {
                    let car = self.recipe_materialize(fields.car, graph, depth + 1)?;
                    let cdr = self.recipe_materialize(fields.cdr, graph, depth + 1)?;
                    LispValue::cons(car, cdr)
                }
            }
        };
        graph.insert(token, boxed);
        Ok(boxed)
    }

    pub(super) fn recipe_snapshot(
        &self,
        pc: u32,
        stack: &[Value],
        handlers: u16,
        binds: u16,
    ) -> Result<Snapshot, EvalError> {
        let mut graph = HashMap::new();
        let stack = stack
            .iter()
            .map(|&value| {
                self.recipe_materialize(value, &mut graph, 0)
                    .map(ValueBits::from_value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let items = stack
            .iter()
            .map(|word| crate::emacs_core::print::print_value(&word.to_value()))
            .collect::<Vec<_>>();
        let printed_stack = if items.is_empty() {
            "nil".into()
        } else {
            format!("({})", items.join(" "))
        };
        Ok(Snapshot {
            pc,
            stack,
            printed_stack,
            handlers,
            binds,
        })
    }

    pub(super) fn root_recipe_current(&mut self) -> Result<(), EvalError> {
        let inst = &self.func.insts[self.current_inst];
        let frame = inst
            .frame
            .ok_or_else(|| invalid("recipe safepoint without original frame"))?;
        let verified = self
            .recipe_verified
            .as_ref()
            .ok_or_else(|| invalid("recipe root capability"))?;
        let roots = roots_at(
            self.func,
            verified,
            self.current_point,
            frame,
            &self.live_before[self.current_inst],
        )
        .map_err(|error| invalid(format!("recipe roots: {error:?}")))?;
        for value in roots {
            let Cell::Lisp(bits) = self.read(value)? else {
                return Err(invalid("nonTagged recipe root"));
            };
            let value = bits.to_value();
            if value.is_heap_object() {
                self.ctx.bc_buf.push(value);
            }
        }
        Ok(())
    }

    fn recipe_print(&self, value: Value, depth: usize) -> Result<String, EvalError> {
        if depth > self.func.values.len() {
            return Err(invalid("cyclic diagnostic recipe"));
        }
        let value = self
            .func
            .resolve(value)
            .ok_or_else(|| invalid("trace alias cycle"))?;
        if let Cell::F64 { bits, .. } = self.read(value)? {
            return Ok(crate::emacs_core::print::format_float(f64::from_bits(bits)));
        }
        if !matches!(self.read(value)?, Cell::Recipe(_)) {
            return Ok(crate::emacs_core::print::print_value(&self.lisp(value)?));
        }
        match self.recipe_version(value)?.fields {
            RecipeFields::Number(_) => {
                let number = self.recipe_number(value)?;
                if !number.ready || number.word != marker() || !number.real_box.to_value().is_nil()
                {
                    Ok(crate::emacs_core::print::print_value(
                        &number.real_box.to_value(),
                    ))
                } else {
                    Ok(crate::emacs_core::print::format_float(f64::from_bits(
                        number.payload,
                    )))
                }
            }
            RecipeFields::Cons(fields) => {
                let Cell::Lisp(boxed) = self.read(fields.real_box)? else {
                    return Err(invalid("trace Cons cache"));
                };
                if !boxed.to_value().is_nil() {
                    return Ok(crate::emacs_core::print::print_value(&boxed.to_value()));
                }
                let car = self.recipe_print(fields.car, depth + 1)?;
                let cdr = self.recipe_print(fields.cdr, depth + 1)?;
                if cdr == "nil" {
                    Ok(format!("({car})"))
                } else if cdr.starts_with('(') && cdr.ends_with(')') {
                    Ok(format!("({car} {})", &cdr[1..cdr.len() - 1]))
                } else {
                    Ok(format!("({car} . {cdr})"))
                }
            }
        }
    }

    pub(super) fn recipe_observe(
        &self,
        pc: u32,
        stack: &[Value],
        handlers: u16,
        binds: u16,
    ) -> Result<RecipeTrace, EvalError> {
        let items = stack
            .iter()
            .map(|&value| self.recipe_print(value, 0))
            .collect::<Result<Vec<_>, _>>()?;
        let identities = stack
            .iter()
            .map(|&value| match self.read(value)? {
                Cell::Recipe(token) => Ok(Some(token)),
                Cell::Lisp(bits) if bits.to_value().is_heap_object() => {
                    Ok(Some(Token::Borrowed(bits)))
                }
                _ => Ok(None),
            })
            .collect::<Result<Vec<_>, EvalError>>()?;
        Ok(RecipeTrace {
            pc,
            printed_stack: if items.is_empty() {
                "nil".into()
            } else {
                format!("({})", items.join(" "))
            },
            handlers,
            binds,
            identities,
        })
    }
}
