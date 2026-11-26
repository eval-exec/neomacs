//! Private schemas and a private backend exercise the real subscription callback
//! without reading or mutating the process's desktop settings backend.

use super::*;
use gio::glib::variant::ToVariant;

fn private_settings(raster_keys: &str) -> (tempfile::TempDir, SettingsSchema, Settings) {
    let directory = tempfile::tempdir().expect("private GSettings schema directory");
    std::fs::write(
        directory.path().join("test.gschema.xml"),
        format!(
            "<schemalist><schema id='org.neomacs.FontRasterTest' path='/org/neomacs/font-raster-test/'>
             <key name='monospace-font-name' type='s'><default>'Monospace 12'</default></key>
             <key name='font-name' type='s'><default>'Sans 11'</default></key>
             <key name='text-scaling-factor' type='d'><default>1.5</default></key>
             {raster_keys}</schema></schemalist>"
        ),
    )
    .expect("write private schema");
    let compiled = std::process::Command::new("glib-compile-schemas")
        .arg("--strict")
        .arg(directory.path())
        .output()
        .expect("GSettings subscription tests require glib-compile-schemas");
    assert!(
        compiled.status.success(),
        "private schema compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let source = SettingsSchemaSource::from_directory(directory.path(), None, false)
        .expect("load private schema source");
    let schema = source
        .lookup("org.neomacs.FontRasterTest", false)
        .expect("private schema");
    let backend = gio::memory_settings_backend_new();
    let settings = Settings::new_full(&schema, Some(&backend), None);
    (directory, schema, settings)
}

const RASTER_KEYS: &str = "
    <key name='font-antialiasing' type='s'><default>'grayscale'</default></key>
    <key name='font-hinting' type='s'><default>'slight'</default></key>
    <key name='font-rgba-order' type='s'><default>'rgb'</default></key>";

fn pgtk(antialias: &str, hinting: &str, rgba: &str) -> SurfaceFontOptions {
    SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
        antialias: GSettingsValue::String(antialias),
        hinting: GSettingsValue::String(hinting),
        rgba: GSettingsValue::String(rgba),
    })
}

fn dispatch(context: &gio::glib::MainContext) {
    // Private memory-backend writes queue only local notifications. Never wait
    // on the default context or a desktop service to make these rows progress.
    while context.pending() {
        context.iteration(false);
    }
}

#[test]
fn raster_subscription_unknown_value_delivers_one_event_with_unchanged_options() {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| {
            let (_directory, schema, settings) = private_settings(RASTER_KEYS);
            let (font_tx, fonts) = crossbeam_channel::unbounded();
            let (raster_tx, raster) = crossbeam_channel::unbounded();
            let observed = watch_settings(&schema, &settings, font_tx, raster_tx);
            settings
                .set_string("font-hinting", "future-unknown")
                .unwrap();
            dispatch(&context);
            assert_eq!(
                raster.try_iter().collect::<Vec<_>>(),
                vec![observed.raster_options]
            );
            assert!(fonts.is_empty());
            settings.disconnect(observed.handler);
        })
        .unwrap();
}

#[test]
fn raster_subscription_two_key_changes_deliver_two_events_with_unchanged_options() {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| {
            let (_directory, schema, settings) = private_settings(RASTER_KEYS);
            let (font_tx, fonts) = crossbeam_channel::unbounded();
            let (raster_tx, raster) = crossbeam_channel::unbounded();
            let observed = watch_settings(&schema, &settings, font_tx, raster_tx);
            settings
                .set_string("font-hinting", "future-hinting")
                .unwrap();
            settings
                .set_string("font-antialiasing", "future-antialiasing")
                .unwrap();
            dispatch(&context);
            assert_eq!(
                raster.try_iter().collect::<Vec<_>>(),
                vec![observed.raster_options, observed.raster_options]
            );
            assert!(fonts.is_empty());
            settings.disconnect(observed.handler);
        })
        .unwrap();
}

