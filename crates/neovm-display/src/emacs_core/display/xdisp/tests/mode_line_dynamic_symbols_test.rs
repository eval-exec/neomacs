use super::super::{
    ModeLineRendered, format_mode_line_for_target, format_mode_line_recursive_in_state,
    format_mode_line_recursive_in_state_with_eval_rooted, mode_line_gc, mode_line_string::Risk,
    mode_line_symbol_value_in_state,
};
use super::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString};
use neovm_value::intern::{
    intern_call_names, intern_uninterned, reset_intern_calls, restore_runtime_interner,
    unintern_canonical_id,
};

fn context() -> Context {
    crate::test_utils::mode_line_dynamic_symbols_test_upper_layer_helpers::context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    })
}

use crate::test_utils::mode_line_dynamic_symbols_test_upper_layer_helpers::Walker;
use crate::test_utils::mode_line_dynamic_symbols_test_upper_layer_helpers::{render, render_into};

/// Like the existing exact-clock fixtures, run without unrelated registry
/// writers. The SAME owned formatter state/accumulator survives both halves.
/// Marker for the fresh process the exact-census fixture below spawns; the
/// exact registry epoch/count values only hold in an otherwise-unwritten process.
const P3SYM_MODE_LINE_CHILD_ENV: &str = "NEOVM_TEST_P3SYM_MODE_LINE_CHILD";

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn p3sym_c5_mode_line_held_identity_survives_fresh_intern_growth_child() {
    if std::env::var_os(P3SYM_MODE_LINE_CHILD_ENV).is_none() {
        return;
    }
    const NAME: &str = "p3sym-c5-held-pass";
    let oracle: Vec<_> = include_str!(
        "../../../../../../neovm-value/src/tagged/gc/mutator_registration/symbol_intern_pass_oracle.expected"
    )
    .lines()
    .collect();
    for (index, walker) in [Walker::Context, Walker::State, Walker::StateWithEval]
        .into_iter()
        .enumerate()
    {
        let mut ctx = context();
        ctx.set_variable("window-dead-windows-table", Value::NIL);
        ctx.gc_collect_exact(); // Only before test values/pass state exist.
        ctx.set_variable(NAME, Value::string("A"));
        let pctx = ModeLinePercentContext::default();
        let held = pctx.resolve_dynamic_symbol(NAME);
        let old_id = held.0;
        assert_eq!(
            neovm_value::intern::lookup_interned(NAME),
            Some(old_id),
            "the registry keeps the canonical identity across fresh intern growth"
        );
        let format = Value::from_sym_id(old_id); // Immediate scalar identity.
        let mut output = ModeLineRendered::default();
        render_into(&mut ctx, &pctx, format, walker, &mut output);
        let first = output.text.clone();
        assert_eq!(first, [u32::from('A')], "first half of the original pass");
        let fresh = format!("p3sym-c5-held-pass-fresh-{index}");
        assert!(neovm_value::intern::lookup_interned(&fresh).is_none());
        let holders = [
            intern("p3sym-c5-held-pass-input-left"),
            intern("p3sym-c5-held-pass-input-right"),
        ];
        for holder in holders {
            ctx.obarray_mut()
                .set_symbol_value_id(holder, Value::string(&fresh));
        }
        // Only the process registry grows here. The World remains at its old
        // extent until C5's real transaction must add chunk/side-table pairs.
        let mut gap = intern_uninterned("p3sym-c5-held-pass-gap");
        let target = (u64::from(gap.to_raw()) / 4096 + 2) * 4096 + 17;
        while u64::from(gap.to_raw()) + 1 < target {
            gap = intern_uninterned("p3sym-c5-held-pass-gap");
        }
        let pass_epoch = pctx.dynamic_symbols.borrow().epoch;
        // No VariableView, RefCell borrow, string Value or Context view enters
        // this closure. Held identity/pass/output are accessed only on return.
        let receipt = neovm_eval::test_support::authority::admit_owned_proof_parallel_for_test(
            &mut ctx,
            |projection| {
                // Value owns the C5 transaction; eval owns the JIT counter
                // type, so the probe crosses as a plain function pointer.
                let receipt = neovm_value::test_support::authority::c5_intern_growth_for_test(
                    projection.era(),
                    holders,
                    neovm_eval::test_support::authority::c5_function_epoch_probe_for_test(),
                );
                let (collector, keeper) = projection.finish().expect("C5 pass clean era finish");
                (collector, keeper, receipt)
            },
        )
        .expect("C5 pass owned admission")
        .expect("C5 pass history after normal serial recovery");
        assert!(ctx.obarray_view().admission_matches_canonical());
        assert!(receipt.after_chunks() >= receipt.before_chunks() + 2);
        assert_eq!(receipt.after_chunks(), receipt.after_sides());
        assert!(receipt.after_sides() > receipt.before_sides());
        assert_eq!(
            receipt.registry_after(),
            (
                receipt.registry_before().0 + 1,
                receipt.registry_before().1 + 1
            )
        );
        assert!(receipt.registry_after().0 > pass_epoch);
        assert_eq!(
            pctx.dynamic_symbols.borrow().epoch,
            pass_epoch,
            "the original pass was retained"
        );
        assert_eq!(held.0, old_id);
        assert_eq!(
            held.value(ctx.variables(), &ctx.edit.buffers)
                .and_then(|value| value.as_utf8_str()),
            Some("A")
        );
        assert_eq!(
            neovm_value::intern::lookup_interned(NAME),
            Some(old_id),
            "fresh intern growth leaves the canonical identity unchanged"
        );
        reset_intern_calls();
        let resolved = pctx.resolve_dynamic_symbol(NAME);
        assert_eq!(resolved.0, old_id);
        assert_eq!(
            intern_call_names(),
            vec![NAME.to_owned()],
            "same pass re-resolves exactly once after the fresh epoch"
        );
        assert_eq!(
            pctx.dynamic_symbols.borrow().epoch,
            receipt.registry_after().0
        );
        reset_intern_calls();
        render_into(&mut ctx, &pctx, format, walker, &mut output);
        assert!(
            !intern_call_names().iter().any(|name| name == NAME),
            "subsequent pass reads stay warm"
        );
        let rendered: String = output
            .text
            .iter()
            .map(|&code| char::from_u32(code).unwrap())
            .collect();
        assert_eq!(
            neovm_eval::emacs_core::print::print_value(&Value::string(&rendered)),
            oracle[0]
        );
        assert_eq!(
            &output.text[..first.len()],
            first,
            "original output preserved"
        );
        assert_eq!(
            neovm_eval::emacs_core::print::print_value(&if resolved.0 == old_id {
                Value::T
            } else {
                Value::NIL
            }),
            oracle[1]
        );
        assert_eq!(ctx.variables().boundp(receipt.winner()), oracle[2] == "t");
        eprintln!(
            "P3SYM_C5_HELD_PASS walker={index} epoch_delta=1 ids_delta=1 chunks_added={} same_identity=1 output={rendered}",
            receipt.after_chunks() - receipt.before_chunks()
        );
    }
}

