//! Frame/display property builtins.
//!
//! Provides stub implementations for display and terminal query functions.
//! Since Neomacs is always a GUI application, most display queries return
//! sensible defaults for a modern graphical display.

use super::error::{EvalResult, Flow, signal};
use super::intern::intern;
use super::terminal::pure::{
    is_terminal_handle, make_alist, terminal_designator_p, terminal_handle_id,
    terminal_runtime_color_cells, terminal_runtime_supports_color,
};
use super::value::*;
use super::{Context, PopupMenuEntry, PopupMenuRequest};
use crate::emacs_core::error::LispCondition;
pub(crate) use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args};
use crate::window::{FrameId, WindowId};
use strum::{EnumString, IntoStaticStr};

/// Clear cached thread-local display values (must be called when heap changes).
pub fn reset_display_thread_locals() {
    super::terminal::pure::reset_terminal_thread_locals();
    super::dispnew::pure::reset_dispnew_thread_locals();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum WindowSystemKind {
    X,
    W32,
    Pc,
    Ns,
    Pgtk,
    Haiku,
    Android,
    Neo,
}

impl WindowSystemKind {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    fn from_symbol_value(value: Value) -> Option<Self> {
        Self::from_symbol_name(value.as_symbol_name()?)
    }

    fn is_neomacs_gui_compatible(self) -> bool {
        matches!(self, Self::Neo | Self::X)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn supports_selections(self) -> bool {
        matches!(
            self,
            Self::X | Self::W32 | Self::Ns | Self::Pgtk | Self::Haiku | Self::Android | Self::Neo
        )
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

pub(crate) fn expect_symbol_key(value: &Value) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_) => Ok(*value),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
    }
}

/// Route symbol-value reads through the full GNU lookup path so
/// LOCALIZED BLV / FORWARDED slot / specpdl let-binding state is
/// observed. See the extended comment on the identical helper in
/// `builtins/misc_eval.rs` (audit finding #3 in
/// `drafts/regex-search-audit.md`).
fn dynamic_or_global_symbol_value(eval: &super::eval::Context, name: &str) -> Option<Value> {
    let id = crate::emacs_core::intern::intern(name);
    eval.eval_symbol_by_id(id).ok()
}

fn dynamic_or_global_symbol_value_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    _dynamic: &[crate::emacs_core::value::OrderedRuntimeBindingMap],
    name: &str,
) -> Option<Value> {
    obarray.symbol_value(name).cloned()
}

fn display_string_text(value: &Value) -> Option<String> {
    // X display designators are ASCII protocol strings (e.g. ":0.0"); decode lossily.
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn global_window_system_symbol(eval: &super::eval::Context) -> Option<Value> {
    dynamic_or_global_symbol_value(eval, "initial-window-system")
        .filter(|value| !value.is_nil())
        .or_else(|| dynamic_or_global_symbol_value(eval, "window-system"))
}

fn global_window_system_symbol_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[crate::emacs_core::value::OrderedRuntimeBindingMap],
) -> Option<Value> {
    dynamic_or_global_symbol_value_in_state(obarray, dynamic, "initial-window-system")
        .filter(|value| !value.is_nil())
        .or_else(|| dynamic_or_global_symbol_value_in_state(obarray, dynamic, "window-system"))
}

fn selected_frame_window_system_symbol(eval: &super::eval::Context) -> Option<Value> {
    eval.frames
        .selected_frame()
        .and_then(|frame| frame.effective_window_system())
}

fn selected_frame_window_system_symbol_in_state(
    frames: &crate::window::FrameManager,
) -> Option<Value> {
    frames
        .selected_frame()
        .and_then(|frame| frame.effective_window_system())
}

pub(crate) fn live_frame_designator_p_in_state(
    frames: &crate::window::FrameManager,
    value: &Value,
) -> bool {
    match value.kind() {
        ValueKind::Fixnum(id) if id >= 0 => frames.get(FrameId(id as u64)).is_some(),
        ValueKind::Veclike(VecLikeType::Frame) => {
            frames.get(FrameId(value.as_frame_id().unwrap())).is_some()
        }
        _ => false,
    }
}

pub(crate) fn frame_window_system_symbol(
    eval: &mut super::eval::Context,
    frame: Option<&Value>,
) -> Result<Option<Value>, Flow> {
    frame_window_system_symbol_in_state(&mut eval.frames, &mut eval.buffers, frame)
}

fn frame_window_system_symbol_in_state(
    frames: &mut crate::window::FrameManager,
    buffers: &mut crate::buffer::BufferManager,
    frame: Option<&Value>,
) -> Result<Option<Value>, Flow> {
    let frame_id = super::window_cmds::resolve_frame_id_in_state(frames, buffers, frame, "framep")?;
    Ok(frames
        .get(frame_id)
        .and_then(|frame| frame.effective_window_system()))
}

fn invalid_get_device_terminal_error(value: &Value) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Invalid argument {} in ‘get-device-terminal’",
            super::print::print_value(value)
        ))],
    )
}

fn display_does_not_exist_error(display: &str) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("Display {display} does not exist"))],
    )
}

fn format_get_device_terminal_arg_eval(eval: &super::eval::Context, value: &Value) -> String {
    let window_id = match value.kind() {
        ValueKind::Veclike(VecLikeType::Window) => Some(WindowId(value.as_window_id().unwrap())),
        _ => None,
    };

    if let Some(window_id) = window_id
        && let Some(frame_id) = eval.frames.find_window_frame_id(window_id)
        && let Some(frame) = eval.frames.get(frame_id)
        && let Some(window) = frame.find_window(window_id)
    {
        if let Some(buffer_id) = window.buffer_id()
            && let Some(buffer) = eval.buffers.get(buffer_id)
        {
            return format!(
                "#<window {} on {}>",
                window_id.0,
                buffer.name_runtime_string_owned()
            );
        }
        return format!(
            "#<window {} on {}>",
            window_id.0,
            frame.name_runtime_string_owned()
        );
    }

    super::print::print_value(value)
}

fn invalid_get_device_terminal_error_eval(eval: &super::eval::Context, value: &Value) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Invalid argument {} in ‘get-device-terminal’",
            format_get_device_terminal_arg_eval(eval, value)
        ))],
    )
}

fn terminal_not_x_display_error(value: &Value) -> Option<Flow> {
    terminal_handle_id(value).map(|tid| {
        signal(
            "error",
            vec![Value::string(format!("Terminal {tid} is not an X display"))],
        )
    })
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn expect_frame_designator(value: &Value) -> Result<(), Flow> {
    match value.kind() {
        ValueKind::Fixnum(id) if id >= 0 => Ok(()),
        ValueKind::Veclike(VecLikeType::Frame) => Ok(()),
        ValueKind::Nil => Ok(()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *value],
        )),
    }
}

pub(crate) fn expect_display_designator_in_state(
    frames: &crate::window::FrameManager,
    value: &Value,
) -> Result<(), Flow> {
    if value.is_nil()
        || terminal_designator_p(value)
        || live_frame_designator_p_in_state(frames, value)
    {
        return Ok(());
    }
    if value.is_string() {
        let display = display_string_text(value).expect("checked string");
        return Err(display_does_not_exist_error(&display));
    }
    Err(invalid_get_device_terminal_error(value))
}

pub(crate) fn live_frame_designator_p(eval: &mut super::eval::Context, value: &Value) -> bool {
    live_frame_designator_p_in_state(&eval.frames, value)
}

fn expect_display_designator_eval(
    eval: &mut super::eval::Context,
    value: &Value,
) -> Result<(), Flow> {
    if value.is_nil() || terminal_designator_p(value) || live_frame_designator_p(eval, value) {
        return Ok(());
    }
    if value.is_string() {
        let display = display_string_text(value).expect("checked string");
        return Err(display_does_not_exist_error(&display));
    }
    Err(invalid_get_device_terminal_error_eval(eval, value))
}

fn expect_optional_display_designator_eval(
    eval: &mut super::eval::Context,
    name: &str,
    args: &[Value],
) -> Result<(), Flow> {
    expect_max_args(name, args, 1)?;
    if let Some(display) = args.first() {
        expect_display_designator_eval(eval, display)?;
    }
    Ok(())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn frame_not_live_error_eval(_eval: &super::eval::Context, value: &Value) -> Flow {
    let printable = match value.kind() {
        ValueKind::String => display_string_text(value).expect("checked string"),
        _ => format_get_device_terminal_arg_eval(_eval, value),
    };
    signal(
        "error",
        vec![Value::string(format!("{printable} is not a live frame"))],
    )
}

fn x_windows_not_initialized_error() -> Flow {
    signal(
        "error",
        vec![Value::string("X windows are not in use or not initialized")],
    )
}

fn x_window_system_frame_error() -> Flow {
    signal(
        "error",
        vec![Value::string("Window system frame should be used")],
    )
}

fn x_selection_unavailable_error() -> Flow {
    signal(
        "error",
        vec![Value::string("X selection unavailable for this frame")],
    )
}

fn x_display_open_error(display: &str) -> Flow {
    signal(
        "error",
        vec![Value::string(format!("Display {display} can’t be opened"))],
    )
}

fn x_display_query_first_arg_error(value: &Value) -> Flow {
    match value.kind() {
        ValueKind::Nil => x_windows_not_initialized_error(),
        ValueKind::String => {
            x_display_open_error(&display_string_text(value).expect("checked string"))
        }
        ValueKind::Veclike(VecLikeType::Frame) => x_window_system_frame_error(),
        _ => {
            if let Some(err) = terminal_not_x_display_error(value) {
                err
            } else {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("frame-live-p"), *value],
                )
            }
        }
    }
}

fn window_system_not_initialized_error() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "Window system is not in use or not initialized",
        )],
    )
}

pub fn gui_window_system_symbol() -> &'static str {
    "neo"
}

pub(crate) fn gui_window_system_active_value(value: Value) -> bool {
    WindowSystemKind::from_symbol_value(value)
        .is_some_and(WindowSystemKind::is_neomacs_gui_compatible)
}

pub(crate) fn x_window_system_active(eval: &super::eval::Context) -> bool {
    let host_window_system =
        selected_frame_window_system_symbol(eval).or_else(|| global_window_system_symbol(eval));
    host_window_system.is_some_and(gui_window_system_active_value)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn x_window_system_active_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[crate::emacs_core::value::OrderedRuntimeBindingMap],
) -> bool {
    let host_window_system = global_window_system_symbol_in_state(obarray, dynamic);
    host_window_system.is_some_and(gui_window_system_active_value)
}

