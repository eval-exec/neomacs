//! GNU `scan_sexps_forward` (src/syntax.c:3174) as a resumable state
//! machine: the one scan loop behind `parse-partial-sexp` and `back_comment`'s
//! forward re-parse.
//!
//! Every variable the loop carries from one character to the next lives in a
//! [`LoopState`] at a loop top, so a scan can stop there and a later call can
//! continue it ([`Entry::Resume`]) with exactly the iterations the
//! uninterrupted scan would have run.
//!
//! Resumes are exact by determinism, not by heuristics. The state at a loop top
//! `p` of a scan with range end `TO > p` depends only on the text, syntax
//! table and `syntax-table` properties over characters `[FROM, p]` (a step
//! peeks at the next character only when that character is still before the
//! range end), on FROM, the starting state and the options -- never on `TO`
//! itself, since every range-end test before `p` compared a position `<= p`.
//! Resuming at `p` under the same inputs therefore runs the same iterations.
//! Two consequences shape every caller:
//!
//! * a loop top is a valid resume point only for a query whose range end lies
//!   strictly after it;
//! * the state a scan reaches AT its range end is not a loop-top state of a
//!   longer scan (the last step did not peek past the end), so it is never
//!   resumed from.
//!
//! A [`ScanMode`] sees the loop tops it asks for. `Plain` compiles to the
//! loop with no hook at all.

use super::*;

/// The loop-carried state of a scan at a loop top: everything the iterations
/// after it read.
///
/// The per-scan caches the loop also keeps (the `syntax-table` property run,
/// the flat ASCII classifier, the memoized property-free run end) are not part
/// of it: each is rebuilt on resume and answers identically by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LoopState {
    /// Absolute 0-based position of the character about to be scanned.
    pub(super) char_pos: usize,
    /// Its Emacs byte position.
    pub(super) byte_pos: EmacsBytePos,
    pub(super) pps: PartialParseState,
    /// Start of the symbol or word run still being scanned (GNU's pending
    /// `curlevel->last` before `symdone`).
    pub(super) atom_start: Option<i64>,
    /// GNU `forw_comment`'s `prev_syntax` for a resumed comment. Consumed by
    /// the first iteration of a fresh scan, so it is `None` at every later
    /// loop top.
    pub(super) comment_resume: Option<CommentResumeSyntax>,
}

/// Where a scan starts.
pub(super) enum Entry {
    /// GNU `scan_sexps_forward` from `from_char` (absolute, 0-based) in
    /// `state`: an OLDSTATE internalized by
    /// [`PartialParseState::from_oldstate`] (GNU `internalize_parse_state`),
    /// which callers do before anything else can run, as GNU does.
    Fresh {
        from_char: usize,
        state: PartialParseState,
        /// Whether an OLDSTATE was given at all: only then can the first
        /// character complete a two-character comment opener begun before
        /// FROM (GNU's entry through `in_2char_comment_start`).
        from_oldstate: bool,
    },
    /// Continue a scan at the loop top it paused or was recorded at.
    Resume {
        at: LoopState,
        /// The syntax the character at `at` was already read with, when the
        /// pause came between reading it and finishing its step (GNU reads a
        /// character's syntax in `INC_FROM` BEFORE `UPDATE_SYNTAX_TABLE_FORWARD`
        /// on the next position can run `syntax-propertize`). Honoured only by
        /// modes with [`ScanMode::OVERRIDE`].
        first_syntax: Option<(SyntaxClass, SyntaxFlags)>,
    },
}

/// How a scan ended.
pub(super) enum ScanEnd {
    Finished(ScanFinish),
    /// The mode asked to pause at this loop top.
    Paused(LoopState),
}

/// A finished scan: the finalized state and where the scan stopped.
pub(super) struct ScanFinish {
    pub(super) state: PartialParseState,
    /// The Lisp position the scan stopped at (point after `parse-partial-sexp`).
    pub(super) stop: i64,
}

