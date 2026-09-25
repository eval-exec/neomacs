use super::report::FinalReport;
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
        "entries_seam=4975 osr_transfers=2 seam_fallbacks=17"
    );

    // Without a command-loop mark there is no delta section.
    let unmarked = FinalReport {
        since_command_loop_ms: None,
        compile_since_loop: None,
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
