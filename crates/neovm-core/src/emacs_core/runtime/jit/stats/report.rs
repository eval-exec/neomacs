//! The end-of-process JIT report (`NEOVM_JIT_COMPILE_STATS=1`).
//!
//! [`FinalReport`] is plain data, collected once when the command loop has
//! returned (see [`super::report_at_exit`]) and rendered by a pure function,
//! so the report format is unit-testable without a `Context`. The periodic
//! lines stay the record for a process that dies by a signal.

use super::{CompileStats, ReportTag, format_summary};

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
        lines.push((
            ReportTag::FinalRuns,
            format!(
                "entries_seam={} osr_transfers={} seam_fallbacks={}",
                self.compile.native_entries, self.osr_transfers, self.seam_fallbacks,
            ),
        ));
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
