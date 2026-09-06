//! Buffer navigation, line operations, and mark/region management builtins.
//!
//! All functions here take `(eval: &mut Context, args: Vec<Value>) -> EvalResult`
//! and are dispatched from `builtins.rs` via `dispatch_builtin`.

use super::error::{EvalResult, Flow, signal};
use super::intern::intern;
use super::syntax::{SyntaxClass, SyntaxTable};
use super::textprop::{buffer_overlay_property_at_byte_pos, lookup_buffer_text_property};
use super::value::{Value, ValueKind, VecLikeType, lexenv_lookup};
use crate::buffer::{
    AccessibleEmacsByteRange, BufferManager, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_fixnum, expect_max_args, expect_min_args};
use malachite::integer::Integer;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// `inhibit-point-motion-hooks` id, interned once — this check runs per
/// point-motion primitive and the by-name lookup re-hashed the string
/// every time.
#[inline(always)]
fn inhibit_point_motion_hooks_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("inhibit-point-motion-hooks"))
}

// ---------------------------------------------------------------------------
// Argument helpers (duplicated from builtins.rs — they are not `pub`)
// ---------------------------------------------------------------------------

// GNU validates the COUNT argument of these motion commands with
// `CHECK_FIXNUM` (see `move_point` and `Fbeginning_of_line`/`Fend_of_line`
// in src/cmds.c), which signals `(wrong-type-argument fixnump …)` — a fixnum
// check, not a position/marker check. Markers and bignums are rejected.

#[derive(Clone, Copy)]
struct LineCountArg {
    original: Value,
    count: i64,
    excessive: bool,
}

fn bignum_line_count(value: &Value) -> i64 {
    let n = value.as_bignum().expect("bignum kind");
    if n >= &Integer::from(0) {
        Value::MOST_POSITIVE_FIXNUM
    } else {
        Value::MOST_NEGATIVE_FIXNUM
    }
}

fn line_count_arg(value: &Value) -> Result<LineCountArg, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(LineCountArg {
            original: *value,
            count: n,
            excessive: false,
        }),
        ValueKind::Veclike(VecLikeType::Bignum) => Ok(LineCountArg {
            original: *value,
            count: bignum_line_count(value),
            excessive: true,
        }),
        // GNU `bol`/`eol` (editfns.c) run the N (line-count) argument through
        // `CHECK_INTEGER`, which signals `integerp` — not `integer-or-marker-p`.
        // N is a line count, never a buffer position, so markers are not valid.
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

fn optional_line_count_arg(args: &[Value], default: i64) -> Result<LineCountArg, Flow> {
    if args.is_empty() || args[0].is_nil() {
        Ok(LineCountArg {
            original: Value::fixnum(default),
            count: default,
            excessive: false,
        })
    } else {
        line_count_arg(&args[0])
    }
}

pub(crate) fn line_beginning_scan_count_arg(args: &[Value]) -> Result<i64, Flow> {
    if args.is_empty() || args[0].is_nil() {
        Ok(0)
    } else {
        Ok(line_count_arg(&args[0])?.count.saturating_sub(1))
    }
}

pub(crate) fn line_end_scan_count_arg(args: &[Value]) -> Result<i64, Flow> {
    if args.is_empty() || args[0].is_nil() {
        Ok(1)
    } else {
        Ok(line_count_arg(&args[0])?.count)
    }
}

fn line_count_result(arg: LineCountArg, shortage: i64) -> Value {
    if !arg.excessive {
        return Value::make_int(shortage);
    }

    let adjustment = shortage - arg.count;
    if let Some(big) = arg.original.as_bignum() {
        return Value::make_integer(big.clone() + Integer::from(adjustment));
    }
    Value::make_int(shortage)
}

/// Get a no-current-buffer signal flow.
fn no_buffer() -> Flow {
    signal("error", vec![Value::string("No current buffer")])
}

