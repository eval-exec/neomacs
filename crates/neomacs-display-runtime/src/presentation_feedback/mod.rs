//! Opt-in native presentation receipts; observations never drive rendering.
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
mod x11;

pub(crate) struct PresentationObserver {
    #[cfg(target_os = "linux")]
    wayland: wayland::PresentationObserver,
    #[cfg(target_os = "linux")]
    x11: x11::Observer,
}
impl PresentationObserver {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            wayland: wayland::PresentationObserver::new(),
            #[cfg(target_os = "linux")]
            x11: x11::Observer::new(),
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn before_present(
        &mut self,
        window: &dyn winit::window::Window,
        device: &wgpu::Device,
        surface: &wgpu::Surface<'_>,
        generation: u64,
        frame: u64,
        layout: neomacs_display_protocol::PresentationId,
        size: (u32, u32),
        scale: f64,
        projected: &[neomacs_display_protocol::input_latency::InputToken],
    ) {
        #[cfg(target_os = "linux")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            match window.window_handle().map(|handle| handle.as_raw()) {
                Ok(RawWindowHandle::Wayland(_)) => self
                    .wayland
                    .before_present(window, frame, layout, size, scale, projected),
                Ok(RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)) => self.x11.before_present(
                    device, surface, generation, frame, layout, size, scale, projected,
                ),
                _ => {}
            }
        }
    }
    #[allow(unused_variables)]
    pub(crate) fn dispatch_deadline(
        &mut self,
        now: neomacs_display_protocol::frame_time::EventTime,
    ) -> Option<neomacs_display_protocol::frame_time::EventTime> {
        #[cfg(target_os = "linux")]
        {
            return self
                .wayland
                .dispatch_deadline(now)
                .into_iter()
                .chain(self.x11.deadline(now))
                .min();
        }
        #[cfg(not(target_os = "linux"))]
        None
    }
    #[allow(unused_variables)]
    pub(crate) fn dispatch_pending<'a>(
        &mut self,
        device: Option<&wgpu::Device>,
        surface: impl FnMut(u64) -> Option<(&'a wgpu::Surface<'static>, u64)>,
    ) {
        #[cfg(target_os = "linux")]
        {
            self.wayland.dispatch_pending();
            if let Some(device) = device {
                self.x11.dispatch(device, surface);
            }
        }
    }
    pub(crate) fn shutdown(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.wayland.shutdown();
            self.x11.shutdown();
        }
    }
}
