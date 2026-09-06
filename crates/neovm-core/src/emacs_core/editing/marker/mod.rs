//! Marker builtins for the Elisp interpreter.
//!
//! Markers track positions in buffers and adjust when text is inserted or
//! deleted before them. GNU Emacs exposes markers as first-class marker
//! objects, not vectors, so NeoVM keeps them as heap objects too.
//!
//! Pure builtins:
//!   `markerp`, `marker-position`, `marker-buffer`,
//!   `marker-insertion-type`, `set-marker-insertion-type`,
//!   `copy-marker`, `make-marker`
//!
//! Eval-dependent builtins:
//!   `set-marker`, `move-marker`, `point-marker`, `point-min-marker`,
//!   `point-max-marker`, `mark-marker`

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::buffer::{BufferId, BufferManager, CharPos0, EmacsBytePos, InsertionType, LispCharPos1};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range};

// ---------------------------------------------------------------------------
// Marker struct (for documentation / internal helpers)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) struct Marker {
    pub buffer: Option<BufferId>,
    pub position: Option<i64>,
    pub insertion_type: bool, // true = advances when text inserted at marker pos
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Marker value helpers
// ---------------------------------------------------------------------------

pub(crate) const MARK_MARKER_ID: u64 = i64::MAX as u64;

pub(crate) fn is_marker(v: &Value) -> bool {
    v.is_marker()
}

pub(crate) fn make_marker_value(
    buffer_id: Option<BufferId>,
    position: Option<LispCharPos1>,
    insertion_type: bool,
) -> Value {
    make_marker_value_with_id(buffer_id, position, insertion_type, None)
}

pub(crate) fn make_marker_value_with_id(
    buffer_id: Option<BufferId>,
    position: Option<LispCharPos1>,
    insertion_type: bool,
    marker_id: Option<u64>,
) -> Value {
    // Seed charpos from the 1-based Lisp position so unregistered markers
    // can preserve a last-position value.  GNU `marker-position` still
    // reports nil unless the marker is attached to a live buffer.
    //
    // Markers that later get registered via register_marker have their
    // charpos/bytepos overwritten through the chain path, so this only
    // matters for unregistered/synthesized markers.
    let charpos = position.map(|p| p.to_char_pos().get()).unwrap_or(0);
    let last_position_valid = matches!(position, Some(p) if p.as_i64() > 0) || buffer_id.is_some();
    Value::make_marker(crate::heap_types::LispMarker {
        buffer: buffer_id,
        insertion_type,
        marker_id,
        bytepos: 0,
        charpos,
        last_position_valid,
        next_marker: std::ptr::null_mut(),
    })
}

pub(crate) fn make_registered_buffer_marker(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    position: LispCharPos1,
    insertion_type: bool,
) -> Value {
    let byte_pos = match buffers.get(buffer_id) {
        Some(buffer) => lisp_pos_to_byte(buffer, position),
        // A marker cannot be registered in a dead buffer.  Return a truly
        // detached marker rather than retaining a stale BufferId alongside a
        // position that marker-position would mistake for a live attachment.
        None => return make_marker_value(None, None, insertion_type),
    };
    let marker = make_marker_value(Some(buffer_id), Some(position), insertion_type);
    let marker_id = buffers.allocate_marker_id();
    set_marker_id(&marker, marker_id);
    // Reuse the just-allocated MarkerObj as the chain node for this
    // buffer; calling `create_marker` here would allocate a *second*
    // MarkerObj with the same marker_id, wasting an allocation and
    // leaving the Lisp-visible Value off-chain.
    if let Some(marker_ptr) = marker
        .as_veclike_ptr()
        .map(|p| p as *mut crate::tagged::header::MarkerObj)
    {
        let _ = buffers.register_marker_id_at_emacs_byte_pos(
            marker_ptr,
            buffer_id,
            marker_id,
            byte_pos,
            if insertion_type {
                InsertionType::After
            } else {
                InsertionType::Before
            },
        );
    }

    marker
}

