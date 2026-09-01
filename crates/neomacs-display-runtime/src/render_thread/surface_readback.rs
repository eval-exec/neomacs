use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer, GlyphRowRole};
use image::{Rgba, RgbaImage};
use neomacs_renderer_wgpu::WgpuRenderer;
use std::path::Path;

pub(crate) fn surface_usage_for_debug_readback(
    supported_usages: wgpu::TextureUsages,
    pending: &mut bool,
    continuous_enabled: bool,
) -> wgpu::TextureUsages {
    if !*pending && !continuous_enabled {
        return wgpu::TextureUsages::RENDER_ATTACHMENT;
    }

    if supported_usages.contains(wgpu::TextureUsages::COPY_SRC) {
        tracing::info!("First-frame surface readback enabled (surface usage includes COPY_SRC)");
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
    } else {
        tracing::warn!(
            "NEOMACS_DEBUG_FIRST_FRAME_READBACK requested, but surface COPY_SRC is unsupported"
        );
        *pending = false;
        wgpu::TextureUsages::RENDER_ATTACHMENT
    }
}

pub(crate) fn maybe_log_first_frame_surface_readback(
    pending: &mut bool,
    texture: &wgpu::Texture,
    renderer: &WgpuRenderer,
    frame: &FrameGlyphBuffer,
    width: u32,
    height: u32,
) {
    if !*pending {
        return;
    }

    log_surface_readback(
        "First-frame surface readback",
        texture,
        renderer,
        frame,
        width,
        height,
    );
    *pending = false;
}

pub(crate) fn maybe_log_debug_surface_readback(
    remaining_frames: &mut u32,
    texture: &wgpu::Texture,
    renderer: &WgpuRenderer,
    frame: &FrameGlyphBuffer,
    width: u32,
    height: u32,
) {
    if *remaining_frames == 0 {
        return;
    }

    let label = format!("Debug surface readback (remaining={})", *remaining_frames);
    log_surface_readback(&label, texture, renderer, frame, width, height);
    *remaining_frames -= 1;
}

fn log_surface_readback(
    label: &str,
    texture: &wgpu::Texture,
    renderer: &WgpuRenderer,
    frame: &FrameGlyphBuffer,
    width: u32,
    height: u32,
) {
    let format = renderer.surface_format();
    let bytes_per_pixel = match readback_bytes_per_pixel(format) {
        Some(bytes_per_pixel) => bytes_per_pixel,
        None => {
            tracing::warn!("{label} skipped: unsupported surface format {:?}", format);
            return;
        }
    };

    let padded_bytes_per_row = align_up(
        width.saturating_mul(bytes_per_pixel),
        wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
    );
    let buffer_size = padded_bytes_per_row as u64 * height as u64;
    let readback = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("First Frame Surface Readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = renderer
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("First Frame Surface Readback Encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let submission = renderer.queue().submit(Some(encoder.finish()));

    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    let poll_result = renderer.device().poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(std::time::Duration::from_secs(2)),
    });
    if let Err(err) = poll_result {
        tracing::warn!("{label} poll failed: {:?}", err);
        return;
    }
    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::warn!("{label} map failed: {:?}", err);
            return;
        }
        Err(err) => {
            tracing::warn!("{label} recv failed: {:?}", err);
            return;
        }
    }

    let mapped = match readback.slice(..).get_mapped_range() {
        Ok(mapped) => mapped,
        Err(err) => {
            tracing::warn!("{label} mapped range failed: {err}");
            readback.unmap();
            return;
        }
    };
    let mode_rect = widest_mode_line_rect(frame);
    let mode_band = mode_rect.map(|(_, y, _, rect_height)| {
        let y0 = y.min(height.saturating_sub(1));
        let y1 = (y + rect_height).min(height).max(y0 + 1);
        average_band_rgba(
            &mapped,
            padded_bytes_per_row as usize,
            width,
            format,
            y0,
            y1,
        )
    });
    let bottom_band = average_band_rgba(
        &mapped,
        padded_bytes_per_row as usize,
        width,
        format,
        height.saturating_sub(8),
        height,
    );

    let mut sample_logs = Vec::new();
    if let Some((x, y, rect_width, rect_height)) = mode_rect {
        let safe_width = rect_width.max(1);
        let safe_height = rect_height.max(1);
        let sample_x = (x + safe_width.saturating_mul(3) / 4).min(width.saturating_sub(1));
        let sample_y = (y + safe_height / 2).min(height.saturating_sub(1));
        if let Some(color) = readback_pixel_rgba(
            &mapped,
            padded_bytes_per_row as usize,
            format,
            sample_x,
            sample_y,
        ) {
            sample_logs.push(format!(
                "mode_line_sample=({}, {}) rgba=({}, {}, {}, {})",
                sample_x, sample_y, color[0], color[1], color[2], color[3]
            ));
        }
    }
    if let Some(color) = readback_pixel_rgba(
        &mapped,
        padded_bytes_per_row as usize,
        format,
        width / 2,
        height.saturating_sub(1),
    ) {
        sample_logs.push(format!(
            "bottom_sample=({}, {}) rgba=({}, {}, {}, {})",
            width / 2,
            height.saturating_sub(1),
            color[0],
            color[1],
            color[2],
            color[3]
        ));
    }
    for sample in colorful_glyph_box_logs(
        frame,
        &mapped,
        padded_bytes_per_row as usize,
        width,
        height,
        format,
    ) {
        sample_logs.push(sample);
    }

    let mode_band_log = mode_band
        .flatten()
        .map(|avg| {
            format!(
                "mode_band_avg=({:.1}, {:.1}, {:.1}, {:.1})",
                avg.0, avg.1, avg.2, avg.3
            )
        })
        .unwrap_or_else(|| "mode_band_avg=missing".to_string());
    let bottom_band_log = bottom_band
        .map(|avg| {
            format!(
                "bottom_band_avg=({:.1}, {:.1}, {:.1}, {:.1})",
                avg.0, avg.1, avg.2, avg.3
            )
        })
        .unwrap_or_else(|| "bottom_band_avg=missing".to_string());
    let diagnostic = format!(
        "{label}: format={:?} {} {} {}",
        format,
        mode_band_log,
        bottom_band_log,
        sample_logs.join(" ")
    );
    tracing::info!("{}", diagnostic);
    eprintln!("{}", diagnostic);

    if let Some(path) = std::env::var_os("NEOMACS_DEBUG_SURFACE_READBACK_PNG")
        && let Err(err) = write_surface_readback_png(
            Path::new(&path),
            &mapped,
            padded_bytes_per_row as usize,
            format,
            width,
            height,
        )
    {
        tracing::warn!(
            "{label} failed to write surface readback PNG {}: {}",
            Path::new(&path).display(),
            err
        );
    }

    drop(mapped);
    readback.unmap();
}

