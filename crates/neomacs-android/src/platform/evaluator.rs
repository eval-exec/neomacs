//! Evaluator construction owned by the Android worker thread.

use std::ffi::CString;
use std::io;

use neomacs_app::host::HostProfile;
use neomacs_app::initial_surface::{
    InitialBackgroundMode, InitialDisplayType, InitialEditorSurfaceSpec, InitialFrameFont,
    InitialFrameMetrics, prepare_initial_editor_surface,
};
use neomacs_app::presentation::PresentationMetrics;
use neomacs_app::runtime_image::{ExtractedRuntimeImage, RuntimeImageSource};
use neomacs_app::session::{NativeEditorWorker, NativeEditorWorkerEvent};
use neomacs_app::startup::{InteractiveGuiStartup, configure_interactive_gui_startup};
use neomacs_layout_engine::font::metrics::FontMetricsService;
use neomacs_layout_engine::font::sizing::FontSizing;
use winit::platform::android::activity::AndroidApp;

/// Start one complete editor session without blocking the Activity thread.
pub(super) fn spawn(
    app: AndroidApp,
    width: u32,
    height: u32,
    emit: impl Fn(NativeEditorWorkerEvent) + Send + 'static,
) -> io::Result<NativeEditorWorker> {
    let font_sizing = FontSizing::logical();
    NativeEditorWorker::spawn(
        "neomacs-android-evaluator",
        move || create_evaluator(app, width, height, font_sizing),
        PresentationMetrics::Scalable(font_sizing),
        emit,
    )
}

fn create_evaluator(
    app: AndroidApp,
    width: u32,
    height: u32,
    font_sizing: FontSizing,
) -> Result<neovm_core::emacs_core::eval::Context, String> {
    let app_data = app
        .internal_data_path()
        .ok_or_else(|| "Android did not provide an internal data directory".to_owned())?;
    let runtime_root = app_data.join("neomacs-runtime");
    let asset_manager = app.asset_manager();
    let image = ExtractedRuntimeImage::prepare_final_from_portable(&runtime_root, |asset_name| {
        let asset_name = CString::new(asset_name).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "runtime image asset name contains NUL",
            )
        })?;
        asset_manager.open(&asset_name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "packaged Android portable runtime image {} was not found",
                    asset_name.to_string_lossy()
                ),
            )
        })
    })
    .map_err(|error| format!("failed to provision Android runtime image: {error}"))?;

    let mut evaluator = RuntimeImageSource::ExtractedFile(image.path())
        .load_for_in_runtime_root(HostProfile::android(), &runtime_root)
        .map_err(|error| format!("failed to restore Android runtime image: {error}"))?;
    evaluator.setup_thread_locals();
    evaluator.set_max_depth(1600);

    let font_pixel_size = font_sizing.face_height_to_layout_pixels(100);
    let metrics = FontMetricsService::new().font_metrics("Monospace", 400, false, font_pixel_size);
    let frame_metrics = InitialFrameMetrics::new(
        width,
        height,
        metrics.char_width.max(1.0),
        metrics.line_height.max(1.0),
        font_pixel_size,
    )
    .map_err(|error| format!("invalid Android opening geometry: {error}"))?;
    let surface = prepare_initial_editor_surface(
        &mut evaluator,
        InitialEditorSurfaceSpec::gui(
            frame_metrics,
            Default::default(),
            InitialDisplayType::Color,
            InitialBackgroundMode::Light,
            InitialFrameFont::named("Monospace"),
        ),
    );
    let startup = InteractiveGuiStartup::new("neomacs-android", &runtime_root, &app_data);
    configure_interactive_gui_startup(&mut evaluator, surface, &startup)
        .map_err(|error| format!("failed to initialize Android GUI startup: {error}"))?;
    Ok(evaluator)
}