fn current_buffer_in_manager(buffers: &BufferManager) -> Result<&crate::buffer::Buffer, Flow> {
    buffers.current_buffer().ok_or_else(no_buffer)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn dynamic_or_global_symbol_value(eval: &super::eval::Context, name: &str) -> Option<Value> {
    let name_id = intern(name);
    if eval.lexical_binding()
        && !eval.obarray.is_special(name)
        && let Some(v) = lexenv_lookup(eval.lexenv, name_id)
    {
        return Some(v);
    }

    if let Some(buf) = eval.buffers.current_buffer()
        && let Some(v) = buf.get_buffer_local(name)
    {
        return Some(v);
    }

    eval.obarray.symbol_value(name).cloned()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert a 1-based Emacs char position to a 0-based byte position in the
/// current buffer.  Clamps to valid range.
fn char_pos_to_byte(buf: &crate::buffer::Buffer, pos: LispCharPos1) -> EmacsBytePos {
    buf.lisp_pos_to_emacs_byte_pos(pos)
}

/// Convert a 0-based byte position to a 1-based Emacs char position.
fn byte_to_char_pos(buf: &crate::buffer::Buffer, byte_pos: EmacsBytePos) -> i64 {
    buf.emacs_byte_pos_to_lisp_char_pos(byte_pos).as_i64()
}

fn skip_chars_limit_byte(
    buffers: &BufferManager,
    buf: &crate::buffer::Buffer,
    value: Option<&Value>,
    default: EmacsBytePos,
) -> Result<EmacsBytePos, Flow> {
    let Some(value) = value else {
        return Ok(default);
    };
    if value.is_nil() {
        return Ok(default);
    }
    let pos = super::position::fix_position_with_buffers(buffers, value)?;
    Ok(char_pos_to_byte(buf, LispCharPos1::new(pos)))
}

fn clamp_byte_to_accessible(buf: &crate::buffer::Buffer, byte_pos: EmacsBytePos) -> EmacsBytePos {
    buf.accessible_emacs_byte_region().clamp(byte_pos)
}

/// Count newlines in the Emacs-byte range [start, end).
fn count_newlines(buf: &crate::buffer::Buffer, start: EmacsBytePos, end: EmacsBytePos) -> usize {
    buf.count_newlines_emacs_byte(start, end.max(start))
}

/// Like `move_by_lines` but confined to the narrowed region `[begv, zv)`.
/// Newlines are found by scanning the buffer in place (no whole-buffer copy),
/// mirroring GNU `scan_newline` (search.c).
fn move_by_lines_narrowed(
    buf: &crate::buffer::Buffer,
    byte_pos: usize,
    n: i64,
    begv: usize,
    zv: usize,
) -> (usize, i64) {
    let mut pos = byte_pos.clamp(begv, zv);
    let mut moved: i64 = 0;
    if n >= 0 {
        if n == 0 {
            return (line_beginning_byte_narrowed(buf, pos, begv), 0);
        }
        for _ in 0..n {
            match buf.next_newline_emacs_byte(EmacsBytePos::new(pos), EmacsBytePos::new(zv)) {
                Some(nl) => {
                    pos = nl.get() + 1;
                    moved += 1;
                }
                None => {
                    pos = zv;
                    break;
                }
            }
        }
    } else {
        for _ in 0..(-n) {
            let bol = line_beginning_byte_narrowed(buf, pos, begv);
            if bol <= begv {
                pos = begv;
                break;
            }
            pos = line_beginning_byte_narrowed(buf, bol - 1, begv);
            moved -= 1;
        }
    }
    (pos, moved)
}

/// Find the beginning of the line containing `byte_pos`, but not before `begv`.
fn line_beginning_byte_narrowed(
    buf: &crate::buffer::Buffer,
    byte_pos: usize,
    begv: usize,
) -> usize {
    match buf.prev_newline_emacs_byte(EmacsBytePos::new(byte_pos), EmacsBytePos::new(begv)) {
        Some(nl) => nl.get() + 1,
        None => begv,
    }
}

/// Find the end of the line containing `byte_pos`, but not past `zv`.
fn line_end_byte_narrowed(buf: &crate::buffer::Buffer, byte_pos: usize, zv: usize) -> usize {
    buf.next_newline_emacs_byte(EmacsBytePos::new(byte_pos), EmacsBytePos::new(zv))
        .map(EmacsBytePos::get)
        .unwrap_or(zv)
}

// ===========================================================================
// Point motion hooks and intangible support
// ===========================================================================

pub(crate) fn check_point_motion_hooks(
    eval: &mut super::eval::Context,
    old_byte: EmacsBytePos,
    new_byte: EmacsBytePos,
) -> Result<(), Flow> {
    if old_byte == new_byte {
        return Ok(());
    }
    let inhibit = eval
        .obarray
        .symbol_value_id(inhibit_point_motion_hooks_sym())
        .cloned()
        .unwrap_or(Value::NIL);
    if inhibit.is_truthy() {
        return Ok(());
    }
    let current_id = match eval.buffers.current_buffer_id() {
        Some(id) => id,
        None => return Ok(()),
    };
    let (old_lisp, new_lisp, leave_before, leave_after, enter_before, enter_after) = {
        let buf = match eval.buffers.get(current_id) {
            Some(b) => b,
            None => return Ok(()),
        };
        let ol = byte_to_char_pos(buf, old_byte);
        let nl = byte_to_char_pos(buf, new_byte);
        let leave_before = point_motion_property(
            &eval.obarray,
            &eval.buffers,
            buf,
            old_byte,
            false,
            "point-left",
        );
        let leave_after = point_motion_property(
            &eval.obarray,
            &eval.buffers,
            buf,
            old_byte,
            true,
            "point-left",
        );
        let enter_before = point_motion_property(
            &eval.obarray,
            &eval.buffers,
            buf,
            new_byte,
            false,
            "point-entered",
        );
        let enter_after = point_motion_property(
            &eval.obarray,
            &eval.buffers,
            buf,
            new_byte,
            true,
            "point-entered",
        );
        (ol, nl, leave_before, leave_after, enter_before, enter_after)
    };

    if leave_before != enter_before && leave_before.is_truthy() {
        eval.apply(
            leave_before,
            vec![Value::fixnum(old_lisp), Value::fixnum(new_lisp)],
        )?;
    }
    if leave_after != enter_after && leave_after.is_truthy() {
        eval.apply(
            leave_after,
            vec![Value::fixnum(old_lisp), Value::fixnum(new_lisp)],
        )?;
    }
    if enter_before != leave_before && enter_before.is_truthy() {
        eval.apply(
            enter_before,
            vec![Value::fixnum(old_lisp), Value::fixnum(new_lisp)],
        )?;
    }
    if enter_after != leave_after && enter_after.is_truthy() {
        eval.apply(
            enter_after,
            vec![Value::fixnum(old_lisp), Value::fixnum(new_lisp)],
        )?;
    }
    Ok(())
}

fn point_motion_property(
    obarray: &super::symbol::Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::Buffer,
    point_byte: EmacsBytePos,
    after_point: bool,
    property: &str,
) -> Value {
    let accessible = buf.accessible_emacs_byte_region();
    if after_point {
        if point_byte >= accessible.end() {
            return Value::NIL;
        }
        lookup_buffer_text_property(
            obarray,
            buffers,
            buf,
            point_byte.get(),
            Value::symbol(property),
        )
    } else {
        if point_byte <= accessible.start() {
            return Value::NIL;
        }
        lookup_buffer_text_property(
            obarray,
            buffers,
            buf,
            point_byte.saturating_sub_len(EmacsByteLen::new(1)).get(),
            Value::symbol(property),
        )
    }
}

fn lookup_buffer_char_property(
    obarray: &super::symbol::Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::Buffer,
    byte_pos: usize,
    prop: Value,
) -> Value {
    if byte_pos >= buf.total_emacs_byte_len().get() {
        return Value::NIL;
    }
    if let Some((value, _overlay)) =
        buffer_overlay_property_at_byte_pos(obarray, buffers, buf, byte_pos, prop, None)
    {
        return value;
    }
    lookup_buffer_text_property(obarray, buffers, buf, byte_pos, prop)
}

fn next_char_property_change(
    buf: &crate::buffer::Buffer,
    byte_pos: EmacsBytePos,
) -> Option<EmacsBytePos> {
    let accessible = buf.accessible_emacs_byte_region();
    let text_next = buf
        .text_props_next_change_after_emacs_byte_pos(byte_pos)
        .filter(|next| *next <= accessible.end());
    let overlay_next = buf
        .overlays
        .next_boundary_after_until_emacs_byte_pos(byte_pos, accessible.end());
    match (text_next, overlay_next) {
        (Some(text), Some(overlay)) => Some(text.min(overlay)),
        (Some(text), None) => Some(text),
        (None, Some(overlay)) => Some(overlay),
        (None, None) => None,
    }
}

fn previous_char_property_change(
    buf: &crate::buffer::Buffer,
    byte_pos: EmacsBytePos,
) -> Option<EmacsBytePos> {
    let accessible = buf.accessible_emacs_byte_region();
    let text_prev = buf
        .text_props_previous_change_before_emacs_byte_pos(byte_pos)
        .filter(|prev| *prev >= accessible.start());
    let overlay_prev = buf
        .overlays
        .previous_boundary_before_since_emacs_byte_pos(byte_pos, accessible.start());
    match (text_prev, overlay_prev) {
        (Some(text), Some(overlay)) => Some(text.max(overlay)),
        (Some(text), None) => Some(text),
        (None, Some(overlay)) => Some(overlay),
        (None, None) => None,
    }
}

pub(crate) fn adjust_for_intangible(
    eval: &super::eval::Context,
    pos: EmacsBytePos,
    direction: i32,
) -> EmacsBytePos {
    let inhibit = eval
        .obarray
        .symbol_value_id(inhibit_point_motion_hooks_sym())
        .cloned()
        .unwrap_or(Value::NIL);
    if inhibit.is_truthy() {
        return pos;
    }
    let current_id = match eval.buffers.current_buffer_id() {
        Some(id) => id,
        None => return pos,
    };
    let buf = match eval.buffers.get(current_id) {
        Some(b) => b,
        None => return pos,
    };
    let accessible = buf.accessible_emacs_byte_region();
    let intangible = lookup_buffer_char_property(
        &eval.obarray,
        &eval.buffers,
        buf,
        pos.get(),
        Value::symbol("intangible"),
    );
    if !intangible.is_truthy() {
        return pos;
    }
    let mut cursor = pos;
    if direction >= 0 {
        loop {
            match next_char_property_change(buf, cursor) {
                Some(next) => {
                    let prop = lookup_buffer_char_property(
                        &eval.obarray,
                        &eval.buffers,
                        buf,
                        next.get(),
                        Value::symbol("intangible"),
                    );
                    cursor = next;
                    if prop != intangible {
                        break;
                    }
                }
                None => {
                    cursor = accessible.end();
                    break;
                }
            }
        }
    } else {
        loop {
            match previous_char_property_change(buf, cursor) {
                Some(prev) => {
                    let check = prev.saturating_sub_len(EmacsByteLen::new(1));
                    let prop = lookup_buffer_char_property(
                        &eval.obarray,
                        &eval.buffers,
                        buf,
                        check.get(),
                        Value::symbol("intangible"),
                    );
                    cursor = prev;
                    if prop != intangible {
                        break;
                    }
                }
                None => {
                    cursor = accessible.start();
                    break;
                }
            }
        }
    }
    cursor
}

// ===========================================================================
// Position predicates
// ===========================================================================

/// (bobp) -- at beginning of buffer?
pub(crate) fn builtin_bobp(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("bobp", &args, 0)?;
    let buf = current_buffer_in_manager(&ctx.buffers)?;
    Ok(Value::bool_val(
        buf.point_emacs_byte_pos() == buf.accessible_emacs_byte_region().start(),
    ))
}

/// (eobp) -- at end of buffer?
pub(crate) fn builtin_eobp(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("eobp", &args, 0)?;
    let buf = current_buffer_in_manager(&ctx.buffers)?;
    Ok(Value::bool_val(
        buf.point_emacs_byte_pos() == buf.accessible_emacs_byte_region().end(),
    ))
}

/// (bolp) -- at beginning of line?
pub(crate) fn builtin_bolp(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("bolp", &args, 0)?;
    let buf = current_buffer_in_manager(&ctx.buffers)?;
    let point = buf.point_emacs_byte_pos();
    if point == buf.accessible_emacs_byte_region().start() {
        return Ok(Value::T);
    }
    Ok(Value::bool_val(
        point == EmacsBytePos::ZERO
            || buf.char_code_before_emacs_byte_pos(point) == Some('\n' as u32),
    ))
}

/// (eolp) -- at end of line?
pub(crate) fn builtin_eolp(ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("eolp", &args, 0)?;
    let buf = current_buffer_in_manager(&ctx.buffers)?;
    let point = buf.point_emacs_byte_pos();
    if point == buf.accessible_emacs_byte_region().end() {
        return Ok(Value::T);
    }
    match buf.char_code_after_emacs_byte_pos(point) {
        Some(code) if code == '\n' as u32 => Ok(Value::T),
        _ => Ok(Value::NIL),
    }
}

// ===========================================================================
// Line operations
// ===========================================================================

/// (line-beginning-position &optional N)
/// Compute the unconstrained beginning-of-line position for the current
/// buffer's point after moving `n - 1` lines. Returns `(bol_charpos,
/// orig_charpos, lines_moved)` mirroring GNU's static `bol` helper
/// (editfns.c) plus the original PT used as anchor for field constraint.
pub(crate) fn pos_bol_compute(
    ctx: &super::eval::Context,
    scan_count: i64,
) -> Result<(i64, i64, i64), Flow> {
    let buf = current_buffer_in_manager(&ctx.buffers)?;
    let accessible = buf.accessible_emacs_byte_region();
    let begv = accessible.start().get();
    let zv = accessible.end().get();
    let point = buf.point_emacs_byte_pos();
    let mut pos = point.get();
    let mut moved: i64 = 0;
    if scan_count != 0 {
        let (new_pos, actual_moved) = move_by_lines_narrowed(buf, pos, scan_count, begv, zv);
        pos = new_pos;
        moved = actual_moved;
    }
    // GNU `bol` (editfns.c) asks `scan_newline_from_point` for N - 1 lines.
    // If a forward scan reaches ZV before finding enough newlines, the
    // returned position is ZV itself, not the beginning of the final
    // unterminated line containing ZV.  `delete-line` relies on this via
    // `(pos-bol 2)` to delete the last line of a buffer.
    let bol = if scan_count > 0 && moved != scan_count && pos == zv {
        zv
    } else {
        line_beginning_byte_narrowed(buf, pos, begv)
    };
    Ok((
        byte_to_char_pos(buf, EmacsBytePos::new(bol)),
        byte_to_char_pos(buf, point),
        moved,
    ))
}

/// Compute the unconstrained end-of-line position for the current buffer's
/// point after moving `n - 1` lines. Returns `(eol_charpos, orig_charpos)`,
/// mirroring GNU's static `eol` helper (editfns.c).
pub(crate) fn pos_eol_compute(
    ctx: &super::eval::Context,
    scan_count: i64,
) -> Result<(i64, i64), Flow> {
    let buf = current_buffer_in_manager(&ctx.buffers)?;
    let accessible = buf.accessible_emacs_byte_region();
    let begv = accessible.start().get();
    let zv = accessible.end().get();
    let point = buf.point_emacs_byte_pos();
    let mut pos = point.get();
    let mut moved = 0;
    let delta = scan_count.saturating_sub(1);
    if delta != 0 {
        let (new_pos, actual_moved) = move_by_lines_narrowed(buf, pos, delta, begv, zv);
        pos = new_pos;
        moved = actual_moved;
    }
    let eol = if delta != 0 && moved != delta && pos == begv {
        begv
    } else {
        line_end_byte_narrowed(buf, pos, zv)
    };
    Ok((
        byte_to_char_pos(buf, EmacsBytePos::new(eol)),
        byte_to_char_pos(buf, point),
    ))
}

pub(crate) fn builtin_line_beginning_position(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("line-beginning-position", &args, 1)?;
    let scan_count = line_beginning_scan_count_arg(&args)?;
    let (bol_charpos, orig_charpos, count) = pos_bol_compute(ctx, scan_count)?;
    // GNU `Fline_beginning_position` (editfns.c:700) constrains the result to
    // the current input field. ESCAPE-FROM-EDGE is t when any lines were
    // scanned (count != 0), nil otherwise; ONLY-IN-LINE is always t.
    crate::emacs_core::builtins::builtin_constrain_to_field(
        ctx,
        vec![
            Value::fixnum(bol_charpos),
            Value::fixnum(orig_charpos),
            if count != 0 { Value::T } else { Value::NIL },
            Value::T,
            Value::NIL,
        ],
    )
}

/// (line-end-position &optional N)
pub(crate) fn builtin_line_end_position(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("line-end-position", &args, 1)?;
    let scan_count = line_end_scan_count_arg(&args)?;
    let (eol_charpos, orig_charpos) = pos_eol_compute(ctx, scan_count)?;
    // GNU `Fline_end_position` (editfns.c:755): constrain to current input
    // field with ESCAPE-FROM-EDGE = nil and ONLY-IN-LINE = t.
    crate::emacs_core::builtins::builtin_constrain_to_field(
        ctx,
        vec![
            Value::fixnum(eol_charpos),
            Value::fixnum(orig_charpos),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
}

/// The narrowing policy for `line-number-at-pos` newline counting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineNumberScope {
    Accessible(AccessibleEmacsByteRange),
    Absolute,
}

impl LineNumberScope {
    /// Return the only byte range that may be counted for this scope.
    ///
    /// GNU `Fline_number_at_pos` clips non-absolute positions to `[BEGV, ZV]`
    /// after resolving markers and numeric positions.  Coupling the origin and
    /// endpoint policy prevents a caller from counting from `BEGV` to an
    /// unclipped marker beyond `ZV`.
    fn counting_range(self, position: EmacsBytePos) -> EmacsByteRange {
        match self {
            Self::Accessible(bounds) => EmacsByteRange::new(bounds.start(), bounds.clamp(position)),
            Self::Absolute => EmacsByteRange::new(EmacsBytePos::ZERO, position),
        }
    }
}

/// (line-number-at-pos &optional POS ABSOLUTE)
pub(crate) fn builtin_line_number_at_pos(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let current_buffer_id = buf.id;
    let byte_pos = if args.is_empty() || args[0].is_nil() {
        buf.point_emacs_byte_pos()
    } else {
        match args[0].kind() {
            ValueKind::Veclike(VecLikeType::Marker) => {
                let marker = args[0].as_marker_data().unwrap();
                if marker.buffer == Some(current_buffer_id) {
                    marker
                        .marker_id
                        .and_then(|marker_id| {
                            eval.buffers
                                .marker_emacs_byte_pos(current_buffer_id, marker_id)
                        })
                        .unwrap_or_else(|| {
                            char_pos_to_byte(buf, CharPos0::new(marker.charpos).to_lisp())
                        })
                } else {
                    char_pos_to_byte(buf, CharPos0::new(marker.charpos).to_lisp())
                }
            }
            ValueKind::Fixnum(pos) => {
                let beg = LispCharPos1::ONE.as_i64();
                let z = buf.z_lisp_char_pos().as_i64();
                if pos < beg || pos > z {
                    return Err(signal(
                        LispCondition::ArgsOutOfRange,
                        vec![args[0], Value::fixnum(beg), Value::fixnum(z)],
                    ));
                }
                char_pos_to_byte(buf, LispCharPos1::new(pos))
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("fixnump"), args[0]],
                ));
            }
        }
    };
    let scope = if args.get(1).is_some_and(|v| v.is_truthy()) {
        LineNumberScope::Absolute
    } else {
        LineNumberScope::Accessible(buf.accessible_emacs_byte_region())
    };
    let range = scope.counting_range(byte_pos);
    let line_num = count_newlines(buf, range.start(), range.end()) + 1;
    Ok(Value::fixnum(line_num as i64))
}

