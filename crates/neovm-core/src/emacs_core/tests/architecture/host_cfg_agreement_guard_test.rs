//! Cross-crate host-cfg agreement and shared-probe wiring (LEAD 04:1xZ
//! guard (b); peer 04:1xZ).
//!
//! The workspace shares one build-script probe (`build_support/host_cfg.rs`)
//! and every crate whose source branches on a host capability cfg includes
//! it. This guard has two halves:
//!
//! 1. Runtime agreement (`host_cfg_views_agree_across_crates`): for every
//!    host cfg the sources use (`neomacs_have_dbus`, `neomacs_have_lcms2`,
//!    `neomacs_have_wkwebview`), each using crate must observe the SAME value
//!    in one build. The observers are independent public surfaces: core is
//!    this test's own `cfg!`, eval is `HostCfgClass::is_active`, value is its
//!    GNU c-features table (the names it would `Fprovide`). System's dbus
//!    view is not read here: its dbusbind bootstrap binder needs a live
//!    tagged heap, so it is covered by the static probe-wiring half below
//!    (system mentions the cfg, so its build script must run the shared
//!    probe) and by the H3b binder/snapshot test. Before the shared probe,
//!    core emitted `neomacs_have_dbus` while eval had no build script at all,
//!    so core said true and eval said false; this test fails on that tree.
//!
//! 2. Static probe wiring (`host_cfg_mentions_require_the_shared_probe`):
//!    any workspace crate whose `src` mentions `neomacs_have_<x>` must have a
//!    build script that includes `build_support/host_cfg.rs` and calls
//!    `host_cfg::configure_all()`, and the shared probe must emit a
//!    check-cfg for `<x>` (a name the probe does not declare cannot come from
//!    the shared probe). `include!`d files count as part of the INCLUDING
//!    crate's source: the spliced tokens are compiled in that crate, so a
//!    `#[cfg(neomacs_have_<x>)]` in an included file is evaluated there even
//!    when the file lives in another crate's `src` (display includes eval's
//!    upper snapshot this way). The mutation test covers mention-without-call,
//!    mention-without-build-script, an unknown name, a missing probe, the
//!    cross-crate include form, and the green forms.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::architecture_production_source_roots::workspace_root;

const SHARED_PROBE: &str = "build_support/host_cfg.rs";
const PROBE_CALL: &str = "host_cfg::configure_all()";
const CFG_PREFIX: &str = "neomacs_have_";

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn walk_rs_files(directory: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !directory.is_dir() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk_rs_files(&path, out)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Every `neomacs_have_<name>` identifier mentioned in `text`.
fn mention_names(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(index) = rest.find(CFG_PREFIX) {
        let after = &rest[index + CFG_PREFIX.len()..];
        let suffix: String = after
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .collect();
        if !suffix.is_empty() {
            names.insert(format!("{CFG_PREFIX}{suffix}"));
        }
        rest = &after[suffix.len()..];
    }
    names
}

/// The cfg names the shared probe declares with `rustc-check-cfg`/`rustc-cfg`.
fn probe_cfg_names(probe_text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in probe_text.lines() {
        if line.contains("rustc-check-cfg") || line.contains("rustc-cfg=") {
            names.extend(mention_names(line));
        }
    }
    names
}

/// The argument text of the `include!` invocation whose opening paren was
/// already consumed, plus the byte count through its matching `)`.
fn include_argument(after: &str) -> Option<(&str, usize)> {
    let mut depth = 1usize;
    for (index, character) in after.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&after[..index], index + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// String literals in `text`, in order. The workspace's `include!` paths are
/// plain ASCII with no escapes.
fn string_literals(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        out.push(&after[..end]);
        rest = &after[end + 1..];
    }
    out
}

/// Files an `include!` splices into `file`, for the two literal forms the
/// workspace's sources use:
///
/// * `include!("relative.rs")` — relative to the including file's directory;
/// * `include!(concat!(env!("CARGO_MANIFEST_DIR"), "/relative.rs"))` —
///   relative to the crate root.
///
/// Other forms (macro-computed or `OUT_DIR` paths) are ignored: they name
/// files the source tree does not contain. Resolution is textual, so an
/// `include!` spelled inside a string or a comment is treated the same; a
/// path that does not exist is filtered out by the caller.
fn included_files(crate_dir: &Path, file: &Path, text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find("include!(") {
        let after = &rest[index + "include!(".len()..];
        let Some((argument, consumed)) = include_argument(after) else {
            break;
        };
        rest = &after[consumed..];
        let argument = argument.trim();
        let resolved = if let Some(literal) = argument.strip_prefix('"') {
            let Some(end) = literal.find('"') else {
                continue;
            };
            file.parent().map(|parent| parent.join(&literal[..end]))
        } else if argument.contains("env!(\"CARGO_MANIFEST_DIR\")") {
            string_literals(argument)
                .pop()
                .map(|literal| crate_dir.join(literal.trim_start_matches('/')))
        } else {
            None
        };
        if let Some(path) = resolved {
            out.push(path);
        }
    }
    out
}

fn workspace_members(workspace: &Path) -> Result<Vec<String>, String> {
    let manifest = read(&workspace.join("Cargo.toml"))?;
    let manifest: toml::Value =
        toml::from_str(&manifest).map_err(|error| format!("Cargo.toml: {error}"))?;
    let members = manifest
        .as_table()
        .and_then(|root| root.get("workspace"))
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "workspace Cargo.toml has no [workspace] members".to_string())?;
    let mut out = Vec::new();
    for member in members {
        let relative = member
            .as_str()
            .ok_or_else(|| "a workspace member is not a string".to_string())?;
        out.push(relative.to_owned());
    }
    Ok(out)
}

