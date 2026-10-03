//! The end-of-process JIT report (`NEOVM_JIT_COMPILE_STATS=1`).
//!
//! [`FinalReport`] is plain data, collected once when the command loop has
//! returned (see [`super::report_at_exit`]) and rendered by a pure function,
//! so the report format is unit-testable without a `Context`. The periodic
//! lines stay the record for a process that dies by a signal.

use super::epoch::EpochCounters;
use super::{CompileStats, ReportTag, format_phases, format_summary};
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
    /// Whether the leaf was compiled with the entry counter (`entries` is
    /// meaningful; its `clif_insts` include the counter's instructions).
    pub(crate) entry_counted: bool,
    pub(crate) entries: u64,
    pub(crate) deopt_at: u64,
    /// Successful chain readbacks, counted by the physical leaf's mutator.
    pub(crate) chain_deopts: u64,
    /// `(innermost source id, original pc, count)` from the cold readback.
    pub(crate) chain_pcs: Vec<(u64, u32, u64)>,
    pub(crate) deopt_rerun: u64,
    pub(crate) signals: u64,
    /// `(pc, count, op)`, most frequent first; `op` is the bytecode op at
    /// `pc` when the pc indexes the named function's body.
    pub(crate) deopt_pcs: Vec<(u32, u64, Option<String>)>,
    pub(crate) deopt_pc_overflow: u64,
    /// The compile stall that produced the leaf, µs (0 = unknown).
    pub(crate) compile_us: u32,
    /// Why the MIR tier did or did not take the leaf's body (a report token,
    /// see `stats::verdict`); `None` prints `-`.
    pub(crate) mir: Option<Box<str>>,
    /// The tier spine's view of the leaf (`tier2`).
    pub(crate) t2: crate::emacs_core::jit::tier2::T2Snapshot,
    /// Compiler counters only; empty for baseline and passes-disabled opt.
    pub(crate) opt_fold: Option<Box<crate::emacs_core::jit::opt::passes::fold::FoldStats>>,
    pub(crate) opt_bool: Option<Box<crate::emacs_core::jit::opt::passes::bools::BoolStats>>,
    pub(crate) opt_reps: Option<Box<crate::emacs_core::jit::opt::ir::RepsCensus>>,
}

impl LeafReportRow {
    fn deopts(&self) -> u64 {
        self.deopt_at + self.deopt_rerun
    }

    /// The leaf's work: native entries plus back-edge poll ticks (each
    /// 255 taken back edges), both counted only under entry counting.
    pub(crate) fn work(&self) -> u64 {
        self.entries + self.t2.polls
    }

    /// The work an upgraded leaf served (all of it), or the work a leaf
    /// did after its countdown's request fired.
    pub(crate) fn work_reached(&self) -> u64 {
        if self.t2.upgraded_leaf {
            self.work()
        } else if self.t2.requested {
            self.work()
                .saturating_sub(self.t2.entries_at_request + self.t2.polls_at_request)
        } else {
            0
        }
    }

    pub(crate) fn render(&self) -> String {
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
        let entries = if self.entry_counted {
            self.entries.to_string()
        } else {
            "-".to_string()
        };
        format!(
            "id={} name={} tier={} mir={} state={} osr_pc={osr} entries={entries} deopt_at={} \
             deopt_rerun={} signals={} regalloc={} clif={} compile_us={} pcs={pcs}",
            self.id,
            self.name.as_deref().unwrap_or("-"),
            self.tier,
            self.mir.as_deref().unwrap_or("-"),
            self.state,
            self.deopt_at,
            self.deopt_rerun,
            self.signals,
            self.regalloc,
            self.clif_insts,
            self.compile_us,
        )
    }

    /// Separate census rows include every chain-bearing leaf, even when
    /// ordinary leaf ranking omits it. No compiler or Lisp execution runs
    /// while the owning mutator renders its exit snapshot.
    fn render_chain(&self) -> String {
        let mut pcs: Vec<String> = self
            .chain_pcs
            .iter()
            .map(|(source, pc, count)| format!("{source}:{pc}:{count}"))
            .collect();
        let named = self
            .chain_pcs
            .iter()
            .map(|(_, _, count)| count)
            .sum::<u64>();
        if self.chain_deopts > named {
            pcs.push(format!("other:{}", self.chain_deopts - named));
        }
        format!(
            "id={} name={} chain_deopts={} inner_pcs={}",
            self.id,
            self.name.as_deref().unwrap_or("-"),
            self.chain_deopts,
            if pcs.is_empty() {
                "-".to_string()
            } else {
                pcs.join(",")
            },
        )
    }
}