/// A loop top as a [`ScanMode`] sees it.
pub(super) struct LoopTop<'a> {
    pub(super) char_pos: usize,
    pub(super) byte_pos: EmacsBytePos,
    pub(super) pps: &'a PartialParseState,
    pub(super) atom_start: Option<i64>,
}

impl LoopTop<'_> {
    /// The state here, for a mode that keeps it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn to_state(&self) -> LoopState {
        LoopState {
            char_pos: self.char_pos,
            byte_pos: self.byte_pos,
            pps: self.pps.clone(),
            atom_start: self.atom_start,
            comment_resume: None,
        }
    }
}

/// What the loop does after a mode's loop-top hook.
pub(super) enum TopAction {
    /// Keep scanning; call the hook again at the first loop top at or after
    /// this absolute position.
    Continue(usize),
    /// Stop here and return the state ([`ScanEnd::Paused`]).
    Pause,
}

/// A scan mode: what, if anything, happens at loop tops.
pub(super) trait ScanMode {
    /// Whether the hook exists at all. `false` removes the per-character
    /// target compare from the monomorphised loop.
    const ACTIVE: bool;
    /// Whether [`Entry::Resume::first_syntax`] can be `Some`. `false` removes
    /// the per-character override test.
    const OVERRIDE: bool = false;
    /// The first absolute position at which to call [`Self::at_loop_top`].
    fn first_target(&self) -> usize {
        usize::MAX
    }
    /// Called at each loop top at or after the current target.
    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction;
}

/// No hook: every Lisp `parse-partial-sexp` that needs nothing else.
pub(super) struct Plain;

impl ScanMode for Plain {
    const ACTIVE: bool = false;
    #[inline(always)]
    fn at_loop_top(&mut self, _top: LoopTop<'_>) -> TopAction {
        TopAction::Continue(usize::MAX)
    }
}

/// Where a recording parse notes safe restart positions for `back_comment`:
/// see `buffer_text::SyntaxSafePositions`.
pub(super) struct SafePositionRecorder<'a> {
    /// The next char position at or after which a safe position is noted.
    pub(super) next_target: usize,
    pub(super) chunk: usize,
    pub(super) points: &'a mut Vec<usize>,
}

impl ScanMode for SafePositionRecorder<'_> {
    const ACTIVE: bool = true;

    fn first_target(&self) -> usize {
        self.next_target
    }

    /// A safe restart position: about to scan a character outside every
    /// comment and string. Every two-character decision about the previous
    /// character was made with this one visible (the parse peeks), and an
    /// escape consumed it rather than stopping before it, so a fresh parse
    /// from here reaches the same comment/string state as this one.
    #[inline]
    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction {
        if top.pps.in_comment.is_none() && top.pps.in_string.is_none() {
            debug_assert!(!top.pps.quoted, "quoted is only set at the range end");
            self.points.push(top.char_pos);
            self.next_target = top.char_pos.saturating_add(self.chunk);
        }
        TopAction::Continue(self.next_target)
    }
}

