//! Display Lisp entries that stay in the core command layer (stage-4 E7 R0).
//!
//! Frame-id, image-library and mouse-position entries bind window/frame records
//! and the command loop's pointer snapshot.  Under R0 the bodies stay core-side;
//! the window/frame records and the display engine remain core-owned and the
//! evaluator context is reached through the core facade.

use crate::emacs_core::builtins::expect_string_lossy;
use crate::emacs_core::builtins::strings::builtin_string_to_number;
use crate::emacs_core::builtins::{NeomacsMonitorInfo, neomacs_monitor_info_snapshot};
use crate::emacs_core::display;
use crate::emacs_core::error::signal;
use crate::emacs_core::intern::resolve_sym;
use crate::tagged::SymbolMode;
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_value::display_vocab::FrameDomain;
use neovm_value::error::{
    EvalResult, Flow, LispCondition, expect_args, expect_args_range, expect_fixnum,
};
use neovm_value::value::*;

/// Eval-dependent variant: defaults to selected frame.
pub(crate) fn builtin_frame_id(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-id", &args, 0, 1)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = crate::emacs_core::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.first(),
        neovm_value::display_vocab::FrameDomain::Live,
    )?;
    let public_id = if fid.0 >= neovm_value::display_vocab::FRAME_ID_BASE {
        fid.0 - neovm_value::display_vocab::FRAME_ID_BASE + 1
    } else {
        fid.0
    };
    Ok(Value::fixnum(public_id as i64))
}

pub(crate) fn builtin_init_image_library(args: Vec<Value>, symbol_mode: SymbolMode) -> EvalResult {
    expect_args("init-image-library", &args, 1)?;
    let available = args[0]
        .as_symbol_name(symbol_mode)
        .is_some_and(crate::emacs_core::image::is_supported_image_type);
    Ok(Value::bool_val(available))
}

/// `(mouse-position-in-root-frame)` -> (X . Y) or nil
///
/// Mirrors GNU `Fmouse_position_in_root_frame` (`src/frame.c:7010-7039`):
/// take `mouse-position` (after `mouse-position-function`) and add the
/// position of its frame and of every ancestor frame.  On a tty
/// `mouse-drag-line` reads the pointer through this primitive
/// (`lisp/mouse.el:862-871`), so answering nil left mode-line and divider
/// drags unable to resize.
pub(crate) fn builtin_mouse_position_in_root_frame(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mouse-position-in-root-frame", &args, 0)?;
    let pos = builtin_mouse_position(eval, vec![])?;
    if !pos.is_cons() {
        return Ok(Value::NIL);
    }
    let Some(frame_id) = pos.cons_car().as_frame_id() else {
        return Ok(Value::NIL);
    };
    let coords = pos.cons_cdr();
    if !coords.is_cons() {
        // GNU reads the cdr with XCAR/XCDR without validating it; a hook that
        // returns this shape crashes the reference build.  Stay nil instead.
        return Ok(Value::NIL);
    }
    let (Some(mut x), Some(mut y)) = (
        mouse_position_coordinate(coords.cons_car()),
        mouse_position_coordinate(coords.cons_cdr()),
    ) else {
        return Ok(Value::NIL);
    };
    let mut current = Some(neovm_value::display_vocab::FrameId(frame_id));
    let mut seen: Vec<neovm_value::display_vocab::FrameId> = Vec::new();
    while let Some(id) = current {
        if seen.contains(&id) {
            break;
        }
        seen.push(id);
        let Some(frame) = eval.frame_manager().get(id) else {
            // `mouse-position-function` can return a frame that has since
            // been deleted; Neomacs keeps no geometry for it.  GNU would read
            // the dead frame's stale left_pos/top_pos, which cannot be
            // reproduced once the frame record is gone.
            return Ok(Value::NIL);
        };
        x += frame.left_pos;
        y += frame.top_pos;
        let parent = frame.parent_frame;
        let Some(parent_id) = parent
            .as_frame_id()
            .map(neovm_value::display_vocab::FrameId)
        else {
            break;
        };
        if eval.frame_manager().get(parent_id).is_none() {
            // A surviving child can reference a parent that delete-frame
            // removed: Neomacs does not yet soft-delete children the way GNU
            // does (src/frame.c:2645-2664).  The root position cannot be
            // computed without that ancestor's offset.
            return Ok(Value::NIL);
        }
        current = Some(parent_id);
    }
    Ok(Value::cons(Value::fixnum(x), Value::fixnum(y)))
}

