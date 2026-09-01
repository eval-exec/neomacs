//! Generate the Linux WPEPlatform/WebKit FFI boundary.
//!
//! Generates Rust bindings for WPE WebKit using bindgen.

#[cfg(feature = "webview")]
use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(wpe_platform_available)");

    #[cfg(feature = "webview")]
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "linux" {
        generate_wpe_bindings(&PathBuf::from(env::var("OUT_DIR").unwrap()));
    }
}

#[cfg(feature = "webview")]
fn generate_wpe_bindings(out_dir: &Path) {
    let wpe_webkit = pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("wpe-webkit-2.0")
        .expect("the webview feature on Linux requires wpe-webkit-2.0");

    generate_wpe_webkit_bindings(out_dir, &wpe_webkit);
    generate_wpe_platform_bindings(out_dir);
}

#[cfg(feature = "webview")]
fn generate_wpe_webkit_bindings(out_dir: &Path, wpe_webkit: &pkg_config::Library) {
    let webkit_header = required_header(wpe_webkit, "wpe/webkit.h");
    let jsc_header = required_header(wpe_webkit, "jsc/jsc.h");
    let mut builder = bindgen::Builder::default()
        .header(webkit_header.to_string_lossy())
        .header(jsc_header.to_string_lossy())
        // WebKit core types
        .allowlist_function("webkit_.*")
        .allowlist_type("WebKit.*")
        .allowlist_var("WEBKIT_.*")
        .allowlist_function("jsc_value_.*")
        // GObject basics we need
        .allowlist_function("g_object_unref")
        .allowlist_function("g_object_ref")
        .allowlist_function("g_free")
        .allowlist_type("GObject")
        .allowlist_type("GType")
        .allowlist_type("gboolean")
        .allowlist_type("gchar")
        .allowlist_type("gpointer")
        .allowlist_type("gdouble")
        .allowlist_type("guint")
        .allowlist_type("guint32")
        .allowlist_type("gint")
        // Generate opaque types for complex GLib types
        .opaque_type("_GValue")
        .opaque_type("_GTypeClass")
        .opaque_type("_GTypeInstance")
        .opaque_type("_GData")
        .opaque_type("_GList")
        .opaque_type("_GSList")
        .opaque_type("_GError")
        .opaque_type("_GBytes")
        .opaque_type("_GVariant")
        .opaque_type("_GCancellable")
        .opaque_type("_GInputStream")
        .opaque_type("_GOutputStream")
        .opaque_type("_GAsyncResult")
        .opaque_type("_GTlsCertificate")
        .opaque_type("_GUri")
        .opaque_type("_cairo.*")
        .opaque_type("_SoupMessage.*")
        .opaque_type("_JSC.*")
        // Blocklist types that are defined in wpe_sys
        .blocklist_type("wpe_view_backend")
        .blocklist_type("wpe_view_backend_.*")
        .generate_comments(true)
        .derive_debug(true)
        .derive_default(true);

    // Add include paths
    for path in &wpe_webkit.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    // Need GLib headers
    if let Ok(glib) = pkg_config::Config::new().probe("glib-2.0") {
        for path in &glib.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
    }

    // Need libsoup headers
    if let Ok(soup) = pkg_config::Config::new().probe("libsoup-3.0") {
        for path in &soup.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
    }

    let bindings = builder
        .generate()
        .expect("Failed to generate wpe-webkit bindings");

    bindings
        .write_to_file(out_dir.join("wpe_webkit_sys.rs"))
        .expect("Failed to write wpe_webkit_sys.rs");

    // Link
    for lib in &wpe_webkit.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
    for path in &wpe_webkit.link_paths {
        println!("cargo:rustc-link-search={}", path.display());
    }
}

#[cfg(feature = "webview")]
fn generate_wpe_platform_bindings(out_dir: &Path) {
    let wpe_platform = pkg_config::Config::new()
        .probe("wpe-platform-2.0")
        .expect("the webview feature on Linux requires wpe-platform-2.0");
    let wpe_headless = pkg_config::Config::new()
        .probe("wpe-platform-headless-2.0")
        .expect("the webview feature on Linux requires wpe-platform-headless-2.0");
    let glib = pkg_config::Config::new()
        .probe("glib-2.0")
        .expect("the webview feature on Linux requires glib-2.0");

    let platform_header = required_header(&wpe_platform, "wpe/wpe-platform.h");
    let headless_header = required_header(&wpe_headless, "wpe/headless/wpe-headless.h");
    let glib_object_header = required_header(&glib, "glib-object.h");

    let mut builder = bindgen::Builder::default()
        .header(platform_header.to_string_lossy())
        .header(headless_header.to_string_lossy())
        .header(glib_object_header.to_string_lossy())
        // WPEDisplay functions
        .allowlist_function("wpe_display_.*")
        .allowlist_type("WPEDisplay.*")
        // WPEView functions
        .allowlist_function("wpe_view_.*")
        .allowlist_type("WPEView.*")
        // WPEBuffer functions
        .allowlist_function("wpe_buffer_.*")
        .allowlist_type("WPEBuffer.*")
        // WPEToplevel functions
        .allowlist_function("wpe_toplevel_.*")
        .allowlist_type("WPEToplevel.*")
        // WPEEvent functions
        .allowlist_function("wpe_event_.*")
        .allowlist_type("WPEEvent.*")
        // WPERectangle
        .allowlist_type("WPERectangle")
        // GObject/GLib functions we need
        .allowlist_function("g_object_unref")
        .allowlist_function("g_object_ref")
        .allowlist_function("g_error_free")
        .allowlist_function("g_bytes_get_data")
        .allowlist_function("g_bytes_unref")
        .allowlist_function("g_bytes_get_size")
        .allowlist_function("g_signal_connect_data")
        .allowlist_function("g_type_check_instance_is_a")
        // GObject/GLib types we need
        .allowlist_type("GError")
        .allowlist_type("GBytes")
        .allowlist_type("gboolean")
        .allowlist_type("gpointer")
        .allowlist_type("guint")
        .allowlist_type("gsize")
        .allowlist_type("GType")
        .allowlist_type("GQuark")
        .allowlist_type("gdouble")
        .allowlist_type("GTypeInstance")
        .generate_comments(true)
        .derive_debug(true)
        .derive_default(true);

    // Add include paths from wpe-platform-2.0
    for path in &wpe_platform.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    // Add headless include paths if available
    for path in &wpe_headless.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    // Need glib headers
    for path in &glib.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    // Need EGL headers
    if let Ok(egl) = pkg_config::Config::new().probe("egl") {
        for path in &egl.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
    }

    let bindings = builder
        .generate()
        .expect("Failed to generate wpe-platform bindings");

    bindings
        .write_to_file(out_dir.join("wpe_platform_sys.rs"))
        .expect("Failed to write wpe_platform_sys.rs");

    // Link wpe-platform
    for lib in &wpe_platform.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
    for path in &wpe_platform.link_paths {
        println!("cargo:rustc-link-search={}", path.display());
    }

    // Link headless platform if available
    for lib in &wpe_headless.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
    for path in &wpe_headless.link_paths {
        println!("cargo:rustc-link-search={}", path.display());
    }

    println!("cargo:rustc-cfg=wpe_platform_available");
}

#[cfg(feature = "webview")]
fn required_header(library: &pkg_config::Library, relative: &str) -> PathBuf {
    library
        .include_paths
        .iter()
        .map(|include| include.join(relative))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| {
            panic!(
                "pkg-config package {} did not expose required header {relative}",
                library.libs.join(", ")
            )
        })
}