pub(crate) fn display_window_system_symbol_eval(
    eval: &mut super::eval::Context,
    display: Option<&Value>,
) -> Result<Option<Value>, Flow> {
    match display {
        None => {
            Ok(selected_frame_window_system_symbol(eval)
                .or_else(|| global_window_system_symbol(eval)))
        }
        Some(d) if d.is_nil() => {
            Ok(selected_frame_window_system_symbol(eval)
                .or_else(|| global_window_system_symbol(eval)))
        }
        Some(d) if terminal_designator_p(d) => Ok(None),
        Some(d) if live_frame_designator_p(eval, d) => frame_window_system_symbol(eval, Some(d)),
        Some(d) if d.is_string() => {
            let display = display_string_text(d).expect("checked string");
            Err(display_does_not_exist_error(&display))
        }
        Some(other) => Err(invalid_get_device_terminal_error_eval(eval, other)),
    }
}

fn frame_window_system_symbol_read_only_in_state(
    frames: &crate::window::FrameManager,
    frame: Option<&Value>,
) -> Result<Option<Value>, Flow> {
    match frame {
        None => Ok(selected_frame_window_system_symbol_in_state(frames)),
        Some(v) if v.is_nil() => Ok(selected_frame_window_system_symbol_in_state(frames)),
        Some(v) => match v.kind() {
            ValueKind::Fixnum(id) if id >= 0 => Ok(frames
                .get(FrameId(id as u64))
                .and_then(|frame| frame.effective_window_system())),
            ValueKind::Veclike(VecLikeType::Frame) => Ok(frames
                .get(FrameId(v.as_frame_id().unwrap()))
                .and_then(|frame| frame.effective_window_system())),
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("framep"), *v],
            )),
        },
    }
}

pub(crate) fn display_window_system_symbol_in_state(
    frames: &crate::window::FrameManager,
    obarray: &crate::emacs_core::symbol::Obarray,
    dynamic: &[crate::emacs_core::value::OrderedRuntimeBindingMap],
    display: Option<&Value>,
) -> Result<Option<Value>, Flow> {
    match display {
        None => Ok(
            frame_window_system_symbol_read_only_in_state(frames, display)?
                .or_else(|| global_window_system_symbol_in_state(obarray, dynamic)),
        ),
        Some(d) if d.is_nil() => Ok(frame_window_system_symbol_read_only_in_state(
            frames, display,
        )?
        .or_else(|| global_window_system_symbol_in_state(obarray, dynamic))),
        Some(d) if terminal_designator_p(d) => Ok(None),
        Some(d) if live_frame_designator_p_in_state(frames, d) => {
            frame_window_system_symbol_read_only_in_state(frames, Some(d))
        }
        Some(d) if d.is_string() => {
            let display = display_string_text(d).expect("checked string");
            Err(display_does_not_exist_error(&display))
        }
        Some(other) => Err(invalid_get_device_terminal_error(other)),
    }
}

const GUI_X_DISPLAY_PLANES: i64 = 24;
const GUI_X_DISPLAY_COLOR_CELLS: i64 = 16_777_216;
const GUI_X_VISUAL_CLASS: &str = "true-color";

fn gui_x_query_target_eval(
    eval: &mut super::eval::Context,
    name: &str,
    args: &[Value],
) -> Result<bool, Flow> {
    expect_max_args(name, args, 1)?;
    // For X-family primitives, only nil / live-frame designators
    // exercise the "X is active" code path. Strings, fixnums, and
    // other non-frame designators all fall through to
    // `x_optional_display_query_error_eval` which produces the
    // GNU-faithful error shape (`Display X can't be opened` for
    // strings, `wrong-type-argument frame-live-p N` for ints).
    if let Some(arg) = args.first()
        && !arg.is_nil()
        && !live_frame_designator_p(eval, arg)
    {
        return Ok(false);
    }
    if !display_window_system_symbol_eval(eval, args.first())?
        .is_some_and(gui_window_system_active_value)
    {
        return Ok(false);
    }
    Ok(match args.first() {
        None => true,
        Some(v) if v.is_nil() => true,
        Some(display) => live_frame_designator_p(eval, display),
    })
}

fn expect_optional_window_system_frame_arg(value: &Value) -> Result<(), Flow> {
    if value.is_nil() || value.is_frame() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *value],
        ))
    }
}

fn parse_geometry_unsigned(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let start = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..*index])
        .ok()?
        .parse::<i64>()
        .ok()
}

fn parse_geometry_signed_offset(bytes: &[u8], index: &mut usize) -> Option<i64> {
    if *index >= bytes.len() {
        return None;
    }
    let sign = match bytes[*index] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    *index += 1;
    Some(sign * parse_geometry_unsigned(bytes, index)?)
}

fn parse_x_geometry(spec: &str) -> Option<Value> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }

    let bytes = spec.as_bytes();
    let mut index = 0usize;
    if bytes[index] == b'=' {
        index += 1;
        if index >= bytes.len() {
            return None;
        }
    }

    let mut width = None;
    let mut height = None;
    let mut left = None;
    let mut top = None;

    let geometry_start = index;
    if let Some(parsed_width) = parse_geometry_unsigned(bytes, &mut index) {
        if index < bytes.len() && bytes[index] == b'x' {
            index += 1;
            let parsed_height = parse_geometry_unsigned(bytes, &mut index)?;
            width = Some(parsed_width);
            height = Some(parsed_height);
        } else {
            index = geometry_start;
        }
    } else if index < bytes.len() && bytes[index] == b'x' {
        return None;
    }

    if index < bytes.len() {
        let parsed_left = parse_geometry_signed_offset(bytes, &mut index)?;
        left = Some(parsed_left);
        if index < bytes.len() {
            let parsed_top = parse_geometry_signed_offset(bytes, &mut index)?;
            top = Some(parsed_top);
        }
    }

    if index != bytes.len() {
        return None;
    }

    if width.is_none() && height.is_none() && left.is_none() && top.is_none() {
        return None;
    }

    let mut pairs = Vec::new();
    if let Some(h) = height {
        pairs.push(Value::cons(Value::symbol("height"), Value::fixnum(h)));
    }
    if let Some(w) = width {
        pairs.push(Value::cons(Value::symbol("width"), Value::fixnum(w)));
    }
    if let Some(y) = top {
        pairs.push(Value::cons(Value::symbol("top"), Value::fixnum(y)));
    }
    if let Some(x) = left {
        pairs.push(Value::cons(Value::symbol("left"), Value::fixnum(x)));
    }
    Some(Value::list(pairs))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn display_optional_capability_p_eval(
    eval: &mut super::eval::Context,
    name: &str,
    args: &[Value],
) -> EvalResult {
    expect_max_args(name, args, 1)?;
    match args.first() {
        None => Ok(Value::NIL),
        Some(v) if v.is_nil() => Ok(Value::NIL),
        Some(display) if is_terminal_handle(display) => Ok(Value::NIL),
        Some(display) if live_frame_designator_p(eval, display) => Ok(Value::NIL),
        Some(v) if v.is_string() => {
            let display = display_string_text(v).expect("checked string");
            Err(signal(
                "error",
                vec![Value::string(format!("Display {display} does not exist"))],
            ))
        }
        Some(other) => Err(invalid_get_device_terminal_error_eval(eval, other)),
    }
}

fn x_optional_display_query_error(name: &str, args: &[Value]) -> EvalResult {
    expect_max_args(name, args, 1)?;
    match args.first() {
        None => Err(x_windows_not_initialized_error()),
        Some(v) if v.is_nil() => Err(x_windows_not_initialized_error()),
        Some(display) if is_terminal_handle(display) => {
            if let Some(err) = terminal_not_x_display_error(display) {
                Err(err)
            } else {
                Err(invalid_get_device_terminal_error(display))
            }
        }
        Some(v) if v.is_string() => {
            let display = display_string_text(v).expect("checked string");
            Err(signal(
                "error",
                vec![Value::string(format!("Display {display} can’t be opened"))],
            ))
        }
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *other],
        )),
    }
}

fn x_optional_display_query_error_eval(
    eval: &mut super::eval::Context,
    name: &str,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args(name, &args, 1)?;
    if let Some(display) = args.first()
        && live_frame_designator_p(eval, display)
    {
        return Err(x_window_system_frame_error());
    }
    x_optional_display_query_error(name, &args)
}

// ---------------------------------------------------------------------------
// Display query builtins
// ---------------------------------------------------------------------------

/// Context-aware variant of `display-graphic-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_graphic_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-graphic-p", &args)?;
    Ok(Value::bool_val(
        display_window_system_symbol_eval(eval, args.first())?
            .is_some_and(|value| value.is_symbol()),
    ))
}

/// Context-aware variant of `display-grayscale-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_grayscale_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    display_optional_capability_p_eval(eval, "display-grayscale-p", &args)
}

/// Context-aware variant of `display-mouse-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_mouse_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    display_optional_capability_p_eval(eval, "display-mouse-p", &args)
}

/// Context-aware variant of `display-popup-menus-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_popup_menus_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    display_optional_capability_p_eval(eval, "display-popup-menus-p", &args)
}

/// Context-aware variant of `display-symbol-keys-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_symbol_keys_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    display_optional_capability_p_eval(eval, "display-symbol-keys-p", &args)
}

/// Context-aware variant of `display-pixel-width`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_pixel_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-pixel-width", &args)?;
    Ok(Value::fixnum(80))
}

/// Context-aware variant of `display-pixel-height`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_pixel_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-pixel-height", &args)?;
    Ok(Value::fixnum(25))
}

/// Context-aware variant of `display-mm-width`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_mm_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-mm-width", &args)?;
    Ok(Value::NIL)
}

/// Context-aware variant of `display-mm-height`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_mm_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-mm-height", &args)?;
    Ok(Value::NIL)
}

/// Context-aware variant of `display-screens`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_screens(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-screens", &args)?;
    Ok(Value::fixnum(1))
}

// `display-color-cells' is GONE, with the seventeen of DIVERGENCES.md 154 it
// was held back from.  GNU has no `DEFUN ("display-color-cells"' anywhere in
// `src/': the name is `lisp/frame.el:2966', and its body dispatches on
// `framep-on-display' to `x-display-color-cells' (src/xfns.c:5714) or
// `tty-display-color-cells' (src/term.c:2226) -- both of which are real
// DEFUNs and both of which stay registered here.
//
// It was the eighteenth name and the one 154 could not delete, because our
// `(load "faces")' reached it while GNU's cannot: GNU's `loadup.el' loads
// `faces' at :160 and `frame' at :255, so the name is void for ninety-five
// files and GNU still bootstraps.  The caller was `show-paren-match's
// `((background dark) (min-colors 4))' clause (lisp/faces.el:3161), whose
// FIRST conjunct matched only because Rust seeded a `background-mode' frame
// parameter that GNU's `make_initial_frame' (src/frame.c:1423) does not set.
// That seeding is gone (DIVERGENCES.md 157), so the clause now fails on its
// first conjunct exactly as it does in GNU and the walk never reaches
// `min-colors'.

