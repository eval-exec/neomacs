use super::report::{FinalReport, LeafReportRow, T2Line, ranked_leaves};
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
        code_memory: Default::default(),
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
        builtin_leaves: String::new(),
        bg: None,
        t2: None,
    };
    let lines = report.render();
    let tags: Vec<&'static str> = lines.iter().map(|(t, _)| (*t).into()).collect();
    assert_eq!(
        tags,
        [
            "neovm-jit-final",
            "neovm-jit-final-phases",
            "neovm-jit-final-code-memory",
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
        body_of(&lines, ReportTag::FinalPhases),
        "origin[-] phase_us[gate=0,mir_build=0,fuse=0,lower=0,setup=0,codegen=0,finalize=0,other=0] \
         phase_total_us=0",
        "no origin rows and no split: every field still prints"
    );
    assert_eq!(
        body_of(&lines, ReportTag::FinalCodeMemory),
        "shared_leaves=0 per_leaf_modules=0 reentrant_fallbacks=0 split_payloads=0 modules_created=0 \
         modules_retired=0 arena_regions=0 arena_page_bytes=0 arena_code_bytes=0 arena_seals=0"
    );
    assert_eq!(
        body_of(&lines, ReportTag::FinalMirBails),
        "gate:generic-call:call=6"
    );
    assert_eq!(body_of(&lines, ReportTag::FinalInline), "-", "empty census");
    assert_eq!(
        body_of(&lines, ReportTag::FinalRuns),
        "entries_all=6000087 entries_seam=4975 deopt_at=6000003 deopt_rerun=2 signals=1 \
         osr_transfers=2 seam_fallbacks=17 leaves_live=2 leaves_retired=1 leaves_fallback=0 leaves_osr=0 \
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
            "id=37 name=j4-add tier=mir mir=taken state=live osr_pc=- entries=6000000 \
             deopt_at=5999998 deopt_rerun=0 signals=0 regalloc=fast clif=58 compile_us=0 \
             pcs=12:5999998/Mul,other:3",
            "id=39 name=j4-add tier=mir mir=taken state=retired osr_pc=- entries=0 deopt_at=0 \
             deopt_rerun=2 signals=0 regalloc=fast clif=58 compile_us=0 pcs=-",
            "id=38 name=j4-add tier=mir mir=taken state=live osr_pc=- entries=77 deopt_at=0 \
             deopt_rerun=0 signals=0 regalloc=fast clif=58 compile_us=0 pcs=-",
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
        chain_deopts: 0,
        chain_pcs: Vec::new(),
        deopt_rerun,
        signals: 0,
        deopt_pcs: if deopt_at > 0 {
            vec![(12, deopt_at, Some("Mul".to_string()))]
        } else {
            Vec::new()
        },
        deopt_pc_overflow: if deopt_at > 0 { 3 } else { 0 },
        compile_us: 0,
        mir: Some("taken".into()),
        t2: Default::default(),
        opt_fold: None,
        opt_bool: None,
        opt_reps: None,
        opt_gvn: None,
    }
}

/// Cold chain rows preserve inner source/pc attribution even for a leaf
/// outside the ranked top-deopt section, and expose the bounded overflow.
#[test]
fn jit_final_report_includes_every_chain_census_with_inner_source_pc() {
    let mut leaves: Vec<_> = (1..=20).map(|id| leaf_row(id, 100, 0)).collect();
    leaves.push(LeafReportRow {
        chain_deopts: 4,
        chain_pcs: vec![(91, 3, 2), (92, 7, 1)],
        ..leaf_row(99, 4, 0)
    });
    let report = FinalReport {
        leaves,
        ..Default::default()
    };
    let lines = report.render();
    assert_eq!(
        body_of(&lines, ReportTag::FinalInlineChain),
        "id=99 name=j4-add chain_deopts=4 inner_pcs=91:3:2,92:7:1,other:1"
    );
    assert_eq!(
        <&'static str>::from(ReportTag::FinalInlineChain),
        "neovm-jit-final-inline-chain"
    );
    assert!(
        !lines
            .iter()
            .any(|(tag, body)| *tag == ReportTag::FinalLeaf && body.starts_with("id=99 "))
    );
}

/// Each leaf row names the MIR verdict of the compile that produced it, as
/// one token right after the tier; a leaf without one (no report knob at its
/// compile, an OSR or AOT leaf) prints `-`.
#[test]
fn jit_final_report_leaf_row_renders_the_mir_verdict() {
    let bailed = LeafReportRow {
        tier: "baseline",
        mir: Some("build:UnsupportedOp(\"mir-unmodelled-control:Switch\")".into()),
        ..leaf_row(7, 0, 0)
    };
    assert_eq!(
        bailed.render(),
        "id=7 name=j4-add tier=baseline mir=build:UnsupportedOp(\"mir-unmodelled-control:Switch\") \
         state=live osr_pc=- entries=0 deopt_at=0 deopt_rerun=0 signals=0 regalloc=fast clif=58 \
         compile_us=0 pcs=-"
    );
    let osr = LeafReportRow {
        tier: "osr",
        mir: None,
        ..leaf_row(8, 0, 0)
    };
    assert!(
        osr.render().contains(" tier=osr mir=- state="),
        "{}",
        osr.render()
    );
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
            "#t2,5,weird;name,7,,,9,0,0,0\n",
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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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
    assert!(row.mir.is_some(), "the compile's MIR verdict: {row:?}");
    let lines = report.render();
    assert!(
        lines.iter().any(|(tag, body)| *tag == ReportTag::FinalLeaf
            && body.starts_with(&format!("id={id} name=jit-report-add "))),
        "{lines:?}"
    );
}

