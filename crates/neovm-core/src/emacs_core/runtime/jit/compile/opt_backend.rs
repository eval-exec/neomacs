//! The opt front and shared-emitter adapter. Threading: plans and SSA variables
//! belong to one compilation; diagnostic output contains only owned text and
//! counters and is serialized across mutators. Lisp constants stay rooted by
//! the synchronous source/front owner and are never dereferenced by IR code.

use super::*;
use crate::emacs_core::jit::opt::{build, ir};

#[cfg(test)]
#[path = "tests/opt_opcode_transport_effects_test.rs"]
mod opcode_transport_effects_tests;

/// Honor a Feedback request's explicit Full policy when the verified opt plan
/// transports at least two parameters simultaneously at an actual join. Entry
/// parameters and serial single-parameter joins retain the outer allocator
/// policy. Enter before ISA/module selection and retain this scope through final
/// leaf metadata; a refused lowering drops it before the baseline retry. Forced
/// allocator configuration still wins. Threading: the plan and copied request
/// belong to this compilation; no request TLS is published. The existing
/// allocator scope restores the enclosing choice on drop. Backend workers
/// receive the resulting allocator through their existing owned job payload.
pub(super) fn quality_scope(
    plan: &ir::Func,
    osr_pc: Option<usize>,
    request: CompileRequest,
) -> Option<lowering::RegallocScope> {
    if osr_pc.is_some()
        || request.regalloc != lowering::RegallocPolicy::Full
        || request.tier
            != super::super::tier2::CompileTier::Upgrade(super::super::tier2::T2Upgrade::Feedback)
        || !plan
            .blocks
            .iter()
            .any(|block| block.preds.len() >= 2 && block.params.len() >= 2)
    {
        return None;
    }
    Some(lowering::RegallocScope::enter(
        lowering::forced_regalloc().unwrap_or(lowering::RegallocChoice::Full),
    ))
}

pub(crate) fn admission(
    ops: &[Op],
    params: ir::ParamShape,
    prefix: usize,
) -> Result<(), CompileError> {
    let admit = jit_opt_admit();
    let reason = if (params.optional > 0 || params.has_rest) && !admit.args {
        Some("opt-admit:args")
    } else if prefix > 0 && !admit.env {
        Some("opt-admit:env")
    } else if ops
        .iter()
        .any(|op| matches!(op, Op::VarRef(_) | Op::VarSet(_)))
        && !admit.vars
    {
        Some("opt-admit:vars")
    } else if leaf::body_has_binds(ops) && !admit.binds {
        Some("opt-admit:binds")
    } else if leaf::body_has_handlers(ops) && !admit.handlers {
        Some("opt-admit:handlers")
    } else if ops.iter().any(|op| matches!(op, Op::Switch)) && !admit.switch {
        Some("opt-admit:switch")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(CompileError::UnsupportedOp(reason));
    }
    if ops.len() > 1000 {
        return Err(CompileError::UnsupportedOp("opt-budget:ops"));
    }
    Ok(())
}

pub(super) fn build_plan(
    ops: &[Op],
    constants: &[Value],
    cfg: &Cfg,
    params: ir::ParamShape,
    prefix: usize,
    osr_pc: Option<usize>,
) -> Result<ir::Func, CompileError> {
    build_plan_with_sqrt_sites(
        ops,
        constants,
        cfg,
        params,
        prefix,
        osr_pc,
        &std::collections::HashSet::new(),
    )
}

/// Add effects of the selected shared-emitter transport before any pass can
/// move values or discard memory facts. The arithmetic primitive body is
/// GC-free, but feedback-selected generic dispatch runs signal hooks/debugger
/// callbacks before returning an error to native code. Other primitive shims
/// stash their signal and exit before dispatch. Threading: this reads only
/// this compilation's copied numeric feedback, never mutable Lisp state.
fn annotate_opcode_transport_effects(func: &mut ir::Func) {
    use crate::emacs_core::jit::opt::mem::Effects;
    for inst in &mut func.insts {
        let (ir::Opcode::Opaque(op) | ir::Opcode::OpaqueBool(op)) = &inst.op else {
            continue;
        };
        if arith_site_takes_generic(op, inst.pc as usize) {
            inst.eff = inst.eff.with(Effects::MAY_REENTER).with(Effects::MAY_GC);
        }
    }
}

