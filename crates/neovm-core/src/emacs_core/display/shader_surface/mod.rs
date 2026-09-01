//! Elisp builtins for shader surfaces (`doc/display-engine/SHADER_SURFACES.md`).
//!
//! `neomacs-surface-create` allocates a compositor-rendered GPU texture from
//! user WGSL (or raw RGBA8 pixels) and returns a GC-managed surface handle;
//! the handle is shown inline via a `(surface :id HANDLE :width W :height H)`
//! display property. Dropping the handle without `neomacs-surface-destroy`
//! frees the GPU objects at the next garbage collection (the sweep queues the
//! id, the evaluator drains it — see `TaggedHeap::pending_surface_destroys`).
//! Consumers also accept a plain integer id for backward compatibility.
//! NeoMacs extension — gate uses on `(featurep 'neomacs-surface)`.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use super::error::{EvalResult, signal};
use super::eval::{
    Context, ShaderSurfaceContent, ShaderSurfaceCreateRequest, ShaderSurfaceLanguage,
    ShaderSurfaceUniformInit, SurfaceChannelKind, VideoResolveRequest, VideoResolveSource,
};
use super::image::{image_resolve_request_from_spec, image_scale_environment_for_frame};
use super::value::{Value, list_to_vec};

fn surface_error(message: impl Into<String>) -> super::error::Flow {
    signal("error", vec![Value::string(message.into())])
}

fn plist_get(args: &[Value], key: &str) -> Option<Value> {
    let mut i = 0;
    while i + 1 < args.len() {
        if args[i].as_symbol_name() == Some(key) {
            return Some(args[i + 1]);
        }
        i += 2;
    }
    None
}

fn number_to_f32(value: Value) -> Option<f32> {
    if let Some(int) = value.as_int() {
        Some(int as f32)
    } else {
        value.as_float().map(|float| float as f32)
    }
}

/// Extract a host surface id from a GC-managed surface handle
/// (`neomacs-surface-create`'s return value) or a plain non-negative fixnum
/// (backward compatibility, and the declarative `:channel0` form).
pub(crate) fn surface_id_from_value(value: Value) -> Option<u32> {
    value.as_surface_handle().or_else(|| {
        value
            .as_int()
            .filter(|id| *id >= 0 && *id <= u32::MAX as i64)
            .map(|id| id as u32)
    })
}

fn dimension(value: Option<Value>, key: &str) -> Result<u32, super::error::Flow> {
    let value =
        value.ok_or_else(|| surface_error(format!("neomacs-surface-create: {key} is required")))?;
    let px = number_to_f32(value)
        .filter(|px| px.is_finite() && *px >= 1.0)
        .ok_or_else(|| {
            surface_error(format!(
                "neomacs-surface-create: {key} must be a positive number"
            ))
        })?;
    Ok(px.round() as u32)
}

/// Parse a `(name . VALUE)` uniform entry: VALUE is a number (one component)
/// or a vector of 1..=4 numbers.
fn parse_uniform_entry(entry: Value) -> Result<ShaderSurfaceUniformInit, super::error::Flow> {
    if !entry.is_cons() {
        return Err(surface_error(
            "neomacs-surface-create: :uniforms entries must be (NAME . VALUE) pairs",
        ));
    }
    let name_value = entry.cons_car();
    let name = name_value
        .as_symbol_name()
        .map(str::to_owned)
        .or_else(|| {
            name_value
                .as_lisp_string()
                .and_then(|s| s.as_utf8_str().map(str::to_owned))
        })
        .ok_or_else(|| {
            surface_error("neomacs-surface-create: uniform names must be symbols or strings")
        })?;
    let value = entry.cons_cdr();
    let mut components = [0.0f32; 4];
    let count;
    if let Some(scalar) = number_to_f32(value) {
        components[0] = scalar;
        count = 1u8;
    } else if let Some(elements) = value.as_vector_data() {
        let elements = elements.as_slice();
        if elements.is_empty() || elements.len() > 4 {
            return Err(surface_error(format!(
                "neomacs-surface-create: uniform {name} must have 1..=4 components"
            )));
        }
        for (slot, element) in elements.iter().enumerate() {
            components[slot] = number_to_f32(*element).ok_or_else(|| {
                surface_error(format!(
                    "neomacs-surface-create: uniform {name} components must be numbers"
                ))
            })?;
        }
        count = elements.len() as u8;
    } else {
        return Err(surface_error(format!(
            "neomacs-surface-create: uniform {name} value must be a number or vector"
        )));
    }
    Ok(ShaderSurfaceUniformInit {
        name,
        value: components,
        components: count,
    })
}

