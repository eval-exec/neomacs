//! Evaluated `(when FORM . SPEC)` display-spec conditions for one window walk.
//!
//! GNU evaluates FORM inline while it walks the text
//! (`handle_single_display_spec`, src/xdisp.c:6130-6160, emacs-31.0.90),
//! binding `object`, `position` and `buffer-position` to the site it is at
//! (src/xdisp.c:6152-6154).  This engine cannot run Lisp while it holds the
//! buffer for a walk, so the forms at every site of the window's span are
//! evaluated once, before the walk, through
//! `Context::display_when_form_holds`, and the answers ride along in the
//! `LayoutBufferSnapshot` the walk reads from, keyed by site.  The classifier
//! then asks [`DisplayWhenConditions::holds`] for the site it is classifying
//! instead of assuming every non-nil FORM is true, which is what made
//! dashboard's `(when (not (display-graphic-p)) …)` clause win on a graphic
//! frame; and one FORM on two spans gets each span's own answer, which is
//! what made `(when (> buffer-position 30) …)` answer for both spans what it
//! answered for the first.
//!
//! The scan cannot reach every site the walk meets (a string reached through
//! a replacing `display` property, a site past the scan's span).  The walk
//! records the clauses it had to take by the structural rule
//! ([`DisplayWhenVisitLog`]); after the walk they are evaluated with GNU's
//! bindings in visit order ([`settle_display_when_visits`]), and an answer
//! that disagrees with the rule replays the frame once with the evaluated
//! answers carried in [`DisplayWhenReplayLedger`].  A second disagreement in
//! the same redisplay keeps the replay's rows and marks them
//! [`DisplayWhenRecord::Unknown`]: nothing produced from them is reused.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use neovm_core::buffer::text_props::PropertyNamePresence;
use neovm_core::buffer::{Buffer, BufferId, CharPos0, EmacsByteRange};
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::display_spec::{DisplayPropertySpecs, display_spec_when_parts};
use neovm_core::emacs_core::display_when::{DisplayWhenSite, DisplayWhenSiteKey};
use neovm_core::emacs_core::intern::WellKnown;
use neovm_core::emacs_core::value::{Value, get_string_text_properties_table_for_value};
use rustc_hash::FxHashMap;

use crate::neovm_bridge::{LayoutBufferView, RustTextPropAccess};

/// The evaluated conditions a walk consults.
///
/// `Structural` is the state of a reader with no evaluation behind it (a
/// synchronous query, a test, a frame-local chrome string): every non-nil
/// FORM holds -- the pre-evaluation behaviour, declared, not GNU.  Once
/// `Evaluated`, a FORM at a site the scan did not see (an object the scan
/// does not cover, a position past the span) falls back to the same rule.
#[derive(Clone, Debug, Default)]
pub enum DisplayWhenConditions {
    #[default]
    Structural,
    Evaluated(Rc<DisplayWhenAnswers>),
}

/// The answers of one window walk's evaluation, by site.
#[derive(Debug)]
pub struct DisplayWhenAnswers {
    /// GNU `object` of every buffer site: the walked buffer.
    buffer: Value,
    /// One answer per evaluated (FORM, object, position, buffer-position).
    exact: FxHashMap<DisplayWhenSiteKey, bool>,
    /// The same answers with the extent of the property run each site
    /// starts, for the walk's reads inside a run (a face or overlay boundary
    /// splits a run into segments; GNU re-evaluates at such a stop with that
    /// stop's `position`, which gets the run's answer here -- declared) and
    /// for string sites, whose frames do not know their anchor.
    runs: Vec<DisplayWhenRun>,
    /// The clauses the walk took by the structural rule.
    visits: DisplayWhenVisitLog,
}

/// The walk's record of the `when` clauses it took by the structural rule:
/// sites the scan did not reach, which the walk met anyway.
///
/// The walk reads the conditions through a shared snapshot, hence the
/// interior mutability; the log is touched only on that structural path, so
/// a walk that meets no such clause pays nothing and allocates nothing.
/// Recording is enabled only for the conditions of a redisplay leaf attempt
/// ([`DisplayWhenConditions::with_visit_recording`]), whose frame function
/// owns a scratch GC root scope: each recorded site's Lisp objects are pushed
/// onto the scratch root stack as they are recorded, so window chrome Lisp
/// that runs after the body walk (and may collect) cannot free a site
/// before [`settle_display_when_visits`] evaluates it.
#[derive(Debug, Default)]
struct DisplayWhenVisitLog {
    recording: bool,
    /// Evaluable sites in visit order, with GNU's bindings.
    sites: RefCell<Vec<DisplayWhenSite>>,
    /// A clause was taken by the structural rule at a site whose bindings
    /// the walk does not know (a string whose buffer anchor is unknown):
    /// it cannot be evaluated after the walk, so its rows stay unverified.
    unevaluable: Cell<bool>,
}