/// (count-lines BEG END)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_count_lines(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("count-lines", &args, 2)?;
    expect_max_args("count-lines", &args, 3)?;
    let current_id = eval.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let beg = crate::emacs_core::position::fix_position_with_buffers(&eval.buffers, &args[0])?;
    let end = crate::emacs_core::position::fix_position_with_buffers(&eval.buffers, &args[1])?;
    let range = crate::emacs_core::builtins::normalize_narrow_region_in_buffers(
        &eval.buffers,
        current_id,
        LispCharPos1::new(beg),
        LispCharPos1::new(end),
        args[0],
        args[1],
    )?;
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let s = range.start();
    let e = range.end();
    let mut n = count_newlines(buf, s, e);
    // GNU Emacs: "can be one more if START is not equal to END and the
    // greater of them is not at the start of a line."
    // i.e., if the region is non-empty and the char before `e` is not '\n'.
    if s != e && e.get() > 0 && buf.char_code_before_emacs_byte_pos(e) != Some('\n' as u32) {
        n += 1;
    }
    Ok(Value::fixnum(n as i64))
}

/// (forward-line &optional N) -> integer
pub(crate) fn builtin_forward_line(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::error::expect_args_range("forward-line", &args, 0, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_forward_line_1(eval, arg(0))
}
/// `forward-line` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_forward_line_1(eval: &mut super::eval::Context, n: Value) -> EvalResult {
    let args: [Value; 1] = [n];
    let line_arg = optional_line_count_arg(&args, 1)?;
    let n = line_arg.count;
    let current_id = eval.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let (accessible, pt, new_pos, moved) = {
        let buf = eval.buffers.get(current_id).ok_or_else(no_buffer)?;
        let accessible = buf.accessible_emacs_byte_region();
        let pt = buf.point_emacs_byte_pos();
        let (new_pos, moved) = move_by_lines_narrowed(
            buf,
            pt.get(),
            n,
            accessible.start().get(),
            accessible.end().get(),
        );
        (accessible, pt, new_pos, moved)
    };
    let old_byte = pt;
    let new_pos = EmacsBytePos::new(new_pos);
    let direction = if n >= 0 { 1 } else { -1 };
    let adjusted = adjust_for_intangible(eval, new_pos, direction);
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, adjusted);

    let mut shortage = n - moved;
    if shortage != 0
        && n > 0
        && accessible.start() < accessible.end()
        && new_pos != pt
        && new_pos.get() > 0
    {
        let at_line_start = eval
            .buffers
            .get(current_id)
            .is_some_and(|buf| buf.char_code_before_emacs_byte_pos(new_pos) == Some('\n' as u32));
        if !at_line_start {
            shortage -= 1;
        }
    }
    check_point_motion_hooks(eval, old_byte, adjusted)?;
    Ok(line_count_result(line_arg, shortage))
}

