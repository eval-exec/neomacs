//! Native publication into one evaluator's owned, unbounded input channel.
//!
//! Conversion and targeting stay in input_bridge. Publication owns the order:
//! shared primary size, conversion, early quit, enqueue, then one native wake.

use std::fmt;
use std::sync::{Arc, Mutex};

use neomacs_display_runtime::tty_input::Publication;
#[cfg(unix)]
use neomacs_display_runtime::tty_input::{PrimaryTtyInput, PrimaryTtyPublisher};
use neovm_core::emacs_core::eval::QuitRequest;
use neovm_core::host_input::HostInputPublisher;
use neovm_core::window::FrameId;

use super::super::{SharedPrimaryWindowSize, record_primary_window_resize};
use super::{
    DisplayEvent, HostInputDelivery, KbInputEvent, convert_display_event,
    finish_host_input_publication, should_log_display_event,
};

/// An observation handle for the existing GUI startup/adoption authority.
/// It observes assignments at publication time; it never owns frame lifecycle.
#[derive(Debug)]
pub(crate) struct GuiPrimaryFrameObserver(Arc<Mutex<Option<FrameId>>>);

/// The original native GUI window uses frame zero until renderer adoption.
/// Primary TTY input instead belongs to terminal zero regardless of selection.
#[derive(Debug)]
pub(crate) enum InputRoute {
    Tty,
    Gui {
        primary_frame: GuiPrimaryFrameObserver,
    },
}

#[derive(Debug)]
enum RoutedInput {
    Deliver(KbInputEvent),
    AwaitingPrimaryResize,
}

impl InputRoute {
    pub(crate) fn gui(primary_frame: Arc<Mutex<Option<FrameId>>>) -> Self {
        Self::Gui {
            primary_frame: GuiPrimaryFrameObserver(primary_frame),
        }
    }

    fn resolve(&self, mut event: KbInputEvent) -> RoutedInput {
        match self {
            Self::Tty => {}
            Self::Gui { primary_frame } => {
                if let KbInputEvent::Resize { emacs_frame_id, .. } = &mut event
                    && *emacs_frame_id == 0
                {
                    let primary = *primary_frame
                        .0
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    match primary {
                        Some(primary) => *emacs_frame_id = primary.0,
                        None => return RoutedInput::AwaitingPrimaryResize,
                    }
                }
            }
        }
        RoutedInput::Deliver(event)
    }
}

/// The production owner is the existing QuitRequest. Its atomic and global
/// attention accounting remain encapsulated by QuitRequest::request.
pub(crate) trait PublicationQuit: Send + fmt::Debug {
    fn request(&self);
}

impl PublicationQuit for QuitRequest {
    fn request(&self) {
        QuitRequest::request(self);
    }
}

/// Per-owner deterministic ordering witness; absent from production layouts.
#[cfg(test)]
enum BeforeEnqueue {
    Inactive,
    Observe(std::sync::Arc<dyn Fn(&KbInputEvent) + Send + Sync>),
}

#[cfg(test)]
impl fmt::Debug for BeforeEnqueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inactive => f.write_str("BeforeEnqueue::Inactive"),
            Self::Observe(_) => f.write_str("BeforeEnqueue::Observe"),
        }
    }
}

#[cfg(test)]
impl BeforeEnqueue {
    fn observe(&self, input: &KbInputEvent) {
        match self {
            Self::Inactive => {}
            Self::Observe(observer) => observer(input),
        }
    }
}

/// Application routing policy; queue and wake authority stay inseparable in core.
#[derive(Debug)]
pub(crate) struct InputPublisher<Q = QuitRequest> {
    input: HostInputPublisher,
    primary_window_size: SharedPrimaryWindowSize,
    route: InputRoute,
    quit_requested: Q,
    #[cfg(test)]
    before_enqueue: BeforeEnqueue,
}