impl DisplayWhenVisitLog {
    fn note(&self, buffer: Value, at: DisplayWhenLookup, property: Value) {
        if !self.recording {
            return;
        }
        let site = match at {
            DisplayWhenLookup::Buffer { position } => DisplayWhenSite {
                property,
                object: buffer,
                position,
                buffer_position: position,
            },
            DisplayWhenLookup::String {
                string,
                index,
                anchor: Some(anchor),
            } => DisplayWhenSite {
                property,
                object: string,
                position: index,
                buffer_position: anchor,
            },
            DisplayWhenLookup::String { anchor: None, .. } => {
                self.unevaluable.set(true);
                return;
            }
            #[cfg(test)]
            DisplayWhenLookup::AnyOccurrence => return,
        };
        let mut sites = self.sites.borrow_mut();
        // One property is asked once per `when' clause, and a row can ask
        // again at the same site; evaluate each site once.
        if sites.iter().any(|seen| {
            seen.property == site.property
                && seen.object == site.object
                && seen.position == site.position
                && seen.buffer_position == site.buffer_position
        }) {
            return;
        }
        neovm_core::emacs_core::eval::push_scratch_gc_roots(&[site.property, site.object]);
        sites.push(site);
    }
}

#[derive(Clone, Copy, Debug)]
struct DisplayWhenRun {
    key: DisplayWhenSiteKey,
    /// Exclusive GNU position in `key.object` where the property run ends.
    end: i64,
    holds: bool,
}

/// A site of the window span to evaluate: GNU's bindings plus the extent of
/// the property run that starts there.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DisplayWhenRunSite {
    pub(crate) site: DisplayWhenSite,
    /// Exclusive GNU position in `site.object` where the run ends.
    pub(crate) end: i64,
}

/// What the walk is classifying when it asks for a FORM's answer: the GNU
/// bindings of the site, as far as the reader knows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayWhenLookup {
    /// A `display` property of the walked buffer at GNU position `position`
    /// (`buffer-position` is the same position there).
    Buffer { position: i64 },
    /// A `display` property inside `string` at character `index`.  `anchor`
    /// is GNU's `buffer-position` when the frame knows it (a string reached
    /// through a buffer `display` property); otherwise the first evaluated
    /// anchor of that site answers (declared: GNU evaluates one string shown
    /// at two anchors twice).
    String {
        string: Value,
        index: i64,
        anchor: Option<i64>,
    },
    /// Any evaluated occurrence of the FORM: the pre-keying behaviour, for
    /// tests that classify a spec shape with no walk behind it.
    #[cfg(test)]
    AnyOccurrence,
}

impl DisplayWhenLookup {
    /// A buffer site from the walk's 0-based character position.
    pub(crate) fn buffer_at_char_pos(char_pos: i64) -> Self {
        Self::Buffer {
            position: char_pos + 1,
        }
    }
}

/// What the conditions know about one FORM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayWhenVerdict {
    Holds,
    Fails,
    /// Not evaluated: a structural reader, or a FORM outside the scan.
    Unseen,
}

impl DisplayWhenConditions {
    pub fn structural() -> Self {
        Self::Structural
    }

    /// The answers for the sites of a walk of `buffer`: `answers` holds one
    /// per evaluated (FORM, site), `sites` the run each site starts.
    pub(crate) fn evaluated(
        buffer: Value,
        sites: &[DisplayWhenRunSite],
        answers: FxHashMap<DisplayWhenSiteKey, bool>,
    ) -> Self {
        let mut runs = Vec::new();
        for run_site in sites {
            DisplayPropertySpecs::of(run_site.site.property).for_each(|spec| {
                if let Some((form, _)) = display_spec_when_parts(spec) {
                    let key = DisplayWhenSiteKey::at(form, &run_site.site);
                    if let Some(&holds) = answers.get(&key) {
                        runs.push(DisplayWhenRun {
                            key,
                            end: run_site.end,
                            holds,
                        });
                    }
                }
                std::ops::ControlFlow::Continue(())
            });
        }
        Self::Evaluated(Rc::new(DisplayWhenAnswers {
            buffer,
            exact: answers,
            runs,
            visits: DisplayWhenVisitLog::default(),
        }))
    }