/// Decode one X/Y element of `mouse-position`.
///
/// `mouse-position` documents nil X/Y on a mouseless terminal.  The reference
/// GNU build evaluates `XFIXNUM` on that nil as zero, so the normal mouseless
/// `(FRAME nil . nil)` still answers the frame offsets.  Any other non-fixnum
/// has no defined GNU result and is rejected instead of copied.
pub(crate) fn mouse_position_coordinate(value: Value) -> Option<i64> {
    if value.is_nil() {
        Some(0)
    } else {
        value.as_fixnum()
    }
}

pub(crate) fn builtin_mouse_pixel_position(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mouse-pixel-position", &args, 0)?;
    current_mouse_position_value(eval, true)
}

pub(crate) fn builtin_mouse_position(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mouse-position", &args, 0)?;
    current_mouse_position_value(eval, false)
}

fn current_mouse_position_value(
    eval: &mut crate::emacs_core::eval::Context,
    pixel_units: bool,
) -> EvalResult {
    let selected_frame = selected_frame_value(eval);
    let (frame_value, x, y) = match eval.command_loop().keyboard.mouse_pixel_position() {
        Some(state) => {
            let frame_value = state
                .frame_id
                .map(|frame_id| Value::make_frame(frame_id.0))
                .unwrap_or(Value::NIL);
            let (x, y) = if pixel_units {
                (Value::fixnum(state.x), Value::fixnum(state.y))
            } else {
                pixel_to_char_mouse_position(eval, state.frame_id, state.x, state.y)
            };
            (frame_value, x, y)
        }
        None => (selected_frame, Value::NIL, Value::NIL),
    };
    maybe_transform_mouse_position(eval, Value::cons(frame_value, Value::cons(x, y)))
}

fn maybe_transform_mouse_position(
    eval: &mut crate::emacs_core::eval::Context,
    value: Value,
) -> EvalResult {
    let transform = eval
        .builtin_var_value_by_name("mouse-position-function")
        .unwrap_or(Value::NIL);
    if transform.is_truthy() {
        eval.apply(transform, vec![value])
    } else {
        Ok(value)
    }
}

fn selected_frame_value(eval: &mut crate::emacs_core::eval::Context) -> Value {
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(frames, buffers);
    Value::make_frame(fid.0)
}

fn pixel_to_char_mouse_position(
    eval: &crate::emacs_core::eval::Context,
    frame_id: Option<neovm_value::display_vocab::FrameId>,
    x: i64,
    y: i64,
) -> (Value, Value) {
    let Some(frame_id) = frame_id else {
        return (Value::NIL, Value::NIL);
    };
    let Some(frame) = eval.frame_manager().get(frame_id) else {
        return (Value::NIL, Value::NIL);
    };
    let char_width = frame.char_width.max(1.0);
    let char_height = frame.char_height.max(1.0);
    (
        Value::fixnum((x as f32 / char_width).floor() as i64),
        Value::fixnum((y as f32 / char_height).floor() as i64),
    )
}

pub(crate) fn builtin_old_selected_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("old-selected-frame", &args, 0)?;
    crate::emacs_core::frame::builtin_selected_frame(eval, Vec::new())
}

pub(crate) fn builtin_query_fontset(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("query-fontset", &args, 1, 2)?;
    // GNU `Fquery_fontset` calls `check_window_system (NULL)` before touching
    // the pattern (src/fontset.c:1248), so a headless session signals instead
    // of resolving the fontset registry.
    if !crate::emacs_core::display::x_window_system_active(eval) {
        return Err(crate::emacs_core::display::window_system_not_initialized_error());
    }
    let pattern = expect_string_lossy(&args[0])?;
    if pattern.is_empty() {
        return Ok(Value::NIL);
    }
    let regexpp = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(
        crate::emacs_core::fontset::query_fontset_registry(&pattern, regexpp)
            .map_or(Value::NIL, Value::string),
    )
}