/// The leaves worth a line: the top [`LEAF_ROWS_PER_SECTION`] by deopts
/// (only those that deopted), most first, then the top
/// [`LEAF_ROWS_PER_SECTION`] of the rest by native entries (only those
/// entered), then the top [`LEAF_ROWS_PER_SECTION`] of the rest by compile
/// stall (the compiles that may never pay back). Ties break by id.
pub(crate) fn ranked_leaves(rows: &[LeafReportRow]) -> Vec<&LeafReportRow> {
    let mut by_deopts: Vec<&LeafReportRow> = rows.iter().filter(|r| r.deopts() > 0).collect();
    by_deopts.sort_by(|a, b| b.deopts().cmp(&a.deopts()).then(a.id.cmp(&b.id)));
    by_deopts.truncate(LEAF_ROWS_PER_SECTION);
    let listed =
        |listed: &[&LeafReportRow], r: &LeafReportRow| listed.iter().any(|d| std::ptr::eq(*d, r));
    let mut by_entries: Vec<&LeafReportRow> = rows
        .iter()
        .filter(|r| r.entries > 0 && !listed(&by_deopts, r))
        .collect();
    by_entries.sort_by(|a, b| b.entries.cmp(&a.entries).then(a.id.cmp(&b.id)));
    by_entries.truncate(LEAF_ROWS_PER_SECTION);
    by_deopts.extend(by_entries);
    let mut by_compile: Vec<&LeafReportRow> = rows
        .iter()
        .filter(|r| r.compile_us > 0 && !listed(&by_deopts, r))
        .collect();
    by_compile.sort_by(|a, b| b.compile_us.cmp(&a.compile_us).then(a.id.cmp(&b.id)));
    by_compile.truncate(LEAF_ROWS_PER_SECTION);
    by_deopts.extend(by_compile);
    by_deopts
}

/// Sources the `[neovm-jit-final-t2]` line names, most work first.
pub(crate) const T2_TOP_SOURCES: usize = 8;

/// The tier spine's exit line (`tier2`): the knobs, the request counters
/// and where the work ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct T2Line {
    pub(crate) on: bool,
    pub(crate) window: u32,
    pub(crate) loop_credit: u32,
    pub(crate) stats: crate::emacs_core::jit::tier2::T2Stats,
}

/// One source's work for the tier-spine line: all of it, what upgraded
/// leaves served, what ran after the request (upgraded included).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct T2SourceWork {
    pub(crate) id: u64,
    pub(crate) name: Option<String>,
    pub(crate) work: u64,
    pub(crate) upgraded: u64,
    pub(crate) reached: u64,
}

/// Per-source work over `rows` (every leaf of a source: entry, retired,
/// OSR), most work first, ties by id.
pub(crate) fn t2_source_work(rows: &[LeafReportRow]) -> Vec<T2SourceWork> {
    let mut by_id: std::collections::BTreeMap<u64, T2SourceWork> = Default::default();
    for r in rows {
        let w = by_id.entry(r.id).or_insert_with(|| T2SourceWork {
            id: r.id,
            ..Default::default()
        });
        if w.name.is_none() {
            w.name = r.name.clone();
        }
        w.work += r.work();
        w.reached += r.work_reached();
        if r.t2.upgraded_leaf {
            w.upgraded += r.work();
        }
    }
    let mut v: Vec<T2SourceWork> = by_id.into_values().collect();
    v.sort_by(|a, b| b.work.cmp(&a.work).then(a.id.cmp(&b.id)));
    v
}

/// `part` as a percentage of `whole`, one decimal (`-` for no whole).
fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        "-".to_string()
    } else {
        format!("{:.1}", part as f64 * 100.0 / whole as f64)
    }
}