pub(crate) fn marker_logical_fields(
    v: &Value,
) -> Option<(Option<BufferId>, Option<LispCharPos1>, bool)> {
    if !v.is_marker() {
        return None;
    };
    let data = v.as_marker_data().unwrap();
    // T7: the stale `position` cache is gone. Equality now compares live
    // charpos (+1 for 1-based Lisp shape) when the marker has any
    // position information — either attached to a buffer, or a
    // bufferless fixture marker seeded from
    // `make_marker_value(None, Some(N), _)`. A truly unset marker has
    // `buffer == None && charpos == 0`, which reports `None` and matches
    // GNU's "points nowhere" semantics.
    let position = if data.buffer.is_some() || data.last_position_valid {
        Some(marker_charpos_to_lisp_pos(data.charpos))
    } else {
        None
    };
    Some((data.buffer, position, data.insertion_type))
}

pub(crate) fn marker_equal_logical_fields(v: &Value) -> Option<(Option<BufferId>, EmacsBytePos)> {
    if !v.is_marker() {
        return None;
    };
    let data = v.as_marker_data().unwrap();
    let bytepos = if data.buffer.is_some() {
        EmacsBytePos::new(data.bytepos)
    } else {
        EmacsBytePos::ZERO
    };
    Some((data.buffer, bytepos))
}

/// Tagged-pointer version: compute equal hash key from a marker Value.
pub(crate) fn marker_equal_hash_key_value(v: &Value) -> HashKey {
    if let Some(marker) = v.as_marker_data() {
        let bytepos = if marker.buffer.is_some() {
            EmacsBytePos::new(marker.bytepos)
        } else {
            EmacsBytePos::ZERO
        };
        HashKey::Marker(Box::new((marker.buffer.map(|buffer| buffer.0), bytepos)))
    } else {
        HashKey::Ptr(v.bits())
    }
}

pub(crate) fn marker_id_value(v: &Value) -> Option<u64> {
    if !v.is_marker() {
        return None;
    };
    v.as_marker_data().unwrap().marker_id
}

fn is_mark_marker(v: &Value) -> bool {
    marker_id_value(v) == Some(MARK_MARKER_ID)
}

fn set_marker_id(v: &Value, mid: u64) {
    if v.is_marker() {
        let _ = v.with_marker_data_mut(|data| {
            data.marker_id = Some(mid);
        });
    }
}

fn marker_charpos_to_lisp_pos(charpos: usize) -> LispCharPos1 {
    CharPos0::new(charpos).to_lisp()
}

pub(crate) fn detach_marker_in_buffers(buffers: &mut BufferManager, marker: &Value) {
    if !is_marker(marker) {
        return;
    }
    if let Some(mid) = marker_id_value(marker) {
        buffers.remove_marker(mid);
    }
    let _ = marker.with_marker_data_mut(|data| {
        data.buffer = None;
        // Preserve charpos/bytepos/last_position_valid so
        // `marker-last-position` reports the last attached location
        // (GNU `unchain_marker`, marker.c:684).
        data.next_marker = std::ptr::null_mut();
    });
}

/// Assert that a value is a marker and return a wrong-type-argument error if
/// it is not.
fn expect_marker(_name: &str, v: &Value) -> Result<(), Flow> {
    if is_marker(v) {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("markerp"), *v],
        ))
    }
}

fn marker_position_value(v: &Value) -> Value {
    if !v.is_marker() {
        return Value::NIL;
    };
    let data = v.as_marker_data().unwrap();
    // GNU `Fmarker_position` returns nil for detached markers.  The saved
    // charpos for detached/dead-buffer markers is exposed by
    // `marker-last-position`, not by `marker-position`.
    if data.buffer.is_some() {
        Value::fixnum(marker_charpos_to_lisp_pos(data.charpos).as_i64())
    } else {
        Value::NIL
    }
}

/// Return marker position as an integer.
///
/// Signals `error` when marker is unset, matching Emacs behavior in position
/// contexts that require a concrete marker location.
pub(crate) fn marker_position_as_int(v: &Value) -> Result<i64, Flow> {
    expect_marker("marker-position", v)?;
    match marker_position_value(v).kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            "error",
            vec![Value::string("Marker does not point anywhere")],
        )),
    }
}