/// Scalar source-PC witnesses are captured by the synchronous mutator front;
/// the worker never performs a Lisp function lookup. Native lowering separately
/// checks the actual immutable callee/builtin witness. Sink off follows the
/// existing pipeline without reading these sites or creating recipe metadata.
pub(super) fn build_plan_with_sqrt_sites(
    ops: &[Op],
    constants: &[Value],
    cfg: &Cfg,
    params: ir::ParamShape,
    prefix: usize,
    osr_pc: Option<usize>,
    sqrt_sites: &std::collections::HashSet<u32>,
) -> Result<ir::Func, CompileError> {
    admission(ops, params, prefix)?;
    // Main's v2 front carries virtual frame chains, named/closure identities,
    // mapping callbacks and entry protocols. This backend's frames replay the
    // physical caller only, so retain the shared baseline for v2 bodies.
    if inline::active_fused().is_some_and(|body| body.is_v2()) {
        return Err(CompileError::UnsupportedOp("opt-admit:inline-v2-chains"));
    }
    let bits: Vec<_> = constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect();
    let fused = inline::active_fused();
    let osr = osr_pc.map(|pc| ir::OsrEntry {
        entry_pc: pc as u32,
        depth: cfg.entry_depth[&pc],
        header: ir::Block(0),
    });
    let mut func = build::build(build::BuildInput {
        ops,
        constants: &bits,
        cfg,
        params,
        dynamic_prefix: prefix,
        fused: fused.as_deref(),
        osr,
    })?;
    annotate_opcode_transport_effects(&mut func);
    if func.insts.len() > 20_000 {
        return Err(CompileError::UnsupportedOp("opt-budget:instructions"));
    }
    let lift = if jit_opt_passes().reps {
        let feedback: Vec<_> = (0..ops.len()).map(active_numeric_feedback).collect();
        Some(
            crate::emacs_core::jit::opt::passes::reps_lift::run(&mut func, &feedback).map_err(
                |error| {
                    tracing::debug!(?error, "opt integer lift refused a compilation");
                    CompileError::UnsupportedOp("opt-reps-lift:verify")
                },
            )?,
        )
    } else {
        None
    };
    if jit_opt_passes().fold {
        let stats = crate::emacs_core::jit::opt::passes::fold::run(&mut func).map_err(|error| {
            tracing::debug!(?error, "opt fold pass refused a compilation");
            CompileError::UnsupportedOp("opt-fold:verify")
        })?;
        tracing::debug!(target: "neovm_jit::opt", ?stats, "opt fold census");
        func.census.fold = Some(stats);
    }
    if jit_opt_passes().bool_rep {
        let stats =
            crate::emacs_core::jit::opt::passes::bools::run(&mut func).map_err(|error| {
                tracing::debug!(?error, "opt Bool pass refused a compilation");
                CompileError::UnsupportedOp("opt-bool:verify")
            })?;
        tracing::debug!(target: "neovm_jit::opt", ?stats, "opt Bool census");
        func.census.bools = Some(stats);
    }
    if jit_opt_passes().gvn {
        let stats = crate::emacs_core::jit::opt::passes::gvn::run(&mut func).map_err(|error| {
            tracing::debug!(?error, "opt GVN pass refused a compilation");
            CompileError::UnsupportedOp("opt-gvn:verify")
        })?;
        tracing::debug!(target: "neovm_jit::opt", ?stats, "opt GVN census");
        func.census.gvn = Some(stats);
    }
    if jit_opt_passes().range {
        if jit_inline_aref_on()
            && !jit_aref_slot0_on()
            && jit_layout::heap::plain_array_offsets().is_some()
        {
            let hints = array_snapshot::admission(ops.len(), constants, prefix);
            let mut proofs = func.array_reads.clone();
            let stats = crate::emacs_core::jit::opt::passes::array_reads::lift(
                &mut func,
                &mut proofs,
                &hints,
            )
            .map_err(|error| {
                tracing::debug!(?error, "opt array lift refused a compilation");
                CompileError::UnsupportedOp("opt-array-lift:verify")
            })?;
            func.array_reads = proofs;
            func.census.arrays = Some(stats);
        }
        let stats =
            crate::emacs_core::jit::opt::passes::range::run(&mut func).map_err(|error| {
                tracing::debug!(?error, "opt Range pass refused a compilation");
                CompileError::UnsupportedOp("opt-range:verify")
            })?;
        func.census.range = Some(stats);
    }
    if jit_opt_passes().licm {
        let stats = crate::emacs_core::jit::opt::passes::licm::run(&mut func).map_err(|error| {
            tracing::debug!(?error, "opt LICM pass refused a compilation");
            CompileError::UnsupportedOp("opt-licm:verify")
        })?;
        func.census.licm = Some(stats);
    }
    if let Some(lift) = lift {
        let selection =
            crate::emacs_core::jit::opt::passes::reps::run(&mut func).map_err(|error| {
                tracing::debug!(
                    ?error,
                    "opt integer representation pass refused a compilation"
                );
                CompileError::UnsupportedOp("opt-reps:verify")
            })?;
        tracing::debug!(target: "neovm_jit::opt", ?lift, ?selection, "opt integer census");
        func.census.reps = Some(ir::RepsCensus { lift, selection });
    }
    if jit_opt_passes().sink {
        let feedback = (0..ops.len())
            .map(active_numeric_feedback)
            .collect::<Vec<_>>();
        let stats = crate::emacs_core::jit::opt::passes::sink::run_with_sqrt_sites(
            &mut func, &feedback, sqrt_sites,
        )
        .map_err(|error| {
            tracing::debug!(?error, "opt Sink pass refused a compilation");
            CompileError::UnsupportedOp("opt-sink:verify")
        })?;
        tracing::debug!(target: "neovm_jit::opt", ?stats, "opt Sink census");
        func.census.sink = Some(stats);
    }
    func.verify().map_err(|error| {
        tracing::debug!(?error, "opt IR verifier refused a compilation");
        CompileError::UnsupportedOp("opt-build:verify")
    })?;
    record(
        Some(&func),
        "lower",
        ops.len(),
        cfg.entry_depth.values().sum(),
    );
    Ok(func)
}

