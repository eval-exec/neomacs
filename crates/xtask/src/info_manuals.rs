//! GNU-shaped Info production shared by fresh-build and release packaging.
//! The inventory follows GNU Emacs 31.1 doc/misc/Makefile.in; Org sources are
//! exported by the newly built editor before makeinfo compiles the manuals.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;

const MANIFEST: &str = ".neomacs-info-manifest";
// INFO_COMMON plus efaq-w32: GNU builds all targets for release distributions.
const MISC: &str = "auth autotype bovine calc ccmode cl dbus dired-x ebrowse ede ediff edt \
    efaq eglot eieio emacs-gnutls emacs-mime epa erc ert eshell eudc eww flymake forms \
    gnus htmlfontify idlwave ido info mairix-el message mh-e modus-themes newsticker \
    nxml-mode octave-mode org pcl-cvs pgg rcirc reftex remember sasl sc semantic ses \
    sieve smtpmail speedbar srecode todo-mode tramp transient url use-package \
    vhdl-mode viper vtable widget wisent woman efaq-w32";

struct Manual {
    source: PathBuf,
    name: String,
    assets: Vec<PathBuf>,
}

/// Build just the manuals using an existing runnable editor.
pub(crate) fn run(repo: &Path, args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let mut args = args.into_iter();
    let mut binary = None;
    let mut runtime = repo.to_owned();
    let mut dry_run = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--binary") => {
                binary = Some(crate::resolve_cli_path(
                    repo,
                    args.next().ok_or("--binary requires a path")?,
                ));
            }
            Some("--runtime-root") => {
                runtime = crate::resolve_cli_path(
                    repo,
                    args.next().ok_or("--runtime-root requires a path")?,
                );
            }
            Some("--dry-run") => dry_run = true,
            _ => {
                return Err(
                    format!("unknown build-info argument: {}", arg.to_string_lossy()).into(),
                );
            }
        }
    }
    build(
        repo,
        &runtime,
        &binary.ok_or("build-info requires --binary PATH")?,
        dry_run,
    )
}

/// Follow GNU's entry-point names and the Elisp manual's auxiliary image files.
fn inventory(docs: &Path, generated: &Path) -> Vec<Manual> {
    let mut manuals = vec![
        Manual {
            source: docs.join("emacs/emacs.texi"),
            name: "emacs".into(),
            assets: Vec::new(),
        },
        Manual {
            source: docs.join("lispref/elisp.texi"),
            name: "elisp".into(),
            assets: vec![
                docs.join("lispref/elisp_type_hierarchy.jpg"),
                docs.join("lispref/elisp_type_hierarchy.txt"),
            ],
        },
        Manual {
            source: docs.join("lispintro/emacs-lisp-intro.texi"),
            name: "eintr".into(),
            assets: Vec::new(),
        },
    ];
    manuals.extend(MISC.split_whitespace().map(|name| {
        let source = match name {
            "ccmode" => docs.join("misc/cc-mode.texi"),
            "org" | "modus-themes" => generated.join(format!("{name}.texi")),
            _ => docs.join("misc").join(format!("{name}.texi")),
        };
        Manual {
            source,
            name: name.into(),
            assets: Vec::new(),
        }
    }));
    manuals
}

