use super::*;
use crate::buffer::{BufferManager, LispCharPos1};
use crate::emacs_core::display;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_fixnum};
use crate::emacs_core::fontset;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use neomacs_display_protocol::SelectionOwner;

// =========================================================================
// fontset.c gap-fill stubs
// =========================================================================

// =========================================================================
// term.c gap-fill stubs
// =========================================================================

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_tty_frame_at(args: Vec<Value>) -> EvalResult {
    expect_args("tty-frame-at", &args, 2)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_tty_frame_geometry(args: Vec<Value>) -> EvalResult {
    expect_args_range("tty-frame-geometry", &args, 0, 1)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_tty_frame_edges(args: Vec<Value>) -> EvalResult {
    expect_args_range("tty-frame-edges", &args, 0, 2)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_tty_frame_list_z_order(args: Vec<Value>) -> EvalResult {
    expect_args_range("tty-frame-list-z-order", &args, 0, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_tty_frame_restack(args: Vec<Value>) -> EvalResult {
    expect_args_range("tty-frame-restack", &args, 2, 3)?;
    Err(signal(
        "error",
        vec![Value::string("tty-frame-restack is not implemented")],
    ))
}

fn tty_display_dimension(
    ctx: &mut crate::emacs_core::eval::Context,
    name: &str,
    args: &[Value],
) -> Result<(i64, i64), Flow> {
    expect_args_range(name, args, 0, 1)?;

    let frame_id = match args.first().map(|value| value.kind()) {
        Some(ValueKind::Veclike(VecLikeType::Frame)) => {
            crate::window::FrameId(args[0].as_frame_id().unwrap())
        }
        _ => crate::emacs_core::window_cmds::ensure_selected_frame_id(ctx),
    };

    let Some(frame) = ctx.frames.get(frame_id) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![
                Value::symbol("framep"),
                args.first().copied().unwrap_or(Value::NIL),
            ],
        ));
    };

    if frame.initial {
        return Ok((80, 25));
    }

    Ok((i64::from(frame.columns()), i64::from(frame.lines())))
}

pub(crate) fn builtin_tty_display_pixel_width(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (width, _) = tty_display_dimension(ctx, "tty-display-pixel-width", &args)?;
    Ok(Value::fixnum(width))
}

pub(crate) fn builtin_tty_display_pixel_height(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let (_, height) = tty_display_dimension(ctx, "tty-display-pixel-height", &args)?;
    Ok(Value::fixnum(height))
}

// =========================================================================
// neomacsfns.c gap-fill stubs
// =========================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct NeomacsMonitorInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub width_mm: i32,
    pub height_mm: i32,
    pub name: Option<String>,
}

pub fn set_neomacs_monitor_info(monitors: Vec<NeomacsMonitorInfo>) {
    NEOMACS_MONITORS.with(|slot| *slot.borrow_mut() = monitors);
}

pub fn neomacs_monitor_info_snapshot() -> Vec<NeomacsMonitorInfo> {
    NEOMACS_MONITORS.with(|slot| slot.borrow().clone())
}

fn set_cached_clipboard_text(text: Option<String>) {
    NEOMACS_CLIPBOARD_TEXT.with(|slot| *slot.borrow_mut() = text);
}

fn cached_clipboard_text() -> Option<String> {
    NEOMACS_CLIPBOARD_TEXT.with(|slot| slot.borrow().clone())
}

fn set_cached_primary_selection_text(text: Option<String>) {
    NEOMACS_PRIMARY_SELECTION_TEXT.with(|slot| *slot.borrow_mut() = text);
}

fn cached_primary_selection_text() -> Option<String> {
    NEOMACS_PRIMARY_SELECTION_TEXT.with(|slot| slot.borrow().clone())
}

fn monitor_geometry_value(monitor: &NeomacsMonitorInfo) -> Value {
    Value::list(vec![
        Value::fixnum(monitor.x as i64),
        Value::fixnum(monitor.y as i64),
        Value::fixnum(monitor.width as i64),
        Value::fixnum(monitor.height as i64),
    ])
}

fn monitor_mm_size_value(monitor: &NeomacsMonitorInfo) -> Value {
    Value::list(vec![
        Value::fixnum(monitor.width_mm as i64),
        Value::fixnum(monitor.height_mm as i64),
    ])
}

fn monitor_alist_value(monitor: &NeomacsMonitorInfo, frames: Value) -> Value {
    Value::list(vec![
        Value::cons(Value::symbol("geometry"), monitor_geometry_value(monitor)),
        Value::cons(Value::symbol("workarea"), monitor_geometry_value(monitor)),
        Value::cons(Value::symbol("mm-size"), monitor_mm_size_value(monitor)),
        Value::cons(Value::symbol("frames"), frames),
        Value::cons(
            Value::symbol("scale-factor"),
            Value::make_float(monitor.scale),
        ),
        Value::cons(
            Value::symbol("name"),
            monitor
                .name
                .as_deref()
                .map(Value::string)
                .unwrap_or(Value::NIL),
        ),
        Value::cons(Value::symbol("source"), Value::string("Neomacs")),
    ])
}

pub(crate) fn builtin_neomacs_frame_geometry(args: Vec<Value>) -> EvalResult {
    expect_args_range("neomacs-frame-geometry", &args, 0, 1)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_neomacs_frame_edges(args: Vec<Value>) -> EvalResult {
    expect_args_range("neomacs-frame-edges", &args, 0, 2)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_neomacs_mouse_absolute_pixel_position(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-mouse-absolute-pixel-position", &args, 0)?;
    Ok(Value::cons(Value::fixnum(0), Value::fixnum(0)))
}

pub(crate) fn builtin_neomacs_set_mouse_absolute_pixel_position(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-set-mouse-absolute-pixel-position", &args, 2)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_neomacs_display_monitor_attributes_list(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("neomacs-display-monitor-attributes-list", &args, 0, 1)?;
    let frames = eval
        .frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::make_frame(fid.0))
        .collect::<Vec<_>>();
    let monitor_values = neomacs_monitor_info_snapshot();
    if monitor_values.is_empty() {
        return Ok(Value::NIL);
    }

    let mut alists = Vec::with_capacity(monitor_values.len());
    for (index, monitor) in monitor_values.iter().enumerate() {
        let frame_list = if index == 0 {
            Value::list(frames.clone())
        } else {
            Value::NIL
        };
        alists.push(monitor_alist_value(monitor, frame_list));
    }
    Ok(Value::list(alists))
}

pub(crate) fn builtin_neomacs_clipboard_set(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-clipboard-set", &args, 1)?;
    let text = match args[0].kind() {
        ValueKind::Nil => None,
        ValueKind::String => args[0]
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())),
        _ => Some(format!("{}", args[0])),
    };
    if let Some(host) = ctx.display_host.as_mut() {
        host.set_clipboard_text(text.as_deref())
            .map_err(|err| signal("error", vec![Value::string(err)]))?;
    } else {
        set_cached_clipboard_text(text);
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_neomacs_clipboard_get(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-clipboard-get", &args, 0)?;
    let text = if let Some(host) = ctx.display_host.as_mut() {
        host.clipboard_text()
            .map_err(|err| signal("error", vec![Value::string(err)]))?
    } else {
        cached_clipboard_text()
    };
    Ok(text.map(Value::string).unwrap_or(Value::NIL))
}

pub(crate) fn builtin_neomacs_primary_selection_set(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-primary-selection-set", &args, 1)?;
    let text = match args[0].kind() {
        ValueKind::Nil => None,
        ValueKind::String => args[0]
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())),
        _ => Some(format!("{}", args[0])),
    };
    if let Some(host) = ctx.display_host.as_mut() {
        host.set_primary_selection_text(text.as_deref())
            .map_err(|err| signal("error", vec![Value::string(err)]))?;
    } else {
        set_cached_primary_selection_text(text);
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_neomacs_primary_selection_get(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-primary-selection-get", &args, 0)?;
    let text = if let Some(host) = ctx.display_host.as_mut() {
        host.primary_selection_text()
            .map_err(|err| signal("error", vec![Value::string(err)]))?
    } else {
        cached_primary_selection_text()
    };
    Ok(text.map(Value::string).unwrap_or(Value::NIL))
}

pub(crate) fn builtin_neomacs_primary_selection_owner(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("neomacs-primary-selection-owner", &args, 0)?;
    let owner = if let Some(host) = ctx.display_host.as_mut() {
        host.primary_selection_owner()
            .map_err(|err| signal("error", vec![Value::string(err)]))?
    } else if cached_primary_selection_text().is_some() {
        SelectionOwner::ThisProcess
    } else {
        SelectionOwner::None
    };
    let symbol = match owner {
        SelectionOwner::ThisProcess => "this-process",
        SelectionOwner::OtherProcess => "other-process",
        SelectionOwner::None => "none",
        SelectionOwner::Unknown => "unknown",
    };
    Ok(Value::symbol(symbol))
}

pub(crate) fn builtin_neomacs_core_backend(args: Vec<Value>) -> EvalResult {
    expect_args("neomacs-core-backend", &args, 0)?;
    Ok(Value::string("rust"))
}

pub(super) fn reset_stubs_thread_locals() {
    super::super::sqlite::reset_sqlite_thread_locals();
    NEOMACS_CLIPBOARD_TEXT.with(|slot| *slot.borrow_mut() = None);
    NEOMACS_PRIMARY_SELECTION_TEXT.with(|slot| *slot.borrow_mut() = None);
    NEOMACS_MONITORS.with(|slot| slot.borrow_mut().clear());
    super::file_notify::reset_file_notify_thread_locals();
}

thread_local! {
    static NEOMACS_CLIPBOARD_TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
    static NEOMACS_PRIMARY_SELECTION_TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
    static NEOMACS_MONITORS: RefCell<Vec<NeomacsMonitorInfo>> = const { RefCell::new(Vec::new()) };
}

/// Resolve a Lisp window designator to a `WindowId`.
///
/// Mirrors GNU's `decode_any_window` for the new_pixel / new_total
/// / new_normal accessor family. A bare integer is interpreted as a
/// raw window id (matching the long-standing test fixtures), and a
/// real window value is unwrapped via `as_window_id`.
fn window_designator_to_id(value: &Value) -> Option<crate::window::WindowId> {
    if let Some(wid) = value.as_window_id() {
        return Some(crate::window::WindowId(wid));
    }
    match value.kind() {
        ValueKind::Fixnum(id) if id >= 0 => Some(crate::window::WindowId(id as u64)),
        _ => None,
    }
}

pub(super) fn window_new_normal_value(
    eval: &super::eval::Context,
    window: Option<&Value>,
) -> Value {
    let Some(id) = window.and_then(window_designator_to_id) else {
        return Value::NIL;
    };
    eval.frames.window_new_normal(id)
}

pub(super) fn set_window_new_normal_value(
    eval: &mut super::eval::Context,
    window: &Value,
    value: Value,
) -> Value {
    if let Some(id) = window_designator_to_id(window) {
        eval.frames.set_window_new_normal(id, value);
    }
    value
}

pub(super) fn window_new_pixel_value(eval: &super::eval::Context, window: Option<&Value>) -> Value {
    let Some(id) = window.and_then(window_designator_to_id) else {
        return Value::fixnum(0);
    };
    Value::fixnum(eval.frames.window_new_pixel(id).unwrap_or(0))
}

pub(super) fn set_window_new_pixel_value(
    eval: &mut super::eval::Context,
    window: &Value,
    size: i64,
    add: bool,
) -> Value {
    let Some(id) = window_designator_to_id(window) else {
        return Value::fixnum(size);
    };
    Value::fixnum(eval.frames.set_window_new_pixel(id, size, add))
}

pub(super) fn window_new_total_value(eval: &super::eval::Context, window: Option<&Value>) -> Value {
    let Some(id) = window.and_then(window_designator_to_id) else {
        return Value::fixnum(0);
    };
    Value::fixnum(eval.frames.window_new_total(id).unwrap_or(0))
}

pub(super) fn set_window_new_total_value(
    eval: &mut super::eval::Context,
    window: &Value,
    size: i64,
    add: bool,
) -> Value {
    let Some(id) = window_designator_to_id(window) else {
        return Value::fixnum(size);
    };
    Value::fixnum(eval.frames.set_window_new_total(id, size, add))
}

fn fillarray_character_code_from_value(value: &Value) -> Result<u32, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n)
            if (0..=crate::emacs_core::emacs_char::MAX_CHAR as i64).contains(&n) =>
        {
            Ok(n as u32)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

pub(crate) fn builtin_fillarray(args: Vec<Value>) -> EvalResult {
    const BOOL_VECTOR_SIZE_SLOT: usize = 1;
    const BOOL_VECTOR_BITS_START: usize = 2;

    expect_args("fillarray", &args, 2)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::CharTable) => {
            super::chartable::fill_char_table_from_fillarray(&args[0], args[1])?;
            Ok(args[0])
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let is_bool_vector = super::chartable::is_bool_vector(&args[0]);
            let is_char_table = !is_bool_vector && super::chartable::is_char_table(&args[0]);
            if is_bool_vector {
                let fill_bit = if args[1].is_nil() { 0 } else { 1 };
                let v = args[0].as_vector_data().unwrap();
                let logical_len = match v.get(BOOL_VECTOR_SIZE_SLOT).map(|val| val.kind()) {
                    Some(ValueKind::Fixnum(n)) if n > 0 => n as usize,
                    _ => 0,
                };
                let available_bits = v.len().saturating_sub(BOOL_VECTOR_BITS_START);
                let bit_count = logical_len.min(available_bits);
                let mut vec = v.clone();
                for bit in vec.iter_mut().skip(BOOL_VECTOR_BITS_START).take(bit_count) {
                    *bit = Value::fixnum(fill_bit);
                }
                let _ = args[0].replace_vector_data(vec);
                return Ok(args[0]);
            }
            if is_char_table {
                super::chartable::fill_char_table_from_fillarray(&args[0], args[1])?;
                return Ok(args[0]);
            }
            let fill_len = args[0].as_vector_data().map_or(0, |vec| vec.len());
            let _ = args[0].replace_vector_data(vec![args[1]; fill_len]);
            Ok(args[0])
        }
        ValueKind::String => {
            let fill = fillarray_character_code_from_value(&args[1])?;
            let string = args[0].as_lisp_string().expect("string");
            let len = string.schars();
            let size_byte = string.sbytes();
            if len == 0 {
                return Ok(args[0]);
            }

            let mut fill_bytes = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
            let fill_len = if string.is_multibyte() {
                let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
                let written = crate::emacs_core::emacs_char::char_string(fill, &mut buf);
                fill_bytes[..written].copy_from_slice(&buf[..written]);
                written
            } else {
                fill_bytes[0] = fill as u8;
                1
            };

            let new_size_byte = len.checked_mul(fill_len).ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Attempt to change byte length of a string")],
                )
            })?;
            if new_size_byte != size_byte {
                return Err(signal(
                    "error",
                    vec![Value::string("Attempt to change byte length of a string")],
                ));
            }

            let _ = args[0].with_lisp_string_mut(|lisp_str| {
                lisp_str.mutate_bytes(|bytes| {
                    if fill_len == 1 && len == size_byte {
                        bytes.fill(fill_bytes[0]);
                    } else {
                        for (idx, byte) in bytes.iter_mut().enumerate() {
                            *byte = fill_bytes[idx % fill_len];
                        }
                    }
                });
            });
            Ok(args[0])
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("arrayp"), args[0]],
        )),
    }
}

