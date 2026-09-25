use super::report::{FinalReport, LeafReportRow, ranked_leaves};
use super::*;

fn stats_with(compiles: u64, entries: u64, mir_taken: u64) -> CompileStats {
    CompileStats {
        total_compiles: compiles,
        compiled_ok: compiles,
        native_entries: entries,
        mir_taken,
        total_us: compiles * 10,
        ..CompileStats::default()
    }
}

fn body_of(lines: &[(ReportTag, String)], tag: ReportTag) -> &str {
    lines
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, b)| b.as_str())
        .unwrap_or_else(|| panic!("no {tag:?} line in {lines:?}"))
}

/// Every section of the exit report prints, with the keys tooling greps for.
#[test]
fn jit_final_report_renders_every_section() {
    let report = FinalReport {
        pid: 42,
        since_command_loop_ms: Some(812),
        compile: stats_with(452, 4975, 53),
        compile_since_loop: Some(stats_with(4, 100, 1)),
        mir_bails: "gate:generic-call:call=6".to_string(),
        inline: String::new(),
        osr_transfers: 2,
        seam_fallbacks: 17,
        function_epoch: 91234,
        epoch: {
            let mut e = super::epoch::EpochCounters::default();
            e.bumps[crate::emacs_core::symbol::FunctionEpochBump::Defalias as usize] = 9877;
            e.bumps[crate::emacs_core::symbol::FunctionEpochBump::Fset as usize] = 41;
            e.unchanged_writes = 233;
            e.spec[super::epoch::SpecRevalidation::Rearmed as usize] = 88;
            e.spec[super::epoch::SpecRevalidation::BindingChanged as usize] = 2;
            e
        },
        epoch_since_loop: Some({
            let mut e = super::epoch::EpochCounters::default();
            e.bumps[crate::emacs_core::symbol::FunctionEpochBump::Defalias as usize] = 500;
            e.spec[super::epoch::SpecRevalidation::Rearmed as usize] = 40;
            e
        }),
        redefined_top: "cl--generic-dispatcher=41".to_string(),
        leaves: vec![
            leaf_row(37, 5_999_998, 0),
            leaf_row(38, 0, 0),
            LeafReportRow {
                state: "retired",
                ..leaf_row(39, 0, 2)
            },
        ],
        dropped: crate::emacs_core::jit::compile::LeafTotals {
            leaves: 1,
            deopt_at: 5,
            deopt_rerun: 0,
            signals: 1,
        },
    };
    let lines = report.render();
    let tags: Vec<&'static str> = lines.iter().map(|(t, _)| (*t).into()).collect();
    assert_eq!(
        tags,
        [
            "neovm-jit-final",
            "neovm-jit-final-mir-bails",
            "neovm-jit-final-inline",
            "neovm-jit-final-runs",
            "neovm-jit-final-fn-epoch",
            "neovm-jit-final-fn-epoch-top",
            "neovm-jit-final-leaf",
            "neovm-jit-final-leaf",
        ]
    );
    let head = body_of(&lines, ReportTag::Final);
    assert!(head.starts_with("pid=42 since_command_loop_ms=812 compiles=452 "));
    assert!(head.contains("mir[taken=53 "), "{head}");
    assert!(
        head.contains("| since_command_loop: compiles=4 ok=4 native_entries=100 "),
        "{head}"
    );
    assert_eq!(
        body_of(&lines, ReportTag::FinalMirBails),
        "gate:generic-call:call=6"
    );
    assert_eq!(body_of(&lines, ReportTag::FinalInline), "-", "empty census");
    assert_eq!(
        body_of(&lines, ReportTag::FinalRuns),
        "entries_seam=4975 deopt_at=6000003 deopt_rerun=2 signals=1 osr_transfers=2 \
         seam_fallbacks=17 leaves_live=2 leaves_retired=1 leaves_osr=0 leaves_dropped=1"
    );
    let leaf_lines: Vec<&str> = lines
        .iter()
        .filter(|(t, _)| *t == ReportTag::FinalLeaf)
        .map(|(_, b)| b.as_str())
        .collect();
    assert_eq!(
        leaf_lines,
        [
            "id=37 name=j4-add tier=mir state=live osr_pc=- deopt_at=5999998 deopt_rerun=0 \
             signals=0 regalloc=fast clif=58 pcs=12:5999998/Mul,other:3",
            "id=39 name=j4-add tier=mir state=retired osr_pc=- deopt_at=0 deopt_rerun=2 \
             signals=0 regalloc=fast clif=58 pcs=-",
        ],
        "only leaves that deopted, most first"
    );

    assert_eq!(
        body_of(&lines, ReportTag::FinalFnEpoch),
        "epoch=91234 total=9918 fset=41 defalias=9877 internal-cell-write=0 pdump-restore=0 \
         fmakunbound=0 silent-clear=0 unintern=0 subr-rewrite=0 compiler-overrides=0 \
         unchanged-writes=233 inline-evicted-leaves=0 spec-rearm=88 spec-rebind=2 \
         | since_command_loop: total=500 defalias=500 spec-rearm=40"
    );
    assert_eq!(
        body_of(&lines, ReportTag::FinalFnEpochTop),
        "cl--generic-dispatcher=41"
    );

    // Without a command-loop mark there is no delta section.
    let unmarked = FinalReport {
        since_command_loop_ms: None,
        compile_since_loop: None,
        epoch_since_loop: None,
        ..report
    };
    let head = unmarked.render().remove(0).1;
    assert!(head.starts_with("pid=42 since_command_loop_ms=- "));
    assert!(!head.contains("since_command_loop:"), "{head}");
}