#[cfg(not(feature = "gc-memory-telemetry"))]
#[test]
fn p3sym_c5_mode_line_held_identity_survives_fresh_intern_growth() {
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "p3sym_c5_mode_line_held_identity_survives_fresh_intern_growth_child",
            "--test-threads",
            "1",
        ])
        .env(P3SYM_MODE_LINE_CHILD_ENV, "1")
        .env_remove("RUST_LOG")
        .output()
        .expect("run the exact-census child");
    assert!(
        output.status.success(),
        "exact-census child failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn mode_line_dynamic_symbols_intern_once_then_warm_all_walkers() {
    let mut ctx = context();
    let name = "ttykey-k5-dynamic-all-walkers";
    ctx.set_variable(name, Value::string("X"));
    // Initialize the independent buffer-slot guard before testing identities.
    mode_line_symbol_value_in_state(ctx.variables(), &[], &ctx.edit.buffers, name);
    let symbol = Value::from_sym_id(intern(name));
    assert_eq!(symbol.as_bare_symbol_name(), Some(name));
    let conditional = Value::list(vec![symbol, Value::string("T"), Value::string("F")]);
    let format = Value::list(vec![Value::string(""), symbol, conditional, symbol]);
    for walker in [Walker::Context, Walker::State, Walker::StateWithEval] {
        let pctx = ModeLinePercentContext::default();
        reset_intern_calls();
        pctx.resolve_dynamic_symbol(name);
        assert_eq!(intern_call_names(), vec![name.to_owned()]);
        let before = pctx.dynamic_symbols.borrow().epoch;
        reset_intern_calls();
        let output = render(&mut ctx, &pctx, format, walker);
        let calls = intern_call_names();
        assert_eq!(output, "XTX");
        // Other tests may concurrently unintern unrelated names. A real
        // registry invalidation legitimately requires another resolution;
        // the warm-count assertion covers exactly an unchanged epoch.
        if before == symbol_registry_epoch_value() {
            assert!(
                !calls.iter().any(|called| called == name),
                "warm pass interned {calls:?}"
            );
        }
    }
}

