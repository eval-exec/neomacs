//! GNU xsettings.c's GSettings font discovery, without GTK initialization.

use gio::{Settings, SettingsBackend, SettingsSchema, SettingsSchemaSource, prelude::*};
use neovm_core::emacs_core::display_host::{SystemFontName, SystemFontRole, SystemFonts};

fn read_font(schema: &SettingsSchema, settings: &Settings, role: SystemFontRole) -> Option<SystemFontName> {
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

pub(super) fn read_system_fonts() -> SystemFonts {
    let Some(schema) = SettingsSchemaSource::default()
        .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
    else {
        return SystemFonts::default();
    };
    if schema.path().is_none() {
        return SystemFonts::default();
    }
    let settings = Settings::new_full(&schema, None::<&SettingsBackend>, None);
    SystemFonts::new(
        read_font(&schema, &settings, SystemFontRole::Monospace),
        read_font(&schema, &settings, SystemFontRole::Application),
    )
}
