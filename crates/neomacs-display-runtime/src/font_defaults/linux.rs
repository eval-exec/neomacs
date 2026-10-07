//! GNU xsettings.c's GSettings preferences, owned by one native context.

use gio::{Settings, SettingsBackend, SettingsSchema, SettingsSchemaSource, prelude::*};
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
    bypass: Option<Arc<AtomicBool>>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // GLib guarantees that an early wakeup makes the next iteration
        // return without blocking, including the check-before-wait race.
        self.context.wakeup();
        if let Some(worker) = self.worker.take() {
            if let Some(bypass) = &self.bypass {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
                while !worker.is_finished() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                if !worker.is_finished() {
                    bypass.store(true, Ordering::Release);
                    return; // Native worker owns its context; never free it here.
                }
            }
            if worker.join().is_err() {
                tracing::error!("desktop font subscription worker panicked");
            }
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

pub(super) fn observe() -> io::Result<super::FontDefaultsObserver> {
    observe_inner(None)
}

pub(super) fn observe_controlled(
    cancelled: &dyn Fn() -> bool,
    bypass: Arc<AtomicBool>,
) -> io::Result<super::FontDefaultsObserver> {
    observe_inner(Some((cancelled, bypass)))
}

fn observe_inner(
    control: Option<(&dyn Fn() -> bool, Arc<AtomicBool>)>,
) -> io::Result<super::FontDefaultsObserver> {
    let context = gio::glib::MainContext::new();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_context = context.clone();
    let worker_stop = Arc::clone(&stop);
    let (initial_tx, initial_rx) = crossbeam_channel::bounded(1);
    let (changes_tx, changes) = crossbeam_channel::unbounded();
    let worker = std::thread::Builder::new()
        .name("desktop-font-settings".into())
        .spawn(move || {
            if worker_stop.load(Ordering::Acquire) {
                return;
            }
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
                    let current = Rc::new(RefCell::new(SystemFonts::default()));
                    let observed = Rc::clone(&current);
                    let watched_schema = schema.clone();
                    let handler = settings.connect_changed(None, move |settings, key| {
                        if !matches!(key, "monospace-font-name" | "font-name") {
                            return;
                        }
                        let next = read_fonts(&watched_schema, settings);
                        if *observed.borrow() != next {
                            *observed.borrow_mut() = next.clone();
                            let _ = changes_tx.send(next);
                        }
                    });
                    // GSettings only emits changed for keys read after connecting.
                    let initial = read_fonts(&schema, &settings);
                    *current.borrow_mut() = initial.clone();
                    if initial_tx.send(Some(initial)).is_ok() {
                        while !worker_stop.load(Ordering::Acquire) {
                            worker_context.iteration(true);
                        }
                    }
                    settings.disconnect(handler);
                    // Settings and callbacks drop on their owning thread. GLib's
                    // default backend itself is a process-lifetime singleton.
                })
                .expect("new desktop settings context must be unowned");
        })?;
    let subscription = Subscription {
        context,
        stop,
        worker: Some(worker),
        bypass: control.as_ref().map(|(_, bypass)| bypass.clone()),
    };
    let initial = if let Some((cancelled, _)) = control {
        loop {
            if cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Native font observation cancelled",
                ));
            }
            match initial_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                Ok(initial) => break initial,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(error) => return Err(io::Error::other(error)),
            }
        }
    } else {
        initial_rx.recv().map_err(io::Error::other)?
    };
    match initial {
        Some(initial) => Ok(super::FontDefaultsObserver {
            initial: super::GuiFontDefaults::Desktop(initial),
            changes,
            _subscription: super::NativeSubscription::Linux {
                _guard: subscription,
            },
        }),
        None => {
            drop(subscription);
            Ok(super::FontDefaultsObserver::unsupported(
                super::GuiFontDefaults::Desktop(SystemFonts::default()),
            ))
        }
    }
}