/// Context-aware variant of `display-planes`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_planes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-planes", &args)?;
    if display_window_system_symbol_eval(eval, args.first())?
        .is_some_and(gui_window_system_active_value)
    {
        Ok(Value::fixnum(24))
    } else if terminal_runtime_supports_color() {
        Ok(Value::fixnum(
            if terminal_runtime_color_cells() >= 16777216 {
                24
            } else {
                8
            },
        ))
    } else {
        Ok(Value::fixnum(3))
    }
}

/// Context-aware variant of `display-visual-class`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_visual_class(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-visual-class", &args)?;
    if display_window_system_symbol_eval(eval, args.first())?
        .is_some_and(gui_window_system_active_value)
    {
        Ok(Value::symbol("true-color"))
    } else if terminal_runtime_supports_color() {
        Ok(Value::symbol("color"))
    } else {
        Ok(Value::symbol("static-gray"))
    }
}

/// Context-aware variant of `display-backing-store`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_backing_store(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-backing-store", &args)?;
    Ok(Value::symbol("not-useful"))
}

/// Context-aware variant of `display-save-under`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_save_under(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-save-under", &args)?;
    Ok(Value::symbol("not-useful"))
}

/// Context-aware variant of `display-selections-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_selections_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-selections-p", &args)?;
    let window_system = display_window_system_symbol_eval(eval, args.first())?;
    Ok(Value::bool_val(
        window_system
            .and_then(WindowSystemKind::from_symbol_value)
            .is_some_and(WindowSystemKind::supports_selections),
    ))
}

/// Context-aware variant of `window-system`.
pub(crate) fn builtin_window_system(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("window-system", &args, 1)?;
    if args.first().is_none_or(|v| v.is_nil()) {
        if let Some(window_system) = selected_frame_window_system_symbol_in_state(&eval.frames) {
            return Ok(window_system);
        }
    } else if let Some(window_system) =
        frame_window_system_symbol_in_state(&mut eval.frames, &mut eval.buffers, args.first())?
    {
        return Ok(window_system);
    } else {
        return Ok(Value::NIL);
    }
    Ok(
        dynamic_or_global_symbol_value_in_state(&eval.obarray, &[], "window-system")
            .unwrap_or(Value::NIL),
    )
}

/// Context-aware variant of `frame-edges`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_edges(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("frame-edges", &args, 0, 2)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
        && !live_frame_designator_p(eval, frame)
    {
        return Err(frame_not_live_error_eval(eval, frame));
    }
    Ok(Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(80),
        Value::fixnum(25),
    ]))
}

/// Context-aware variant of `display-images-p`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_images_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-images-p", &args)?;
    Ok(Value::bool_val(eval.display_host.is_some()))
}

/// GNU `tty_supports_face_attributes_p` (src/xfaces.c): can THIS terminal render
/// the requested attributes, as a visible difference from the default face?
///
/// GNU's structure, kept arm for arm:
///
/// * attributes a terminal has no notion of (`:family`, `:foundry`, `:stipple`,
///   `:height`, `:width`, `:overline`, `:box`) make the answer false outright;
/// * an attribute equal to the default face's is not a difference, so false;
/// * everything else turns into a capability to test, and the terminal's
///   terminfo record answers it (`tty_capable_p`).
///
/// GNU deliberately reports `:slant` unsupported when the terminal has no
/// `sitm` even though `turn_on_face` fakes it with `dim`, "because the faked
/// result is too different from what the face specifies" — so the renderer's
/// fallback does NOT make the attribute supported. Both read the same record.
fn tty_supports_face_attributes_p(
    default_face: &crate::face::Face,
    requested: &crate::face::Face,
) -> bool {
    use neomacs_display_protocol::tty_capabilities::TtyCapability;

    // Attributes a tty cannot express at all.
    if requested.family.is_some()
        || requested.foundry.is_some()
        || requested.stipple.is_some()
        || requested.height.is_some()
        || requested.width.is_some()
        || requested.overline.is_some()
        || !matches!(
            requested.box_border,
            crate::face::FaceDecoration::Unspecified
        )
    {
        return false;
    }

    let capabilities = super::terminal::pure::terminal_runtime_attribute_capabilities();
    let mut tested = Vec::new();

    // Weight: heavier than normal needs bold, lighter needs dim, and a weight
    // matching the default is no difference. GNU compares against 100 in its own
    // 0..210 scale, whose `normal' is `FontWeight::NORMAL`.
    if let Some(weight) = requested.weight {
        let normal = crate::face::FontWeight::NORMAL.gnu_numeric();
        let default_weight = default_face
            .weight
            .unwrap_or(crate::face::FontWeight::NORMAL)
            .gnu_numeric();
        let weight = weight.gnu_numeric();
        if weight > normal {
            if default_weight > normal {
                return false;
            }
            tested.push(TtyCapability::Bold);
        } else if weight < normal {
            if default_weight < normal {
                return false;
            }
            tested.push(TtyCapability::Dim);
        } else if default_weight == normal {
            return false;
        }
    }

    // Slant: anything other than roman, and different from the default.
    if let Some(slant) = requested.slant {
        let default_slant = default_face
            .slant
            .unwrap_or(crate::face::FontSlant::Normal)
            .gnu_numeric();
        if slant.gnu_numeric() == crate::face::FontSlant::Normal.gnu_numeric()
            || slant.gnu_numeric() == default_slant
        {
            return false;
        }
        tested.push(TtyCapability::Italic);
    }

    // Underline: a style other than a plain line needs the parameterized
    // `Smulx`; a plain underline needs `us`. Either way it must differ from the
    // default face.
    match &requested.underline {
        crate::face::FaceDecoration::Unspecified => {}
        crate::face::FaceDecoration::Disabled => {
            if matches!(
                default_face.underline,
                crate::face::FaceDecoration::Disabled | crate::face::FaceDecoration::Unspecified
            ) {
                return false;
            }
            tested.push(TtyCapability::Underline);
        }
        crate::face::FaceDecoration::Enabled(underline) => {
            if let Some(default_underline) = default_face.underline.enabled()
                && default_underline.style == underline.style
            {
                return false;
            }
            if underline.style == crate::face::UnderlineStyle::Line {
                tested.push(TtyCapability::Underline);
            } else {
                tested.push(TtyCapability::UnderlineStyled);
            }
        }
    }

    if let Some(inverse) = requested.inverse_video {
        if Some(inverse) == default_face.inverse_video {
            return false;
        }
        tested.push(TtyCapability::Inverse);
    }

    if let Some(strike_through) = requested.strike_through {
        if Some(strike_through) == default_face.strike_through {
            return false;
        }
        tested.push(TtyCapability::StrikeThrough);
    }

    // Colors: GNU checks that a requested color both differs from the default
    // face's and survives the terminal's palette closely enough
    // (`TTY_SAME_COLOR_THRESHOLD`). A color equal to the default is no
    // difference.
    if let Some(foreground) = requested.foreground
        && Some(foreground) == default_face.foreground
    {
        return false;
    }
    if let Some(background) = requested.background
        && Some(background) == default_face.background
    {
        return false;
    }
    let requests_color = requested.foreground.is_some() || requested.background.is_some();
    if requests_color && !capabilities.supports_color() {
        return false;
    }

    if tested.is_empty() {
        // Nothing testable was requested: a color-only request is supported when
        // the terminal has colors, and an empty request is not a difference.
        return requests_color;
    }
    tested
        .into_iter()
        .all(|capability| capabilities.supports(capability))
}

/// Context-aware variant of `display-supports-face-attributes-p`.
///
/// Emacs accepts broad argument shapes here in batch mode and still returns
/// nil as long as arity is valid.
pub(crate) fn builtin_display_supports_face_attributes_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("display-supports-face-attributes-p", &args, 1, 2)?;
    let Some(attributes) = super::value::list_to_vec(&args[0]) else {
        return Ok(Value::NIL);
    };
    let Some(frame_id) = args
        .get(1)
        .filter(|display| !display.is_nil())
        .and_then(|display| display.as_frame_id())
        .map(FrameId)
        .or_else(|| eval.frames.selected_frame().map(|frame| frame.id))
    else {
        return Ok(Value::NIL);
    };
    if eval.frames.get(frame_id).is_none() {
        return Ok(Value::NIL);
    }

    // GNU realizes the requested attributes over the frame's default face,
    // then verifies that the selected font both differs from the default and
    // reasonably matches the request (`gui_supports_face_attributes_p`,
    // xfaces.c).  Keep font selection behind DisplayHost: core owns GNU face
    // semantics while the GUI host owns the platform font database.
    let default_face = eval.face_table().resolve("default");
    let requested_attributes = crate::face::Face::from_plist("anonymous", &attributes);
    let requested_face = default_face.merge(&requested_attributes);
    // Stipple is a GUI-only background fill (GNU realizes it to a pixmap). On a
    // graphical frame report it supported so `indent-bars` selects the stipple
    // path instead of falling back to a character bar; a terminal frame never
    // supports it.
    if requested_attributes.stipple.is_some() {
        return Ok(Value::bool_val(eval.display_host.is_some()));
    }
    let Some(host) = eval.display_host.as_mut() else {
        // A terminal frame: GNU dispatches on the frame type
        // (`FRAME_WINDOW_P (f) ? gui_supports_face_attributes_p : ...`) and
        // answers from the terminal's own capabilities
        // (`tty_supports_face_attributes_p`, xfaces.c), never from font
        // selection.
        //
        // NOTE: keyed off the absence of a display host rather than the frame's
        // window system, because a frame built without one still reaches the GUI
        // path in tests. Whether the live `-nw` frontend attaches a host -- in
        // which case this branch is not reached there -- is unverified; the unit
        // coverage below pins the semantics either way.
        return Ok(Value::bool_val(tty_supports_face_attributes_p(
            &default_face,
            &requested_attributes,
        )));
    };
    let default_font = host
        .resolve_frame_font(
            frame_id,
            super::display_host::FrameFontRequest::from_face(default_face),
        )
        .ok()
        .flatten();
    let requested_font = host
        .resolve_frame_font(
            frame_id,
            super::display_host::FrameFontRequest::from_face(requested_face),
        )
        .ok()
        .flatten();
    let supported = match (default_font, requested_font) {
        (Some(default), Some(requested)) if default != requested => {
            requested_attributes
                .slant
                .is_none_or(|slant| requested.font.slant.is_italic() == slant.is_italic())
                && requested_attributes
                    .weight
                    .is_none_or(|weight| requested.font.weight() == weight)
                && requested_attributes
                    .width
                    .is_none_or(|width| requested.font.width() == width)
        }
        _ => false,
    };
    Ok(Value::bool_val(supported))
}