    /// Record the clauses the walk takes by the structural rule (see
    /// [`DisplayWhenVisitLog`]).  Only a redisplay leaf attempt enables this,
    /// under its frame function's scratch GC root scope.
    pub(crate) fn with_visit_recording(mut self) -> Self {
        if let Self::Evaluated(answers) = &mut self
            && let Some(answers) = Rc::get_mut(answers)
        {
            answers.visits.recording = true;
        }
        self
    }

    /// Add the answers evaluated after an earlier walk of this window in the
    /// same redisplay (the replay's evidence; see
    /// [`DisplayWhenReplayLedger`]).  Their keys are exact sites, so the
    /// replay finds them where the first walk asked.
    pub(crate) fn with_carried_answers(
        mut self,
        carried: Option<&FxHashMap<DisplayWhenSiteKey, bool>>,
    ) -> Self {
        if let Some(carried) = carried
            && let Self::Evaluated(answers) = &mut self
            && let Some(answers) = Rc::get_mut(answers)
        {
            for (key, holds) in carried {
                answers.exact.entry(*key).or_insert(*holds);
            }
        }
        self
    }

    /// The conditions of a walk that reuses retained rows without evaluating
    /// anything (GNU `try_cursor_movement`): no answers.  A clause asked
    /// anyway is recorded and evaluated after the walk like any other.
    pub(crate) fn reused_without_evaluation(buffer: Value) -> Self {
        Self::evaluated(buffer, &[], FxHashMap::default())
    }

    /// Answers by FORM alone, at no site: every lookup finds them as "any
    /// occurrence".
    #[cfg(test)]
    pub(crate) fn evaluated_forms(results: FxHashMap<Value, bool>) -> Self {
        let runs = results
            .iter()
            .map(|(&form, &holds)| DisplayWhenRun {
                key: DisplayWhenSiteKey {
                    form,
                    object: Value::NIL,
                    position: 0,
                    buffer_position: 0,
                },
                end: i64::MAX,
                holds,
            })
            .collect();
        let exact = results
            .into_iter()
            .map(|(form, holds)| {
                (
                    DisplayWhenSiteKey {
                        form,
                        object: Value::NIL,
                        position: 0,
                        buffer_position: 0,
                    },
                    holds,
                )
            })
            .collect();
        Self::Evaluated(Rc::new(DisplayWhenAnswers {
            buffer: Value::NIL,
            exact,
            runs,
            visits: DisplayWhenVisitLog::default(),
        }))
    }

    /// Whether the walk recorded clauses to evaluate after it (Lisp will run).
    #[inline]
    pub(crate) fn has_recorded_visits(&self) -> bool {
        matches!(self, Self::Evaluated(answers) if !answers.visits.sites.borrow().is_empty())
    }

    /// Retained rows have no dependency record for arbitrary Lisp FORM
    /// results. Rewalk a window with evaluated forms (or a failed evaluation)
    /// instead of replaying glyphs produced by a previous answer.
    pub(crate) fn allows_body_reuse(&self) -> bool {
        matches!(self, Self::Evaluated(answers) if answers.exact.is_empty())
    }

    /// The evaluated result for FORM at the site the walk is classifying:
    /// literal `nil`/`t` first (GNU takes both as they are, src/xdisp.c:6141),
    /// then the site's own answer, then the answer of the run the position
    /// lies in.
    pub(crate) fn verdict(&self, form: Value, at: DisplayWhenLookup) -> DisplayWhenVerdict {
        if form.is_nil() {
            return DisplayWhenVerdict::Fails;
        }
        if form.is_bare_symbol_named("t") {
            return DisplayWhenVerdict::Holds;
        }
        let Self::Evaluated(answers) = self else {
            return DisplayWhenVerdict::Unseen;
        };
        let found = match at {
            DisplayWhenLookup::Buffer { position } => {
                let key = DisplayWhenSiteKey {
                    form,
                    object: answers.buffer,
                    position,
                    buffer_position: position,
                };
                answers.exact.get(&key).copied().or_else(|| {
                    answers
                        .runs
                        .iter()
                        .find(|run| {
                            run.key.form == form
                                && run.key.object == answers.buffer
                                && run.key.position <= position
                                && position < run.end
                        })
                        .map(|run| run.holds)
                })
            }
            DisplayWhenLookup::String {
                string,
                index,
                anchor,
            } => anchor
                .and_then(|anchor| {
                    answers
                        .exact
                        .get(&DisplayWhenSiteKey {
                            form,
                            object: string,
                            position: index,
                            buffer_position: anchor,
                        })
                        .copied()
                })
                .or_else(|| {
                    answers
                        .runs
                        .iter()
                        .find(|run| {
                            run.key.form == form
                                && run.key.object == string
                                && run.key.position <= index
                                && index < run.end
                        })
                        .map(|run| run.holds)
                }),
            #[cfg(test)]
            DisplayWhenLookup::AnyOccurrence => answers
                .runs
                .iter()
                .find(|run| run.key.form == form)
                .map(|run| run.holds),
        };
        match found {
            Some(true) => DisplayWhenVerdict::Holds,
            Some(false) => DisplayWhenVerdict::Fails,
            None => DisplayWhenVerdict::Unseen,
        }
    }

