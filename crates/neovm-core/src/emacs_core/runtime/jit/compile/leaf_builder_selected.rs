//! Selected range/sink twin of the unchanged main semantic emitter driver. Threading: all builder state belongs to
//! the active compiler; it is never shared with another mutator.

use super::*;

/// Module-generic build seam for [`lower_leaf_full`]: sets up the leaf ABI
/// signature, lowers the bytecode `ops` through a `FunctionBuilder`, then
/// hands the function to `sink` ([`LeafSink::define_leaf`]), returning its
/// `FuncId`. CLIF output is byte-identical to the previous in-line lowering
/// (pure extraction).
///
/// Generic over the sink so the same lowering drives the JIT (a per-leaf
/// `JITModule` or the persistent per-thread backend) and the `ObjectModule`
/// AOT path, unchanged. The
/// address-stable buffers (`spec_slots`/`deopt_spill`/`deopt_meta`/`reloc_data`)
/// are borrowed: their addresses are baked into the generated code, and the
/// caller retains ownership to move them into the `CompiledLeaf`.
///
/// This fn deliberately contains NONE of the three ObjectModule-incompatible
/// JIT seams, which stay in the [`lower_leaf_full`] wrapper:
///   * `builder.symbol(...)`    — AOT: `Linkage::Import` resolved via dlopen.
///   * `finalize_definitions()` — AOT: `ObjectModule::finish()`.
///   * `get_finalized_function` — AOT: `dlsym` of the exported entry symbol.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_selected_leaf_fn<S: LeafSink>(
    sink: &mut S,
    ops: &[Op],
    constants: &[Value],
    arity: usize,
    cfg: &Cfg,
    known_fixnum_slots: &HashMap<usize, Vec<bool>>,
    spec_sites: &HashMap<usize, SpecSite>,
    spec_slots: &[SpecSlot],
    n: usize,
    deopt_spill: &[core::cell::Cell<i64>],
    deopt_meta: &DeoptCells,
    reloc_data: &[Value],
    reloc_index: &std::collections::HashMap<usize, u32>,
    has_backedge: bool,
    needs_rt: bool,
    // R2-E (baseline-tier AOT): false → JIT (bases baked as `iconst`, byte-identical
    // to before); true → AOT (reloc-base + deopt bases loaded from the per-thread
    // `LeafSidecar` 4th entry arg, since the addresses are session-specific). Mirrors
    // `build_mir_leaf_fn`'s `aot` flag. Same RESULTS either way.
    aot: bool,
    // The exported entry symbol + its linkage. JIT: `("__neovm_jit_leaf", Local)`.
    // AOT: the content-hash entry name + `Export` so the loader can `dlsym` it.
    entry_name: &str,
    entry_linkage: Linkage,
    // OSR (on-stack replacement, JIT-only): when `Some(osr_pc)`, the function
    // entry does NOT seed args + jump to block 0; instead it seeds the operand
    // stack (`entry_depth[osr_pc]` tagged Values read from the `args` pointer)
    // and jumps STRAIGHT to the loop-header block at `osr_pc`, so the interpreter
    // can transfer a hot loop into native code mid-execution. Blocks unreachable
    // from `osr_pc` (the pre-loop prologue) are pruned so no dangling SSA survives.
    osr_pc: Option<usize>,
    // `make-closure` patched prefix of the source (JIT only): those leading
    // constant slots load through the callee constant base in the 4th entry
    // param instead of baking. 0 = plain function / AOT.
    dynamic_prefix: usize,
    // JIT only: what the code writes into the leaf's `LeafObs` -- the entry
    // counter (incremented first thing in the entry block,
    // `lowering::emit_entry_count`) and the poll tick counter when entry
    // counting was on at compile time, and a profiling leaf's countdown
    // (`t2_profile`). `LeafEmit::NONE` = none of them.
    emit: LeafEmit<'_>,
    // The entry's shape (`reg_abi`): the memory ABI for AOT and OSR, the
    // register ABI for an eligible JIT body when the knob is on.
    abi: LeafAbi,
    opt: Option<&crate::emacs_core::jit::opt::ir::Func>,
    sqrt_witnesses: &sqrt_snapshot::SqrtCallWitnesses,
    chains: &mut Vec<super::super::vframe::DeoptChain>,
) -> Result<cranelift_module::FuncId, CompileError> {
    debug_assert!(
        abi == LeafAbi::Memory || (!aot && osr_pc.is_none()),
        "AOT and OSR entries keep the memory ABI"
    );
    let variable_raw = uniform_raw_osr_slots(cfg, known_fixnum_slots, osr_pc);
    let has_raw_slots = variable_raw.iter().any(|&raw| raw);
    lowering::imm_pool_reset();
    cold_exits::begin_function();
    LAST_IR_STATS.with(|c| c.set((0, 0, 0, 0)));
    lowering::flonum_census_reset();
    heap_inline::inline_heap_sites_reset();
    inline_vars::begin_function(ops, constants, cfg, aot);
    let frontend_config = sink.module().target_config();
    let call_conv = frontend_config.default_call_conv;
    let ptr_ty = frontend_config.pointer_type();

    // ABI: fn(vmctx: *mut Context, args: *const i64, out: *mut i64) -> i64.
    // Reads `arity` argument words from `args`; returns STATUS_OK + writes the
    // result bits via `out` on success, STATUS_DEOPT on a failed guard, or
    // STATUS_SIGNAL when a runtime call raised a Flow (stashed for
    // `take_pending_flow`). `vmctx` is only used by runtime-call shims.
    // Unified 4-param entry ABI: fn(vmctx, args, out, sidecar) -> status (see
    // build_mir_leaf_fn). JIT (`aot=false`) declares but ignores `sidecar` (bases
    // stay `iconst`); AOT (`aot=true`) reads its reloc/deopt bases from it. The
    // register ABI (`reg_abi`) takes the arguments as parameters instead and
    // returns (value, status).
    let sig = abi.signature(call_conv, ptr_ty);

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = sink.take_builder_context();
    let frames = inline_frames::Frames::new(deopt_meta);
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);

        // Declare the runtime-call machinery into this function if the body
        // re-enters the runtime (`Cons` / `Call`).
        let mut rt = if needs_rt {
            // Declare the JIT-only round-1 subr-speculation shims iff the body has
            // round-1 subr-kind spec sites. The AOT baseline emit classifies ONLY
            // CBSym sites (`find_cbsym_spec_sites`; the `Op::Call` subr pass is
            // increment B), so this is always false for AOT — an ObjectModule
            // never declares the round-1 subr import names. CBSym-kind sites are
            // deliberately NOT counted here: they get their own R2 shims, so a
            // CBSym-only body never imports the `Op::Call` spec shims.
            let subr_spec = spec_sites.values().any(|site| site.kind.is_round1_subr());
            // The R2 CallBuiltinSym intrinsic shims (Tier-A read / Tier-B
            // dispatch-skip), declared when a CBSym-kind site exists. UNLIKE the
            // round-1 shims these are NOW emitted by AOT too (increment A): CBSym
            // classification is obarray-free, so `find_cbsym_spec_sites` populates
            // this for the baseline `ObjectModule` and the two shims become imports
            // resolved against the host at `dlopen`.
            let cbsym_spec = spec_sites.values().any(|site| site.kind.is_cbsym());
            let shapes = jit_direct_shapes();
            let groups = ShimGroups {
                subr_spec,
                cbsym_spec,
                tier2_profile: emit.t2.is_some(),
                direct_shapes: !aot && (shapes.optional || shapes.rest),
                call_census: !aot && jit_call_census_on(),
                direct_framed: !aot && shapes.framed,
                hof: inline::active_fused().is_some_and(|body| {
                    body.v2.as_ref().is_some_and(|side| !side.hof_at.is_empty())
                }),
            };
            let groups = super::shim_refs::SelectedShimGroups {
                main: groups,
                array_profile: super::array_profile::selected(aot, emit.t2)
                    && ops.iter().any(|op| *op == Op::Aref),
                sink_versions: !aot
                    && opt.is_some_and(|func| !func.sink_recipes.source_sqrt_sites.is_empty()),
            };
            let refs = RtRefs::new_selected(
                sink.shim_ids_selected(call_conv, ptr_ty, groups)?,
                groups,
                fb.func,
                call_conv,
                ptr_ty,
            );
            let vmctx_var = fb.declare_var(ptr_ty);
            let max_call_args = ops
                .iter()
                .filter_map(|o| match o {
                    Op::Call(n) | Op::Apply(n) | Op::List(n) | Op::Concat(n) => Some(*n as usize),
                    Op::CallBuiltin(_, n) | Op::CallBuiltinSym(_, n) => Some(*n as usize),
                    Op::Nconc => Some(2),
                    Op::Substring | Op::Aset => Some(3),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let call_args_slot = fb.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (max_call_args.max(1) * 8) as u32,
                3,
            ));
            let call_result_slot =
                fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            Some(RtCtx {
                refs,
                vmctx_var,
                ptr_ty,
                call_args_slot,
                call_result_slot,
                rootwin: None,
                heap: None,
                inline_alloc: !aot && jit_inline_alloc_on(),
                generational: std::cell::Cell::new(aot.then_some(false)),
                direct_sites: std::cell::Cell::new(0),
                self_direct_source: direct_call::source_for_abi(abi),
                poll: emit.poll(),
                inline_entry_cache: None,
            })
        } else {
            None
        };

        // A pure profiling leaf needs only the tier-request import. Keeping
        // this separate preserves its existing runtime-free shape when off.
        let t2_refs = if emit.t2.is_some() && rt.is_none() {
            Some(t2_profile::pure_entry_refs(
                sink, fb.func, call_conv, ptr_ty,
            )?)
        } else {
            None
        };

        // SSA variables: one I64 slot per operand-stack position (carries the
        // stack across block edges), plus one for the out pointer (used by
        // `Return` in any block).
        let vars: Vec<Variable> = (0..cfg.max_depth)
            .map(|_| fb.declare_var(types::I64))
            .collect();
        let out_var = fb.declare_var(ptr_ty);
        let opt_ssa = opt.map(|func| opt_emission::SsaValues::new(&mut fb, func, &variable_raw));

        // One CLIF block per bytecode basic block.
        let block_for: HashMap<usize, Block> = cfg
            .leaders
            .iter()
            .map(|&l| (l, fb.create_block()))
            .collect();
        let opt_blocks = opt.map(|func| {
            func.blocks
                .iter()
                .map(|_| fb.create_block())
                .collect::<Vec<_>>()
        });
        // Deopt-buffer base addresses (for the JIT `iconst` path). The CLIF
        // `DeoptRefs` is materialized in the entry block below (the baseline tier
        // has no AOT path yet, so always the `iconst` form).
        let spill_base_addr = deopt_spill.as_ptr() as i64;
        let meta_pc_addr = &deopt_meta.pc as *const core::cell::Cell<i64> as i64;
        let meta_depth_addr = &deopt_meta.depth as *const core::cell::Cell<i64> as i64;
        let meta_handlers_addr = &deopt_meta.handlers as *const core::cell::Cell<i64> as i64;
        // Shared signal-propagation block (returns STATUS_SIGNAL), created
        // lazily by the first `Call` lowering.
        let mut signal_exit: Option<Block> = None;
        // Backward-jump quit counter (the interpreter's u8 `quitcounter`), kept
        // in a stack slot so every block can bump it.
        let backedge_counter: Option<StackSlot> = has_backedge.then(|| {
            fb.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3))
        });

        // Function-entry block: stash vmctx + the out pointer, load args into
        // the slot variables, then jump into bytecode block 0.
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        if let Some(counter) = emit.entry_counter {
            debug_assert!(!aot, "AOT code never counts entries");
            lowering::emit_entry_count(&mut fb, ptr_ty, counter);
        }
        let entry_params = fb.block_params(entry).to_vec();
        if let Some(t2) = emit.t2 {
            debug_assert!(!aot && osr_pc.is_none(), "only a JIT entry leaf profiles");
            let refs = rt
                .as_ref()
                .map(|rt| &rt.refs)
                .or(t2_refs.as_ref())
                .expect("profiling refs");
            t2_profile::emit_entry_countdown(&mut fb, ptr_ty, refs, t2);
        }
        // A function entry that can re-enter Lisp signals "Bytecode stack
        // overflow" before the native stack runs out (`stack_guard`); the
        // rest of the entry code then runs in the block after the guard, on
        // its parameters. An OSR entry is not a recursion level: the
        // interpreter frame it continues was entered through a probed path.
        let entry_params = match rt.as_ref() {
            Some(rt) if osr_pc.is_none() && stack_guard::body_may_reenter_lisp(ops) => {
                stack_guard::emit_entry_stack_guard(&mut fb, rt, abi, &entry_params)
            }
            _ => entry_params,
        };
        // Memory: `[vmctx, args, out, sidecar]`. Register: `[vmctx, aux,
        // a0..]`, `aux` taking the sidecar word's place (a JIT body reads it
        // only as its callee constant base) and no `args`/`out` at all.
        let (vmctx_param, args_ptr, out_ptr, fourth_param) = match abi {
            LeafAbi::Memory => (
                entry_params[0],
                Some(entry_params[1]),
                Some(entry_params[2]),
                entry_params[3],
            ),
            LeafAbi::Register { .. } => (entry_params[0], None, None, entry_params[1]),
        };
        // R2-E: the 4th entry param (the per-thread `*const LeafSidecar`). Read only
        // in AOT mode; JIT ignores it. The entry block dominates every block, so a
        // base materialized here is valid in any (incl. cold deopt) block.
        let sidecar_param = aot.then_some(fourth_param);
        // JIT leaf of a `make-closure`-patched source: the same 4th entry param is
        // the EXECUTING CALLEE's constant base (`CompiledLeaf::call_consts`); the
        // patched slots load off it (`lower_simple_op` `Op::Constant`). Bound in
        // the entry block so it dominates every block.
        debug_assert!(
            !(aot && dynamic_prefix > 0),
            "AOT never targets a patched source"
        );
        let consts_base = (!aot && dynamic_prefix > 0).then_some(fourth_param);
        // R1a: base address of the heap-constant reloc vector, materialized once in
        // entry (dominates all blocks); the baseline Op::Constant loads off it by
        // index. JIT bakes the Box address as `iconst`; AOT loads it from the
        // sidecar (session-specific). `None` when the body references no heap consts.
        let reloc_base = if reloc_data.is_empty() {
            None
        } else if aot {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            Some(fb.ins().load(
                ptr_ty,
                MemFlagsData::trusted(),
                sc,
                LeafSidecar::OFF_RELOC_BASE,
            ))
        } else {
            Some(fb.ins().iconst(ptr_ty, reloc_data.as_ptr() as i64))
        };
        // R2 increment B2: the AOT sidecar's spec-slot / spec-expected array bases,
        // loaded ONCE here (the entry block dominates every block, incl. cold deopt)
        // iff this is an AOT body with at least one `Op::Call` subr/bytecode spec
        // site (CBSym sites are slotless — they read neither base). JIT
        // (`aot=false`) emits NOTHING here (bases stay `None`; each site bakes its
        // slot/expected as `iconst`), so the JIT lowering is byte-identical to
        // pre-B2. `None` when the leaf has no such site, so a null sidecar base is
        // never loaded/indexed.
        let has_op_call_spec = spec_sites.values().any(|s| s.kind.to_spec_disc().is_some());
        let (spec_slot_base, spec_expected_base) = if aot && has_op_call_spec {
            let sc = sidecar_param.expect("AOT sets sidecar_param");
            (
                Some(fb.ins().load(
                    ptr_ty,
                    MemFlagsData::trusted(),
                    sc,
                    LeafSidecar::OFF_SPEC_SLOT_BASE,
                )),
                Some(fb.ins().load(
                    ptr_ty,
                    MemFlagsData::trusted(),
                    sc,
                    LeafSidecar::OFF_SPEC_EXPECTED_BASE,
                )),
            )
        } else {
            (None, None)
        };
        // Deopt-buffer bases as entry-block values: JIT (`aot=false`) → the `iconst`
        // form (deferred to the cold deopt blocks, byte-identical to pre-R2-E); AOT
        // (`aot=true`) → loaded from the sidecar. The baseline's precise deopt spills
        // operand-stack + pc/depth/handlers — exactly the sidecar's carried bases (at
        // D0 there are no spec slots, so no per-spec-slot state beyond these).
        let deopt_refs = materialize_deopt_refs(
            &mut fb,
            ptr_ty,
            aot,
            /*has_precise_deopt=*/ true,
            sidecar_param,
            spill_base_addr,
            meta_pc_addr,
            meta_depth_addr,
            meta_handlers_addr,
        );
        // Pool the common immediates once per function (see
        // `lowering::imm_pool_define`); `imm_pool_reset` ran at function start.
        lowering::imm_pool_define(&mut fb, lowering::POOLED_IMMEDIATES);
        if let Some(rt) = rt.as_mut() {
            fb.def_var(rt.vmctx_var, vmctx_param);
            // The heap pointer every inline heap site reads, loaded once here
            // (the entry block dominates every block, the OSR jump included)
            // when the body has two such sites or one in a loop; else each
            // site loads its own. Only a body with inline sites loads it, and
            // such a body dereferences its vmctx anyway.
            if !aot
                && (inline_hof::hoist_callback_heap_ptr()
                    || heap_inline::hoist_heap_ptr(
                        ops,
                        has_back_edge(ops),
                        rt.inline_alloc,
                        |pc| {
                            matches!(
                                active_numeric_feedback(pc),
                                crate::emacs_core::jit::NumericFeedback::Float
                            )
                        },
                    ))
            {
                rt.heap = Some(heap_inline::load_heap_ptr(&mut fb, vmctx_param));
            }
            // Root-window base + capacity check once per activation instead
            // of once per site (`lowering::HoistedRootWin`). Only for a body
            // with rooting sites: those dereference the vmctx anyway, so it is
            // a real Context; a site-free body may be entered with a null one.
            // Two sites, or one inside a loop: a single straight-line site
            // executes once per activation, like the prologue, and its inline
            // form costs about the same, but a site in a loop runs its check
            // on every iteration (the 3M-call benchmark: −2.4% hoisted).
            let sites = count_rooting_sites(ops);
            if cfg.max_depth > 0 && (sites >= 2 || (sites == 1 && has_back_edge(ops))) {
                lowering::emit_hoisted_root_window_prologue(
                    &mut fb,
                    rt,
                    vmctx_param,
                    opt.map_or(cfg.max_depth, |func| cfg.max_depth + func.values.len()),
                );
            }
        }
        if !aot && let Some(rt) = rt.as_mut() {
            rt.inline_entry_cache =
                inline::active_fused()
                    .filter(|body| body.is_v2())
                    .and_then(|body| {
                        inline_entry_cache::EntryCache::build(
                            &mut fb,
                            &body,
                            cfg,
                            arity,
                            osr_pc,
                            dynamic_prefix,
                        )
                    });
        }
        if let Some(out_ptr) = out_ptr {
            fb.def_var(out_var, out_ptr);
        }
        if let Some(slot) = backedge_counter {
            // The interpreter starts quitcounter at 1.
            let one = fb.ins().iconst(types::I64, 1);
            fb.ins().stack_store(ptr_ty, one, slot, 0);
        }
        // Entry seeding + jump target. Normal: seed the `arity` args into the
        // bottom slots and jump to bytecode block 0. OSR: the `args` pointer holds
        // the live OPERAND STACK snapshot (`entry_depth[osr_pc]` tagged Values), so
        // seed those slots and jump STRAIGHT to the loop-header block at `osr_pc`.
        let (seed_count, jump_target) = match osr_pc {
            Some(p) => (cfg.entry_depth[&p], block_for[&p]),
            None => (arity, block_for[&0]),
        };
        for (i, var) in vars.iter().take(seed_count).enumerate() {
            let v = match args_ptr {
                Some(args_ptr) => fb.ins().load(
                    types::I64,
                    MemFlagsData::trusted(),
                    args_ptr,
                    (i * 8) as i32,
                ),
                // Register ABI: the arguments are the entry's parameters.
                None => entry_params[2 + i],
            };
            fb.def_var(*var, v);
        }
        // A live OSR snapshot is an additional predecessor of the header.
        // Check exactly the type facts that normal entry established before
        // using them to elide guards in the body. Rejection captures the whole
        // unchanged tagged snapshot at the header, before any bytecode effect.
        let mut entry_deopts = Vec::new();
        if let Some(rt) = rt.as_ref()
            && rt
                .inline_entry_cache
                .as_ref()
                .is_some_and(|cache| cache.physical_protocol_admitted())
        {
            let values = vars[..seed_count]
                .iter()
                .map(|&var| fb.use_var(var))
                .collect::<Vec<_>>();
            inline_physical::emit(
                &mut fb,
                rt,
                &frames,
                osr_pc.unwrap_or(0),
                &values,
                &mut entry_deopts,
            );
        }
        if let Some(pc) = osr_pc
            && let Some(slots) = known_fixnum_slots.get(&pc)
            && slots.iter().any(|&known| known)
        {
            let stack: Vec<ClifValue> = vars[..seed_count]
                .iter()
                .map(|&var| fb.use_var(var))
                .collect();
            let reps = vec![SlotRep::Tagged; seed_count];
            let deopt = deopt_site(&mut fb, pc, 0, &stack, &reps, &mut entry_deopts);
            let unknown = HashSet::new();
            for (&value, &known) in stack.iter().zip(slots) {
                if known {
                    guard_fixnum(&mut fb, deopt, value, &unknown);
                }
            }
        }
        for (slot, &var) in vars.iter().take(seed_count).enumerate() {
            if variable_raw[slot] {
                let tagged = fb.use_var(var);
                let raw = lowering::sshr_imm_p(&mut fb, tagged, FIXNUM_SHIFT as i64);
                fb.def_var(var, raw);
            }
        }
        let jump_target = match (opt, &opt_blocks) {
            (Some(func), Some(blocks)) => blocks[func.entry.index()],
            _ => jump_target,
        };
        fb.ins().jump(jump_target, &[]);
        if has_raw_slots {
            for site in &entry_deopts {
                fb.set_cold_block(site.block);
            }
        }
        // An OSR entry snapshot is all tagged: no flonum crosses an edge.
        emit_pending_deopts(&mut fb, deopt_refs, &mut entry_deopts, None, abi);

        if let (Some(func), Some(values), Some(blocks)) = (opt, &opt_ssa, &opt_blocks) {
            #[cfg(test)]
            let native_validation =
                crate::emacs_core::jit::opt::native_verify_observer::NativeRegion::enter();
            // Sink-selected native validation retains proof results from one
            // full Func check. Non-Sink Array/ordinary paths keep their prior
            // admission and verification sequence exactly.
            let native_proof = if !func.sink_recipes.owners.is_empty() {
                if !func.array_reads.reads.is_empty() && (aot || !jit_opt_passes().range) {
                    return Err(CompileError::UnsupportedOp("opt-array:pass-off"));
                }
                if aot || !jit_opt_passes().sink {
                    return Err(CompileError::UnsupportedOp("opt-sink:pass-off"));
                }
                let invalid = if func.array_reads.reads.is_empty() {
                    "opt-sink:final-proof"
                } else {
                    "opt-array:ir"
                };
                Some(
                    crate::emacs_core::jit::opt::verify::verify_for_native(func)
                        .map_err(|_| CompileError::UnsupportedOp(invalid))?,
                )
            } else {
                None
            };
            let legacy_arrays = if native_proof.is_none() && !func.array_reads.reads.is_empty() {
                if aot || !jit_opt_passes().range {
                    return Err(CompileError::UnsupportedOp("opt-array:pass-off"));
                }
                func.verify()
                    .map_err(|_| CompileError::UnsupportedOp("opt-array:ir"))?;
                Some(
                    crate::emacs_core::jit::opt::passes::array_reads::verify_reads(
                        func,
                        &func.array_reads,
                    )
                    .map_err(|_| CompileError::UnsupportedOp("opt-array:final-proof"))?,
                )
            } else {
                None
            };
            // Consume the same immutable Func retained by the bundle. No child
            // proof is paired with an unrelated or subsequently mutated graph.
            let func = native_proof.as_ref().map_or(func, |proof| proof.func());
            let verified_arrays = native_proof
                .as_ref()
                .and_then(|proof| proof.arrays())
                .or(legacy_arrays.as_ref());
            let verified_sink = native_proof.as_ref().and_then(|proof| proof.sink());
            #[cfg(test)]
            drop(native_validation);
            opt_emission::emit(opt_emission::EmitContext {
                fb: &mut fb,
                func,
                values,
                blocks,
                cfg,
                seed_vars: &vars,
                variable_raw: &variable_raw,
                constants,
                rt: rt.as_ref(),
                spec_sites,
                spec_slots,
                deopt_refs,
                signal_exit: &mut signal_exit,
                backedge_counter,
                out_var,
                out_present: out_ptr.is_some(),
                abi,
                reloc_base,
                reloc_index,
                aot,
                spec_slot_base,
                spec_expected_base,
                dynamic_prefix,
                consts_base,
                ops,
                known_fixnum_slots,
                verified_arrays: verified_arrays,
                verified_sink: verified_sink,
                numeric_facts: Default::default(),
                sqrt_witnesses: sqrt_witnesses,
                point: crate::emacs_core::jit::opt::sink_recipes::RecipePoint::Entry(func.entry),
                sink_produced: HashMap::new(),
            })?;
        } else {
            for (leader_index, &l) in cfg.leaders.iter().enumerate() {
                let blk = block_for[&l];
                fb.switch_to_block(blk);
                // A leader may be reached from any store history: drop the
                // root-window record (see `lowering::RootWinCarry`).
                lowering::rootwin_carry_reset();
                // An unreachable block — no path from the entry, so the dataflow
                // gave it no entry depth: the byte-compiler emits code after an
                // unconditional exit that nothing targets (ebrowse, eglot, wdired,
                // texinfo, ns-win all have one) — lowers to a trap. Nothing it
                // does can run, and anything reachable only through it is
                // unreachable too, so skipping it leaves every jump target with
                // a depth. Indexing here panicked ("no entry found for key") the
                // moment the profitability gate stopped vetoing such bodies.
                let Some(&depth) = cfg.entry_depth.get(&l) else {
                    fb.ins()
                        .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
                    continue;
                };
                let mut stack: Vec<ClifValue> = (0..depth).map(|k| fb.use_var(vars[k])).collect();
                // OSR slot representations are uniform at all reachable leaders.
                let mut reps: Vec<SlotRep> = variable_raw[..depth]
                    .iter()
                    .map(|&raw| SlotRep::raw_if(raw))
                    .collect();
                // Cross-block known-fixnum operands at this block's entry: each slot
                // the dataflow analysis proved fixnum maps to its just-materialized
                // ClifValue. StackRef/Dup keep the same ClifValue, so the set stays
                // valid as the block runs; `guard_fixnum` elides guards for members.
                let known_fixnum: HashSet<ClifValue> = known_fixnum_slots
                    .get(&l)
                    .map(|slots| {
                        slots
                            .iter()
                            .enumerate()
                            .filter_map(|(k, &is_fix)| {
                                (is_fix && !variable_raw[k])
                                    .then(|| stack.get(k).copied())
                                    .flatten()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                // Active handler frames at block entry (static), kept in sync as
                // PopHandler ops run; signal sites inside a protected extent queue
                // a dispatch block here, filled after the block's terminator.
                let mut physical_binds = cfg.entry_binds.get(&l).copied().unwrap_or(0);
                let mut handlers: Vec<HandlerStatic> =
                    cfg.entry_handlers.get(&l).cloned().unwrap_or_default();
                let mut pending: Vec<PendingDispatch> = Vec::new();
                let mut pending_deopt: Vec<PendingDeopt> = Vec::new();

                let end = cfg.leaders.get(leader_index + 1).copied().unwrap_or(n);
                let mut terminated = false;
                for (off, op) in ops[l..end].iter().enumerate() {
                    let i = l + off;
                    // An inlined region's ops deopt to the CALL they replaced,
                    // with the caller's pre-call stack — captured here, at the
                    // region's first op, where the stack still holds
                    // `[...residual, callee, args]`.
                    if let Some(fused) = inline::active_fused().filter(|f| !f.is_v2()) {
                        match fused.region_at(i) {
                            Some(region) if i == region.start => {
                                let region = region.clone();
                                // Regions stay allocation-free (`inline.rs`: a
                                // collection inside one would strand its
                                // framestate), so box any flonum BEFORE the
                                // snapshot: the region's deopt replays the call
                                // with boxed arguments.
                                box_all_flonums(&mut fb, rt.as_ref(), &mut stack, &mut reps);
                                lowering::set_active_region(Some(lowering::RegionDeopt {
                                    call_site_pc: region.call_site_pc,
                                    stack: stack.clone(),
                                    reps: reps.clone(),
                                    chain: None,
                                }));
                                // ...and the splice is a speculation, so check the
                                // slot still holds the callee whose body is next.
                                lowering::emit_region_entry_guard(
                                    &mut fb,
                                    &region,
                                    handlers.len(),
                                    &stack,
                                    &reps,
                                    &mut pending_deopt,
                                )?;
                            }
                            None => lowering::set_active_region(None),
                            // A region is single-entry and the walk ascends, so
                            // its snapshot was taken at its start and is still
                            // the active one. Check it instead of trusting it: a
                            // deopt under someone else's framestate replays the
                            // wrong call.
                            Some(region) => {
                                if lowering::active_region_call_site() != Some(region.call_site_pc)
                                {
                                    return Err(CompileError::UnsupportedOp("inline-region-entry"));
                                }
                            }
                        }
                    }
                    if let Some(site) =
                        inline::active_fused().and_then(|f| f.admitted_hof_at(i).copied())
                    {
                        inline_hof::emit(
                            &frames,
                            &mut fb,
                            i,
                            site,
                            rt.as_ref().expect("HOF runtime"),
                            physical_binds,
                            &mut stack,
                            &mut reps,
                            &mut pending_deopt,
                            &mut signal_exit,
                            reloc_base,
                            reloc_index,
                        )?;
                        continue;
                    }
                    if frames.before_op(
                        &mut fb,
                        i,
                        op,
                        rt.as_ref(),
                        physical_binds,
                        handlers.len(),
                        &mut stack,
                        &mut reps,
                        &mut pending_deopt,
                        reloc_index,
                    )? {
                        continue;
                    }
                    match op {
                        Op::VarBind(_)
                        | Op::SaveCurrentBuffer
                        | Op::SaveExcursion
                        | Op::SaveRestriction
                        | Op::UnwindProtectPop => physical_binds += 1,
                        Op::Unbind(n) => physical_binds -= *n as usize,
                        _ => {}
                    }
                    // Terminators consume / snapshot / spill the operand stack as tagged
                    // Values; force-tag any raw slots and box any flonums first (the
                    // block's slot state is discarded after the terminator, so no
                    // per-pop lockstep is needed past this point).
                    if matches!(
                        op,
                        Op::Throw
                            | Op::Switch
                            | Op::PushConditionCase(_)
                            | Op::PushConditionCaseRaw(_)
                            | Op::PushCatch(_)
                    ) {
                        materialize_model_stack(&mut fb, rt.as_ref(), &mut stack, &mut reps);
                    } else if matches!(op, Op::Return) {
                        // Only the returned value escapes: box it if it is a
                        // flonum. The slots below it die here, so a flonum among
                        // them is never boxed.
                        if let Some(top) = stack.len().checked_sub(1)
                            && reps[top].is_flonum()
                        {
                            let rt = rt.as_ref().expect("a flonum implies the runtime refs");
                            box_flonum_slot(&mut fb, rt, &mut stack, &mut reps, top);
                        }
                        retag_raw_fixnums(&mut fb, &mut stack, &mut reps);
                    }
                    match op {
                        Op::Return => {
                            let result = stack.pop().ok_or(CompileError::StackUnderflow)?;
                            let out = out_ptr.map(|_| fb.use_var(out_var));
                            reg_abi::emit_leaf_return(&mut fb, abi, out, Some(result), STATUS_OK);
                            terminated = true;
                            break;
                        }
                        Op::Throw => {
                            // Stash Flow::Throw{tag, value} and exit via the signal
                            // path; inside a protected extent that path is the
                            // handler dispatch (a same-function `catch` is caught
                            // natively via the match shim).
                            let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
                            let tag = stack.pop().ok_or(CompileError::StackUnderflow)?;
                            let rt = rt.as_ref().ok_or(CompileError::UnsupportedOp("throw"))?;
                            let throw_flow = rt.refs.get(fb.func, Shim::ThrowFlow);
                            fb.ins().call(throw_flow, &[tag, value]);
                            let se = signal_target_for_site(
                                &mut fb,
                                &mut signal_exit,
                                &handlers,
                                &mut pending,
                                &stack,
                                &reps,
                            );
                            fb.ins().jump(se, &[]);
                            terminated = true;
                            break;
                        }
                        Op::Goto(t) => {
                            write_edge_stack_to_vars(
                                &mut fb,
                                rt.as_ref(),
                                &vars,
                                &mut stack,
                                &mut reps,
                                &variable_raw,
                            );
                            let tu = *t as usize;
                            if tu <= i {
                                // Backward jump: bump the quit counter and poll on
                                // wrap, exactly like the interpreter's branch_to!.
                                let (rt, slot) = (
                                    rt.as_ref().expect("backedge implies rt"),
                                    backedge_counter.expect("backedge implies counter"),
                                );
                                emit_backedge_jump(
                                    &mut fb,
                                    rt,
                                    slot,
                                    &mut signal_exit,
                                    &vars,
                                    &variable_raw,
                                    cfg.entry_depth[&tu],
                                    block_for[&tu],
                                    &handlers,
                                    &mut pending,
                                    inline_physical::poll_admission(
                                        rt,
                                        &frames,
                                        tu,
                                        &mut pending_deopt,
                                    ),
                                );
                            } else {
                                fb.ins().jump(block_for[&tu], &[]);
                            }
                            terminated = true;
                            break;
                        }
                        Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                            let cond = stack.pop().ok_or(CompileError::StackUnderflow)?;
                            let rep = reps.pop().ok_or(CompileError::StackUnderflow)?;
                            let cond = if rep == SlotRep::RawFixnum {
                                retag_fixnum(&mut fb, cond)
                            } else {
                                cond
                            };
                            write_edge_stack_to_vars(
                                &mut fb,
                                rt.as_ref(),
                                &vars,
                                &mut stack,
                                &mut reps,
                                &variable_raw,
                            );
                            let is_nil =
                                fb.ins()
                                    .icmp_imm_u(IntCC::Equal, cond, Value::NIL.bits() as i64);
                            let tu = *t as usize;
                            let mut target = block_for[&tu];
                            let fallthrough = block_for[&(i + 1)];
                            let backedge = (tu <= i).then(|| fb.create_block());
                            if let Some(tramp) = backedge {
                                target = tramp;
                            }
                            // brif takes the `then` block when the condition is true.
                            if matches!(op, Op::GotoIfNil(_)) {
                                fb.ins().brif(is_nil, target, &[], fallthrough, &[]);
                            } else {
                                fb.ins().brif(is_nil, fallthrough, &[], target, &[]);
                            }
                            if let Some(tramp) = backedge {
                                // Taken-edge trampoline carrying the back-edge poll.
                                fb.switch_to_block(tramp);
                                fb.seal_block(tramp);
                                let (rt, slot) = (
                                    rt.as_ref().expect("backedge implies rt"),
                                    backedge_counter.expect("backedge implies counter"),
                                );
                                emit_backedge_jump(
                                    &mut fb,
                                    rt,
                                    slot,
                                    &mut signal_exit,
                                    &vars,
                                    &variable_raw,
                                    cfg.entry_depth[&tu],
                                    block_for[&tu],
                                    &handlers,
                                    &mut pending,
                                    inline_physical::poll_admission(
                                        rt,
                                        &frames,
                                        tu,
                                        &mut pending_deopt,
                                    ),
                                );
                            }
                            terminated = true;
                            break;
                        }
                        Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                            // Peek the condition without popping; write the FULL stack
                            // (cond on top) to vars. The jump-taken successor reads it
                            // all (depth D); the fall-through (depth D-1) ignores the
                            // top slot — implementing the "ElsePop".
                            let cond = *stack.last().ok_or(CompileError::StackUnderflow)?;
                            let rep = *reps.last().ok_or(CompileError::StackUnderflow)?;
                            let cond = if rep == SlotRep::RawFixnum {
                                retag_fixnum(&mut fb, cond)
                            } else {
                                cond
                            };
                            write_edge_stack_to_vars(
                                &mut fb,
                                rt.as_ref(),
                                &vars,
                                &mut stack,
                                &mut reps,
                                &variable_raw,
                            );
                            let is_nil =
                                fb.ins()
                                    .icmp_imm_u(IntCC::Equal, cond, Value::NIL.bits() as i64);
                            let tu = *t as usize;
                            let mut target = block_for[&tu];
                            let fallthrough = block_for[&(i + 1)];
                            let backedge = (tu <= i).then(|| fb.create_block());
                            if let Some(tramp) = backedge {
                                target = tramp;
                            }
                            if matches!(op, Op::GotoIfNilElsePop(_)) {
                                fb.ins().brif(is_nil, target, &[], fallthrough, &[]);
                            } else {
                                fb.ins().brif(is_nil, fallthrough, &[], target, &[]);
                            }
                            if let Some(tramp) = backedge {
                                fb.switch_to_block(tramp);
                                fb.seal_block(tramp);
                                let (rt, slot) = (
                                    rt.as_ref().expect("backedge implies rt"),
                                    backedge_counter.expect("backedge implies counter"),
                                );
                                emit_backedge_jump(
                                    &mut fb,
                                    rt,
                                    slot,
                                    &mut signal_exit,
                                    &vars,
                                    &variable_raw,
                                    cfg.entry_depth[&tu],
                                    block_for[&tu],
                                    &handlers,
                                    &mut pending,
                                    inline_physical::poll_admission(
                                        rt,
                                        &frames,
                                        tu,
                                        &mut pending_deopt,
                                    ),
                                );
                            }
                            terminated = true;
                            break;
                        }
                        Op::Switch => {
                            // [dispatch table] -> a static target, or fall
                            // through on a miss (`switch_dispatch`).
                            let rt_ref =
                                rt.as_ref().ok_or(CompileError::UnsupportedOp("switch"))?;
                            let table = stack.pop().ok_or(CompileError::StackUnderflow)?;
                            let dispatch = stack.pop().ok_or(CompileError::StackUnderflow)?;
                            reps.truncate(stack.len());
                            write_edge_stack_to_vars(
                                &mut fb,
                                rt.as_ref(),
                                &vars,
                                &mut stack,
                                &mut reps,
                                &variable_raw,
                            );
                            let targets = cfg.switch_targets.get(&i).expect("resolved in analyze");
                            let sig = signal_target_for_site(
                                &mut fb,
                                &mut signal_exit,
                                &handlers,
                                &mut pending,
                                &stack,
                                &reps,
                            );
                            let fall = block_for[&(i + 1)];
                            let inline = if !aot && jit_inline_switch_on() {
                                switch_dispatch::inline_switch_for_site(
                                    ops,
                                    constants,
                                    dynamic_prefix,
                                    &cfg.leaders,
                                    i,
                                    targets,
                                )
                            } else {
                                None
                            };
                            let mut landings = switch_dispatch::BaselineSwitchLandings {
                                site: i,
                                targets,
                                block_for: &block_for,
                                entry_depth: &cfg.entry_depth,
                                rt: rt_ref,
                                backedge_counter,
                                signal_exit: &mut signal_exit,
                                vars: &vars,
                                variable_raw: &variable_raw,
                                handlers: &handlers,
                                pending: &mut pending,
                                trampolines: Vec::new(),
                                unfilled: Vec::new(),
                            };
                            switch_dispatch::emit_switch_dispatch(
                                &mut fb,
                                rt_ref,
                                dispatch,
                                table,
                                targets,
                                fall,
                                sig,
                                &mut landings,
                                inline.as_ref(),
                            );
                            terminated = true;
                            break;
                        }
                        Op::PushConditionCase(t)
                        | Op::PushConditionCaseRaw(t)
                        | Op::PushCatch(t) => {
                            // Register the handler frame via the shim (interpreter
                            // arm parity), then end the block with an "anchor"
                            // edge: a never-taken branch to the handler target
                            // that (a) guarantees the target block always has a
                            // Cranelift predecessor with every entry var defined
                            // (its real entries are the runtime match dispatches)
                            // and (b) falls through to the protected body.
                            let rt_ref =
                                rt.as_ref().ok_or(CompileError::UnsupportedOp("handler"))?;
                            let tu = *t as usize;
                            let vmctx = fb.use_var(rt_ref.vmctx_var);
                            // The target serves two consumers that need DIFFERENT
                            // numbering. The compiled dispatch below reaches its
                            // handler through `block_for`, keyed by the pc of the
                            // ops being lowered — fused. But the shim stores this
                            // operand in a `ResumeTarget`, and the only code that
                            // reads it back is a RESUMED INTERPRETER frame, which
                            // jumps to it in the UNFUSED ops. So the runtime gets
                            // the original pc and `block_for` keeps the fused one.
                            let runtime_target = inline::active_fused()
                                .and_then(|f| f.caller_pc(tu))
                                .unwrap_or(tu);
                            let t_v = fb.ins().iconst(types::I64, runtime_target as i64);
                            match op {
                                Op::PushConditionCase(_) => {
                                    let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                    let push_cc = rt_ref.refs.get(fb.func, Shim::PushCc);
                                    fb.ins().call(push_cc, &[vmctx, t_v, d_v]);
                                }
                                Op::PushConditionCaseRaw(_) => {
                                    let conditions =
                                        stack.pop().ok_or(CompileError::StackUnderflow)?;
                                    let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                    let push_cc_raw = rt_ref.refs.get(fb.func, Shim::PushCcRaw);
                                    fb.ins().call(push_cc_raw, &[vmctx, t_v, d_v, conditions]);
                                }
                                Op::PushCatch(_) => {
                                    let tag = stack.pop().ok_or(CompileError::StackUnderflow)?;
                                    let d_v = fb.ins().iconst(types::I64, stack.len() as i64);
                                    let push_catch = rt_ref.refs.get(fb.func, Shim::PushCatch);
                                    fb.ins().call(push_catch, &[vmctx, t_v, d_v, tag]);
                                }
                                _ => unreachable!("matched Push* above"),
                            }
                            reps.truncate(stack.len());
                            write_edge_stack_to_vars(
                                &mut fb,
                                rt.as_ref(),
                                &vars,
                                &mut stack,
                                &mut reps,
                                &variable_raw,
                            );
                            // Placeholder error-value slot for the never-taken
                            // anchor edge (real entries define it from the shim).
                            let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
                            fb.def_var(vars[stack.len()], nil);
                            let never = fb.ins().iconst(types::I8, 0);
                            fb.ins()
                                .brif(never, block_for[&tu], &[], block_for[&(i + 1)], &[]);
                            terminated = true;
                            break;
                        }
                        Op::PopHandler => {
                            // Normal exit from the protected extent: drop the
                            // runtime frame and the static tracking entry.
                            let rt_ref =
                                rt.as_ref().ok_or(CompileError::UnsupportedOp("handler"))?;
                            let vmctx = fb.use_var(rt_ref.vmctx_var);
                            let pop_handler = rt_ref.refs.get(fb.func, Shim::PopHandler);
                            fb.ins().call(pop_handler, &[vmctx]);
                            handlers
                                .pop()
                                .ok_or(CompileError::UnsupportedOp("unbalanced-pophandler"))?;
                        }
                        other => {
                            let spec = spec_sites.get(&i).map(|site| {
                                (
                                    site.sym,
                                    site.expected_bits,
                                    &spec_slots[site.slot] as *const SpecSlot as i64,
                                    // R2 increment B2: the slot index the AOT sidecar's
                                    // spec-slot / spec-expected arrays are keyed by.
                                    site.slot,
                                    site.kind,
                                )
                            });
                            // A tail call's residual is dead: the emitter sees
                            // the call's own operands only, so it roots nothing.
                            let dead = lowering::tail_call_dead_residuals(
                                other,
                                ops.get(i + 1),
                                !handlers.is_empty(),
                                spec.map(|(_, _, _, _, kind)| kind),
                                stack.len(),
                            )
                            .unwrap_or(0);
                            let residual: Vec<ClifValue> = stack.drain(..dead).collect();
                            let residual_reps: Vec<SlotRep> = reps.drain(..dead).collect();
                            lowering::lower_simple_op_with_array_profile(
                                &mut fb,
                                i,
                                &mut pending_deopt,
                                &mut signal_exit,
                                constants,
                                &mut stack,
                                &mut reps,
                                rt.as_ref(),
                                &handlers,
                                &mut pending,
                                spec,
                                other,
                                &known_fixnum,
                                reloc_base,
                                reloc_index,
                                aot,
                                spec_slot_base,
                                spec_expected_base,
                                dynamic_prefix,
                                consts_base,
                            )?;
                            stack.splice(0..0, residual);
                            reps.splice(0..0, residual_reps);
                            // `lower_simple_op` keeps `reps` in lockstep with `stack`
                            // (it re-syncs after a non-unboxing op itself).
                        }
                    }
                }
                if !terminated {
                    // Fall through with the uniform variable representations.
                    write_edge_stack_to_vars(
                        &mut fb,
                        rt.as_ref(),
                        &vars,
                        &mut stack,
                        &mut reps,
                        &variable_raw,
                    );
                    fb.ins().jump(block_for[&end], &[]);
                }
                // Keep failed-guard reconstruction out of the ordinary emitted
                // path. Cranelift sinks cold blocks during final code emission;
                // this is a layout hint, not a register-allocation weight.
                // A snapshot holding a flonum boxes it there: cold too.
                for site in &pending_deopt {
                    if has_raw_slots || site.holds_flonum() {
                        fb.set_cold_block(site.block);
                    }
                }
                // Fill the precise-deopt exit blocks queued by this block's guards.
                emit_pending_deopts(
                    &mut fb,
                    deopt_refs,
                    &mut pending_deopt,
                    rt.as_ref().map(|rt| &rt.refs),
                    abi,
                );
                // Fill the handler-dispatch blocks queued by this block's signal
                // sites (the builder can switch blocks now that it's terminated).
                if !pending.is_empty() {
                    let rt_ref = rt.as_ref().expect("pending dispatches imply rt");
                    emit_pending_dispatches(
                        &mut fb,
                        rt_ref,
                        &mut signal_exit,
                        &vars,
                        &block_for,
                        &mut pending,
                    )?;
                }
            }
        }

        // Terminate the shared signal block (return STATUS_SIGNAL) iff used.
        if let Some(sb) = signal_exit {
            fb.switch_to_block(sb);
            cold_exits::mark_exit_cold(&mut fb, sb, cold_exits::ColdExit::Signal);
            reg_abi::emit_leaf_return(&mut fb, abi, None, None, STATUS_SIGNAL);
        }

        fb.seal_all_blocks();
        fb.finalize(frontend_config);
    }
    chains.extend(frames.finish());
    sink.return_builder_context(fbctx);
    LAST_IR_STATS.with(|c| {
        let (_, _, sites, slots) = c.get();
        c.set((
            func.dfg.num_insts() as u32,
            func.layout.blocks().count() as u32,
            sites,
            slots,
        ));
    });

    note_clif_size(&func);
    let (rw_emitted, rw_elided) = lowering::rootwin_counters();
    lowering::dump_clif(
        &func,
        &format!(
            "baseline ops={} rw_stores={rw_emitted} rw_elided={rw_elided} entry={entry_name}",
            ops.len()
        ),
    );
    sink.define_leaf(
        LeafEntry {
            name: entry_name,
            linkage: entry_linkage,
            signature: &sig,
        },
        func,
        super::super::stats::asm_dump::want_disasm(aot),
    )
}