/// Run the scan loop from `entry` to `to_char` (absolute, 0-based, already
/// clamped to the accessible region by the caller).
///
/// `from_char <= to_char` for a fresh entry; a resume is only ever made at a
/// loop top strictly before `to_char`.
#[allow(clippy::too_many_arguments)] // mirrors GNU `scan_sexps_forward`'s parameters
pub(super) fn run_parse_loop<M: ScanMode>(
    buf: &Buffer,
    table: &SyntaxTable,
    entry: Entry,
    to_char: usize,
    target_depth: Option<i64>,
    stop_before: bool,
    commentstop: CommentStopMode,
    props: SyntaxProperties<'_>,
    escape_policy: CommentEndEscapePolicy,
    mode: &mut M,
) -> ScanEnd {
    let (
        from_char,
        mut chars,
        mut state,
        mut atom_start,
        mut comment_resume_syntax,
        mut first_syntax,
        from_oldstate,
    ) = match entry {
        Entry::Fresh {
            from_char,
            state,
            from_oldstate,
        } => {
            let point_min = buf.accessible_char_region().start().get();
            let comment_resume = (state.in_comment.is_some() && from_char != point_min)
                .then(|| CommentResumeSyntax::from_parse_state(state.prev_syntax));
            (
                from_char,
                ParseBufferChars::new(buf, offset_char_pos(CharPos0::ZERO, from_char)),
                state,
                None,
                comment_resume,
                None,
                from_oldstate,
            )
        }
        Entry::Resume { at, first_syntax } => {
            debug_assert!(at.char_pos < to_char, "a resume point lies before TO");
            (
                at.char_pos,
                ParseBufferChars::at_emacs_byte(buf, at.byte_pos),
                at.pps,
                at.atom_start,
                at.comment_resume,
                first_syntax,
                false,
            )
        }
    };
    debug_assert!(
        M::OVERRIDE || first_syntax.is_none(),
        "only an overriding mode resumes mid-step"
    );
    let to_idx = to_char - from_char;
    // `prop_cache` carries both the `syntax-table` property run cache and the
    // lazily-filled ASCII syntax memo consumed by
    // `effective_syntax_entry_for_abs_char`.
    let prop_cache = SyntaxPropRange::new(props);

    // Long parses classify ASCII chars through a flat local table while the
    // prop cache positively covers the position with no `syntax-table`
    // property: one array index replaces the layered per-char prop-cell
    // check + Option<Cell> memo decode.  Entries are `syntax_entry_from_table`
    // — the identical computation the memo caches — so this is
    // behavior-preserving by construction. (The fill was once gated on span
    // length behind an `Option`; the gate is long gone, so the discriminant
    // test was costing a branch on every character.)
    let flat_ascii = flat_ascii_entries_for_table(table);

    // An IGNORING scan's run cache is built by `PropRunCells::covering_everything`
    // — `start = 0`, `end = usize::MAX`, `value = None` — and can never refill
    // (`syntax_table_prop_at_char` only reaches `refill_run` on the `Honor`
    // arm).  The cache is therefore property-free at every buffer position in
    // such a scan.  Hoisting that fact here replaces three `Cell` loads plus
    // two compares per character — unhoistable by the compiler, since interior
    // mutability forces a reload after every call — with one register test.
    let props_prop_free_everywhere = matches!(props, SyntaxProperties::Ignore);

    // Memoized end of the current prop-free run (see
    // `SyntaxPropRange::prop_free_run_end`): positions below it are known
    // covered and property-free, so the per-character probe collapses to one
    // compare.  Zero = nothing known; every classifier call resets it, so a
    // refill can never be observed through a stale endpoint.
    let mut prop_free_until: usize = 0;

    let mut idx = 0;
    let mut comment_exit = ParseCommentExit::RangeBoundary;

    let finish_atom = |state: &mut PartialParseState, atom_start: &mut Option<i64>| {
        if let Some(start) = atom_start.take() {
            state.finish_current_level_sexp(start);
        }
    };

    // GNU `scan_sexps_forward` enters its loop through `in_2char_comment_start`
    // (src/syntax.c:3265): a parse RESUMED from an OLDSTATE whose element 10
    // names a comment-start-first character pairs it with the first character
    // here, the pair straddling the previous TO. This parser pairs by peeking
    // inside its own range, so without the entry check a split `/*` (or a
    // nested `(*`) was read as punctuation and a star: a resumed parse -- every
    // syntax-ppss cache midpoint -- missed the comment and miscounted parens.
    let mut stopped_at_entry = false;
    if from_oldstate
        && state.in_comment.is_none()
        && state.in_string.is_none()
        && !state.quoted
        && to_idx > 0
    {
        let prev_flags = SyntaxFlags::new(((state.prev_syntax >> 16) & 0xff) as u8);
        if prev_flags.contains(SyntaxFlags::COMMENT_START_FIRST) {
            let (_, next_flags) =
                syntax_class_and_flags(buf, table, chars.peek(), from_char, &prop_cache);
            if next_flags.contains(SyntaxFlags::COMMENT_START_SECOND) {
                state.in_comment = Some(ParseCommentState::Syntax {
                    depth: 1,
                    flavor: CommentFlavor::two_char_start(prev_flags, next_flags),
                });
                // GNU `comstr_start = prev_from`: the Lisp position of the
                // pair's first character, the one before FROM.
                state.comment_or_string_start = Some(from_char as i64);
                chars.skip();
                idx = 1;
                state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                // GNU `atcomment: if (commentstop || boundary_stop) goto done`.
                if commentstop != CommentStopMode::None {
                    comment_exit = ParseCommentExit::StoppedAtEntry;
                    stopped_at_entry = true;
                }
            }
        }
    }

    // Stopped at an entry comment: the loop does not run (one compare, not a
    // flag test per character).
    let loop_end = if stopped_at_entry { idx } else { to_idx };
    let mut target = if M::ACTIVE {
        mode.first_target()
    } else {
        usize::MAX
    };
    let mut paused = false;
    while idx < loop_end {
        let abs_char = from_char + idx;
        // The mode's loop-top hook: a register compare against the next
        // position it asked to see, compiled out entirely for `Plain`.
        if M::ACTIVE && abs_char >= target {
            match mode.at_loop_top(LoopTop {
                char_pos: abs_char,
                byte_pos: chars.byte_pos,
                pps: &state,
                atom_start,
            }) {
                TopAction::Continue(next) => target = next,
                TopAction::Pause => {
                    paused = true;
                    break;
                }
            }
        }
        let pos1 = (abs_char + 1) as i64;
        let ch = chars.next();
        // A resume in the middle of a step brings the syntax the character
        // was read with before the pause (`Entry::Resume::first_syntax`);
        // constant `None` for every mode that cannot pause there.
        let overridden = if M::OVERRIDE {
            first_syntax.take()
        } else {
            None
        };
        let (class, flags) = if let Some(first) = overridden {
            first
        } else {
            // Flat-table fast path, in ascending order of cost: a register
            // test (ignoring scan), a register compare against the memoized
            // run end, then the three-`Cell` probe that also refreshes the
            // memo.
            let flat_ok = (ch as u32) < 128
                && if props_prop_free_everywhere || abs_char < prop_free_until {
                    true
                } else if let Some(run_end) = prop_cache.prop_free_run_end(abs_char) {
                    prop_free_until = run_end;
                    true
                } else {
                    false
                };
            if flat_ok {
                let entry = flat_ascii[ch as usize];
                (entry.class, entry.flags)
            } else {
                // The classifier may refill the run cache, which retires the
                // memoized endpoint. Dropping it costs one re-probe on the
                // next character and keeps the memo an under-approximation.
                prop_free_until = 0;
                let entry =
                    effective_syntax_entry_for_abs_char(buf, table, ch, abs_char, &prop_cache);
                (entry.class, entry.flags)
            }
        };
        let resumed_after = comment_resume_syntax.take();

        // GNU INC_FROM records `prev_from_syntax` for every position it steps
        // over; element 10 of the result reports it when the final position
        // holds a quote/comment-delimiter-first construct.  Specific arms below
        // reset it to Smax exactly where GNU does (2-char comment start, string
        // and comment terminators).
        state.prev_syntax = parse_prev_syntax_int(class, flags);

        if state.quoted {
            state.quoted = false;
            if state.in_comment.is_none() {
                idx += 1;
                continue;
            }
            // GNU enters `startincomment` before consulting `start_quoted`.
            // In a resumed comment the old quoted bit describes how the
            // previous range ended; it must not consume the first character
            // of this range (which may begin a two-character closer).
        }

        if let Some(string_state) = state.in_string {
            match class {
                SyntaxClass::Escape | SyntaxClass::CharQuote => {
                    idx += 1;
                    if idx < to_idx {
                        chars.skip();
                        idx += 1;
                    } else {
                        state.quoted = true;
                    }
                    continue;
                }
                SyntaxClass::StringFence if string_state == ParseStringState::Fence => {
                    state.finish_string();
                    idx += 1;
                    if commentstop == CommentStopMode::SyntaxTable {
                        break;
                    }
                    continue;
                }
                SyntaxClass::StringDelim if matches!(string_state, ParseStringState::Delim(term) if ch == term) =>
                {
                    state.finish_string();
                    idx += 1;
                    if commentstop == CommentStopMode::SyntaxTable {
                        break;
                    }
                    continue;
                }
                _ => {
                    idx += 1;
                    continue;
                }
            }
        }

        if let Some(comment_state) = state.in_comment {
            match comment_state {
                ParseCommentState::Fence => {
                    if class == SyntaxClass::CommentFence {
                        idx += 1;
                        state.in_comment = None;
                        state.comment_or_string_start = None;
                        state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        if commentstop == CommentStopMode::SyntaxTable {
                            break;
                        }
                        continue;
                    }
                    if escape_policy == CommentEndEscapePolicy::EscapeQuotesEnder
                        && matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote)
                    {
                        idx += 1;
                        if idx < to_idx {
                            chars.skip();
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        } else {
                            state.quoted = true;
                        }
                        continue;
                    }
                    idx += 1;
                    continue;
                }
                ParseCommentState::Syntax {
                    depth: comment_depth,
                    flavor,
                } => {
                    // GNU enters `forw_comment` at `forw_incomment` when an
                    // OLDSTATE supplies `prev_syntax`.  Resolve that
                    // boundary-spanning pair before interpreting CURRENT on
                    // its own, with ender-before-nested-opener precedence.
                    if let Some(previous) = resumed_after {
                        let boundary_marker = previous.marker_with(flags);
                        if boundary_marker.ender == Some(flavor) {
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            if state.close_comment_level(comment_depth, flavor)
                                == CommentEnderEffect::CommentClosed
                                && commentstop == CommentStopMode::SyntaxTable
                            {
                                break;
                            }
                            continue;
                        }
                        if flavor.nesting.is_nested() && boundary_marker.opener == Some(flavor) {
                            state.in_comment = Some(ParseCommentState::Syntax {
                                depth: comment_depth + 1,
                                flavor,
                            });
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            continue;
                        }
                    }

                    if class == SyntaxClass::EndComment
                        && CommentFlavor::single(flags) == flavor
                        && !resumed_after
                            .is_some_and(|previous| previous.quotes_single_ender(escape_policy))
                    {
                        idx += 1;
                        let effect = state.close_comment_level(comment_depth, flavor);
                        if effect == CommentEnderEffect::CommentClosed {
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                            if commentstop == CommentStopMode::SyntaxTable {
                                break;
                            }
                        }
                        continue;
                    }

                    // GNU applies a nested single-character opener before
                    // considering a two-character marker beginning at the
                    // same character.
                    let mut effective_comment_depth = comment_depth;
                    if flavor.nesting.is_nested()
                        && class == SyntaxClass::Comment
                        && CommentFlavor::single(flags) == flavor
                    {
                        effective_comment_depth += 1;
                        state.in_comment = Some(ParseCommentState::Syntax {
                            depth: effective_comment_depth,
                            flavor,
                        });
                    }

                    if escape_policy == CommentEndEscapePolicy::EscapeQuotesEnder
                        && matches!(class, SyntaxClass::Escape | SyntaxClass::CharQuote)
                    {
                        idx += 1;
                        if idx < to_idx {
                            chars.skip();
                            idx += 1;
                            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        } else {
                            state.quoted = true;
                        }
                        continue;
                    }

                    let pair = if (flags.contains(SyntaxFlags::COMMENT_END_FIRST)
                        || flags.contains(SyntaxFlags::COMMENT_START_FIRST))
                        && idx + 1 < to_idx
                    {
                        let (_, next_flags) = syntax_class_and_flags(
                            buf,
                            table,
                            chars.peek(),
                            abs_char + 1,
                            &prop_cache,
                        );
                        // Look-ahead may refill the run: retire the memo.
                        prop_free_until = 0;
                        Some(CommentMarkerCapabilities::between(flags, next_flags))
                    } else {
                        None
                    };

                    if pair.is_some_and(|capabilities| capabilities.ender == Some(flavor)) {
                        chars.skip();
                        idx += 2;
                        state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        if state.close_comment_level(effective_comment_depth, flavor)
                            == CommentEnderEffect::CommentClosed
                            && commentstop == CommentStopMode::SyntaxTable
                        {
                            break;
                        }
                        continue;
                    }

                    if flavor.nesting.is_nested()
                        && pair.is_some_and(|capabilities| capabilities.opener == Some(flavor))
                    {
                        state.in_comment = Some(ParseCommentState::Syntax {
                            depth: effective_comment_depth + 1,
                            flavor,
                        });
                        chars.skip();
                        idx += 2;
                        state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
                        continue;
                    }

                    idx += 1;
                    continue;
                }
            }
        }

        // GNU recognizes an atomic two-character comment opener immediately
        // after advancing over its first character.  The pair therefore
        // overrides STOPBEFORE and every raw base class on that character.
        let opening_pair = if flags.contains(SyntaxFlags::COMMENT_START_FIRST) && idx + 1 < to_idx {
            let (_, next_flags) =
                syntax_class_and_flags(buf, table, chars.peek(), abs_char + 1, &prop_cache);
            // Look-ahead may refill the run: retire the memo.
            prop_free_until = 0;
            next_flags
                .contains(SyntaxFlags::COMMENT_START_SECOND)
                .then_some(CommentFlavor::two_char_start(flags, next_flags))
        } else {
            None
        };

        if let Some(flavor) = opening_pair {
            // GNU detects the pair inside `symstarted`: word-like raw syntax
            // keeps the preceding atom pending, while every other class first
            // takes the ordinary `symdone` path.
            if !matches!(
                class,
                SyntaxClass::Word
                    | SyntaxClass::Symbol
                    | SyntaxClass::Quote
                    | SyntaxClass::Escape
                    | SyntaxClass::CharQuote
            ) {
                finish_atom(&mut state, &mut atom_start);
            } else {
                // The jump from GNU's `symstarted` into `atcomment` bypasses
                // `symdone`; the interrupted atom is neither pending nor a
                // completed sexp after the comment scan.
                atom_start = None;
            }
            state.in_comment = Some(ParseCommentState::Syntax { depth: 1, flavor });
            state.comment_or_string_start = Some(pos1);
            chars.skip();
            idx += 2;
            state.prev_syntax = PARSE_PREV_SYNTAX_SMAX;
            if commentstop != CommentStopMode::None {
                comment_exit = ParseCommentExit::StoppedAtEntry;
                break;
            }
            continue;
        }

        if stop_before
            && matches!(
                class,
                SyntaxClass::Escape
                    | SyntaxClass::CharQuote
                    | SyntaxClass::Word
                    | SyntaxClass::Symbol
                    | SyntaxClass::Open
                    | SyntaxClass::StringDelim
                    | SyntaxClass::StringFence
            )
        {
            break;
        }

        // GNU `symstarted` continues a symbol/atom run across word, symbol,
        // quote AND escape/char-quote constituents; only some other syntax
        // class triggers `symdone' (which records the completed sexp).  Escape
        // and char-quote must therefore NOT finish the pending atom here —
        // doing so wrongly promoted an escaped run to a completed sexp (the
        // `\(a\)` / `a\(b\)c` divergence).
        if !matches!(
            class,
            SyntaxClass::Word
                | SyntaxClass::Symbol
                | SyntaxClass::Quote
                | SyntaxClass::Escape
                | SyntaxClass::CharQuote
        ) {
            finish_atom(&mut state, &mut atom_start);
        }

        match class {
            SyntaxClass::Open => {
                state.open_level(pos1);
                idx += 1;
                if target_depth == Some(state.depth) {
                    break;
                }
                continue;
            }
            SyntaxClass::Close => {
                state.close_level();
                idx += 1;
                if target_depth == Some(state.depth) {
                    break;
                }
                continue;
            }
            SyntaxClass::StringDelim => {
                state.in_string = Some(ParseStringState::Delim(ch));
                state.in_string_from_oldstate = false;
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop == CommentStopMode::SyntaxTable {
                    break;
                }
                continue;
            }
            SyntaxClass::StringFence => {
                state.in_string = Some(ParseStringState::Fence);
                state.in_string_from_oldstate = false;
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop == CommentStopMode::SyntaxTable {
                    break;
                }
                continue;
            }
            SyntaxClass::Comment => {
                state.in_comment = Some(ParseCommentState::Syntax {
                    depth: 1,
                    flavor: CommentFlavor::single(flags),
                });
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop != CommentStopMode::None {
                    comment_exit = ParseCommentExit::StoppedAtEntry;
                    break;
                }
                continue;
            }
            SyntaxClass::CommentFence => {
                state.in_comment = Some(ParseCommentState::Fence);
                state.comment_or_string_start = Some(pos1);
                idx += 1;
                if commentstop != CommentStopMode::None {
                    comment_exit = ParseCommentExit::StoppedAtEntry;
                    break;
                }
                continue;
            }
            SyntaxClass::Escape | SyntaxClass::CharQuote => {
                // GNU `scan_sexps_forward` treats an escape/char-quote like the
                // start of a symbol run: it records `curlevel->last = prev_from`
                // (anchoring the atom at the escape) and skips the quoted char,
                // then continues scanning the symbol.  The atom is only promoted
                // to a *completed* sexp (`curlevel->prev`, our element 2) when a
                // non-symbol char ends the run via `symdone'.
                atom_start.get_or_insert(pos1);
                if idx + 1 < to_idx {
                    chars.skip();
                    idx += 2;
                    continue;
                }
                // Escape with no following char: GNU jumps to `endquoted',
                // which sets state->quoted but BYPASSES `symdone', so the
                // pending atom is never recorded as a completed sexp.  Drop it
                // so element 2 stays nil (matching GNU's `\(a\)`).  prev_syntax
                // already holds the escape's syntax for element 10.
                atom_start = None;
                state.quoted = true;
                idx += 1;
                continue;
            }
            SyntaxClass::Word | SyntaxClass::Symbol => {
                atom_start.get_or_insert(pos1);
            }
            SyntaxClass::Quote => {
                // GNU `scan_sexps_forward`: a top-level Squote (expression
                // prefix, e.g. `'`, backquote, `,`, `#`) falls into the
                // `default' arm — "Ignore whitespace, punctuation, quote,
                // endcomment." — so it does NOT begin an atom and never
                // becomes element 2 (last-complete-sexp start).  But within an
                // in-progress symbol run Squote is a constituent (the inner
                // `symstarted` loop's `case Squote: break;`), so an already
                // started atom keeps running across it (the `finish_atom`
                // guard above already keeps Quote in the run).
            }
            SyntaxClass::Whitespace | SyntaxClass::EndComment => {}
            _ => {}
        }

        idx += 1;
    }

    if paused {
        debug_assert!(first_syntax.is_none(), "a pause never strands an override");
        return ScanEnd::Paused(LoopState {
            char_pos: from_char + idx,
            byte_pos: chars.byte_pos,
            pps: state,
            atom_start,
            comment_resume: comment_resume_syntax,
        });
    }

    if comment_exit == ParseCommentExit::RangeBoundary {
        state.finalize_incomplete_comment_syntax();
    }
    finish_atom(&mut state, &mut atom_start);

    ScanEnd::Finished(ScanFinish {
        state,
        stop: char_pos_to_lisp_i64(from_char + idx),
    })
}

#[cfg(test)]
#[path = "tests/parse_loop_resume.rs"]
mod resume_tests;
