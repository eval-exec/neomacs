//! GNU's just-in-time `syntax-propertize` inside `parse-partial-sexp`
//! (U0.7, P3.4 C0d).
//!
//! GNU `scan_sexps_forward` (src/syntax.c:3174) steps with `INC_FROM`, whose
//! `UPDATE_SYNTAX_TABLE_FORWARD` runs `parse_sexp_propertize` (src/syntax.c:480)
//! when the scan enters a position that `syntax-propertize--done` does not
//! cover: with `parse-sexp-lookup-properties` non-nil, entering position P
//! with `done <= P` and `done < ZV` calls `(internal--syntax-propertize (min ZV
//! (1+ P)))` through `safe_calln` (errors are muted, a `throw` is not), signals
//! if the call changed the text or left `done` where it was, and scans on
//! under the syntax table and properties as they now are. Which positions a
//! scan enters, and so where it calls Lisp, is GNU's control flow exactly:
//!
//! * FROM, always (`SETUP_SYNTAX_TABLE`), before any character is read -- even
//!   when FROM = TO;
//! * P + 1 after the character at P has been read (`INC_FROM` reads the
//!   character, then updates), when P + 1 < TO; the rest of that step (a peek
//!   at a two-character construct, the character after an escape) reads text
//!   propertized by the call;
//! * TO itself only when a character inside a comment was consumed by
//!   `forw_comment`, whose own `inc_both` + `UPDATE_SYNTAX_TABLE_FORWARD` has no
//!   `from < end` guard; a comment's opener and its closing character are
//!   consumed by `INC_FROM` and do not reach TO;
//! * the position after a character STOPBEFORE stops at: GNU has already
//!   stepped over it before backing up.
//!
//! The loop pauses at the loop top where the next entry is due (`Suspend`
//! below). An entry in the middle of a step pauses at the step's loop top and
//! resumes with the syntax its character was read with before the call; an
//! entry at the end of a two-character step is the next loop top, or the stop
//! position when the step ended the scan.
//!
//! `NEOVM_PPS_PROPERTIZE=0` restores the old behaviour (never propertize), for
//! attribution only: the default is GNU's.

use super::parse_loop::{Entry, LoopState, LoopTop, ScanEnd, ScanMode, TopAction, run_parse_loop};
use super::*;
use crate::buffer::BufferId;
use std::sync::atomic::{AtomicU8, Ordering};

const KNOB_UNREAD: u8 = 0;
const KNOB_ON: u8 = 1;
const KNOB_OFF: u8 = 2;

static PPS_PROPERTIZE: AtomicU8 = AtomicU8::new(KNOB_UNREAD);

#[cfg(test)]
thread_local! {
    /// Test override of `NEOVM_PPS_PROPERTIZE`.
    pub(crate) static PPS_PROPERTIZE_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Whether `parse-partial-sexp` runs `syntax-propertize` as GNU does
/// (`NEOVM_PPS_PROPERTIZE`, default on; `0`/`off`/`no`/`false` disables it).
#[inline]
pub(crate) fn pps_propertize_on() -> bool {
    #[cfg(test)]
    if let Some(on) = PPS_PROPERTIZE_OVERRIDE.with(Cell::get) {
        return on;
    }
    match PPS_PROPERTIZE.load(Ordering::Relaxed) {
        KNOB_ON => true,
        KNOB_OFF => false,
        _ => read_pps_propertize_knob(),
    }
}

#[cold]
#[inline(never)]
fn read_pps_propertize_knob() -> bool {
    let on = parse_pps_propertize_knob(std::env::var("NEOVM_PPS_PROPERTIZE").ok().as_deref());
    PPS_PROPERTIZE.store(if on { KNOB_ON } else { KNOB_OFF }, Ordering::Relaxed);
    tracing::debug!(on, "NEOVM_PPS_PROPERTIZE read");
    on
}

pub(crate) fn parse_pps_propertize_knob(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("0" | "off" | "no" | "false" | "nil")
    )
}

