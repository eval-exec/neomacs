//! Xlib target defaults, captured once without retaining native handles.
use super::{SelectedLinuxBackend, X11_DISPLAY_PROBE_TIMEOUT, query_x11_font_resources};
use crate::display_raster::DisplayRasterOptions;
use neomacs_font_materializer::fontconfig::{
    SurfaceFontOptions, SurfaceSubpixelOrder, XftResources,
};
use std::time::Duration;

#[derive(Default)]
pub(super) struct Resources {
    pub(super) antialias: Option<String>,
    pub(super) hinting: Option<String>,
    pub(super) hintstyle: Option<String>,
    pub(super) rgba: Option<String>,
    pub(super) lcdfilter: Option<String>,
    pub(super) subpixel_order: i32,
}

impl Resources {
    fn options(&self) -> SurfaceFontOptions {
        SurfaceFontOptions::from_xlib_resources(
            XftResources {
                antialias: self.antialias.as_deref(),
                hinting: self.hinting.as_deref(),
                hintstyle: self.hintstyle.as_deref(),
                rgba: self.rgba.as_deref(),
                lcdfilter: self.lcdfilter.as_deref(),
            },
            SurfaceSubpixelOrder::from_xrender(self.subpixel_order),
        )
    }
}

pub(super) fn observe(backend: SelectedLinuxBackend) -> DisplayRasterOptions {
    select(backend, || {
        bounded(X11_DISPLAY_PROBE_TIMEOUT, query_x11_font_resources)
    })
}

fn select(
    backend: SelectedLinuxBackend,
    query: impl FnOnce() -> SurfaceFontOptions,
) -> DisplayRasterOptions {
    match backend {
        SelectedLinuxBackend::X11 => DisplayRasterOptions::X11(query()),
        SelectedLinuxBackend::Wayland => DisplayRasterOptions::Wayland,
    }
}

fn bounded(
    timeout: Duration,
    query: impl FnOnce() -> Resources + Send + 'static,
) -> SurfaceFontOptions {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("x11-font-raster-probe".into())
        .spawn(move || {
            let _ = tx.send(query());
        });
    let resources = if spawned.is_ok() {
        rx.recv_timeout(timeout).unwrap_or_else(|error| {
            tracing::warn!(%error, "X11 raster probe unavailable; using Xlib defaults");
            Resources::default()
        })
    } else {
        tracing::warn!("cannot start X11 raster probe; using Xlib defaults");
        Resources::default()
    };
    // Even failed/timed-out discovery is an Xlib target: missing resources
    // mean Cairo's Xlib defaults, not the image/Wayland DEFAULT options.
    // A timed-out worker owns its connection until it can finish and close it.
    resources.options()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_wayland_never_reads_x11_resources() {
        assert_eq!(
            select(SelectedLinuxBackend::Wayland, || panic!("X11 query")),
            DisplayRasterOptions::Wayland
        );
    }

    #[test]
    fn selected_x11_preserves_owned_resource_values_and_screen_order() {
        let resources = Resources {
            hintstyle: Some("hintslight".into()),
            antialias: Some("true".into()),
            rgba: Some("bgr".into()),
            subpixel_order: 1,
            ..Resources::default()
        };
        let expected = SurfaceFontOptions::from_xlib_resources(
            XftResources {
                hintstyle: Some("hintslight"),
                antialias: Some("true"),
                rgba: Some("bgr"),
                ..XftResources::default()
            },
            SurfaceSubpixelOrder::from_xrender(1),
        );
        assert_eq!(
            select(SelectedLinuxBackend::X11, || resources.options()),
            DisplayRasterOptions::X11(expected)
        );
    }

    #[test]
    fn timed_out_x11_capture_keeps_xlib_defaults_instead_of_pattern_defaults() {
        let (release, held) = std::sync::mpsc::channel::<()>();
        let fallback = bounded(Duration::ZERO, move || {
            // The probe cannot finish before bounded returns, even when the
            // test thread is descheduled longer than the requested timeout.
            let _ = held.recv();
            Resources {
                hintstyle: Some("hintslight".into()),
                ..Resources::default()
            }
        });
        drop(release);
        assert_eq!(fallback, Resources::default().options());
        assert_ne!(fallback, SurfaceFontOptions::default());
    }
}