pub(crate) fn lower_best(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    obarray: Option<&Obarray>,
    osr_pc: Option<usize>,
    prefix: usize,
    opt_params: Option<ir::ParamShape>,
) -> Result<CompiledLeaf, CompileError> {
    lower_best_requested(
        ops, constants, arity, offset_map, obarray, osr_pc, prefix, opt_params, None,
    )
}

/// The selected Opt frontend carries its copied request through this attempt.
/// Threading: request metadata is owned compiler data, never Lisp state; a
/// refused plan drops its quality scope before the shared baseline retry.
pub(super) fn lower_best_requested(
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    obarray: Option<&Obarray>,
    osr_pc: Option<usize>,
    prefix: usize,
    opt_params: Option<ir::ParamShape>,
    opt_request: Option<CompileRequest>,
) -> Result<CompiledLeaf, CompileError> {
    // The shared baseline only clears root-window counters when it emits a
    // hoisted prologue. Opt T1 bodies can have no prologue, so reset this
    // compiler-thread state before each attempt, including the baseline retry.
    // Legacy emission keeps its original initialization and diagnostics.
    let reset_opt_counts = jit_opt_mode() == OptMode::Opt;
    if let Some(params) = opt_params {
        if reset_opt_counts {
            lowering::rootwin_counters_reset();
        }
        match lower_leaf_full_osr_with_plan_impl(
            ops,
            constants,
            arity,
            offset_map,
            obarray,
            osr_pc,
            prefix,
            Some(params),
            None,
            opt_request,
        ) {
            Ok(leaf) => return Ok(leaf),
            Err(error) => {
                record(None, &format!("opt-bail:{error:?}"), ops.len(), 0);
                tracing::debug!(target: "neovm_jit::opt", ?error, "opt backend declined; keeping baseline");
            }
        }
    }
    if reset_opt_counts {
        lowering::rootwin_counters_reset();
    }
    lower_leaf_full_osr(ops, constants, arity, offset_map, obarray, osr_pc, prefix)
}

