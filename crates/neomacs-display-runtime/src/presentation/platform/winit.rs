//! Winit-owned popup surfaces on the GUI event-loop thread.

use neomacs_display_protocol::{PopupConstraintPolicy, PopupPlacement, PopupPreferredSide};
use std::sync::Arc;
use winit::dpi::{LogicalPosition, LogicalSize, Position, Size};
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{
    Window, WindowAnchor, WindowAttributes, WindowConstraintAdjustment, WindowGravity,
    WindowPositioner, WindowType,
};

use super::super::surface::PopupSurface;

impl PopupSurface {
    pub(in crate::presentation) fn create(
        event_loop: &dyn ActiveEventLoop,
        parent: Arc<dyn Window>,
        placement: PopupPlacement,
        extent: (f32, f32),
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        role: super::super::PopupRole,
    ) -> Result<Self, String> {
        if cfg!(any(target_os = "android", target_arch = "wasm32")) {
            return Err("native popup presentation is not implemented on this platform".into());
        }
        let scale = parent.scale_factor().max(f64::EPSILON) as f32;
        let gpu_limit = device.limits().max_texture_dimension_2d as f32 / scale;
        let screen_height = parent
            .current_monitor()
            .and_then(|monitor| monitor.current_video_mode())
            .map_or(gpu_limit, |mode| mode.size().height as f32 / scale);
        // A long menu gets a scrollable native viewport, not an oversized GPU
        // allocation. This limit is the output/device extent, never the editor.
        let extent = (
            extent.0.min(gpu_limit).max(1.0),
            extent.1.min(screen_height).min(gpu_limit).max(1.0),
        );
        let attrs = WindowAttributes::default()
            .with_title(match role {
                super::super::PopupRole::Menu => "Neomacs menu",
                super::super::PopupRole::Tooltip => "Neomacs tooltip",
            })
            .with_surface_size(LogicalSize::new(extent.0.max(1.0), extent.1.max(1.0)))
            .with_decorations(false)
            .with_active(role == super::super::PopupRole::Menu)
            .with_window_type(WindowType::Popup)
            .with_positioner(native_positioner(placement));
        let handle = parent.window_handle().map_err(|e| e.to_string())?.as_raw();
        // SAFETY: _parent retains the parent window until after this popup is dropped.
        let attrs = unsafe { attrs.with_parent_window(Some(handle)) };
        let window: Arc<dyn Window> =
            Arc::from(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        if role == super::super::PopupRole::Tooltip {
            window
                .set_cursor_hittest(false)
                .map_err(|e| e.to_string())?;
        }
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(adapter);
        if !caps.formats.contains(&format) {
            return Err("menu surface does not support the renderer's texture format".into());
        }
        let size = window.surface_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(device, &config);
        window.request_redraw();
        tracing::info!(window_id = ?window.id(), parent_id = ?parent.id(), ?role, "created native popup");
        Ok(Self {
            surface,
            window,
            _parent: parent,
            config,
            submitted: false,
        })
    }
}

fn native_positioner(placement: PopupPlacement) -> WindowPositioner {
    let anchor = placement.anchor();
    let offset = placement.offset();
    let adjustment = match placement.constraint() {
        PopupConstraintPolicy::None => WindowConstraintAdjustment::empty(),
        PopupConstraintPolicy::Shift { .. } => {
            WindowConstraintAdjustment::SLIDE_X
                | WindowConstraintAdjustment::SLIDE_Y
                | WindowConstraintAdjustment::RESIZE_Y
        }
        PopupConstraintPolicy::FlipAndShift { .. } => {
            WindowConstraintAdjustment::SLIDE_X
                | WindowConstraintAdjustment::SLIDE_Y
                | WindowConstraintAdjustment::FLIP_X
                | WindowConstraintAdjustment::FLIP_Y
                | WindowConstraintAdjustment::RESIZE_Y
        }
    };
    // Frame-viewport padding is not a global screen margin. Native popup
    // constraints use the compositor's usable area; never apply the old
    // editor-viewport inset here.
    let (edge, gravity) = match placement.preferred_side() {
        PopupPreferredSide::AtAnchor => (WindowAnchor::TopLeft, WindowGravity::BottomRight),
        PopupPreferredSide::Below => (WindowAnchor::BottomLeft, WindowGravity::BottomRight),
        PopupPreferredSide::Above => (WindowAnchor::TopLeft, WindowGravity::TopRight),
        PopupPreferredSide::Right => (WindowAnchor::TopRight, WindowGravity::BottomRight),
        PopupPreferredSide::Left => (WindowAnchor::TopLeft, WindowGravity::BottomLeft),
    };
    WindowPositioner::new(
        edge,
        (
            Position::Logical(LogicalPosition::new(anchor.x as f64, anchor.y as f64)),
            Size::Logical(LogicalSize::new(
                anchor.width.max(1.0) as f64,
                anchor.height.max(1.0) as f64,
            )),
        ),
        Position::Logical(LogicalPosition::new(offset.x as f64, offset.y as f64)),
        gravity,
        adjustment,
    )
}

impl PopupSurface {
    pub(in crate::presentation) fn reposition(&self, placement: PopupPlacement) {
        self.window.set_positioner(native_positioner(placement));
    }
}