/// (beginning-of-line &optional N)
pub(crate) fn builtin_beginning_of_line(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_fixnum(&args[0])?
    };
    // GNU `Fbeginning_of_line` (cmds.c:148) is literally
    // `SET_PT (XFIXNUM (Fline_beginning_position (n)))`. Delegate to our
    // line-beginning-position builtin so field constraints
    // (`Fconstrain_to_field`) apply uniformly.
    let constrained = builtin_line_beginning_position(eval, vec![Value::fixnum(n)])?;
    let target_char = match constrained.kind() {
        ValueKind::Fixnum(v) => v,
        _ => return Ok(Value::NIL),
    };
    let current_id = eval.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let target_byte = {
        let buf = eval.buffers.get(current_id).ok_or_else(no_buffer)?;
        buf.char_pos_to_emacs_byte_pos_clamped(LispCharPos1::new(target_char).to_char_pos())
    };
    let old_byte = {
        let buf = eval.buffers.get(current_id).ok_or_else(no_buffer)?;
        buf.point_emacs_byte_pos()
    };
    let adjusted = adjust_for_intangible(eval, target_byte, -1);
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, adjusted);
    check_point_motion_hooks(eval, old_byte, adjusted)?;
    Ok(Value::NIL)
}

/// (end-of-line &optional N)
pub(crate) fn builtin_end_of_line(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_fixnum(&args[0])?
    };
    // GNU `Fend_of_line` (cmds.c:172) calls `Fline_end_position` (which
    // applies field constraints) then SET_PTs to that, looping over
    // intangible-then-newline corner cases. Mirror that pattern.
    let current_id = eval.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let old_byte = {
        let buf = eval.buffers.get(current_id).ok_or_else(no_buffer)?;
        buf.point_emacs_byte_pos()
    };
    let constrained = builtin_line_end_position(eval, vec![Value::fixnum(n)])?;
    let target_char = match constrained.kind() {
        ValueKind::Fixnum(v) => v,
        _ => return Ok(Value::NIL),
    };
    let target_byte = {
        let buf = eval.buffers.get(current_id).ok_or_else(no_buffer)?;
        buf.char_pos_to_emacs_byte_pos_clamped(LispCharPos1::new(target_char).to_char_pos())
    };
    let adjusted = adjust_for_intangible(eval, target_byte, 1);
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, adjusted);
    check_point_motion_hooks(eval, old_byte, adjusted)?;
    Ok(Value::NIL)
}

