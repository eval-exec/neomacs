//! GNU xsettings.c's GSettings preferences, owned by one native context.

use gio::{Settings, SettingsBackend, SettingsSchema, SettingsSchemaSource, prelude::*};
use neomacs_display_protocol::Dpi;
use neomacs_font_materializer::fontconfig::{GSettingsValue, PgtkFontSettings, SurfaceFontOptions};
use neovm_core::emacs_core::display_host::{SystemFontName, SystemFontRole, SystemFonts};
use std::{
    cell::RefCell,
    io,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub(super) struct Subscription {
    context: gio::glib::MainContext,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // GLib guarantees that an early wakeup makes the next iteration
        // return without blocking, including the check-before-wait race.
        self.context.wakeup();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::error!("desktop font subscription worker panicked");
        }
    }
}

fn read_font(
    schema: &SettingsSchema,
    settings: &Settings,
    role: SystemFontRole,
) -> Option<SystemFontName> {
    let key = match role {
        SystemFontRole::Monospace => "monospace-font-name",
        SystemFontRole::Application => "font-name",
    };
    if !schema.has_key(key) {
        return None;
    }
    // Inspect the variant instead of using the asserting string getter: an
    // absent/malformed system schema must not abort the editor.
    let value = settings.value(key);
    SystemFontName::new(value.str()?.to_owned())
}

fn read_fonts(schema: &SettingsSchema, settings: &Settings) -> SystemFonts {
    SystemFonts::new(
        read_font(schema, settings, SystemFontRole::Monospace),
        read_font(schema, settings, SystemFontRole::Application),
    )
}

fn text_dpi_from_factor(value: Option<&gio::glib::Variant>) -> Option<Dpi> {
    let factor = value?.get::<f64>()?;
    // GNOME's text baseline is 96 DPI. Its compositor/window scale is a
    // separate setting and must not enter Wayland's logical font size.
    Dpi::new((96.0 * factor) as f32).ok()
}

fn read_text_dpi(schema: &SettingsSchema, settings: &Settings) -> Option<Dpi> {
    let key = "text-scaling-factor";
    if !schema.has_key(key) {
        return None;
    }
    text_dpi_from_factor(Some(&settings.value(key)))
}

fn read_value(
    schema: &SettingsSchema,
    settings: &Settings,
    key: &str,
) -> Option<gio::glib::Variant> {
    schema.has_key(key).then(|| settings.value(key))
}

fn raster_value(value: Option<&gio::glib::Variant>) -> GSettingsValue<'_> {
    match value {
        None => GSettingsValue::Missing,
        // Variant::str also accepts object paths and signatures. GNU checks
        // G_VARIANT_TYPE_STRING specifically before reading these settings.
        Some(value) if value.type_() == gio::glib::VariantTy::STRING => match value.str() {
            Some(value) => GSettingsValue::String(value),
            None => GSettingsValue::Invalid,
        },
        Some(_) => GSettingsValue::Invalid,
    }
}

fn read_raster_options(
    previous: SurfaceFontOptions,
    schema: &SettingsSchema,
    settings: &Settings,
) -> SurfaceFontOptions {
    let antialias = read_value(schema, settings, "font-antialiasing");
    let hinting = read_value(schema, settings, "font-hinting");
    let rgba = read_value(schema, settings, "font-rgba-order");
    // GNU xsettings.c retains each Cairo option when its GSettings value is
    // absent, has the wrong variant type, or names an unknown value.
    previous.with_pgtk_settings(PgtkFontSettings {
        antialias: raster_value(antialias.as_ref()),
        hinting: raster_value(hinting.as_ref()),
        rgba: raster_value(rgba.as_ref()),
    })
}

struct SettingsObservation {
    fonts: SystemFonts,
    text_dpi: Option<Dpi>,
    raster_options: SurfaceFontOptions,
    handler: gio::glib::SignalHandlerId,
}