pub(crate) fn marker_position_as_int_with_buffers(
    buffers: &BufferManager,
    v: &Value,
) -> Result<i64, Flow> {
    expect_marker("marker-position", v)?;

    // Special mark-marker: resolves via buffer's tracked mark-char.
    // (See buffer.rs mark_char() for how the mark position is stored.)
    if is_mark_marker(v)
        && let Some(buf_id) = marker_buffer_id(v)
        && let Some(buf) = buffers.get(buf_id)
    {
        return match buf.mark_char_pos() {
            Some(char_pos) => Ok(char_pos.to_lisp().as_i64()),
            None => Err(signal(
                "error",
                vec![Value::string("Marker does not point anywhere")],
            )),
        };
    }

    let data = v.as_marker_data().unwrap();
    // GNU position contexts require an attached marker; detached markers
    // still retain a charpos for `marker-last-position`, but they do not
    // point anywhere for `marker-position`.
    if data.buffer.is_some() {
        Ok(marker_charpos_to_lisp_pos(data.charpos).as_i64())
    } else {
        Err(signal(
            "error",
            vec![Value::string("Marker does not point anywhere")],
        ))
    }
}

pub(crate) fn marker_position_as_int_eval(
    eval: &super::eval::Context,
    v: &Value,
) -> Result<i64, Flow> {
    marker_position_as_int_with_buffers(&eval.buffers, v)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn marker_buffer_value(v: &Value) -> Value {
    if !v.is_marker() {
        return Value::NIL;
    };
    match v.as_marker_data().unwrap().buffer {
        Some(buffer_id) => Value::make_buffer(buffer_id),
        None => Value::NIL,
    }
}

fn marker_insertion_type_value(v: &Value) -> Value {
    if !v.is_marker() {
        return Value::NIL;
    };
    Value::bool_val(v.as_marker_data().unwrap().insertion_type)
}

fn marker_buffer_id(v: &Value) -> Option<BufferId> {
    if !v.is_marker() {
        return None;
    };
    v.as_marker_data().unwrap().buffer
}

fn lisp_pos_to_byte(buf: &crate::buffer::Buffer, lisp_pos: LispCharPos1) -> EmacsBytePos {
    // GNU Emacs: set-marker clamps to the full buffer, not the narrowed region.
    buf.lisp_pos_to_full_buffer_emacs_byte_pos(lisp_pos)
}

fn marker_targets_current_mark(marker: &Value) -> bool {
    is_mark_marker(marker)
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (markerp OBJECT) -> t if OBJECT is a marker, nil otherwise
pub(crate) fn builtin_markerp(args: Vec<Value>) -> EvalResult {
    expect_args("markerp", &args, 1)?;
    Ok(Value::bool_val(is_marker(&args[0])))
}

/// Eval-dependent marker-position that reads adjusted positions from the buffer.
pub(crate) fn builtin_marker_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::error::expect_args("marker-position", &args, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_marker_position_1(eval, arg(0))
}
/// `marker-position` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_marker_position_1(
    eval: &mut super::eval::Context,
    marker: Value,
) -> EvalResult {
    let args: [Value; 1] = [marker];
    builtin_marker_position_in_buffers(&eval.buffers, &args)
}

pub(crate) fn builtin_marker_position_in_buffers(
    _buffers: &BufferManager,
    args: &[Value],
) -> EvalResult {
    expect_args("marker-position", &args, 1)?;
    expect_marker("marker-position", &args[0])?;
    Ok(marker_position_value(&args[0]))
}

/// Context-aware marker-buffer that returns nil for killed buffers.
/// GNU returns nil when the marker's buffer has been killed.
pub(crate) fn builtin_marker_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("marker-buffer", &args, 1)?;
    expect_marker("marker-buffer", &args[0])?;
    if let Some(buffer_id) = marker_buffer_id(&args[0])
        && eval.buffers.get(buffer_id).is_some()
    {
        return Ok(Value::make_buffer(buffer_id));
    }
    Ok(Value::NIL)
}