    /// The verdict of any evaluated occurrence of FORM.
    #[cfg(test)]
    pub(crate) fn verdict_any(&self, form: Value) -> DisplayWhenVerdict {
        self.verdict(form, DisplayWhenLookup::AnyOccurrence)
    }

    /// Whether a `(when FORM . SPEC)` spec of `property` applies at `at`; an
    /// unseen FORM holds (the structural rule), and the walk records the site
    /// so the clause is evaluated after the walk.
    pub(crate) fn holds(&self, form: Value, at: DisplayWhenLookup, property: Value) -> bool {
        match self.verdict(form, at) {
            DisplayWhenVerdict::Holds => true,
            DisplayWhenVerdict::Fails => false,
            DisplayWhenVerdict::Unseen => {
                if let Self::Evaluated(answers) = self {
                    answers.visits.note(answers.buffer, at, property);
                }
                true
            }
        }
    }
}

/// What a window walk's `when` lookups leave behind for its retained rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DisplayWhenRecord {
    /// No `when` FORM was evaluated for the window's span and the walk took
    /// no clause by the structural rule.
    #[default]
    NoSites,
    /// Every clause the walk took came from an evaluated answer, or from the
    /// structural rule where the evaluation after the walk agreed with it.
    Known,
    /// A clause the walk took stays unverified: the evaluation failed, the
    /// site could not be evaluated, or it disagreed again after the one
    /// replay.  Rows produced from it must not be reused.
    Unknown,
}

/// What the accepted frame remembers about the `when` answers a window's
/// retained rows were built from, for GNU's cursor-motion reuse: the record,
/// and the chrome-dirty generation (GNU `update_mode_lines`, folding in
/// `prevent_redisplay_optimizations_p` from `force-mode-line-update`) at the
/// redisplay that produced them.  Numeric: no Lisp object outlives the frame.
/// Stored per window in [`crate::frame_visual_history::FrameVisualHistory`],
/// which is heap-backed per frame; it must not grow `RetainedWindowMatrix`
/// (embedded inline in the thread-local `ScrollCoverage`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisplayWhenRetention {
    pub(crate) record: DisplayWhenRecord,
    pub(crate) mode_line_generation: u64,
}

/// The outcome of evaluating the clauses a walk took by the structural rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayWhenSettlement {
    Settled(DisplayWhenRecord),
    /// An answer disagreed with the rule the walk drew by: relayout the frame
    /// once with the answers carried in the ledger.
    Replay,
}

/// The answers evaluated after a walk, carried to the frame relayout that
/// replays it, the one-replay budget per window, and the record each window's
/// walk settled in the current frame pass.  Owned by one logical frame
/// attempt (`RedisplayLispLedger`, a stack local of the frame function), so
/// none of it is thread-local or otherwise per-thread state that outlives an
/// attempt; its keys' Lisp objects are pushed onto the scratch GC root stack
/// under the frame function's scope when they are carried, so the Lisp the
/// replay runs first (fontification, the span scan) cannot collect them and
/// let another object take their address.
#[derive(Debug, Default)]
pub(crate) struct DisplayWhenReplayLedger {
    windows: FxHashMap<neovm_core::window::WindowId, DisplayWhenCarry>,
}

#[derive(Debug, Default)]
struct DisplayWhenCarry {
    answers: FxHashMap<DisplayWhenSiteKey, bool>,
    replayed: bool,
    /// What this frame pass's walk settled for the window; `None` until a
    /// leaf settles one (a window the pass did not lay out keeps no record).
    settled: Option<DisplayWhenRecord>,
}

impl DisplayWhenReplayLedger {
    /// The answers carried for WINDOW's replay, if any.
    #[inline]
    pub(crate) fn carried(
        &self,
        window: neovm_core::window::WindowId,
    ) -> Option<&FxHashMap<DisplayWhenSiteKey, bool>> {
        if self.windows.is_empty() {
            return None;
        }
        self.windows
            .get(&window)
            .map(|carry| &carry.answers)
            .filter(|answers| !answers.is_empty())
    }