/// Read the BITS argument (vector of integers or string of rows) into raw row
/// values, mirroring GNU `Faref`: a vector element is its integer value; a
/// string element is its character code (one byte for unibyte rows).
fn fringe_bits_rows(bits: &Value) -> Result<Vec<u32>, Flow> {
    if let Some(data) = bits.as_vector_data() {
        return Ok(data
            .iter()
            .map(|elt| elt.as_fixnum().map(|n| n as u32).unwrap_or(0))
            .collect());
    }
    if let Some(s) = bits.as_lisp_string() {
        if let Some(text) = s.as_utf8_str() {
            return Ok(text.chars().map(|c| c as u32).collect());
        }
        // Unibyte / raw-byte string: each byte is one row value.
        return Ok(s.as_bytes().iter().map(|b| u32::from(*b)).collect());
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("arrayp"), *bits],
    ))
}

/// Parse the ALIGN argument per GNU `Fdefine_fringe_bitmap`. Returns
/// `(align, periodic)` where `periodic` selects the repeat behaviour. ALIGN may
/// be a symbol (`top`/`bottom`/`center`/nil) or a list `(ALIGN PERIODIC)`.
fn parse_fringe_align(
    align: Option<&Value>,
) -> Result<(super::fringe_bitmap::FringeBitmapAlign, bool), Flow> {
    use super::fringe_bitmap::FringeBitmapAlign;
    let Some(align) = align else {
        return Ok((FringeBitmapAlign::Center, false));
    };
    if align.is_nil() {
        return Ok((FringeBitmapAlign::Center, false));
    }
    let (align_sym, periodic) = if align.is_cons() {
        let periodic = {
            let cdr = align.cons_cdr();
            cdr.is_cons() && !cdr.cons_car().is_nil()
        };
        (align.cons_car(), periodic)
    } else {
        (*align, false)
    };
    let align_kind = match align_sym.as_symbol_name() {
        Some("top") => FringeBitmapAlign::Top,
        Some("bottom") => FringeBitmapAlign::Bottom,
        Some("center") => FringeBitmapAlign::Center,
        _ if align_sym.is_nil() => FringeBitmapAlign::Center,
        _ => {
            return Err(signal("error", vec![Value::string("Bad align argument")]));
        }
    };
    Ok((align_kind, periodic))
}