/// `syntax-propertize--done` when a GNU propertize trigger is live in the
/// current buffer, else `i64::MAX` (no position can trigger): GNU's
/// `parse_sexp_lookup_properties` test of `UPDATE_SYNTAX_TABLE_FORWARD`,
/// `parse_sexp_propertize`'s `done < ZV`, and an `internal--syntax-propertize`
/// to call. A void `done` reads as GNU's initial -1.
fn live_done(eval: &crate::emacs_core::eval::Context) -> i64 {
    if !parse_sexp_lookup_properties_enabled(eval)
        || eval
            .obarray
            .symbol_function_id(internal_syntax_propertize_sym())
            .is_none()
    {
        return i64::MAX;
    }
    let Some(done) = eval
        .builtin_var_value(syntax_propertize_done_sym())
        .unwrap_or(Value::fixnum(-1))
        .as_fixnum()
    else {
        return i64::MAX;
    };
    let Some(buf) = eval.buffers.current_buffer() else {
        return i64::MAX;
    };
    if done < buf.accessible_char_region().end_lisp().as_i64() {
        done
    } else {
        i64::MAX
    }
}

/// Whether a `parse-partial-sexp` to TO may have to run `syntax-propertize`:
/// no position it can enter lies beyond TO. The caller has already found
/// `parse-sexp-lookup-properties` non-nil.
#[inline]
pub(super) fn may_propertize(eval: &crate::emacs_core::eval::Context, to: i64) -> bool {
    if !pps_propertize_on() {
        return false;
    }
    // The common case first: `done' already covers TO (`syntax-ppss' and
    // font-lock propertize before they parse), one variable read.
    let covered = eval
        .builtin_var_value(syntax_propertize_done_sym())
        .unwrap_or(Value::fixnum(-1))
        .as_fixnum()
        .is_none_or(|done| done > to);
    !covered && live_done(eval) <= to
}

/// Where the scan paused for a trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerPause {
    /// Entering the loop top's own position (the previous step ended there).
    AtLoopTop,
    /// Entering the next position, after the loop top's character is read.
    MidStep,
}

/// The loop-top hook: pause where GNU would call `syntax-propertize`, and
/// note whether the last loop top was inside a comment (the TO rule).
struct Suspend {
    /// `syntax-propertize--done` while a trigger is live, else `i64::MAX`.
    done: i64,
    /// TO, as a Lisp position.
    to: i64,
    last_top_in_comment: bool,
    pause: TriggerPause,
}

impl ScanMode for Suspend {
    const ACTIVE: bool = true;
    const OVERRIDE: bool = true;

    fn first_target(&self) -> usize {
        // The loop top at absolute char c is GNU position c + 1. A trigger
        // needs c + 2 >= done; the TO rule needs the last two loop tops.
        let by_done = self.done.saturating_sub(2).max(0) as usize;
        let by_end = self.to.saturating_sub(3).max(0) as usize;
        by_done.min(by_end)
    }

    fn at_loop_top(&mut self, top: LoopTop<'_>) -> TopAction {
        self.last_top_in_comment = top.pps.in_comment.is_some();
        let pos = top.char_pos as i64 + 1;
        if self.done <= pos {
            self.pause = TriggerPause::AtLoopTop;
            return TopAction::Pause;
        }
        if self.done == pos + 1 && pos + 1 < self.to {
            self.pause = TriggerPause::MidStep;
            return TopAction::Pause;
        }
        TopAction::Continue(top.char_pos + 1)
    }
}

/// The syntax the loop reads the character at `at` with, under the current
/// syntax table and properties (the loop's flat ASCII path computes the same
/// entries).
fn syntax_of_char_at(
    buf: &Buffer,
    table: &SyntaxTable,
    props: SyntaxProperties<'_>,
    at: &LoopState,
) -> (SyntaxClass, SyntaxFlags) {
    let ch = ParseBufferChars::at_emacs_byte(buf, at.byte_pos).peek();
    let entry = effective_syntax_entry_for_abs_char(
        buf,
        table,
        ch,
        at.char_pos,
        &SyntaxPropRange::new(props),
    );
    (entry.class, entry.flags)
}

