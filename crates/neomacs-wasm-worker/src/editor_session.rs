//! Runtime-image restoration and the evaluator-owned side of one browser session.

use std::path::Path;
use std::time::Duration;

use neomacs_app::initial_surface::{
    InitialBackgroundMode, InitialDisplayType, InitialEditorSurfaceSpec, InitialFrameFont,
    InitialFrameMetrics, prepare_initial_editor_surface,
};
use neomacs_app::presentation::PresentationMetrics;
use neomacs_app::runtime_image::AuthenticatedPortableRuntimeImage;
use neomacs_app::runtime_resources::{MountedRuntimeResources, RuntimeResourceBundle};
use neomacs_app::session::{
    EditorSession, EditorSessionExit, FrontendFrameInbox, FrontendFrameReceive, FrontendInputPort,
};
use neomacs_app::startup::{InteractiveGuiStartup, configure_interactive_gui_startup};
use neomacs_layout_engine::font::sizing::FontSizing;
use neomacs_wasm_protocol::{BrowserColorScheme, BrowserEditorStartup, BrowserInputBatch};
use neovm_core::emacs_core::wait::{HostInputWaitBackend, HostInputWaitError};
use neovm_core::window::FrameDisplayIdentity;

use crate::browser_host::{self, HostWake};

pub(crate) fn run() -> Result<EditorSessionExit, String> {
    browser_host::report_status("restoring portable runtime image");
    let startup = decode_startup(browser_host::startup_bytes()?)?;
    let runtime_image = browser_host::runtime_image_bytes()?;
    let runtime_image_id = browser_host::runtime_image_id_bytes()?;
    let runtime_image = AuthenticatedPortableRuntimeImage::from_assets(
        &runtime_image,
        &runtime_image_id,
    )
    .map_err(|error| format!("failed to authenticate browser runtime image: {error}"))?;
    browser_host::report_status("mounting authenticated runtime resources");
    let runtime_resource_bundle = browser_host::runtime_resource_bundle_bytes()?;
    let runtime_resource_id = browser_host::runtime_resource_id_bytes()?;
    let runtime_resource_bundle = RuntimeResourceBundle::from_assets(
        &runtime_resource_bundle,
        &runtime_resource_id,
    )
    .map_err(|error| format!("invalid browser runtime resource assets: {error}"))?;
    let runtime_resources = MountedRuntimeResources::from_bundle(
        Path::new("/neomacs"),
        runtime_resource_bundle,
    )
    .map_err(|error| format!("failed to mount browser runtime resources: {error}"))?;
    let mut evaluator = runtime_image
        .load_for_with_mounted_runtime_resources(
            neomacs_app::host::HostProfile::WASM,
            runtime_resources,
        )
        .map_err(|error| format!("failed to restore browser runtime image: {error}"))?;

    let (width, height) = startup.physical_extent();
    let (character_width, character_height) = startup.character_size();
    let metrics = InitialFrameMetrics::new(
        width,
        height,
        character_width,
        character_height,
        startup.font_pixel_size(),
    )
    .map_err(|error| format!("invalid browser opening geometry: {error}"))?;
    let background = match startup.color_scheme() {
        BrowserColorScheme::Light => InitialBackgroundMode::Light,
        BrowserColorScheme::Dark => InitialBackgroundMode::Dark,
    };
    let surface = prepare_initial_editor_surface(
        &mut evaluator,
        InitialEditorSurfaceSpec::gui(
            metrics,
            FrameDisplayIdentity::default(),
            InitialDisplayType::Color,
            background,
            InitialFrameFont::named("monospace"),
        ),
    );
    let invocation = InteractiveGuiStartup::new(
        "neomacs-wasm",
        Path::new("/neomacs/bin"),
        Path::new("/"),
    )
    .with_arguments(["--quick", "--no-splash"]);
    configure_interactive_gui_startup(&mut evaluator, surface, &invocation)
        .map_err(|error| format!("failed to configure browser startup: {error:?}"))?;

    let (mut session, frontend) = EditorSession::attach(
        evaluator,
        PresentationMetrics::Scalable(FontSizing::logical()),
        || {},
    );
    let (input, frames) = frontend.split();
    session.install_host_input_wait_backend(BrowserWorkerTransport { input, frames });
    browser_host::report_status("entering editor command loop");
    Ok(session.run())
}

fn decode_startup(bytes: Vec<u8>) -> Result<BrowserEditorStartup, String> {
    let startup: BrowserEditorStartup = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid browser startup message: {error}"))?;
    startup
        .validate()
        .map_err(|error| format!("invalid browser startup message: {error}"))?;
    Ok(startup)
}

struct BrowserWorkerTransport {
    input: FrontendInputPort,
    frames: FrontendFrameInbox,
}

impl BrowserWorkerTransport {
    fn publish_latest_frame(&mut self) -> Result<(), HostInputWaitError> {
        let pending = match self.frames.try_latest() {
            FrontendFrameReceive::Empty => return Ok(()),
            FrontendFrameReceive::Disconnected => {
                return Err(HostInputWaitError::new(
                    "editor presentation stream disconnected",
                ));
            }
            FrontendFrameReceive::Frame(pending) => pending,
        };
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(pending.state(), &mut bytes).map_err(|error| {
            HostInputWaitError::new(format!("failed to encode browser presentation: {error}"))
        })?;
        browser_host::send_frame(&bytes).map_err(HostInputWaitError::new)?;
        let _state = pending.hand_off_to_remote_frontend();
        Ok(())
    }

    fn submit_input_batch(&self) -> Result<(), HostInputWaitError> {
        let bytes = browser_host::take_input_bytes().map_err(HostInputWaitError::new)?;
        let batch: BrowserInputBatch = serde_json::from_slice(&bytes).map_err(|error| {
            HostInputWaitError::new(format!("invalid browser input batch: {error}"))
        })?;
        let batch = batch.try_into_frontend_batch().map_err(|error| {
            HostInputWaitError::new(format!("invalid browser input batch: {error}"))
        })?;
        let sequence = batch.sequence();
        for event in batch.into_events() {
            self.input.submit(&event).map_err(|error| {
                HostInputWaitError::new(format!("failed to submit browser input: {error}"))
            })?;
        }
        browser_host::acknowledge_input(sequence).map_err(HostInputWaitError::new)?;
        Ok(())
    }
}

impl HostInputWaitBackend for BrowserWorkerTransport {
    fn wait_for_input(&mut self, timeout: Duration) -> Result<(), HostInputWaitError> {
        self.publish_latest_frame()?;
        match browser_host::wait(timeout).map_err(HostInputWaitError::new)? {
            HostWake::Input => self.submit_input_batch(),
            HostWake::TimedOut => Ok(()),
        }
    }
}