/// Frontend-owned requirements, never runtime or mutator state. Threading:
/// immutable scalar requirements belong to one compilation and are passed
/// through the selected backend; workers use only their owned import list.
pub(super) struct Selection {
    pub(super) array_profile: bool,
    pub(super) sink_versions: bool,
    native_body: bool,
}

impl Selection {
    pub(super) fn is_selected(&self) -> bool {
        self.array_profile || self.sink_versions || self.native_body
    }
}

/// Normal T1 observation requires Range plus an active Tier2 profile pointer.
/// Selected native graphs still enter proof validation when supplied by an
/// explicit override, so an unsupported graph cannot bypass pass-off checks.
pub(super) fn requirements(
    aot: bool,
    t2: Option<crate::emacs_core::jit::tier2::T2Emit>,
    ops: &[Op],
    opt: Option<&crate::emacs_core::jit::opt::ir::Func>,
) -> Selection {
    let passes = jit_opt_passes();
    let array_profile =
        super::array_profile::selected(aot, t2) && ops.iter().any(|op| *op == Op::Aref);
    let sink_versions =
        !aot && opt.is_some_and(|func| !func.sink_recipes.source_sqrt_sites.is_empty());
    let native_body = !aot
        && opt.is_some_and(|func| {
            (jit_opt_mode() == OptMode::Opt && (passes.range || passes.licm || passes.sink))
                || !func.array_reads.reads.is_empty()
                || !func.sink_recipes.owners.is_empty()
                || !func.sink_recipes.source_sqrt_sites.is_empty()
        });
    Selection {
        array_profile,
        sink_versions,
        native_body,
    }
}
