//! Lisp tooltip validation. Native windows and timers belong to DisplayHost.
use super::*;
use neomacs_display_protocol::tooltip::TooltipRequest;
use strum::EnumString;

#[derive(Clone, Copy, Debug, EnumString)]
#[strum(serialize_all = "kebab-case")]
enum TooltipParameter {
    ForegroundColor,
    BackgroundColor,
    BorderColor,
    BorderWidth,
    InternalBorderWidth,
    Left,
    Top,
    Right,
    Bottom,
}

impl TooltipParameter {
    fn apply(self, value: Value, request: &mut TooltipRequest) -> Result<(), Flow> {
        fn color(value: Value) -> Result<u32, Flow> {
            let name = value.as_str_owned().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), value],
                )
            })?;
            crate::face::Color::from_hex(&name)
                .or_else(|| crate::face::Color::from_name(&name))
                .map(|color| color.to_pixel())
                .ok_or_else(|| {
                    signal(
                        "error",
                        vec![Value::string(format!("Undefined color: {name}"))],
                    )
                })
        }
        fn width(value: Value) -> Result<u32, Flow> {
            value
                .as_fixnum()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| {
                    signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("natnump"), value],
                    )
                })
        }
        match self {
            Self::ForegroundColor => request.foreground = Some(color(value)?),
            Self::BackgroundColor => request.background = Some(color(value)?),
            Self::BorderColor => request.border = Some(color(value)?),
            Self::BorderWidth => request.border_width = width(value)?,
            Self::InternalBorderWidth => request.padding = width(value)?,
            Self::Left | Self::Top | Self::Right | Self::Bottom => {
                if !value.is_nil() {
                    return Err(signal(
                        "error",
                        vec![Value::string(
                            "Absolute desktop tooltip placement is not supported; use pointer-relative offsets",
                        )],
                    ));
                }
            }
        }
        Ok(())
    }
}

pub(super) fn menu_tooltips(
    ctx: &mut Context,
    frame: FrameId,
) -> Option<neomacs_display_protocol::tooltip::MenuTooltips> {
    if !ctx
        .display_host
        .as_ref()
        .is_some_and(|h| h.owns_native_menu_tooltips())
        || ctx.visible_variable_value_or_nil("tooltip-mode").is_nil()
    {
        return None;
    }
    ctx.sync_runtime_faces_for_frame(frame);
    let face = ctx.face_table().resolve("tooltip");
    let duration = |name: &str, fallback: f64| {
        let value = ctx.visible_variable_value_or_nil(name);
        let seconds = value
            .as_fixnum()
            .map(|n| n as f64)
            .or_else(|| value.as_float())
            .unwrap_or(fallback);
        std::time::Duration::try_from_secs_f64(seconds)
            .unwrap_or(std::time::Duration::from_secs_f64(fallback))
    };
    Some(neomacs_display_protocol::tooltip::MenuTooltips {
        appearance: TooltipRequest {
            offset: (5, 20),
            timeout: duration("tooltip-hide-delay", 10.0),
            foreground: face.foreground.map(|c| c.to_pixel()),
            background: face.background.map(|c| c.to_pixel()),
            ..Default::default()
        },
        delay: duration("tooltip-delay", 0.7),
        short_delay: duration("tooltip-short-delay", 0.1),
        recent: duration("tooltip-recent-seconds", 1.0),
    })
}

pub(crate) fn builtin_x_show_tip_eval(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("x-show-tip", &args, 1, 6)?;
    let text = args[0]
        .as_str_owned()
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            )
        })?
        .to_owned();
    let frame = super::super::window_cmds::resolve_frame_id_in_state(
        &mut ctx.frames,
        &mut ctx.buffers,
        args.get(1),
        "frame-live-p",
    )?;
    if !ctx
        .frames
        .get(frame)
        .and_then(|f| f.effective_window_system())
        .is_some_and(gui_window_system_active_value)
    {
        return Err(x_window_system_frame_error());
    }
    let mut request = TooltipRequest {
        text: if text.is_empty() { " ".into() } else { text },
        ..Default::default()
    };
    ctx.sync_runtime_faces_for_frame(frame);
    let face = ctx.face_table().resolve("tooltip");
    request.foreground = face.foreground.map(|c| c.to_pixel());
    request.background = face.background.map(|c| c.to_pixel());
    if !request.text.is_empty() {
        let generation = crate::emacs_core::textprop::builtin_get_text_property_3(
            ctx,
            Value::fixnum(0),
            Value::symbol("neomacs-tooltip-generation"),
            args[0],
        )?;
        request.generation = generation
            .as_fixnum()
            .map(|raw| neomacs_display_protocol::tooltip::TooltipGeneration::from_raw(raw as u64));
    }
    let integer = |value: Value, natural: bool| -> Result<i64, Flow> {
        value
            .as_fixnum()
            .filter(|v| !natural || *v >= 0)
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![
                        Value::symbol(if natural { "natnump" } else { "integerp" }),
                        value,
                    ],
                )
            })
    };
    let timeout = args
        .get(3)
        .copied()
        .filter(|v| !v.is_nil())
        .unwrap_or_else(|| ctx.visible_variable_value_or_nil("x-show-tooltip-timeout"));
    if !timeout.is_nil() {
        request.timeout = std::time::Duration::from_secs(integer(timeout, true)? as u64);
    }
    for (index, offset) in [(4, &mut request.offset.0), (5, &mut request.offset.1)] {
        if let Some(value) = args.get(index).copied().filter(|v| !v.is_nil()) {
            *offset = i32::try_from(integer(value, false)?)
                .map_err(|_| signal("error", vec![Value::string("Tooltip offset out of range")]))?;
        }
    }
    let mut params = args.get(2).copied().unwrap_or(Value::NIL);
    while params.is_cons() {
        let entry = params.cons_car();
        if entry.is_cons() {
            if let Some(parameter) = entry
                .cons_car()
                .as_symbol_name()
                .and_then(|name| name.parse::<TooltipParameter>().ok())
            {
                parameter.apply(entry.cons_cdr(), &mut request)?;
            }
        }
        params = params.cons_cdr();
    }
    let size = ctx.visible_variable_value_or_nil("x-max-tooltip-size");
    if size.is_cons() {
        if let (Some(w), Some(h)) = (size.cons_car().as_fixnum(), size.cons_cdr().as_fixnum()) {
            if w > 0 && h > 0 {
                request.max_size = neomacs_display_protocol::tooltip::TooltipLimits::new(
                    w.min(u32::MAX as i64) as u32,
                    h.min(u32::MAX as i64) as u32,
                );
            }
        }
    }
    request.runs = text_runs(ctx, frame, args[0], &request)?;
    let host = ctx
        .display_host
        .as_mut()
        .ok_or_else(x_window_system_frame_error)?;
    host.show_tooltip(frame, request)
        .map_err(|e| signal("error", vec![Value::string(e)]))?;
    Ok(Value::NIL)
}