fn census_path() -> Option<&'static std::path::Path> {
    static PATH: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var_os("NEOVM_JIT_OPT_BUILD_CENSUS").map(std::path::PathBuf::from)
    })
    .as_deref()
}

/// Mutator-side build census, separate from backend selection: it can inspect
/// every compile while leaving emitted CLIF unchanged. Constants are copied to
/// bits after the baseline CFG has read any Switch table on its owning mutator.
pub(super) fn build_census(f: &ByteCodeFunction, fused: Option<&inline::FusedBody>) {
    if census_path().is_none() {
        return;
    }
    let params = ir::ParamShape {
        required: f.params.required.len(),
        optional: f.params.optional.len(),
        has_rest: f.params.rest.is_some(),
    };
    let prefix = f.jit_runtime().patched_prefix();
    let masked = mask_dynamic_prefix(&f.constants, prefix);
    let (ops, constants, offsets) = match fused {
        Some(body) => (
            body.ops.as_slice(),
            body.constants.as_slice(),
            body.offset_map.as_deref(),
        ),
        None => (
            f.executable_ops(),
            masked.as_slice(),
            f.executable_gnu_byte_offset_map(),
        ),
    };
    let cfg = match analyze_cfg(ops, constants, offsets, params.native_arity()) {
        Ok(cfg) => cfg,
        Err(error) => {
            record(None, &format!("cfg-bail:{error:?}"), ops.len(), 0);
            return;
        }
    };
    let bits: Vec<_> = constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect();
    let result = build::build(build::BuildInput {
        ops,
        constants: &bits,
        cfg: &cfg,
        params,
        dynamic_prefix: prefix,
        fused,
        osr: None,
    });
    match result {
        Ok(func) => match func.verify() {
            Ok(()) => record(
                Some(&func),
                "built",
                ops.len(),
                cfg.entry_depth.values().sum(),
            ),
            Err(error) => record(
                None,
                &format!("verify-bail:{error:?}"),
                ops.len(),
                cfg.entry_depth.values().sum(),
            ),
        },
        Err(error) => record(
            None,
            &format!("build-bail:{error:?}"),
            ops.len(),
            cfg.entry_depth.values().sum(),
        ),
    }
}

/// Diagnostics contain no Lisp objects and are safe to append from multiple
/// compilers. The mutex only serializes output; it is never on an execution path.
fn record(func: Option<&ir::Func>, verdict: &str, ops: usize, baseline_params: usize) {
    let name =
        super::super::stats::perf_map::active_label_name().unwrap_or_else(|| "anonymous".into());
    if let Some(path) = census_path() {
        use std::io::Write;
        static OUTPUT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = OUTPUT.lock().unwrap_or_else(|p| p.into_inner());
        if let Ok(mut output) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let zero = ir::OptCensus::default();
            let c = func.map_or(&zero, |f| &f.census);
            let frame_uses = func.map_or(0, |f| {
                f.insts.iter().filter(|inst| inst.frame.is_some()).count()
                    + f.source_states.iter().flatten().count()
                    + f.blocks
                        .iter()
                        .filter(|block| matches!(block.term, ir::Term::Deopt(_)))
                        .count()
            });
            let _ = writeln!(
                output,
                "{name}\t{verdict}\t{ops}\t{}\t{}\t{}\t{}\t{baseline_params}\t{}\t{}\t{frame_uses}\t{}",
                c.blocks,
                c.insts,
                c.phis,
                c.frames,
                c.critical_edges,
                c.refinements,
                c.dead_leaders
            );
        }
    }
    if let Some(func) = func {
        static DUMP: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
        if let Some(path) = DUMP
            .get_or_init(|| std::env::var_os("NEOVM_JIT_DUMP_OPT").map(std::path::PathBuf::from))
        {
            use std::io::Write;
            static OUTPUT: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let _guard = OUTPUT.lock().unwrap_or_else(|p| p.into_inner());
            if let Ok(mut output) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(output, "# {name} {verdict}\n{}", func.display());
            }
        }
    }
}
