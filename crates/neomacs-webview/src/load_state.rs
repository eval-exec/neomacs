//! The one owner of a page's load state: milestones, observed readings,
//! deduplication and terminal ordering.
//!
//! A native browser reports the same load two ways.  Its navigation delegate
//! delivers discrete milestones (started, redirected, committed, finished),
//! and its observable properties (`title`, `URL`, `estimatedProgress` on
//! WKWebView; `DocumentTitle`/`Source` on WebView2) change continuously and
//! are read by a property observer or sampled once per service turn.  GNU's
//! GTK build reads the same three as GObject properties and lets
//! `notify::estimated-load-progress` drive
//! `xwidget-webkit-estimated-load-progress`, while `load-changed` phases come
//! from the load-changed signal (src/xwidget.c:2427-2447, emacs-31.0.90).
//!
//! Two emitters of the same progress value drift: a finished load published
//! `1.0` from the milestone and again from the observer, and a failed load
//! could be followed by a lower sampled value.  [`PageLoadState`] is the
//! single writer.  Milestones and readings both go through it, each value is
//! reported once per change, and a progress reading is accepted only while a
//! load is in flight, so nothing follows the terminal `1.0` until the next
//! load starts.

use neomacs_display_protocol::WebViewId;

use crate::backend::NavigationMilestone;
use crate::{WebViewEvent, WebViewGeneration};

/// Where a page is in its current load, as the milestones reported it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum LoadStatus {
    /// No load has started since creation.
    #[default]
    Idle,
    /// Between a start milestone and its finish.
    Loading,
    /// The last load finished (or failed); its progress is terminal.
    Finished,
}

#[derive(Debug, Default)]
pub(crate) struct PageLoadState {
    title: Option<String>,
    uri: Option<String>,
    progress: f64,
    status: LoadStatus,
}

impl PageLoadState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record a navigation milestone and return the events it implies, in
    /// GNU's order: progress first, then the `load-changed` phase, then the
    /// completion event for a finished load.
    pub(crate) fn milestone(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        milestone: NavigationMilestone,
    ) -> Vec<WebViewEvent> {
        let mut events = Vec::new();
        self.status = match milestone {
            NavigationMilestone::Started => LoadStatus::Loading,
            NavigationMilestone::Redirected | NavigationMilestone::Committed => {
                LoadStatus::Loading
            }
            NavigationMilestone::Finished => LoadStatus::Finished,
        };
        if let Some(progress) = milestone.progress_marker() {
            events.extend(self.set_progress(id, generation, progress));
        }
        events.push(WebViewEvent::LoadChanged {
            id,
            generation,
            phase: milestone.phase(),
        });
        if milestone == NavigationMilestone::Finished {
            events.push(WebViewEvent::LoadFinished {
                id,
                generation,
                navigation: None,
            });
        }
        events
    }

    /// Record one reading of the observable page properties and return the
    /// events it implies.
    ///
    /// A title or URI is reported whenever it differs from the last one,
    /// including when it goes away: WKWebView answers nil for an untitled
    /// document, and GNU reads the live value (`xwidget-webkit-title` is a
    /// property read, src/xwidget.c:3070-3085), so the frontend must not keep
    /// the previous page's string.  Nothing is reported before the first
    /// value exists.
    ///
    /// A progress reading counts only while a load is in flight: before the
    /// first start it is noise from an idle view, and after a finish it would
    /// contradict the terminal `1.0` the milestone published.
    pub(crate) fn observe(
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
            events.push(WebViewEvent::TitleChanged {
                id,
                generation,
                title: title.unwrap_or_default(),
            });
        }
        if uri != self.uri {
            self.uri = uri.clone();
            events.push(WebViewEvent::UriChanged {
                id,
                generation,
                uri: uri.unwrap_or_default(),
            });
        }
        if self.status == LoadStatus::Loading {
            events.extend(self.set_progress(id, generation, progress));
        }
        events
    }

    /// `estimatedProgress` is documented as 0.0..=1.0; anything else from the
    /// framework is clamped, and an unchanged value is not an event.
    fn set_progress(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        progress: f64,
    ) -> Option<WebViewEvent> {
        let progress = progress.clamp(0.0, 1.0);
        if (progress - self.progress).abs() <= f64::EPSILON {
            return None;
        }
        self.progress = progress;
        Some(WebViewEvent::LoadProgressChanged {
            id,
            generation,
            progress,
        })
    }
}

#[cfg(test)]
#[path = "load_state_test.rs"]
mod tests;