fn write_surface_readback_png(
    path: &Path,
    mapped: &[u8],
    padded_bytes_per_row: usize,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let mut image = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = readback_pixel_rgba(mapped, padded_bytes_per_row, format, x, y)
                .ok_or_else(|| {
                    format!("failed to decode pixel at ({}, {}) from {:?}", x, y, format)
                })?;
            image.put_pixel(x, y, Rgba(pixel));
        }
    }

    image
        .save(path)
        .map_err(|err| format!("image save error: {err}"))?;
    tracing::info!("Wrote debug surface readback PNG {}", path.display());
    Ok(())
}

fn widest_mode_line_rect(frame: &FrameGlyphBuffer) -> Option<(u32, u32, u32, u32)> {
    frame
        .glyphs
        .iter()
        .filter_map(|glyph| match glyph {
            FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                row_role,
                ..
            } if *row_role == GlyphRowRole::ModeLine => Some((*x, *y, *width, *height)),
            _ => None,
        })
        .max_by(|lhs, rhs| lhs.2.total_cmp(&rhs.2))
        .map(|(x, y, width, height)| {
            let rect_x = x.max(0.0).floor() as u32;
            let rect_y = y.max(0.0).floor() as u32;
            let rect_w = width.max(1.0).ceil() as u32;
            let rect_h = height.max(1.0).ceil() as u32;
            (rect_x, rect_y, rect_w, rect_h)
        })
}

