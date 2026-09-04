//! The one owner of a page's load state: milestones, observed readings,
//! deduplication and terminal ordering.
//!
//! A native browser reports the same load two ways.  Its navigation delegate
//! delivers discrete milestones (started, redirected, committed, finished),
//! and its observable properties (`title`, `URL`, `estimatedProgress` on
//! WKWebView; `DocumentTitle`/`Source` on WebView2) change continuously and
//! are read by a property observer or sampled once per service turn.  GNU
//! reads the three live when Lisp asks (`xwidget-webkit-estimated-load-
//! progress`, src/xwidget.c:3087-3110 and nsxwidget.m:374-378 in
//! emacs-31.0.90; `xwidget-webkit-title`, xwidget.c:3070-3085), polled by
//! `xwidget-webkit-callback`'s 0.5 s timer (lisp/xwidget.el:432-459), while
//! the `load-changed` phases arrive as events (xwidget.c:2427-2447).  This
//! port pushes progress, title and URI as events instead, so their
//! deduplication is its own responsibility.
//!
//! Two emitters of the same progress value drift: a finished load published
//! `1.0` from the milestone and again from the observer, and a failed load
//! could be followed by a lower sampled value.  [`PageLoadState`] is the
//! single writer.  Milestones and readings both go through it, each value is
//! reported once per change, and a progress reading is accepted only while a
//! load is in flight, so nothing follows the terminal `1.0` until the next
//! load starts.

use std::num::NonZeroUsize;

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
    /// The last load finished, or failed (a failure is delivered as the
    /// finished milestone, after GNU's GTK build); its progress is terminal.
    Finished,
}

/// Identity of the native navigation whose delegate callback is being folded
/// into [`PageLoadState`].
///
/// WebKit supplies the same `WKNavigation` object to every callback for one
/// operation.  Keeping that identity separate from the logical WebView
/// generation prevents a late failure or completion from an older operation
/// from terminalizing the navigation currently loading in the same view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum NativeNavigation {
    Identified(NonZeroUsize),
    Unidentified,
}

impl NativeNavigation {
    /// Derive an opaque identity from a native navigation object.  A live
    /// reference is necessarily non-null and its address is used only for
    /// equality while WebKit owns the object.
    #[cfg(target_os = "macos")]
    pub(crate) fn of<T>(navigation: &T) -> Self {
        let address = NonZeroUsize::new(std::ptr::from_ref(navigation).addr())
            .expect("a reference has a non-null address");
        Self::Identified(address)
    }

    #[cfg(test)]
    pub(crate) fn identified(address: usize) -> Self {
        Self::Identified(NonZeroUsize::new(address).expect("test navigation ids are non-zero"))
    }
}

#[derive(Debug, Default)]
pub(crate) struct PageLoadState {
    /// The last title reported to the frontend; a page without one is `""`,
    /// whether WKWebView answered nil or an empty string, so the two cannot
    /// be reported as a change of each other.
    title: String,
    uri: String,
    progress: f64,
    status: LoadStatus,
    navigation: Option<NativeNavigation>,
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
        navigation: NativeNavigation,
        milestone: NavigationMilestone,
    ) -> Vec<WebViewEvent> {
        let accepted = match milestone {
            NavigationMilestone::Started => {
                self.navigation = Some(navigation);
                true
            }
            NavigationMilestone::Redirected | NavigationMilestone::Committed => {
                match self.navigation {
                    None => {
                        self.navigation = Some(navigation);
                        true
                    }
                    Some(active) => active == navigation && self.status != LoadStatus::Finished,
                }
            }
            NavigationMilestone::Finished => match self.navigation {
                None => {
                    self.navigation = Some(navigation);
                    true
                }
                Some(active) => active == navigation && self.status != LoadStatus::Finished,
            },
        };
        if !accepted {
            return Vec::new();
        }
        let mut events = Vec::new();
        self.status = match milestone {
            NavigationMilestone::Started => LoadStatus::Loading,
            NavigationMilestone::Redirected | NavigationMilestone::Committed => LoadStatus::Loading,
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
    /// A title or URI is reported whenever it differs from the last one
    /// reported, including when it goes away: WKWebView answers nil (or an
    /// empty string) for an untitled document, and GNU reads the live value
    /// (`xwidget-webkit-title`, src/xwidget.c:3070-3085), so the frontend must
    /// not keep the previous page's string.  A view that has never had a
    /// title reports nothing until it gets one.
    ///
    /// A progress reading counts only while a load is in flight: before the
    /// first start it is noise from an idle view, and after a finish it would
    /// contradict the terminal `1.0` the milestone published.  WebKit posts
    /// its first `estimatedProgress` (0.1) just before
    /// `didStartProvisionalNavigation`; that reading is dropped and the same
    /// value is re-derived from the first sample after the start, so a load
    /// always reports `0.0` first.
    pub(crate) fn observe(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        title: Option<String>,
        uri: Option<String>,
        progress: f64,
    ) -> Vec<WebViewEvent> {
        let mut events = Vec::new();
        let title = title.unwrap_or_default();
        if title != self.title {
            self.title = title.clone();
            events.push(WebViewEvent::TitleChanged {
                id,
                generation,
                title,
            });
        }
        let uri = uri.unwrap_or_default();
        if uri != self.uri {
            self.uri = uri.clone();
            events.push(WebViewEvent::UriChanged {
                id,
                generation,
                uri,
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