pub(crate) fn builtin_reconsider_frame_fonts(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("reconsider-frame-fonts", &args, 1)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let frame_id = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        Some(&args[0]),
        neovm_value::display_vocab::FrameDomain::Live,
    )?;
    if eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.effective_window_system())
        .is_none()
    {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    crate::emacs_core::font::seed_live_frame_default_face_from_font_parameter(eval, frame_id);
    Ok(Value::NIL)
}

/// `(tool-bar-pixel-width &optional FRAME)`; GNU `decode_any_frame`.
///
/// GNU decodes before it answers (`src/frame.c:4439`):
///
///     #ifdef FRAME_TOOLBAR_WIDTH
///       struct frame *f = decode_any_frame (frame);
///       if (FRAME_WINDOW_P (f)) return make_fixnum (FRAME_TOOLBAR_WIDTH (f));
///     #endif
///       return make_fixnum (0);
///
/// so a non-frame signals `framep` and never reaches the constant.  The width
/// is 0 until a side-mounted tool bar exists to measure, but the DECODE is not
/// a placeholder -- it is the whole Lisp-visible contract of the argument, and
/// dropping it is what let `(tool-bar-pixel-width SOME-WINDOW)` answer 0.
///
/// `decode_any_frame`, not the live decoder: GNU's is a bare `CHECK_FRAME`, so
/// a dead frame is accepted here just as `framep` accepts one.
pub(crate) fn builtin_tool_bar_pixel_width(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("tool-bar-pixel-width", &args, 0, 1)?;
    match args.first().filter(|frame| !frame.is_nil()) {
        Some(frame) => {
            let (frames, buffers) = eval.frame_manager_and_buffers_mut();
            crate::emacs_core::window_cmds::resolve_frame_id_in_state(
                frames,
                buffers,
                Some(frame),
                neovm_value::display_vocab::FrameDomain::Any,
            )?;
        }
        None => {
            let (frames, buffers) = eval.frame_manager_and_buffers_mut();
            crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(frames, buffers);
        }
    }
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_xw_display_color_p_ctx(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("xw-display-color-p", &args, 0, 1)?;
    if let Some(display) = args.first() {
        crate::emacs_core::display::expect_display_designator_in_state(
            &ctx.frame_manager(),
            display,
        )?;
    }
    if crate::emacs_core::display::display_window_system_symbol_in_state(
        &ctx.frame_manager(),
        ctx.variables(),
        &[],
        args.first(),
    )?
    .is_some_and(crate::emacs_core::display::gui_window_system_active_value)
    {
        Ok(Value::T)
    } else {
        Ok(Value::NIL)
    }
}

// --- Frame/TTY/fontset Lisp entries moved from the evaluator builtins ---

fn tty_display_dimension(
    ctx: &mut crate::emacs_core::eval::Context,
    name: &str,
    args: &[Value],
) -> Result<(i64, i64), Flow> {
    expect_args_range(name, args, 0, 1)?;

    let frame_id = match args.first().map(|value| value.kind()) {
        Some(ValueKind::Veclike(VecLikeType::Frame)) => {
            neovm_value::display_vocab::FrameId(args[0].as_frame_id().unwrap())
        }
        _ => crate::emacs_core::window_cmds::ensure_selected_frame_id(ctx),
    };

    let Some(frame) = ctx.frame_manager().get(frame_id) else {
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

pub(crate) fn builtin_frame_set_was_invisible(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("frame--set-was-invisible", &args, 2)?;
    expect_live_frame_or_nil(eval, &args[0])?;
    Ok(args[1])
}

pub(crate) fn builtin_frame_after_make_frame(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("frame-after-make-frame", &args, 2)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let frame_id = crate::emacs_core::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.first(),
        FrameDomain::Live,
    )?;
    if args[1].is_truthy() {
        let request = eval.frame_manager().get(frame_id).and_then(|frame| {
            (frame.effective_window_system().is_some()
                && frame.parent_frame.as_frame_id().is_none())
            .then(|| crate::emacs_core::eval::GuiFrameHostRequest::for_frame(frame))
        });
        if let Some(request) = request
            && let Some(host) = eval.display_host.as_mut()
        {
            host.finish_gui_frame_creation(request)
                .map_err(|message| signal("error", vec![Value::string(message)]))?;
        }
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_frame_font_cache(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-font-cache", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        expect_live_frame_or_nil(eval, frame)?;
    }
    Ok(Value::NIL)
}

fn expect_live_frame_or_nil(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> Result<(), Flow> {
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        Some(value),
        FrameDomain::Live,
    )
    .map(|_| ())
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

pub(crate) fn builtin_fontset_font(args: Vec<Value>) -> EvalResult {
    expect_args_range("fontset-font", &args, 2, 3)?;
    let ch = expect_characterp_from_int(&args[1])?;
    crate::emacs_core::fontset::fontset_font(
        &args[0],
        ch,
        args.get(2).is_some_and(|value| !value.is_nil()),
    )
}

pub(crate) fn builtin_lower_frame(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("lower-frame", &args, 0, 1)?;
    if let Some(frame) = args.first() {
        // GNU's `Flower_frame' decodes with `decode_live_frame'
        // (`src/frame.c'), which is `CHECK_LIVE_FRAME' -- a DELETED frame is
        // rejected, not merely a non-frame.  The tag-only check that used to be
        // here returned nil for a deleted frame.
        expect_live_frame_or_nil(eval, frame)?;
    }
    Ok(Value::NIL)
}

// --- monitor and pointer entries moved from the evaluator builtins ---

fn monitor_geometry_value(monitor: &NeomacsMonitorInfo) -> Value {
    // A broken scale factor keeps the old device-pixel values rather than
    // signaling, so init files that read monitor attributes keep loading.
    let rect = display::monitor_rect_in_display_pixels(monitor).unwrap_or([
        monitor.x as i64,
        monitor.y as i64,
        monitor.width as i64,
        monitor.height as i64,
    ]);
    Value::list(rect.map(Value::fixnum).to_vec())
}

fn monitor_mm_size_value(monitor: &NeomacsMonitorInfo) -> Value {
    Value::list(vec![
        Value::fixnum(monitor.width_mm as i64),
        Value::fixnum(monitor.height_mm as i64),
    ])
}

/// Whether monitor alists carry `scale-factor`. GNU adds it only on PGTK,
/// whose geometry is in device pixels (frame.c, `#ifdef HAVE_PGTK`); NS and
/// X builds omit it. Callers such as `org--get-display-dpi` divide the
/// geometry by it when present.
#[derive(Clone, Copy)]
enum MonitorScaleFactor {
    Omit,
    Report,
}

fn monitor_alist_value(
    monitor: &NeomacsMonitorInfo,
    frames: Value,
    scale_factor: MonitorScaleFactor,
) -> Value {
    let mut attributes = vec![
        Value::cons(Value::symbol("geometry"), monitor_geometry_value(monitor)),
        Value::cons(Value::symbol("workarea"), monitor_geometry_value(monitor)),
        Value::cons(Value::symbol("mm-size"), monitor_mm_size_value(monitor)),
    ];
    // GNU's order: mm-size, scale-factor, frames.
    if let MonitorScaleFactor::Report = scale_factor {
        attributes.push(Value::cons(
            Value::symbol("scale-factor"),
            Value::make_float(monitor.scale),
        ));
    }
    attributes.extend([
        Value::cons(Value::symbol("frames"), frames),
        Value::cons(
            Value::symbol("name"),
            monitor
                .name
                .as_deref()
                .map(Value::string)
                .unwrap_or(Value::NIL),
        ),
        Value::cons(Value::symbol("source"), Value::string("Neomacs")),
    ]);
    Value::list(attributes)
}

pub(crate) fn builtin_neomacs_display_monitor_attributes_list(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("neomacs-display-monitor-attributes-list", &args, 0, 1)?;
    let frame_ids = eval.frame_manager().frame_list();
    let frames = frame_ids
        .iter()
        .map(|fid| Value::make_frame(fid.0))
        .collect::<Vec<_>>();
    let monitor_values = neomacs_monitor_info_snapshot();
    if monitor_values.is_empty() {
        return Ok(Value::NIL);
    }

    // The monitor snapshot comes from the one graphical frontend, so its
    // backend decides, whichever frame is selected. Wayland is the PGTK case:
    // device-pixel geometry plus a scale factor.
    let on_wayland = frame_ids.iter().any(|&fid| {
        eval.frame_manager().get(fid).is_some_and(|frame| {
            matches!(
                frame.display_identity(),
                crate::window::FrameDisplayIdentity::Graphical(identity)
                    if identity.backend() == neomacs_display_protocol::GraphicalBackend::Wayland
            )
        })
    });
    let scale_factor = if on_wayland {
        MonitorScaleFactor::Report
    } else {
        MonitorScaleFactor::Omit
    };
    let mut alists = Vec::with_capacity(monitor_values.len());
    for (index, monitor) in monitor_values.iter().enumerate() {
        let frame_list = if index == 0 {
            Value::list(frames.clone())
        } else {
            Value::NIL
        };
        alists.push(monitor_alist_value(monitor, frame_list, scale_factor));
    }
    Ok(Value::list(alists))
}

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

// --- window/frame/mouse entries moved from the evaluator builtins ---

/// Extract an integer, signaling `(wrong-type-argument integerp V)`.
/// Mirrors the evaluator builtins' `expect_int` for the mouse entries moved
/// here (GNU's `CHECK_TYPE_RANGED_INTEGER` predicates on `integerp`).
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

pub(crate) fn builtin_set_mouse_pixel_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-mouse-pixel-position", &args, 3)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        Some(&args[0]),
        neovm_value::display_vocab::FrameDomain::Live,
    )?;
    let x = expect_int(&args[1])?;
    let y = expect_int(&args[2])?;
    eval.command_loop_mut()
        .keyboard
        .set_mouse_pixel_position(Some(fid), x, y);
    Ok(Value::NIL)
}

pub(crate) fn builtin_set_mouse_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-mouse-position", &args, 3)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        Some(&args[0]),
        neovm_value::display_vocab::FrameDomain::Live,
    )?;
    let x = expect_int(&args[1])?;
    let y = expect_int(&args[2])?;
    let Some(frame) = eval.frame_manager().get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    let char_width = frame.char_width.max(1.0).round() as i64;
    let char_height = frame.char_height.max(1.0).round() as i64;
    let pixel_x = x.saturating_mul(char_width).saturating_add(char_width / 2);
    let pixel_y = y
        .saturating_mul(char_height)
        .saturating_add(char_height / 2);
    eval.command_loop_mut()
        .keyboard
        .set_mouse_pixel_position(Some(fid), pixel_x, pixel_y);
    Ok(Value::NIL)
}