/// Resolve a `:channel0` value into `(kind, cache id)`: a surface id
/// (integer), an `(image :file/:data …)` spec resolved through the async
/// image catalog, or a `(video :file/:uri …)` spec resolved through the
/// video host (memoized like the declarative display path). Channel-only
/// videos default `:autoplay` to t — a never-playing channel would sample
/// black forever.
fn resolve_channel_value(
    eval: &mut Context,
    value: Value,
) -> Result<(SurfaceChannelKind, u32), super::error::Flow> {
    if let Some(id) = surface_id_from_value(value) {
        return Ok((SurfaceChannelKind::Surface, id));
    }
    let head = value
        .is_cons()
        .then(|| value.cons_car())
        .and_then(|car| car.as_symbol_name().map(str::to_owned));
    match head.as_deref() {
        Some("image") => {
            let environment = image_scale_environment_for_frame(eval, None).unwrap_or_default();
            let request = image_resolve_request_from_spec(
                &value,
                environment,
                eval.face_table().default_face_colors(),
            )
            .ok_or_else(|| {
                surface_error("neomacs-surface-create: invalid image spec in :channel0")
            })?;
            let catalog = eval
                .display_host
                .as_ref()
                .and_then(|host| host.image_catalog())
                .ok_or_else(|| {
                    surface_error("neomacs-surface-create: no image catalog for :channel0")
                })?;
            let image_id = catalog.lookup(request).placement().image_id();
            Ok((SurfaceChannelKind::Image, image_id.get()))
        }
        Some("video") => {
            let items = list_to_vec(&value).ok_or_else(|| {
                surface_error("neomacs-surface-create: invalid video spec in :channel0")
            })?;
            let mut source = None;
            let mut autoplay = true;
            let mut loop_count = -1i32;
            let mut i = 1usize;
            while i + 1 < items.len() {
                let entry = items[i + 1];
                match items[i].as_symbol_name() {
                    Some(":file") => {
                        source = entry
                            .as_lisp_string()
                            .cloned()
                            .map(VideoResolveSource::File);
                    }
                    Some(":uri") => {
                        source = entry.as_lisp_string().cloned().map(VideoResolveSource::Uri);
                    }
                    Some(":autoplay") => autoplay = !entry.is_nil(),
                    Some(":loop") => {
                        loop_count = if entry.is_nil() {
                            0
                        } else {
                            entry.as_int().map(|n| n as i32).unwrap_or(-1)
                        };
                    }
                    _ => {}
                }
                i += 2;
            }
            let request = VideoResolveRequest {
                source: source.ok_or_else(|| {
                    surface_error(
                        "neomacs-surface-create: :channel0 video spec needs :file or :uri",
                    )
                })?,
                loop_count,
                autoplay,
            };
            let host = eval.display_host.as_ref().ok_or_else(|| {
                surface_error("neomacs-surface-create: no display host for :channel0")
            })?;
            let resolved = host
                .request_video(request)
                .map_err(surface_error)?
                .ok_or_else(|| {
                    surface_error("neomacs-surface-create: video :channel0 unavailable")
                })?;
            Ok((SurfaceChannelKind::Video, resolved.video_id))
        }
        _ => Err(surface_error(
            "neomacs-surface-create: :channel0 must be a surface id, (image …), or (video …)",
        )),
    }
}

