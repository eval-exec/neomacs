//! Build script for neomacs-display
//!
//! Generates:
//! 1. C header files for FFI using cbindgen
//! 2. Rust bindings for WPE WebKit using bindgen

use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Allow the wpe_platform_available cfg flag
    println!("cargo::rustc-check-cfg=cfg(wpe_platform_available)");

    // Generate C headers with cbindgen
    generate_c_headers(&crate_dir);

    // Generate WPE bindings with bindgen (if feature enabled)
    #[cfg(feature = "wpe-webkit")]
    generate_wpe_bindings(&out_dir);
}

fn generate_c_headers(crate_dir: &str) {
    if which::which("cbindgen").is_ok() {
        let output_file = PathBuf::from(crate_dir)
            .join("include")
            .join("neomacs_display.h");

        std::fs::create_dir_all(PathBuf::from(crate_dir).join("include")).ok();

        let config = cbindgen::Config::from_file("cbindgen.toml").unwrap_or_default();

        cbindgen::Builder::new()
            .with_crate(crate_dir)
            .with_config(config)
            .generate()
            .map(|bindings| bindings.write_to_file(&output_file))
            .ok();

        println!("cargo:rerun-if-changed=src/ffi.rs");
        println!("cargo:rerun-if-changed=cbindgen.toml");
    }
}

#[cfg(feature = "wpe-webkit")]
fn generate_wpe_bindings(out_dir: &PathBuf) {
    // Find WPE libraries using pkg-config
    let wpe = pkg_config::Config::new()
        .atleast_version("1.0")
        .probe("wpe-1.0");

    let wpe_fdo = pkg_config::Config::new()
        .atleast_version("1.0")
        .probe("wpebackend-fdo-1.0");

    let wpe_webkit = pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("wpe-webkit-2.0");

    // Generate EGL bindings for DMA-BUF export
    generate_egl_bindings(out_dir);

    // Generate libwpe bindings
    if let Ok(wpe) = wpe {
        generate_libwpe_bindings(out_dir, &wpe);
    } else {
        println!("cargo:warning=libwpe not found, WPE WebKit support disabled");
    }

    // Generate wpebackend-fdo bindings
    if let Ok(wpe_fdo) = wpe_fdo {
        generate_wpe_fdo_bindings(out_dir, &wpe_fdo);
    } else {
        println!("cargo:warning=wpebackend-fdo not found, WPE WebKit support disabled");
    }

    // Link wpe-webkit and generate bindings
    if let Ok(wpe_webkit) = wpe_webkit {
        generate_wpe_webkit_bindings(out_dir, &wpe_webkit);
    } else {
        println!("cargo:warning=wpe-webkit not found, WPE WebKit support disabled");
    }

    // Generate WPE Platform API bindings (for dma-buf GPU rendering)
    generate_wpe_platform_bindings(out_dir);
}

#[cfg(feature = "wpe-webkit")]
fn generate_egl_bindings(out_dir: &PathBuf) {
    // EGL bindings for DMA-BUF export
    let egl = pkg_config::Config::new().probe("egl");
    
    if let Ok(egl) = egl {
        let mut builder = bindgen::Builder::default()
            .header_contents(
                "egl_wrapper.h",
                r#"
                #define EGL_EGLEXT_PROTOTYPES
                #include <EGL/egl.h>
                #include <EGL/eglext.h>
                "#,
            )
            // Core EGL types
            .allowlist_type("EGLDisplay")
            .allowlist_type("EGLSurface")
            .allowlist_type("EGLContext")
            .allowlist_type("EGLConfig")
            .allowlist_type("EGLImage")
            .allowlist_type("EGLImageKHR")
            .allowlist_type("EGLint")
            .allowlist_type("EGLBoolean")
            .allowlist_type("EGLAttrib")
            // Core EGL functions
            .allowlist_function("eglGetDisplay")
            .allowlist_function("eglInitialize")
            .allowlist_function("eglGetProcAddress")
            .allowlist_function("eglQueryString")
            .allowlist_function("eglGetCurrentDisplay")
            .allowlist_function("eglGetCurrentContext")
            // DMA-BUF related constants
            .allowlist_var("EGL_.*")
            .generate_comments(true)
            .derive_debug(true);

        // Add include paths
        for path in &egl.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }

        let bindings = builder
            .generate()
            .expect("Failed to generate EGL bindings");

        bindings
            .write_to_file(out_dir.join("egl_sys.rs"))
            .expect("Failed to write egl_sys.rs");

        // Link EGL
        for lib in &egl.libs {
            println!("cargo:rustc-link-lib={}", lib);
        }
        for path in &egl.link_paths {
            println!("cargo:rustc-link-search={}", path.display());
        }
    } else {
        println!("cargo:warning=EGL not found");
    }
}

