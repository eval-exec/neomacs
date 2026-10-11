//! Read-only candidate classification for P2.3's inline census.
//!
//! This is a selection census, not an inliner: it cannot change eligibility,
//! feedback, bytecode, or the function cells it inspects. Closure tags are
//! followed within a basic block; at joins only the existing fuser's agreed
//! constant tags are trusted, so unknown targets remain explicitly unknown.
//! Threading: all borrowed Lisp data belongs to the compiling mutator;
//! returned rows hold source ids and owned strings, never Lisp state.

use super::{analyze_cfg, site_verdict};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::compile::param_shape::JitParamShape;
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::{Value, ValueKind};

/// The statically recognized call shape, separate from its admission verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CensusShape {
    Constant,
    Named,
    Closure,
    Funcall,
    Hof,
    Dynamic,
    Apply,
}

impl CensusShape {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Constant => "constant",
            Self::Named => "named",
            Self::Closure => "closure",
            Self::Funcall => "funcall",
            Self::Hof => "hof",
            Self::Dynamic => "dynamic",
            Self::Apply => "apply",
        }
    }
}

/// One copied candidate-site verdict. No Lisp value is retained after the
/// compile probe returns; source ids remain meaningful after source GC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CensusSite {
    pub(crate) pc: usize,
    pub(crate) shape: CensusShape,
    pub(crate) target_source: Option<u64>,
    pub(crate) replay: Result<(), String>,
    pub(crate) v2: Result<(), String>,
}

/// Structural v1 eligibility from P2.3 §4.7, before hotness and growth
/// budgets. An executing callback need not be rejected for its pre-call
/// heat. The compiler-site probe adds that separate cold-target verdict.
pub(crate) fn census_callee_verdict(f: &ByteCodeFunction, nargs: usize) -> Result<(), String> {
    if f.env.is_some() {
        return Err("env".into());
    }
    let arity = JitParamShape::try_from(f)
        .ok()
        .and_then(JitParamShape::fixed_arity)
        .ok_or_else(|| "arglist".to_string())?;
    if arity != nargs {
        return Err("arity".into());
    }
    if nargs > 0 && !f.lexical && !matches!(f.arglist.kind(), ValueKind::Fixnum(_)) {
        return Err("dynamic-params".into());
    }
    if !f.executes_sealed_ops() || !f.executes_verified_ops() {
        return Err("unverified".into());
    }
    let ops = f.executable_ops();
    if ops.len() > 60 {
        return Err("size".into());
    }
    for op in ops {
        match op {
            Op::Switch => return Err("switch".into()),
            Op::PushConditionCase(_)
            | Op::PushConditionCaseRaw(_)
            | Op::PushCatch(_)
            | Op::PopHandler
            | Op::UnwindProtectPop => return Err("handlers".into()),
            _ => {}
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Tag {
    Unknown,
    Constant(u16),
    Closure(u16),
}

impl Tag {
    fn constant(self) -> Option<u16> {
        match self {
            Self::Constant(i) => Some(i),
            Self::Unknown | Self::Closure(_) => None,
        }
    }
}

fn constant_name(tag: Tag, constants: &[Value]) -> Option<&'static str> {
    constants
        .get(tag.constant()? as usize)?
        .as_symbol_id()
        .map(crate::emacs_core::intern::resolve_sym)
}

fn resolve_target(
    tag: Tag,
    constants: &[Value],
    obarray: Option<&Obarray>,
) -> Option<&'static ByteCodeFunction> {
    let i = match tag {
        Tag::Constant(i) | Tag::Closure(i) => i,
        Tag::Unknown => return None,
    };
    let mut value = *constants.get(i as usize)?;
    // Follow symbol aliases without invoking Lisp or autoloading. A cycle
    // or unusually long alias chain is reported as unresolved.
    for _ in 0..32 {
        if let Some(f) = value.bytecode_data_if_materialized() {
            return Some(f);
        }
        value = obarray?.symbol_function_id(value.as_symbol_id()?)?;
    }
    None
}

/// Probe every call in the original body, independent of the fuser knob
/// and MIR's acceptance. Repeated probes are diagnostic only.
pub(crate) fn census_sites(f: &ByteCodeFunction, obarray: Option<&Obarray>) -> Vec<CensusSite> {
    let ops = f.executable_ops();
    let Ok(params) = JitParamShape::try_from(f) else {
        return Vec::new();
    };
    let nargs = params.entry_depth();
    let cfg = analyze_cfg(ops, &f.constants, f.executable_gnu_byte_offset_map(), nargs).ok();
    let entries = cfg
        .as_ref()
        .map(|cfg| super::super::compile::spec_tag_entry_states(ops, &f.constants, &cfg.leaders));
    let mut tags = vec![Tag::Unknown; nargs];
    let mut rows = Vec::new();
    let mut handlers = 0;
    for (pc, op) in ops.iter().enumerate() {
        if let Some(cfg) = &cfg {
            if cfg.leaders.binary_search(&pc).is_ok() {
                tags = entries.as_ref().and_then(|e| e.get(&pc)).map_or_else(
                    || vec![Tag::Unknown; cfg.entry_depth.get(&pc).copied().unwrap_or(0)],
                    |entry| {
                        entry
                            .iter()
                            .map(|&t| t.map_or(Tag::Unknown, Tag::Constant))
                            .collect()
                    },
                );
                handlers = cfg.entry_handlers.get(&pc).map_or(0, Vec::len);
            }
        }
        let mut result_tag = Tag::Unknown;
        if let Op::Call(n) | Op::Apply(n) = op {
            let count = *n as usize;
            let fun_at = tags.len().checked_sub(count + 1);
            let fun = fun_at
                .and_then(|i| tags.get(i))
                .copied()
                .unwrap_or(Tag::Unknown);
            let name = constant_name(fun, &f.constants);
            let (shape, target, target_nargs) = if matches!(op, Op::Apply(_)) {
                (CensusShape::Apply, Tag::Unknown, 0)
            } else if matches!(name, Some("mapc" | "mapcar")) && count == 2 {
                (
                    CensusShape::Hof,
                    fun_at
                        .and_then(|i| tags.get(i + 1))
                        .copied()
                        .unwrap_or(Tag::Unknown),
                    1,
                )
            } else if name == Some("funcall") && count > 0 {
                (
                    CensusShape::Funcall,
                    fun_at
                        .and_then(|i| tags.get(i + 1))
                        .copied()
                        .unwrap_or(Tag::Unknown),
                    count - 1,
                )
            } else {
                let shape = match fun {
                    Tag::Closure(_) => CensusShape::Closure,
                    Tag::Constant(_) if name.is_some() => CensusShape::Named,
                    Tag::Constant(_) => CensusShape::Constant,
                    Tag::Unknown => CensusShape::Dynamic,
                };
                (shape, fun, count)
            };
            let callee = resolve_target(target, &f.constants, obarray);
            let replay = match (shape, callee) {
                (CensusShape::Constant, Some(callee)) => {
                    site_verdict(callee, target_nargs, f.constants.len()).map(|_| ())
                }
                _ => Err("shape".into()),
            };
            let v2 = if matches!(shape, CensusShape::Apply) {
                Err("apply".into())
            } else if handlers > 0 {
                Err("caller-handlers".into())
            } else if let Some(callee) = callee {
                if callee.jit_runtime().heat() == 0 {
                    Err("cold".into())
                } else {
                    census_callee_verdict(callee, target_nargs)
                }
            } else {
                Err("unresolved-target".into())
            };
            rows.push(CensusSite {
                pc,
                shape,
                target_source: callee.map(|f| f.source_id),
                replay,
                v2,
            });
            // make-closure's first arg is a template, not its result; keep
            // that provenance until the next basic-block join.
            if name == Some("make-closure") && count > 0 {
                if let Some(Tag::Constant(template)) = fun_at.and_then(|i| tags.get(i + 1)) {
                    result_tag = Tag::Closure(*template);
                }
            }
        }
        transfer(op, &mut tags, result_tag);
        match op {
            Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_) | Op::PushCatch(_) => {
                handlers += 1
            }
            Op::PopHandler => handlers = handlers.saturating_sub(1),
            _ => {}
        }
    }
    rows
}