// ---------------------------------------------------------------------------
// X display builtins (compatibility stubs)
// ---------------------------------------------------------------------------

/// (x-display-list) -> nil in batch-style vm context.
pub(crate) fn builtin_x_display_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-display-list", &args, 0)?;
    Ok(Value::NIL)
}

/// (x-frame-edges &optional FRAME TYPE) -> nil in batch/no-X context.
pub(crate) fn builtin_x_frame_edges(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-frame-edges", &args, 2)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
        && !frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(Value::NIL)
}

/// (x-frame-geometry &optional FRAME) -> nil in batch/no-X context.
pub(crate) fn builtin_x_frame_geometry(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-frame-geometry", &args, 1)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
        && !frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(Value::NIL)
}

/// (x-frame-list-z-order &optional DISPLAY) -> error in batch/no-X context.
pub(crate) fn builtin_x_frame_list_z_order(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-frame-list-z-order", &args, 1)?;
    match args.first() {
        None => Err(x_windows_not_initialized_error()),
        Some(display) => Err(x_display_query_first_arg_error(display)),
    }
}

/// (x-frame-restack FRAME1 FRAME2 &optional ABOVE) -> error in batch/no-X context.
///
/// Oracle batch behavior crashes on valid-arity runtime calls in this
/// environment, so we only expose arity/fboundp compatibility surface and a
/// conservative batch/no-X error result.
pub(crate) fn builtin_x_frame_restack(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-frame-restack", &args, 2, 3)?;
    Err(x_window_system_frame_error())
}

/// (x-mouse-absolute-pixel-position) -> nil in batch/no-X context.
pub(crate) fn builtin_x_mouse_absolute_pixel_position(args: Vec<Value>) -> EvalResult {
    expect_args("x-mouse-absolute-pixel-position", &args, 0)?;
    Ok(Value::NIL)
}

/// (x-set-mouse-absolute-pixel-position X Y) -> nil in batch/no-X context.
pub(crate) fn builtin_x_set_mouse_absolute_pixel_position(args: Vec<Value>) -> EvalResult {
    expect_args("x-set-mouse-absolute-pixel-position", &args, 2)?;
    Ok(Value::NIL)
}

/// (x-send-client-message DISPLAY PROP VALUE-0 VALUE-1 VALUE-2 VALUE-3) -> error in batch/no-X context.
pub(crate) fn builtin_x_send_client_message(args: Vec<Value>) -> EvalResult {
    expect_args("x-send-client-message", &args, 6)?;
    Err(x_display_query_first_arg_error(&args[0]))
}

fn validate_x_popup_dialog_args(args: &[Value]) -> Result<(), Flow> {
    expect_args_range("x-popup-dialog", args, 2, 3)?;

    if !args[0].is_frame() && !args[0].is_t() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("windowp"), Value::NIL],
        ));
    }

    let contents = &args[1];
    if contents.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), Value::NIL],
        ));
    }

    let (title, rest) = if contents.is_cons() {
        (contents.cons_car(), contents.cons_cdr())
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *contents],
        ));
    };

    if !title.is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), title],
        ));
    }

    if !rest.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), rest],
        ));
    }

    Ok(())
}

/// (x-popup-dialog POSITION CONTENTS &optional HEADER) -> nil/error in batch context.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_x_popup_dialog_batch(args: Vec<Value>) -> EvalResult {
    validate_x_popup_dialog_args(&args)?;
    Ok(Value::NIL)
}

