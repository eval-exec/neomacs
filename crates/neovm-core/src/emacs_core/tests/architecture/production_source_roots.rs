//! Shared production-source roots for the architecture scanners (peer item
//! #59, RULE 19).
//!
//! During the split every scanner kept its pre-split
//! `env!("CARGO_MANIFEST_DIR")/src` root untouched (RULE 19). Production code
//! now lives in `neovm-core` plus the extracted lower crates, so a core-only
//! root silently stops scanning the moved trees: the guard keeps passing while
//! the code it names is no longer read. Every architecture scanner routes its
//! source walk through the roots below instead.
//!
//! A moved tree keeps its layout below its crate's `src/`, so the pre-split
//! spelling `src/<path>` still names the same module: the helpers expose both
//! that legacy spelling and the cross-crate `<crate>/src/<path>` one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::architecture_moving_set::MOVING_CRATES;

/// The value extraction manifest: pre-split core-relative path -> destination
/// below `crates/neovm-value/src`. G0 members left `emacs_core/runtime/...` for
/// the value crate root, so their historical spellings cannot be recovered by
/// searching the roots alone.
const VALUE_MOVE_MANIFEST: &str = include_str!("moved_production_files.tsv");

/// Files that moved again after the manifest that recorded their extraction
/// (later refactors inside the owning crate). Each row is
/// `(legacy core path, owning crate, crate-src-relative path)`.
const POST_MANIFEST_ALIASES: &[(&str, &str, &str)] = &[(
    "emacs_core/system/profiler/mod.rs",
    "neovm-eval",
    "emacs_core/runtime/eval/profiler.rs",
)];

fn value_move_map() -> &'static BTreeMap<&'static str, &'static str> {
    static MAP: OnceLock<BTreeMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = BTreeMap::new();
        for line in VALUE_MOVE_MANIFEST.lines() {
            if line.is_empty() || line.starts_with('#') || line.starts_with("old_rel") {
                continue;
            }
            if let Some((old, destination)) = line.split_once('\t') {
                map.insert(old, destination);
            }
        }
        map
    })
}

/// One production source tree: the core crate or an extracted lower crate.
pub(crate) struct ProductionSourceRoot {
    /// The crate directory under `<workspace>/crates`.
    pub(crate) crate_dir: String,
    /// `<workspace>/crates/<crate_dir>/src`.
    pub(crate) src: PathBuf,
}

impl ProductionSourceRoot {
    /// The file at `relative`, which is slash-separated and relative to this
    /// crate's `src/`.
    pub(crate) fn file(&self, relative: &str) -> PathBuf {
        self.src.join(relative)
    }
}

/// One `*.rs` file in a production source tree.
pub(crate) struct ProductionSourceFile {
    /// The crate directory that owns the file.
    pub(crate) crate_dir: String,
    /// The absolute path on disk.
    pub(crate) path: PathBuf,
    /// Path relative to the owning crate's `src/`, with `/` separators.
    pub(crate) relative: String,
}

impl ProductionSourceFile {
    /// The pre-split core-relative spelling, `src/<relative>`. The moved trees
    /// kept their layout below `src/`, so allow-site tables written against
    /// `crates/neovm-core/src` keep their meaning.
    pub(crate) fn legacy(&self) -> String {
        format!("src/{}", self.relative)
    }

    /// The cross-crate spelling, `<crate>/src/<relative>`.
    pub(crate) fn workspace_relative(&self) -> String {
        format!("{}/src/{}", self.crate_dir, self.relative)
    }

    /// Whether the file lives below a `tests/` directory or is a `test_*`
    /// module. Scanners that cover only production code share this rule.
    pub(crate) fn is_test_path(&self) -> bool {
        self.relative
            .split('/')
            .any(|component| component == "tests")
            || self
                .relative
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("test_"))
    }
}

/// The workspace root that holds `crates/`.
pub(crate) fn workspace_root() -> PathBuf {
    neomacs_infra::crate_root!()
        .parent()
        .and_then(Path::parent)
        .expect("crates/neovm-core is two levels below the workspace root")
        .to_path_buf()
}

/// The crate directory for a `MOVING_CRATES` entry (`neovm_eval` ->
/// `neovm-eval`).
fn crate_dir_name(name: &str) -> String {
    name.replace('_', "-")
}

/// The workspace's `crates/` directory. Scanners that intentionally cover
/// crates outside the moving set (workspace TLS, registered resets, host
/// effect owners) still derive the directory from here.
pub(crate) fn crates_dir() -> PathBuf {
    workspace_root().join("crates")
}

