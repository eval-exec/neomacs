//! The end-of-process JIT report (`NEOVM_JIT_COMPILE_STATS=1`).
//!
//! [`FinalReport`] is plain data, collected once when the command loop has
//! returned (see [`super::report_at_exit`]) and rendered by a pure function,
//! so the report format is unit-testable without a `Context`. The periodic
//! lines stay the record for a process that dies by a signal.

use super::epoch::EpochCounters;
use super::{CompileStats, ReportTag, format_summary};
use crate::emacs_core::jit::compile::LeafTotals;

/// How many leaves each ranked leaf section prints.
pub(crate) const LEAF_ROWS_PER_SECTION: usize = 16;

/// One compiled leaf in the exit report (plain data; see
/// `cache::leaf_report_rows` for where the counters come from).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LeafReportRow {
    pub(crate) id: u64,
    /// The Lisp function currently bound to this leaf's source, if any.
    pub(crate) name: Option<String>,
    pub(crate) tier: &'static str,
    pub(crate) state: &'static str,
    pub(crate) osr_pc: Option<u32>,
    pub(crate) regalloc: &'static str,
    pub(crate) clif_insts: u32,
    pub(crate) deopt_at: u64,
    pub(crate) deopt_rerun: u64,
    pub(crate) signals: u64,
    /// `(pc, count, op)`, most frequent first; `op` is the bytecode op at
    /// `pc` when the pc indexes the named function's body.
    pub(crate) deopt_pcs: Vec<(u32, u64, Option<String>)>,
    pub(crate) deopt_pc_overflow: u64,
}

impl LeafReportRow {
    fn deopts(&self) -> u64 {
        self.deopt_at + self.deopt_rerun
    }

    fn render(&self) -> String {
        let osr = self
            .osr_pc
            .map_or_else(|| "-".to_string(), |pc| pc.to_string());
        let mut pcs: Vec<String> = self
            .deopt_pcs
            .iter()
            .map(|(pc, n, op)| match op {
                Some(op) => format!("{pc}:{n}/{op}"),
                None => format!("{pc}:{n}"),
            })
            .collect();
        if self.deopt_pc_overflow > 0 {
            pcs.push(format!("other:{}", self.deopt_pc_overflow));
        }
        let pcs = if pcs.is_empty() {
            "-".to_string()
        } else {
            pcs.join(",")
        };
        format!(
            "id={} name={} tier={} state={} osr_pc={osr} deopt_at={} deopt_rerun={} \
             signals={} regalloc={} clif={} pcs={pcs}",
            self.id,
            self.name.as_deref().unwrap_or("-"),
            self.tier,
            self.state,
            self.deopt_at,
            self.deopt_rerun,
            self.signals,
            self.regalloc,
            self.clif_insts,
        )
    }
}

/// The leaves worth a line: the top [`LEAF_ROWS_PER_SECTION`] by deopts
/// (only those that deopted), most first. Ties break by id.
pub(crate) fn ranked_leaves(rows: &[LeafReportRow]) -> Vec<&LeafReportRow> {
    let mut by_deopts: Vec<&LeafReportRow> = rows.iter().filter(|r| r.deopts() > 0).collect();
    by_deopts.sort_by(|a, b| b.deopts().cmp(&a.deopts()).then(a.id.cmp(&b.id)));
    by_deopts.truncate(LEAF_ROWS_PER_SECTION);
    by_deopts
}

