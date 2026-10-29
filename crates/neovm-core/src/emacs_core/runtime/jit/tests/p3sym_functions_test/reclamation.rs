//! Stopped GC over real installed/retired native leaves and their root census.
use super::*;
use crate::tagged::gc::CollectionReport;

struct Settings {
    tier: Option<compile::Tier2Knob>,
}
impl Settings {
    fn enter(named: bool) -> Self {
        let tier = compile::tier2_forced_for_test();
        compile::force_tier2_for_test(Some(compile::Tier2Knob {
            on: false,
            window: compile::Tier2Knob::DEFAULT_WINDOW,
            loop_credit: compile::Tier2Knob::DEFAULT_LOOP_CREDIT,
        }));
        compile::force_inline2_for_test(Some(if named {
            compile::Inline2Mode::Named
        } else {
            compile::Inline2Mode::Off
        }));
        Self { tier }
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        compile::force_inline2_for_test(None);
        compile::force_tier2_for_test(self.tier);
    }
}

fn reloc_count(ctx: &Context, word: usize) -> usize {
    let mut roots = Vec::new();
    cache::collect_jit_reloc_gc_roots_for_heap(&mut roots, ctx.tagged_heap.identity());
    roots.iter().filter(|value| value.bits() == word).count()
}

fn stopped_collection(ctx: &mut Context, old_word: usize) -> (CollectionReport, bool, bool) {
    // A fresh legacy census is essential: a previous era's frozen roots may
    // legitimately contain a definition that was live when that era started.
    // No running owner or legacy entry is needed for these lifetime checks;
    // the preceding edit uses the real two-owner bridge.
    let result = ctx
        .admit_owned_proof_parallel(|projection| {
            let result = (|| {
                let era = projection.era();
                let mut stopped = era.stop().map_err(|e| format!("GC stop: {e:?}"))?;
                let before = era
                    .with_stopped_heap_for_test(&stopped, |heap| {
                        heap.owns_heap_value_for_test(Value::from_bits(old_word))
                    })
                    .map_err(|e| format!("before GC membership: {e:?}"))?;
                let collected = era.collect_stopped(&mut stopped);
                let after = era.with_stopped_heap_for_test(&stopped, |heap| {
                    // Allocation membership only; never decode an old payload.
                    heap.owns_heap_value_for_test(Value::from_bits(old_word))
                });
                let completed = stopped.complete();
                let report = collected.map_err(|e| format!("stopped GC: {e:?}"))?;
                let after = after.map_err(|e| format!("after GC membership: {e:?}"))?;
                completed.map_err(|e| format!("GC complete: {e:?}"))?;
                Ok::<_, String>((report, before, after))
            })();
            let (collector, keeper) = projection.finish().expect("definition GC clean finish");
            (collector, keeper, result)
        })
        .expect("definition GC owned admission");
    // Assertions and error reporting follow stop retirement and restoration
    // of the original serial Context, independently of the observed liveness.
    result.expect("definition GC after clean serial recovery")
}

