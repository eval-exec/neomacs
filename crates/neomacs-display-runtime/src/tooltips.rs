//! Passive native tooltip lifetime. Callers own help selection and delay policy.
use crate::presentation::{PopupHost, PopupRole};
use neomacs_display_protocol::tooltip::TooltipRequest;
use neomacs_display_protocol::{
    Point, PopupConstraintPolicy, PopupPlacement, PopupPreferredSide, Rect,
};
use neomacs_renderer_wgpu::{TooltipLayout, WgpuGlyphAtlas, WgpuRenderer};
use std::{sync::Arc, time::Instant};
use winit::{
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

pub(crate) struct TooltipOwner {
    pub frame: u64,
    pub parent: Arc<dyn Window>,
    pub anchor: Rect,
    pub metrics: (f32, f32, f32),
    pub fonts: neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer,
}

struct ReadyTooltip {
    owner: TooltipOwner,
    request: TooltipRequest,
    expires: Instant,
    source: TooltipSource,
}

pub(crate) enum TooltipSource {
    Lisp(neomacs_display_protocol::tooltip::TooltipTicket),
    NativeMenu,
}

impl TooltipSource {
    fn is_current(&self) -> bool {
        match self {
            Self::Lisp(ticket) => ticket.is_current(),
            Self::NativeMenu => true,
        }
    }
}

fn placement(owner: &TooltipOwner, request: &TooltipRequest) -> PopupPlacement {
    PopupPlacement::new(
        owner.anchor,
        PopupPreferredSide::Below,
        Point::new(request.offset.0 as f32, request.offset.1 as f32),
        PopupConstraintPolicy::FlipAndShift { padding: 0.0 },
    )
}

impl Drop for ReadyTooltip {
    fn drop(&mut self) {
        if let TooltipSource::Lisp(ticket) = &self.source {
            ticket.cancel();
        }
    }
}

// A mapped tooltip owns all native/GPU resources together. Drop its child
// surface before releasing the retained parent.
struct MappedTooltip {
    host: PopupHost,
    paint: TooltipLayout,
    atlas: WgpuGlyphAtlas,
    ready: ReadyTooltip,
}

#[derive(Default)]
enum Presentation {
    #[default]
    Hidden,
    Ready(ReadyTooltip),
    Mapped(Box<MappedTooltip>),
}

#[derive(Default)]
pub(crate) struct Tooltips {
    state: Presentation,
}

impl Tooltips {
    pub fn show(
        &mut self,
        owner: TooltipOwner,
        request: TooltipRequest,
        source: TooltipSource,
        now: Instant,
    ) {
        if !source.is_current() {
            return;
        }
        let expires = now.checked_add(request.timeout).unwrap_or(now);
        if let Presentation::Mapped(mapped) = &mut self.state {
            if mapped.ready.owner.parent.id() == owner.parent.id()
                && mapped.ready.request.same_content(&request)
            {
                mapped.ready.expires = expires;
                if mapped.ready.owner.anchor != owner.anchor
                    || mapped.ready.request.offset != request.offset
                {
                    mapped.host.reposition(0, placement(&owner, &request));
                }
                mapped.ready.owner = owner;
                mapped.ready.request = request;
                if let TooltipSource::Lisp(ticket) = &mapped.ready.source {
                    ticket.cancel();
                }
                if let TooltipSource::Lisp(ticket) = &source
                    && mapped.host.mapped_window(0).is_some()
                {
                    ticket.mark_visible();
                }
                mapped.ready.source = source;
                return;
            }
        }
        self.state = Presentation::Ready(ReadyTooltip {
            owner,
            request,
            expires,
            source,
        });
    }

    pub fn hide(&mut self) -> bool {
        let visible = matches!(self.state, Presentation::Mapped(_));
        self.state = Presentation::Hidden;
        visible
    }

    fn ready(&self) -> Option<&ReadyTooltip> {
        match &self.state {
            Presentation::Hidden => None,
            Presentation::Ready(ready) => Some(ready),
            Presentation::Mapped(mapped) => Some(&mapped.ready),
        }
    }

    pub fn owner(&self) -> Option<u64> {
        self.ready().map(|r| r.owner.frame)
    }
    pub fn owns_parent(&self, id: WindowId) -> bool {
        self.ready().is_some_and(|r| r.owner.parent.id() == id)
    }
    pub fn dismiss(&mut self, ticket: &neomacs_display_protocol::tooltip::TooltipTicket) {
        if self.ready().is_some_and(
            |r| matches!(&r.source, TooltipSource::Lisp(current) if current.same_request(ticket)),
        ) {
            self.hide();
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.ready().map(|r| r.expires)
    }

    pub fn sync(
        &mut self,
        now: Instant,
        event_loop: &dyn ActiveEventLoop,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        if self.deadline().is_some_and(|d| d <= now) {
            self.hide();
        }
        if self.ready().is_some_and(|r| !r.source.is_current()) {
            self.hide();
        }
        self.state = match std::mem::take(&mut self.state) {
            Presentation::Ready(ready) => Presentation::Mapped(Box::new(MappedTooltip::create(
                ready, event_loop, instance, adapter, device, queue, format,
            )?)),
            state @ (Presentation::Hidden | Presentation::Mapped(_)) => state,
        };
        Ok(())
    }

    pub fn event(
        &mut self,
        id: WindowId,
        event: &WindowEvent,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut WgpuRenderer,
    ) -> bool {
        let Presentation::Mapped(mapped) = &mut self.state else {
            return false;
        };
        let Some(depth) = mapped.host.depth(id) else {
            return false;
        };
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.hide();
            }
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                let size = match event {
                    WindowEvent::SurfaceResized(size) => Some(*size),
                    _ => None,
                };
                let geometry = mapped.host.resize(depth, device, size);
                mapped.remeasure(
                    geometry.device_scale().get(),
                    geometry.logical_size().width(),
                    geometry.logical_size().height(),
                    device,
                    queue,
                );
            }
            WindowEvent::RedrawRequested => {
                mapped.host.draw(depth, device, queue, |target| {
                    renderer.render_native_tooltip(target, &mapped.paint, &mut mapped.atlas)
                });
                if mapped.host.mapped_window(depth).is_some() {
                    if let TooltipSource::Lisp(ticket) = &mapped.ready.source {
                        ticket.mark_visible();
                    }
                }
            }
            _ => {}
        }
        true
    }
}