    /// Record what this frame pass's walk settled for WINDOW.  Last write
    /// wins, like the per-frame list this replaces; the replay budget and its
    /// carried answers are untouched.
    #[inline]
    pub(crate) fn settle_window(
        &mut self,
        window: neovm_core::window::WindowId,
        record: DisplayWhenRecord,
    ) {
        self.windows.entry(window).or_default().settled = Some(record);
    }

    /// Every record this frame pass settled, in no particular order, for the
    /// accepted-frame dependency map.
    pub(crate) fn settled_entries(
        &self,
    ) -> impl Iterator<Item = (neovm_core::window::WindowId, DisplayWhenRecord)> + '_ {
        self.windows
            .iter()
            .filter_map(|(window, carry)| carry.settled.map(|record| (*window, record)))
    }

    /// Start a new frame pass: forget the records the previous pass settled.
    /// The one-replay budget is per logical attempt, not per pass, and stays.
    pub(crate) fn begin_frame_pass(&mut self) {
        for carry in self.windows.values_mut() {
            carry.settled = None;
        }
    }

    /// How many replays this ledger granted (tests).
    #[cfg(test)]
    pub(crate) fn replays(&self) -> usize {
        self.windows.values().filter(|carry| carry.replayed).count()
    }
}

/// Evaluate, with GNU's bindings and in visit order, the clauses the walk of
/// WINDOW took by the structural rule, and decide what its rows are worth.
///
/// Runs after the window's body and chrome, before the leaf is accepted;
/// the caller re-checks the attempt's freshness afterwards, since the FORMs
/// are arbitrary Lisp.  GNU evaluated them during its walk instead
/// (declared: the order relative to chrome differs).  A walk that recorded
/// nothing -- every window without unscanned `when` clauses -- settles here
/// without leaving the caller.
#[inline]
pub(crate) fn settle_display_when_visits(
    evaluator: &mut Context,
    buf_id: BufferId,
    window_id: neovm_core::window::WindowId,
    conditions: &DisplayWhenConditions,
    target: crate::display_property::DisplayPropertyTarget,
    ledger: &mut DisplayWhenReplayLedger,
) -> DisplayWhenSettlement {
    let DisplayWhenConditions::Evaluated(answers) = conditions else {
        // The span's forms could not be evaluated: the walk drew by the
        // structural rule throughout.
        return DisplayWhenSettlement::Settled(DisplayWhenRecord::Unknown);
    };
    if answers.visits.unevaluable.get() {
        return DisplayWhenSettlement::Settled(DisplayWhenRecord::Unknown);
    }
    if answers.visits.sites.borrow().is_empty() {
        return DisplayWhenSettlement::Settled(if answers.exact.is_empty() {
            DisplayWhenRecord::NoSites
        } else {
            DisplayWhenRecord::Known
        });
    }
    evaluate_recorded_display_when_visits(evaluator, buf_id, window_id, answers, target, ledger)
}

#[inline(never)]
fn evaluate_recorded_display_when_visits(
    evaluator: &mut Context,
    buf_id: BufferId,
    window_id: neovm_core::window::WindowId,
    answers: &DisplayWhenAnswers,
    target: crate::display_property::DisplayPropertyTarget,
    ledger: &mut DisplayWhenReplayLedger,
) -> DisplayWhenSettlement {
    let sites = std::mem::take(&mut *answers.visits.sites.borrow_mut());
    let evaluated =
        match evaluator.evaluate_display_when_sites(buf_id, Some(window_id), &sites, |spec| {
            crate::display_property::classify_resolved_display_spec(spec, target)
                .replacement()
                .is_some()
        }) {
            Ok(evaluated) => evaluated,
            Err(flow) => {
                tracing::warn!(
                    ?flow,
                    "display `when' forms met by the walk could not be evaluated; \
                 its rows stay unverified"
                );
                return DisplayWhenSettlement::Settled(DisplayWhenRecord::Unknown);
            }
        };
    // The walk drew every one of these clauses as holding.
    if evaluated.values().all(|holds| *holds) {
        return DisplayWhenSettlement::Settled(DisplayWhenRecord::Known);
    }
    let carry = ledger.windows.entry(window_id).or_default();
    if carry.replayed {
        // The bound: one replay per window and redisplay.  Keep the replay's
        // rows (the structural rule decided the clauses it newly met) and
        // never reuse them.
        return DisplayWhenSettlement::Settled(DisplayWhenRecord::Unknown);
    }
    carry.replayed = true;
    for (key, holds) in evaluated {
        neovm_core::emacs_core::eval::push_scratch_gc_roots(&[key.form, key.object]);
        carry.answers.insert(key, holds);
    }
    DisplayWhenSettlement::Replay
}