/// Every production source tree: `neovm-core` first, then every extracted
/// crate in `MOVING_CRATES` order.
pub(crate) fn production_source_roots() -> Vec<ProductionSourceRoot> {
    let crates = crates_dir();
    let mut roots = Vec::with_capacity(MOVING_CRATES.len() + 1);
    roots.push(ProductionSourceRoot {
        crate_dir: "neovm-core".to_owned(),
        src: crates.join("neovm-core/src"),
    });
    for name in MOVING_CRATES {
        let directory = crate_dir_name(name);
        roots.push(ProductionSourceRoot {
            src: crates.join(&directory).join("src"),
            crate_dir: directory,
        });
    }
    roots
}

/// One root by crate directory name (`neovm-core`, `neovm-eval`, ...).
pub(crate) fn production_source_root(crate_dir: &str) -> ProductionSourceRoot {
    production_source_roots()
        .into_iter()
        .find(|root| root.crate_dir == crate_dir)
        .unwrap_or_else(|| panic!("{crate_dir} is not a production source root"))
}

fn collect(files: &mut Vec<ProductionSourceFile>, root: &ProductionSourceRoot, directory: &Path) {
    for entry in std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
    {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            collect(files, root, &path);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path
                .strip_prefix(&root.src)
                .unwrap_or_else(|_| panic!("{} lives under {}", path.display(), root.src.display()))
                .to_string_lossy()
                .replace('\\', "/");
            files.push(ProductionSourceFile {
                crate_dir: root.crate_dir.clone(),
                path,
                relative,
            });
        }
    }
}

/// Every `*.rs` file in every production source tree, sorted by
/// [`ProductionSourceFile::workspace_relative`].
pub(crate) fn production_source_files() -> Vec<ProductionSourceFile> {
    let mut files = Vec::new();
    for root in production_source_roots() {
        assert!(
            root.src.is_dir(),
            "production source root {} is missing",
            root.src.display()
        );
        collect(&mut files, &root, &root.src);
    }
    files.sort_by_key(ProductionSourceFile::workspace_relative);
    files
}