static_assertions::assert_impl_all!(GuiPrimaryFrameObserver: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(GuiPrimaryFrameObserver: Clone, Copy);
static_assertions::assert_impl_all!(InputPublisher: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(InputPublisher: Clone, Copy);

impl<Q: PublicationQuit> InputPublisher<Q> {
    /// The Context owner installs this typed connection before constructing a router.
    pub(crate) fn new(
        input: HostInputPublisher,
        route: InputRoute,
        primary_window_size: SharedPrimaryWindowSize,
        quit_requested: Q,
    ) -> Self {
        Self {
            input,
            primary_window_size,
            route,
            quit_requested,
            #[cfg(test)]
            before_enqueue: BeforeEnqueue::Inactive,
        }
    }

    pub(crate) fn publish_display(&mut self, event: &DisplayEvent) -> Publication {
        let should_log = should_log_display_event(event);
        if should_log {
            tracing::debug!("input-bridge: received display event {:?}", event);
        }
        record_primary_window_resize(&self.primary_window_size, event);
        let route = &self.route;
        let converted = convert_display_event(event)
            .into_iter()
            .filter_map(|input| match route.resolve(input) {
                RoutedInput::Deliver(input) => {
                    if should_log {
                        tracing::debug!(
                            "input-bridge: converted display event {:?} to keyboard event {:?}",
                            event,
                            input
                        );
                    }
                    Some(input)
                }
                RoutedInput::AwaitingPrimaryResize => None,
            });
        self.publish_batch(converted)
    }

    /// Font events are already core input; avoid materializing a singleton iterator.
    pub(crate) fn publish_core(&mut self, input: KbInputEvent) -> Publication {
        self.prepare_enqueue(&input);
        publication_result(finish_host_input_publication(self.input.send(input)))
    }

    /// One definition of early quit and the application ordering witness.
    fn prepare_enqueue(&self, input: &KbInputEvent) {
        if input.requests_default_quit() {
            self.quit_requested.request();
        }
        #[cfg(test)]
        self.before_enqueue.observe(input);
    }

    fn publish_batch(&self, inputs: impl IntoIterator<Item = KbInputEvent>) -> Publication {
        let prepared = inputs.into_iter().map(|input| {
            self.prepare_enqueue(&input);
            input
        });
        publication_result(finish_host_input_publication(
            self.input.send_batch(prepared),
        ))
    }
}

fn publication_result(delivery: HostInputDelivery) -> Publication {
    match delivery {
        HostInputDelivery::Published => Publication::Delivered,
        HostInputDelivery::ReceiverClosed => Publication::ReceiverClosed,
    }
}

/// A GUI-route router cannot be used as the runtime's primary TTY publisher.
#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct PrimaryInputPublisher<Q = QuitRequest> {
    inner: InputPublisher<Q>,
}

static_assertions::assert_impl_all!(PrimaryInputPublisher: Send, fmt::Debug);
static_assertions::assert_eq_size!(PrimaryInputPublisher, InputPublisher);
static_assertions::assert_not_impl_any!(PrimaryInputPublisher: Clone, Copy);
#[cfg(unix)]
static_assertions::assert_not_impl_any!(InputPublisher: PrimaryTtyPublisher);

impl<Q: PublicationQuit> PrimaryInputPublisher<Q> {
    pub(crate) fn new(
        input: HostInputPublisher,
        primary_window_size: SharedPrimaryWindowSize,
        quit_requested: Q,
    ) -> Self {
        Self {
            inner: InputPublisher::new(input, InputRoute::Tty, primary_window_size, quit_requested),
        }
    }
}

#[cfg(not(unix))]
impl PrimaryInputPublisher {
    pub(crate) fn into_legacy(self) -> InputPublisher {
        self.inner
    }
}

#[cfg(unix)]
impl<Q: PublicationQuit + 'static> PrimaryTtyPublisher for PrimaryInputPublisher<Q> {
    fn publish(&mut self, input: PrimaryTtyInput) -> Publication {
        self.inner.publish_display(&DisplayEvent::from(input))
    }
}

#[cfg(test)]
#[path = "publication/tests/publication_test.rs"]
mod tests;