/// Buffer-aware marker-buffer for the VM fast dispatch path.
/// Returns nil for killed buffers (same as the eval-aware version).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_marker_buffer_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("marker-buffer", &args, 1)?;
    expect_marker("marker-buffer", &args[0])?;
    if let Some(buffer_id) = marker_buffer_id(&args[0])
        && buffers.get(buffer_id).is_some()
    {
        return Ok(Value::make_buffer(buffer_id));
    }
    Ok(Value::NIL)
}

/// (marker-insertion-type MARKER) -> t or nil
pub(crate) fn builtin_marker_insertion_type(args: Vec<Value>) -> EvalResult {
    expect_args("marker-insertion-type", &args, 1)?;
    expect_marker("marker-insertion-type", &args[0])?;
    Ok(marker_insertion_type_value(&args[0]))
}

/// Eval-dependent set-marker-insertion-type that also updates the buffer's
/// marker entry so insertion behavior changes immediately.
pub(crate) fn builtin_set_marker_insertion_type(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_set_marker_insertion_type_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_set_marker_insertion_type_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-marker-insertion-type", &args, 2)?;
    expect_marker("set-marker-insertion-type", &args[0])?;
    let new_type = args[1].is_truthy();
    if args[0].is_marker() {
        let _ = args[0].with_marker_data_mut(|data| {
            data.insertion_type = new_type;
        });
    }

    if let Some(mid) = marker_id_value(&args[0]) {
        let ins_type = if new_type {
            InsertionType::After
        } else {
            InsertionType::Before
        };
        buffers.update_marker_insertion_type(mid, ins_type);
    }

    Ok(args[1])
}

/// (make-marker) -> new empty marker (no buffer, no position)
pub(crate) fn builtin_make_marker(args: Vec<Value>) -> EvalResult {
    expect_args("make-marker", &args, 0)?;
    Ok(make_marker_value(None, None, false))
}

/// Eval-dependent copy-marker that registers the new marker in the buffer.
pub(crate) fn builtin_copy_marker(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_copy_marker_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_copy_marker_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    // GNU `Fcopy_marker` (marker.c:753) is `(0, 2)`: with no arguments, it
    // returns a fresh marker that points nowhere; passing nil for MARKER
    // does the same.  TYPE is also optional.
    expect_args_range("copy-marker", &args, 0, 2)?;
    let insertion_type = if args.len() > 1 {
        args[1].is_truthy()
    } else {
        false
    };

    if args.is_empty() || args[0].is_nil() {
        return Ok(make_marker_value(None, None, insertion_type));
    }

    let src = &args[0];
    match src.kind() {
        ValueKind::Veclike(VecLikeType::Marker) | ValueKind::Fixnum(_) => {
            let marker = make_marker_value(None, None, insertion_type);
            let buffer = if src.is_marker() {
                marker_buffer_id(src)
                    .map(Value::make_buffer)
                    .unwrap_or(Value::NIL)
            } else {
                Value::NIL
            };
            builtin_set_marker_in_buffers(buffers, &[marker, *src, buffer])?;
            let _ = marker.with_marker_data_mut(|data| {
                data.insertion_type = insertion_type;
            });
            Ok(marker)
        }
        ValueKind::Nil => Ok(make_marker_value(None, None, insertion_type)),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), args[0]],
        )),
    }
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// (set-marker MARKER POSITION &optional BUFFER) -> MARKER
///
/// Set the position and (optionally) the buffer of MARKER.  If POSITION is
/// nil, the marker is unset (points nowhere).  BUFFER defaults to the current
/// buffer.
pub(crate) fn builtin_set_marker(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    crate::emacs_core::error::expect_args_range("set-marker", &args, 2, 3)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_set_marker_3(eval, arg(0), arg(1), arg(2))
}
/// `set-marker` as registered: fixed arity 3, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a3` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_set_marker_3(
    eval: &mut super::eval::Context,
    marker: Value,
    position: Value,
    buffer: Value,
) -> EvalResult {
    let args: [Value; 3] = [marker, position, buffer];
    builtin_set_marker_in_buffers(&mut eval.buffers, &args)
}