impl T2Line {
    /// The line's body for `rows`.
    pub(crate) fn render(&self, rows: &[LeafReportRow]) -> String {
        let sources = t2_source_work(rows);
        let work: u64 = sources.iter().map(|s| s.work).sum();
        let upgraded: u64 = sources.iter().map(|s| s.upgraded).sum();
        let reached: u64 = sources.iter().map(|s| s.reached).sum();
        let top: Vec<String> = sources
            .iter()
            .filter(|s| s.work > 0)
            .take(T2_TOP_SOURCES)
            .map(|s| {
                format!(
                    "{}:{}:{}:{}:{}",
                    s.id,
                    csv_field(s.name.as_deref().unwrap_or("-")),
                    s.work,
                    percent(s.upgraded, s.work),
                    percent(s.reached, s.work),
                )
            })
            .collect();
        let st = &self.stats;
        format!(
            "tier2={} window={} loop_credit={} requests={} kept={} stale={} due={} \
             upgraded={} hof_credits={} unstable={} budget_denied={} not_worth={} \
             deferred={} failed={} reverted={} work={work} upgraded_work={upgraded} ({}%) \
             reached_work={reached} ({}%) top={}",
            if self.on { "on" } else { "off" },
            self.window,
            self.loop_credit,
            st.requests,
            st.kept,
            st.stale,
            st.due,
            st.upgraded,
            st.hof_credits,
            st.unstable,
            st.budget_denied,
            st.not_worth,
            st.deferred,
            st.failed,
            st.reverted,
            percent(upgraded, work),
            percent(reached, work),
            if top.is_empty() {
                "-".to_string()
            } else {
                top.join(",")
            },
        )
    }
}