/// The terminal source position reached by a proved GNU mini measurement.
/// Ordinary/presentation scans keep the existing inclusive overlay anchor.
/// This numeric selector belongs to one exclusive evaluator attempt; it owns
/// no Lisp state and is never published to another mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DisplayWhenEndBoundary {
    InclusiveAnchor,
    BufferPositionReached,
}

/// Evaluate the `when` forms of every display spec the walk of
/// `[from, to]` of `buf_id` can reach: `display`, `line-prefix` and
/// `wrap-prefix` text properties and overlay properties, overlay
/// `before-string`/`after-string`s, the buffer-local and default
/// `line-prefix`/`wrap-prefix` values, and the `display` properties inside
/// all of those strings.
///
/// The forms run with `buf_id` current and the nonselected window's point
/// temporarily installed, as GNU's iterator does (src/xdisp.c:20533-20535,
/// 20578-20600). A FORM is evaluated once per site -- per occurrence, with
/// that occurrence's `object`, `position` and `buffer-position`, as GNU
/// binds them -- where a site is the start of a `display` property run the
/// walk meets (GNU evaluates again at every stop inside the run, with that
/// stop's position; the run's answer serves those here, declared).  Overlay
/// strings displayed at a boundary outside `[from, to]` are not reached by
/// this walk and are not evaluated.  Not scanned, so left to the structural rule
/// (declared): the `display` properties of strings reached through a
/// replacing `display` property (modifiers only there) and of margin
/// strings; frame-local chrome strings never get evaluated results.
pub(crate) fn evaluate_window_display_when_forms(
    evaluator: &mut Context,
    buf_id: BufferId,
    window_id: Option<u64>,
    from: CharPos0,
    to: CharPos0,
    target: crate::display_property::DisplayPropertyTarget,
) -> DisplayWhenConditions {
    evaluate_window_display_when_forms_at_end(
        evaluator,
        buf_id,
        window_id,
        from,
        to,
        target,
        DisplayWhenEndBoundary::InclusiveAnchor,
    )
}

pub(crate) fn evaluate_window_display_when_forms_at_end(
    evaluator: &mut Context,
    buf_id: BufferId,
    window_id: Option<u64>,
    from: CharPos0,
    to: CharPos0,
    target: crate::display_property::DisplayPropertyTarget,
    end_boundary: DisplayWhenEndBoundary,
) -> DisplayWhenConditions {
    // GNU's `Vline_prefix` / `Vwrap_prefix` DEFVARs, read through the same
    // buffer-defaults precedence as the generic helper but without resolving
    // the two fixed spellings by name on every layout pass.
    let default_prefixes: Vec<Value> = [
        &crate::fixed_symbols::LINE_PREFIX,
        &crate::fixed_symbols::WRAP_PREFIX,
    ]
    .into_iter()
    .filter_map(|symbol| evaluator.buffer_default_value_fixed(symbol))
    .collect();
    let run_sites = match evaluator.buffer_manager().get(buf_id) {
        Some(buffer) => collect_when_form_run_sites(
            buffer,
            window_id,
            from,
            to,
            &default_prefixes,
            end_boundary,
        ),
        None => Vec::new(),
    };
    let sites: Vec<DisplayWhenSite> = run_sites.iter().map(|run| run.site).collect();
    match evaluator.evaluate_display_when_sites(
        buf_id,
        window_id.map(neovm_core::window::WindowId),
        &sites,
        |spec| {
            crate::display_property::classify_resolved_display_spec(spec, target)
                .replacement()
                .is_some()
        },
    ) {
        Ok(answers) => {
            DisplayWhenConditions::evaluated(Value::make_buffer(buf_id), &run_sites, answers)
        }
        // A `throw` (or an exit) out of a display FORM unwinds redisplay in
        // GNU; redisplay here has no such exit, so the forms of this span
        // fall back to the structural rule for this attempt (declared).  The
        // same fallback covers a buffer that is not live any more.
        Err(flow) => {
            tracing::warn!(
                ?flow,
                "display `when' forms could not be evaluated for this layout; \
                 using the structural rule"
            );
            DisplayWhenConditions::structural()
        }
    }
}

#[cfg(test)]
fn collect_when_form_sites(
    buffer: &Buffer,
    window_id: Option<u64>,
    from: CharPos0,
    to: CharPos0,
    default_prefixes: &[Value],
) -> Vec<DisplayWhenSite> {
    collect_when_form_run_sites(
        buffer,
        window_id,
        from,
        to,
        default_prefixes,
        DisplayWhenEndBoundary::InclusiveAnchor,
    )
    .into_iter()
    .map(|run| run.site)
    .collect()
}