/// Stats snapshotted at the command-loop mark subtract out: the delta is
/// exactly what happened after the mark.
#[test]
fn jit_final_report_since_command_loop_reports_deltas() {
    reset_compile_stats();
    record_mir(MirFunnel::Taken);
    record_retier();
    let before = compile_stats_snapshot();
    record_mir(MirFunnel::Taken);
    record_mir(MirFunnel::Taken);
    record_mir(MirFunnel::InlinedCallees(3));
    let after = compile_stats_snapshot();
    let delta = after.since(&before);
    assert_eq!(delta.mir_taken, 2);
    assert_eq!(delta.mir_inlined_callees, 3);
    assert_eq!(delta.retiers, 0, "the retier happened before the mark");
    assert_eq!(after.mir_taken, 3);
}

fn leaf_row(id: u64, deopt_at: u64, deopt_rerun: u64) -> LeafReportRow {
    LeafReportRow {
        id,
        name: Some("j4-add".to_string()),
        tier: "mir",
        state: "live",
        osr_pc: None,
        regalloc: "fast",
        clif_insts: 58,
        deopt_at,
        deopt_rerun,
        signals: 0,
        deopt_pcs: if deopt_at > 0 {
            vec![(12, deopt_at, Some("Mul".to_string()))]
        } else {
            Vec::new()
        },
        deopt_pc_overflow: if deopt_at > 0 { 3 } else { 0 },
    }
}

/// Leaves rank by deopts (precise plus rerun), most first, ties by id, and
/// each section is capped.
#[test]
fn jit_final_report_leaf_rows_sorted_by_deopts() {
    let mut rows: Vec<LeafReportRow> = (1..=40).map(|id| leaf_row(id, id % 7, 0)).collect();
    rows.push(leaf_row(100, 0, 50));
    let ranked = ranked_leaves(&rows);
    assert_eq!(ranked.len(), super::report::LEAF_ROWS_PER_SECTION);
    assert_eq!(ranked[0].id, 100, "a rerun deopt counts too");
    let deopts: Vec<u64> = ranked.iter().map(|r| r.deopt_at + r.deopt_rerun).collect();
    assert!(deopts.windows(2).all(|w| w[0] >= w[1]), "{deopts:?}");
    assert_eq!(ranked[1].id, 6, "ties break by id: 6, 13, 20, ...");
    assert!(ranked.iter().all(|r| r.deopt_at + r.deopt_rerun > 0));
}