/// The leaf builtin census prints after the epoch section, only when a leaf
/// site was compiled.
#[test]
fn jit_final_report_prints_the_builtin_leaf_census_when_present() {
    let with = FinalReport {
        builtin_leaves: "nth:opcode_sites=3,bcall_sites=0,generic=0,signal=1".to_string(),
        ..FinalReport::default()
    };
    let lines = with.render();
    assert_eq!(
        body_of(&lines, ReportTag::FinalBuiltinLeaves),
        "nth:opcode_sites=3,bcall_sites=0,generic=0,signal=1"
    );
    let tag: &'static str = ReportTag::FinalBuiltinLeaves.into();
    assert_eq!(tag, "neovm-jit-final-builtin-leaves");
    let without = FinalReport::default().render();
    assert!(
        without
            .iter()
            .all(|(t, _)| *t != ReportTag::FinalBuiltinLeaves)
    );
}

/// After the deopt and entry sections, the costliest remaining compiles
/// print (a leaf that cost much and ran little is a compile that never paid
/// back), never repeating a listed leaf and skipping unknown stalls.
#[test]
fn jit_final_report_leaf_rows_then_by_compile_stall() {
    let mut rows: Vec<LeafReportRow> = (1..=40)
        .map(|id| LeafReportRow {
            compile_us: id as u32 * 100,
            ..leaf_row(id, 0, 0)
        })
        .collect();
    rows[39].entries = 5; // id 40: entered, so listed by entries first
    rows.push(leaf_row(41, 0, 0)); // compile_us 0: unknown, never listed
    let ranked = ranked_leaves(&rows);
    let ids: Vec<u64> = ranked.iter().map(|r| r.id).collect();
    assert_eq!(ids[0], 40, "the entries section first");
    assert_eq!(
        &ids[1..4],
        &[39, 38, 37],
        "then by compile stall, costliest first"
    );
    assert_eq!(ids.len(), 1 + super::report::LEAF_ROWS_PER_SECTION);
    assert!(!ids.contains(&41));
    assert!(
        ranked[1]
            .render()
            .contains(" clif=58 compile_us=3900 pcs=-"),
        "{}",
        ranked[1].render()
    );
}

/// The background-compile line prints only off the legacy path, with the
/// keys tooling greps for.
#[test]
fn jit_final_report_prints_the_bg_line_off_the_legacy_path() {
    let with = FinalReport {
        bg: Some(crate::emacs_core::jit::bg::BgReport {
            mode: "on",
            workers: 1,
            in_flight_at_exit: 2,
            ..Default::default()
        }),
        ..FinalReport::default()
    };
    let lines = with.render();
    let body = body_of(&lines, ReportTag::FinalBg);
    for key in [
        "mode=on",
        "workers=1",
        "enqueued=osr:0,first_sight:0,entry:0,upgrade:0",
        "discarded=superseded:0,heap_changed:0,epoch_moved:0,failed:0,dropped:0",
        "backend_us=0",
        "pending_probes=0",
        "osr_waits=0",
        "refused=0",
        "native_calls_while_pending=0",
        "in_flight_at_exit=2",
        "worker_panics=0",
    ] {
        assert!(body.contains(key), "{key} in {body}");
    }
    assert!(
        FinalReport::default()
            .render()
            .iter()
            .all(|(tag, _)| *tag != ReportTag::FinalBg),
        "no line on the legacy path"
    );
}

/// The exit line: knobs, counters, and the per-source work split.
#[test]
fn jit_final_report_t2_line_renders_the_work_split() {
    use crate::emacs_core::jit::tier2::{T2Snapshot, T2Stats};
    let row = |id, entries, polls, origin: &'static str, state: &'static str, at: (u64, u64)| {
        LeafReportRow {
            id,
            name: Some("f".to_string()),
            entry_counted: true,
            entries,
            t2: T2Snapshot {
                origin,
                state,
                polls,
                requested: state != "idle",
                entries_at_request: at.0,
                polls_at_request: at.1,
                upgraded_leaf: origin == "upgrade",
            },
            ..Default::default()
        }
    };
    let rows = vec![
        // id 1: T1 requested at 10 entries, then an upgrade served 90.
        row(1, 10, 0, "profiling", "upgraded", (10, 0)),
        row(1, 90, 0, "upgrade", "idle", (0, 0)),
        // id 2: a kept loop leaf, requested at tick 5 of 105.
        row(2, 1, 105, "profiling", "kept", (1, 5)),
    ];
    let line = T2Line {
        on: true,
        window: 10,
        loop_credit: 64,
        stats: T2Stats {
            requests: 2,
            kept: 1,
            due: 1,
            upgraded: 1,
            ..Default::default()
        },
    }
    .render(&rows);
    assert!(
        line.starts_with(
            "tier2=on window=10 loop_credit=64 requests=2 kept=1 stale=0 due=1 upgraded=1 \
             hof_credits=0 unstable=0 budget_denied=0 not_worth=0 \
             deferred=0 failed=0 reverted=0 work=206 upgraded_work=90 (43.7%) reached_work=190 (92.2%) top="
        ),
        "{line}"
    );
    assert!(
        line.contains("top=2:f:106:0.0:94.3,1:f:100:90.0:90.0"),
        "{line}"
    );
}