pub(crate) fn builtin_display_update_for_mouse_movement(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("display--update-for-mouse-movement", &args, 3)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        Some(&args[0]),
        neovm_value::display_vocab::FrameDomain::Live,
    )?;
    let x = expect_fixnum(&args[1])?;
    let y = expect_fixnum(&args[2])?;
    eval.note_mouse_move_for_frame(Some(fid), x, y);
    Ok(Value::NIL)
}

pub(crate) fn builtin_frame_scroll_bar_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-scroll-bar-height", &args, 0, 1)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.first(),
        FrameDomain::Any,
    )?;
    // GNU returns FRAME_SCROLL_BAR_AREA_HEIGHT (src/frame.c:4496-4501): the
    // configured height while the frame enables horizontal scroll bars, and
    // zero otherwise.
    Ok(Value::fixnum(eval.frame_manager().get(fid).map_or(
        0,
        crate::window::Frame::horizontal_scroll_bar_area_height,
    )))
}

pub(crate) fn builtin_frame_scroll_bar_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-scroll-bar-width", &args, 0, 1)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.first(),
        FrameDomain::Any,
    )?;
    Ok(Value::fixnum(eval.frame_manager().get(fid).map_or(
        0,
        crate::window::Frame::vertical_scroll_bar_area_width,
    )))
}