fn history(named: bool) {
    let kind = if named { "named" } else { "mir" };
    let _profit = Profit::bypass();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Legacy);
    let _settings = Settings::enter(named);
    let (mut ctx, ids, targets, callers, replacement) = fixture(&format!("p3sym-c4-gc-{kind}"));
    let old_word = targets[0].bits();
    let _roots = ctx.retain_gc_roots(callers.into_iter().chain([replacement]));
    // Scalar MIR embeds the result, not the old bytecode definition. Keep
    // an explicit old-definition lease for the first collection in that row.
    // Named v2 instead retains its actual selected bytecode in relocations.
    let old_lease = (!named).then(|| ctx.retain_gc_roots([targets[0]]));
    if named {
        let threshold = super::super::retier_heat().expect("fixture enables heat admission");
        callers[0]
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .set_heat_for_test(threshold);
    }
    let before = printed(native(&mut ctx, callers[0]).unwrap());
    assert_eq!(
        format!(
            "({before} {})",
            direct(&mut ctx, Value::from_sym_id(ids[0]))
        ),
        oracle()[0]
    );
    // NEOVM_JIT_INLINE controls the bytecode fuser. Legacy MIR still inlines
    // this pure scalar callee with that setting off; require its real
    // dependency in both configurations rather than guessing from the knob.
    assert_eq!(cache::inline_dependent_count_for_test(ids[0]), 1);
    let selected_roots = reloc_count(&ctx, old_word);
    assert_eq!(selected_roots > 0, named, "actual compiled root census");
    let caller_id = callers[0]
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .compiled_id_or_assign();
    let original_leaf = cache::compiled_leaf_ptr_for_test(caller_id).map(|p| p as usize);
    assert!(original_leaf.is_some());
    test_support::settle(&mut ctx);
    let clock = ctx.obarray.function_epoch();
    let source = input(&mut ctx, replacement);
    ctx.c4_function_era_for_test(ids, FunctionEraEdit::Replace { symbol: 0 }, source, |_| {});
    coherent(&ctx, clock + 1);
    assert_eq!(cache::inline_dependent_count_for_test(ids[0]), 0);
    assert!(cache::compiled_leaf_ptr_for_test(caller_id).is_none());
    let first_after = printed(native(&mut ctx, callers[0]).unwrap());
    let (named_clock, named_target) = ctx.c4_resolve_named_for_test(ids[0]);
    assert_eq!(
        (named_clock, named_target),
        (clock + 1, Some(replacement.bits()))
    );
    let retired_roots = reloc_count(&ctx, old_word);
    assert_eq!(
        retired_roots > 0,
        named,
        "retired leaf root census after keeper eviction"
    );
    let (retained, before_gc, after_gc) = stopped_collection(&mut ctx, old_word);
    assert!(retained.major);
    assert_eq!(retained.frozen_owners, 0);
    assert!(before_gc && after_gc, "old definition remains rooted");
    coherent(&ctx, clock + 1);
    let old_answer = direct(&mut ctx, Value::from_bits(old_word));
    let current_answer = direct(&mut ctx, Value::from_sym_id(ids[0]));
    assert_eq!(
        format!("({old_answer} {first_after} {current_answer})"),
        oracle()[3]
    );
    eprintln!(
        "P3SYM_C4_DEFINITION_GC kind={kind} phase=retained cycle={} major={} old_before=1 old_after=1 reloc_old={retired_roots}",
        retained.cycle, retained.major
    );
    drop(old_lease);
    if named {
        // Keeper eviction deliberately retains executable leaves for their
        // possible native callers. At this explicit quiescent boundary no
        // activation is live; release all actual cache leaves before claiming
        // reclamation. This is not automatic retirement-store reclamation.
        test_support::settle(&mut ctx);
        assert_eq!(cache::native_depth(), 0);
        cache::clear();
    }
    assert_eq!(reloc_count(&ctx, old_word), 0);
    let (released, before_gc, after_gc) = stopped_collection(&mut ctx, old_word);
    assert!(released.major);
    assert_eq!(released.frozen_owners, 0);
    assert!(
        before_gc && !after_gc,
        "unrooted old definition is reclaimed"
    );
    assert!(released.cycle > retained.cycle);
    coherent(&ctx, clock + 1);
    eprintln!(
        "P3SYM_C4_DEFINITION_GC kind={kind} phase=released cycle={} major={} old_before=1 old_after=0 reloc_old=0",
        released.cycle, released.major
    );
    // Never decode or execute old_word again. Current callers remain usable.
    assert_eq!(
        format!(
            "({} {})",
            printed(native(&mut ctx, callers[0]).unwrap()),
            direct(&mut ctx, Value::from_sym_id(ids[0]))
        ),
        oracle()[5]
    );
    coherent(&ctx, clock + 1);
}

#[test]
fn p3sym_c4_compiled_scalar_definition_reclaims_after_lease_release() {
    history(false);
}

#[test]
fn p3sym_c4_compiled_named_definition_reclaims_after_quiescent_cache_release() {
    if !super::super::inline::jit_inline_on() {
        eprintln!("P3SYM_C4_RECLAMATION_INAPPLICABLE NEOVM_JIT_INLINE=off");
        return;
    }
    history(true);
}