/// Pass metadata must stay absent by default and remain a distinct short CSV
/// record when selected, even for an OSR leaf or a comma-bearing source name.
#[test]
fn jit_final_report_boolean_census_is_optional_and_eleven_columns() {
    use crate::emacs_core::jit::opt::passes::bools::BoolStats;
    let mut leaf = leaf_row(13, 0, 0);
    let absent = FinalReport {
        leaves: vec![leaf.clone()],
        ..Default::default()
    };
    assert!(
        absent
            .profile_leaf_rows()
            .iter()
            .all(|r| !r.starts_with("#opt-bool,"))
    );
    leaf.name = Some("bool,name".into());
    leaf.osr_pc = Some(7);
    leaf.opt_bool = Some(Box::new(BoolStats {
        opaque_producers: 1,
        constant_producers: 2,
        phi_params: 3,
        refinements: 4,
        selects: 5,
        nil_tests: 6,
        tagged_views: 7,
    }));
    let present = FinalReport {
        leaves: vec![leaf],
        ..Default::default()
    };
    let rows = present.profile_leaf_rows();
    let bool_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.starts_with("#opt-bool,"))
        .collect();
    assert_eq!(bool_rows.len(), 1);
    assert_eq!(
        bool_rows[0].as_str(),
        "#opt-bool,13,bool;name,7,1,2,3,4,5,6,7\n"
    );
    assert_eq!(bool_rows[0].trim_end().split(',').count(), 11);
}

#[test]
fn jit_final_report_integer_census_is_optional_and_twelve_columns() {
    use crate::emacs_core::jit::opt::{
        ir::RepsCensus,
        passes::{reps::RepsStats, reps_lift::LiftStats},
    };
    let mut leaf = leaf_row(17, 0, 0);
    let absent = FinalReport {
        leaves: vec![leaf.clone()],
        ..Default::default()
    };
    assert!(
        absent
            .profile_leaf_rows()
            .iter()
            .all(|r| !r.starts_with("#opt-reps,"))
    );
    leaf.name = Some("integer,name".into());
    leaf.osr_pc = Some(9);
    leaf.opt_reps = Some(Box::new(RepsCensus {
        lift: LiftStats {
            lifted_arithmetic: 1,
            lifted_comparisons: 2,
            type_guards: 3,
        },
        selection: RepsStats {
            raw_values: 4,
            raw_phis: 5,
            tagged_arithmetic: 6,
            raw_arithmetic: 7,
            tagged_views: 8,
        },
    }));
    let present = FinalReport {
        leaves: vec![leaf],
        ..Default::default()
    };
    let rows = present.profile_leaf_rows();
    let reps_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.starts_with("#opt-reps,"))
        .collect();
    assert_eq!(reps_rows.len(), 1);
    assert_eq!(
        reps_rows[0].as_str(),
        "#opt-reps,17,integer;name,9,1,2,3,4,5,6,7,8\n"
    );
    assert_eq!(reps_rows[0].trim_end().split(',').count(), 12);
}

#[test]
fn jit_final_report_gvn_census_is_optional_and_seven_columns() {
    use crate::emacs_core::jit::opt::passes::gvn::GvnStats;
    let mut leaf = leaf_row(19, 0, 0);
    let absent = FinalReport {
        leaves: vec![leaf.clone()],
        ..Default::default()
    };
    assert!(
        absent
            .profile_leaf_rows()
            .iter()
            .all(|r| !r.starts_with("#opt-gvn,"))
    );
    leaf.name = Some("gvn,name".into());
    leaf.osr_pc = Some(11);
    leaf.opt_gvn = Some(Box::new(GvnStats {
        pure_reuses: 1,
        load_reuses: 2,
        store_forwards: 3,
    }));
    let present = FinalReport {
        leaves: vec![leaf],
        ..Default::default()
    };
    let rows = present.profile_leaf_rows();
    let gvn_rows: Vec<_> = rows.iter().filter(|r| r.starts_with("#opt-gvn,")).collect();
    assert_eq!(gvn_rows.len(), 1);
    assert_eq!(gvn_rows[0].as_str(), "#opt-gvn,19,gvn;name,11,1,2,3\n");
    assert_eq!(gvn_rows[0].trim_end().split(',').count(), 7);
}
