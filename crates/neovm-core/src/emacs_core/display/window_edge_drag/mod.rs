//! Whether the pointer is currently holding a window edge, and which drag that is.
//!
//! Machinery with no elisp surface and no GNU `src/*.c` mirror: GNU has
//! nothing to record here because GNU's redisplay always installs instantly,
//! and only a compositor that animates between presentations has to know which
//! ones the user's hand is placing.
//!
//! The extent of a drag is the pointer button. A press that lands on a region
//! whose drag moves a window edge opens a session; releasing that button closes
//! it. That is a fact about the interaction rather than about Lisp:
//! `lisp/mouse.el`'s `mouse-drag-line` sets `track-mouse` to `dragging` and
//! installs a `set-transient-map`, then *returns*, so the drag runs one command
//! per pointer movement and there is no command extent to bind to — and
//! `track-mouse` itself is a `setq` convention of three particular commands,
//! which a rebound `[mode-line down-mouse-1]` would not follow.

use neomacs_display_protocol::PresentedRegionKind;
use neomacs_display_protocol::presentation_origin::{InteractionSessionId, PresentationOrigin};

use crate::keyboard::MouseButton;

/// A press that took hold of a window edge.
///
/// Constructible only from a region whose drag moves one, so a press on
/// ordinary text cannot open a resize session: there is no value to pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowEdgeGrab(MouseButton);

impl WindowEdgeGrab {
    /// The grab a press on `region` makes, or `None` if dragging there moves
    /// no window edge.
    #[must_use]
    pub fn of(button: MouseButton, region: PresentedRegionKind) -> Option<Self> {
        region.dragged_window_edge().map(|_| Self(button))
    }
}

/// The window-edge drag the pointer is running, if any.
///
/// Holds no dynamic binding and needs no unwind, so a non-local exit cannot
/// leak one. If a drag were ever abandoned in a way that reached neither a
/// release nor a focus loss, the degraded state is "install instantly", which
/// is invisible — unlike a stuck animation, which is not.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowEdgeDrag {
    /// The most recent drag: the one running, or the last one that finished.
    ///
    /// One field rather than a live drag beside a separate counter, because
    /// two fields could disagree about which ids have been handed out — and an
    /// id handed out twice would make an ancient drag's commits look current.
    latest: Option<Session>,
}

#[derive(Clone, Copy, Debug)]
struct Session {
    id: InteractionSessionId,
    /// The button still holding the edge, or `None` once it let go.
    held_by: Option<MouseButton>,
}

impl WindowEdgeDrag {
    /// Take hold of a window edge.
    pub fn grabbed(&mut self, grab: WindowEdgeGrab) {
        if self.held_by().is_some() {
            // A second button pressed mid-drag does not start a second drag:
            // the edge is already in hand, and it is let go by the button that
            // took it.
            return;
        }
        let id = self
            .latest
            .map_or(InteractionSessionId::FIRST, |session| session.id.next());
        self.latest = Some(Session {
            id,
            held_by: Some(grab.0),
        });
    }

    /// Let go of `button`.
    ///
    /// Releasing some other button leaves the drag running, matching the
    /// transient map `mouse-drag-line` installs: it reacts to the button that
    /// started the drag and ignores the rest.
    pub fn released(&mut self, button: MouseButton) {
        if self.held_by() == Some(button)
            && let Some(session) = self.latest.as_mut()
        {
            session.held_by = None;
        }
    }

    /// The pointer was taken away without a release.
    ///
    /// A frame that loses focus mid-drag never sees the button come up, and a
    /// session left open would suppress motion for every later presentation.
    pub fn abandoned(&mut self) {
        if let Some(session) = self.latest.as_mut() {
            session.held_by = None;
        }
    }

    /// Why a presentation composed now was produced.
    #[must_use]
    pub fn origin(&self) -> PresentationOrigin {
        match self.latest {
            Some(Session {
                id,
                held_by: Some(_),
            }) => PresentationOrigin::InteractiveResize { session: id },
            _ => PresentationOrigin::Ordinary,
        }
    }

    fn held_by(&self) -> Option<MouseButton> {
        self.latest.and_then(|session| session.held_by)
    }
}

impl crate::emacs_core::Context {
    /// Why a presentation sealed now was produced.
    ///
    /// Read by the layout engine at the moment it seals, so the stamp
    /// describes the presentation actually being published rather than a guess
    /// made earlier in redisplay.
    #[must_use]
    pub fn presentation_origin(&self) -> PresentationOrigin {
        self.window_edge_drag.origin()
    }
}

#[cfg(test)]
#[path = "window_edge_drag_test.rs"]
mod tests;