// ===========================================================================
// Character movement
// ===========================================================================

/// (forward-char &optional N)
///
/// Mirrors GNU `Fforward_char` (`src/cmds.c:69`) and `move_point` at
/// `src/cmds.c:36`. The accessible portion of the buffer is bounded by
/// `BEGV` / `ZV` (the narrowing region), not the absolute buffer
/// extents — `forward-char` must clamp to and signal against those
/// fields, otherwise narrowing is silently ignored (audit §7.1).
pub(crate) fn builtin_forward_char(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::error::expect_args_range("forward-char", &args, 0, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_forward_char_1(eval, arg(0))
}
/// `forward-char` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_forward_char_1(eval: &mut super::eval::Context, n: Value) -> EvalResult {
    let args: [Value; 1] = [n];
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_fixnum(&args[0])?
    };
    let current_id = eval.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let (old_byte, cur_char, begv_char, zv_char, new_byte) = {
        let buf = eval.buffers.get(current_id).ok_or_else(no_buffer)?;
        let old_byte = buf.point_emacs_byte_pos();
        let cur_char = buf.point_char_pos().get();
        let begv_char = buf.point_min_char_pos().get();
        let zv_char = buf.point_max_char_pos().get();
        let desired = cur_char as i64 + n;
        let clamped_char = desired.clamp(begv_char as i64, zv_char as i64) as usize;
        (
            old_byte,
            cur_char,
            begv_char,
            zv_char,
            buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(clamped_char)),
        )
    };
    let direction = if n >= 0 { 1 } else { -1 };
    let adjusted = adjust_for_intangible(eval, new_byte, direction);
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, adjusted);
    // GNU `move_point`: signal beginning-of-buffer / end-of-buffer when
    // the requested position falls outside the accessible portion.
    let desired = cur_char as i64 + n;
    if desired < begv_char as i64 {
        return Err(signal(LispCondition::BeginningOfBuffer, vec![]));
    }
    if desired > zv_char as i64 {
        return Err(signal(LispCondition::EndOfBuffer, vec![]));
    }
    check_point_motion_hooks(eval, old_byte, adjusted)?;
    Ok(Value::NIL)
}

