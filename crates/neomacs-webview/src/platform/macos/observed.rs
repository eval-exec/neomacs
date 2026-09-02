//! The observable page state of one WKWebView, diffed into events.
//!
//! WKWebView exposes `title`, `URL` and `estimatedProgress` as KVO-compliant
//! properties; GNU's GTK build reads the same three as GObject properties and
//! `notify::estimated-load-progress` drives
//! `xwidget-webkit-estimated-load-progress`.  The observer callback and the
//! per-turn sampling fallback both hand their readings here, so a value is
//! reported exactly once per change whichever path saw it first, and the
//! logic stays testable without AppKit.

use neomacs_display_protocol::WebViewId;

use crate::{WebViewEvent, WebViewGeneration};

#[derive(Debug, Default)]
pub(super) struct ObservedWebState {
    title: Option<String>,
    uri: Option<String>,
    progress: f64,
}

impl ObservedWebState {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Record one reading and return the events it implies.
    ///
    /// A `None` title or URI is remembered (so the next real value is a
    /// change) but never reported: WKWebView answers nil before the first
    /// load and during some transitions, and GNU never sends an empty title
    /// or URI for those.
    pub(super) fn observe(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        title: Option<String>,
        uri: Option<String>,
        progress: f64,
    ) -> Vec<WebViewEvent> {
        let mut events = Vec::new();
        if title != self.title {
            self.title = title.clone();
            if let Some(title) = title {
                events.push(WebViewEvent::TitleChanged {
                    id,
                    generation,
                    title,
                });
            }
        }
        if uri != self.uri {
            self.uri = uri.clone();
            if let Some(uri) = uri {
                events.push(WebViewEvent::UriChanged {
                    id,
                    generation,
                    uri,
                });
            }
        }
        let progress = progress.clamp(0.0, 1.0);
        if (progress - self.progress).abs() > f64::EPSILON {
            self.progress = progress;
            events.push(WebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress,
            });
        }
        events
    }
}
