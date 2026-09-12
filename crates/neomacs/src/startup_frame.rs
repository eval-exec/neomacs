//! One-shot preparation before the first native window exists.
//!
//! Font parsing/opening stays on the evaluator thread after its TLS is ready.
//! The native thread receives only geometry, never Lisp values or font handles.

use super::startup_font::{BootstrapFont, StartupFont};
use super::{BootstrapDisplayConfig, BootstrapResult, Context, FrontendKind, PrimaryWindowSize};

#[derive(Debug, strum::Display)]
pub(super) enum StartupFrameError {
    #[strum(serialize = "no usable initial GUI font was found")]
    NoUsableFont,
    #[strum(serialize = "GUI startup requires a graphical display")]
    NotGraphical,
}

/// Its dimensions and opened font cannot be replaced independently. Consuming
/// installation passes both to bootstrap, preventing a second font selection.
pub(super) struct PreparedGuiFrame {
    display: BootstrapDisplayConfig,
    font: StartupFont,
    size: PrimaryWindowSize,
}

impl PreparedGuiFrame {
    pub(super) fn prepare(display: BootstrapDisplayConfig) -> Result<Self, StartupFrameError> {
        let BootstrapFont::Gui(font) =
            BootstrapFont::select(&display).ok_or(StartupFrameError::NoUsableFont)?
        else {
            return Err(StartupFrameError::NotGraphical);
        };
        let (width, height) = super::startup_dimensions(
            FrontendKind::Gui,
            font.metrics(),
            display.interactivity.is_batch(),
        );
        Ok(Self {
            display,
            font,
            size: PrimaryWindowSize { width, height },
        })
    }

    pub(super) fn size(&self) -> PrimaryWindowSize {
        self.size
    }

    pub(super) fn install(self, eval: &mut Context) -> BootstrapResult {
        super::bootstrap_buffers_with_font(
            eval,
            self.size.width,
            self.size.height,
            self.display,
            BootstrapFont::Gui(self.font),
        )
    }
}

type StartupResult = Result<PrimaryWindowSize, StartupFrameError>;

/// Consumed by either success or failure. Dropping it during evaluator unwind
/// disconnects the receiver, so the native thread cannot wait forever on panic.
pub(super) struct StartupFrameReply(crossbeam_channel::Sender<StartupResult>);

impl StartupFrameReply {
    pub(super) fn channel() -> (Self, crossbeam_channel::Receiver<StartupResult>) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        (Self(tx), rx)
    }

    pub(super) fn ready(
        self,
        frame: &PreparedGuiFrame,
    ) -> Result<(), crossbeam_channel::SendError<StartupResult>> {
        self.0.send(Ok(frame.size()))
    }

    pub(super) fn failed(self, error: StartupFrameError) {
        let _ = self.0.send(Err(error));
    }
}