/// Everything the exit report prints. Filled by [`super::report_at_exit`];
/// tests build one by hand.
#[derive(Clone, Debug, Default)]
pub(crate) struct FinalReport {
    pub(crate) pid: u32,
    /// Milliseconds from the outer command loop's entry to the report, or
    /// `None` when [`super::mark_command_loop_entry`] never ran.
    pub(crate) since_command_loop_ms: Option<u64>,
    /// This thread's compile aggregates for the whole process.
    pub(crate) compile: CompileStats,
    /// The same aggregates counted from the command-loop mark on (startup and
    /// loadup excluded), when the mark ran.
    pub(crate) compile_since_loop: Option<CompileStats>,
    /// The MIR-bail census, rendered (`reason=count,...`).
    pub(crate) mir_bails: String,
    /// The fuser census, rendered (`verdict=count,...`).
    pub(crate) inline: String,
    /// Process-wide OSR transfers actually taken (`cache::OSR_TRANSFER_COUNT`).
    pub(crate) osr_transfers: u64,
    /// Process-wide dispatch-seam interpreter fallbacks
    /// (`cache::SEAM_INTERP_FALLBACK_COUNT`).
    pub(crate) seam_fallbacks: u64,
    /// The obarray's `function_epoch` at exit (a cross-check on the bump
    /// total: they differ only by bumps on other threads or obarrays).
    pub(crate) function_epoch: u64,
    /// This thread's `function_epoch` bumps by reason.
    pub(crate) epoch: EpochCounters,
    /// The same, counted from the command-loop mark on.
    pub(crate) epoch_since_loop: Option<EpochCounters>,
    /// The most-redefined symbols, rendered (`name=count,...`).
    pub(crate) redefined_top: String,
    /// Every leaf this thread still holds.
    pub(crate) leaves: Vec<LeafReportRow>,
    /// Summed counters of the leaves the caches dropped.
    pub(crate) dropped: LeafTotals,
}

impl FinalReport {
    /// The report as `(tag, body)` lines, in print order. Pure.
    pub(crate) fn render(&self) -> Vec<(ReportTag, String)> {
        let mut lines = Vec::new();
        let since = self
            .since_command_loop_ms
            .map_or_else(|| "-".to_string(), |ms| ms.to_string());
        let mut head = format!(
            "pid={} since_command_loop_ms={since} {}",
            self.pid,
            format_summary(&self.compile)
        );
        if let Some(delta) = &self.compile_since_loop {
            head.push_str(&format!(
                " | since_command_loop: compiles={} ok={} native_entries={} dispatch={}/{} \
                 total_us={} retiers={} mir_taken={}",
                delta.total_compiles,
                delta.compiled_ok,
                delta.native_entries,
                delta.dispatch_said_compiled,
                delta.dispatch_consulted,
                delta.total_us,
                delta.retiers,
                delta.mir_taken,
            ));
        }
        lines.push((ReportTag::Final, head));
        lines.push((ReportTag::FinalMirBails, or_dash(&self.mir_bails)));
        lines.push((ReportTag::FinalInline, or_dash(&self.inline)));
        let (mut deopt_at, mut deopt_rerun, mut signals) = (
            self.dropped.deopt_at,
            self.dropped.deopt_rerun,
            self.dropped.signals,
        );
        let (mut live, mut retired, mut osr) = (0u64, 0u64, 0u64);
        for r in &self.leaves {
            deopt_at += r.deopt_at;
            deopt_rerun += r.deopt_rerun;
            signals += r.signals;
            match r.state {
                "live" => live += 1,
                "retired" => retired += 1,
                _ => osr += 1,
            }
        }
        lines.push((
            ReportTag::FinalRuns,
            format!(
                "entries_seam={} deopt_at={deopt_at} deopt_rerun={deopt_rerun} signals={signals} \
                 osr_transfers={} seam_fallbacks={} leaves_live={live} leaves_retired={retired} \
                 leaves_osr={osr} leaves_dropped={}",
                self.compile.native_entries,
                self.osr_transfers,
                self.seam_fallbacks,
                self.dropped.leaves,
            ),
        ));
        let mut fn_epoch = format!("epoch={} {}", self.function_epoch, self.epoch.render());
        if let Some(delta) = &self.epoch_since_loop {
            fn_epoch.push_str(&format!(
                " | since_command_loop: {}",
                delta.render_nonzero()
            ));
        }
        lines.push((ReportTag::FinalFnEpoch, fn_epoch));
        lines.push((ReportTag::FinalFnEpochTop, or_dash(&self.redefined_top)));
        for row in ranked_leaves(&self.leaves) {
            lines.push((ReportTag::FinalLeaf, row.render()));
        }
        lines
    }
}

/// An empty census renders as `-`, so every section always prints a line.
fn or_dash(s: &str) -> String {
    if s.is_empty() {
        "-".to_string()
    } else {
        s.to_string()
    }
}