/// Part (2): every crate that mentions a host cfg must run the shared probe,
/// and the probe must declare that cfg.
fn host_cfg_probe_violation(workspace: &Path) -> Result<(), String> {
    let probe_text = std::fs::read_to_string(workspace.join(SHARED_PROBE)).map_err(|error| {
        format!("{SHARED_PROBE}: {error} (the shared host-cfg probe is missing)")
    })?;
    let probe_names = probe_cfg_names(&probe_text);
    if probe_names.is_empty() {
        return Err(format!(
            "{SHARED_PROBE} declares no `{CFG_PREFIX}*` check-cfg names"
        ));
    }
    let mut violations = Vec::new();
    for member in workspace_members(workspace)? {
        let directory = workspace.join(&member);
        let mut files = Vec::new();
        walk_rs_files(&directory.join("src"), &mut files)?;
        let mut mentioned = BTreeSet::new();
        // An `include!` splices the target's tokens into THIS crate, so the
        // target's cfg mentions are evaluated here and belong to this crate's
        // requirement — even when the target sits in another crate's `src`.
        // Follow them transitively; `seen` bounds a self-including file.
        let mut pending = files;
        let mut seen = BTreeSet::new();
        while let Some(file) = pending.pop() {
            if !seen.insert(file.clone()) {
                continue;
            }
            let text = read(&file)?;
            mentioned.extend(mention_names(&text));
            pending.extend(
                included_files(&directory, &file, &text)
                    .into_iter()
                    .filter(|path| path.is_file()),
            );
        }
        if mentioned.is_empty() {
            continue;
        }
        let build_rs_path = directory.join("build.rs");
        let build_rs = std::fs::read_to_string(&build_rs_path).ok();
        for name in &mentioned {
            let label = format!("{member}: source mentions `{name}`");
            if !probe_names.contains(name) {
                violations.push(format!(
                    "{label} but the shared probe emits no such cfg (probe names: {probe_names:?})"
                ));
                continue;
            }
            let Some(build_rs) = build_rs.as_deref() else {
                violations.push(format!("{label} but the crate has no build.rs"));
                continue;
            };
            if !build_rs.contains(SHARED_PROBE) || !build_rs.contains(PROBE_CALL) {
                violations.push(format!(
                    "{label} but its build.rs does not include and run the shared probe ({SHARED_PROBE} + {PROBE_CALL})"
                ));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        violations.sort();
        Err(format!(
            "host-cfg mentions without the shared build-script probe:\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Part (1): the four independent views of each shared host cfg must agree.
#[test]
fn host_cfg_views_agree_across_crates() {
    use neovm_eval::host_cfg::HostCfgClass;

    let value_features: BTreeSet<&'static str> =
        neovm_value::system_vocab::c_features::initial_feature_names()
            .into_iter()
            .collect();
    let value_provides = |feature: &str| value_features.contains(feature);

    // dbus: core, eval and value branch on it; system's view is the
    // probe-wiring half (below) plus the H3b binder test.
    let core_dbus = cfg!(neomacs_have_dbus);
    let eval_dbus = HostCfgClass::Dbus.is_active();
    let value_dbus = value_provides("dbusbind");
    assert_eq!(
        core_dbus, eval_dbus,
        "neomacs_have_dbus: core says {core_dbus}, eval says {eval_dbus}"
    );
    assert_eq!(
        core_dbus, value_dbus,
        "neomacs_have_dbus: core says {core_dbus}, value provides dbusbind = {value_dbus}"
    );

    // lcms2: core, eval and value branch on it (system has no lcms2 branch).
    let core_lcms2 = cfg!(neomacs_have_lcms2);
    let eval_lcms2 = HostCfgClass::Lcms2.is_active();
    let value_lcms2 = value_provides("lcms2");
    assert_eq!(
        core_lcms2, eval_lcms2,
        "neomacs_have_lcms2: core says {core_lcms2}, eval says {eval_lcms2}"
    );
    assert_eq!(
        core_lcms2, value_lcms2,
        "neomacs_have_lcms2: core says {core_lcms2}, value provides lcms2 = {value_lcms2}"
    );

    // wkwebview: core and value branch on it (eval has no such branch).
    let core_wkwebview = cfg!(neomacs_have_wkwebview);
    let value_wkwebview = value_provides("xwidget-internal");
    assert_eq!(
        core_wkwebview, value_wkwebview,
        "neomacs_have_wkwebview: core says {core_wkwebview}, value provides xwidget-internal = {value_wkwebview}"
    );
}

/// Part (2) on the real workspace.
#[test]
fn host_cfg_mentions_require_the_shared_probe() {
    host_cfg_probe_violation(&workspace_root())
        .unwrap_or_else(|error| panic!("HOST-CFG PROBE: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE_FIXTURE: &str = r#"
pub fn configure_all() {
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_dbus)");
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_lcms2)");
    println!("cargo:rustc-check-cfg=cfg(neomacs_have_wkwebview)");
}
"#;

    fn write_workspace(root: &Path) {
        std::fs::create_dir_all(root.join("build_support")).expect("build_support");
        std::fs::write(root.join(SHARED_PROBE), PROBE_FIXTURE).expect("probe fixture");
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        )
        .expect("root manifest");
    }

    fn probe_build_rs(include: bool, call: bool) -> String {
        let mut text = String::new();
        if include {
            text.push_str("#[path = \"../../build_support/host_cfg.rs\"]\nmod host_cfg;\n");
        }
        text.push_str("fn main() {\n");
        if call {
            text.push_str("    host_cfg::configure_all();\n");
        }
        text.push_str("}\n");
        text
    }

    fn write_member(root: &Path, name: &str, source: &str, build_rs: Option<&str>) {
        let directory = root.join(name);
        std::fs::create_dir_all(directory.join("src")).expect("src");
        std::fs::write(
            directory.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n"),
        )
        .expect("member manifest");
        std::fs::write(directory.join("src/lib.rs"), source).expect("source");
        let build_path = directory.join("build.rs");
        match build_rs {
            Some(text) => std::fs::write(build_path, text).expect("build script"),
            None => {
                let _ = std::fs::remove_file(build_path);
            }
        }
    }

    #[test]
    fn host_cfg_probe_mutations_are_literal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_workspace(root);

        // GREEN: a mention with the shared probe included and called.
        write_member(
            root,
            "a",
            "pub fn f() -> bool { cfg!(neomacs_have_dbus) }\n",
            Some(&probe_build_rs(true, true)),
        );
        write_member(root, "b", "pub fn g() {}\n", None);
        host_cfg_probe_violation(root).expect("well-wired mention");

        // RED (the required mutation): a mention without the probe call.
        write_member(
            root,
            "a",
            "pub fn f() -> bool { cfg!(neomacs_have_dbus) }\n",
            Some(&probe_build_rs(true, false)),
        );
        let error = host_cfg_probe_violation(root).expect_err("mention without the call");
        assert!(error.contains("neomacs_have_dbus"), "{error}");
        assert!(error.contains("build.rs"), "{error}");

        // RED: a mention with no build script at all.
        write_member(
            root,
            "a",
            "pub fn f() -> bool { cfg!(neomacs_have_dbus) }\n",
            None,
        );
        let error = host_cfg_probe_violation(root).expect_err("mention without build.rs");
        assert!(error.contains("no build.rs"), "{error}");

        // RED: a mention of a name the shared probe does not emit. The
        // fixture name is assembled from CFG_PREFIX so this guard's own
        // source does not contain a literal cfg the real-tree scan would
        // (correctly) reject.
        let unknown_source = format!("pub fn f() -> bool {{ cfg!({CFG_PREFIX}unknown) }}\n");
        write_member(
            root,
            "a",
            &unknown_source,
            Some(&probe_build_rs(true, true)),
        );
        let error = host_cfg_probe_violation(root).expect_err("unknown cfg name");
        assert!(error.contains("emits no such cfg"), "{error}");

        // RED: the shared probe itself is missing.
        std::fs::remove_file(root.join(SHARED_PROBE)).expect("remove probe");
        assert!(host_cfg_probe_violation(root).is_err());
    }

    /// `include!` makes its target part of the INCLUDING crate's source: the
    /// spliced `#[cfg(neomacs_have_<x>)]` tokens are evaluated by the
    /// including crate, so that crate must run the shared probe. The pre-fix
    /// scan read only files under the member's own `src` and was silently
    /// green on this fixture (the display-includes-eval-snapshot case).
    #[test]
    fn host_cfg_probe_follows_cross_crate_include_directives() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_workspace(root);
        // The included source lives in `a`'s src, outside `b`'s src tree;
        // `a` itself is probe-wired, so only `b`'s requirement is at issue.
        std::fs::create_dir_all(root.join("a/src")).expect("a src");
        std::fs::write(
            root.join("a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .expect("a manifest");
        std::fs::write(root.join("a/src/lib.rs"), "pub fn a() {}\n").expect("a lib");
        std::fs::write(
            root.join("a/src/snapshot.rs"),
            "#[cfg(neomacs_have_dbus)]\npub fn row() {}\n",
        )
        .expect("a snapshot");
        std::fs::write(root.join("a/build.rs"), probe_build_rs(true, true)).expect("a build");

        // RED: `b` includes the cross-crate snapshot but runs no probe.
        write_member(
            root,
            "b",
            "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../a/src/snapshot.rs\"));\n",
            None,
        );
        let error =
            host_cfg_probe_violation(root).expect_err("cross-crate include without the probe");
        assert!(
            error.contains("b: source mentions `neomacs_have_dbus`"),
            "{error}"
        );
        assert!(error.contains("no build.rs"), "{error}");

        // GREEN: the same include with `b`'s own build script wired.
        write_member(
            root,
            "b",
            "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../a/src/snapshot.rs\"));\n",
            Some(&probe_build_rs(true, true)),
        );
        host_cfg_probe_violation(root).expect("cross-crate include with the probe");

        // RED: the plain relative form follows the INCLUDING file's dir.
        std::fs::create_dir_all(root.join("shared")).expect("shared");
        std::fs::write(
            root.join("shared/snapshot.rs"),
            "#[cfg(neomacs_have_dbus)]\npub fn shared_row() {}\n",
        )
        .expect("shared snapshot");
        write_member(root, "b", "include!(\"../../shared/snapshot.rs\");\n", None);
        let error = host_cfg_probe_violation(root).expect_err("relative include without the probe");
        assert!(
            error.contains("b: source mentions `neomacs_have_dbus`"),
            "{error}"
        );
        assert!(error.contains("no build.rs"), "{error}");

        // GREEN: the same relative include, now probe-wired.
        write_member(
            root,
            "b",
            "include!(\"../../shared/snapshot.rs\");\n",
            Some(&probe_build_rs(true, true)),
        );
        host_cfg_probe_violation(root).expect("relative include with the probe");
    }
}