pub(crate) fn builtin_define_fringe_bitmap(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    use super::fringe_bitmap::{FringeBitmap, fit_rows_to_height, parse_bits_rows};
    expect_args_range("define-fringe-bitmap", &args, 2, 5)?;
    let symbols_with_pos_enabled = ctx.symbols_with_pos_enabled;
    let Some(sym) = super::symbols::symbol_id_checked(&args[0], symbols_with_pos_enabled) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };
    if !matches!(
        args[1].kind(),
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::String
    ) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("arrayp"), args[1]],
        ));
    }
    let raw_rows = fringe_bits_rows(&args[1])?;
    let natural_height = raw_rows.len().min(255) as u8;

    // WIDTH: integer 1..=16, default 8. GNU errors if outside the range.
    let width: u8 = match args.get(3) {
        Some(width) if !width.is_nil() => {
            let requested = expect_fixnum(width)?;
            let clamped = requested.clamp(1, 16);
            if clamped != requested {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![*width, Value::string("Width must be from 1 to 16")],
                ));
            }
            clamped as u8
        }
        _ => 8,
    };

    // HEIGHT: integer clamped to 0..=255, default = number of BITS rows.
    let mut height: Option<u8> = match args.get(2) {
        Some(h) if !h.is_nil() => Some(expect_fixnum(h)?.clamp(0, 255) as u8),
        _ => None,
    };

    // ALIGN: symbol or `(ALIGN PERIODIC)`. A periodic bitmap repeats its rows;
    // GNU then forces height to 255 and records the natural length as period.
    let (align, periodic) = parse_fringe_align(args.get(4))?;
    let mut period: u8 = 0;
    if periodic {
        period = natural_height;
        height = Some(255);
    }

    let mut rows = parse_bits_rows(&raw_rows, width);
    let final_height;
    if periodic {
        // Repeat the natural rows to fill 255 rows (single-tile downstream still
        // renders correctly; this keeps the stored data faithful to GNU).
        let target = 255usize;
        if !rows.is_empty() {
            let tile = rows.clone();
            rows.clear();
            while rows.len() < target {
                rows.extend_from_slice(&tile);
            }
            rows.truncate(target);
        }
        final_height = 255u8;
    } else {
        let (fitted, h) = fit_rows_to_height(rows, height);
        rows = fitted;
        final_height = h;
    }

    let existing_index =
        super::symbols::symbol_property_get(ctx, args[0], Value::symbol("fringe"))?
            .1
            .and_then(|v| v.as_fixnum())
            .filter(|n| *n >= 0)
            .map(|n| n as u32);

    let bitmap = FringeBitmap {
        bits: rows,
        height: final_height,
        width,
        period,
        align,
        face: None,
    };
    let already_defined = existing_index.or_else(|| ctx.fringe_bitmaps.index_of(sym));
    let index = ctx.fringe_bitmaps.define(sym, existing_index, bitmap);

    // GNU registers a *newly* named bitmap in `fringe-bitmaps' alongside its
    // `fringe' property (fringe.c:1655), both guarded by the same "not already
    // known" test, so redefining an existing bitmap does not list it twice.
    if already_defined.is_none() {
        let listed = ctx.eval_symbol("fringe-bitmaps").unwrap_or(Value::NIL);
        ctx.assign("fringe-bitmaps", Value::cons(args[0], listed));
    }

    ctx.note_macro_expansion_mutation();
    super::symbols::put_in_obarray_values(
        ctx.obarray_mut(),
        args[0],
        Value::symbol("fringe"),
        Value::fixnum(index as i64),
        symbols_with_pos_enabled,
    )?;

    Ok(args[0])
}