impl MappedTooltip {
    fn remeasure(
        &mut self,
        scale: f32,
        width: f32,
        height: f32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.atlas.set_scale_factor(scale);
        self.paint = TooltipLayout::measure_with_atlas(
            &self.ready.request,
            self.ready.owner.metrics.2,
            self.ready.owner.metrics.1,
            scale,
            &mut self.atlas,
            device,
            queue,
        );
        self.paint.fit_surface(width, height);
    }

    fn create(
        ready: ReadyTooltip,
        event_loop: &dyn ActiveEventLoop,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let request = &ready.request;
        let owner = &ready.owner;
        let mut atlas = WgpuGlyphAtlas::new_with_scale(device, owner.parent.scale_factor() as f32);
        atlas.set_metrics(owner.metrics.0, owner.metrics.1);
        let mut bindings =
            neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer::with_size(0.0, 0.0);
        bindings.clone_font_bindings_from(&owner.fonts);
        for run in &request.runs {
            bindings.faces.insert(run.face.id, run.face.clone());
            if let Some(font) = &run.font {
                bindings.fonts.insert(font.id, font.clone());
            }
        }
        atlas.set_current_frame_fonts(bindings.font_bindings());
        let paint = TooltipLayout::measure_with_atlas(
            request,
            owner.metrics.2,
            owner.metrics.1,
            owner.parent.scale_factor() as f32,
            &mut atlas,
            device,
            queue,
        );
        let (width, height) = paint.extent();
        let placement = placement(owner, request);
        let mut host = PopupHost::default();
        let geometry = host
            .open_with_role(
                event_loop,
                owner.parent.clone(),
                placement,
                (width, height),
                instance,
                adapter,
                device,
                format,
                PopupRole::Tooltip,
            )?
            .ok_or("tooltip owner is not mapped")?;
        let size = geometry.logical_size();
        let mut mapped = Self {
            host,
            paint,
            atlas,
            ready,
        };
        mapped.remeasure(
            geometry.device_scale().get(),
            size.width(),
            size.height(),
            device,
            queue,
        );
        Ok(mapped)
    }
}