fn popup_menu_string(value: Value) -> Option<String> {
    // Menu labels are display text; decode lossily (exact for ASCII/Unicode labels).
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn popup_menu_key_event_from_path(path: &[Value]) -> Value {
    Value::list(path.to_vec())
}

fn popup_menu_item_from_binding(
    _key: Value,
    def: Value,
    depth: u32,
    is_tty: bool,
) -> Option<(PopupMenuEntry, Option<Value>)> {
    if !def.is_cons() {
        return None;
    }

    let car = def.cons_car();
    let cdr = def.cons_cdr();

    if super::keymap::KeymapMarker::MenuItem.is_value(car) && cdr.is_cons() {
        let label = popup_menu_string(cdr.cons_car())?;
        let tail = cdr.cons_cdr();
        let command = if tail.is_cons() {
            tail.cons_car()
        } else {
            Value::NIL
        };
        let submenu = super::keymap::is_list_keymap(&command);
        return Some((
            PopupMenuEntry {
                label: submenu_label(label, submenu, is_tty),
                shortcut: String::new(),
                enabled: !command.is_nil(),
                separator: false,
                submenu,
                depth,
            },
            submenu.then_some(command),
        ));
    }

    let label = popup_menu_string(car)?;
    let submenu = super::keymap::is_list_keymap(&cdr);
    Some((
        PopupMenuEntry {
            label: submenu_label(label, submenu, is_tty),
            shortcut: String::new(),
            enabled: !cdr.is_nil(),
            separator: false,
            submenu,
            depth,
        },
        submenu.then_some(cdr),
    ))
}

/// Append GNU's submenu indicator to a TTY menu label.
///
/// GNU `single_menu_item` (src/menu.c:407-413): when the menu-updating frame is
/// a TTY frame and the item is itself a keymap (a submenu), it concatenates the
/// `AUTO_STRING (" >")` suffix so the collapsed line shows it opens a submenu.
/// On window-system frames the toolkit draws the submenu arrow itself, so no
/// suffix is added there.
fn submenu_label(label: String, submenu: bool, is_tty: bool) -> String {
    if submenu && is_tty {
        format!("{label} >")
    } else {
        label
    }
}

fn popup_menu_from_keymap(
    menu: Value,
    is_tty: bool,
    obarray: &crate::emacs_core::symbol::Obarray,
) -> Option<(Vec<PopupMenuEntry>, Vec<Value>)> {
    if !super::keymap::is_list_keymap(&menu) {
        return None;
    }
    let mut entries = Vec::new();
    let mut events = Vec::new();

    fn append_keymap(
        menu: Value,
        depth: u32,
        is_tty: bool,
        obarray: &crate::emacs_core::symbol::Obarray,
        path: &mut Vec<Value>,
        entries: &mut Vec<PopupMenuEntry>,
        events: &mut Vec<Value>,
    ) {
        if depth > 32 {
            return;
        }

        super::keymap::list_keymap_for_each_binding(&menu, Some(obarray), |key, def| {
            let Some((entry, submenu)) = popup_menu_item_from_binding(key, def, depth, is_tty)
            else {
                return;
            };

            path.push(key);
            entries.push(entry);
            events.push(popup_menu_key_event_from_path(path));

            // GNU `single_menu_item` (src/menu.c:422-433) recurses into a
            // submenu's panes (`single_keymap_panes`) only inside the
            // `#if USE_X_TOOLKIT || USE_GTK || HAVE_NS || ...` block, i.e.
            // exclusively for window-system frames whose toolkit renders nested
            // panes. For a TTY frame that code is compiled out: the submenu is
            // pushed as a single collapsed line (ending in `" >"`) and its
            // children are shown on demand by `tty_menu_activate` (src/term.c).
            // So on TTY we must NOT inline the submenu's children here —
            // recursing flattens, e.g., all of Help -> Describe's items into
            // the parent pane and pushes later items off-screen.
            if !is_tty && let Some(child_menu) = submenu {
                append_keymap(
                    child_menu,
                    depth + 1,
                    is_tty,
                    obarray,
                    path,
                    entries,
                    events,
                );
            }

            path.pop();
        });
    }

    append_keymap(
        menu,
        0,
        is_tty,
        obarray,
        &mut Vec::new(),
        &mut entries,
        &mut events,
    );
    Some((entries, events))
}

#[derive(Clone, Copy)]
struct PopupMenuPosition {
    placement: neomacs_display_protocol::PopupPlacement,
}

impl PopupMenuPosition {
    fn at(x: f32, y: f32) -> Self {
        Self {
            placement: neomacs_display_protocol::PopupPlacement::at(
                neomacs_display_protocol::Point::new(x, y),
            ),
        }
    }

    fn estimated_origin(self) -> neomacs_display_protocol::Point {
        self.placement
            .preferred_origin(neomacs_display_protocol::Size::ZERO)
    }
}

#[derive(Clone, Copy, Debug)]
struct PopupMenuPositionDebug {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    top_level_xy: Option<(f32, f32)>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    posn_len: Option<usize>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    area: Option<&'static str>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    posn_xy: Option<(f32, f32)>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    anchor_xy: Option<(f32, f32)>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    width_height: Option<(f32, f32)>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    frame_id: Option<FrameId>,
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    frame_menu_bar_height: Option<u32>,
    used_anchor: bool,
    used_pending_anchor: bool,
}

fn popup_menu_xy(value: Value) -> Option<(f32, f32)> {
    if let Some(xy) = list_to_vec(&value)
        && xy.len() >= 2
    {
        return Some((
            xy[0].as_fixnum().unwrap_or(0) as f32,
            xy[1].as_fixnum().unwrap_or(0) as f32,
        ));
    }
    if value.is_cons() {
        return Some((
            value.cons_car().as_fixnum().unwrap_or(0) as f32,
            value.cons_cdr().as_fixnum().unwrap_or(0) as f32,
        ));
    }
    None
}

fn popup_menu_position(ctx: &mut Context, position: Value) -> PopupMenuPosition {
    let Some(items) = list_to_vec(&position) else {
        tracing::debug!("x-popup-menu position: non-list position, fallback=(0, 0)");
        return PopupMenuPosition::at(0.0, 0.0);
    };
    if let Some(first) = items.first()
        && let Some((x, y)) = popup_menu_xy(*first)
    {
        tracing::debug!(?x, ?y, "x-popup-menu position: using top-level xy position");
        return PopupMenuPosition::at(x, y);
    }
    if let Some(second) = items.get(1)
        && let Some(posn) = list_to_vec(second)
        && posn.len() >= 3
        && let Some((mut x, mut y)) = popup_menu_xy(posn[2])
    {
        let area = posn.get(1).and_then(|area| {
            list_to_vec(area)
                .and_then(|items| items.first().copied())
                .and_then(|value| value.as_symbol_name())
        });
        let menu_bar = area.is_some_and(|name| name == "menu-bar");
        let anchor_xy = posn.get(8).and_then(|anchor| popup_menu_xy(*anchor));
        let width_height = posn
            .get(9)
            .and_then(|width_height| popup_menu_xy(*width_height));
        let posn_frame_id = posn
            .first()
            .and_then(|value| value.as_frame_id().map(crate::window::FrameId));
        let frame_id = posn
            .first()
            .and_then(|value| value.as_frame_id().map(crate::window::FrameId))
            .or_else(|| ctx.frame_manager().selected_frame().map(|frame| frame.id));
        let frame_menu_bar_height = frame_id
            .and_then(|frame_id| ctx.frame_manager().get(frame_id))
            .map(|frame| frame.menu_bar_height);
        let mut position_debug = PopupMenuPositionDebug {
            top_level_xy: None,
            posn_len: Some(posn.len()),
            area,
            posn_xy: Some((x, y)),
            anchor_xy,
            width_height,
            frame_id,
            frame_menu_bar_height,
            used_anchor: false,
            used_pending_anchor: false,
        };
        let placement = if menu_bar {
            let native_anchor = if let Some(anchor) = ctx.pending_menu_bar_popup_anchor.as_ref()
                && posn_frame_id.is_none_or(|id| id == anchor.frame_id)
            {
                let anchor = neomacs_display_protocol::Rect::new(
                    anchor.x as f32,
                    anchor.y as f32,
                    anchor.width.max(0) as f32,
                    anchor.height.max(0) as f32,
                );
                ctx.pending_menu_bar_popup_anchor = None;
                position_debug.used_pending_anchor = true;
                Some(anchor)
            } else {
                None
            };
            let anchor = native_anchor.unwrap_or_else(|| {
                let (anchor_x, _anchor_y) = anchor_xy.unwrap_or((x, y));
                let (width, reported_height) = width_height.unwrap_or((0.0, 0.0));
                let height = frame_menu_bar_height
                    .filter(|height| *height > 0)
                    .map_or(reported_height, |height| height as f32);
                position_debug.used_anchor = anchor_xy.is_some();
                neomacs_display_protocol::Rect::new(anchor_x, 0.0, width.max(0.0), height.max(0.0))
            });
            x = anchor.x;
            y = anchor.bottom();
            neomacs_display_protocol::PopupPlacement::new(
                anchor,
                neomacs_display_protocol::PopupPreferredSide::Below,
                neomacs_display_protocol::Point::ZERO,
                neomacs_display_protocol::PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
            )
        } else {
            neomacs_display_protocol::PopupPlacement::at(neomacs_display_protocol::Point::new(x, y))
        };
        tracing::debug!(
            position = ?position_debug,
            final_x = x,
            final_y = y,
            "x-popup-menu position: resolved from event position"
        );
        return PopupMenuPosition { placement };
    }
    tracing::debug!(
        list_len = items.len(),
        "x-popup-menu position: unsupported position shape, fallback=(0, 0)"
    );
    PopupMenuPosition::at(0.0, 0.0)
}

fn menu_bar_navigation_position(
    ctx: &Context,
    position: Value,
    direction: MenuBarNavigationDirection,
) -> Option<Value> {
    let items = list_to_vec(&position)?;
    let current_key = *items.first()?;
    let menu_bar_items = super::builtins::symbols::menu_bar_top_level_items(ctx);
    let current_index = menu_bar_items
        .iter()
        .position(|(key, _)| key.bits() == current_key.bits())?;

    let next_index = match direction {
        MenuBarNavigationDirection::Left => current_index.saturating_sub(1),
        MenuBarNavigationDirection::Right => (current_index + 1).min(menu_bar_items.len() - 1),
    };
    if next_index == current_index {
        return None;
    }

    let x = menu_bar_items
        .iter()
        .take(next_index)
        .map(|(_, label)| label.chars().count() as i64 + 1)
        .sum::<i64>();
    Some(Value::cons(Value::fixnum(x), Value::fixnum(0)))
}

enum MenuBarNavigationDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum TtyMenuNavigationCommand {
    TtyMenuNextItem,
    TtyMenuPrevItem,
    TtyMenuNextMenu,
    TtyMenuPrevMenu,
    TtyMenuSelect,
    TtyMenuExit,
    KeyboardQuit,
    KeyboardEscapeQuit,
}

impl TtyMenuNavigationCommand {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

fn popup_dialog_from_contents(
    contents: Value,
) -> Option<(String, Vec<PopupMenuEntry>, Vec<Value>)> {
    let title = popup_menu_string(contents.cons_car())?;
    let mut rest = contents.cons_cdr();
    let mut entries = Vec::new();
    let mut values = Vec::new();
    let mut depth = 0;

    while rest.is_cons() && depth < 256 {
        let item = rest.cons_car();
        if item.is_nil() {
            entries.push(PopupMenuEntry {
                label: String::new(),
                shortcut: String::new(),
                enabled: false,
                separator: true,
                submenu: false,
                depth: 0,
            });
            values.push(Value::NIL);
        } else if item.is_string() {
            entries.push(PopupMenuEntry {
                label: popup_menu_string(item)?,
                shortcut: String::new(),
                enabled: false,
                separator: false,
                submenu: false,
                depth: 0,
            });
            values.push(Value::NIL);
        } else if item.is_cons()
            && let Some(label) = popup_menu_string(item.cons_car())
        {
            entries.push(PopupMenuEntry {
                label,
                shortcut: String::new(),
                enabled: true,
                separator: false,
                submenu: false,
                depth: 0,
            });
            values.push(item.cons_cdr());
        }

        rest = rest.cons_cdr();
        depth += 1;
    }

    Some((title, entries, values))
}

fn popup_dialog_position(ctx: &Context, position: Value) -> (FrameId, f32, f32) {
    let frame_id = position
        .as_frame_id()
        .map(FrameId)
        .filter(|id| ctx.frame_manager().get(*id).is_some())
        .or_else(|| ctx.frame_manager().selected_frame().map(|frame| frame.id))
        .unwrap_or(FrameId(0));

    let (x, y) = ctx
        .frame_manager()
        .get(frame_id)
        .map(|frame| (frame.width as f32 / 2.0, frame.height as f32 / 2.0))
        .unwrap_or((0.0, 0.0));

    (frame_id, x, y)
}

pub(crate) fn builtin_x_popup_dialog(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    validate_x_popup_dialog_args(&args)?;

    if ctx.input_rx.is_none() {
        tracing::info!("x-popup-dialog interactive: no input receiver");
        return Ok(Value::NIL);
    }
    if ctx.display_host.is_none() {
        tracing::info!("x-popup-dialog interactive: no display host");
        return Ok(Value::NIL);
    }

    let Some((title, entries, values)) = popup_dialog_from_contents(args[1]) else {
        tracing::info!("x-popup-dialog interactive: malformed dialog contents");
        return Ok(Value::NIL);
    };
    if entries.is_empty() {
        tracing::info!("x-popup-dialog interactive: dialog has no entries");
        return Ok(Value::NIL);
    }

    let (frame_id, x, y) = popup_dialog_position(ctx, args[0]);
    let placement =
        neomacs_display_protocol::PopupPlacement::at(neomacs_display_protocol::Point::new(x, y));
    let visible_rows = ctx
        .display_host
        .as_ref()
        .and_then(|host| host.popup_menu_visible_rows(x, y, entries.len()))
        .unwrap_or(entries.len())
        .min(entries.len());
    if visible_rows == 0 {
        tracing::info!(
            x,
            y,
            entries = entries.len(),
            "x-popup-dialog interactive: host reported zero visible rows"
        );
        return Ok(Value::NIL);
    }

    tracing::info!(
        x,
        y,
        entries = entries.len(),
        visible_rows,
        "x-popup-dialog interactive: showing popup"
    );

    let mut selected = entries
        .iter()
        .position(|entry| entry.enabled && !entry.separator)
        .unwrap_or(0);

    let specpdl_count = ctx.specpdl.len();
    for value in &values {
        ctx.push_specpdl_root(*value);
    }
    ctx.try_specbind_or_unwind_to(
        specpdl_count,
        intern("overriding-terminal-local-map"),
        Value::NIL,
    )?;
    ctx.try_specbind_or_unwind_to(specpdl_count, intern("track-mouse"), Value::NIL)?;

    let result = x_popup_menu_interactive_loop(
        ctx,
        args[0],
        &entries,
        &values,
        visible_rows,
        placement,
        frame_id,
        Some(&title),
        &mut selected,
    );

    let result_root_scope = ctx.save_vm_roots();
    ctx.push_eval_result_roots(&result);
    let _ = ctx.display_host.as_mut().map(|host| host.hide_popup_menu());
    ctx.redisplay_with_force(true);
    ctx.restore_vm_roots(result_root_scope);
    ctx.unbind_to_with_result(specpdl_count, result)
}

fn x_popup_menu_interactive(ctx: &mut Context, position: Value, menu: Value) -> EvalResult {
    // GNU keys submenu rendering off `Vmenu_updating_frame` being a TTY frame
    // (`is_tty_frame`, src/menu.c:407). For `x-popup-menu` that is the selected
    // frame; a TTY frame has no window system (`effective_window_system` =
    // None). On TTY each submenu collapses to one `" >"` line instead of being
    // inlined; on a window-system frame the toolkit owns nested panes.
    let is_tty = selected_frame_window_system_symbol(ctx).is_none();
    let Some((entries, events)) = popup_menu_from_keymap(menu, is_tty, ctx.obarray()) else {
        tracing::info!("x-popup-menu interactive: menu is not a keymap");
        return Ok(Value::NIL);
    };
    if entries.is_empty() {
        tracing::info!("x-popup-menu interactive: menu has no entries");
        return Ok(Value::NIL);
    }
    if ctx.input_rx.is_none() {
        tracing::info!("x-popup-menu interactive: no input receiver");
        return Ok(Value::NIL);
    }
    if ctx.display_host.is_none() {
        tracing::info!("x-popup-menu interactive: no display host");
        return Ok(Value::NIL);
    }

    let popup_position = popup_menu_position(ctx, position);
    let placement = popup_position.placement;
    let estimated_origin = popup_position.estimated_origin();
    let (x, y) = (estimated_origin.x, estimated_origin.y);
    let visible_rows = ctx
        .display_host
        .as_ref()
        .and_then(|host| host.popup_menu_visible_rows(x, y, entries.len()))
        .unwrap_or(entries.len())
        .min(entries.len());
    if visible_rows == 0 {
        tracing::info!(
            x,
            y,
            entries = entries.len(),
            "x-popup-menu interactive: host reported zero visible rows"
        );
        return Ok(Value::NIL);
    }
    tracing::info!(
        x,
        y,
        entries = entries.len(),
        visible_rows,
        "x-popup-menu interactive: showing popup"
    );
    let mut selected = 0;
    let frame_id = ctx
        .frame_manager()
        .selected_frame()
        .map(|frame| frame.id)
        .unwrap_or(FrameId(0));

    let specpdl_count = ctx.specpdl.len();
    for event in &events {
        ctx.push_specpdl_root(*event);
    }
    // Native popup menus are modal in the display layer.  The renderer owns
    // hover/keyboard navigation and reports only committed selections here.
    // Letting the TTY menu map see raw mouse movement can execute menu items
    // from hover alone.
    ctx.try_specbind_or_unwind_to(
        specpdl_count,
        intern("overriding-terminal-local-map"),
        Value::NIL,
    )?;
    ctx.try_specbind_or_unwind_to(specpdl_count, intern("track-mouse"), Value::NIL)?;

    let result = x_popup_menu_interactive_loop(
        ctx,
        position,
        &entries,
        &events,
        visible_rows,
        placement,
        frame_id,
        None,
        &mut selected,
    );

    let result_root_scope = ctx.save_vm_roots();
    ctx.push_eval_result_roots(&result);
    let _ = ctx.display_host.as_mut().map(|host| host.hide_popup_menu());
    ctx.redisplay_with_force(true);
    ctx.restore_vm_roots(result_root_scope);
    ctx.unbind_to_with_result(specpdl_count, result)
}

#[allow(clippy::too_many_arguments)] // popup-loop inputs mirror the display-host boundary
fn x_popup_menu_interactive_loop(
    ctx: &mut Context,
    position: Value,
    entries: &[PopupMenuEntry],
    events: &[Value],
    visible_rows: usize,
    placement: neomacs_display_protocol::PopupPlacement,
    frame_id: FrameId,
    title: Option<&str>,
    selected: &mut usize,
) -> EvalResult {
    show_popup_menu_selection(ctx, frame_id, placement, title, entries, *selected)?;

    loop {
        let (keys, binding) = ctx.read_key_sequence()?;
        if let Some(index) = popup_menu_selection_index(&keys) {
            if index < 0 {
                return Ok(Value::NIL);
            }
            return Ok(events.get(index as usize).copied().unwrap_or(Value::NIL));
        }

        let command = binding
            .as_symbol_name()
            .and_then(TtyMenuNavigationCommand::from_symbol_name)
            .or_else(|| native_popup_navigation_command(&keys));

        match command {
            Some(TtyMenuNavigationCommand::TtyMenuNextItem) => {
                *selected = (*selected + 1).min(visible_rows.saturating_sub(1));
                show_popup_menu_selection(ctx, frame_id, placement, title, entries, *selected)?;
            }
            Some(TtyMenuNavigationCommand::TtyMenuPrevItem) => {
                *selected = (*selected).saturating_sub(1);
                show_popup_menu_selection(ctx, frame_id, placement, title, entries, *selected)?;
            }
            Some(TtyMenuNavigationCommand::TtyMenuNextMenu) => {
                if let Some(new_position) =
                    menu_bar_navigation_position(ctx, position, MenuBarNavigationDirection::Right)
                {
                    return Ok(new_position);
                }
            }
            Some(TtyMenuNavigationCommand::TtyMenuPrevMenu) => {
                if let Some(new_position) =
                    menu_bar_navigation_position(ctx, position, MenuBarNavigationDirection::Left)
                {
                    return Ok(new_position);
                }
            }
            Some(TtyMenuNavigationCommand::TtyMenuSelect) => {
                if popup_menu_entry_selectable(entries, *selected) {
                    return Ok(events.get(*selected).copied().unwrap_or(Value::NIL));
                }
            }
            Some(
                TtyMenuNavigationCommand::TtyMenuExit
                | TtyMenuNavigationCommand::KeyboardQuit
                | TtyMenuNavigationCommand::KeyboardEscapeQuit,
            ) => {
                return Ok(Value::NIL);
            }
            _ => {}
        }
    }
}

fn show_popup_menu_selection(
    ctx: &mut Context,
    frame_id: FrameId,
    placement: neomacs_display_protocol::PopupPlacement,
    title: Option<&str>,
    entries: &[PopupMenuEntry],
    selected: usize,
) -> Result<(), Flow> {
    let Some(host) = ctx.display_host.as_mut() else {
        return Ok(());
    };
    host.show_popup_menu(PopupMenuRequest {
        frame_id,
        placement,
        title: title.map(str::to_owned),
        entries: entries.to_vec(),
        selected,
    })
    .map_err(|err| signal("error", vec![Value::string(err)]))
}

fn popup_menu_entry_selectable(entries: &[PopupMenuEntry], index: usize) -> bool {
    entries
        .get(index)
        .is_some_and(|entry| entry.enabled && !entry.separator)
}

fn popup_menu_selection_index(keys: &[Value]) -> Option<i32> {
    let event = keys.first()?;
    let parts = list_to_vec(event)?;
    if parts.len() != 2 || parts[0].as_symbol_name() != Some("menu-selection") {
        return None;
    }
    Some(parts[1].as_fixnum()? as i32)
}

fn native_popup_navigation_command(keys: &[Value]) -> Option<TtyMenuNavigationCommand> {
    let event = *keys.first()?;
    if let Some(name) = event.as_symbol_name() {
        return match name {
            "down" => Some(TtyMenuNavigationCommand::TtyMenuNextItem),
            "up" => Some(TtyMenuNavigationCommand::TtyMenuPrevItem),
            "right" => Some(TtyMenuNavigationCommand::TtyMenuNextMenu),
            "left" => Some(TtyMenuNavigationCommand::TtyMenuPrevMenu),
            // Named forms of the RET/ESC characters. Since 220938220 ("preserve
            // named GUI key events") a GUI `<return>`/`<escape>` is delivered as
            // the symbol, not char 13/27, and `function-key-map` may not have
            // translated it (e.g. a raw menu read), so treat the symbol the same
            // as its control character below.
            "return" | "kp-enter" => Some(TtyMenuNavigationCommand::TtyMenuSelect),
            "escape" => Some(TtyMenuNavigationCommand::KeyboardEscapeQuit),
            _ => None,
        };
    }

    match event.as_fixnum()? {
        13 => Some(TtyMenuNavigationCommand::TtyMenuSelect),
        27 => Some(TtyMenuNavigationCommand::KeyboardEscapeQuit),
        _ => None,
    }
}

/// Decode the place-to-put-the-menu half of an `x-popup-menu` POSITION whose
/// car is the `(X Y)` cons, exactly as GNU `x_popup_menu_1` does.
///
/// GNU reads the designator from the SECOND element and the coordinates from
/// the first (`src/menu.c:1144-1150`):
///
/// ```c
///     tem = Fcar (position);
///     if (CONSP (tem))
///       { window = Fcar (Fcdr (position)); x = XCAR (tem); y = Fcar (XCDR (tem)); }
/// ```
///
/// When both coordinates are nil it abandons the designator entirely and uses
/// the current mouse position instead (`src/menu.c:1182-1184` plus
/// `1228-1235`, which sets WINDOW to the selected frame), so nothing about the
/// supplied value is checked in that case.  Otherwise the designator may be a
/// FRAME or a LIVE WINDOW, and only something that is neither is a `windowp`
/// error (`src/menu.c:1239-1269`); an internal window fails the liveness check
/// with `window-live-p`.
fn decode_popup_menu_position_window(
    ctx: &mut Context,
    position_car: Value,
    position_cdr: Value,
) -> Result<(), Flow> {
    let x = position_car.cons_car();
    let y = {
        let rest = position_car.cons_cdr();
        if rest.is_cons() {
            rest.cons_car()
        } else {
            Value::NIL
        }
    };
    if x.is_nil() && y.is_nil() {
        // GNU's `get_current_pos_p` path: WINDOW is replaced by the selected
        // frame before the decode below ever runs.
        return Ok(());
    }

    let window = if position_cdr.is_cons() {
        position_cdr.cons_car()
    } else {
        Value::NIL
    };
    if window.is_frame() {
        return Ok(());
    }
    if let Some(id) = window.as_window_id() {
        if ctx.frames.is_live_window_id(crate::window::WindowId(id)) {
            return Ok(());
        }
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), window],
        ));
    }
    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("windowp"), window],
    ))
}

