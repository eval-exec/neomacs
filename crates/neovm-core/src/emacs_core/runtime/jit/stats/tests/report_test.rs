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
            LeafReportRow {
                entries: 6_000_000,
                ..leaf_row(37, 5_999_998, 0)
            },
            LeafReportRow {
                entries: 77,
                ..leaf_row(38, 0, 0)
            },
            LeafReportRow {
                state: "retired",
                ..leaf_row(39, 0, 2)
            },
        ],
        dropped: crate::emacs_core::jit::compile::LeafTotals {
            leaves: 1,
            entries: 10,
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
        "entries_all=6000087 entries_seam=4975 deopt_at=6000003 deopt_rerun=2 signals=1 \
         osr_transfers=2 seam_fallbacks=17 leaves_live=2 leaves_retired=1 leaves_osr=0 \
         leaves_dropped=1"
    );
    let leaf_lines: Vec<&str> = lines
        .iter()
        .filter(|(t, _)| *t == ReportTag::FinalLeaf)
        .map(|(_, b)| b.as_str())
        .collect();
    assert_eq!(
        leaf_lines,
        [
            "id=37 name=j4-add tier=mir state=live osr_pc=- entries=6000000 deopt_at=5999998 \
             deopt_rerun=0 signals=0 regalloc=fast clif=58 pcs=12:5999998/Mul,other:3",
            "id=39 name=j4-add tier=mir state=retired osr_pc=- entries=0 deopt_at=0 \
             deopt_rerun=2 signals=0 regalloc=fast clif=58 pcs=-",
            "id=38 name=j4-add tier=mir state=live osr_pc=- entries=77 deopt_at=0 \
             deopt_rerun=0 signals=0 regalloc=fast clif=58 pcs=-",
        ],
        "the leaves that deopted, most first, then the rest by entries"
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
        entry_counted: true,
        entries: 0,
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
    let ranked: Vec<&LeafReportRow> = ranked
        .into_iter()
        .filter(|r| r.deopt_at + r.deopt_rerun > 0)
        .collect();
    assert_eq!(ranked.len(), super::report::LEAF_ROWS_PER_SECTION);
    assert_eq!(ranked[0].id, 100, "a rerun deopt counts too");
    let deopts: Vec<u64> = ranked.iter().map(|r| r.deopt_at + r.deopt_rerun).collect();
    assert!(deopts.windows(2).all(|w| w[0] >= w[1]), "{deopts:?}");
    assert_eq!(ranked[1].id, 6, "ties break by id: 6, 13, 20, ...");
    assert!(ranked.iter().all(|r| r.deopt_at + r.deopt_rerun > 0));
}

/// The entries section never repeats a leaf the deopt section printed, and
/// is capped on its own.
#[test]
fn jit_final_report_leaf_rows_then_by_entries() {
    let mut rows: Vec<LeafReportRow> = (1..=40)
        .map(|id| LeafReportRow {
            entries: id * 10,
            ..leaf_row(id, 0, 0)
        })
        .collect();
    rows[39].deopt_at = 1; // id 40: most entries, but also deopted
    let ranked = ranked_leaves(&rows);
    let ids: Vec<u64> = ranked.iter().map(|r| r.id).collect();
    assert_eq!(ids[0], 40, "the deopt section first");
    assert_eq!(&ids[1..4], &[39, 38, 37], "then by entries, most first");
    assert_eq!(ids.len(), 1 + super::report::LEAF_ROWS_PER_SECTION);
    assert_eq!(ids.iter().filter(|&&id| id == 40).count(), 1, "no repeat");
}

/// The `#leaf` rows appended to `NEOVM_JIT_PROFILE` have fewer than 13
/// columns, so the census reader (which keeps rows with >= 13) skips them,
/// and a comma in a name cannot add a column.
#[test]
fn jit_final_report_profile_leaf_rows_have_fewer_than_13_columns() {
    let report = FinalReport {
        leaves: vec![
            LeafReportRow {
                name: Some("weird,name".to_string()),
                entries: 9,
                osr_pc: Some(7),
                ..leaf_row(5, 3, 1)
            },
            LeafReportRow {
                entry_counted: false,
                name: None,
                ..leaf_row(6, 0, 0)
            },
        ],
        ..FinalReport::default()
    };
    let rows = report.profile_leaf_rows();
    assert_eq!(
        rows,
        [
            "#leaf,5,weird;name,mir,7,9,3,1,0,12:3\n",
            "#leaf,6,-,mir,-,-,0,0,0,-\n",
        ]
    );
    for row in &rows {
        assert_eq!(row.trim_end().split(',').count(), 10, "{row}");
    }
}

/// End to end on a real Context: a leaf bound to a symbol is named through
/// the exit walk, its entries and deopts are collected, and its precise
/// deopt pc is annotated with the bytecode op there.
#[test]
fn jit_final_report_collects_named_leaves_from_a_context() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::SymId;
    use crate::emacs_core::value::{LambdaParams, Value};
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    force_observe_for_test(ObserveOverride {
        stats: true,
        naming: false,
        entry_count: true,
    });
    let mut ev = Context::new();
    // (defun jit-report-add (x) (+ (identity x) 1)): the Add follows a call,
    // so its guard is a precise deopt at pc 4.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0),
        Op::StackRef(1),
        Op::Call(1),
        Op::Constant(1),
        Op::Add,
        Op::Return,
    ];
    f.constants = vec![Value::symbol("identity"), Value::make_int(1)].into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    let sym = Value::symbol("jit-report-add");
    ev.obarray
        .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(f));
    let fval = ev
        .obarray
        .symbol_function_id(sym.as_symbol_id().unwrap())
        .expect("bound");
    let bc = fval.get_bytecode_data().expect("bytecode");
    let ctx = &mut ev as *mut Context;
    let run = |arg: Value| crate::emacs_core::jit::try_run_compiled(ctx, bc, fval, &[arg]);
    assert_eq!(
        run(Value::make_int(41)).expect("no signal"),
        Some(Value::make_int(42).bits())
    );
    // (+ nil 1) signals on the interpreter after the deopt resumes.
    assert!(run(Value::NIL).is_err(), "wrong-type-argument");
    let id = bc.jit_runtime().compiled_id().expect("compiled");

    let report = super::collect_final_report(&ev);
    let row = report
        .leaves
        .iter()
        .find(|r| r.id == id)
        .expect("the leaf is reported");
    assert_eq!(row.name.as_deref(), Some("jit-report-add"));
    assert!(row.entry_counted);
    assert_eq!(row.entries, 2, "{row:?}");
    assert_eq!(row.deopt_at, 1, "a guard after a call is precise: {row:?}");
    assert_eq!(row.deopt_rerun, 0, "{row:?}");
    let (pc, n, op) = &row.deopt_pcs[0];
    assert_eq!((*pc, *n), (4, 1), "{row:?}");
    assert_eq!(op.as_deref(), Some("Add"));
    let lines = report.render();
    assert!(
        lines.iter().any(|(tag, body)| *tag == ReportTag::FinalLeaf
            && body.starts_with(&format!("id={id} name=jit-report-add "))),
        "{lines:?}"
    );
}