/// `fringe-bitmaps` with SYMBOL removed, as GNU's `Fdelq` leaves it.
fn fringe_bitmaps_without(ctx: &mut crate::emacs_core::eval::Context, symbol: Value) -> Value {
    let mut kept = Vec::new();
    let mut tail = ctx.eval_symbol("fringe-bitmaps").unwrap_or(Value::NIL);
    while tail.is_cons() {
        let entry = tail.cons_car();
        if entry != symbol {
            kept.push(entry);
        }
        tail = tail.cons_cdr();
    }
    Value::list(kept)
}

pub(crate) fn builtin_destroy_fringe_bitmap(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("destroy-fringe-bitmap", &args, 1)?;
    let symbols_with_pos_enabled = ctx.symbols_with_pos_enabled;
    let Some(sym) = super::symbols::symbol_id_checked(&args[0], symbols_with_pos_enabled) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };
    // A standard bitmap keeps its listing and its `fringe' property: GNU only
    // unregisters indices at or above the standard range (fringe.c:1409-1414).
    let user_defined = ctx
        .fringe_bitmaps
        .index_of(sym)
        .is_some_and(|index| index >= super::fringe_bitmap::FIRST_USER_FRINGE_BITMAP_INDEX);
    ctx.fringe_bitmaps.destroy(sym);
    ctx.note_macro_expansion_mutation();
    if user_defined {
        let remaining = fringe_bitmaps_without(ctx, args[0]);
        ctx.assign("fringe-bitmaps", remaining);
        super::symbols::put_in_obarray_values(
            ctx.obarray_mut(),
            args[0],
            Value::symbol("fringe"),
            Value::NIL,
            symbols_with_pos_enabled,
        )?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_display_line_is_continued_p(args: Vec<Value>) -> EvalResult {
    expect_args("display--line-is-continued-p", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_display_update_for_mouse_movement(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("display--update-for-mouse-movement", &args, 3)?;
    let fid = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let x = expect_fixnum(&args[1])?;
    let y = expect_fixnum(&args[2])?;
    eval.note_mouse_move_for_frame(Some(fid), x, y);
    Ok(Value::NIL)
}

pub(crate) fn builtin_external_debugging_output(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("external-debugging-output", &args, 1)?;
    let ch = expect_fixnum(&args[0])?;
    if ch < 0 {
        return Err(signal(
            "error",
            vec![Value::string("Invalid character: f03fffff")],
        ));
    }
    let character = u32::try_from(ch)
        .ok()
        .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
        .ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(format!("Invalid character: {ch:x}"))],
            )
        })?;
    // GNU `printchar_to_stream` first serializes the full Emacs character
    // (`CHAR_STRING`, including byte8/non-Unicode codes), then encodes it with
    // `coding-system-for-write` or `locale-coding-system`.  Keeping this at the
    // stream boundary lets every callable printer target pass real Emacs
    // character codes without collapsing them through Rust Unicode.
    let internal = crate::heap_types::LispString::from_emacs_bytes(character.to_emacs_bytes());
    let coding = {
        let explicit = eval.visible_variable_value_or_nil("coding-system-for-write");
        if explicit.is_nil() {
            eval.visible_variable_value_or_nil("locale-coding-system")
        } else {
            explicit
        }
    };
    let eol_conversion = eval.eol_conversion();
    let bytes = match coding.as_symbol_name() {
        Some(name) => crate::encoding::encode_lisp_string(&internal, name, eol_conversion),
        None => internal.as_bytes().to_vec(),
    };
    if let Some(file) = eval.debugging_output_file.as_mut() {
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|_| file.flush())
            .map_err(|err| {
                signal(
                    LispCondition::FileError,
                    vec![Value::string(err.to_string())],
                )
            })?;
    } else {
        use std::io::Write;
        std::io::stderr()
            .write_all(&bytes)
            .and_then(|_| std::io::stderr().flush())
            .map_err(|err| {
                signal(
                    LispCondition::FileError,
                    vec![Value::string(err.to_string())],
                )
            })?;
    }
    Ok(args[0])
}