/// (neomacs-surface-create &rest PLIST)
///
/// Keys: `:shader WGSL-STRING` or `:pixels UNIBYTE-STRING` (exactly one),
/// `:width N`, `:height N` (required), `:uniforms ALIST`, `:animate BOOL`
/// (default t for shader surfaces). Returns a GC-managed surface handle —
/// when Lisp drops the handle, the next garbage collection frees the GPU
/// objects, so an un-destroyed surface no longer leaks until exit. Signals
/// an error otherwise — including WGSL compile errors with naga diagnostics.
fn create(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if !args.len().is_multiple_of(2) {
        return Err(surface_error(
            "neomacs-surface-create: expected keyword/value pairs",
        ));
    }
    let width = dimension(plist_get(&args, ":width"), ":width")?;
    let height = dimension(plist_get(&args, ":height"), ":height")?;
    let wgsl = plist_get(&args, ":shader").filter(|value| !value.is_nil());
    let glsl = plist_get(&args, ":glsl").filter(|value| !value.is_nil());
    let pixels = plist_get(&args, ":pixels").filter(|value| !value.is_nil());
    let animate = plist_get(&args, ":animate")
        .map(|value| !value.is_nil())
        .unwrap_or(true);
    // `:fps N` caps the animation re-render rate; a non-positive or missing
    // value means uncapped (render at display refresh).
    let fps = plist_get(&args, ":fps")
        .and_then(|value| value.as_fixnum())
        .filter(|n| *n > 0)
        .map(|n| n as u32);
    if wgsl.is_some() && glsl.is_some() {
        return Err(surface_error(
            "neomacs-surface-create: :shader (WGSL) and :glsl are mutually exclusive",
        ));
    }
    let language = if glsl.is_some() {
        ShaderSurfaceLanguage::Glsl
    } else {
        ShaderSurfaceLanguage::Wgsl
    };
    let shader = wgsl.or(glsl);

    let content = match (shader, pixels) {
        (Some(shader), None) => {
            let source = shader
                .as_lisp_string()
                .and_then(|s| s.as_utf8_str().map(str::to_owned))
                .ok_or_else(|| {
                    surface_error("neomacs-surface-create: :shader/:glsl must be a string")
                })?;
            let mut uniforms = Vec::new();
            if let Some(list) = plist_get(&args, ":uniforms").filter(|value| !value.is_nil()) {
                let entries = list_to_vec(&list).ok_or_else(|| {
                    surface_error("neomacs-surface-create: :uniforms must be an alist")
                })?;
                for entry in entries {
                    uniforms.push(parse_uniform_entry(entry)?);
                }
            }
            let channel0 = match plist_get(&args, ":channel0").filter(|value| !value.is_nil()) {
                Some(value) => Some(resolve_channel_value(eval, value)?),
                None => None,
            };
            ShaderSurfaceContent::Shader {
                language,
                source,
                uniforms,
                channel0,
            }
        }
        (None, Some(pixels)) => {
            let data = pixels
                .as_lisp_string()
                .map(|s| s.as_bytes().to_vec())
                .ok_or_else(|| {
                    surface_error(
                        "neomacs-surface-create: :pixels must be a unibyte string of RGBA bytes",
                    )
                })?;
            let expected = width as usize * height as usize * 4;
            if data.len() < expected {
                return Err(surface_error(format!(
                    "neomacs-surface-create: :pixels has {} bytes, need {expected} ({width}x{height} RGBA)",
                    data.len()
                )));
            }
            ShaderSurfaceContent::Pixels { data }
        }
        (Some(_), Some(_)) => {
            return Err(surface_error(
                "neomacs-surface-create: :shader/:glsl and :pixels are mutually exclusive",
            ));
        }
        (None, None) => {
            return Err(surface_error(
                "neomacs-surface-create: one of :shader, :glsl, or :pixels is required",
            ));
        }
    };

    let animate = animate && matches!(content, ShaderSurfaceContent::Shader { .. });
    let request = ShaderSurfaceCreateRequest {
        content,
        width,
        height,
        animate,
        fps,
    };
    let host = eval.display_host.as_ref().ok_or_else(|| {
        surface_error("neomacs-surface-create: no GUI display host in this session")
    })?;
    let id = host.create_shader_surface(request).map_err(surface_error)?;
    Ok(Value::make_surface_handle(id))
}

/// (neomacs-surface-set-uniform ID NAME VALUE)
///
/// ID is a surface handle from `neomacs-surface-create` (or a plain integer
/// id). NAME is the symbol/string used in `:uniforms` at create time; VALUE
/// is a number or a vector of 1..=4 numbers. Cheap: writes a uniform slot,
/// no shader recompile.
fn set_uniform(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let id = surface_id_from_value(args[0]).ok_or_else(|| {
        surface_error("neomacs-surface-set-uniform: ID must be a surface handle or id")
    })?;
    let entry = parse_uniform_entry(Value::cons(args[1], args[2]))?;
    if let Some(host) = eval.display_host.as_ref() {
        host.set_shader_surface_uniform(id, &entry.name, entry.value)
            .map_err(surface_error)?;
    }
    Ok(Value::NIL)
}