fn transfer(op: &Op, tags: &mut Vec<Tag>, result: Tag) {
    match op {
        Op::Constant(i) => tags.push(Tag::Constant(*i)),
        Op::StackRef(n) => tags.push(
            tags.len()
                .checked_sub(*n as usize + 1)
                .and_then(|i| tags.get(i))
                .copied()
                .unwrap_or(Tag::Unknown),
        ),
        Op::Dup => tags.push(tags.last().copied().unwrap_or(Tag::Unknown)),
        Op::StackSet(n) => {
            if let Some(i) = tags.len().checked_sub(*n as usize + 1) {
                tags[i] = tags.last().copied().unwrap_or(Tag::Unknown);
            }
            tags.pop();
        }
        Op::DiscardN(raw) => {
            let top = tags.last().copied().unwrap_or(Tag::Unknown);
            tags.truncate(tags.len().saturating_sub((*raw & 0x7f) as usize));
            if raw & 0x80 != 0
                && let Some(last) = tags.last_mut()
            {
                *last = top;
            }
        }
        Op::Goto(_) | Op::PopHandler | Op::Unbind(_) | Op::PushConditionCase(_) => {}
        _ => match super::super::compile::simple_effect(op) {
            Ok((needs, delta)) => {
                tags.truncate(tags.len().saturating_sub(needs));
                tags.extend(std::iter::repeat_n(
                    result,
                    (needs as i64 + delta).max(0) as usize,
                ));
            }
            Err(_) => tags.clear(),
        },
    }
}