/// The formatter reads a symbol-valued format element by identity, as GNU's
/// `display_mode_element` does (`XSYMBOL (elt)`); a canonical symbol must not
/// need a by-name resolution at any of the three walkers.
#[test]
fn canonical_format_symbols_render_without_interning() {
    let mut ctx = context();
    let name = "ttykey-k13-canonical-format-symbol";
    ctx.set_variable(name, Value::string("X"));
    let symbol = Value::from_sym_id(intern(name));
    let format = Value::list(vec![Value::string(""), symbol]);
    for walker in [Walker::Context, Walker::State, Walker::StateWithEval] {
        let pctx = ModeLinePercentContext::default();
        reset_intern_calls();
        let output = render(&mut ctx, &pctx, format, walker);
        assert_eq!(output, "X");
        let calls = intern_call_names();
        assert!(
            !calls.iter().any(|called| called == name),
            "canonical format symbol interned on a cold pass: {calls:?}"
        );
    }
}

#[test]
fn mode_line_dynamic_symbols_new_pass_resolves_again() {
    let first = ModeLinePercentContext::default();
    let name = "ttykey-k5-dynamic-new-pass";
    let first_id = first.resolve_dynamic_symbol(name).0;
    reset_intern_calls();
    let second = ModeLinePercentContext::default();
    assert_eq!(second.resolve_dynamic_symbol(name).0, first_id);
    assert_eq!(intern_call_names(), vec![name.to_owned()]);
}

#[test]
fn mode_line_dynamic_symbols_promote_without_forgetting_inline_entries() {
    let mut symbols = DynamicSymbols::default();
    let names = [
        "ttykey-k5-dynamic-inline-a",
        "ttykey-k5-dynamic-inline-b",
        "ttykey-k5-dynamic-inline-c",
        "ttykey-k5-dynamic-inline-d",
        "ttykey-k5-dynamic-inline-e",
        "ttykey-k5-dynamic-inline-f",
    ];
    let ids: Vec<_> = names.iter().map(|name| intern(name)).collect();
    // Use the same insertion seam as resolve, isolating representation
    // promotion from unrelated parallel tests' registry invalidations.
    for (&name, &id) in names.iter().zip(&ids) {
        symbols.remember(name, id);
    }
    assert!(matches!(symbols.names, Names::Hashed(_)));
    let Names::Hashed(entries) = &symbols.names else {
        panic!("fifth entry must promote");
    };
    for (name, id) in names.into_iter().zip(ids) {
        assert_eq!(entries.get(name), Some(&id));
    }
}

#[test]
fn mode_line_dynamic_symbols_read_live_local_default_and_alias_values() {
    let mut ctx = context();
    ctx.eval_str(
        "(progn (setq ttykey-k5-dynamic-default 11 ttykey-k5-dynamic-other 22)
                         (defvaralias 'ttykey-k5-dynamic-alias 'ttykey-k5-dynamic-default))",
    )
    .expect("default and alias");
    let pctx = ModeLinePercentContext::default();
    let name = "ttykey-k5-dynamic-default";
    let query = pctx.resolve_dynamic_symbol(name);
    assert_eq!(
        query.value(ctx.variables(), &ctx.edit.buffers),
        Some(Value::fixnum(11))
    );
    let buffer = ctx.edit.buffers.current_buffer_id().expect("buffer");
    ctx.set_buffer_local_binding_by_id(buffer, intern(name), Value::fixnum(33))
        .expect("local binding");
    assert_eq!(
        pctx.resolve_dynamic_symbol(name)
            .value(ctx.variables(), &ctx.edit.buffers),
        Some(Value::fixnum(33))
    );
    let alias = "ttykey-k5-dynamic-alias";
    let before = pctx
        .resolve_dynamic_symbol(alias)
        .value(ctx.variables(), &ctx.edit.buffers);
    assert_eq!(
        before,
        mode_line_symbol_value_in_state(ctx.variables(), &[], &ctx.edit.buffers, alias)
    );
    ctx.eval_str("(defvaralias 'ttykey-k5-dynamic-alias 'ttykey-k5-dynamic-other)")
        .expect("retarget alias");
    assert_eq!(
        pctx.resolve_dynamic_symbol(alias)
            .value(ctx.variables(), &ctx.edit.buffers),
        Some(Value::fixnum(22))
    );
}