pub(crate) fn builtin_internal_labeled_narrow_to_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_internal_labeled_narrow_to_region_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_internal_labeled_narrow_to_region_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal--labeled-narrow-to-region", &args, 3)?;
    let start = super::super::buffer::expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = super::super::buffer::expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let label = args[2];
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_range = super::super::buffer::normalize_narrow_region_in_buffers(
        buffers,
        current_id,
        LispCharPos1::new(start),
        LispCharPos1::new(end),
        args[0],
        args[1],
    )?;
    let _ = buffers.internal_labeled_narrow_to_emacs_byte_range(current_id, byte_range, label);
    Ok(Value::NIL)
}

pub(crate) fn builtin_internal_labeled_widen(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_internal_labeled_widen_in_buffers(&mut eval.buffers, args)
}

pub(crate) fn builtin_internal_labeled_widen_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal--labeled-widen", &args, 1)?;
    let current_id = buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = buffers.internal_labeled_widen(current_id, &args[0]);
    Ok(Value::NIL)
}

pub(crate) fn builtin_internal_obarray_buckets(args: Vec<Value>) -> EvalResult {
    expect_args("internal--obarray-buckets", &args, 1)?;
    let obarray_val = expect_obarray_vector_id(&args[0])?;
    let buckets = super::symbols::obarray_buckets(obarray_val).unwrap_or_default();
    Ok(Value::list(buckets))
}

pub(crate) fn builtin_handle_save_session(args: Vec<Value>) -> EvalResult {
    expect_args("handle-save-session", &args, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_handle_switch_frame(args: Vec<Value>) -> EvalResult {
    expect_args("handle-switch-frame", &args, 1)?;
    let frame = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Frame) => args[0],
        ValueKind::Cons => {
            let pair_car = args[0].cons_car();
            let pair_cdr = args[0].cons_cdr();
            match pair_car.as_symbol_name() {
                Some("switch-frame") => {
                    let cdr = pair_cdr;
                    match cdr.kind() {
                        ValueKind::Cons => cdr.cons_car(),
                        _ => {
                            return Err(signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("framep"), args[0]],
                            ));
                        }
                    }
                }
                _ => {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("framep"), args[0]],
                    ));
                }
            }
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("framep"), args[0]],
            ));
        }
    };
    if !frame.is_frame() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("framep"), frame],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_init_image_library(args: Vec<Value>) -> EvalResult {
    expect_args("init-image-library", &args, 1)?;
    let available = args[0]
        .as_symbol_name()
        .is_some_and(super::super::image::is_supported_image_type);
    Ok(Value::bool_val(available))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_describe_buffer_bindings(args: Vec<Value>) -> EvalResult {
    expect_args_range("describe-buffer-bindings", &args, 1, 3)?;
    if !args[0].is_buffer() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), args[0]],
        ));
    }
    if let Some(prefixes) = args.get(1)
        && !prefixes.is_nil()
        && !(prefixes.is_cons()
            || prefixes.is_vector()
            || prefixes.is_string()
            || prefixes.is_nil())
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *prefixes],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_describe_vector(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("describe-vector", &args, 1, 2)?;
    let is_char_table = super::chartable::is_char_table(&args[0]);
    if !is_char_table && !matches!(args[0].kind(), ValueKind::Veclike(VecLikeType::Vector)) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vector-or-char-table-p"), args[0]],
        ));
    }
    let formatter = args
        .get(1)
        .copied()
        .filter(|value| !value.is_nil())
        .unwrap_or_else(|| Value::symbol("princ"));

    if is_char_table {
        // The collected (key . value) pairs hold fresh range conses and the
        // char-table's values in a Rust Vec while the FORMATTER runs
        // arbitrary Lisp per entry; thread them onto one rooted holder so a
        // formatter that mutates the char-table (or just triggers GC) cannot
        // free later entries mid-loop.
        let entries = describe_vector_char_table_entries(&args[0])?;
        let mut holder = Value::NIL;
        for (key, value) in entries.iter().rev() {
            holder = Value::cons(*key, Value::cons(*value, holder));
        }
        let root_scope = eval.save_specpdl_roots();
        eval.push_specpdl_root(holder);
        let result = (|| -> Result<(), Flow> {
            let mut first = true;
            for (key, value) in entries {
                describe_vector_insert_entry(eval, formatter, key, value, &mut first)?;
            }
            Ok(())
        })();
        eval.restore_specpdl_roots(root_scope);
        result?;
    } else if let Some(items) = args[0].as_vector_data() {
        let mut first = true;
        for (index, value) in items.iter().enumerate() {
            if value.is_nil() {
                continue;
            }
            let key = Value::fixnum(index as i64);
            describe_vector_insert_entry(eval, formatter, key, *value, &mut first)?;
        }
    }

    Ok(Value::NIL)
}