/// Generate into a private staging directory and publish only a complete set.
pub(crate) fn build(repo: &Path, runtime: &Path, binary: &Path, dry_run: bool) -> Result<()> {
    let output = runtime.join("info");
    println!(
        "+ build GNU Info manuals and directory index: {}",
        output.display()
    );
    if dry_run {
        println!(
            "  export Org manuals with {}; compile {} unsplit manuals with makeinfo/install-info",
            binary.display(),
            inventory(&repo.join("docs"), Path::new("generated")).len()
        );
        return Ok(());
    }
    fs::create_dir_all(runtime)?;
    let stage = tempfile::Builder::new()
        .prefix(".neomacs-info-")
        .tempdir_in(runtime)?;
    let generated = stage.path().join("generated");
    fs::create_dir(&generated)?;
    let docs = repo.join("docs");
    let version_file = generated.join("emacs-version");
    let mut export = Command::new(binary);
    export.current_dir(docs.join("misc"))
        .env("NEOMACS_RUNTIME_ROOT", runtime)
        .env_remove("EMACSLOADPATH")
        .args(["--batch", "-Q", "-l", "ox-texinfo", "--eval",
            "(setq gc-cons-threshold 50000000 org-confirm-babel-evaluate nil org-id-track-globally nil)"])
        .arg("--eval").arg(format!("(with-temp-file {} (insert emacs-version))", crate::elisp_string_literal(&version_file)));
    for name in ["org", "modus-themes"] {
        export
            .args(["-f", "org-texinfo-export-to-texinfo-batch"])
            .arg(docs.join("misc").join(format!("{name}.org")))
            .arg(generated.join(format!("{name}.texi")));
    }
    checked(&mut export, "export Org manuals with Neomacs")?;
    let version = fs::read_to_string(version_file)?;
    if version.trim().is_empty() || version.contains(['\n', '\r', '@']) {
        return Err("editor returned an invalid Emacs manual version".into());
    }
    let template = fs::read_to_string(docs.join("emacs/emacsver.texi.in"))?;
    fs::write(
        generated.join("emacsver.texi"),
        template.replace("@version@", version.trim()),
    )?;
    let (makeinfo, install_info) = host_tools()?;
    let staged_info = stage.path().join("info");
    compile_and_publish(
        &inventory(&docs, &generated),
        &generated,
        &docs,
        &staged_info,
        &output,
        &makeinfo,
        &install_info,
    )?;
    println!(
        "  INFO  published {} manuals and info/dir",
        inventory(&docs, &generated).len()
    );
    Ok(())
}

/// Report failed tools instead of publishing partial compiler output.
fn checked(command: &mut Command, operation: &str) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .map_err(|e| format!("{operation}: could not run {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{operation} failed: {status}").into());
    }
    Ok(())
}

/// Explicit host-tool paths also support Windows' native makeinfo.bat launcher.
fn host_tools() -> Result<(OsString, OsString)> {
    let invocation_dir = std::env::current_dir()?;
    let resolve = |variable, default: &str| {
        let tool = std::env::var_os(variable).unwrap_or_else(|| default.into());
        let path = Path::new(&tool);
        // Preserve PATH lookup for command names; resolve explicit relative
        // paths before each child changes to its manual's source directory.
        if path.is_relative() && path.components().count() > 1 {
            invocation_dir.join(path).into_os_string()
        } else {
            tool
        }
    };
    Ok((
        resolve("MAKEINFO", "makeinfo"),
        resolve("INSTALL_INFO", "install-info"),
    ))
}

/// Compile and index every artifact before replacing the installed tree.
fn compile_and_publish(
    manuals: &[Manual],
    generated: &Path,
    docs: &Path,
    output: &Path,
    destination: &Path,
    makeinfo: &OsString,
    install_info: &OsString,
) -> Result<()> {
    fs::create_dir(output)?;
    let mut manifest = String::new();
    for manual in manuals {
        let file = output.join(format!("{}.info", manual.name));
        let mut command = Command::new(makeinfo);
        command
            .current_dir(
                manual
                    .source
                    .parent()
                    .ok_or("manual source has no parent")?,
            )
            .arg("--no-split")
            .arg("-I")
            .arg(generated)
            .arg("-I")
            .arg(docs.join("emacs"))
            .arg("-I")
            .arg(docs.join("misc"))
            .arg("-I")
            .arg(
                manual
                    .source
                    .parent()
                    .ok_or("manual source has no parent")?,
            );
        if manual.name == "emacs-mime" {
            command.arg("--enable-encoding");
        }
        command.arg("-o").arg(&file).arg(&manual.source);
        checked(
            &mut command,
            &format!("compile Info manual {}", manual.name),
        )?;
        if fs::metadata(&file)?.len() == 0 {
            return Err(format!("makeinfo produced an empty manual: {}", file.display()).into());
        }
        checked(
            Command::new(install_info)
                .arg("--dir-file")
                .arg(output.join("dir"))
                .arg(&file),
            &format!("index Info manual {}", manual.name),
        )?;
        for asset in &manual.assets {
            fs::copy(
                asset,
                output.join(asset.file_name().ok_or("manual asset has no basename")?),
            )?;
        }
        manifest.push_str(&format!("{}.info\n", manual.name));
    }
    if fs::metadata(output.join("dir"))?.len() == 0 {
        return Err("install-info produced an empty directory index".into());
    }
    fs::write(output.join(MANIFEST), manifest)?;
    publish(output, destination)
}