fn text_runs(
    ctx: &mut Context,
    frame: FrameId,
    text: Value,
    request: &TooltipRequest,
) -> Result<Vec<neomacs_display_protocol::tooltip::TooltipTextRun>, Flow> {
    use neomacs_display_protocol::{Color, FaceAttributes, FaceId};
    let length = text.as_lisp_string().unwrap().schars();
    if length == 0 {
        return Ok(Vec::new());
    }
    // Include gaps as well as explicit intervals. Interval endpoints, not a
    // property lookup per character, bound the amount of Lisp-side work.
    let mut endpoints = vec![0, length];
    for run in get_string_text_properties_for_value(text).unwrap_or_default() {
        endpoints.extend([run.start.min(length), run.end.min(length)]);
    }
    endpoints.sort_unstable();
    endpoints.dedup();
    let mut base = ctx.face_table().resolve("tooltip");
    let color = |c: u32| crate::face::Color::rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    base.foreground = request.foreground.map(color);
    base.background = request.background.map(color);
    let mut runs = Vec::new();
    for (index, span) in endpoints.windows(2).enumerate() {
        let reference = crate::emacs_core::textprop::builtin_get_text_property_3(
            ctx,
            Value::fixnum(span[0] as i64),
            Value::symbol("face"),
            text,
        )?;
        let face = base.merge(&ctx.face_table().resolve_reference(reference));
        let mut paint =
            neomacs_display_protocol::face::Face::new(FaceId::new(0x7000_0000 + index as u32));
        paint.foreground = Color::from_pixel(face.foreground.map_or(0, |c| c.to_pixel()));
        paint.background = Color::from_pixel(face.background.map_or(0xffffe0, |c| c.to_pixel()));
        if face.inverse_video == Some(true) {
            std::mem::swap(&mut paint.foreground, &mut paint.background);
        }
        paint.font_weight = face.weight.map_or(400, |w| w.css_weight());
        paint
            .attributes
            .set(FaceAttributes::BOLD, paint.font_weight >= 700);
        paint.attributes.set(
            FaceAttributes::ITALIC,
            face.slant
                .is_some_and(|s| s != crate::face::FontSlant::Normal),
        );
        if let Some(underline) = face.underline.enabled() {
            paint.attributes.insert(FaceAttributes::UNDERLINE);
            paint.underline_style = match underline.style {
                crate::face::UnderlineStyle::Line => neomacs_display_protocol::UnderlineStyle::Line,
                crate::face::UnderlineStyle::DoubleLine => {
                    neomacs_display_protocol::UnderlineStyle::Double
                }
                crate::face::UnderlineStyle::Wave => neomacs_display_protocol::UnderlineStyle::Wave,
                crate::face::UnderlineStyle::Dots => {
                    neomacs_display_protocol::UnderlineStyle::Dotted
                }
                crate::face::UnderlineStyle::Dashes => {
                    neomacs_display_protocol::UnderlineStyle::Dashed
                }
            };
            paint.underline_color = underline.color.map(|c| Color::from_pixel(c.to_pixel()));
        }
        paint
            .attributes
            .set(FaceAttributes::OVERLINE, face.overline == Some(true));
        paint.attributes.set(
            FaceAttributes::STRIKE_THROUGH,
            face.strike_through == Some(true),
        );
        let font = if let Some(host) = ctx.display_host.as_mut() {
            host.resolve_frame_font(
                frame,
                crate::emacs_core::display_host::FrameFontRequest::from_face(face),
            )
            .map_err(|e| signal("error", vec![Value::string(e)]))?
            .map(|resolved| resolved.font.resolved)
        } else {
            None
        };
        if let Some(font) = &font {
            paint.default_resolved_font_id = Some(font.id);
            paint.font_family = font.family.clone();
            paint.font_size = font.pixel_size;
            paint.font_ascent = font.ascent_px as i32;
            paint.font_descent = font.descent_px as i32;
        }
        runs.push(neomacs_display_protocol::tooltip::TooltipTextRun {
            range: span[0]..span[1],
            face: paint,
            font,
        });
    }
    Ok(runs)
}

pub(crate) fn builtin_x_hide_tip_eval(ctx: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("x-hide-tip", &args, 0)?;
    let Some(host) = ctx.display_host.as_mut() else {
        return Ok(Value::NIL);
    };
    let hidden = host
        .hide_tooltip()
        .map_err(|e| signal("error", vec![Value::string(e)]))?;
    Ok(if hidden { Value::T } else { Value::NIL })
}

#[cfg(test)]
#[path = "tests/tooltip_test.rs"]
mod tests;