fn describe_vector_insert_entry(
    eval: &mut crate::emacs_core::eval::Context,
    formatter: Value,
    key: Value,
    value: Value,
    first: &mut bool,
) -> EvalResult {
    if *first {
        super::super::buffer::builtin_insert(eval, vec![Value::string("\n")])?;
        *first = false;
    }

    let key_text = describe_vector_key_name(key);
    super::super::buffer::builtin_insert(eval, vec![Value::string(&key_text)])?;

    // GNU keymap.c:describe_vector_princ indents to column 16 with minimum
    // one separating column before calling the element describer.
    let key_width = key_text.chars().count();
    let spaces = if key_width < 16 { 16 - key_width } else { 1 };
    super::super::buffer::builtin_insert(eval, vec![Value::string(" ".repeat(spaces))])?;
    eval.apply(formatter, vec![value])?;
    super::super::buffer::builtin_insert(eval, vec![Value::string("\n")])?;
    Ok(Value::NIL)
}

fn describe_vector_char_table_entries(table: &Value) -> Result<Vec<(Value, Value)>, Flow> {
    let entries = super::chartable::char_table_local_entries(table)?;
    let mut slots = vec![Value::NIL; 256];
    for (key, value) in entries {
        match key.kind() {
            ValueKind::Fixnum(ch) if (0..=255).contains(&ch) => {
                slots[ch as usize] = value;
            }
            ValueKind::Cons => {
                let start = key.cons_car().as_fixnum().unwrap_or(0).clamp(0, 255);
                let end = key
                    .cons_cdr()
                    .as_fixnum()
                    .unwrap_or(start)
                    .clamp(start, 255);
                for ch in start..=end {
                    slots[ch as usize] = value;
                }
            }
            _ => {}
        }
    }

    let mut runs = Vec::new();
    let mut start = 0usize;
    while start < slots.len() {
        if slots[start].is_nil() {
            start += 1;
            continue;
        }
        let value = slots[start];
        let mut end = start;
        while end + 1 < slots.len() && slots[end + 1] == value {
            end += 1;
        }
        let key = if start == end {
            Value::fixnum(start as i64)
        } else {
            Value::cons(Value::fixnum(start as i64), Value::fixnum(end as i64))
        };
        runs.push((key, value));
        start = end + 1;
    }
    Ok(runs)
}

fn describe_vector_key_name(key: Value) -> String {
    if key.is_cons() {
        let start = key.cons_car().as_fixnum().unwrap_or(0);
        let end = key.cons_cdr().as_fixnum().unwrap_or(start);
        format!(
            "{} .. {}",
            describe_vector_char_name(start),
            describe_vector_char_name(end)
        )
    } else {
        describe_vector_char_name(key.as_fixnum().unwrap_or(0))
    }
}

fn describe_vector_char_name(code: i64) -> String {
    match code {
        0 => "C-@".to_string(),
        1..=8 => format!(
            "C-{}",
            char::from_u32((code as u32) + b'a' as u32 - 1).unwrap()
        ),
        9 => "TAB".to_string(),
        10 => "C-j".to_string(),
        11 => "C-k".to_string(),
        12 => "C-l".to_string(),
        13 => "RET".to_string(),
        14..=26 => format!(
            "C-{}",
            char::from_u32((code as u32) + b'a' as u32 - 1).unwrap()
        ),
        27 => "ESC".to_string(),
        28 => "C-\\".to_string(),
        29 => "C-]".to_string(),
        30 => "C-^".to_string(),
        31 => "C-_".to_string(),
        32 => "SPC".to_string(),
        127 => "DEL".to_string(),
        _ => char::from_u32(code as u32)
            .map(|ch| ch.to_string())
            .unwrap_or_else(|| code.to_string()),
    }
}

pub(crate) fn builtin_frame_set_was_invisible(args: Vec<Value>) -> EvalResult {
    expect_args("frame--set-was-invisible", &args, 2)?;
    expect_frame_live_or_nil(&args[0])?;
    Ok(args[1])
}

pub(crate) fn builtin_frame_after_make_frame(args: Vec<Value>) -> EvalResult {
    expect_args("frame-after-make-frame", &args, 2)?;
    expect_frame_live_or_nil(&args[0])?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_ancestor_p(args: Vec<Value>) -> EvalResult {
    expect_args("frame-ancestor-p", &args, 2)?;
    expect_frame_live_or_nil(&args[0])?;
    expect_frame_live_or_nil(&args[1])?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_bottom_divider_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-bottom-divider-width", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_child_frame_border_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-child-frame-border-width", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_focus(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-focus", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_frame_font_cache(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-font-cache", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_frame_fringe_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-fringe-width", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_internal_border_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-internal-border-width", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_frame_or_buffer_changed_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-or-buffer-changed-p", &args, 0, 1)?;
    let Some(symbol) = args.first() else {
        return Ok(Value::T);
    };
    if symbol.is_nil() {
        return Ok(Value::NIL);
    }
    if symbol.as_symbol_name().is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *symbol],
        ));
    }
    Err(signal(LispCondition::VoidVariable, vec![*symbol]))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_parent(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-parent", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_frame_pointer_visible_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-pointer-visible-p", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::T)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_right_divider_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-right-divider-width", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_frame_scroll_bar_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-scroll-bar-height", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_frame_scroll_bar_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-scroll-bar-width", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_window_state_change(args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-window-state-change", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_frame_live_or_nil(frame)?;
    }
    Ok(Value::NIL)
}