/// (backward-char &optional N)
pub(crate) fn builtin_backward_char(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let n = if args.is_empty() || args[0].is_nil() {
        1
    } else {
        expect_fixnum(&args[0])?
    };
    // backward-char N == forward-char (- N)
    builtin_forward_char(eval, vec![Value::fixnum(-n)])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
#[strum(serialize_all = "lowercase")]
enum SkipCharClass {
    Alnum = 1,
    Alpha = 2,
    Word = 3,
    Graph = 4,
    Print = 5,
    Lower = 6,
    Upper = 7,
    Punct = 8,
    Cntrl = 9,
    Digit = 10,
    Xdigit = 11,
    Blank = 12,
    Space = 13,
    Multibyte = 14,
    Nonascii = 15,
    Ascii = 16,
    Unibyte = 17,
}

#[derive(Clone, Debug)]
struct SkipCharsSet {
    negate: bool,
    ranges: Vec<(u32, u32)>,
    classes: Vec<SkipCharClass>,
}

/// Parse GNU ISO C character class syntax used by `skip_chars`.
///
/// Mirrors GNU `re_wctype_parse`: only a leading `[:name:]` token is a class;
/// if the token is closed but the class name is invalid, `skip_chars` signals
/// `Invalid ISO C character class`.  If there is no closing `:]`, the leading
/// `[` is treated as an ordinary character by the caller.
fn parse_skip_char_class(codes: &[u32], i: usize) -> Result<Option<(SkipCharClass, usize)>, Flow> {
    if codes.get(i) != Some(&('[' as u32)) || codes.get(i + 1) != Some(&(':' as u32)) {
        return Ok(None);
    }

    let mut end = i + 2;
    while end + 1 < codes.len() {
        if codes[end] == ':' as u32 && codes[end + 1] == ']' as u32 {
            let name: String = codes[i + 2..end]
                .iter()
                .filter_map(|code| char::from_u32(*code))
                .collect();
            let Ok(class) = name.parse::<SkipCharClass>() else {
                return Err(signal(
                    "error",
                    vec![Value::string("Invalid ISO C character class")],
                ));
            };
            return Ok(Some((class, end + 2)));
        }
        end += 1;
    }

    Ok(None)
}

/// Parse a skip-chars set matching GNU `syntax.c:skip_chars` behavior.
/// Handles `\` as escape character, `-` as range operator, and ISO C
/// character classes such as `[:alpha:]`.
fn parse_skip_chars_set(codes: &[u32]) -> Result<SkipCharsSet, Flow> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut classes: Vec<SkipCharClass> = Vec::new();
    let mut negate = false;
    let mut i = 0;

    if i < codes.len() && codes[i] == '^' as u32 {
        negate = true;
        i += 1;
    }

    while i < codes.len() {
        if let Some((class, next_i)) = parse_skip_char_class(codes, i)? {
            if !classes.contains(&class) {
                classes.push(class);
            }
            i = next_i;
            continue;
        }

        // Handle backslash escape (GNU syntax.c: `\-` = literal `-`)
        let c = if codes[i] == '\\' as u32 && i + 1 < codes.len() {
            i += 1;
            codes[i]
        } else {
            codes[i]
        };
        i += 1;

        // Check for range: c followed by `-` and another char
        if i + 1 < codes.len() && codes[i] == '-' as u32 {
            i += 1; // skip '-'
            let end_c = if codes[i] == '\\' as u32 && i + 1 < codes.len() {
                i += 1;
                codes[i]
            } else {
                codes[i]
            };
            i += 1;
            if c <= end_c {
                ranges.push((c, end_c));
            }
        } else {
            ranges.push((c, c));
        }
    }

    Ok(SkipCharsSet {
        negate,
        ranges,
        classes,
    })
}