pub(crate) fn builtin_set_marker_in_buffers(
    buffers: &mut BufferManager,
    args: &[Value],
) -> EvalResult {
    expect_args_range("set-marker", &args, 2, 3)?;
    expect_marker("set-marker", &args[0])?;

    let targets_current_mark = marker_targets_current_mark(&args[0]);

    let buffer_id: Option<BufferId> = if args.len() > 2 && args[2].is_truthy() {
        match args[2].kind() {
            ValueKind::Veclike(VecLikeType::Buffer) => args[2]
                .as_buffer_id()
                .and_then(|id| buffers.get(id).map(|_| id)),
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("bufferp"), args[2]],
                ));
            }
        }
    } else {
        // Default to current buffer
        buffers.current_buffer().map(|b| b.id)
    };

    // Resolve position
    let position: Option<LispCharPos1> = match args[1].kind() {
        ValueKind::Nil => None,
        ValueKind::Fixnum(n) => Some(LispCharPos1::new(n)),
        ValueKind::Veclike(VecLikeType::Marker) => {
            marker_position_as_int_with_buffers(buffers, &args[1])
                .ok()
                .map(LispCharPos1::new)
        }
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integer-or-marker-p"), args[1]],
            ));
        }
    };

    // GNU Emacs: when position is nil, the marker is detached from its buffer.
    let buffer_id = position.and(buffer_id);

    // Clamp position to the full buffer range (1 .. total_chars+1), matching
    // GNU Emacs which clamps to the whole buffer, ignoring narrowing.
    let position = match (position, buffer_id) {
        (Some(pos), Some(buf_id)) => {
            if let Some(buf) = buffers.get(buf_id) {
                let max_pos = buf.z_lisp_char_pos().as_i64();
                Some(LispCharPos1::new(pos.as_i64().clamp(1, max_pos)))
            } else {
                Some(pos)
            }
        }
        (other, _) => other,
    };

    register_marker_in_buffers(buffers, &args[0], buffer_id, position);

    if args[0].is_marker() {
        let _ = args[0].with_marker_data_mut(|data| {
            data.buffer = buffer_id;
            // GNU `unchain_marker` (marker.c:684) preserves the marker's
            // last attached charpos when it's detached, so
            // `marker-last-position` keeps reporting it.  Stamp
            // `last_position_valid` here whenever a real position is
            // supplied (or buffer is set to a live buffer); never clear
            // charpos/bytepos on detach.
            if position.is_some() || buffer_id.is_some() {
                data.last_position_valid = true;
            }
        });
    }

    if targets_current_mark {
        let target_buf_id = buffer_id.or_else(|| buffers.current_buffer().map(|buf| buf.id));
        if let Some(buf_id) = target_buf_id {
            match position {
                Some(pos) => {
                    if let Some(byte_pos) =
                        buffers.get(buf_id).map(|buf| lisp_pos_to_byte(buf, pos))
                    {
                        let _ = buffers.set_buffer_mark_emacs_byte_pos(buf_id, byte_pos);
                    }
                }
                None => {
                    let _ = buffers.clear_buffer_mark(buf_id);
                }
            }
        }
    }

    Ok(args[0])
}

// `move-marker' is NOT a subr.  GNU has no DEFUN of that name; it is
// `(defalias 'move-marker #'set-marker)' at lisp/subr.el:2280, so
// `symbol-function' answers the SYMBOL `set-marker' and a compiled caller
// emits the Bset_marker opcode.  DIVERGENCES.md 148.

/// Register a Lisp marker in the target buffer's marker list so that
/// insert/delete operations automatically adjust its position.
#[allow(dead_code)]
fn register_marker_in_buffer(
    eval: &mut super::eval::Context,
    marker: &Value,
    buffer_id: Option<BufferId>,
    position: Option<LispCharPos1>,
) {
    register_marker_in_buffers(&mut eval.buffers, marker, buffer_id, position);
}