/// (neomacs-surface-destroy ID) — free the surface's GPU objects now.
///
/// ID is a surface handle or a plain integer id. Optional with handles — GC
/// frees a dropped handle's surface anyway — but immediate for hot swaps
/// (e.g. the shader playground). If the handle is later swept by GC, the
/// second free of the already-missing id is a render-thread no-op.
fn destroy(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let id = surface_id_from_value(args[0]).ok_or_else(|| {
        surface_error("neomacs-surface-destroy: ID must be a surface handle or id")
    })?;
    if let Some(host) = eval.display_host.as_ref() {
        host.destroy_shader_surface(id).map_err(surface_error)?;
    }
    Ok(Value::NIL)
}

/// (neomacs-surface-available-p) — non-nil when a GUI display host that can
/// render shader surfaces is attached.
fn available(eval: &mut Context, _args: Vec<Value>) -> EvalResult {
    Ok(Value::bool_val(eval.display_host.is_some()))
}

/// (neomacs-frame-shader SOURCE &optional LANGUAGE UNIFORMS)
///
/// Install a full-frame post shader: SOURCE defines `mainImage`, applied
/// over the whole rendered frame (the frame is `iChannel0`; note the frame
/// texture is top-left origin while fragCoord is y-up, so the pixel under
/// fragCoord is at uv `(fragCoord.x, iResolution.y - fragCoord.y) /
/// iResolution.xy`). LANGUAGE is `wgsl` (default) or `glsl`
/// (Shadertoy-dialect). UNIFORMS is a `((NAME . VALUE) …)` alist like
/// `neomacs-surface-create's :uniforms — each entry generates a `u_NAME()`
/// accessor and its slot can be updated live with
/// `neomacs-frame-shader-set-uniform`. nil SOURCE removes the shader.
/// Signals on compile errors.
fn set_frame_shader(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let language = match args.get(1).copied().filter(|value| !value.is_nil()) {
        None => ShaderSurfaceLanguage::Wgsl,
        Some(value) if value.is_symbol_named("wgsl") => ShaderSurfaceLanguage::Wgsl,
        Some(value) if value.is_symbol_named("glsl") => ShaderSurfaceLanguage::Glsl,
        Some(_) => {
            return Err(surface_error(
                "neomacs-frame-shader: LANGUAGE must be `wgsl' or `glsl'",
            ));
        }
    };
    let source = if args[0].is_nil() {
        None
    } else {
        let text = args[0]
            .as_lisp_string()
            .and_then(|s| s.as_utf8_str().map(str::to_owned))
            .ok_or_else(|| surface_error("neomacs-frame-shader: SOURCE must be a string or nil"))?;
        let mut uniforms = Vec::new();
        if let Some(list) = args.get(2).copied().filter(|value| !value.is_nil()) {
            let entries = list_to_vec(&list)
                .ok_or_else(|| surface_error("neomacs-frame-shader: UNIFORMS must be an alist"))?;
            for entry in entries {
                uniforms.push(parse_uniform_entry(entry)?);
            }
        }
        Some((text, language, uniforms))
    };
    let host = eval.display_host.as_ref().ok_or_else(|| {
        surface_error("neomacs-frame-shader: no GUI display host in this session")
    })?;
    host.set_frame_shader(source).map_err(surface_error)?;
    Ok(Value::NIL)
}

/// (neomacs-frame-shader-set-uniform NAME VALUE)
///
/// Update one named uniform on the installed full-frame post shader. NAME
/// was declared in `neomacs-frame-shader's UNIFORMS alist; VALUE is a number
/// or a vector of 1..=4 numbers. Cheap: writes a uniform slot, no shader
/// recompile. Signals an error when no frame shader is installed.
fn set_frame_shader_uniform(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    let entry = parse_uniform_entry(Value::cons(args[0], args[1]))?;
    let host = eval.display_host.as_ref().ok_or_else(|| {
        surface_error("neomacs-frame-shader-set-uniform: no GUI display host in this session")
    })?;
    host.set_frame_shader_uniform(&entry.name, entry.value)
        .map_err(|err| surface_error(format!("neomacs-frame-shader-set-uniform: {err}")))?;
    Ok(Value::NIL)
}
