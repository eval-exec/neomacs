//! Native publication into one evaluator's owned, unbounded input channel.
//!
//! Conversion and targeting stay in input_bridge. Publication owns the order:
//! shared primary size, conversion, early quit, enqueue, then one native wake.

use std::fmt;
use std::io;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender};
use neomacs_display_runtime::tty_input::Publication;
#[cfg(unix)]
use neomacs_display_runtime::tty_input::{PrimaryTtyInput, PrimaryTtyPublisher};
use neovm_core::emacs_core::eval::QuitRequest;
use neovm_core::emacs_core::process::WaitNotifier;
use neovm_core::window::FrameId;

use super::super::{SharedPrimaryWindowSize, record_primary_window_resize};
use super::{DisplayEvent, KbInputEvent, convert_display_event, should_log_display_event};

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

/// Generic only to make native failure/ordering controls deterministic. The
/// production instantiation owns NativeInputWake directly, with no vtable.
pub(crate) trait PublicationWake: Send + fmt::Debug {
    fn notify(&self) -> io::Result<()>;
}

/// The optional notifier is a platform capability, not publication state.
pub(crate) struct NativeInputWake {
    notifier: Option<WaitNotifier>,
}

impl fmt::Debug for NativeInputWake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.notifier {
            Some(_) => f.write_str("NativeInputWake::Available"),
            None => f.write_str("NativeInputWake::Unavailable"),
        }
    }
}

impl PublicationWake for NativeInputWake {
    fn notify(&self) -> io::Result<()> {
        match &self.notifier {
            Some(notifier) => notifier.notify(),
            None => Ok(()),
        }
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

#[derive(Debug)]
pub(crate) struct InputPublisher<W = NativeInputWake, Q = QuitRequest> {
    sender: Sender<KbInputEvent>,
    primary_window_size: SharedPrimaryWindowSize,
    route: InputRoute,
    wake: W,
    quit_requested: Q,
    #[cfg(test)]
    before_enqueue: BeforeEnqueue,
}

static_assertions::assert_impl_all!(GuiPrimaryFrameObserver: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(GuiPrimaryFrameObserver: Clone, Copy);
static_assertions::assert_impl_all!(NativeInputWake: Send, Sync, fmt::Debug);
static_assertions::assert_impl_all!(InputPublisher: Send, Sync, fmt::Debug);
static_assertions::assert_not_impl_any!(InputPublisher: Clone, Copy);

impl InputPublisher {
    /// Only this factory creates the production publisher. A caller cannot
    /// supply a bounded sender and block the raw reader while Lisp is busy.
    pub(crate) fn unbounded(
        route: InputRoute,
        primary_window_size: SharedPrimaryWindowSize,
        quit_requested: QuitRequest,
        notifier: Option<WaitNotifier>,
    ) -> (Self, Receiver<KbInputEvent>) {
        Self::with_resources(
            route,
            primary_window_size,
            quit_requested,
            NativeInputWake { notifier },
        )
    }

    /// Existing image/secondary-device producers share this same queue.
    pub(crate) fn sender(&self) -> &Sender<KbInputEvent> {
        &self.sender
    }

    pub(crate) fn clone_notifier(&self) -> Option<WaitNotifier> {
        self.wake.notifier.clone()
    }

    pub(crate) fn clone_quit_request(&self) -> QuitRequest {
        self.quit_requested.clone()
    }
}

impl<W: PublicationWake, Q: PublicationQuit> InputPublisher<W, Q> {
    fn with_resources(
        route: InputRoute,
        primary_window_size: SharedPrimaryWindowSize,
        quit_requested: Q,
        wake: W,
    ) -> (Self, Receiver<KbInputEvent>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                sender,
                primary_window_size,
                route,
                wake,
                quit_requested,
                #[cfg(test)]
                before_enqueue: BeforeEnqueue::Inactive,
            },
            receiver,
        )
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

    /// Font preferences already arrive as evaluator input; they use the same
    /// early-quit/enqueue/wake authority as converted display input.
    pub(crate) fn publish_core(&mut self, input: KbInputEvent) -> Publication {
        self.publish_batch(std::iter::once(input))
    }

    fn publish_batch(&self, inputs: impl IntoIterator<Item = KbInputEvent>) -> Publication {
        let mut inputs = inputs.into_iter().peekable();
        if inputs.peek().is_none() {
            // Ignored display facts have nothing to queue or wake.
            return Publication::Delivered;
        }
        for input in inputs {
            if input.requests_default_quit() {
                self.quit_requested.request();
            }
            #[cfg(test)]
            self.before_enqueue.observe(&input);
            if self.sender.send(input).is_err() {
                return Publication::ReceiverClosed;
            }
        }
        if let Err(error) = self.wake.notify() {
            // The queue owns the input already. Returning ReceiverClosed or
            // retrying here would replay input after a notification failure.
            tracing::error!(%error, "input bridge failed to wake evaluator");
        }
        Publication::Delivered
    }
}

/// Only the fixed-TTY factory can create a production primary publisher.
/// Its private inner owner cannot be replaced by a GUI-route publisher.
#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct PrimaryInputPublisher<W = NativeInputWake, Q = QuitRequest> {
    inner: InputPublisher<W, Q>,
}

static_assertions::assert_impl_all!(PrimaryInputPublisher: Send, fmt::Debug);
static_assertions::assert_eq_size!(PrimaryInputPublisher, InputPublisher);
static_assertions::assert_not_impl_any!(PrimaryInputPublisher: Clone, Copy);
#[cfg(unix)]
static_assertions::assert_not_impl_any!(InputPublisher: PrimaryTtyPublisher);

impl PrimaryInputPublisher {
    pub(crate) fn unbounded(
        primary_window_size: SharedPrimaryWindowSize,
        quit_requested: QuitRequest,
        notifier: Option<WaitNotifier>,
    ) -> (Self, Receiver<KbInputEvent>) {
        Self::with_resources(
            primary_window_size,
            quit_requested,
            NativeInputWake { notifier },
        )
    }

    pub(crate) fn sender(&self) -> &Sender<KbInputEvent> {
        self.inner.sender()
    }

    pub(crate) fn clone_notifier(&self) -> Option<WaitNotifier> {
        self.inner.clone_notifier()
    }

    pub(crate) fn clone_quit_request(&self) -> QuitRequest {
        self.inner.clone_quit_request()
    }

    #[cfg(not(unix))]
    pub(crate) fn into_legacy(self) -> InputPublisher {
        self.inner
    }
}

impl<W: PublicationWake, Q: PublicationQuit> PrimaryInputPublisher<W, Q> {
    fn with_resources(
        primary_window_size: SharedPrimaryWindowSize,
        quit_requested: Q,
        wake: W,
    ) -> (Self, Receiver<KbInputEvent>) {
        let (inner, receiver) = InputPublisher::with_resources(
            InputRoute::Tty,
            primary_window_size,
            quit_requested,
            wake,
        );
        (Self { inner }, receiver)
    }
}

#[cfg(unix)]
impl<W: PublicationWake + 'static, Q: PublicationQuit + 'static> PrimaryTtyPublisher
    for PrimaryInputPublisher<W, Q>
{
    fn publish(&mut self, input: PrimaryTtyInput) -> Publication {
        self.inner.publish_display(&DisplayEvent::from(input))
    }
}

#[cfg(test)]
#[path = "publication/tests/publication_test.rs"]
mod tests;
