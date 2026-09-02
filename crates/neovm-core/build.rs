use std::fs;
use std::path::{Path, PathBuf};

// SINGLE SOURCE OF TRUTH (ledger 206): the recipe for every Lisp file this
// build generates lives in `build_support/generated_lisp.rs` and is included
// HERE and by `crates/xtask/src/main.rs` from the same file, so the two build paths
// cannot produce different bytes for one artifact.  They used to: this build
// script ran a Rust reimplementation of GNU's `admin/unidata/*.awk` while
// xtask ran the awk itself, and whichever went last decided
// `lisp/international/emoji-zwj.el` -- invalidating the `.elc` beside it on
// every profile switch, and shipping a double-escaped flag regexp that stopped
// country flags composing. Same arrangement as
// `emacs_core/runtime/jit/shim_names.rs` below.
#[path = "build_support/generated_lisp.rs"]
mod generated_lisp;

// Single source of truth (R2-C2): the `neovm_jit_*` shim names, shared with
// runtime/jit/aot.rs (MIR_SHIM_NAMES) + crates/neomacs/build.rs via `include!` so the
// emit/salt set and both export sets can never drift.
include!("src/emacs_core/runtime/jit/shim_names.rs");

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let project_root = PathBuf::from(
        std::env::var_os("CARGO_WORKSPACE_DIR")
            .expect("workspace .cargo/config.toml must set CARGO_WORKSPACE_DIR"),
    );

    // GNU's `${configuration}` -- the autoconf host triple that names the
    // architecture-dependent install directory
    // (`archlibdir='${libexecdir}/emacs/${version}/${configuration}'`,
    // configure.ac:290).  Cargo only exposes TARGET to build scripts, so
    // republish it as a rustc-env for `emacs_core::path_exec`.  Deliberately
    // NOT wired into `system-configuration`: that variable answers a pinned
    // GNU spelling for oracle parity and must not start reporting the Rust
    // triple.
    println!(
        "cargo:rustc-env=NEOVM_HOST_TRIPLE={}",
        std::env::var("TARGET").expect("cargo sets TARGET for build scripts")
    );

    detect_lcms2();
    detect_wkwebview();
    ensure_generated_unicode_lisp(&project_root);
    generate_x11_color_table(&project_root, &manifest_dir);

    // R1c call-bearing AOT: export the host's `neovm_jit_*` shims
    // (`#[unsafe(no_mangle)] pub`, anchored by `JIT_SHIM_ANCHOR`) into the TEST
    // binaries' DYNAMIC symbol table, where a `dlopen`'d call/cons AOT `.so` binds
    // its undefined imports against them. `-rdynamic` alone is insufficient under
    // the workspace linker (it doesn't promote these otherwise-unreferenced fns
    // to the dynamic table), so we additionally name each shim with
    // `--export-dynamic-symbol`. `rustc-link-arg-tests` applies ONLY to test
    // binaries — the lib + production binaries are untouched (production export is
    // R2's job). Gated on the `jit` feature (the only config that emits AOT).
    //
    // R2 CARRY-FORWARD (R2-B5): the PRODUCTION binary (neomacs-bin) that loads the
    // dump-time preload `.so` MUST replicate BOTH the `#[unsafe(no_mangle)] pub`
    // shims AND this per-shim `--export-dynamic-symbol` export in ITS build.rs —
    // under the `wild` linker, plain `-rdynamic` does NOT promote these
    // address-only-referenced fns to the dynamic table, so the preload `.so`'s
    // `neovm_jit_*` imports would otherwise fail to resolve at dlopen and abort on
    // first shim call. Do not assume `-rdynamic` alone suffices.
    if std::env::var_os("CARGO_FEATURE_JIT").is_some() && cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg-tests=-rdynamic");
        // NEOVM_JIT_SHIM_NAMES is `include!`-ed at module scope (above) from the
        // single-source shim_names.rs — same set aot.rs salts/exports.
        for shim in NEOVM_JIT_SHIM_NAMES {
            println!("cargo:rustc-link-arg-tests=-Wl,--export-dynamic-symbol={shim}");
        }
    }
    println!(
        "cargo:rerun-if-changed={}",
        project_root.join("etc/rgb.txt").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("build_support/generated_lisp.rs")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("src/emacs_core/runtime/jit/shim_names.rs")
            .display()
    );
}

/// Run GNU's own awk over GNU's own Unicode data, exactly as
/// `admin/unidata/Makefile.in:110-123` does, for every row of the one recipe
/// table.
///
/// Cargo re-runs a build script whenever a `rerun-if-changed` path moves, so
/// every input of every recipe is declared here; the recipes are then run
/// unconditionally (all of ~50 ms for both files) and each output is written
/// only if its bytes actually changed.  That "only if" is load-bearing: an
/// identical rewrite would push the `.el`'s mtime past the `.elc` compiled
/// from it, and ledger 202's refusal would stop every in-process test in the
/// tree -- which is precisely the state the old, second generator left behind
/// on every profile switch (ledger 203 §7.4).
///
/// A missing awk is a hard error, as it is for GNU: `configure` will not
/// configure a tree without one, and `cargo xtask fresh-build` already runs
/// four awk scripts unconditionally.
///
/// Ledger 206.
fn ensure_generated_unicode_lisp(project_root: &Path) {
    let roots = generated_lisp::GeneratedLispRoots::of_project(project_root);
    for recipe in generated_lisp::AWK_GENERATED_UNICODE_LISP {
        for dependency in recipe.dependencies(&roots) {
            println!("cargo:rerun-if-changed={}", dependency.display());
        }
        match recipe.regenerate(&roots) {
            Ok(generated_lisp::Regenerated::Unchanged) => {}
            Ok(generated_lisp::Regenerated::Written) => {
                println!(
                    "cargo:warning=regenerated lisp/{} from {} (GNU {})",
                    recipe.output, recipe.script, recipe.gnu_rule,
                );
            }
            Err(err) => panic!("{err}"),
        }
    }
}