/// Take GNU's two keymap branches for an `x-popup-menu` MENU argument, and
/// report whether one of them applied.
///
/// GNU `x_popup_menu_1` (`src/menu.c:1294-1364`):
///
/// ```c
///   keymap = get_keymap (menu, 0, 0);
///   if (CONSP (keymap)) { keymap_panes (&menu, 1); ... }
///   else if (CONSP (menu) && KEYMAPP (XCAR (menu)))
///     { ... maps[i++] = keymap = get_keymap (XCAR (tem), 1, 0); ... }
///   else { title = Fcar (menu); CHECK_STRING (title); list_of_panes (Fcdr (menu)); }
/// ```
///
/// The list-of-keymaps branch resolves EVERY element with `error = 1`, so a
/// list that starts with a keymap and continues with anything else signals
/// `keymapp` on that element -- not `stringp` on the list.
fn decode_popup_menu_keymap_argument(ctx: &mut Context, menu: Value) -> Result<bool, Flow> {
    if super::keymap::get_keymap_in_runtime(ctx, &menu, false, false)?.is_truthy() {
        return Ok(true);
    }
    if menu.is_cons()
        && super::keymap::get_keymap_in_runtime(ctx, &menu.cons_car(), false, false)?.is_truthy()
    {
        let mut rest = menu;
        while rest.is_cons() {
            super::keymap::get_keymap_in_runtime(ctx, &rest.cons_car(), true, false)?;
            rest = rest.cons_cdr();
        }
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn builtin_x_popup_menu_batch(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("x-popup-menu", &args, 2)?;
    let position = &args[0];
    let menu = &args[1];

    if position.is_nil() {
        return Ok(Value::NIL);
    }

    // GNU `x_popup_menu_1` handles t before decoding list/event positions:
    // it means "use the current mouse position".  The initial batch frame
    // cannot display a menu, but GNU still validates the menu descriptor and
    // then returns nil rather than rejecting the documented sentinel.
    if *position != Value::T {
        let (position_car, position_cdr) = if position.is_cons() {
            (position.cons_car(), position.cons_cdr())
        } else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), *position],
            ));
        };

        if !position_car.is_list() {
            if position_car.is_fixnum() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), position_car],
                ));
            }
            if menu.is_nil() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), Value::NIL],
                ));
            }
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("consp"), Value::T],
            ));
        }

        if !position_cdr.is_list() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), position_cdr],
            ));
        }

        if !position_car.is_nil() {
            decode_popup_menu_position_window(ctx, position_car, position_cdr)?;
        }
    }

    // GNU decodes MENU in three branches (`src/menu.c:1294-1364`), and only the
    // last one requires a string title.  A keymap, or a list of keymaps, is
    // turned into panes by `keymap_panes` and carries its title in the keymap
    // prompt, so `CHECK_STRING (title)` never runs for it.  `imenu` reaches
    // exactly that branch: `imenu--mouse-menu` builds a keymap and
    // `popup-menu` hands it over as `(indirect-function map)`.
    if decode_popup_menu_keymap_argument(ctx, *menu)? {
        return Ok(Value::NIL);
    }

    // The remaining branch is GNU's "old-fashioned menu":
    // MENU = (TITLE . REST), REST either nil or (PANE . _)
    // PANE = (PANE-TITLE . PANE-ITEMS)
    if menu.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), Value::NIL],
        ));
    }

    let (title, rest) = if menu.is_cons() {
        (menu.cons_car(), menu.cons_cdr())
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *menu],
        ));
    };

    if !title.is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), title],
        ));
    }

    if rest.is_nil() {
        return Ok(Value::NIL);
    }

    let pane = if rest.is_cons() {
        rest.cons_car()
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), rest],
        ));
    };

    let (pane_title, pane_items) = if pane.is_cons() {
        (pane.cons_car(), pane.cons_cdr())
    } else if pane.is_nil() {
        (Value::NIL, Value::NIL)
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), pane],
        ));
    };

    if !pane_title.is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), pane_title],
        ));
    }

    if !pane_items.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), pane_items],
        ));
    }

    Ok(Value::NIL)
}

