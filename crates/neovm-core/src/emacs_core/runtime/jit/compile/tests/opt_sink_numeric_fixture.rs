//! Helpers for isolated Sink lowering regressions. Every semantic expectation
//! is observed from the sealed Tier-0 program before inspecting selected CLIF.
//! Contexts, roots, feedback snapshots and compiler overrides are test-local.

use super::super::compile_pipeline_tests::{captured_clif, function};
use super::super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::tier2::{CompileTier, T2Upgrade};
use crate::emacs_core::print::print_value;
use std::collections::{HashMap, HashSet};

pub(super) struct Settings;
impl Settings {
    pub(super) fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_flonum_mode_for_test(Some(FlonumMode::Resident));
        force_opt_passes_for_test(Some(OptPasses::default()));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_flonum_mode_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        force_deopt_for_test(false);
    }
}
pub(super) struct Roots(usize);
impl Roots {
    pub(super) fn enter() -> Self {
        Self(save_scratch_gc_roots())
    }
    pub(super) fn add(&self, values: &[Value]) {
        push_scratch_gc_roots(values);
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

pub(super) fn program(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    function(ops, constants, arity)
}
pub(super) fn tier0(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    args: &[Value],
) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(f, args.to_vec())
}
pub(super) fn resume(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    exit: &DeoptResume,
) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        f,
        Value::NIL,
        exit.pc,
        &exit.stack,
        exit.handlers,
        &exit.binds,
        exit.spec_base,
        exit.cond_base,
    )
}
pub(super) fn compile(
    ctx: &Context,
    f: &ByteCodeFunction,
    sink: bool,
    roots: &Roots,
) -> (CompiledLeaf, String) {
    force_opt_passes_for_test(Some(OptPasses {
        sink,
        ..OptPasses::default()
    }));
    // Only types actually observed by the preceding Tier-0 calls are published.
    let _feedback = publish_numeric_feedback(f);
    let mut leaf = None;
    let clif = captured_clif(|| {
        leaf = Some(
            compile_bytecode_function_requested(
                f,
                Some(&ctx.obarray),
                CompileRequest {
                    tier: CompileTier::Upgrade(T2Upgrade::Feedback),
                    regalloc: lowering::RegallocPolicy::Full,
                    bypass_profit_gate: true,
                    origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                },
            )
            .expect("verified semantic program is native-capable"),
        );
    });
    assert_eq!(clif.len(), 1);
    let leaf = leaf.unwrap();
    assert_eq!(leaf.tier(), LeafTier::Opt);
    roots.add(leaf.reloc_values());
    (leaf, clif.into_iter().next().unwrap())
}
pub(super) fn native_result(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    args: &[Value],
) -> Result<Value, Flow> {
    match leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), args) {
        NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
        NativeRun::DeoptAt(exit) => resume(ctx, f, &exit),
        NativeRun::Signal => Err(super::super::shims::take_pending_flow()
            .expect("native signal carries its actual pending Flow")),
        other => panic!("precise native success/replay required: {other:?}"),
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Observation {
    Fix(i64),
    Float(u64),
    Other(String),
    Signal(String, String, Option<String>),
}
pub(super) fn observe(value: &Result<Value, Flow>) -> Observation {
    match value {
        Ok(value) if value.is_fixnum() => Observation::Fix(value.as_fixnum().unwrap()),
        Ok(value) if value.is_float() => Observation::Float(value.xfloat().to_bits()),
        Ok(value) => Observation::Other(print_value(value)),
        Err(flow) => {
            let signal = flow.as_signal().expect("sealed numeric program signals");
            Observation::Signal(
                signal.symbol_name().to_owned(),
                print_value(&Value::list(signal.data.clone())),
                signal.raw_data.map(|value| print_value(&value)),
            )
        }
    }
}
pub(super) fn check_semantics(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    cases: &[Vec<Value>],
    roots: &Roots,
) {
    for args in cases {
        let expected = tier0(ctx, f, args);
        if let Ok(value) = &expected {
            roots.add(&[*value]);
        }
        let actual = native_result(ctx, f, leaf, args);
        if let Ok(value) = &actual {
            roots.add(&[*value]);
        }
        assert_eq!(observe(&actual), observe(&expected));
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

/// Parse actual CLIF instruction tokens, ignoring annotations and aliases.
/// These assertions measure compiler structure, not machine instruction cost.
struct Clif {
    entry: Option<String>,
    blocks: HashMap<String, Vec<String>>,
    constants: HashMap<String, (String, i64)>,
    aliases: HashMap<String, String>,
}
fn parse(clif: &str) -> Clif {
    let mut parsed = Clif {
        entry: None,
        blocks: HashMap::new(),
        constants: HashMap::new(),
        aliases: HashMap::new(),
    };
    let mut block = String::new();
    for line in clif.lines() {
        let line = line.split(';').next().unwrap().trim();
        if line.starts_with("block") {
            block = line.split(['(', ' ', ':']).next().unwrap().to_owned();
            parsed.entry.get_or_insert_with(|| block.clone());
            parsed.blocks.entry(block.clone()).or_default();
        } else if let Some((from, to)) = line.split_once(" -> ") {
            if from.starts_with('v') {
                parsed.aliases.insert(from.to_owned(), to.to_owned());
            }
        } else if !block.is_empty() && !line.is_empty() && line != "}" {
            if let Some((value, instruction)) = line.split_once(" = ") {
                let mut fields = instruction.split_whitespace();
                if let (Some(op), Some(bits)) = (fields.next(), fields.next()) {
                    if let Some(ty) = op.strip_prefix("iconst.") {
                        let bits = bits.replace('_', "");
                        let number = if let Some(hex) = bits.strip_prefix("0x") {
                            i64::from_str_radix(hex, 16).ok()
                        } else {
                            bits.parse::<i64>().ok()
                        };
                        if let Some(number) = number {
                            parsed
                                .constants
                                .insert(value.to_owned(), (ty.to_owned(), number));
                        }
                    }
                }
            }
            parsed.blocks.get_mut(&block).unwrap().push(line.to_owned());
        }
    }
    parsed
}
fn constant(parsed: &Clif, value: &str) -> Option<(String, i64)> {
    let mut value = value;
    let mut seen = HashSet::new();
    while let Some(next) = parsed.aliases.get(value) {
        if !seen.insert(value) {
            return None;
        }
        value = next;
    }
    parsed.constants.get(value).cloned()
}
fn branch(line: &str) -> Option<(&str, &str, &str)> {
    let args = line.strip_prefix("brif ")?.split(", ").collect::<Vec<_>>();
    if args.len() != 3 {
        return None;
    }
    Some((
        args[0],
        args[1].split('(').next()?,
        args[2].split('(').next()?,
    ))
}
fn simple_jump(lines: &[String]) -> bool {
    lines.len() == 1 && lines[0].starts_with("jump ")
}
pub(super) fn readiness_diamonds(clif: &str) -> usize {
    let parsed = parse(clif);
    parsed
        .blocks
        .values()
        .flat_map(|lines| lines.iter())
        .filter(|line| {
            let Some((flag, ready, borrowed)) = branch(line) else {
                return false;
            };
            if !matches!(constant(&parsed, flag), Some((ty, 0 | 1)) if ty == "i8") {
                return false;
            }
            let (Some(ready), Some(borrowed)) =
                (parsed.blocks.get(ready), parsed.blocks.get(borrowed))
            else {
                return false;
            };
            simple_jump(ready)
                && borrowed.iter().any(|line| line.starts_with("brif "))
                && borrowed.iter().any(|line| line.contains("band"))
        })
        .count()
}
pub(super) fn generic_fresh_cache_diamonds(clif: &str) -> usize {
    let parsed = parse(clif);
    parsed
        .blocks
        .values()
        .flat_map(|lines| lines.iter())
        .filter(|line| {
            let Some((flag, ready, borrowed)) = branch(line) else {
                return false;
            };
            if constant(&parsed, flag) != Some(("i8".to_owned(), 1)) {
                return false;
            }
            let (Some(ready), Some(borrowed)) =
                (parsed.blocks.get(ready), parsed.blocks.get(borrowed))
            else {
                return false;
            };
            simple_jump(borrowed)
                && ready.iter().any(|line| line.starts_with("brif "))
                && ready.iter().any(|line| line.contains("icmp ne"))
                && !ready.iter().any(|line| line.contains("band"))
        })
        .count()
}
pub(super) fn unused_snapshot_word_nil_constants(clif: &str) -> usize {
    let parsed = parse(clif);
    // Exclude ONLY the NIL definition in the ABI prologue's independently
    // identifiable complete common-immediate pool. Source/frame constants in
    // the entry or elsewhere are still counted. imm_pool_define runs before
    // authoritative IR emission; dropping an arbitrary entry block or all its
    // constants would conceal real source snapshot duplication.
    let pool = super::super::lowering::POOLED_IMMEDIATES;
    let pooled_nil = parsed
        .entry
        .as_ref()
        .and_then(|entry| parsed.blocks.get(entry))
        .and_then(|entry| {
            entry
                .windows(pool.len())
                .find(|window| {
                    window.iter().zip(pool).all(|(line, bits)| {
                        line.split_once(" = ")
                            .and_then(|(value, _)| constant(&parsed, value))
                            == Some(("i64".to_owned(), *bits))
                    })
                })
                .and_then(|window| {
                    window
                        .iter()
                        .zip(pool)
                        .find(|(_, bits)| **bits == Value::NIL.bits() as i64)
                        .and_then(|(line, _)| {
                            line.split_once(" = ").map(|(value, _)| value.to_owned())
                        })
                })
        });
    let mut used = HashSet::new();
    for line in parsed.blocks.values().flat_map(|lines| lines.iter()) {
        let operands = line.split_once(" = ").map_or(line.as_str(), |(_, rhs)| rhs);
        for token in operands.split(|c: char| !c.is_ascii_alphanumeric()) {
            if token.starts_with('v') && token[1..].chars().all(|c| c.is_ascii_digit()) {
                used.insert(token.to_owned());
            }
        }
    }
    for target in parsed.aliases.values() {
        used.insert(target.clone());
    }
    parsed
        .constants
        .iter()
        .filter(|(value, (ty, bits))| {
            ty == "i64"
                && *bits == Value::NIL.bits() as i64
                && !used.contains(*value)
                && pooled_nil.as_ref() != Some(*value)
        })
        .count()
}
pub(super) fn has_integer_multiply_arm(clif: &str) -> bool {
    clif.lines()
        .any(|line| line.split(';').next().unwrap().contains("sextend.i128"))
}
pub(super) fn has_sqrt(clif: &str) -> bool {
    clif.lines()
        .filter_map(|line| line.split_once(" = "))
        .filter_map(|(_, instruction)| instruction.split_whitespace().next())
        .any(|op| matches!(op, "sqrt" | "sqrt.f64"))
}