#[cfg(feature = "wpe-webkit")]
fn generate_wpe_webkit_bindings(out_dir: &PathBuf, wpe_webkit: &pkg_config::Library) {
    let mut builder = bindgen::Builder::default()
        .header_contents(
            "wpe_webkit_wrapper.h",
            r#"
            #include <wpe/webkit.h>
            "#,
        )
        // WebKit core types
        .allowlist_function("webkit_.*")
        .allowlist_type("WebKit.*")
        .allowlist_var("WEBKIT_.*")
        // GObject basics we need
        .allowlist_function("g_object_unref")
        .allowlist_function("g_object_ref")
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

#[cfg(feature = "wpe-webkit")]
fn generate_libwpe_bindings(out_dir: &PathBuf, wpe: &pkg_config::Library) {
    let mut builder = bindgen::Builder::default()
        .header_contents(
            "wpe_wrapper.h",
            r#"
            #include <wpe/wpe.h>
            "#,
        )
        .allowlist_function("wpe_.*")
        .allowlist_type("wpe_.*")
        .allowlist_var("WPE_.*")
        .generate_comments(true)
        .derive_debug(true)
        .derive_default(true);

    // Add include paths
    for path in &wpe.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder
        .generate()
        .expect("Failed to generate libwpe bindings");

    bindings
        .write_to_file(out_dir.join("wpe_sys.rs"))
        .expect("Failed to write wpe_sys.rs");

    // Link
    for lib in &wpe.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
    for path in &wpe.link_paths {
        println!("cargo:rustc-link-search={}", path.display());
    }

    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(feature = "wpe-webkit")]
fn generate_wpe_fdo_bindings(out_dir: &PathBuf, wpe_fdo: &pkg_config::Library) {
    let mut builder = bindgen::Builder::default()
        .header_contents(
            "wpe_fdo_wrapper.h",
            r#"
            #include <wpe/fdo.h>
            #include <wpe/fdo-egl.h>
            #include <wpe/unstable/fdo-shm.h>
            #include <wpe/unstable/fdo-dmabuf.h>
            // Include wayland-server for wl_shm_buffer functions
            #include <wayland-server-core.h>
            "#,
        )
        .allowlist_function("wpe_fdo_.*")
        .allowlist_function("wpe_view_backend_exportable_fdo.*")
        .allowlist_type("wpe_fdo_.*")
        .allowlist_type("wpe_view_backend_exportable_fdo.*")
        .allowlist_type("EGLImageKHR")
        .allowlist_var("WPE_FDO_.*")
        // wl_shm_buffer functions for SHM mode
        .allowlist_function("wl_shm_buffer_get_data")
        .allowlist_function("wl_shm_buffer_get_stride")
        .allowlist_function("wl_shm_buffer_get_format")
        .allowlist_function("wl_shm_buffer_get_width")
        .allowlist_function("wl_shm_buffer_get_height")
        .allowlist_function("wl_shm_buffer_begin_access")
        .allowlist_function("wl_shm_buffer_end_access")
        // wl_shm format constants
        .allowlist_var("WL_SHM_FORMAT_.*")
        .generate_comments(true)
        .derive_debug(true)
        .derive_default(true)
        // Block types that are defined in wpe_sys
        .blocklist_type("wpe_view_backend");

    // Add include paths
    for path in &wpe_fdo.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    // Also need EGL headers
    if let Ok(egl) = pkg_config::Config::new().probe("egl") {
        for path in &egl.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
    }

    // Need wayland-server headers for wl_shm_buffer
    if let Ok(wayland_server) = pkg_config::Config::new().probe("wayland-server") {
        for path in &wayland_server.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
        // Link wayland-server
        for lib in &wayland_server.libs {
            println!("cargo:rustc-link-lib={}", lib);
        }
        for path in &wayland_server.link_paths {
            println!("cargo:rustc-link-search={}", path.display());
        }
    }

    let bindings = builder
        .generate()
        .expect("Failed to generate wpe-fdo bindings");

    bindings
        .write_to_file(out_dir.join("wpe_fdo_sys.rs"))
        .expect("Failed to write wpe_fdo_sys.rs");

    // Link
    for lib in &wpe_fdo.libs {
        println!("cargo:rustc-link-lib={}", lib);
    }
    for path in &wpe_fdo.link_paths {
        println!("cargo:rustc-link-search={}", path.display());
    }
}

#[cfg(feature = "wpe-webkit")]
fn generate_wpe_platform_bindings(out_dir: &PathBuf) {
    // Check if wpe-platform-2.0 is available
    let wpe_platform = match pkg_config::Config::new().probe("wpe-platform-2.0") {
        Ok(lib) => lib,
        Err(e) => {
            println!("cargo:warning=wpe-platform-2.0 not found: {}, skipping WPE Platform bindings", e);
            return;
        }
    };

    // Also get headless platform
    let wpe_headless = pkg_config::Config::new()
        .probe("wpe-platform-headless-2.0")
        .ok();

    let mut builder = bindgen::Builder::default()
        .header_contents(
            "wpe_platform_wrapper.h",
            r#"
            #include <wpe/wpe-platform.h>
            #include <wpe/headless/wpe-headless.h>
            #include <glib-object.h>
            "#,
        )
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
        .generate_comments(true)
        .derive_debug(true)
        .derive_default(true);

    // Add include paths from wpe-platform-2.0
    for path in &wpe_platform.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    // Add headless include paths if available
    if let Some(ref headless) = wpe_headless {
        for path in &headless.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
    }

    // Need glib headers
    if let Ok(glib) = pkg_config::Config::new().probe("glib-2.0") {
        for path in &glib.include_paths {
            builder = builder.clang_arg(format!("-I{}", path.display()));
        }
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
    if let Some(ref headless) = wpe_headless {
        for lib in &headless.libs {
            println!("cargo:rustc-link-lib={}", lib);
        }
        for path in &headless.link_paths {
            println!("cargo:rustc-link-search={}", path.display());
        }
    }

    println!("cargo:rustc-cfg=wpe_platform_available");
}