/// (x-popup-menu POSITION MENU) -> selected event, nil, or GNU-compatible error.
pub(crate) fn builtin_x_popup_menu(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("x-popup-menu", &args, 2)?;
    let position = args[0];
    let menu = args[1];

    if ctx.display_host.is_some() && ctx.input_rx.is_some() && super::keymap::is_list_keymap(&menu)
    {
        return x_popup_menu_interactive(ctx, position, menu);
    }

    builtin_x_popup_menu_batch(ctx, args)
}

/// (x-synchronize DISPLAY &optional NO-OP) -> error in batch/no-X context.
pub(crate) fn builtin_x_synchronize(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-synchronize", &args, 1, 2)?;
    Err(x_windows_not_initialized_error())
}

/// (x-translate-coordinates DISPLAY X Y &optional FRAME SOURCE-FRAME) -> error in batch/no-X context.
pub(crate) fn builtin_x_translate_coordinates(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-translate-coordinates", &args, 1, 6)?;
    Err(x_display_query_first_arg_error(&args[0]))
}

/// (x-register-dnd-atom ATOM &optional OLD-ATOM) -> error in batch/no-X context.
pub(crate) fn builtin_x_register_dnd_atom(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-register-dnd-atom", &args, 1, 2)?;
    Err(x_window_system_frame_error())
}

/// (x-export-frames &optional FRAME TYPE) -> error in batch/no-X context.
pub(crate) fn builtin_x_export_frames(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-export-frames", &args, 2)?;
    match args.first() {
        None => Err(x_window_system_frame_error()),
        Some(frame) if frame.is_nil() || frame.is_frame() => Err(x_window_system_frame_error()),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *other],
        )),
    }
}

/// (x-focus-frame FRAME &optional NO-ACTIVATE) -> nil for live GUI frames.
pub(crate) fn builtin_x_focus_frame(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("x-focus-frame", &args, 1, 2)?;
    let fid = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        args.first(),
        "frame-live-p",
    )?;
    if eval
        .frames
        .get(fid)
        .and_then(|frame| frame.effective_window_system())
        .is_some_and(gui_window_system_active_value)
    {
        Ok(Value::NIL)
    } else {
        Err(x_window_system_frame_error())
    }
}

/// (x-get-clipboard) -> nil in batch/no-X context.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_x_get_clipboard(args: Vec<Value>) -> EvalResult {
    expect_args("x-get-clipboard", &args, 0)?;
    Ok(Value::NIL)
}

/// (x-get-modifier-masks &optional DISPLAY) -> error in batch/no-X context.
pub(crate) fn builtin_x_get_modifier_masks(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-get-modifier-masks", &args, 1)?;
    match args.first() {
        None => Err(x_windows_not_initialized_error()),
        Some(display) if display.is_nil() => Err(x_windows_not_initialized_error()),
        Some(v) if v.is_frame() => Err(x_window_system_frame_error()),
        Some(display) => Err(x_display_query_first_arg_error(display)),
    }
}

/// (x-hide-tip) -> nil in batch/no-X context.
pub(crate) fn builtin_x_hide_tip(args: Vec<Value>) -> EvalResult {
    expect_args("x-hide-tip", &args, 0)?;
    Ok(Value::NIL)
}

/// (x-show-tip STRING &optional FRAME PARMS TIMEOUT DX DY) -> error in batch/no-X context.
pub(crate) fn builtin_x_show_tip(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-show-tip", &args, 1, 6)?;
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    Err(x_window_system_frame_error())
}

/// (x-internal-focus-input-context FRAME) -> nil in batch/no-X context.
pub(crate) fn builtin_x_internal_focus_input_context(args: Vec<Value>) -> EvalResult {
    expect_args("x-internal-focus-input-context", &args, 1)?;
    Ok(Value::NIL)
}

/// (x-wm-set-size-hint &optional FRAME) -> error in batch/no-X context.
pub(crate) fn builtin_x_wm_set_size_hint(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-wm-set-size-hint", &args, 1)?;
    match args.first() {
        None => Err(x_window_system_frame_error()),
        Some(frame) if frame.is_nil() => Err(x_window_system_frame_error()),
        Some(v) if v.is_frame() => Err(x_window_system_frame_error()),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *other],
        )),
    }
}

/// (x-backspace-delete-keys-p &optional FRAME) -> error in batch/no-X context.
pub(crate) fn builtin_x_backspace_delete_keys_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-backspace-delete-keys-p", &args, 1)?;
    if let Some(frame) = args.first() {
        expect_optional_window_system_frame_arg(frame)?;
    }
    Err(x_window_system_frame_error())
}

/// (x-family-fonts &optional FAMILY FRAME) -> nil in batch/no-X context.
pub(crate) fn builtin_x_family_fonts(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-family-fonts", &args, 2)?;
    if let Some(frame) = args.get(1) {
        expect_optional_window_system_frame_arg(frame)?;
    }
    if let Some(family) = args.first()
        && !family.is_nil()
        && !family.is_string()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *family],
        ));
    }
    Ok(Value::NIL)
}

/// (x-get-atom-name ATOM &optional FRAME) -> error in batch/no-X context.
pub(crate) fn builtin_x_get_atom_name(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-get-atom-name", &args, 1, 2)?;
    if let Some(frame) = args.get(1) {
        expect_optional_window_system_frame_arg(frame)?;
    }
    Err(x_window_system_frame_error())
}

pub(crate) fn builtin_x_get_resource(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("x-get-resource", &args, 2, 4)?;
    if x_window_system_active(eval) {
        return Ok(Value::NIL);
    }
    Err(window_system_not_initialized_error())
}

pub(crate) fn builtin_x_list_fonts(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("x-list-fonts", &args, 1, 5)?;
    if x_window_system_active(eval) {
        return Ok(Value::NIL);
    }
    Err(window_system_not_initialized_error())
}

/// (x-parse-geometry STRING) -> alist or nil.
pub(crate) fn builtin_x_parse_geometry(args: Vec<Value>) -> EvalResult {
    expect_args("x-parse-geometry", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => {
            let spec = display_string_text(&args[0]).expect("checked string");
            Ok(parse_x_geometry(&spec).unwrap_or(Value::NIL))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )),
    }
}

/// (x-change-window-property PROPERTY VALUE &optional FRAME TYPE FORMAT OUTER-P DELETE-P)
/// -> error in batch/no-X context.
pub(crate) fn builtin_x_change_window_property(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-change-window-property", &args, 2, 7)?;
    if let Some(frame) = args.get(2) {
        expect_optional_window_system_frame_arg(frame)?;
    }
    Err(x_window_system_frame_error())
}

/// (x-delete-window-property PROPERTY &optional FRAME TYPE) -> error in batch/no-X context.
pub(crate) fn builtin_x_delete_window_property(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-delete-window-property", &args, 1, 3)?;
    if let Some(frame) = args.get(1) {
        expect_optional_window_system_frame_arg(frame)?;
    }
    Err(x_window_system_frame_error())
}

/// (x-disown-selection-internal SELECTION &optional TYPE FRAME) -> nil.
pub(crate) fn builtin_x_disown_selection_internal(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-disown-selection-internal", &args, 1, 3)?;
    Ok(Value::NIL)
}

/// (x-get-local-selection &optional SELECTION TYPE) -> nil/error.
pub(crate) fn builtin_x_get_local_selection(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-get-local-selection", &args, 2)?;
    let selection = args.first().cloned().unwrap_or(Value::NIL);
    if !selection.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), selection],
        ));
    }
    Ok(Value::NIL)
}

/// (x-get-selection-internal SELECTION TYPE &optional DATA-TYPE FRAME)
/// -> error in batch/no-X context.
pub(crate) fn builtin_x_get_selection_internal(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-get-selection-internal", &args, 2, 4)?;
    Err(x_selection_unavailable_error())
}

/// (x-own-selection-internal SELECTION VALUE &optional FRAME)
/// -> error in batch/no-X context.
pub(crate) fn builtin_x_own_selection_internal(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-own-selection-internal", &args, 2, 3)?;
    Err(x_selection_unavailable_error())
}

/// (gui-get-selection &optional TYPE DATA-TYPE) -> nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gui_get_selection(args: Vec<Value>) -> EvalResult {
    expect_max_args("gui-get-selection", &args, 2)?;
    Ok(Value::NIL)
}

