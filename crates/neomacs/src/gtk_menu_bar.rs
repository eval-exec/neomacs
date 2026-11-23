//! GTK menu-bar requisition, measured the way GNU does.
//!
//! `xg_update_frame_menubar` stores `gtk_widget_get_preferred_size` on the
//! live menu bar (`gtkutil.c:4214-4231`). `xg_frame_set_char_size` adds that
//! height, not a text row, to the outer window (`gtkutil.c:1181-1182`).
//! A bare `GtkMenuBar` reports a natural height of 0; the widget GNU measures
//! already has items, so this probe appends one label before asking.

use std::ffi::c_void;
use std::path::PathBuf;

#[repr(C)]
struct GtkRequisition {
    width: i32,
    height: i32,
}

pub(crate) fn install_measured_height() {
    match measure() {
        Some(px) => {
            tracing::info!(px, "gtk menu bar requisition");
            neovm_core::emacs_core::window_cmds::install_toolkit_menu_bar_height(px);
        }
        None => tracing::debug!("gtk menu bar requisition unavailable"),
    }
}

fn measure() -> Option<u32> {
    if std::env::var_os("DISPLAY").is_none() {
        return None;
    }
    // Keep the library mapped. `gtk_init_check` registers process-lifetime
    // state, so closing it after a successful init is unsafe.
    let lib = Box::leak(Box::new(load_gtk()?));
    unsafe { measure_in(lib) }
}

fn load_gtk() -> Option<libloading::Library> {
    try_load("libgtk-3.so.0")
        .or_else(|| try_load("/usr/lib/x86_64-linux-gnu/libgtk-3.so.0"))
        .or_else(|| try_load("/usr/lib64/libgtk-3.so.0"))
        .or_else(|| try_load("/usr/lib/libgtk-3.so.0"))
        .or_else(load_nix_store_gtk)
}

fn try_load(name: impl libloading::AsFilename) -> Option<libloading::Library> {
    unsafe { libloading::Library::new(name) }.ok()
}

/// Nix store entries are not on the default loader path. Several GTK 3
/// builds can coexist; prefer the highest version, then a successful open.
fn load_nix_store_gtk() -> Option<libloading::Library> {
    let mut found: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir("/nix/store").ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(version) = name.split_once("-gtk+3-").map(|(_, rest)| rest) else {
            continue;
        };
        let path = entry.path().join("lib/libgtk-3.so.0");
        if !path.is_file() {
            continue;
        }
        let version = version
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect();
        found.push((version, path));
    }
    found.sort();
    found.into_iter().rev().find_map(|(_, path)| try_load(path))
}

unsafe fn measure_in(lib: &'static libloading::Library) -> Option<u32> {
    let init = unsafe {
        lib.get::<unsafe extern "C" fn(*mut i32, *mut *mut *mut i8) -> i32>(b"gtk_init_check\0")
    }
    .ok()?;
    if unsafe { init(std::ptr::null_mut(), std::ptr::null_mut()) } == 0 {
        return None;
    }
    let new_window =
        unsafe { lib.get::<unsafe extern "C" fn() -> *mut c_void>(b"gtk_offscreen_window_new\0") }
            .ok()?;
    let new_bar =
        unsafe { lib.get::<unsafe extern "C" fn() -> *mut c_void>(b"gtk_menu_bar_new\0") }.ok()?;
    let new_item = unsafe {
        lib.get::<unsafe extern "C" fn(*const i8) -> *mut c_void>(b"gtk_menu_item_new_with_label\0")
    }
    .ok()?;
    let add = unsafe {
        lib.get::<unsafe extern "C" fn(*mut c_void, *mut c_void)>(b"gtk_container_add\0")
    }
    .ok()?;
    let append = unsafe {
        lib.get::<unsafe extern "C" fn(*mut c_void, *mut c_void)>(b"gtk_menu_shell_append\0")
    }
    .ok()?;
    let show_all =
        unsafe { lib.get::<unsafe extern "C" fn(*mut c_void)>(b"gtk_widget_show_all\0") }.ok()?;
    let preferred = unsafe {
        lib.get::<unsafe extern "C" fn(*mut c_void, *mut GtkRequisition, *mut GtkRequisition)>(
            b"gtk_widget_get_preferred_size\0",
        )
    }
    .ok()?;
    let scale_of = unsafe {
        lib.get::<unsafe extern "C" fn(*mut c_void) -> i32>(b"gtk_widget_get_scale_factor\0")
    }
    .ok()?;
    let destroy =
        unsafe { lib.get::<unsafe extern "C" fn(*mut c_void)>(b"gtk_widget_destroy\0") }.ok()?;

    let window = unsafe { new_window() };
    let bar = unsafe { new_bar() };
    let item = unsafe { new_item(c"File".as_ptr()) };
    if window.is_null() || bar.is_null() || item.is_null() {
        return None;
    }
    unsafe {
        add(window, bar);
        append(bar, item);
        show_all(window);
    }
    let mut natural = GtkRequisition {
        width: 0,
        height: 0,
    };
    unsafe { preferred(bar, std::ptr::null_mut(), &mut natural) };
    let scale = unsafe { scale_of(bar) }.max(1) as u32;
    unsafe { destroy(window) };
    let height = u32::try_from(natural.height.max(0))
        .ok()?
        .saturating_mul(scale);
    (height > 0).then_some(height)
}