/// The three fixed property spellings the display-when scan reads. The
/// spelling and the identity come from the same `WellKnown` variant, so the
/// scan is a static-id read instead of a per-row `intern`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WhenScanProperty {
    Display,
    LinePrefix,
    WrapPrefix,
}

impl WhenScanProperty {
    const ALL: [Self; 3] = [Self::Display, Self::LinePrefix, Self::WrapPrefix];

    const fn well_known(self) -> WellKnown {
        match self {
            Self::Display => WellKnown::Display,
            Self::LinePrefix => WellKnown::LinePrefix,
            Self::WrapPrefix => WellKnown::WrapPrefix,
        }
    }
}

fn collect_when_form_run_sites(
    buffer: &Buffer,
    window_id: Option<u64>,
    from: CharPos0,
    to: CharPos0,
    default_prefixes: &[Value],
    end_boundary: DisplayWhenEndBoundary,
) -> Vec<DisplayWhenRunSite> {
    let mut sites = Vec::new();
    let buffer_object = Value::make_buffer(buffer.id());
    let to = to.min(buffer.layout_point_max_char_pos()).max(from);
    let gnu_pos = |pos: CharPos0| pos.get() as i64 + 1;

    for property in WhenScanProperty::ALL {
        let name_value = Value::symbol(property.well_known());
        // This proof covers only raw buffer properties. Overlays, prefix
        // variables and properties inside strings are independent sources
        // and must still be scanned below.
        if buffer.text_props_property_name_presence(name_value)
            == PropertyNamePresence::DefinitelyAbsent
        {
            continue;
        }
        let mut pos = from;
        while pos < to {
            let bytepos = buffer.layout_char_pos_to_emacs_byte_pos(pos);
            let next = buffer.next_watched_property_change_at_char_pos(pos, to, &[name_value]);
            let run_end = if next <= pos { to } else { next };
            if let Some(value) = buffer.layout_text_prop_at_emacs_byte_pos(bytepos, name_value) {
                if property == WhenScanProperty::Display {
                    collect_when_forms(
                        value,
                        buffer_object,
                        gnu_pos(pos),
                        gnu_pos(run_end),
                        gnu_pos(pos),
                        &mut sites,
                    );
                } else {
                    collect_prefix_string_when_forms(value, gnu_pos(pos), &mut sites);
                }
            }
            if next <= pos {
                break;
            }
            pos = next;
        }
    }

    // Overlay `display`, `line-prefix` and `wrap-prefix` properties, for the
    // overlays of this window (GNU skips one scoped to another window both
    // for its strings, src/xdisp.c:7153-7156, and for its properties,
    // `overlay_matches_window` in `get_char_property_and_overlay`).
    // Declared: a prefix from an overlay is bound to the overlay's start; GNU
    // reads the prefix at each row start (src/xdisp.c:25131-25132, 25180).
    let overlays = buffer.overlays();
    let mut boundaries: Vec<CharPos0> = Vec::new();
    for overlay in overlays.overlays_in_emacs_byte_range(EmacsByteRange::new(
        buffer
            .layout_char_pos_to_emacs_byte_pos(from)
            .saturating_sub_len(neovm_core::buffer::EmacsByteLen::new(1)),
        buffer
            .layout_char_pos_to_emacs_byte_pos(to)
            .add_len(neovm_core::buffer::EmacsByteLen::new(1)),
    )) {
        if !overlays.overlay_applies_to_window(overlay, window_id) {
            continue;
        }
        let (Some(start), Some(end)) = (
            overlays
                .overlay_start_emacs_byte_pos(overlay)
                .map(|bytepos| buffer.layout_emacs_byte_pos_to_char_pos(bytepos)),
            overlays
                .overlay_end_emacs_byte_pos(overlay)
                .map(|bytepos| buffer.layout_emacs_byte_pos_to_char_pos(bytepos)),
        ) else {
            continue;
        };
        // A string is displayed where the walk stops at the overlay's start or
        // end.  Boundaries in `[from, to]` are kept: `to` is the end of the
        // buffer for every window whose text ends inside the span, and GNU
        // loads the strings anchored at ZV when the iterator arrives there
        // (`next_element_from_buffer`, src/xdisp.c:9846-9860; the walk's
        // end-of-buffer anchor path).  A `to` short of the buffer end is the
        // fontification budget rather than a row edge, so a boundary there
        // and forms of rows past the last displayed one are evaluated
        // although GNU would not reach them (declared).
        for at in [start, end] {
            if at >= from
                && at <= to
                && (end_boundary == DisplayWhenEndBoundary::InclusiveAnchor || at < to)
            {
                boundaries.push(at);
            }
        }
        // Properties come from overlays covering a displayed position: GNU
        // `get_char_property_and_overlay` skips an overlay whose end is at
        // or before the position (`node->end < pos + 1`, src/textprop.c:
        // 652-653), so an empty overlay contributes none.  Declared: GNU
        // then keeps only the highest-priority overlay's value, or the text
        // property when no overlay supplies one (src/textprop.c:656-675);
        // every covering overlay's form and the text property's are
        // evaluated here.
        if start < end && start < to && end > from {
            let at = start.max(from);
            if let Some(value) =
                overlays.overlay_get_named(overlay, Value::symbol(&crate::fixed_symbols::DISPLAY))
            {
                collect_when_forms(
                    value,
                    buffer_object,
                    gnu_pos(at),
                    gnu_pos(end),
                    gnu_pos(at),
                    &mut sites,
                );
            }
            for symbol in [WellKnown::LinePrefix, WellKnown::WrapPrefix] {
                if let Some(value) = overlays.overlay_get_named(overlay, Value::symbol(symbol)) {
                    collect_prefix_string_when_forms(value, gnu_pos(at), &mut sites);
                }
            }
        }
    }
    // Overlay before- and after-strings: ask the walk's own resolver which
    // strings it displays at each boundary, so the `window` filter, the
    // invisible-text rule (a hidden overlay shows both strings at whichever
    // end the walk reaches, src/xdisp.c:7158-7175) and the one-position load
    // window around the iterator (src/xdisp.c:7141-7150) have one owner.
    // `buffer-position` is that position (`it->current.pos`,
    // src/xdisp.c:5919-5923).
    boundaries.sort_unstable();
    boundaries.dedup();
    let access = RustTextPropAccess::new_for_optional_window(buffer, window_id);
    for at in boundaries {
        for entry in access.overlay_strings_at(at.get() as i64) {
            collect_prefix_string_when_forms(entry.string, gnu_pos(at), &mut sites);
        }
    }

    // The buffer-local and the default `line-prefix`/`wrap-prefix` values
    // (GNU reads `Vline_prefix`/`Vwrap_prefix`, whichever binding applies).
    for symbol in [WellKnown::LinePrefix, WellKnown::WrapPrefix] {
        if let Some(value) = buffer.buffer_local_value_id(symbol.id()) {
            collect_prefix_string_when_forms(value, gnu_pos(from), &mut sites);
        }
    }
    for value in default_prefixes {
        collect_prefix_string_when_forms(*value, gnu_pos(from), &mut sites);
    }
    sites
}