fn current_buffer_or_error(eval: &crate::emacs_core::eval::Context) -> Result<&Buffer, Flow> {
    eval.buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

/// GNU `parse_sexp_propertize` once its trigger condition holds at CHARPOS:
/// the call, then its two checks.
fn run_propertize(
    eval: &mut crate::emacs_core::eval::Context,
    buffer_id: BufferId,
    charpos: i64,
) -> Result<(), Flow> {
    let (zv, modiff) = {
        let buf = current_buffer_or_error(eval)?;
        (
            buf.accessible_char_region().end_lisp().as_i64(),
            buf.chars_modified_tick(),
        )
    };
    tracing::trace!(charpos, "parse-partial-sexp runs syntax-propertize");
    eval.safe_funcall(
        Value::from_sym_id(internal_syntax_propertize_sym()),
        vec![Value::fixnum(zv.min(charpos + 1))],
    )?;
    let buf = current_buffer_or_error(eval)?;
    if buf.id != buffer_id {
        // GNU would go on scanning whatever buffer is now current, at
        // positions of the old one; stop instead.
        return Err(signal(
            "error",
            vec![Value::string(
                "internal--syntax-propertize changed the current buffer",
            )],
        ));
    }
    if buf.chars_modified_tick() != modiff {
        return Err(signal(
            "error",
            vec![Value::string(
                "internal--syntax-propertize modified the buffer!",
            )],
        ));
    }
    let zv = buf.accessible_char_region().end_lisp().as_i64();
    let done = eval
        .builtin_var_value(syntax_propertize_done_sym())
        .unwrap_or(Value::fixnum(-1))
        .as_fixnum();
    if done.is_some_and(|done| done <= charpos && done < zv) {
        return Err(signal(
            "error",
            vec![Value::string(
                "internal--syntax-propertize did not move syntax-propertize--done",
            )],
        ));
    }
    Ok(())
}

/// Enter CHARPOS: propertize if GNU's trigger holds there.
fn propertize_if_due(
    eval: &mut crate::emacs_core::eval::Context,
    buffer_id: BufferId,
    charpos: i64,
) -> Result<(), Flow> {
    if live_done(eval) <= charpos {
        run_propertize(eval, buffer_id, charpos)?;
    }
    Ok(())
}

/// `parse-partial-sexp` of the current buffer from FROM to TO (validated Lisp
/// positions) starting in `state`, calling `syntax-propertize` where GNU does.
/// Returns the finished state and the stop position; the caller moves point.
#[cold]
#[inline(never)]
#[allow(clippy::too_many_arguments)] // parse-partial-sexp's arguments
pub(super) fn parse_partial_sexp_propertizing(
    eval: &mut crate::emacs_core::eval::Context,
    from: i64,
    to: i64,
    target_depth: Option<i64>,
    stop_before: bool,
    state: PartialParseState,
    from_oldstate: bool,
    commentstop: CommentStopMode,
) -> Result<(PartialParseState, i64), Flow> {
    let (buffer_id, from_char, to_char) = {
        let buf = current_buffer_or_error(eval)?;
        let (from_char, to_char) = clamped_parse_range(buf, from, to);
        (buf.id, from_char, to_char)
    };
    // GNU `SETUP_SYNTAX_TABLE (from, 1)`.
    propertize_if_due(eval, buffer_id, from)?;
    let mut entry = Entry::Fresh {
        from_char,
        state,
        from_oldstate,
    };
    let mut mode = Suspend {
        done: i64::MAX,
        to,
        last_top_in_comment: false,
        pause: TriggerPause::AtLoopTop,
    };
    loop {
        mode.done = live_done(eval);
        let (end, first_syntax) = {
            let buf = current_buffer_or_error(eval)?;
            let table = SyntaxTable::for_buffer(buf);
            let honor = parse_sexp_lookup_properties_enabled(eval);
            let props = SyntaxProperties::for_scan(honor, &eval.obarray, &eval.buffers);
            let escape_policy = CommentEndEscapePolicy::for_context(eval);
            let end = run_parse_loop(
                buf,
                &table,
                entry,
                to_char,
                target_depth,
                stop_before,
                commentstop,
                props,
                escape_policy,
                &mut mode,
            );
            // GNU read the paused character before the call it is about to
            // make: keep the syntax it saw.
            let first_syntax = match &end {
                ScanEnd::Paused(at) if mode.pause == TriggerPause::MidStep => {
                    Some(syntax_of_char_at(buf, &table, props, at))
                }
                _ => None,
            };
            (end, first_syntax)
        };
        match end {
            ScanEnd::Paused(at) => {
                let charpos = at.char_pos as i64
                    + match mode.pause {
                        TriggerPause::AtLoopTop => 1,
                        TriggerPause::MidStep => 2,
                    };
                run_propertize(eval, buffer_id, charpos)?;
                entry = Entry::Resume { at, first_syntax };
            }
            ScanEnd::Finished(finish) => {
                // The position the last step entered: checked already unless
                // a two-character step ended the scan (a stop before TO), or a
                // comment character was consumed up to TO.
                let comment_to = finish.stop == to
                    && mode.last_top_in_comment
                    && finish.state.in_comment.is_some();
                if finish.stop < to || comment_to {
                    propertize_if_due(eval, buffer_id, finish.stop)?;
                }
                return Ok((finish.state, finish.stop));
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/pps_propertize_knob.rs"]
mod knob_tests;
