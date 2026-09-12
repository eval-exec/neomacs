//! Exercise platform policy through Lisp using a real, process-local Fontconfig
//! catalog. This does not pretend to run AppKit or DirectWrite on Linux.

use super::*;
use neomacs_display_protocol::{DisplayObservation, GraphicalBackend, GraphicalDisplayIdentity};
use neomacs_display_runtime::font_defaults::GuiFontDefaults;
use neovm_core::emacs_core::display_host::SystemFontName;
use std::{fs, path::PathBuf, process::Command};

#[derive(Clone, Copy, Debug, strum::EnumString, strum::AsRefStr)]
enum CatalogCase {
    AllWindowsFamilies,
    MissingCourierNew,
    OnlyFixedsys,
    PortableFallback,
    CocoaNamedDefault,
}

impl CatalogCase {
    fn families(self) -> &'static [&'static str] {
        match self {
            Self::AllWindowsFamilies => &["Courier New", "Courier", "Fixedsys", "Test Mono"],
            Self::MissingCourierNew => &["Courier", "Fixedsys", "Test Mono"],
            Self::OnlyFixedsys => &["Fixedsys", "Test Mono"],
            Self::PortableFallback | Self::CocoaNamedDefault => &["Test Mono"],
        }
    }
}

#[test]
fn windows_startup_prefers_courier_new_over_other_available_families() {
    check_in_isolated_catalog(CatalogCase::AllWindowsFamilies);
}

#[test]
fn windows_startup_advances_to_courier_when_courier_new_is_missing() {
    check_in_isolated_catalog(CatalogCase::MissingCourierNew);
}

#[test]
fn windows_startup_uses_fixedsys_pixel_size_after_courier_families_fail() {
    check_in_isolated_catalog(CatalogCase::OnlyFixedsys);
}

#[test]
fn windows_startup_retains_neomacs_portable_last_resort() {
    check_in_isolated_catalog(CatalogCase::PortableFallback);
}

#[test]
fn cocoa_named_fixed_pitch_default_uses_twelve_points() {
    check_in_isolated_catalog(CatalogCase::CocoaNamedDefault);
}

fn check_in_isolated_catalog(case: CatalogCase) {
    let artifact_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/neomacs-test-font-catalogs");
    fs::create_dir_all(&artifact_root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix(case.as_ref())
        .tempdir_in(artifact_root)
        .unwrap();
    let root = directory.path();
    let fonts = root.join("fonts");
    fs::create_dir_all(&fonts).unwrap();
    let fonts = fonts.canonicalize().unwrap();
    let matched = Command::new("fc-match")
        .env_remove("FONTCONFIG_FILE")
        .args(["-f", "%{file}", "DejaVu Sans Mono"])
        .output()
        .expect("resolve outline test font");
    assert!(matched.status.success());
    let source = PathBuf::from(String::from_utf8(matched.stdout).unwrap());
    assert!(source.is_file(), "outline test font must exist: {source:?}");
    let mut config = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\"><fontconfig><dir>{}</dir>",
        fonts.display(),
    );
    for (index, family) in case.families().iter().enumerate() {
        let font = fonts.join(format!("face-{index}.ttf"));
        fs::copy(&source, &font).unwrap();
        // Use Fontconfig's native scan rules, not a fake selector. Each case
        // has its own catalog and process; global font caches stay untouched.
        config.push_str(&format!(
            "<match target=\"scan\"><test name=\"file\" compare=\"eq\"><string>{}</string></test><edit name=\"family\" mode=\"assign\"><string>{family}</string></edit></match>",
            font.display(),
        ));
    }
    config.push_str("</fontconfig>");
    let config_path = root.join("fonts.conf");
    fs::write(&config_path, config).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "startup::tests::platform_fonts::isolated_startup_font_probe",
            "--nocapture",
        ])
        .env("FONTCONFIG_FILE", config_path)
        .env("NEOMACS_TEST_FONT_CATALOG", case.as_ref())
        .output()
        .expect("run isolated startup probe");
    assert!(
        output.status.success(),
        "{case:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "invoked by parent tests with a process-local native font catalog"]
fn isolated_startup_font_probe() {
    let case: CatalogCase = std::env::var("NEOMACS_TEST_FONT_CATALOG")
        .unwrap()
        .parse()
        .unwrap();
    let (backend, observation, family, pixels) = match case {
        CatalogCase::AllWindowsFamilies => (
            GraphicalBackend::Windows,
            DisplayObservation::Windows,
            "Courier New",
            13,
        ),
        CatalogCase::MissingCourierNew => (
            GraphicalBackend::Windows,
            DisplayObservation::Windows,
            "Courier",
            13,
        ),
        CatalogCase::OnlyFixedsys => (
            GraphicalBackend::Windows,
            DisplayObservation::Windows,
            "Fixedsys",
            12,
        ),
        CatalogCase::PortableFallback => (
            GraphicalBackend::Windows,
            DisplayObservation::Windows,
            "Test Mono",
            13,
        ),
        CatalogCase::CocoaNamedDefault => (
            GraphicalBackend::Cocoa,
            DisplayObservation::Cocoa,
            "Test Mono",
            12,
        ),
    };
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES).unwrap();
    let mut display = bootstrap_gui_display_config(
        Interactivity::Interactive,
        gui_frame_font_scale_from_observation(observation),
        GraphicalDisplayIdentity::anonymous_connection(backend),
    );
    if matches!(case, CatalogCase::CocoaNamedDefault) {
        display.font_defaults = GuiFontDefaults::Cocoa {
            fixed_pitch: SystemFontName::new("Test Mono".to_owned()),
        };
    }
    bootstrap_buffers(&mut eval, 960, 640, display);
    let actual_family = eval.eval_str("(face-attribute 'default :family)").unwrap();
    assert_eq!(actual_family.as_utf8_str(), Some(family));
    assert_eq!(
        eval.eval_str("(font-get (frame-parameter nil 'font-parameter) :size)")
            .unwrap()
            .as_int(),
        Some(pixels)
    );
    assert!(eval.eval_str("(font-get-system-font)").unwrap().is_nil());
}