/// Replace an old generated set only after successful compilation and indexing.
fn publish(staged: &Path, output: &Path) -> Result<()> {
    let backup = tempfile::Builder::new()
        .prefix(".neomacs-info-old-")
        .tempdir_in(output.parent().ok_or("Info output has no parent")?)?;
    let old = backup.path().join("info");
    let had_old = output.try_exists()?;
    if had_old {
        fs::rename(output, &old)?;
    }
    if let Err(error) = fs::rename(staged, output) {
        if had_old && let Err(restore_error) = fs::rename(&old, output) {
            let retained = backup.keep().join("info");
            return Err(format!("could not publish Info manuals: {error}; could not restore previous set: {restore_error}; previous manuals retained at {}", retained.display()).into());
        }
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_inventory_matches_gnu_release_targets() {
        let all = inventory(Path::new("docs"), Path::new("generated"));
        assert_eq!(all.len(), 66);
        assert_eq!(
            all.iter().find(|m| m.name == "ccmode").unwrap().source,
            Path::new("docs/misc/cc-mode.texi")
        );
        assert_eq!(
            all.iter().find(|m| m.name == "info").unwrap().source,
            Path::new("docs/misc/info.texi")
        );
        assert_eq!(
            all.iter().find(|m| m.name == "semantic").unwrap().source,
            Path::new("docs/misc/semantic.texi")
        );
        assert_eq!(
            all.iter()
                .filter(|m| m.source.starts_with("generated"))
                .count(),
            2
        );
    }

    #[test]
    fn info_generation_handles_includes_version_encoding_and_index() {
        let (makeinfo, install_info) = host_tools().unwrap();
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("fixture.texi");
        fs::write(root.path().join("emacsver.texi"), "@set EMACSVER 31.1\n").unwrap();
        fs::write(
            root.path().join("nested.texi"),
            "Version @value{EMACSVER}: café\n",
        )
        .unwrap();
        fs::write(&source, "\\input texinfo\n@setfilename fixture.info\n@documentencoding UTF-8\n@dircategory Fixture\n@direntry\n* Fixture: (fixture). Test manual.\n@end direntry\n@include emacsver.texi\n@node Top\n@top Fixture\n@include nested.texi\n@bye\n").unwrap();
        let output = root.path().join("info");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("stale.info"), "old output").unwrap();
        let image = root.path().join("fixture.jpg");
        fs::write(&image, "image fixture").unwrap();
        compile_and_publish(
            &[Manual {
                source,
                name: "fixture".into(),
                assets: vec![image],
            }],
            root.path(),
            root.path(),
            &root.path().join("stage"),
            &output,
            &makeinfo,
            &install_info,
        )
        .unwrap();
        let text = fs::read_to_string(output.join("fixture.info")).unwrap();
        assert!(text.contains("Node: Top"));
        assert!(!output.join("stale.info").exists());
        assert!(text.contains("Version 31.1: café"));
        assert_eq!(
            fs::read_to_string(output.join("fixture.jpg")).unwrap(),
            "image fixture"
        );
        assert!(
            fs::read_to_string(output.join("dir"))
                .unwrap()
                .contains("Fixture: (fixture)")
        );
        assert_eq!(
            fs::read_to_string(output.join(MANIFEST)).unwrap(),
            "fixture.info\n"
        );
    }

    #[test]
    fn info_failed_compilation_does_not_replace_existing_manuals() {
        let (makeinfo, install_info) = host_tools().unwrap();
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("info");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("elisp.info"), "existing manual").unwrap();
        let source = root.path().join("bad.texi");
        fs::write(&source, "\\input texinfo\n@setfilename bad.info\n@node Top\n@top Bad\n@include nonexistent-file.texi\n@bye\n").unwrap();
        assert!(
            compile_and_publish(
                &[Manual {
                    source,
                    name: "bad".into(),
                    assets: Vec::new()
                }],
                root.path(),
                root.path(),
                &root.path().join("stage"),
                &old,
                &makeinfo,
                &install_info
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(old.join("elisp.info")).unwrap(),
            "existing manual"
        );
        assert!(!old.join("bad.info").exists());
    }

    #[test]
    fn info_failed_publish_restores_previous_directory() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("info");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("elisp.info"), "previous manual").unwrap();
        assert!(publish(&root.path().join("missing-stage"), &old).is_err());
        assert_eq!(
            fs::read_to_string(old.join("elisp.info")).unwrap(),
            "previous manual"
        );
    }
}