// --- frame.c missing builtins ---

/// Eval-dependent variant: defaults to selected frame.
pub(crate) fn builtin_frame_id(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-id", &args, 0, 1)?;
    let fid = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "frame-live-p",
    )?;
    let public_id = if fid.0 >= crate::window::FRAME_ID_BASE {
        fid.0 - crate::window::FRAME_ID_BASE + 1
    } else {
        fid.0
    };
    Ok(Value::fixnum(public_id as i64))
}

/// Eval-dependent variant: defaults to selected frame.
pub(crate) fn builtin_frame_root_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-root-frame", &args, 0, 1)?;
    let fid = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "frame-live-p",
    )?;
    let root = eval.frames.root_frame_id(fid).unwrap_or(fid);
    Ok(Value::make_frame(root.0))
}

/// `(mouse-position-in-root-frame)` — stub, returns nil.
pub(crate) fn builtin_mouse_position_in_root_frame(args: Vec<Value>) -> EvalResult {
    expect_args("mouse-position-in-root-frame", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_gap_position(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gap-position", &args, 0)?;
    let buffer = ctx
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    Ok(Value::fixnum(buffer.gap_position_lisp()))
}

pub(crate) fn builtin_gap_size(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("gap-size", &args, 0)?;
    Ok(Value::fixnum(
        ctx.buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
            .gap_size_lisp(),
    ))
}

/// `(garbage-collect-maybe FACTOR)` -> t if it collected, else nil.
///
/// GNU `Fgarbage_collect_maybe` (alloc.c): "Call `garbage-collect' if enough
/// allocation happened. If FACTOR is a positive number N, it means to run GC
/// if more than 1/Nth of the allocations needed to trigger automatic
/// allocation took place." GNU computes
/// `since_gc = gc_threshold - consing_until_gc` (bytes consed since the last
/// GC) and, when `FACTOR >= 1 && since_gc > gc_threshold / FACTOR`, runs
/// `garbage_collect ()` and returns `t`; otherwise it returns `nil`. FACTOR
/// must be a non-negative fixnum (`CHECK_FIXNAT`).
///
/// Our heap exposes the same quantities: `bytes_since_gc()` is GNU's
/// `since_gc` and `should_collect()` is the FACTOR-of-1 special case
/// (`since_gc >= gc_threshold`). We implement GNU's exact FACTOR-scaled
/// condition and drive the real collector via `gc_collect_exact()`.
pub(crate) fn builtin_garbage_collect_maybe(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("garbage-collect-maybe", &args, 1)?;
    let Some(factor) = args[0].as_fixnum() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), args[0]],
        ));
    };
    if factor < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), Value::fixnum(factor)],
        ));
    }

    let since_gc = eval.tagged_heap.bytes_since_gc();
    let threshold = eval.tagged_heap.gc_threshold();
    if factor >= 1 && since_gc > threshold / (factor as usize) {
        eval.gc_collect_exact();
        Ok(Value::T)
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn builtin_garbage_collect_heapsize(args: Vec<Value>) -> EvalResult {
    expect_args("garbage-collect-heapsize", &args, 0)?;
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_get_unicode_property_internal(args: Vec<Value>) -> EvalResult {
    expect_args("get-unicode-property-internal", &args, 2)?;
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("char-table-p"), args[0]],
    ))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) const FACE_ATTRIBUTES_VECTOR_LEN: usize = 20;

pub(crate) fn builtin_font_get_system_font(args: Vec<Value>) -> EvalResult {
    expect_args("font-get-system-font", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_font_get_system_normal_font(args: Vec<Value>) -> EvalResult {
    expect_args("font-get-system-normal-font", &args, 0)?;
    Ok(Value::NIL)
}

fn expect_characterp_from_int(value: &Value) -> Result<char, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) if n >= 0 => char::from_u32(n as u32).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), *value],
            )
        }),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn unspecified_face_attributes_vector() -> Value {
    Value::vector(vec![
        Value::symbol("unspecified");
        FACE_ATTRIBUTES_VECTOR_LEN
    ])
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_face_attributes_as_vector(args: Vec<Value>) -> EvalResult {
    expect_args("face-attributes-as-vector", &args, 1)?;
    Ok(unspecified_face_attributes_vector())
}

pub(crate) fn builtin_fontset_font(args: Vec<Value>) -> EvalResult {
    expect_args_range("fontset-font", &args, 2, 3)?;
    let ch = expect_characterp_from_int(&args[1])?;
    fontset::fontset_font(
        &args[0],
        ch,
        args.get(2).is_some_and(|value| !value.is_nil()),
    )
}

pub(crate) fn builtin_fontset_info(args: Vec<Value>) -> EvalResult {
    expect_args_range("fontset-info", &args, 1, 2)?;
    Err(signal(
        "error",
        vec![Value::string(
            "Window system is not in use or not initialized",
        )],
    ))
}

pub(crate) fn builtin_fontset_list(args: Vec<Value>) -> EvalResult {
    expect_args("fontset-list", &args, 0)?;
    Ok(super::symbols::fontset_list_value())
}

fn expect_window_live_or_nil(value: &Value) -> Result<(), Flow> {
    if value.is_nil() || value.is_window() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), *value],
        ))
    }
}

pub(super) fn expect_window_valid_or_nil(value: &Value) -> Result<(), Flow> {
    if value.is_nil() || value.is_window() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-valid-p"), *value],
        ))
    }
}

fn expect_frame_live_or_nil(value: &Value) -> Result<(), Flow> {
    if value.is_nil() || value.is_frame() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *value],
        ))
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_window_bottom_divider_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-bottom-divider-width", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