fn skip_char_in_explicit_ranges(set: &SkipCharsSet, code: u32) -> bool {
    set.ranges
        .iter()
        .any(|(start, end)| code >= *start && code <= *end)
}

fn skip_char_class_matches(class: SkipCharClass, code: u32, syntax_table: &SyntaxTable) -> bool {
    match class {
        SkipCharClass::Alnum => {
            is_ascii_alpha_code(code) || is_ascii_digit_code(code) || non_ascii_alnum(code)
        }
        SkipCharClass::Alpha => is_ascii_alpha_code(code) || non_ascii_alpha(code),
        SkipCharClass::Blank => {
            code == b' ' as u32 || code == b'\t' as u32 || non_ascii_blank(code)
        }
        SkipCharClass::Cntrl => code < 0x20 || code == 0x7f,
        SkipCharClass::Digit => is_ascii_digit_code(code),
        SkipCharClass::Graph => {
            if code <= 0xff {
                code > b' ' as u32 && !(0x7f..=0xa0).contains(&code)
            } else {
                char::from_u32(code).is_some_and(|ch| !ch.is_control() && !ch.is_whitespace())
            }
        }
        SkipCharClass::Lower => char::from_u32(code).is_some_and(char::is_lowercase),
        SkipCharClass::Print => {
            if code <= 0xff {
                code >= b' ' as u32 && !(0x7f..=0x9f).contains(&code)
            } else {
                char::from_u32(code).is_some_and(|ch| !ch.is_control())
            }
        }
        SkipCharClass::Punct => {
            if code < 0x80 {
                code > b' ' as u32
                    && code < 0x7f
                    && !is_ascii_alpha_code(code)
                    && !is_ascii_digit_code(code)
            } else {
                syntax_table.char_syntax_code(code) != SyntaxClass::Word
            }
        }
        SkipCharClass::Space => syntax_table.char_syntax_code(code) == SyntaxClass::Whitespace,
        SkipCharClass::Upper => char::from_u32(code).is_some_and(char::is_uppercase),
        SkipCharClass::Xdigit => (code as u8).is_ascii_hexdigit() && code <= 0x7f,
        SkipCharClass::Ascii => code < 0x80,
        SkipCharClass::Word => syntax_table.char_syntax_code(code) == SyntaxClass::Word,
        SkipCharClass::Nonascii => code >= 0x80,
        // Neomacs stores raw byte characters as Emacs character codes in the
        // 0x3FFF80..0x3FFFFF range, matching GNU's BYTE8_TO_CHAR space.
        SkipCharClass::Unibyte => code <= 0xff || (0x3f_ff80..=0x3f_ffff).contains(&code),
        SkipCharClass::Multibyte => {
            !skip_char_class_matches(SkipCharClass::Unibyte, code, syntax_table)
        }
    }
}

fn skip_char_matches(set: &SkipCharsSet, code: u32, syntax_table: &SyntaxTable) -> bool {
    let in_set = set
        .classes
        .iter()
        .any(|class| skip_char_class_matches(*class, code, syntax_table))
        || skip_char_in_explicit_ranges(set, code);
    if set.negate { !in_set } else { in_set }
}

fn non_ascii_alpha(code: u32) -> bool {
    code >= 0x80 && char::from_u32(code).is_some_and(char::is_alphabetic)
}

fn is_ascii_alpha_code(code: u32) -> bool {
    code <= 0x7f && (code as u8).is_ascii_alphabetic()
}