#[test]
fn raster_subscription_captures_startup_and_keeps_font_notifications_independent() {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| {
            let (_directory, schema, settings) = private_settings(RASTER_KEYS);
            let (font_tx, fonts) = crossbeam_channel::unbounded();
            let (raster_tx, raster) = crossbeam_channel::unbounded();
            let observed = watch_settings(&schema, &settings, font_tx, raster_tx);
            let initial = pgtk("grayscale", "slight", "rgb");
            assert_eq!(observed.raster_options, initial);
            assert_eq!(observed.text_dpi.map(Dpi::get), Some(144.0));
            assert!(fonts.is_empty());
            assert!(raster.is_empty());

            settings.set_string("font-hinting", "full").unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), pgtk("grayscale", "full", "rgb"));
            assert!(
                fonts.is_empty(),
                "raster changes must not reopen named fonts"
            );
            // Backends may suppress identical writes. Once a changed signal is
            // emitted, GNU still delivers font-render for the unchanged policy.
            settings.emit_by_name::<()>("changed", &[&"font-hinting"]);
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), pgtk("grayscale", "full", "rgb"));
            assert!(raster.is_empty());

            settings
                .set_string("monospace-font-name", "Monospace 18")
                .unwrap();
            dispatch(&context);
            assert_ne!(fonts.try_recv().unwrap(), observed.fonts);
            assert!(raster.is_empty(), "font names do not change raster policy");
            settings.set_string("font-hinting", "slight").unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), initial);
            assert!(fonts.is_empty());
            settings.disconnect(observed.handler);
        })
        .unwrap();
}

#[test]
fn raster_subscription_retains_last_valid_fields_through_unknown_values() {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| {
            let (_directory, schema, settings) = private_settings(RASTER_KEYS);
            let (font_tx, _fonts) = crossbeam_channel::unbounded();
            let (raster_tx, raster) = crossbeam_channel::unbounded();
            let observed = watch_settings(&schema, &settings, font_tx, raster_tx);
            settings.set_string("font-hinting", "full").unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), pgtk("grayscale", "full", "rgb"));
            settings
                .set_string("font-hinting", "future-unknown")
                .unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), pgtk("grayscale", "full", "rgb"));
            settings.set_string("font-antialiasing", "rgba").unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), pgtk("rgba", "full", "rgb"));
            settings.set_string("font-rgba-order", "bgr").unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), pgtk("rgba", "full", "bgr"));
            settings.disconnect(observed.handler);
        })
        .unwrap();
}

#[test]
fn raster_subscription_accepts_missing_keys_and_wrong_variant_types() {
    let context = gio::glib::MainContext::new();
    context
        .with_thread_default(|| {
            let (_directory, schema, settings) = private_settings(
                "<key name='font-antialiasing' type='i'><default>1</default></key>
             <key name='font-hinting' type='s'><default>'unknown'</default></key>",
            );
            let (font_tx, _fonts) = crossbeam_channel::unbounded();
            let (raster_tx, raster) = crossbeam_channel::unbounded();
            let observed = watch_settings(&schema, &settings, font_tx, raster_tx);
            assert_eq!(observed.raster_options, SurfaceFontOptions::default());
            assert_eq!(raster_value(None), GSettingsValue::Missing);
            assert_eq!(
                raster_value(Some(&1_i32.to_variant())),
                GSettingsValue::Invalid
            );
            let path = gio::glib::variant::ObjectPath::try_from("/full").unwrap();
            assert_eq!(
                raster_value(Some(&path.to_variant())),
                GSettingsValue::Invalid
            );
            settings.set_int("font-antialiasing", 2).unwrap();
            dispatch(&context);
            assert_eq!(raster.try_recv().unwrap(), SurfaceFontOptions::default());
            settings.set_string("font-hinting", "full").unwrap();
            dispatch(&context);
            assert_eq!(
                raster.try_recv().unwrap(),
                SurfaceFontOptions::from_pgtk_settings(PgtkFontSettings {
                    hinting: GSettingsValue::String("full"),
                    ..Default::default()
                })
            );
            settings.disconnect(observed.handler);
        })
        .unwrap();
}