fn average_band_rgba(
    mapped: &[u8],
    padded_bytes_per_row: usize,
    width: u32,
    format: wgpu::TextureFormat,
    y_start: u32,
    y_end: u32,
) -> Option<(f32, f32, f32, f32)> {
    if y_start >= y_end || width == 0 {
        return None;
    }

    let mut total = [0u64; 4];
    let mut count = 0u64;
    for y in y_start..y_end {
        for x in 0..width {
            let pixel = readback_pixel_rgba(mapped, padded_bytes_per_row, format, x, y)?;
            total[0] += pixel[0] as u64;
            total[1] += pixel[1] as u64;
            total[2] += pixel[2] as u64;
            total[3] += pixel[3] as u64;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }

    Some((
        total[0] as f32 / count as f32,
        total[1] as f32 / count as f32,
        total[2] as f32 / count as f32,
        total[3] as f32 / count as f32,
    ))
}

fn average_box_rgba(
    mapped: &[u8],
    padded_bytes_per_row: usize,
    surface_width: u32,
    surface_height: u32,
    format: wgpu::TextureFormat,
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
) -> Option<(f32, f32, f32, f32)> {
    if x_start >= x_end || y_start >= y_end || surface_width == 0 || surface_height == 0 {
        return None;
    }

    let clamped_x_end = x_end.min(surface_width);
    let clamped_y_end = y_end.min(surface_height);
    if x_start >= clamped_x_end || y_start >= clamped_y_end {
        return None;
    }

    let mut total = [0u64; 4];
    let mut count = 0u64;
    for y in y_start..clamped_y_end {
        for x in x_start..clamped_x_end {
            let pixel = readback_pixel_rgba(mapped, padded_bytes_per_row, format, x, y)?;
            total[0] += pixel[0] as u64;
            total[1] += pixel[1] as u64;
            total[2] += pixel[2] as u64;
            total[3] += pixel[3] as u64;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }

    Some((
        total[0] as f32 / count as f32,
        total[1] as f32 / count as f32,
        total[2] as f32 / count as f32,
        total[3] as f32 / count as f32,
    ))
}

fn colorful_glyph_box_logs(
    frame: &FrameGlyphBuffer,
    mapped: &[u8],
    padded_bytes_per_row: usize,
    surface_width: u32,
    surface_height: u32,
    format: wgpu::TextureFormat,
) -> Vec<String> {
    let mut logs = Vec::new();
    for glyph in frame.glyphs.iter().filter_map(|glyph| match glyph {
        FrameGlyph::Char {
            char,
            x,
            y,
            width,
            height,
            face_id,
            ..
        } if !char.is_whitespace() && !color_is_grayscale(frame.resolved_face(*face_id).fg) => {
            Some((
                *char,
                *x,
                *y,
                *width,
                *height,
                frame.resolved_face(*face_id).fg,
            ))
        }
        _ => None,
    }) {
        if logs.len() >= 4 {
            break;
        }

        let (ch, x, y, width, height, fg) = glyph;
        let x0 = x.max(0.0).floor() as u32;
        let y0 = y.max(0.0).floor() as u32;
        let x1 = (x + width).max(0.0).ceil() as u32;
        let y1 = (y + height).max(0.0).ceil() as u32;
        let avg = average_box_rgba(
            mapped,
            padded_bytes_per_row,
            surface_width,
            surface_height,
            format,
            x0,
            x1,
            y0,
            y1,
        );
        let avg_log = avg
            .map(|rgba| {
                format!(
                    "glyph_box='{}' box=({},{})->({},{}) fg=({},{},{},{}) avg=({:.1},{:.1},{:.1},{:.1})",
                    ch,
                    x0,
                    y0,
                    x1,
                    y1,
                    (fg.r * 255.0).round() as u8,
                    (fg.g * 255.0).round() as u8,
                    (fg.b * 255.0).round() as u8,
                    (fg.a * 255.0).round() as u8,
                    rgba.0,
                    rgba.1,
                    rgba.2,
                    rgba.3,
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "glyph_box='{}' box=({},{})->({},{}) fg=({},{},{},{}) avg=missing",
                    ch,
                    x0,
                    y0,
                    x1,
                    y1,
                    (fg.r * 255.0).round() as u8,
                    (fg.g * 255.0).round() as u8,
                    (fg.b * 255.0).round() as u8,
                    (fg.a * 255.0).round() as u8,
                )
            });
        logs.push(avg_log);
    }
    logs
}

fn color_is_grayscale(color: crate::core::types::Color) -> bool {
    (color.r - color.g).abs() < 0.001 && (color.g - color.b).abs() < 0.001
}

fn readback_pixel_rgba(
    mapped: &[u8],
    padded_bytes_per_row: usize,
    format: wgpu::TextureFormat,
    x: u32,
    y: u32,
) -> Option<[u8; 4]> {
    let bytes_per_pixel = readback_bytes_per_pixel(format)? as usize;
    let row_offset = y as usize * padded_bytes_per_row;
    let pixel_offset = row_offset + x as usize * bytes_per_pixel;
    let pixel = mapped.get(pixel_offset..pixel_offset + bytes_per_pixel)?;
    match format {
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
            Some([pixel[2], pixel[1], pixel[0], pixel[3]])
        }
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
            Some([pixel[0], pixel[1], pixel[2], pixel[3]])
        }
        _ => None,
    }
}

fn readback_bytes_per_pixel(format: wgpu::TextureFormat) -> Option<u32> {
    match format {
        wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Bgra8UnormSrgb
        | wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb => Some(4),
        _ => None,
    }
}

fn align_up(value: u32, alignment: u32) -> u32 {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}