/// Whether this build has a native inline web view.
///
/// The backend is `crates/neomacs-webview/src/platform/macos`, compiled only
/// under `neomacs-webview`'s `webview` feature, which `neomacs` forwards to
/// this crate's `webview` feature.  There is no library to look for --
/// `WebKit.framework` ships with macOS -- so the probe is "the feature is on
/// and the target is macOS".  Either half alone is wrong: the feature on
/// Linux selects the WPE path, whose `xwidget-internal` is still NotBuilt,
/// and macOS without the feature has no backend behind the symbol.
fn detect_wkwebview() {
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_wkwebview)");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_WEBVIEW");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_WEBVIEW").is_some()
    {
        println!("cargo:rustc-cfg=neomacs_have_wkwebview");
    }
}

fn detect_lcms2() {
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_lcms2)");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=LCMS2_NO_PKG_CONFIG");

    if std::env::var_os("LCMS2_NO_PKG_CONFIG").is_some() {
        return;
    }

    let Ok(library) = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("lcms2")
    else {
        return;
    };

    println!("cargo:rustc-cfg=neomacs_have_lcms2");
    let candidates = lcms2_library_candidates(&library.link_paths);
    if !candidates.is_empty() {
        println!("cargo:rustc-env=NEOMACS_LCMS2_LIBRARY_CANDIDATES={candidates}");
    }
}

fn lcms2_library_candidates(paths: &[PathBuf]) -> String {
    let mut candidates = Vec::new();
    let names: &[&str] = std::cfg_select! {
        target_os = "windows" => &["liblcms2-2.dll", "lcms2.dll"],
        target_os = "macos" => &["liblcms2.2.dylib", "liblcms2.dylib"],
        target_os = "linux" => &["liblcms2.so.2", "liblcms2.so"],
        unix => &["liblcms2.so.2", "liblcms2.so"],
        _ => &["lcms2"],
    };

    for path in paths {
        for name in names {
            let candidate = path.join(name);
            if candidate.exists() {
                candidates.push(candidate.display().to_string());
            }
        }
    }

    candidates.join(":")
}

/// Parse etc/rgb.txt and generate a Rust source file with a static
/// color lookup function. This gives us the full X11 color database
/// (788 colors including grey0-grey100, DarkGoldenrod, etc.) with
/// zero runtime file I/O — the table is compiled into the binary.
fn generate_x11_color_table(project_root: &Path, _manifest_dir: &Path) {
    let rgb_path = project_root.join("etc/rgb.txt");
    println!("cargo:rerun-if-changed={}", rgb_path.display());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let out_path = out_dir.join("x11_colors.rs");

    let content = fs::read_to_string(&rgb_path).unwrap_or_else(|e| {
        eprintln!("cargo:warning=Cannot read {}: {}", rgb_path.display(), e);
        String::new()
    });

    // Parse rgb.txt: "R G B\t\tColorName"
    // Collect unique (lowercase_name -> (r, g, b)), also add no-space variants.
    let mut colors: std::collections::BTreeMap<String, (u8, u8, u8)> =
        std::collections::BTreeMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let r = parts.next().and_then(|s| s.parse::<u8>().ok());
        let g = parts.next().and_then(|s| s.parse::<u8>().ok());
        let b = parts.next().and_then(|s| s.parse::<u8>().ok());
        // Remaining is the color name (may contain spaces)
        let name: String = parts.collect::<Vec<_>>().join(" ");

        if let (Some(r), Some(g), Some(b)) = (r, g, b)
            && !name.is_empty()
        {
            let lower = name.to_lowercase();
            let no_spaces = lower.replace(' ', "");
            colors.entry(lower.clone()).or_insert((r, g, b));
            if no_spaces != lower {
                colors.entry(no_spaces).or_insert((r, g, b));
            }
        }
    }

    // Generate Rust source: a function with a match statement.
    let mut code = String::new();
    code.push_str("/// Auto-generated from etc/rgb.txt — do not edit.\n");
    code.push_str("/// X11 color name lookup (case-insensitive).\n");
    code.push_str("pub fn x11_color_lookup(name: &str) -> Option<(u8, u8, u8)> {\n");
    code.push_str("    match name.to_lowercase().as_str() {\n");
    for (name, (r, g, b)) in &colors {
        code.push_str(&format!(
            "        {:?} => Some(({}, {}, {})),\n",
            name, r, g, b
        ));
    }
    code.push_str("        _ => None,\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    fs::write(&out_path, &code).expect("Failed to write x11_colors.rs");
    eprintln!(
        "cargo:warning=Generated X11 color table: {} entries from {}",
        colors.len(),
        rgb_path.display()
    );
}