fn watch_settings(
    schema: &SettingsSchema,
    settings: &Settings,
    font_changes: crossbeam_channel::Sender<SystemFonts>,
    raster_changes: crossbeam_channel::Sender<SurfaceFontOptions>,
) -> SettingsObservation {
    let fonts = Rc::new(RefCell::new(SystemFonts::default()));
    let raster = Rc::new(RefCell::new(SurfaceFontOptions::default()));
    let observed_fonts = Rc::clone(&fonts);
    let observed_raster = Rc::clone(&raster);
    let watched_schema = schema.clone();
    let handler = settings.connect_changed(None, move |settings, key| {
        match key {
            "monospace-font-name" | "font-name" => {
                let next = read_fonts(&watched_schema, settings);
                if *observed_fonts.borrow() != next {
                    *observed_fonts.borrow_mut() = next.clone();
                    let _ = font_changes.send(next);
                }
            }
            "font-antialiasing" | "font-hinting" | "font-rgba-order" => {
                let next =
                    read_raster_options(*observed_raster.borrow(), &watched_schema, settings);
                *observed_raster.borrow_mut() = next;
                // GNU xsettings.c:446-459 emits font-render after every
                // changed signal, even if apply_* retains the old options.
                // Native cache policy may deduplicate; Lisp notification may not.
                let _ = raster_changes.send(next);
            }
            _ => {}
        }
    });
    // GSettings only emits changed for keys read after connecting. Capture all
    // startup values on the same context that owns the backend and callbacks.
    let initial_fonts = read_fonts(schema, settings);
    *fonts.borrow_mut() = initial_fonts.clone();
    let raster_options = read_raster_options(SurfaceFontOptions::default(), schema, settings);
    *raster.borrow_mut() = raster_options;
    SettingsObservation {
        fonts: initial_fonts,
        text_dpi: read_text_dpi(schema, settings),
        raster_options,
        handler,
    }
}

pub(super) fn observe() -> io::Result<super::FontDefaultsObserver> {
    let context = gio::glib::MainContext::new();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_context = context.clone();
    let worker_stop = Arc::clone(&stop);
    let (initial_tx, initial_rx) = crossbeam_channel::bounded(1);
    let (changes_tx, changes) = crossbeam_channel::unbounded();
    let (raster_tx, raster_changes) = crossbeam_channel::unbounded();
    let worker = std::thread::Builder::new()
        .name("desktop-font-settings".into())
        .spawn(move || {
            worker_context
                .with_thread_default(|| {
                    let Some(schema) = SettingsSchemaSource::default()
                        .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
                        .filter(|schema| schema.path().is_some())
                    else {
                        let _ = initial_tx.send(None);
                        return;
                    };
                    // The first default-backend construction also owns its file
                    // monitor context. Startup and live reads must happen here.
                    let settings = Settings::new_full(&schema, None::<&SettingsBackend>, None);
                    let observation = watch_settings(&schema, &settings, changes_tx, raster_tx);
                    if initial_tx
                        .send(Some((
                            observation.fonts,
                            observation.text_dpi,
                            observation.raster_options,
                        )))
                        .is_ok()
                    {
                        while !worker_stop.load(Ordering::Acquire) {
                            worker_context.iteration(true);
                        }
                    }
                    settings.disconnect(observation.handler);
                    // Settings and callbacks drop on their owning thread. GLib's
                    // default backend itself is a process-lifetime singleton.
                })
                .expect("new desktop settings context must be unowned");
        })?;
    let subscription = Subscription {
        context,
        stop,
        worker: Some(worker),
    };
    match initial_rx.recv().map_err(io::Error::other)? {
        Some((initial, initial_text_dpi, initial_raster_options)) => {
            Ok(super::FontDefaultsObserver {
                initial: super::GuiFontDefaults::Desktop(initial),
                initial_text_dpi,
                changes,
                initial_raster_options,
                raster_changes,
                _subscription: super::NativeSubscription::Linux {
                    _guard: subscription,
                },
            })
        }
        None => {
            drop(subscription);
            Ok(super::FontDefaultsObserver::unsupported(
                super::GuiFontDefaults::Desktop(SystemFonts::default()),
            ))
        }
    }
}

#[cfg(test)]
#[path = "linux_settings_test.rs"]
mod settings_tests;

#[cfg(test)]
mod tests {
    use super::text_dpi_from_factor;
    use gio::glib::variant::ToVariant;

    #[test]
    fn desktop_text_factor_requires_a_positive_finite_double() {
        assert_eq!(text_dpi_from_factor(None), None);
        assert_eq!(text_dpi_from_factor(Some(&"2.0".to_variant())), None);
        assert_eq!(text_dpi_from_factor(Some(&2_i32.to_variant())), None);
        for invalid in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
        ] {
            assert_eq!(text_dpi_from_factor(Some(&invalid.to_variant())), None);
        }
        for (factor, expected) in [(0.5_f64, 48.0), (1.0, 96.0), (1.5, 144.0), (2.0, 192.0)] {
            assert_eq!(
                text_dpi_from_factor(Some(&factor.to_variant())).map(|dpi| dpi.get()),
                Some(expected)
            );
        }
    }
}