pub(crate) fn builtin_frame_root_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("frame-root-frame", &args, 0, 1)?;
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    let fid = super::window_cmds::resolve_frame_id_in_state(
        frames,
        buffers,
        args.first(),
        neovm_value::display_vocab::FrameDomain::Live,
    )?;
    let root = eval.frame_manager().root_frame_id(fid).unwrap_or(fid);
    Ok(Value::make_frame(root.0))
}

// --- fontset and face-resource entries moved from the evaluator builtins ---

fn face_boolean_x_resource_value(
    value: &str,
    signal_on_invalid: bool,
) -> Result<Option<Value>, Flow> {
    if value.eq_ignore_ascii_case("on") || value.eq_ignore_ascii_case("true") {
        Ok(Some(Value::T))
    } else if value.eq_ignore_ascii_case("off") || value.eq_ignore_ascii_case("false") {
        Ok(Some(Value::NIL))
    } else if value.eq_ignore_ascii_case("unspecified") {
        Ok(Some(Value::symbol("unspecified")))
    } else if signal_on_invalid {
        Err(signal(
            "error",
            vec![
                Value::string("Invalid face attribute value from X resource"),
                Value::string(value),
            ],
        ))
    } else {
        Ok(None)
    }
}

pub(crate) fn builtin_internal_set_lisp_face_attribute_from_resource(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range(
        "internal-set-lisp-face-attribute-from-resource",
        &args,
        3,
        4,
    )?;
    if symbol_id(&args[0], eval.symbol_mode()).is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }
    let resource_value = expect_string_lossy(&args[2])?;

    let Some(attr_id) = symbol_id(&args[1], eval.symbol_mode()) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    };
    let attr_name = resolve_sym(attr_id);

    let converted_value = if resource_value.eq_ignore_ascii_case("unspecified") {
        Value::symbol("unspecified")
    } else {
        match attr_name {
            ":height" => {
                let number =
                    builtin_string_to_number(eval, Value::string(&resource_value), Value::NIL)?;
                match number.kind() {
                    ValueKind::Fixnum(height) if height > 0 => number,
                    _ => {
                        return Err(signal(
                            "error",
                            vec![
                                Value::string("Invalid face height from X resource"),
                                Value::string(resource_value),
                            ],
                        ));
                    }
                }
            }
            ":bold" | ":italic" => {
                face_boolean_x_resource_value(&resource_value, true)?.expect("signal=true")
            }
            ":weight" | ":slant" | ":width" => Value::symbol(&resource_value),
            ":inverse-video" | ":extend" => {
                face_boolean_x_resource_value(&resource_value, true)?.expect("signal=true")
            }
            ":underline" | ":overline" | ":strike-through" => {
                face_boolean_x_resource_value(&resource_value, false)?
                    .unwrap_or_else(|| Value::string(&resource_value))
            }
            ":box" | ":inherit" => {
                let read_result = crate::emacs_core::reader::builtin_read_from_string(
                    eval,
                    vec![Value::string(&resource_value)],
                )?;
                read_result.cons_car()
            }
            _ => Value::string(&resource_value),
        }
    };

    let mut setter_args = vec![args[0], args[1], converted_value];
    if let Some(frame) = args.get(3) {
        setter_args.push(*frame);
    }
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(eval, setter_args)
}

pub(crate) fn builtin_new_fontset(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("new-fontset", &args, 2)?;
    let name = expect_string_lossy(&args[0])?;
    let char_script_table = eval.builtin_var_value_by_name("char-script-table");
    let charset_script_alist = eval.builtin_var_value_by_name("charset-script-alist");
    let font_encoding_alist = eval.builtin_var_value_by_name("font-encoding-alist");
    let registered = crate::emacs_core::fontset::new_fontset(
        &name,
        &args[1],
        char_script_table.as_ref(),
        charset_script_alist.as_ref(),
        font_encoding_alist.as_ref(),
    )?;
    Ok(Value::string(registered))
}

pub(crate) fn builtin_set_fontset_font(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-fontset-font", &args, 3, 5)?;
    let char_script_table = eval.builtin_var_value_by_name("char-script-table");
    let charset_script_alist = eval.builtin_var_value_by_name("charset-script-alist");
    let font_encoding_alist = eval.builtin_var_value_by_name("font-encoding-alist");
    crate::emacs_core::fontset::set_fontset_font(
        &args[0],
        &args[1],
        &args[2],
        args.get(4),
        char_script_table.as_ref(),
        charset_script_alist.as_ref(),
        font_encoding_alist.as_ref(),
    )
}