#[test]
fn mode_line_dynamic_symbols_read_live_projected_owner_bindings() {
    let mut ctx = context();
    let name = "ttykey-k5-dynamic-projected-owner";
    ctx.set_variable(name, Value::fixnum(41));
    let id = intern(name);
    ctx.project_thread_variable_for_test(id)
        .expect("plain cell projects");
    let pctx = ModeLinePercentContext::default();
    assert_eq!(
        pctx.resolve_dynamic_symbol(name)
            .value(ctx.variables(), &ctx.edit.buffers),
        Some(Value::fixnum(41))
    );
    let depth = ctx.specpdl().len();
    ctx.try_specbind(id, Value::fixnum(42))
        .expect("owner binding");
    let value = pctx
        .resolve_dynamic_symbol(name)
        .value(ctx.variables(), &ctx.edit.buffers);
    ctx.unbind_to(depth);
    assert_eq!(value, Some(Value::fixnum(42)));
    assert_eq!(
        pctx.resolve_dynamic_symbol(name)
            .value(ctx.variables(), &ctx.edit.buffers),
        Some(Value::fixnum(41))
    );
}

#[test]
fn mode_line_dynamic_symbols_eval_observes_live_values_and_risky_properties() {
    let mut ctx = context();
    let format = ctx
        .eval_str(
            "(progn
           (setq ttykey-k5-dynamic-callback-value \"A\"
                 ttykey-k5-dynamic-callback-risk '(:eval \"R\"))
           (put 'ttykey-k5-dynamic-callback-risk 'risky-local-variable t)
           '(\"\" ttykey-k5-dynamic-callback-value
             ttykey-k5-dynamic-callback-risk
             (:eval (progn
                      (setq ttykey-k5-dynamic-callback-value \"B\")
                      (put 'ttykey-k5-dynamic-callback-risk 'risky-local-variable nil)
                      nil))
             ttykey-k5-dynamic-callback-value
             ttykey-k5-dynamic-callback-risk))",
        )
        .expect("callback format");
    let pctx = ModeLinePercentContext::default();
    assert_eq!(render(&mut ctx, &pctx, format, Walker::Context), "ARB");
}

#[test]
fn mode_line_dynamic_symbols_split_callback_keeps_live_value_reads() {
    let mut ctx = context();
    let name = "ttykey-k5-dynamic-split-value";
    ctx.set_variable(name, Value::list(vec![Value::string("A")]));
    let source = ctx.default_value_only_by_name(name).expect("source list");
    // Risky evaluation does not affect an ordinary list of literal strings.
    let symbol = Value::from_sym_id(intern(name));
    let format = Value::list(vec![
        Value::string(""),
        symbol,
        Value::list(vec![Value::symbol(":eval"), Value::NIL]),
        symbol,
    ]);
    let pctx = ModeLinePercentContext::default();
    let roots = mode_line_gc::ScratchRoots::new();
    let mut result = ModeLineRendered::default();
    let mut callbacks = 0;
    format_mode_line_recursive_in_state_with_eval_rooted(
        ctx.variables(),
        &[],
        &ctx.edit.buffers,
        &ctx.processes(),
        &pctx,
        &format,
        &mut result,
        0,
        false,
        &mut |_, _| {
            callbacks += 1;
            source.set_car(Value::string("B"));
            Ok(Value::NIL)
        },
        &roots,
    )
    .expect("split callback");
    assert_eq!(result.text, vec![u32::from('A'), u32::from('B')]);
    assert_eq!(callbacks, 1);
}