/// Commas would split a CSV column: `name` fields use `;` instead.
fn csv_field(s: &str) -> String {
    s.replace(',', ";")
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
    /// This thread's persistent JIT backend and code arena.
    pub(crate) code_memory: crate::emacs_core::jit::compile::shared::CodeMemoryStats,
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
    /// The leaf builtin census (`leaf_abi::render_leaf_stats`), or empty.
    pub(crate) builtin_leaves: String,
    /// Background compilation (`jit::bg`), unless on the legacy path.
    pub(crate) bg: Option<crate::emacs_core::jit::bg::BgReport>,
    /// The tier spine (`tier2`), when it is on or counted anything.
    pub(crate) t2: Option<T2Line>,
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
                 total_us={} retiers={} mir_taken={} deopts={} reopts={}",
                delta.total_compiles,
                delta.compiled_ok,
                delta.native_entries,
                delta.dispatch_said_compiled,
                delta.dispatch_consulted,
                delta.total_us,
                delta.retiers,
                delta.mir_taken,
                delta.deopts(),
                delta.reopt_levels.iter().sum::<u64>(),
            ));
        }
        lines.push((ReportTag::Final, head));
        lines.push((ReportTag::FinalPhases, format_phases(&self.compile)));
        lines.push((ReportTag::FinalCodeMemory, self.code_memory.render()));
        if let Some(bg) = &self.bg {
            lines.push((ReportTag::FinalBg, bg.render()));
        }
        lines.push((ReportTag::FinalMirBails, or_dash(&self.mir_bails)));
        lines.push((ReportTag::FinalInline, or_dash(&self.inline)));
        for row in self.leaves.iter().filter(|row| row.chain_deopts > 0) {
            lines.push((ReportTag::FinalInlineChain, row.render_chain()));
        }
        let (mut entries_all, mut deopt_at, mut deopt_rerun, mut signals) = (
            self.dropped.entries,
            self.dropped.deopt_at,
            self.dropped.deopt_rerun,
            self.dropped.signals,
        );
        let (mut live, mut retired, mut fallback, mut osr) = (0u64, 0u64, 0u64, 0u64);
        for r in &self.leaves {
            entries_all += r.entries;
            deopt_at += r.deopt_at;
            deopt_rerun += r.deopt_rerun;
            signals += r.signals;
            match r.state {
                "live" => live += 1,
                "retired" => retired += 1,
                "fallback" => fallback += 1,
                _ => osr += 1,
            }
        }
        lines.push((
            ReportTag::FinalRuns,
            format!(
                "entries_all={entries_all} entries_seam={} deopt_at={deopt_at} \
                 deopt_rerun={deopt_rerun} signals={signals} \
                 osr_transfers={} seam_fallbacks={} leaves_live={live} leaves_retired={retired} \
                 leaves_fallback={fallback} leaves_osr={osr} leaves_dropped={}",
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
        if !self.builtin_leaves.is_empty() {
            lines.push((ReportTag::FinalBuiltinLeaves, self.builtin_leaves.clone()));
        }
        if let Some(t2) = &self.t2 {
            lines.push((ReportTag::FinalT2, t2.render(&self.leaves)));
        }
        for row in ranked_leaves(&self.leaves) {
            lines.push((ReportTag::FinalLeaf, row.render()));
        }
        lines
    }
}

impl FinalReport {
    /// The `NEOVM_JIT_PROFILE` rows appended at exit, one per leaf:
    /// `#leaf,compiled_id,name,tier,osr_pc,entries,deopt_at,deopt_rerun,signals,top_deopt_pc`.
    /// Ten columns — fewer than the 13 the census reader requires of a
    /// compile row, so it skips them; they join the compile rows on
    /// `compiled_id`.
    pub(crate) fn profile_leaf_rows(&self) -> Vec<String> {
        let leaf_rows = self.leaves.iter().map(|r| {
            let osr = r
                .osr_pc
                .map_or_else(|| "-".to_string(), |pc| pc.to_string());
            let entries = if r.entry_counted {
                r.entries.to_string()
            } else {
                "-".to_string()
            };
            let top_pc = r
                .deopt_pcs
                .first()
                .map_or_else(|| "-".to_string(), |(pc, n, _)| format!("{pc}:{n}"));
            format!(
                "#leaf,{},{},{},{osr},{entries},{},{},{},{top_pc}\n",
                r.id,
                csv_field(r.name.as_deref().unwrap_or("-")),
                r.tier,
                r.deopt_at,
                r.deopt_rerun,
                r.signals,
            )
        });
        // The tier spine's rows, for the leaves whose work was counted:
        // `#t2,compiled_id,name,osr_pc,origin,state,entries,polls,
        // entries_at_request,polls_at_request` (ten columns, like `#leaf`).
        let t2_rows = self.leaves.iter().filter(|r| r.entry_counted).map(|r| {
            let osr = r
                .osr_pc
                .map_or_else(|| "-".to_string(), |pc| pc.to_string());
            format!(
                "#t2,{},{},{osr},{},{},{},{},{},{}\n",
                r.id,
                csv_field(r.name.as_deref().unwrap_or("-")),
                r.t2.origin,
                r.t2.state,
                r.entries,
                r.t2.polls,
                r.t2.entries_at_request,
                r.t2.polls_at_request,
            )
        });
        // Twelve columns, distinct from compile rows and joined by source id,
        // name and OSR entry. Missing rows mean the pass was not selected.
        let opt_rows = self.leaves.iter().filter_map(|r| {
            let c = r.opt_fold.as_ref()?;
            let osr = r.osr_pc.map_or_else(|| "-".to_owned(), |pc| pc.to_string());
            Some(format!(
                "#opt-fold,{},{},{osr},{},{},{},{},{},{},{},{}\n",
                r.id,
                csv_field(r.name.as_deref().unwrap_or("-")),
                c.guards_folded,
                c.guards_narrowed,
                c.constants_folded,
                c.branches_folded,
                c.threaded_edges,
                c.cons_loads,
                c.deopts,
                usize::from(c.analysis_bailed),
            ))
        });
        // Selected-pass census is immutable compiler metadata; no hot runtime
        // updates or mutator-dependent state are introduced.
        let bool_rows = self.leaves.iter().filter_map(|r| {
            let c = r.opt_bool.as_ref()?;
            let osr = r.osr_pc.map_or_else(|| "-".to_owned(), |pc| pc.to_string());
            Some(format!(
                "#opt-bool,{},{},{osr},{},{},{},{},{},{},{}\n",
                r.id,
                csv_field(r.name.as_deref().unwrap_or("-")),
                c.opaque_producers,
                c.constant_producers,
                c.phi_params,
                c.refinements,
                c.selects,
                c.nil_tests,
                c.tagged_views
            ))
        });
        let reps_rows = self.leaves.iter().filter_map(|r| {
            let c = r.opt_reps.as_ref()?;
            let osr = r.osr_pc.map_or_else(|| "-".to_owned(), |pc| pc.to_string());
            Some(format!(
                "#opt-reps,{},{},{osr},{},{},{},{},{},{},{},{}\n",
                r.id,
                csv_field(r.name.as_deref().unwrap_or("-")),
                c.lift.lifted_arithmetic,
                c.lift.lifted_comparisons,
                c.lift.type_guards,
                c.selection.raw_values,
                c.selection.raw_phis,
                c.selection.tagged_arithmetic,
                c.selection.raw_arithmetic,
                c.selection.tagged_views,
            ))
        });
        leaf_rows
            .chain(t2_rows)
            .chain(opt_rows)
            .chain(bool_rows)
            .chain(reps_rows)
            .collect()
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