/// Keep a whole property containing a nonliteral FORM. Evaluation must see
/// preceding replacements, including unconditional ones, to decide reachability.
/// `end` is the exclusive GNU position in `object` where the run ends.
fn collect_when_forms(
    value: Value,
    object: Value,
    position: i64,
    end: i64,
    buffer_position: i64,
    sites: &mut Vec<DisplayWhenRunSite>,
) {
    let specs = DisplayPropertySpecs::of(value);
    specs.for_each(|spec| {
        if let Some((form, _)) = display_spec_when_parts(spec)
            && !form.is_nil()
            && !form.is_bare_symbol_named("t")
        {
            sites.push(DisplayWhenRunSite {
                site: DisplayWhenSite {
                    property: value,
                    object,
                    position,
                    buffer_position,
                },
                end,
            });
            return std::ops::ControlFlow::Break(());
        }
        std::ops::ControlFlow::Continue(())
    });
}

/// The `when` forms inside a prefix STRING's own `display` properties, with
/// `object` bound to the string and `position` to the index in it; each
/// run of one `display` value is one site.
fn collect_prefix_string_when_forms(
    string: Value,
    buffer_position: i64,
    sites: &mut Vec<DisplayWhenRunSite>,
) {
    let Some(lisp_string) = string.as_lisp_string() else {
        return;
    };
    let Some(props) = get_string_text_properties_table_for_value(string) else {
        return;
    };
    let display = Value::symbol(&crate::fixed_symbols::DISPLAY);
    let length = lisp_string.schars();
    let mut run: Option<(Value, usize)> = None;
    for index in 0..=length {
        let value = if index < length {
            props.get_property_at_char_pos(CharPos0::new(index), display)
        } else {
            None
        };
        if run.map(|(previous, _)| previous) == value {
            continue;
        }
        if let Some((previous, start)) = run.take() {
            collect_when_forms(
                previous,
                string,
                start as i64,
                index as i64,
                buffer_position,
                sites,
            );
        }
        run = value.map(|value| (value, index));
    }
}

#[cfg(test)]
#[path = "display_when/tests/display_when_test.rs"]
mod tests;