#[test]
fn mode_line_dynamic_symbols_eval_retains_object_after_actual_registry_change() {
    let mut ctx = context();
    let name = "ttykey-k5-dynamic-eval-reintern";
    let format = ctx
        .eval_str(
            "(progn
           (setq ttykey-k5-dynamic-eval-reintern \"old\")
           '(\"\" ttykey-k5-dynamic-eval-reintern
             (:eval (progn
                      (unintern \"ttykey-k5-dynamic-eval-reintern\" nil)
                      (set (intern \"ttykey-k5-dynamic-eval-reintern\") \"new\")
                      (setq ttykey-k5-dynamic-eval-did-reintern t)
                      nil))
             ttykey-k5-dynamic-eval-reintern))",
        )
        .expect("unintern callback format");
    let old = intern(name);
    let pctx = ModeLinePercentContext::default();
    let output = render(&mut ctx, &pctx, format, Walker::Context);
    assert_eq!(
        ctx.default_value_only_by_name("ttykey-k5-dynamic-eval-did-reintern"),
        Some(Value::T),
        "the callback must reach its successful reintern marker"
    );
    // GNU retained-during-reintern TTY oracle produces OLDOLD.
    assert_eq!(output, "oldold");
    assert_ne!(intern(name), old, "actual registry identity changed");
    assert_eq!(pctx.resolve_dynamic_symbol(name).0, intern(name));
}

#[test]
fn mode_line_dynamic_symbols_restore_invalidates_only_pass_identities() {
    let mut ctx = context();
    let name = "ttykey-k5-dynamic-restore";
    ctx.set_variable(name, Value::fixnum(51));
    let pctx = ModeLinePercentContext::default();
    let old = pctx.resolve_dynamic_symbol(name).0;
    assert!(unintern_canonical_id(old));
    let restored = restore_runtime_interner(&[LispString::from_utf8(name)], &[0], Some(&[true]))
        .expect("unique canonical restore");
    let new = restored.into_parts().1[0];
    assert_ne!(new, old);
    ctx.obarray_mut()
        .set_symbol_value_id(new, Value::fixnum(52));
    assert_eq!(
        pctx.resolve_dynamic_symbol(name)
            .value(ctx.variables(), &ctx.edit.buffers),
        Some(Value::fixnum(52))
    );
    assert_eq!(pctx.resolve_dynamic_symbol(name).0, new);
}

#[test]
fn mode_line_dynamic_symbols_keep_object_identity_for_noncanonical_and_encoded_names() {
    let mut ctx = context();
    for (name, atom) in [
        (
            "ttykey-k5-dynamic-unibyte-é",
            LispString::from_unibyte_slice("ttykey-k5-dynamic-unibyte-é".as_bytes()),
        ),
        (
            "ttykey-k5-dynamic-supplied",
            LispString::from_emacs_bytes_with_declared_chars(
                b"ttykey-k5-dynamic-supplied".to_vec(),
                GnuDeclaredCharacterCount::try_from(1_i128).expect("declared count"),
            ),
        ),
    ] {
        let input_id = ctx.obarray_mut().intern_lisp_string(&atom);
        let spelling_id = intern(name);
        assert_ne!(input_id, spelling_id);
        ctx.obarray_mut()
            .set_symbol_value_id(input_id, Value::string("input-object"));
        ctx.obarray_mut()
            .set_symbol_value_id(spelling_id, Value::string("spelling"));
        let format = Value::from_sym_id(input_id);
        for walker in [Walker::Context, Walker::State, Walker::StateWithEval] {
            assert_eq!(
                render(&mut ctx, &ModeLinePercentContext::default(), format, walker),
                "input-object"
            );
        }
    }
    let name = "ttykey-k5-dynamic-noncanonical";
    ctx.set_variable(name, Value::string("canonical"));
    let uninterned = intern_uninterned(name);
    ctx.obarray_mut()
        .set_symbol_value_id(uninterned, Value::string("uninterned"));
    assert_eq!(
        render(
            &mut ctx,
            &ModeLinePercentContext::default(),
            Value::from_sym_id(uninterned),
            Walker::Context
        ),
        "uninterned"
    );
}

#[test]
fn mode_line_dynamic_symbols_physical_atom_and_conditional_admission_matches_gnu() {
    let mut ctx = context();
    let name = "ttykey-k5-dynamic-positioned";
    ctx.set_variable(name, Value::string("positioned"));
    // SAFETY: the test owns this Context's World on this thread; the call
    // only allocates through the heap view.
    let positioned = unsafe { ctx.tagged_heap_mut() }
        .alloc_symbol_with_pos(Value::from_sym_id(intern(name)), Value::fixnum(3));
    assert!(
        positioned
            .as_symbol_name(neovm_value::tagged::SymbolMode::Bare)
            .is_none()
    );
    assert_eq!(
        positioned.as_symbol_name(neovm_value::tagged::SymbolMode::Positioned),
        Some(name)
    );
    ctx.set_variable("symbols-with-pos-enabled", Value::T);
    assert_eq!(
        ctx.symbol_mode(),
        neovm_value::tagged::SymbolMode::Positioned
    );
    assert_eq!(
        render(
            &mut ctx,
            &ModeLinePercentContext::default(),
            positioned,
            Walker::Context
        ),
        "*invalid*"
    );
    // GNU's conditional CAR uses SYMBOLP/XSYMBOL, unlike atom dispatch.
    let actual = intern_uninterned(name);
    ctx.obarray_mut().set_symbol_value_id(actual, Value::NIL);
    ctx.obarray_mut()
        .set_symbol_value_id(intern(name), Value::T);
    // SAFETY: same owning-thread argument as the heap allocation above.
    let positioned_actual = unsafe { ctx.tagged_heap_mut() }
        .alloc_symbol_with_pos(Value::from_sym_id(actual), Value::fixnum(7));
    let conditional = Value::list(vec![
        positioned_actual,
        Value::string("TRUE"),
        Value::string("FALSE"),
    ]);
    assert_eq!(
        render(
            &mut ctx,
            &ModeLinePercentContext::default(),
            conditional,
            Walker::Context
        ),
        "FALSE"
    );
    let atom = LispString::from_unibyte_slice(b"ttykey-k5-dynamic-invalid-\xff");
    let invalid = ctx.obarray_mut().intern_lisp_string(&atom);
    ctx.obarray_mut()
        .set_symbol_value_id(invalid, Value::string("ACTUAL-INTERNED-BYTE"));
    assert!(Value::from_sym_id(invalid).as_bare_symbol_name().is_none());
    for walker in [Walker::Context, Walker::State, Walker::StateWithEval] {
        assert_eq!(
            render(
                &mut ctx,
                &ModeLinePercentContext::default(),
                Value::from_sym_id(invalid),
                walker
            ),
            "ACTUAL-INTERNED-BYTE"
        );
    }
}

#[test]
fn mode_line_dynamic_symbols_plain_void_and_alias_void_match_owner_fallback() {
    let mut ctx = context();
    let projected = "ttykey-k5-dynamic-void-owner-present";
    ctx.set_variable(projected, Value::fixnum(61));
    let projected_id = intern(projected);
    ctx.project_thread_variable_for_test(projected_id)
        .expect("project owner variable");
    let depth = ctx.specpdl().len();
    ctx.try_specbind(projected_id, Value::fixnum(62))
        .expect("owner slab active");
    ctx.eval_str(
        "(progn (setq ttykey-k5-dynamic-plain-void 63)
                         (defvaralias 'ttykey-k5-dynamic-alias-void 'ttykey-k5-dynamic-plain-void)
                         (makunbound 'ttykey-k5-dynamic-plain-void))",
    )
    .expect("void plain and alias cells");
    let pctx = ModeLinePercentContext::default();
    for name in [
        "ttykey-k5-dynamic-plain-void",
        "ttykey-k5-dynamic-alias-void",
        "ttykey-k5-dynamic-never-bound",
    ] {
        let actual = pctx
            .resolve_dynamic_symbol(name)
            .value(ctx.variables(), &ctx.edit.buffers);
        assert_eq!(
            actual,
            mode_line_symbol_value_in_state(ctx.variables(), &[], &ctx.edit.buffers, name)
        );
        assert!(
            actual.is_none(),
            "owner slab must not invent a value for {name}"
        );
    }
    ctx.unbind_to(depth);
}

/// GNU 31.1 initialized-TTY oracle: make-symbol's value and risk property,
/// not those of an interned same-name symbol, own the formatter's reads.
#[test]
fn mode_line_symbol_object_identity_and_conditional_match_gnu() {
    let mut ctx = context();
    let name = "mode-line-symbol-object-identity";
    let canonical = intern(name);
    let actual = intern_uninterned(name);
    ctx.obarray_mut()
        .set_symbol_value_id(canonical, Value::string("CANONICAL"));
    ctx.obarray_mut()
        .set_symbol_value_id(actual, Value::string("ACTUAL"));
    let format = Value::from_sym_id(actual);
    assert_eq!(
        render(
            &mut ctx,
            &ModeLinePercentContext::default(),
            format,
            Walker::Context
        ),
        "ACTUAL"
    );
    ctx.obarray_mut().set_symbol_value_id(actual, Value::NIL);
    ctx.obarray_mut().set_symbol_value_id(canonical, Value::T);
    let conditional = Value::list(vec![format, Value::string("TRUE"), Value::string("FALSE")]);
    assert_eq!(
        render(
            &mut ctx,
            &ModeLinePercentContext::default(),
            conditional,
            Walker::Context
        ),
        "FALSE"
    );
}