/// `(window-lines-pixel-dimensions &optional WINDOW FIRST LAST BODY INVERSE NO-RESTRICT)`
///
/// GNU `src/window.c::Fwindow_lines_pixel_dimensions` walks the
/// window's display matrix and returns a list of
/// `(width . height)` pairs (one per glyph row) plus the
/// total height. neomacs's display matrix lives in the layout
/// engine, not in `neovm-core`, so this builtin cannot read it
/// directly without going through the renderer round trip.
///
/// Window audit Low 13 in `drafts/window-system-audit.md`:
/// returning `nil` is the GNU-documented "no information
/// available" answer (the same value GNU uses on a TTY frame
/// before any redisplay), so callers that probe with
/// `(or (window-lines-pixel-dimensions ...) ...)` get the
/// expected fallback. Building real glyph-row data requires
/// piping the matrix builder snapshot back into neovm-core,
/// which is part of the cursor audit Finding 11
/// (`display_and_set_cursor` collapse) restructuring.
pub(crate) fn builtin_window_lines_pixel_dimensions(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-lines-pixel-dimensions", &args, 0, 6)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_window_new_normal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-new-normal", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_valid_or_nil(window)?;
    }
    Ok(window_new_normal_value(eval, args.first()))
}

pub(crate) fn builtin_window_new_pixel(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-new-pixel", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_valid_or_nil(window)?;
    }
    Ok(window_new_pixel_value(eval, args.first()))
}

pub(crate) fn builtin_window_new_total(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("window-new-total", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_valid_or_nil(window)?;
    }
    Ok(window_new_total_value(eval, args.first()))
}

pub(crate) fn builtin_window_old_body_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-old-body-pixel-height", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_window_old_body_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-old-body-pixel-width", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_window_old_pixel_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-old-pixel-height", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_valid_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_window_old_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-old-pixel-width", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_valid_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_window_right_divider_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-right-divider-width", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_window_scroll_bar_height(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-scroll-bar-height", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_window_scroll_bar_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-scroll-bar-width", &args, 0, 1)?;
    if let Some(window) = args.first() {
        expect_window_live_or_nil(window)?;
    }
    Ok(Value::fixnum(0))
}

// =========================================================================
// eval.c gap-fill stubs
// =========================================================================

/// GNU eval.c:838 — return SYMBOL's toplevel buffer-local value in BUFFER.
///
/// "Toplevel" means outside any let binding.  This pure stub returns nil;
/// a full implementation needs eval access (buffer manager + dynamic stack)
/// and is dispatched via the eval-backed path in builtins/mod.rs.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_buffer_local_toplevel_value(args: Vec<Value>) -> EvalResult {
    expect_args_range("buffer-local-toplevel-value", &args, 1, 2)?;
    Ok(Value::NIL)
}

/// GNU eval.c:857 — set SYMBOL's toplevel buffer-local value in BUFFER.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_set_buffer_local_toplevel_value(args: Vec<Value>) -> EvalResult {
    expect_args_range("set-buffer-local-toplevel-value", &args, 2, 3)?;
    Ok(args[1])
}

pub(crate) fn builtin_debugger_trap(args: Vec<Value>) -> EvalResult {
    expect_args("debugger-trap", &args, 0)?;
    Ok(Value::NIL)
}

// =========================================================================
// coding.c gap-fill stubs
// =========================================================================

// =========================================================================
// buffer.c gap-fill stubs
// =========================================================================

// =========================================================================
// =========================================================================
// thread.c gap-fill stubs
// =========================================================================

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_thread_buffer_disposition(args: Vec<Value>) -> EvalResult {
    expect_args("thread-buffer-disposition", &args, 1)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_thread_set_buffer_disposition(args: Vec<Value>) -> EvalResult {
    expect_args("thread-set-buffer-disposition", &args, 2)?;
    // Stub: ignore the set
    Ok(Value::NIL)
}

// =========================================================================
// window.c gap-fill stubs
// =========================================================================

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_window_discard_buffer_from_window(args: Vec<Value>) -> EvalResult {
    expect_args_range("window-discard-buffer-from-window", &args, 2, 3)?;
    Ok(Value::NIL)
}

// `window-cursor-info` is implemented in
// `crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs::builtin_window_cursor_info`
// (cursor audit Finding 2). The placeholder that lived here used to
// return `nil` unconditionally.

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_combine_windows(args: Vec<Value>) -> EvalResult {
    expect_args("combine-windows", &args, 2)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_uncombine_window(args: Vec<Value>) -> EvalResult {
    expect_args("uncombine-window", &args, 1)?;
    Ok(Value::NIL)
}

// =========================================================================
// frame.c gap-fill stubs
// =========================================================================

pub(crate) fn builtin_frame_windows_min_size(args: Vec<Value>) -> EvalResult {
    expect_args("frame-windows-min-size", &args, 4)?;
    Ok(Value::fixnum(0))
}

// =========================================================================
// xdisp.c gap-fill stubs
// =========================================================================

pub(crate) fn builtin_remember_mouse_glyph(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("remember-mouse-glyph", &args, 3)?;
    if !args[0].is_nil() && !display::live_frame_designator_p(eval, &args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), args[0]],
        ));
    }
    if !display::display_window_system_symbol_eval(eval, Some(&args[0]))?
        .is_some_and(display::gui_window_system_active_value)
    {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    let _x = expect_fixnum(&args[1])?;
    let _y = expect_fixnum(&args[2])?;
    Ok(Value::NIL)
}

// =========================================================================
// image.c gap-fill stubs
// =========================================================================

// =========================================================================
// font.c gap-fill stubs
// =========================================================================

// =========================================================================
// emacs.c / version.c gap-fill stubs for loadup.el
// =========================================================================

// `emacs-repository-get-version' (lisp/version.el:183) and
// `emacs-repository-get-branch' (:231) used to be stubbed here "for
// loadup.el".  They are not needed: loadup.el loads version.el at :128 and
// first calls them at :429 (DIVERGENCES.md 152).
