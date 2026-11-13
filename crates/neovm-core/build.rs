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

// Single source of truth for the host capability cfgs (dbus, lcms2,
// wkwebview), shared with neovm-system, neovm-value and neovm-eval so every
// crate's view of one host agrees. See build_support/host_cfg.rs.
#[path = "../../build_support/host_cfg.rs"]
mod host_cfg;

// Single source of truth (R2-C2): the `neovm_jit_*` shim names, shared with
// runtime/jit/aot.rs (MIR_SHIM_NAMES) + crates/neomacs/build.rs via `include!` so the
// emit/salt set and both export sets can never drift.  The relative path below
// is the ONE spelling of that source file: the same macro feeds `include!` and
// the `rerun-if-changed` watcher, so the two can never disagree again (a watch
// path that names nothing makes cargo treat the file as changed on every
// invocation and rebuild this crate forever).
macro_rules! shim_names_rel {
    () => {
        "../neovm-eval/src/emacs_core/runtime/jit/shim_names.rs"
    };
}
include!(shim_names_rel!());

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

    host_cfg::configure_all();
    ensure_generated_unicode_lisp(&project_root);
    ensure_generated_charset_lisp(&project_root);

    // R1c call-bearing AOT: export the host's `neovm_jit_*` shims
    // (`#[unsafe(no_mangle)] pub`, anchored by `JIT_SHIM_ANCHOR`) into the TEST
    // binaries' DYNAMIC symbol table, where a `dlopen`'d call/cons AOT `.so` binds
    // its undefined imports against them. `-rdynamic` alone is insufficient under
    // the workspace linker (it doesn't promote these otherwise-unreferenced fns
    // to the dynamic table), so we additionally name each shim with
    // `--export-dynamic-symbol`. Gated on the `jit` feature (the only config that
    // emits AOT).
    //
    // Both carriers are emitted: `rustc-link-arg` (every linkable target of this
    // package — for a library crate that is exactly its test/bench binaries, the
    // lib itself never links, and production binaries live in other packages and
    // carry their own R2 export) AND `rustc-link-arg-tests`. Measured with the
    // P2 test-target combo on cargo 1.96.1 + `wild`: the `-tests` lines reach the
    // build-script output but do NOT reach the lib-test link command, so a
    // `dlopen`'d AOT `.so` fails with "undefined symbol: neovm_jit_backedge";
    // `rustc-link-arg` does reach it (83 `neovm_jit_*` dynamic symbols after the
    // fix, 0 before, all present in the static table).
    //
    // R2 CARRY-FORWARD (R2-B5): the PRODUCTION binary (neomacs-bin) that loads the
    // dump-time preload `.so` MUST replicate BOTH the `#[unsafe(no_mangle)] pub`
    // shims AND this per-shim `--export-dynamic-symbol` export in ITS build.rs —
    // under the `wild` linker, plain `-rdynamic` does NOT promote these
    // address-only-referenced fns to the dynamic table, so the preload `.so`'s
    // `neovm_jit_*` imports would otherwise fail to resolve at dlopen and abort on
    // first shim call. Do not assume `-rdynamic` alone suffices.
    if std::env::var_os("CARGO_FEATURE_JIT").is_some() {
        #[cfg(target_os = "linux")]
        {
            println!("cargo:rustc-link-arg=-rdynamic");
            println!("cargo:rustc-link-arg-tests=-rdynamic");
            // NEOVM_JIT_SHIM_NAMES is `include!`-ed at module scope (above) from the
            // single-source shim_names.rs — same set aot.rs salts/exports.
            for shim in NEOVM_JIT_SHIM_NAMES {
                println!("cargo:rustc-link-arg=-Wl,--export-dynamic-symbol={shim}");
                println!("cargo:rustc-link-arg-tests=-Wl,--export-dynamic-symbol={shim}");
            }
        }
        // Mach-O has no `--export-dynamic-symbol`.  ld64's `-export_dynamic`
        // (`-rdynamic` in Apple clang) puts every global — the
        // `#[unsafe(no_mangle)]` shims included — into the test binary's
        // dynamic symbol table, where a `-bundle -undefined dynamic_lookup`
        // AOT module resolves them at dlopen.
        #[cfg(target_os = "macos")]
        println!("cargo:rustc-link-arg-tests=-rdynamic");
    }
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir
            .join("build_support/generated_lisp.rs")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join(shim_names_rel!()).display()
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
        watch_generated_output(&recipe.output_path(&roots));
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

/// Watch a generated file so its *absence* reruns this build script.
///
/// Cargo decides whether to rerun a build script from the files it is told to
/// watch, and the recipe inputs say nothing about whether the output still
/// exists: delete `lisp/international/cp51932.el` and, with the awk script and
/// map unchanged, nothing reruns -- the tree stays unbootable until some
/// unrelated edit happens to touch this script.  GNU's `make` does not have
/// that hole (its target *is* the file), and watching the output closes it:
/// cargo treats a missing watched file as changed, so the next build
/// regenerates it.  Verified by deleting both charset outputs and rebuilding.
fn watch_generated_output(output: &Path) {
    println!("cargo:rerun-if-changed={}", output.display());
}

/// Generate GNU's charset translation Lisp (`lisp/international/cp51932.el`
/// and `eucjp-ms.el`) from the same recipe table `cargo xtask fresh-build`
/// iterates.
///
/// These two files are load-bearing for anything that boots: `lisp/loadup.el`
/// loads them with no noerror flag (loadup.el:231-232), so a checkout whose
/// build never produced them cannot dump a runtime image -- which is exactly
/// the tree a fresh `cargo nextest run` has.  GNU's `make` generates them as
/// part of building, so this build does too.
fn ensure_generated_charset_lisp(project_root: &Path) {
    let roots = generated_lisp::GeneratedLispRoots::of_project(project_root);
    for recipe in generated_lisp::AWK_GENERATED_CHARSET_LISP {
        for dependency in recipe.dependencies(&roots) {
            println!("cargo:rerun-if-changed={}", dependency.display());
        }
        watch_generated_output(&recipe.output_path(&roots));
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