fn register_marker_in_buffers(
    buffers: &mut BufferManager,
    marker: &Value,
    buffer_id: Option<BufferId>,
    position: Option<LispCharPos1>,
) {
    if is_mark_marker(marker) {
        return;
    }

    // Read insertion type from marker vector
    let insertion_type_val = marker_insertion_type_value(marker);
    let ins_type = if insertion_type_val.is_truthy() {
        crate::buffer::InsertionType::After
    } else {
        crate::buffer::InsertionType::Before
    };

    // Get or assign a marker-id
    let existing_mid = marker_id_value(marker);

    // Remove old registration from all buffers (this also unchains the
    // marker on the old buffer's intrusive chain, clearing
    // LispMarker.buffer/bytepos/charpos).
    if let Some(mid) = existing_mid {
        buffers.remove_marker(mid);
    }

    if let (Some(buf_id), Some(pos)) = (buffer_id, position) {
        let mid = existing_mid.unwrap_or_else(|| buffers.allocate_marker_id());
        set_marker_id(marker, mid);
        // Resolve the MarkerObj pointer from the Lisp-visible Value so
        // register_marker_id can splice it into buf_id's chain.
        let marker_ptr = marker
            .as_veclike_ptr()
            .map(|p| p as *mut crate::tagged::header::MarkerObj);
        // Defensive: the marker's old chain was cleared by `remove_marker`
        // above if it had a previous buffer binding. But Values freshly
        // created via `make_marker_value` (point-marker etc.) don't go
        // through that path — they arrive here with `next_marker == null`
        // by construction of `LispMarker`. If for some reason this marker
        // is still on a chain (e.g. same-buffer re-registration where
        // `existing_mid` matched a chain entry we just removed), the
        // chain_splice_at_head precondition would fire; belt-and-braces
        // unlink from this buffer first.
        if let Some(ptr) = marker_ptr {
            let _ = buffers.unlink_marker_ptr(buf_id, ptr);
        }
        if let (Some(ptr), Some(byte_pos)) = (
            marker_ptr,
            buffers.get(buf_id).map(|buf| lisp_pos_to_byte(buf, pos)),
        ) {
            let _ =
                buffers.register_marker_id_at_emacs_byte_pos(ptr, buf_id, mid, byte_pos, ins_type);
        }
    }
}

/// (point-marker) -> marker at current point
pub(crate) fn builtin_point_marker(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_point_marker_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_point_marker_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("point-marker", &args, 0)?;
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let pos = buf.point_lisp_char_pos();
    let buffer_id = buf.id;
    let marker = make_marker_value(Some(buffer_id), Some(pos), false);
    register_marker_in_buffers(buffers, &marker, Some(buffer_id), Some(pos));
    Ok(marker)
}

/// (point-min-marker) -> marker at point-min
pub(crate) fn builtin_point_min_marker(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_point_min_marker_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_point_min_marker_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("point-min-marker", &args, 0)?;
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let pos = buf.point_min_lisp_char_pos();
    let buffer_id = buf.id;
    let marker = make_marker_value(Some(buffer_id), Some(pos), false);
    register_marker_in_buffers(buffers, &marker, Some(buffer_id), Some(pos));
    Ok(marker)
}

/// (point-max-marker) -> marker at point-max
pub(crate) fn builtin_point_max_marker(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_point_max_marker_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_point_max_marker_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("point-max-marker", &args, 0)?;
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let pos = buf.point_max_lisp_char_pos();
    let buffer_id = buf.id;
    let marker = make_marker_value(Some(buffer_id), Some(pos), false);
    register_marker_in_buffers(buffers, &marker, Some(buffer_id), Some(pos));
    Ok(marker)
}

/// (mark-marker) -> marker at mark, or error if no mark set
pub(crate) fn builtin_mark_marker(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("mark-marker", &args, 0)?;
    let buffer_id = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .id;

    // Return the real marker created by set_mark_emacs_byte_pos, if it exists.
    if let Some(buf) = eval.buffers.get(buffer_id)
        && !buf.mark_marker_ptr.is_null()
    {
        unsafe {
            return Ok(Value::from_veclike_ptr(
                buf.mark_marker_ptr as *const crate::tagged::header::VecLikeHeader,
            ));
        }
    }

    // No mark set — return a detached marker.
    Ok(make_marker_value_with_id(
        None,
        None,
        false,
        Some(MARK_MARKER_ID),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