/// Resolve the production file that now owns `path`.
///
/// Accepts the pre-split `src/<relative>` spelling, a crate-relative
/// `<relative>`, or the cross-crate `<crate>/src/<relative>` one. Historical
/// spellings whose extraction rewrote the path (the value manifest and later
/// in-crate moves) are resolved first. Otherwise the file must exist in exactly
/// one root: a scanner table that names a deleted or duplicated file must fail
/// loudly instead of scanning nothing.
pub(crate) fn production_file(path: &str) -> PathBuf {
    let path = path.replace('\\', "/");
    let roots = production_source_roots();
    let (wanted_crate, relative) = if let Some(rest) = path.strip_prefix("src/") {
        (None, rest.to_owned())
    } else if let Some((first, rest)) = path.split_once("/src/") {
        if roots.iter().any(|root| root.crate_dir == first) {
            (Some(first.to_owned()), rest.to_owned())
        } else {
            (None, path.clone())
        }
    } else {
        (None, path.clone())
    };
    if wanted_crate.is_none() {
        if let Some(destination) = value_move_map().get(relative.as_str()) {
            let candidate = production_source_root("neovm-value").file(destination);
            if candidate.is_file() {
                return candidate;
            }
        }
        if let Some((_, crate_dir, moved)) = POST_MANIFEST_ALIASES
            .iter()
            .find(|(legacy, _, _)| relative.as_str() == *legacy)
        {
            let candidate = production_source_root(crate_dir).file(moved);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    let found: Vec<PathBuf> = roots
        .iter()
        .filter(|root| {
            wanted_crate
                .as_deref()
                .is_none_or(|name| root.crate_dir.as_str() == name)
        })
        .map(|root| root.file(&relative))
        .filter(|candidate| candidate.is_file())
        .collect();
    let searched = roots
        .iter()
        .map(|root| root.src.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    match found.as_slice() {
        [file] => file.clone(),
        [] => panic!("no production source owns {path}; searched {searched}"),
        _ => panic!("ambiguous production source {path}: {found:?}"),
    }
}

/// The production file at `relative` below `crate_dir`'s `src/`.
pub(crate) fn production_file_in(crate_dir: &str, relative: &str) -> PathBuf {
    let file = production_source_root(crate_dir).file(relative);
    assert!(
        file.is_file(),
        "{crate_dir}/src/{relative} is not a production file ({})",
        file.display()
    );
    file
}

/// The production directory at `relative` below `crate_dir`'s `src/`.
///
/// Subtree scanners (the parallel-exec substrate, value's `tagged` module)
/// name a directory whose files they walk. Requiring `is_dir` keeps the same
/// fail-closed contract as [`production_file_in`]: a renamed or deleted
/// directory fails loudly instead of scanning nothing.
pub(crate) fn production_dir_in(crate_dir: &str, relative: &str) -> PathBuf {
    let dir = production_source_root(crate_dir).src.join(relative);
    assert!(
        dir.is_dir(),
        "{crate_dir}/src/{relative} is not a production directory ({})",
        dir.display()
    );
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_crate_contributes_production_sources() {
        let files = production_source_files();
        for root in production_source_roots() {
            let count = files
                .iter()
                .filter(|file| file.crate_dir == root.crate_dir)
                .count();
            assert!(
                count > 0,
                "{} contributes no production sources below {}",
                root.crate_dir,
                root.src.display()
            );
        }
        assert!(
            files.len() > 500,
            "the shared root set found only {} source files",
            files.len()
        );
    }

    #[test]
    fn legacy_paths_resolve_to_the_owning_crate() {
        // neovm-value owns the moved GC; neovm-eval owns the evaluator tree.
        assert_eq!(
            production_file("src/tagged/gc/incremental.rs"),
            production_file_in("neovm-value", "tagged/gc/incremental.rs")
        );
        assert_eq!(
            production_file("src/emacs_core/runtime/eval/green/carried.rs"),
            production_file_in("neovm-eval", "emacs_core/runtime/eval/green/carried.rs")
        );
        assert_eq!(
            production_file("neovm-edit/src/buffer/overlay_index.rs"),
            production_file_in("neovm-edit", "buffer/overlay_index.rs")
        );
    }

    #[test]
    fn historical_value_paths_follow_the_extraction_manifest() {
        // G0 members left `emacs_core/runtime/...` for the value crate root.
        assert_eq!(
            production_file("src/emacs_core/runtime/error/mod.rs"),
            production_file_in("neovm-value", "error/mod.rs")
        );
        assert_eq!(
            production_file("src/emacs_core/text/emacs_char/mod.rs"),
            production_file_in("neovm-value", "emacs_char/mod.rs")
        );
        // Later in-crate refactors are recorded as explicit aliases.
        assert_eq!(
            production_file("src/emacs_core/system/profiler/mod.rs"),
            production_file_in("neovm-eval", "emacs_core/runtime/eval/profiler.rs")
        );
    }

    #[test]
    fn legacy_and_workspace_spellings_preserve_the_moved_layout() {
        let files = production_source_files();
        let edit = files
            .iter()
            .find(|file| {
                file.crate_dir == "neovm-edit" && file.relative == "buffer/overlay_index.rs"
            })
            .expect("neovm-edit owns buffer/overlay_index.rs");
        assert_eq!(edit.legacy(), "src/buffer/overlay_index.rs");
        assert_eq!(
            edit.workspace_relative(),
            "neovm-edit/src/buffer/overlay_index.rs"
        );
        let core = files
            .iter()
            .find(|file| file.crate_dir == "neovm-core" && file.relative == "lib.rs")
            .expect("the core crate has lib.rs");
        assert_eq!(core.legacy(), "src/lib.rs");
        assert_eq!(core.workspace_relative(), "neovm-core/src/lib.rs");
    }

    /// RULE 19 meta-guard: a scanner may not build a private production root.
    /// The shared helpers above are the only allowed route to `src/`.
    #[test]
    fn architecture_scanners_use_the_shared_production_roots() {
        let directory = neomacs_infra::crate_root!().join("src/emacs_core/tests/architecture");
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&directory).expect("architecture test directory") {
            let path = entry.expect("architecture entry").path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("architecture file name");
            // The helper owns the only direct root construction; the moving-set
            // table names crates but walks no sources.
            if name == "production_source_roots.rs" || name == "moving_set.rs" {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read architecture scanner");
            let code = super::super::architecture_plist_head_seam_guard_test::code_only(&source);
            for marker in ["CARGO_MANIFEST_DIR", "crate_root!"] {
                let mut from = 0;
                while let Some(offset) = code[from..].find(marker) {
                    let start = from + offset;
                    let end = code[start..]
                        .find(';')
                        .map(|length| start + length)
                        .unwrap_or(code.len())
                        .min(start + 400);
                    let statement = &code[start..end];
                    if statement.contains("join(\"src") {
                        let line = code[..start].matches('\n').count() + 1;
                        offenders.push(format!(
                            "{name}:{line}: direct {marker} source root; \
                             route through production_source_roots()"
                        ));
                    }
                    from = start + marker.len();
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "RULE 19 scanner re-root: {} private production root(s):\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }
}