fn is_ascii_digit_code(code: u32) -> bool {
    code <= 0x7f && (code as u8).is_ascii_digit()
}

fn non_ascii_alnum(code: u32) -> bool {
    code >= 0x80 && char::from_u32(code).is_some_and(char::is_alphanumeric)
}

fn non_ascii_blank(code: u32) -> bool {
    code >= 0x80
        && char::from_u32(code).is_some_and(|ch| ch.is_whitespace() && ch != '\n' && ch != '\r')
}

/// (skip-chars-forward STRING &optional LIM)
pub(crate) fn builtin_skip_chars_forward(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("skip-chars-forward", &args, 1)?;
    let set_codes = match ctx.lisp_string(args[0]) {
        Some(string) => super::builtins::lisp_string_char_codes(string),
        None => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    let char_set = parse_skip_chars_set(&set_codes)?;
    let current_id = ctx.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let (start_pos, pos, limit, moved_chars) = {
        let buf = ctx.buffers.get(current_id).ok_or_else(no_buffer)?;
        let syntax_table = SyntaxTable::for_buffer(buf);
        let accessible = buf.accessible_emacs_byte_region();
        let lim_byte = skip_chars_limit_byte(&ctx.buffers, buf, args.get(1), accessible.end())?;
        let start_pos = buf.point_emacs_byte_pos();
        let mut pos = start_pos;
        let mut moved_chars = 0_i64;
        let limit = lim_byte.min(accessible.end());

        while pos < limit {
            if let Some(code) = buf.char_code_after_emacs_byte_pos(pos) {
                if !skip_char_matches(&char_set, code, &syntax_table) {
                    break;
                }
                pos = pos.add_len(
                    buf.char_after_emacs_byte_len(pos)
                        .expect("char width should exist at valid point"),
                );
                moved_chars += 1;
            } else {
                break;
            }
        }

        (start_pos, pos, limit, moved_chars)
    };

    debug_assert!(pos >= start_pos || limit <= start_pos);
    let _ = ctx.buffers.goto_buffer_emacs_byte_pos(current_id, pos);
    Ok(Value::fixnum(moved_chars))
}

/// (skip-chars-backward STRING &optional LIM)
pub(crate) fn builtin_skip_chars_backward(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("skip-chars-backward", &args, 1)?;
    let set_codes = match ctx.lisp_string(args[0]) {
        Some(string) => super::builtins::lisp_string_char_codes(string),
        None => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    let char_set = parse_skip_chars_set(&set_codes)?;
    let current_id = ctx.buffers.current_buffer_id().ok_or_else(no_buffer)?;
    let (pos, moved_chars) = {
        let buf = ctx.buffers.get(current_id).ok_or_else(no_buffer)?;
        let syntax_table = SyntaxTable::for_buffer(buf);
        let accessible = buf.accessible_emacs_byte_region();
        let limit = skip_chars_limit_byte(&ctx.buffers, buf, args.get(1), accessible.start())?;
        let start_pos = buf.point_emacs_byte_pos();
        let mut pos = start_pos;
        let mut moved_chars = 0_i64;

        while pos > limit {
            // Find the character before `pos`.
            if let Some(code) = buf.char_code_before_emacs_byte_pos(pos) {
                if !skip_char_matches(&char_set, code, &syntax_table) {
                    break;
                }
                pos = pos.saturating_sub_len(
                    buf.char_before_emacs_byte_len(pos)
                        .expect("char width should exist before valid point"),
                );
                moved_chars -= 1;
            } else {
                break;
            }
        }

        debug_assert!(pos <= start_pos);
        (pos, moved_chars)
    };
    let _ = ctx.buffers.goto_buffer_emacs_byte_pos(current_id, pos);
    Ok(Value::fixnum(moved_chars))
}

// ===========================================================================
// Mark and region
// ===========================================================================

/// (region-beginning) -> integer
pub(crate) fn builtin_region_beginning(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("region-beginning", &args, 0)?;
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let mark = buf.mark_emacs_byte_pos().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(
                "The mark is not set now, so there is no region",
            )],
        )
    })?;
    let pt = clamp_byte_to_accessible(buf, buf.point_emacs_byte_pos());
    let mark = clamp_byte_to_accessible(buf, mark);
    let start = pt.min(mark);
    Ok(Value::fixnum(byte_to_char_pos(buf, start)))
}

/// (region-end) -> integer
pub(crate) fn builtin_region_end(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("region-end", &args, 0)?;
    let buf = eval.buffers.current_buffer().ok_or_else(no_buffer)?;
    let mark = buf.mark_emacs_byte_pos().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(
                "The mark is not set now, so there is no region",
            )],
        )
    })?;
    let pt = clamp_byte_to_accessible(buf, buf.point_emacs_byte_pos());
    let mark = clamp_byte_to_accessible(buf, mark);
    let end = pt.max(mark);
    Ok(Value::fixnum(byte_to_char_pos(buf, end)))
}

// ===========================================================================
// transient-mark-mode
// ===========================================================================
//
// The FUNCTION is not here.  GNU splits this name: the VARIABLE is C
// (DEFVAR_LISP "transient-mark-mode", src/buffer.c:5835) and the COMMAND is
// Lisp -- `(define-minor-mode transient-mark-mode ... :global t :variable
// (default-value 'transient-mark-mode))' at lisp/simple.el:7614.  The Rust
// subr was not a command at all (`commandp' nil, no `interactive-form'), took
// any number of arguments, and never ran `transient-mark-mode-hook'
// (DIVERGENCES.md 152).

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