/// (gui-get-primary-selection) -> error in batch/no-X context.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gui_get_primary_selection(args: Vec<Value>) -> EvalResult {
    expect_args("gui-get-primary-selection", &args, 0)?;
    Err(signal(
        "error",
        vec![Value::string("No selection is available")],
    ))
}

/// (gui-select-text TEXT) -> nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gui_select_text(args: Vec<Value>) -> EvalResult {
    expect_args("gui-select-text", &args, 1)?;
    Ok(Value::NIL)
}

/// (gui-selection-value) -> nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gui_selection_value(args: Vec<Value>) -> EvalResult {
    expect_args("gui-selection-value", &args, 0)?;
    Ok(Value::NIL)
}

/// (gui-set-selection TYPE VALUE) -> nil.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_gui_set_selection(args: Vec<Value>) -> EvalResult {
    expect_args("gui-set-selection", &args, 2)?;
    Ok(Value::NIL)
}

/// (x-selection-exists-p &optional SELECTION TYPE) -> nil in batch/no-X context.
pub(crate) fn builtin_x_selection_exists_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-selection-exists-p", &args, 2)?;
    if let Some(selection) = args.first()
        && !selection.is_nil()
    {
        expect_symbol_key(selection)?;
    }
    Ok(Value::NIL)
}

/// (x-selection-owner-p &optional SELECTION TYPE) -> nil in batch/no-X context.
pub(crate) fn builtin_x_selection_owner_p(args: Vec<Value>) -> EvalResult {
    expect_max_args("x-selection-owner-p", &args, 2)?;
    if let Some(selection) = args.first()
        && !selection.is_nil()
    {
        expect_symbol_key(selection)?;
    }
    Ok(Value::NIL)
}

/// (x-uses-old-gtk-dialog) -> nil
pub(crate) fn builtin_x_uses_old_gtk_dialog(args: Vec<Value>) -> EvalResult {
    expect_args("x-uses-old-gtk-dialog", &args, 0)?;
    Ok(Value::NIL)
}

/// (x-window-property PROPERTY &optional FRAME TYPE DELETE-P VECTOR-RET-P) -> error in batch/no-X context.
pub(crate) fn builtin_x_window_property(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-window-property", &args, 1, 6)?;
    if let Some(frame) = args.get(1) {
        expect_optional_window_system_frame_arg(frame)?;
    }
    Err(x_window_system_frame_error())
}

/// (x-window-property-attributes PROPERTY &optional FRAME TYPE) -> error in batch/no-X context.
pub(crate) fn builtin_x_window_property_attributes(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-window-property-attributes", &args, 1, 3)?;
    if let Some(frame) = args.get(1) {
        expect_optional_window_system_frame_arg(frame)?;
    }
    Err(x_window_system_frame_error())
}

/// Context-aware variant of `x-server-version`.
pub(crate) fn builtin_x_server_version(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-server-version", args)
}

/// Context-aware variant of `x-server-max-request-size`.
pub(crate) fn builtin_x_server_max_request_size(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-server-max-request-size", args)
}

/// Context-aware variant of `x-display-grayscale-p`.
pub(crate) fn builtin_x_display_grayscale_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-grayscale-p", &args)? {
        return Ok(Value::T);
    }
    x_optional_display_query_error_eval(eval, "x-display-grayscale-p", args)
}

/// Context-aware variant of `x-display-backing-store`.
pub(crate) fn builtin_x_display_backing_store(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-display-backing-store", args)
}

/// Context-aware variant of `x-display-color-cells`.
pub(crate) fn builtin_x_display_color_cells(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-color-cells", &args)? {
        return Ok(Value::fixnum(GUI_X_DISPLAY_COLOR_CELLS));
    }
    x_optional_display_query_error_eval(eval, "x-display-color-cells", args)
}

/// Context-aware variant of `x-display-mm-height`.
pub(crate) fn builtin_x_display_mm_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-mm-height", &args)? {
        // The generic `display-mm-height` contract permits nil when the
        // backend cannot report a trustworthy physical size. Winit exposes
        // pixel size and scale factor, but not monitor dimensions in
        // millimeters, so do not invent a physical measurement.
        return Ok(Value::NIL);
    }
    x_optional_display_query_error_eval(eval, "x-display-mm-height", args)
}

/// Context-aware variant of `x-display-mm-width`.
pub(crate) fn builtin_x_display_mm_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-mm-width", &args)? {
        // See `builtin_x_display_mm_height`: unavailable physical monitor
        // dimensions are represented as nil, not as an X-frame error.
        return Ok(Value::NIL);
    }
    x_optional_display_query_error_eval(eval, "x-display-mm-width", args)
}

/// Context-aware variant of `x-display-monitor-attributes-list`.
pub(crate) fn builtin_x_display_monitor_attributes_list(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-display-monitor-attributes-list", args)
}

/// Context-aware variant of `x-display-planes`.
pub(crate) fn builtin_x_display_planes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-planes", &args)? {
        return Ok(Value::fixnum(GUI_X_DISPLAY_PLANES));
    }
    x_optional_display_query_error_eval(eval, "x-display-planes", args)
}

/// Context-aware variant of `x-display-save-under`.
pub(crate) fn builtin_x_display_save_under(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-display-save-under", args)
}

/// Context-aware variant of `x-display-screens`.
pub(crate) fn builtin_x_display_screens(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-display-screens", args)
}

/// Context-aware variant of `x-display-visual-class`.
pub(crate) fn builtin_x_display_visual_class(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-visual-class", &args)? {
        return Ok(Value::symbol(GUI_X_VISUAL_CLASS));
    }
    x_optional_display_query_error_eval(eval, "x-display-visual-class", args)
}

/// Context-aware variant of `x-server-input-extension-version`.
pub(crate) fn builtin_x_server_input_extension_version(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-server-input-extension-version", args)
}

/// Context-aware variant of `x-server-vendor`.
pub(crate) fn builtin_x_server_vendor(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    x_optional_display_query_error_eval(eval, "x-server-vendor", args)
}

/// Context-aware variant of `x-display-set-last-user-time`.
///
/// In batch/no-X context, payload class follows USER-TIME argument designator
/// semantics, including live-frame and terminal handle message mapping.
pub(crate) fn builtin_x_display_set_last_user_time(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("x-display-set-last-user-time", &args, 1, 2)?;
    let query_args: Vec<Value> = args.get(1).cloned().into_iter().collect();
    x_optional_display_query_error_eval(eval, "x-display-set-last-user-time", query_args)
}

pub(crate) fn builtin_x_open_connection(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("x-open-connection", &args, 1, 3)?;
    if x_window_system_active(eval) {
        return Ok(Value::NIL);
    }
    match args[0].kind() {
        ValueKind::Nil => Err(signal(
            "error",
            vec![Value::string("Display nil can’t be opened")],
        )),
        ValueKind::String => {
            let display = display_string_text(&args[0]).expect("checked string");
            Err(signal(
                "error",
                vec![Value::string(format!("Display {display} can’t be opened"))],
            ))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )),
    }
}

/// Context-aware variant of `x-close-connection`.
///
/// Live frame designators map to batch-compatible frame-class errors.
pub(crate) fn builtin_x_close_connection(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("x-close-connection", &args, 1)?;
    if let Some(display) = args.first()
        && live_frame_designator_p(eval, display)
    {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    match args[0].kind() {
        ValueKind::Nil => Err(signal(
            "error",
            vec![Value::string("X windows are not in use or not initialized")],
        )),
        ValueKind::String => {
            let display = display_string_text(&args[0]).expect("checked string");
            Err(signal(
                "error",
                vec![Value::string(format!("Display {display} can’t be opened"))],
            ))
        }
        _ => {
            if let Some(err) = terminal_not_x_display_error(&args[0]) {
                Err(err)
            } else {
                Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("frame-live-p"), args[0]],
                ))
            }
        }
    }
}

/// Context-aware variant of `x-display-pixel-width`.
///
/// Accepts live frame designators and maps them to the same batch/no-X error
/// class as nil/current-display queries.
pub(crate) fn builtin_x_display_pixel_width(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-pixel-width", &args)? {
        return Ok(Value::fixnum(80));
    }
    x_optional_display_query_error_eval(eval, "x-display-pixel-width", args)
}

/// Context-aware variant of `x-display-pixel-height`.
///
/// Accepts live frame designators and maps them to the same batch/no-X error
/// class as nil/current-display queries.
pub(crate) fn builtin_x_display_pixel_height(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if gui_x_query_target_eval(eval, "x-display-pixel-height", &args)? {
        return Ok(Value::fixnum(25));
    }
    x_optional_display_query_error_eval(eval, "x-display-pixel-height", args)
}

// ---------------------------------------------------------------------------
// Monitor attribute builtins
// ---------------------------------------------------------------------------

/// Context-aware variant of `display-monitor-attributes-list`.
///
/// This populates the `frames` slot from the live frame list.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_display_monitor_attributes_list(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "display-monitor-attributes-list", &args)?;

    let _ = super::window_cmds::ensure_selected_frame_id(eval);
    let frames = eval
        .frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::make_frame(fid.0))
        .collect::<Vec<_>>();
    Ok(Value::list(vec![make_monitor_alist(Value::list(frames))]))
}

/// Context-aware variant of `frame-monitor-attributes`.
///
/// This populates the `frames` slot from the live frame list.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_frame_monitor_attributes(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_optional_display_designator_eval(eval, "frame-monitor-attributes", &args)?;

    let _ = super::window_cmds::ensure_selected_frame_id(eval);
    let frames = eval
        .frames
        .frame_list()
        .into_iter()
        .map(|fid| Value::make_frame(fid.0))
        .collect::<Vec<_>>();
    Ok(make_monitor_alist(Value::list(frames)))
}

/// Build a single monitor alist with reasonable default values.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn make_monitor_alist(frames: Value) -> Value {
    // geometry: (x y width height)
    let geometry = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(80),
        Value::fixnum(25),
    ]);

    // workarea: (x y width height)
    let workarea = Value::list(vec![
        Value::fixnum(0),
        Value::fixnum(0),
        Value::fixnum(80),
        Value::fixnum(25),
    ]);

    // mm-size: (width-mm height-mm)
    let mm_size = Value::list(vec![Value::NIL, Value::NIL]);

    make_alist(vec![
        (Value::symbol("geometry"), geometry),
        (Value::symbol("workarea"), workarea),
        (Value::symbol("mm-size"), mm_size),
        (Value::symbol("frames"), frames),
    ])
}
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
