//! T-S1 and T-S4: the front/backend split changes where the code is
//! produced, never the code.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, captured_code, corpus, function, mask_code_text,
};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::compile::shared::code_memory_stats;
use crate::emacs_core::jit::stats;
use crate::emacs_core::value::Value;

/// Compile every pipeline-corpus body (a fresh function object each, so each
/// compiles) through the first-sight seam under `mode`, and return its
/// machine code and CLIF, addresses masked.
fn corpus_under(mode: BgMode) -> (Vec<String>, Vec<String>) {
    force_mode_for_test(Some(mode));
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let mut code = Vec::new();
    let clif = captured_clif(|| {
        code = captured_code(|| {
            for (ops, constants, arity) in corpus() {
                let f = function(ops, constants, arity);
                assert!(
                    cache::resolve_compiled_leaf_ptr(ctx, &f).is_some(),
                    "{mode:?}: every corpus body compiles: {:?}",
                    f.ops
                );
            }
        })
    });
    force_mode_for_test(None);
    (code, clif.iter().map(|c| mask_clif(c)).collect())
}

/// [`mask_code_text`] for CLIF, whose immediates print digit-grouped
/// (`0x7fff_e833_27a0`): baked box addresses legitimately differ.
fn mask_clif(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("0x") {
        out.push_str(&rest[..at]);
        let digits = rest[at + 2..]
            .bytes()
            .take_while(|b| b.is_ascii_hexdigit() || *b == b'_')
            .count();
        if rest[at + 2..at + 2 + digits]
            .bytes()
            .filter(u8::is_ascii_hexdigit)
            .count()
            >= 6
        {
            out.push_str("0xADDR");
        } else {
            out.push_str(&rest[at..at + 2 + digits]);
        }
        rest = &rest[at + 2 + digits..];
    }
    out.push_str(rest);
    mask_code_text(&out)
}

/// T-S1: `sync` compiles the corpus to the CLIF and the machine code
/// `legacy` does, and it does go through the split.
#[test]
fn jit_bg_sync_split_is_code_and_clif_identical_to_legacy() {
    let before = code_memory_stats().split_payloads;
    let legacy = corpus_under(BgMode::Legacy);
    let after_legacy = code_memory_stats().split_payloads;
    let sync = corpus_under(BgMode::Sync);
    let after_sync = code_memory_stats().split_payloads;
    assert_eq!(legacy.0.len(), corpus().len(), "{:?}", legacy.0);
    assert_eq!(legacy.1.len(), corpus().len(), "{:?}", legacy.1);
    assert_eq!(legacy.0, sync.0, "machine code");
    assert_eq!(legacy.1, sync.1, "CLIF");
    assert_eq!(after_legacy, before, "legacy never packages a payload");
    assert_eq!(
        after_sync - after_legacy,
        corpus().len() as u64,
        "sync packages every leaf"
    );
}

/// The split run in line gives the same answers through the dispatch seam,
/// including a loop (an OSR-shaped body) and a call back into Lisp.
#[test]
fn jit_bg_sync_split_runs_the_corpus() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // (lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i))
    let (ops, constants, arity) = corpus().swap_remove(3);
    assert!(ops.iter().any(|op| matches!(op, Op::Goto(_))));
    let f = function(ops, constants, arity);
    let f_val = Value::make_bytecode(f.clone());
    let got = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(1000)])
        .expect("no signal");
    assert_eq!(got, Some(Value::make_int(1000).bits()));
    // (lambda (x) (car (cons x x)))
    let (ops, constants, arity) = corpus().swap_remove(1);
    let g = function(ops, constants, arity);
    let g_val = Value::make_bytecode(g.clone());
    let got = crate::emacs_core::jit::try_run_compiled(ctx, &g, g_val, &[Value::make_int(7)])
        .expect("no signal");
    assert_eq!(got, Some(Value::make_int(7).bits()));
    force_mode_for_test(None);
}

/// T-S4: under per-function names every compile of one id declares a
/// unique entry name in the backend's module (a recompile, a re-tier and an
/// OSR variant reuse the label).
#[test]
fn jit_bg_sync_split_names_repeated_labels_uniquely() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deopt_for_test(false);
    stats::force_observe_for_test(stats::ObserveOverride {
        stats: false,
        naming: true,
        entry_count: false,
    });
    let (ops, constants, arity) = corpus().swap_remove(0);
    let f = function(ops, constants, arity);
    for _ in 0..3 {
        crate::emacs_core::jit::compile::compile_bytecode_function_with(&f, None)
            .expect("each compile of the same label declares a fresh name");
    }
    force_mode_for_test(None);
}

#[test]
fn jit_bg_mode_parses_its_values() {
    assert_eq!(parse_mode(None), BgMode::Legacy);
    assert_eq!(parse_mode(Some("legacy")), BgMode::Legacy);
    assert_eq!(parse_mode(Some("bogus")), BgMode::Legacy);
    assert_eq!(parse_mode(Some("sync")), BgMode::Sync);
    assert_eq!(parse_mode(Some(" sync ")), BgMode::Sync);
}
