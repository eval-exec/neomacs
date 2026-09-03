use std::fmt;
use std::task::{Context, Poll, Waker};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BrowserPresentationId(u64);

impl BrowserPresentationId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserFrameProvenance {
    Bootstrap,
    Editor(BrowserPresentationId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserPresentationAttempt {
    Presented,
    Skipped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BrowserPresentationFailure {
    WindowCreation(String),
    RendererInitialization(String),
    SurfaceGeometry(String),
    BootstrapFrame(String),
    DisplayScale(String),
    Rendering(String),
}

impl fmt::Display for BrowserPresentationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (context, message) = match self {
            Self::WindowCreation(message) => {
                ("failed to create the browser canvas window", message)
            }
            Self::RendererInitialization(message) => {
                ("failed to initialize browser GPU presentation", message)
            }
            Self::SurfaceGeometry(message) => ("invalid browser surface geometry", message),
            Self::BootstrapFrame(message) => ("failed to build the browser initial frame", message),
            Self::DisplayScale(message) => ("invalid browser display scale", message),
            Self::Rendering(message) => ("browser GPU presentation failed", message),
        };
        write!(formatter, "{context}: {message}")
    }
}

pub(crate) type BrowserPresentationResult =
    Result<BrowserPresentationId, BrowserPresentationFailure>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum FirstEditorPresentationState {
    Pending,
    Presented(BrowserPresentationId),
    Failed(BrowserPresentationFailure),
}

pub(crate) struct FirstEditorPresentationLatch {
    state: FirstEditorPresentationState,
    waiters: Vec<Waker>,
}

impl Default for FirstEditorPresentationLatch {
    fn default() -> Self {
        Self {
            state: FirstEditorPresentationState::Pending,
            waiters: Vec::new(),
        }
    }
}

impl FirstEditorPresentationLatch {
    pub(crate) fn observe(
        &mut self,
        provenance: BrowserFrameProvenance,
        attempt: BrowserPresentationAttempt,
    ) {
        if self.state != FirstEditorPresentationState::Pending {
            return;
        }
        let (BrowserFrameProvenance::Editor(presentation), BrowserPresentationAttempt::Presented) =
            (provenance, attempt)
        else {
            return;
        };
        self.state = FirstEditorPresentationState::Presented(presentation);
        self.wake_waiters();
    }

    pub(crate) fn fail(&mut self, failure: BrowserPresentationFailure) {
        if self.state != FirstEditorPresentationState::Pending {
            return;
        }
        self.state = FirstEditorPresentationState::Failed(failure);
        self.wake_waiters();
    }

    pub(crate) fn poll(&mut self, context: &mut Context<'_>) -> Poll<BrowserPresentationResult> {
        match &self.state {
            FirstEditorPresentationState::Pending => {
                if !self
                    .waiters
                    .iter()
                    .any(|waiter| waiter.will_wake(context.waker()))
                {
                    self.waiters.push(context.waker().clone());
                }
                Poll::Pending
            }
            FirstEditorPresentationState::Presented(presentation) => Poll::Ready(Ok(*presentation)),
            FirstEditorPresentationState::Failed(failure) => Poll::Ready(Err(failure.clone())),
        }
    }

    fn wake_waiters(&mut self) {
        for waiter in self.waiters.drain(..) {
            waiter.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};

    use super::*;

    fn poll(latch: &mut FirstEditorPresentationLatch) -> Poll<BrowserPresentationResult> {
        let mut context = Context::from_waker(Waker::noop());
        latch.poll(&mut context)
    }

    #[test]
    fn bootstrap_presentation_does_not_mark_the_editor_ready() {
        let mut latch = FirstEditorPresentationLatch::default();

        latch.observe(
            BrowserFrameProvenance::Bootstrap,
            BrowserPresentationAttempt::Presented,
        );

        assert_eq!(poll(&mut latch), Poll::Pending);
    }

    #[test]
    fn skipped_editor_presentation_stays_pending() {
        let mut latch = FirstEditorPresentationLatch::default();

        latch.observe(
            BrowserFrameProvenance::Editor(BrowserPresentationId::new(17)),
            BrowserPresentationAttempt::Skipped,
        );

        assert_eq!(poll(&mut latch), Poll::Pending);
    }

    #[test]
    fn first_presented_editor_frame_resolves_exactly_once() {
        let mut latch = FirstEditorPresentationLatch::default();
        let first = BrowserPresentationId::new(23);

        latch.observe(
            BrowserFrameProvenance::Editor(first),
            BrowserPresentationAttempt::Presented,
        );
        latch.observe(
            BrowserFrameProvenance::Editor(BrowserPresentationId::new(24)),
            BrowserPresentationAttempt::Presented,
        );

        assert_eq!(first.get(), 23);
        assert_eq!(poll(&mut latch), Poll::Ready(Ok(first)));
    }

    #[test]
    fn initialization_failure_rejects_readiness() {
        let mut latch = FirstEditorPresentationLatch::default();
        let failure = BrowserPresentationFailure::RendererInitialization("no adapter".into());

        latch.fail(failure.clone());

        assert_eq!(poll(&mut latch), Poll::Ready(Err(failure)));
    }

    #[test]
    fn failures_retain_the_browser_boundary_context() {
        let failures = [
            (
                BrowserPresentationFailure::WindowCreation("window".into()),
                "failed to create the browser canvas window: window",
            ),
            (
                BrowserPresentationFailure::RendererInitialization("renderer".into()),
                "failed to initialize browser GPU presentation: renderer",
            ),
            (
                BrowserPresentationFailure::SurfaceGeometry("geometry".into()),
                "invalid browser surface geometry: geometry",
            ),
            (
                BrowserPresentationFailure::BootstrapFrame("bootstrap".into()),
                "failed to build the browser initial frame: bootstrap",
            ),
            (
                BrowserPresentationFailure::DisplayScale("scale".into()),
                "invalid browser display scale: scale",
            ),
            (
                BrowserPresentationFailure::Rendering("render".into()),
                "browser GPU presentation failed: render",
            ),
        ];

        for (failure, expected) in failures {
            assert_eq!(failure.to_string(), expected);
        }
    }
}
