use std::{env, io, path::PathBuf, process::Command};
use vergen_gitcl::{Cargo, Emitter, Gitcl, Rustc};

mod build_support;

use build_support::{CargoProfileName, OwnedGitWorktree, tracked_top_level_entries};

// The full `neovm_jit_*` runtime-shim set an AOT preload `.so` imports
// (`#[unsafe(no_mangle)] pub` in neovm-core). SINGLE SOURCE OF TRUTH (R2-C2):
// the list lives in `crates/neovm-core/src/emacs_core/runtime/jit/shim_names.rs` and is
// `include!`-ed here (as `NEOVM_JIT_SHIM_NAMES`) so this production export set
// can never drift from neovm-core's `MIR_SHIM_NAMES` or its lib build.rs export
// set. Still MUST match the shim DEFINITIONS in jit/compile.rs + `JIT_SHIM_ANCHOR`.
// R2-B5: the PRODUCTION `neomacs` binary exports these into its DYNAMIC symbol
// table so the dump-time `libneomacs-preload.so` (R2) binds its undefined shim
// imports at `dlopen`. Under the workspace linker `wild`, plain `-rdynamic` does
// NOT promote these address-only-referenced fns — each must be named with
// `--export-dynamic-symbol` (the R1c carry-forward; without it the preload `.so`
// aborts on first shim call).
include!("../neovm-core/src/emacs_core/runtime/jit/shim_names.rs");

/// R2-B5: export the `neovm_jit_*` shims into the `neomacs` binary's dynamic
/// symbol table (Linux + `jit` only) so the AOT preload `.so` resolves them at
/// dlopen. Targets ONLY the `neomacs` bin (`rustc-link-arg-bin=neomacs=`), not
/// `mock-display` or any test.
fn export_jit_shims_for_aot(target_os: &str) {
    if target_os != "linux" || env::var_os("CARGO_FEATURE_JIT").is_none() {
        return;
    }
    println!("cargo:rustc-link-arg-bin=neomacs=-rdynamic");
    for shim in NEOVM_JIT_SHIM_NAMES {
        println!("cargo:rustc-link-arg-bin=neomacs=-Wl,--export-dynamic-symbol={shim}");
    }
}

fn emit_build_provenance() -> Result<(), Box<dyn std::error::Error>> {
    // Only source- and toolchain-derived values belong in the binary. In
    // particular, do not emit wall-clock build time: that would make two
    // builds of the same revision differ for no semantic reason.
    let owned_worktree = discover_owned_git_worktree()?;
    let cargo = Cargo::builder().target_triple(true).build();
    let rustc = Rustc::builder().semver(true).build();
    let mut emitter = Emitter::default();
    if let Some(owned_worktree) = owned_worktree.as_ref() {
        let mut git = Gitcl::builder()
            .sha(false)
            .dirty(false)
            .commit_timestamp(true)
            .build();
        git.at_path(owned_worktree.as_path().to_path_buf());
        emitter.add_instructions(&git)?;
    }
    emitter
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .emit()?;
    if let Some(owned_worktree) = owned_worktree {
        emit_git_worktree_rerun_inputs(owned_worktree)?;
    }

    // Cargo's PROFILE variable deliberately collapses custom profiles to their
    // `dev`/`release` ancestor. The artifact directory retains custom profile
    // names, including Neomacs' release-pgo*, so recover the selected identity
    // from OUT_DIR: <profile-dir>/build/<package-hash>/out. Cargo's built-in
    // dev/test and release/bench pairs share directories; xtask provides the
    // exact selection, while a direct Cargo invocation reports the honest
    // ambiguous class rather than guessing.
    println!("cargo:rerun-if-env-changed=NEOMACS_BUILD_PROFILE");
    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Cargo did not provide OUT_DIR")
        })?);
    let explicitly_selected_profile = env::var("NEOMACS_BUILD_PROFILE").ok();
    let profile =
        CargoProfileName::from_build_inputs(&out_dir, explicitly_selected_profile.as_deref())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Cargo profile {:?} does not match artifact directory {}",
                        explicitly_selected_profile,
                        out_dir.display(),
                    ),
                )
            })?;
    println!("cargo:rustc-env=NEOMACS_BUILD_PROFILE={}", profile.as_str());
    Ok(())
}

fn git_stdout(args: &[&str], current_dir: Option<&std::path::Path>) -> io::Result<Option<Vec<u8>>> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(output.status.success().then_some(output.stdout))
}

fn discover_owned_git_worktree() -> Result<Option<OwnedGitWorktree>, Box<dyn std::error::Error>> {
    let workspace_root = PathBuf::from(env::var_os("CARGO_WORKSPACE_DIR").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "workspace .cargo/config.toml did not provide CARGO_WORKSPACE_DIR",
        )
    })?);
    let Some(discovered_root) =
        git_stdout(&["rev-parse", "--show-toplevel"], Some(&workspace_root))?
    else {
        return Ok(None);
    };
    let workspace_root = workspace_root.canonicalize()?;
    let discovered_root =
        PathBuf::from(std::str::from_utf8(&discovered_root)?.trim()).canonicalize()?;
    if workspace_root != discovered_root {
        return Ok(None);
    }
    let manifests_are_tracked = git_stdout(
        &[
            "ls-files",
            "--error-unmatch",
            "--",
            "Cargo.toml",
            "crates/neomacs/Cargo.toml",
        ],
        Some(&workspace_root),
    )?
    .is_some();
    Ok(OwnedGitWorktree::from_canonical_roots(
        workspace_root,
        &discovered_root,
        manifests_are_tracked,
    ))
}

fn emit_git_worktree_rerun_inputs(
    owned_worktree: OwnedGitWorktree,
) -> Result<(), Box<dyn std::error::Error>> {
    let worktree_root = owned_worktree.into_path_buf();
    let tracked_paths =
        git_stdout(&["ls-files", "-z"], Some(&worktree_root))?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "verified Neomacs Git worktree became unavailable",
            )
        })?;

    // Vergen watches HEAD and its ref, which keeps the SHA fresh after commits.
    // Dirty state also depends on tracked worktree contents. Watch the compact
    // set of tracked top-level entries rather than the workspace root, because
    // the latter contains target/ and would make every build invalidate itself.
    for entry in tracked_top_level_entries(&tracked_paths)? {
        println!(
            "cargo:rerun-if-changed={}",
            worktree_root.join(entry).display()
        );
    }

    // A newly tracked top-level path was not in the previous list, so its file
    // cannot wake Cargo yet. The index closes that gap; ordinary edits are
    // covered by the file/directory inputs above.
    if let Some(index_path) = git_stdout(
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        Some(&worktree_root),
    )? {
        let index_path = std::str::from_utf8(&index_path)?.trim();
        if !index_path.is_empty() {
            println!("cargo:rerun-if-changed={index_path}");
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    emit_build_provenance()?;
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    println!("cargo:rerun-if-changed=../neovm-core/src/emacs_core/runtime/jit/shim_names.rs");
    export_jit_shims_for_aot(&target_os);
    if target_os == "windows" {
        let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        match target_env.as_str() {
            "msvc" => println!("cargo:rustc-link-arg-bin=neomacs=/STACK:134217728"),
            "gnu" => println!("cargo:rustc-link-arg-bin=neomacs=-Wl,--stack,134217728"),
            _ => {}
        }
        return Ok(());
    }

    let candidates: &[&str] = match target_os.as_str() {
        "linux" => &["ncursesw", "ncurses"],
        "macos" => &["ncurses", "ncursesw"],
        _ => return Ok(()),
    };

    for name in candidates {
        if let Ok(library) = pkg_config::Config::new().probe(name) {
            for path in library.link_paths {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path.display());
            }
            return Ok(());
        }
    }

    println!("cargo:rustc-link-lib={}", candidates[0]);
    Ok(())
}
