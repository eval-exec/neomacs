use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::num::{NonZeroI32, NonZeroUsize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, EmacsRuntime, FLYCHECK_MELPA_PIN, S_MELPA_PIN,
    TIDE_MELPA_PIN, prepare_cached_locked_melpa_package,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(240);
const PINNED_TSSERVER_SHA256: &str =
    "708b584a9937448f5400b09817774823e6ae339000ddeabc0e7766dfa428793a";
const PINNED_TSSERVER_BUNDLE_MANIFEST_SHA256: &str =
    "f95ede2ee0564044ca2d61127e75fcd4b3af5b260998823c108d3c59add400da";
const PINNED_DIAGNOSTICS_CAPTURE_SHA256: &str =
    "a756232b9c742d43fd31c85883dc3042b6d36c2e07e0e221f2d76bddb63c65ee";
const PINNED_DIAGNOSTICS_BUNDLE_EVIDENCE_SHA256: &str =
    "95a29c1bc66a1f28753f65bbed67e0cf98baf37fc5e6ab8d7025363ff36aaa2f";
const PINNED_TSSERVER_BUNDLE: &[(&str, &str)] = &[
    (
        "lib.decorators.d.ts",
        "189c0703923150aa30673fa3de411346d727cc44a11c75d05d7cf9ef095daa22",
    ),
    (
        "lib.decorators.legacy.d.ts",
        "782dec38049b92d4e85c1585fbea5474a219c6984a35b004963b00beb1aab538",
    ),
    (
        "lib.dom.d.ts",
        "3dda5344576193a4ae48b8d03f105c86f20b2f2aff0a1d1fd7935f5d68649654",
    ),
    (
        "lib.dom.iterable.d.ts",
        "35299ae4a62086698444a5aaee27fc7aa377c68cbb90b441c9ace246ffd05c97",
    ),
    (
        "lib.es2015.collection.d.ts",
        "17bea081b9c0541f39dd1ae9bc8c78bdd561879a682e60e2f25f688c0ecab248",
    ),
    (
        "lib.es2015.core.d.ts",
        "9d9885c728913c1d16e0d2831b40341d6ad9a0ceecaabc55209b306ad9c736a5",
    ),
    (
        "lib.es2015.d.ts",
        "45b7ab580deca34ae9729e97c13cfd999df04416a79116c3bfb483804f85ded4",
    ),
    (
        "lib.es2015.generator.d.ts",
        "4443e68b35f3332f753eacc66a04ac1d2053b8b035a0e0ac1d455392b5e243b3",
    ),
    (
        "lib.es2015.iterable.d.ts",
        "ab22100fdd0d24cfc2cc59d0a00fc8cf449830d9c4030dc54390a46bd562e929",
    ),
    (
        "lib.es2015.promise.d.ts",
        "f7bd636ae3a4623c503359ada74510c4005df5b36de7f23e1db8a5c543fd176b",
    ),
    (
        "lib.es2015.proxy.d.ts",
        "ce691fb9e5c64efb9547083e4a34091bcbe5bdb41027e310ebba8f7d96a98671",
    ),
    (
        "lib.es2015.reflect.d.ts",
        "8d697a2a929a5fcb38b7a65594020fcef05ec1630804a33748829c5ff53640d0",
    ),
    (
        "lib.es2015.symbol.d.ts",
        "0c20f4d2358eb679e4ae8a4432bdd96c857a2960fd6800b21ec4008ec59d60ea",
    ),
    (
        "lib.es2015.symbol.wellknown.d.ts",
        "36ae84ccc0633f7c0787bc6108386c8b773e95d3b052d9464a99cd9b8795fbec",
    ),
    (
        "lib.es2016.array.include.d.ts",
        "82d0d8e269b9eeac02c3bd1c9e884e85d483fcb2cd168bccd6bc54df663da031",
    ),
    (
        "lib.es2016.d.ts",
        "dc48272d7c333ccf58034c0026162576b7d50ea0e69c3b9292f803fc20720fd5",
    ),
    (
        "lib.es2017.d.ts",
        "27147504487dc1159369da4f4da8a26406364624fa9bc3db632f7d94a5bae2c3",
    ),
    (
        "lib.es2017.intl.d.ts",
        "376d554d042fb409cb55b5cbaf0b2b4b7e669619493c5d18d5fa8bd67273f82a",
    ),
    (
        "lib.es2017.object.d.ts",
        "b8deab98702588840be73d67f02412a2d45a417a3c097b2e96f7f3a42ac483d1",
    ),
    (
        "lib.es2017.sharedmemory.d.ts",
        "4738f2420687fd85629c9efb470793bb753709c2379e5f85bc1815d875ceadcd",
    ),
    (
        "lib.es2017.string.d.ts",
        "2f11ff796926e0832f9ae148008138ad583bd181899ab7dd768a2666700b1893",
    ),
    (
        "lib.es2017.typedarrays.d.ts",
        "9fc46429fbe091ac5ad2608c657201eb68b6f1b8341bd6d670047d32ed0a88fa",
    ),
    (
        "lib.es2018.asyncgenerator.d.ts",
        "61c37c1de663cf4171e1192466e52c7a382afa58da01b1dc75058f032ddf0839",
    ),
    (
        "lib.es2018.asynciterable.d.ts",
        "c4138a3dd7cd6cf1f363ca0f905554e8d81b45844feea17786cdf1626cb8ea06",
    ),
    (
        "lib.es2018.d.ts",
        "5e1c4c362065a6b95ff952c0eab010f04dcd2c3494e813b493ecfd4fcb9fc0d8",
    ),
    (
        "lib.es2018.intl.d.ts",
        "6ff3e2452b055d8f0ec026511c6582b55d935675af67cdb67dd1dc671e8065df",
    ),
    (
        "lib.es2018.promise.d.ts",
        "03de17b810f426a2f47396b0b99b53a82c1b60e9cba7a7edda47f9bb077882f4",
    ),
    (
        "lib.es2018.regexp.d.ts",
        "8184c6ddf48f0c98429326b428478ecc6143c27f79b79e85740f17e6feb090f1",
    ),
    (
        "lib.es2019.array.d.ts",
        "261c4d2cf86ac5a89ad3fb3fafed74cbb6f2f7c1d139b0540933df567d64a6ca",
    ),
    (
        "lib.es2019.d.ts",
        "68d73b4a11549f9c0b7d352d10e91e5dca8faa3322bfb77b661839c42b1ddec7",
    ),
    (
        "lib.es2019.intl.d.ts",
        "15a630d6817718a2ddd7088c4f83e4673fde19fa992d2eae2cf51132a302a5d3",
    ),
    (
        "lib.es2019.object.d.ts",
        "6af1425e9973f4924fca986636ac19a0cf9909a7e0d9d3009c349e6244e957b6",
    ),
    (
        "lib.es2019.string.d.ts",
        "576711e016cf4f1804676043e6a0a5414252560eb57de9faceee34d79798c850",
    ),
    (
        "lib.es2019.symbol.d.ts",
        "89c1b1281ba7b8a96efc676b11b264de7a8374c5ea1e6617f11880a13fc56dc6",
    ),
    (
        "lib.es2020.bigint.d.ts",
        "f06948deb2a51aae25184561c9640fb66afeddb34531a9212d011792b1d19e0a",
    ),
    (
        "lib.es2020.d.ts",
        "5efce4fc3c29ea84e8928f97adec086e3dc876365e0982cc8479a07954a3efd4",
    ),
    (
        "lib.es2020.date.d.ts",
        "01e0ee7e1f661acedb08b51f8a9b7d7f959e9cdb6441360f06522cc3aea1bf2e",
    ),
    (
        "lib.es2020.full.d.ts",
        "322cc0ca9c311414642c0d7ef3b57beedbac198ca074e3e109a4be4c366dcb81",
    ),
    (
        "lib.es2020.intl.d.ts",
        "9cc66b0513ad41cb5f5372cca86ef83a0d37d1c1017580b7dace3ea5661836df",
    ),
    (
        "lib.es2020.number.d.ts",
        "368af93f74c9c932edd84c58883e736c9e3d53cec1fe24c0b0ff451f529ceab1",
    ),
    (
        "lib.es2020.promise.d.ts",
        "ac17a97f816d53d9dd79b0d235e1c0ed54a8cc6a0677e9a3d61efb480b2a3e4e",
    ),
    (
        "lib.es2020.sharedmemory.d.ts",
        "bf14a426dbbf1022d11bd08d6b8e709a2e9d246f0c6c1032f3b2edb9a902adbe",
    ),
    (
        "lib.es2020.string.d.ts",
        "ec0104fee478075cb5171e5f4e3f23add8e02d845ae0165bfa3f1099241fa2aa",
    ),
    (
        "lib.es2020.symbol.wellknown.d.ts",
        "2b72d528b2e2fe3c57889ca7baef5e13a56c957b946906d03767c642f386bbc3",
    ),
    (
        "lib.es5.d.ts",
        "f59215c5f1d886b05395ee7aca73e0ac69ddfad2843aa88530e797879d511bad",
    ),
    (
        "lib.scripthost.d.ts",
        "7d2dbc2a0250400af0809b0ad5f84686e84c73526de931f84560e483eb16b03c",
    ),
    (
        "lib.webworker.importscripts.d.ts",
        "c5c5565225fce2ede835725a92a28ece149f83542aa4866cfb10290bff7b8996",
    ),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplayRuntimeIdentity {
    interpreter: DirectExecutable,
    tsserver: DirectExecutable,
    tsserver_bundle: Vec<(TsserverRelativePath, Sha256Digest)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectExecutable {
    path: PathBuf,
    digest: Sha256Digest,
}

impl DirectExecutable {
    fn preflight(
        path: impl AsRef<Path>,
        expected_digest: Option<Sha256Digest>,
    ) -> Result<Self, String> {
        let path = fs::canonicalize(path.as_ref()).map_err(|error| {
            format!(
                "cannot canonicalize replay executable {:?}: {error}",
                path.as_ref()
            )
        })?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect replay executable {path:?}: {error}"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "replay executable is not a direct regular file: {path:?}"
            ));
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("cannot read replay executable {path:?}: {error}"))?;
        let digest = Sha256Digest::of(&bytes);
        if let Some(expected) = expected_digest
            && digest != expected
        {
            return Err(format!(
                "replay executable {path:?} digest mismatch: expected {}, got {}",
                expected.hex(),
                digest.hex(),
            ));
        }
        Ok(Self { path, digest })
    }

    fn utf8_path(&self) -> Result<&str, String> {
        self.path
            .to_str()
            .ok_or_else(|| format!("replay executable path is not UTF-8: {:?}", self.path))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InterpreterExecutable(DirectExecutable);

impl InterpreterExecutable {
    fn preflight(path: impl AsRef<Path>) -> Result<Self, String> {
        let artifact = DirectExecutable::preflight(path, None)?;
        #[cfg(unix)]
        if fs::metadata(&artifact.path)
            .map_err(|error| format!("cannot inspect interpreter {:?}: {error}", artifact.path))?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(format!(
                "Tide replay interpreter is not executable: {:?}",
                artifact.path,
            ));
        }
        Ok(Self(artifact))
    }
}

impl ReplayRuntimeIdentity {
    pub(crate) fn preflight() -> Result<Self, String> {
        let interpreter = resolve_path_executable("python3")?.0;
        let package =
            prepare_cached_locked_melpa_package(&EmacsRuntime::gnu_emacs(), TIDE_MELPA_PIN)?;
        let tsserver = DirectExecutable::preflight(
            package.join("tsserver/tsserver.js"),
            Some(Sha256Digest::parse(PINNED_TSSERVER_SHA256)?),
        )?;
        let tsserver_bundle = preflight_tsserver_bundle(&tsserver)?;
        Ok(Self {
            interpreter,
            tsserver,
            tsserver_bundle,
        })
    }
}

fn preflight_tsserver_bundle(
    tsserver: &DirectExecutable,
) -> Result<Vec<(TsserverRelativePath, Sha256Digest)>, String> {
    let entries = validate_tsserver_bundle_manifest(PINNED_TSSERVER_BUNDLE)?;
    let base = tsserver
        .path
        .parent()
        .ok_or_else(|| "preflighted tsserver has no parent directory".to_owned())?;
    let base_metadata = fs::symlink_metadata(base)
        .map_err(|error| format!("cannot inspect tsserver bundle directory {base:?}: {error}"))?;
    if !base_metadata.file_type().is_dir() || base_metadata.file_type().is_symlink() {
        return Err(format!(
            "tsserver bundle base is not a direct directory: {base:?}"
        ));
    }
    if fs::canonicalize(base)
        .map_err(|error| format!("cannot canonicalize tsserver bundle directory: {error}"))?
        != base
    {
        return Err(format!(
            "tsserver bundle base is not canonically spelled: {base:?}"
        ));
    }
    for (relative, expected_digest) in &entries {
        let path = base.join(relative.display());
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect tsserver bundle file {path:?}: {error}"))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "tsserver bundle member is not a direct regular file: {path:?}"
            ));
        }
        let canonical = fs::canonicalize(&path).map_err(|error| {
            format!("cannot canonicalize tsserver bundle file {path:?}: {error}")
        })?;
        if canonical != path || canonical.parent() != Some(base) {
            return Err(format!(
                "tsserver bundle member escaped its canonical parent: {path:?}"
            ));
        }
        let actual_digest = Sha256Digest::of(
            &fs::read(&path)
                .map_err(|error| format!("cannot read tsserver bundle file {path:?}: {error}"))?,
        );
        if actual_digest != *expected_digest {
            return Err(format!(
                "tsserver bundle member {path:?} digest mismatch: expected {}, got {}",
                expected_digest.hex(),
                actual_digest.hex(),
            ));
        }
    }
    Ok(entries)
}

fn validate_tsserver_bundle_manifest(
    manifest_entries: &[(&str, &str)],
) -> Result<Vec<(TsserverRelativePath, Sha256Digest)>, String> {
    if manifest_entries.len() != 47 {
        return Err(format!(
            "pinned Tide tsserver bundle has {} entries instead of 47",
            manifest_entries.len(),
        ));
    }
    let mut manifest = String::new();
    let mut previous = None;
    let mut entries = Vec::with_capacity(manifest_entries.len());
    for &(relative, digest) in manifest_entries {
        if previous.is_some_and(|prior| prior >= relative) {
            return Err(format!(
                "pinned Tide tsserver bundle is not strictly sorted at {relative:?}"
            ));
        }
        previous = Some(relative);
        let relative = TsserverRelativePath::new(relative)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(format!(
                "pinned Tide tsserver bundle digest is not lowercase SHA-256: {digest:?}"
            ));
        }
        let digest = Sha256Digest::parse(digest)?;
        writeln!(manifest, "{} {}", relative.display(), digest.hex())
            .expect("writing to a String cannot fail");
        entries.push((relative, digest));
    }
    if Sha256Digest::of(manifest.as_bytes())
        != Sha256Digest::parse(PINNED_TSSERVER_BUNDLE_MANIFEST_SHA256)?
    {
        return Err("pinned Tide tsserver bundle manifest digest mismatch".to_owned());
    }

    Ok(entries)
}

fn resolve_path_executable(name: &str) -> Result<InterpreterExecutable, String> {
    let path = std::env::var_os("PATH")
        .ok_or_else(|| "PATH is unavailable for python3 preflight".to_owned())?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return InterpreterExecutable::preflight(candidate);
        }
    }
    Err(format!(
        "cannot find {name:?} on PATH for Tide replay preflight"
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TideScenario {
    Lifecycle,
    Navigation,
    References,
    Diagnostics,
    Edits,
    Rename,
    FailureRecovery,
}

impl TideScenario {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Lifecycle => "lifecycle",
            Self::Navigation => "navigation",
            Self::References => "references",
            Self::Diagnostics => "diagnostics",
            Self::Edits => "edits",
            Self::Rename => "rename",
            Self::FailureRecovery => "failure-recovery",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticKind {
    Syntactic,
    Semantic,
    Suggestion,
}

impl DiagnosticKind {
    const fn command(self) -> &'static str {
        match self {
            Self::Syntactic => "syntacticDiagnosticsSync",
            Self::Semantic => "semanticDiagnosticsSync",
            Self::Suggestion => "suggestionDiagnosticsSync",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct RequestOrdinal(NonZeroUsize);

impl RequestOrdinal {
    pub(crate) fn new(value: usize) -> Result<Self, String> {
        NonZeroUsize::new(value)
            .map(Self)
            .ok_or_else(|| "a Tide request ordinal must be nonzero".to_owned())
    }

    const fn get(self) -> usize {
        self.0.get()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceRelativePath(String);

impl WorkspaceRelativePath {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(format!(
                "Tide fixture path must be nonempty and relative: {path:?}"
            ));
        }
        if path.components().any(|part| {
            matches!(
                part,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        }) {
            return Err(format!(
                "Tide fixture path escapes its owned root: {path:?}"
            ));
        }
        let path = path
            .to_str()
            .ok_or_else(|| format!("Tide fixture path is not valid UTF-8: {path:?}"))?;
        if path.contains('\\') {
            return Err(format!(
                "Tide fixture path uses a platform-ambiguous backslash: {path:?}"
            ));
        }
        let canonical_spelling = PathBuf::from(path)
            .components()
            .map(|component| match component {
                Component::Normal(component) => component
                    .to_str()
                    .expect("the complete path was already validated as UTF-8"),
                _ => unreachable!("non-normal path components were rejected above"),
            })
            .collect::<Vec<_>>()
            .join("/");
        if canonical_spelling != path {
            return Err(format!(
                "Tide fixture path has a non-canonical spelling: {path:?}"
            ));
        }
        Ok(Self(path.to_owned()))
    }

    fn display(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TsserverRelativePath(String);

impl TsserverRelativePath {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(format!(
                "Tide tsserver bundle path must be nonempty and relative: {path:?}"
            ));
        }
        if path.components().any(|part| {
            matches!(
                part,
                Component::CurDir
                    | Component::ParentDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        }) {
            return Err(format!(
                "Tide tsserver bundle path escapes its installed directory: {path:?}"
            ));
        }
        let path = path
            .to_str()
            .ok_or_else(|| format!("Tide tsserver bundle path is not UTF-8: {path:?}"))?;
        if path.contains('\\') {
            return Err(format!(
                "Tide tsserver bundle path uses a platform-ambiguous backslash: {path:?}"
            ));
        }
        let canonical_spelling = PathBuf::from(path)
            .components()
            .map(|component| match component {
                Component::Normal(component) => component
                    .to_str()
                    .expect("the complete tsserver path was already validated as UTF-8"),
                _ => unreachable!("non-normal tsserver path components were rejected above"),
            })
            .collect::<Vec<_>>()
            .join("/");
        if canonical_spelling != path {
            return Err(format!(
                "Tide tsserver bundle path has a non-canonical spelling: {path:?}"
            ));
        }
        if !PINNED_TSSERVER_BUNDLE
            .iter()
            .any(|(expected, _)| *expected == path)
        {
            return Err(format!(
                "Tide tsserver bundle path is not in the pinned manifest: {path:?}"
            ));
        }
        Ok(Self(path.to_owned()))
    }

    fn display(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedAbsoluteRoot(PathBuf);

impl OwnedAbsoluteRoot {
    fn new(workspace_root: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Result<Self, String> {
        let workspace_root = workspace_root.into();
        let root = root.into();
        if !workspace_root.is_absolute() || !root.is_absolute() {
            return Err("Tide workspace and owner roots must be absolute".to_owned());
        }
        if workspace_root.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        }) || root.components().any(|component| {
            matches!(
                component,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        }) {
            return Err(format!("Tide owner root is not canonical: {root:?}"));
        }
        let workspace_root = fs::canonicalize(&workspace_root).map_err(|error| {
            format!("cannot canonicalize Tide workspace {workspace_root:?}: {error}")
        })?;
        let workspace_tmp = fs::canonicalize(workspace_root.join("tmp"))
            .map_err(|error| format!("cannot canonicalize repository tmp: {error}"))?;
        let parent = root
            .parent()
            .ok_or_else(|| format!("Tide owner root has no parent: {root:?}"))?;
        let parent = fs::canonicalize(parent).map_err(|error| {
            format!("cannot canonicalize Tide owner parent {parent:?}: {error}")
        })?;
        let leaf = root
            .file_name()
            .ok_or_else(|| format!("Tide owner root has no leaf: {root:?}"))?;
        let root = parent.join(leaf);
        if root == workspace_tmp || !root.starts_with(&workspace_tmp) {
            return Err(format!(
                "Tide owner root must be a child of repository tmp: {root:?}"
            ));
        }
        match fs::symlink_metadata(&root) {
            Ok(_) => {
                return Err(format!(
                    "Tide owner root must not preexist, including as a symlink: {root:?}"
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot verify that Tide owner root is absent {root:?}: {error}"
                ));
            }
        }
        Ok(Self(root))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LineOffset {
    line: NonZeroUsize,
    offset: NonZeroUsize,
}

impl LineOffset {
    pub(crate) fn new(line: usize, offset: usize) -> Result<Self, String> {
        let line = NonZeroUsize::new(line)
            .ok_or_else(|| "TypeScript protocol lines are one-based".to_owned())?;
        let offset = NonZeroUsize::new(offset)
            .ok_or_else(|| "TypeScript protocol offsets are one-based".to_owned())?;
        Ok(Self { line, offset })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileRequest {
    file: WorkspaceRelativePath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PointRequest {
    file: WorkspaceRelativePath,
    point: LineOffset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RangeRequest {
    file: WorkspaceRelativePath,
    start: LineOffset,
    end: LineOffset,
}

impl RangeRequest {
    pub(crate) fn new(
        file: WorkspaceRelativePath,
        start: LineOffset,
        end: LineOffset,
    ) -> Result<Self, String> {
        if (end.line, end.offset) < (start.line, start.offset) {
            return Err("a Tide format range cannot end before it starts".to_owned());
        }
        Ok(Self { file, start, end })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenRequest {
    file: WorkspaceRelativePath,
    script_kind: OpenScriptKind,
    manual_file_content: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenScriptKind {
    Explicit(ScriptKind),
    Inferred,
}

impl OpenRequest {
    pub(crate) fn immediate(
        file: WorkspaceRelativePath,
        script_kind: ScriptKind,
    ) -> Result<Self, String> {
        if !file.display().ends_with(".js") || script_kind != ScriptKind::JavaScript {
            return Err(format!(
                "the Tide JavaScript corpus needs an explicit JS open for a .js file: {}",
                file.display(),
            ));
        }
        Ok(Self {
            file,
            script_kind: OpenScriptKind::Explicit(script_kind),
            manual_file_content: None,
        })
    }

    pub(crate) fn inferred(file: WorkspaceRelativePath) -> Result<Self, String> {
        let extension = Path::new(file.display())
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        if matches!(
            extension.to_ascii_uppercase().as_str(),
            "TS" | "JS" | "TSX" | "JSX"
        ) {
            return Err(format!(
                "Tide source emits scriptKindName for supported source extension: {}",
                file.display(),
            ));
        }
        Ok(Self {
            file,
            script_kind: OpenScriptKind::Inferred,
            manual_file_content: None,
        })
    }

    pub(crate) fn manual(
        file: WorkspaceRelativePath,
        script_kind: ScriptKind,
        file_content: String,
    ) -> Result<Self, String> {
        let mut request = Self::immediate(file, script_kind)?;
        request.manual_file_content = Some(file_content);
        Ok(request)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScriptKind {
    JavaScript,
}

impl ScriptKind {
    const fn protocol_name(self) -> &'static str {
        match self {
            Self::JavaScript => "JS",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FormatOptions {
    tab_size: NonZeroUsize,
    indent_size: NonZeroUsize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UserPreferences {
    include_module_exports: bool,
    include_insert_text: bool,
    allow_new_files: bool,
    generate_return_in_doc_template: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigureRequest {
    file: WorkspaceRelativePath,
    host_info: HostInfoToken,
    format: FormatOptions,
    preferences: UserPreferences,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HostInfoToken;

impl HostInfoToken {
    const fn normalized() -> Self {
        Self
    }

    const fn protocol_value(self) -> &'static str {
        "[HOSTINFO]"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileNameListRequest {
    Null,
    Include,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectInfoRequest {
    file: WorkspaceRelativePath,
    file_names: FileNameListRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NavToRequest {
    query: String,
    file: WorkspaceRelativePath,
    current_file_only: bool,
}

impl NavToRequest {
    pub(crate) fn new(
        query: impl Into<String>,
        file: WorkspaceRelativePath,
        current_file_only: bool,
    ) -> Result<Self, String> {
        let query = query.into();
        if query.is_empty() {
            return Err("a Tide navto query must be nonempty".to_owned());
        }
        Ok(Self {
            query,
            file,
            current_file_only,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileRenameRequest {
    old_file: WorkspaceRelativePath,
    new_file: WorkspaceRelativePath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReloadRequest {
    file: WorkspaceRelativePath,
    temporary_file: TideTempFileToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TideTempFileToken {
    source: WorkspaceRelativePath,
    content_digest: Sha256Digest,
}

impl TideTempFileToken {
    pub(crate) fn new(source: WorkspaceRelativePath, content_digest: Sha256Digest) -> Self {
        Self {
            source,
            content_digest,
        }
    }

    const fn protocol_value(&self) -> &'static str {
        "[TIDE-TMP]"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectErrorsRequest {
    file: WorkspaceRelativePath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixtureFile {
    path: WorkspaceRelativePath,
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl FixtureFile {
    pub(crate) fn new(
        path: WorkspaceRelativePath,
        bytes: Vec<u8>,
        expected_digest: Sha256Digest,
    ) -> Result<Self, String> {
        let digest = Sha256Digest::of(&bytes);
        if digest != expected_digest {
            return Err(format!(
                "Tide fixture {} digest mismatch: expected {}, got {}",
                path.display(),
                expected_digest.hex(),
                digest.hex(),
            ));
        }
        Ok(Self {
            path,
            bytes,
            digest,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixtureManifest(Vec<FixtureFile>);

impl FixtureManifest {
    pub(crate) fn new(mut files: Vec<FixtureFile>) -> Result<Self, String> {
        if files.is_empty() {
            return Err("a Tide replay needs a nonempty fixture manifest".to_owned());
        }
        files.sort_by(|left, right| left.path.display().cmp(right.path.display()));
        if files.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err("a Tide fixture manifest cannot contain duplicate paths".to_owned());
        }
        Ok(Self(files))
    }

    fn generation(&self) -> FixtureGeneration {
        FixtureGeneration(vec![
            self.0
                .iter()
                .map(|fixture| FixtureExpectation::Present {
                    path: fixture.path.clone(),
                    digest: fixture.digest,
                })
                .collect(),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FixtureExpectation {
    Present {
        path: WorkspaceRelativePath,
        digest: Sha256Digest,
    },
    Missing(WorkspaceRelativePath),
}

impl FixtureExpectation {
    fn path(&self) -> &WorkspaceRelativePath {
        match self {
            Self::Present { path, .. } | Self::Missing(path) => path,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixtureGeneration(Vec<Vec<FixtureExpectation>>);

impl FixtureGeneration {
    pub(crate) fn new(mut files: Vec<FixtureExpectation>) -> Result<Self, String> {
        if files.is_empty() {
            return Err("a Tide exchange needs a nonempty fixture generation".to_owned());
        }
        files.sort_by(|left, right| left.path().display().cmp(right.path().display()));
        if files
            .windows(2)
            .any(|pair| pair[0].path() == pair[1].path())
        {
            return Err("a Tide fixture generation cannot contain duplicate paths".to_owned());
        }
        Ok(Self(vec![files]))
    }

    pub(crate) fn one_of(generations: Vec<Self>) -> Result<Self, String> {
        if generations.len() < 2 {
            return Err("a Tide fixture transition needs at least two alternatives".to_owned());
        }
        let alternatives = generations
            .into_iter()
            .map(|generation| {
                if generation.0.len() != 1 {
                    return Err("nested Tide fixture transitions are forbidden".to_owned());
                }
                Ok(generation.0.into_iter().next().unwrap())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let paths = alternatives[0]
            .iter()
            .map(|fixture| fixture.path().display())
            .collect::<Vec<_>>();
        if alternatives.iter().any(|alternative| {
            alternative
                .iter()
                .map(|fixture| fixture.path().display())
                .collect::<Vec<_>>()
                != paths
        }) {
            return Err(
                "Tide fixture transition alternatives must own the same complete path set"
                    .to_owned(),
            );
        }
        if alternatives.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("Tide fixture transition alternatives must be distinct".to_owned());
        }
        Ok(Self(alternatives))
    }

    fn json(&self) -> serde_json::Value {
        let alternatives = self
            .0
            .iter()
            .map(|alternative| {
                alternative
                    .iter()
                    .map(|fixture| match fixture {
                        FixtureExpectation::Present { path, digest } => serde_json::json!({
                            "path": path.display(),
                            "state": "present",
                            "sha256": digest.hex(),
                        }),
                        FixtureExpectation::Missing(path) => serde_json::json!({
                            "path": path.display(),
                            "state": "missing",
                        }),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if alternatives.len() == 1 {
            serde_json::json!(alternatives.into_iter().next().unwrap())
        } else {
            serde_json::json!({ "one_of": alternatives })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TsRequest {
    Open(OpenRequest),
    Configure(ConfigureRequest),
    Status,
    ProjectInfo(ProjectInfoRequest),
    QuickInfoFull(PointRequest),
    QuickInfo(PointRequest),
    SignatureHelp(PointRequest),
    Definition(PointRequest),
    NavTo(NavToRequest),
    NavTree(FileRequest),
    References(PointRequest),
    Diagnostics(DiagnosticKind, FileRequest),
    DocumentHighlights(PointRequest),
    Format(RangeRequest),
    OrganizeImports(FileRequest),
    DocCommentTemplate(PointRequest),
    Rename(PointRequest),
    FileRename(FileRenameRequest),
    Reload(ReloadRequest),
    Close(FileRequest),
    ProjectErrors(ProjectErrorsRequest),
}

impl TsRequest {
    fn command(&self) -> &'static str {
        match self {
            Self::Open(_) => "open",
            Self::Configure(_) => "configure",
            Self::Status => "status",
            Self::ProjectInfo(_) => "projectInfo",
            Self::QuickInfoFull(_) => "quickinfo-full",
            Self::QuickInfo(_) => "quickinfo",
            Self::SignatureHelp(_) => "signatureHelp",
            Self::Definition(_) => "definition",
            Self::NavTo(_) => "navto",
            Self::NavTree(_) => "navtree",
            Self::References(_) => "references",
            Self::Diagnostics(kind, _) => kind.command(),
            Self::DocumentHighlights(_) => "documentHighlights",
            Self::Format(_) => "format",
            Self::OrganizeImports(_) => "organizeImports",
            Self::DocCommentTemplate(_) => "docCommentTemplate",
            Self::Rename(_) => "rename",
            Self::FileRename(_) => "getEditsForFileRename",
            Self::Reload(_) => "reload",
            Self::Close(_) => "close",
            Self::ProjectErrors(_) => "geterrForProject",
        }
    }

    const fn callback_policy(&self) -> CallbackPolicy {
        match self {
            Self::Open(_)
            | Self::Configure(_)
            | Self::Reload(_)
            | Self::Close(_)
            | Self::ProjectErrors(_) => CallbackPolicy::NotRegistered,
            _ => CallbackPolicy::Registered,
        }
    }

    const fn wire_response_count(&self) -> usize {
        match self {
            Self::Open(_) | Self::Close(_) | Self::ProjectErrors(_) => 0,
            // TS 5.1.3 writes both the immediate acknowledgement and the
            // later reloadFinished response. Tide deliberately registers no
            // callback, but both frames still traverse its real filter.
            Self::Reload(_) => 2,
            _ => 1,
        }
    }

    const fn requires_request_completed(&self) -> bool {
        matches!(self, Self::ProjectErrors(_))
    }

    fn token_plan(&self) -> Vec<RequestToken> {
        let root = |path: &WorkspaceRelativePath, field: &[JsonPathSegment]| RequestToken {
            field: field.to_vec(),
            kind: RequestTokenKind::RootPath(path.clone()),
        };
        let file_field = [
            JsonPathSegment::Key("arguments"),
            JsonPathSegment::Key("file"),
        ];
        match self {
            Self::Open(request) => vec![root(&request.file, &file_field)],
            Self::Configure(request) => vec![
                RequestToken {
                    field: vec![
                        JsonPathSegment::Key("arguments"),
                        JsonPathSegment::Key("hostInfo"),
                    ],
                    kind: RequestTokenKind::HostInfo,
                },
                root(&request.file, &file_field),
            ],
            Self::Status => Vec::new(),
            Self::ProjectInfo(request) => vec![root(&request.file, &file_field)],
            Self::QuickInfoFull(request)
            | Self::QuickInfo(request)
            | Self::SignatureHelp(request)
            | Self::Definition(request)
            | Self::References(request)
            | Self::Rename(request)
            | Self::DocCommentTemplate(request)
            | Self::DocumentHighlights(request) => {
                let mut tokens = vec![root(&request.file, &file_field)];
                if matches!(self, Self::DocumentHighlights(_)) {
                    tokens.push(root(
                        &request.file,
                        &[
                            JsonPathSegment::Key("arguments"),
                            JsonPathSegment::Key("filesToSearch"),
                            JsonPathSegment::Index(0),
                        ],
                    ));
                }
                tokens
            }
            Self::NavTo(request) => vec![root(&request.file, &file_field)],
            Self::NavTree(request)
            | Self::Diagnostics(_, request)
            | Self::Close(request)
            | Self::OrganizeImports(request) => {
                if matches!(self, Self::OrganizeImports(_)) {
                    vec![root(
                        &request.file,
                        &[
                            JsonPathSegment::Key("arguments"),
                            JsonPathSegment::Key("scope"),
                            JsonPathSegment::Key("args"),
                            JsonPathSegment::Key("file"),
                        ],
                    )]
                } else {
                    vec![root(&request.file, &file_field)]
                }
            }
            Self::Format(request) => vec![root(&request.file, &file_field)],
            Self::FileRename(request) => vec![
                root(
                    &request.old_file,
                    &[
                        JsonPathSegment::Key("arguments"),
                        JsonPathSegment::Key("oldFilePath"),
                    ],
                ),
                root(
                    &request.new_file,
                    &[
                        JsonPathSegment::Key("arguments"),
                        JsonPathSegment::Key("newFilePath"),
                    ],
                ),
                root(&request.old_file, &file_field),
            ],
            Self::Reload(request) => vec![
                root(&request.file, &file_field),
                RequestToken {
                    field: vec![
                        JsonPathSegment::Key("arguments"),
                        JsonPathSegment::Key("tmpfile"),
                    ],
                    kind: RequestTokenKind::TideTemp(request.temporary_file.clone()),
                },
            ],
            Self::ProjectErrors(request) => vec![root(&request.file, &file_field)],
        }
    }

    fn normalized_json(&self, ordinal: RequestOrdinal) -> String {
        let seq = ordinal.get();
        let arguments = match self {
            Self::Open(request) => {
                let mut arguments = format!("{{\"file\":{}", json_path(&request.file));
                if let OpenScriptKind::Explicit(script_kind) = request.script_kind {
                    write!(
                        arguments,
                        ",\"scriptKindName\":{}",
                        json_string(script_kind.protocol_name()),
                    )
                    .expect("writing to a String cannot fail");
                }
                if let Some(file_content) = &request.manual_file_content {
                    write!(arguments, ",\"fileContent\":{}", json_string(file_content))
                        .expect("writing to a String cannot fail");
                }
                arguments.push('}');
                arguments
            }
            Self::Configure(request) => format!(
                concat!(
                    "{{\"hostInfo\":{},\"file\":{},",
                    "\"formatOptions\":{{\"tabSize\":{},\"indentSize\":{}}},",
                    "\"preferences\":{{\"includeCompletionsForModuleExports\":{},",
                    "\"includeCompletionsWithInsertText\":{},",
                    "\"allowTextChangesInNewFiles\":{},",
                    "\"generateReturnInDocTemplate\":{}}}}}"
                ),
                json_string(request.host_info.protocol_value()),
                json_path(&request.file),
                request.format.tab_size,
                request.format.indent_size,
                request.preferences.include_module_exports,
                request.preferences.include_insert_text,
                request.preferences.allow_new_files,
                request.preferences.generate_return_in_doc_template,
            ),
            Self::Status => "null".to_owned(),
            Self::ProjectInfo(request) => format!(
                "{{\"file\":{},\"needFileNameList\":{}}}",
                json_path(&request.file),
                match request.file_names {
                    FileNameListRequest::Null => "null",
                    FileNameListRequest::Include => "true",
                },
            ),
            Self::QuickInfoFull(request)
            | Self::QuickInfo(request)
            | Self::SignatureHelp(request)
            | Self::Definition(request)
            | Self::References(request)
            | Self::Rename(request)
            | Self::DocCommentTemplate(request) => point_arguments(request),
            Self::NavTo(request) => format!(
                "{{\"file\":{},\"searchValue\":{},\"maxResultCount\":100,\"currentFileOnly\":{}}}",
                json_path(&request.file),
                json_string(&request.query),
                request.current_file_only,
            ),
            Self::NavTree(request) | Self::Diagnostics(_, request) | Self::Close(request) => {
                format!("{{\"file\":{}}}", json_path(&request.file))
            }
            Self::OrganizeImports(request) => format!(
                "{{\"scope\":{{\"type\":\"file\",\"args\":{{\"file\":{}}}}}}}",
                json_path(&request.file),
            ),
            Self::DocumentHighlights(request) => format!(
                "{{\"file\":{},\"line\":{},\"offset\":{},\"filesToSearch\":[{}]}}",
                json_path(&request.file),
                request.point.line,
                request.point.offset,
                json_path(&request.file),
            ),
            Self::Format(request) => format!(
                concat!(
                    "{{\"file\":{},\"line\":{},\"offset\":{},",
                    "\"endLine\":{},\"endOffset\":{}}}"
                ),
                json_path(&request.file),
                request.start.line,
                request.start.offset,
                request.end.line,
                request.end.offset,
            ),
            Self::FileRename(request) => format!(
                "{{\"oldFilePath\":{},\"newFilePath\":{},\"file\":{}}}",
                json_path(&request.old_file),
                json_path(&request.new_file),
                json_path(&request.old_file),
            ),
            Self::Reload(request) => format!(
                "{{\"file\":{},\"tmpfile\":{}}}",
                json_path(&request.file),
                json_string(request.temporary_file.protocol_value()),
            ),
            Self::ProjectErrors(request) => {
                format!("{{\"file\":{},\"delay\":{}}}", json_path(&request.file), 0,)
            }
        };
        format!(
            "{{\"command\":{},\"seq\":{},\"arguments\":{arguments}}}",
            json_string(self.command()),
            json_string(&seq.to_string()),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum JsonPathSegment {
    Key(&'static str),
    Index(usize),
}

impl JsonPathSegment {
    fn json(&self) -> Value {
        match self {
            Self::Key(key) => Value::String((*key).to_owned()),
            Self::Index(index) => serde_json::json!(index),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RequestTokenKind {
    RootPath(WorkspaceRelativePath),
    HostInfo,
    TideTemp(TideTempFileToken),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RequestToken {
    field: Vec<JsonPathSegment>,
    kind: RequestTokenKind,
}

impl RequestToken {
    fn json(&self) -> Value {
        let field = self
            .field
            .iter()
            .map(JsonPathSegment::json)
            .collect::<Vec<_>>();
        match &self.kind {
            RequestTokenKind::RootPath(path) => serde_json::json!({
                "field": field,
                "kind": "root-path",
                "relative": path.display(),
            }),
            RequestTokenKind::HostInfo => serde_json::json!({
                "field": field,
                "kind": "host-info",
            }),
            RequestTokenKind::TideTemp(temporary) => serde_json::json!({
                "field": field,
                "kind": "tide-temp",
                "source": temporary.source.display(),
                "sha256": temporary.content_digest.hex(),
            }),
        }
    }

    fn validate(&self, request: &Value) -> Result<(), String> {
        let mut current = request;
        for segment in &self.field {
            current = match segment {
                JsonPathSegment::Key(key) => current.get(*key).ok_or_else(|| {
                    format!("Tide request token points at missing object key {key:?}")
                })?,
                JsonPathSegment::Index(index) => current.get(*index).ok_or_else(|| {
                    format!("Tide request token points at missing array index {index}")
                })?,
            };
        }
        let expected = match &self.kind {
            RequestTokenKind::RootPath(path) => format!("[ROOT]/{}", path.display()),
            RequestTokenKind::HostInfo => "[HOSTINFO]".to_owned(),
            RequestTokenKind::TideTemp(_) => "[TIDE-TMP]".to_owned(),
        };
        if current.as_str() != Some(&expected) {
            return Err(format!(
                "Tide request token field {:?} expected {expected:?}, got {current}",
                self.field,
            ));
        }
        Ok(())
    }
}

fn reserved_request_token_count(value: &Value) -> usize {
    match value {
        Value::String(value)
            if value == "[HOSTINFO]"
                || value == "[TIDE-TMP]"
                || value == "[TSSERVER]"
                || value.starts_with("[ROOT]/") =>
        {
            1
        }
        Value::Array(values) => values.iter().map(reserved_request_token_count).sum(),
        Value::Object(values) => values.values().map(reserved_request_token_count).sum(),
        _ => 0,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResponseTokenKind {
    RootPath(WorkspaceRelativePath),
    ProjectRoot,
    TsserverPath,
    TsserverBundledPath(TsserverRelativePath),
    EmbeddedRootPath {
        prefix: RecordedLiteral,
        path: WorkspaceRelativePath,
        suffix: RecordedLiteral,
    },
    ProjectId(WorkspaceRelativePath),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordedLiteral(String);

impl RecordedLiteral {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.contains("[ROOT]")
            || value.contains("[TSSERVER]")
            || value.contains("[TSSERVER-DIR]")
            || value.contains("[PROJECT-ID]")
        {
            return Err("Tide recorded literal contains a reserved response token".to_owned());
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResponseToken {
    field: Vec<JsonPathSegment>,
    kind: ResponseTokenKind,
}

impl ResponseToken {
    fn root_path(field: Vec<JsonPathSegment>, relative: WorkspaceRelativePath) -> Self {
        Self {
            field,
            kind: ResponseTokenKind::RootPath(relative),
        }
    }

    fn tsserver(field: Vec<JsonPathSegment>) -> Self {
        Self {
            field,
            kind: ResponseTokenKind::TsserverPath,
        }
    }

    fn tsserver_bundled(field: Vec<JsonPathSegment>, relative: TsserverRelativePath) -> Self {
        Self {
            field,
            kind: ResponseTokenKind::TsserverBundledPath(relative),
        }
    }

    fn project_root(field: Vec<JsonPathSegment>) -> Self {
        Self {
            field,
            kind: ResponseTokenKind::ProjectRoot,
        }
    }

    fn embedded_root_path(
        field: Vec<JsonPathSegment>,
        prefix: RecordedLiteral,
        path: WorkspaceRelativePath,
        suffix: RecordedLiteral,
    ) -> Self {
        Self {
            field,
            kind: ResponseTokenKind::EmbeddedRootPath {
                prefix,
                path,
                suffix,
            },
        }
    }

    fn project_id(field: Vec<JsonPathSegment>, config: WorkspaceRelativePath) -> Self {
        Self {
            field,
            kind: ResponseTokenKind::ProjectId(config),
        }
    }

    fn expected(&self) -> String {
        match &self.kind {
            ResponseTokenKind::RootPath(relative) => {
                format!("[ROOT]/{}", relative.display())
            }
            ResponseTokenKind::ProjectRoot => "[ROOT]".to_owned(),
            ResponseTokenKind::TsserverPath => "[TSSERVER]".to_owned(),
            ResponseTokenKind::TsserverBundledPath(relative) => {
                format!("[TSSERVER-DIR]/{}", relative.display())
            }
            ResponseTokenKind::EmbeddedRootPath {
                prefix,
                path,
                suffix,
            } => format!(
                "{}[ROOT]/{}{}",
                prefix.as_str(),
                path.display(),
                suffix.as_str()
            ),
            ResponseTokenKind::ProjectId(_) => "[PROJECT-ID]".to_owned(),
        }
    }

    fn validate(&self, body: &Value) -> Result<(), String> {
        let mut current = body;
        for segment in &self.field {
            current = match segment {
                JsonPathSegment::Key(key) => current.get(*key).ok_or_else(|| {
                    format!("Tide response token points at missing object key {key:?}")
                })?,
                JsonPathSegment::Index(index) => current.get(*index).ok_or_else(|| {
                    format!("Tide response token points at missing array index {index}")
                })?,
            };
        }
        let expected = self.expected();
        if current.as_str() != Some(&expected) {
            return Err(format!(
                "Tide response token field {:?} expected {:?}, got {current}",
                self.field, expected,
            ));
        }
        Ok(())
    }

    fn json(&self) -> Value {
        let field = self
            .field
            .iter()
            .map(JsonPathSegment::json)
            .collect::<Vec<_>>();
        match &self.kind {
            ResponseTokenKind::RootPath(relative) => serde_json::json!({
                "field": field,
                "kind": "root-path",
                "relative": relative.display(),
            }),
            ResponseTokenKind::ProjectRoot => serde_json::json!({
                "field": field,
                "kind": "project-root",
            }),
            ResponseTokenKind::TsserverPath => serde_json::json!({
                "field": field,
                "kind": "tsserver-path",
            }),
            ResponseTokenKind::TsserverBundledPath(relative) => serde_json::json!({
                "field": field,
                "kind": "tsserver-bundled-path",
                "relative": relative.display(),
            }),
            ResponseTokenKind::EmbeddedRootPath {
                prefix,
                path,
                suffix,
            } => serde_json::json!({
                "field": field,
                "kind": "embedded-root-path",
                "prefix": prefix.as_str(),
                "relative": path.display(),
                "suffix": suffix.as_str(),
            }),
            ResponseTokenKind::ProjectId(config) => serde_json::json!({
                "field": field,
                "kind": "project-id",
                "relative": config.display(),
            }),
        }
    }

    fn owns_reserved_scalar(&self) -> bool {
        !matches!(&self.kind, ResponseTokenKind::EmbeddedRootPath { .. })
    }
}

fn reserved_response_token_count(value: &Value) -> usize {
    match value {
        Value::String(value)
            if value == "[ROOT]"
                || value.starts_with("[ROOT]/")
                || value == "[TSSERVER]"
                || value.starts_with("[TSSERVER-DIR]/")
                || value == "[PROJECT-ID]" =>
        {
            1
        }
        Value::Array(values) => values.iter().map(reserved_response_token_count).sum(),
        Value::Object(values) => values.values().map(reserved_response_token_count).sum(),
        _ => 0,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaptureProjectIdEvidence {
    capture_config_path_base64: String,
    raw_project_id: Sha256Digest,
    raw_telemetry_frame_digest: Sha256Digest,
    raw_project_loading_frame_digest: Sha256Digest,
    config_path: WorkspaceRelativePath,
    telemetry_tokens: Vec<ResponseToken>,
    project_loading_tokens: Vec<ResponseToken>,
    telemetry_field: Vec<JsonPathSegment>,
    project_name_field: Vec<JsonPathSegment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapturedProjectFrames {
    project_loading: ApprovedFrame,
    telemetry: ApprovedFrame,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapturedTsserverBundleFrames(Vec<ApprovedFrame>);

impl CapturedTsserverBundleFrames {
    pub(crate) fn into_frames(self) -> Vec<ApprovedFrame> {
        self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CaptureTsserverBundleEvidence {
    capture_bundle_directory: String,
    capture_root: String,
    capture_stream_digest: Sha256Digest,
    frames: Vec<(usize, ApprovedFrame)>,
    bundled_token_count: usize,
    root_token_count: usize,
    members: BTreeSet<TsserverRelativePath>,
    evidence_stream: Vec<u8>,
}

impl CaptureTsserverBundleEvidence {
    pub(crate) fn new(
        capture_bundle_directory_base64: &str,
        capture_root_base64: &str,
        capture_stream_digest: Sha256Digest,
    ) -> Result<Self, String> {
        let capture_bundle_directory = canonical_capture_absolute_path(
            capture_bundle_directory_base64,
            "tsserver bundle directory",
        )?;
        let capture_root = canonical_capture_absolute_path(capture_root_base64, "project root")?;
        if capture_bundle_directory == capture_root {
            return Err(
                "captured Tide root and tsserver bundle directory must be distinct".to_owned(),
            );
        }
        let required_suffix = "/tide-20260219.336/tsserver";
        if !capture_bundle_directory.ends_with(required_suffix) {
            return Err(
                "captured Tide tsserver bundle directory has the wrong package role".to_owned(),
            );
        }
        if capture_stream_digest != Sha256Digest::parse(PINNED_DIAGNOSTICS_CAPTURE_SHA256)? {
            return Err("captured Tide diagnostics stream digest mismatch".to_owned());
        }
        validate_tsserver_bundle_manifest(PINNED_TSSERVER_BUNDLE)?;
        Ok(Self {
            capture_bundle_directory,
            capture_root,
            capture_stream_digest,
            frames: Vec::with_capacity(426),
            bundled_token_count: 0,
            root_token_count: 0,
            members: BTreeSet::new(),
            evidence_stream: Vec::with_capacity(426 * 174),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn approve_frame(
        &mut self,
        capture_row: usize,
        normalized_bytes: Vec<u8>,
        normalized_digest: Sha256Digest,
        raw_digest: Sha256Digest,
        delivery: DeliveryPlan,
        tokens: Vec<ResponseToken>,
    ) -> Result<(), String> {
        if capture_row == 0
            || self
                .frames
                .last()
                .is_some_and(|(prior_row, _)| *prior_row >= capture_row)
        {
            return Err("captured Tide bundle frame rows are not strictly ordered".to_owned());
        }
        let template = CapturedFrameTemplate::parse(normalized_bytes)?;
        template.validate_normalized_tokens(&tokens)?;
        let mut raw_body = template.body.clone();
        let mut bundled = 0;
        let mut roots = 0;
        let mut frame_members = BTreeSet::new();
        for token in &tokens {
            validate_bundle_capture_token_field(token)?;
            let field = json_field_mut(&mut raw_body, &token.field)
                .ok_or_else(|| "captured Tide bundle token field disappeared".to_owned())?;
            if field.as_str() != Some(token.expected().as_str()) {
                return Err("captured Tide bundle token marker changed".to_owned());
            }
            let expanded = match &token.kind {
                ResponseTokenKind::TsserverBundledPath(relative) => {
                    bundled += 1;
                    frame_members.insert(relative.clone());
                    format!("{}/{}", self.capture_bundle_directory, relative.display())
                }
                ResponseTokenKind::RootPath(relative) => {
                    roots += 1;
                    capture_owned_path(&self.capture_root, relative)
                }
                _ => {
                    return Err(
                        "captured Tide bundle evidence has an unapproved mixed token kind"
                            .to_owned(),
                    );
                }
            };
            *field = Value::String(expanded);
        }
        if bundled == 0 {
            return Err("captured Tide bundle frame has no bundled path token".to_owned());
        }
        let raw = captured_frame_bytes(&raw_body, template.trailing_newline)?;
        if Sha256Digest::of(&raw) != raw_digest {
            return Err("captured Tide bundled raw frame digest mismatch".to_owned());
        }
        let normalized = captured_frame_bytes(&template.body, template.trailing_newline)?;
        if Sha256Digest::of(&normalized) != normalized_digest {
            return Err("captured Tide bundled normalized frame digest mismatch".to_owned());
        }
        let frame = ApprovedFrame::new_bundle_capture_provenance(
            normalized,
            normalized_digest,
            delivery,
            tokens,
        )?;
        self.bundled_token_count += bundled;
        self.root_token_count += roots;
        self.members.extend(frame_members);
        writeln!(
            self.evidence_stream,
            "{{\"normalized_sha256\":\"{}\",\"raw_sha256\":\"{}\",\"row\":{capture_row}}}",
            normalized_digest.hex(),
            raw_digest.hex(),
        )
        .expect("writing to a byte vector cannot fail");
        self.frames.push((capture_row, frame));
        Ok(())
    }

    pub(crate) fn finalize(self) -> Result<CapturedTsserverBundleFrames, String> {
        if self.capture_stream_digest != Sha256Digest::parse(PINNED_DIAGNOSTICS_CAPTURE_SHA256)?
            || self.frames.len() != 426
            || self.bundled_token_count != 736
            || self.root_token_count != 12
            || self.members.len() != 47
            || Sha256Digest::of(&self.evidence_stream)
                != Sha256Digest::parse(PINNED_DIAGNOSTICS_BUNDLE_EVIDENCE_SHA256)?
            || !PINNED_TSSERVER_BUNDLE.iter().all(|(relative, _)| {
                self.members
                    .iter()
                    .any(|member| member.display() == *relative)
            })
        {
            return Err(format!(
                "captured Tide bundle corpus is incomplete: frames={}, bundled={}, root={}, members={}",
                self.frames.len(),
                self.bundled_token_count,
                self.root_token_count,
                self.members.len(),
            ));
        }
        Ok(CapturedTsserverBundleFrames(
            self.frames.into_iter().map(|(_, frame)| frame).collect(),
        ))
    }
}

impl CaptureProjectIdEvidence {
    #[allow(clippy::too_many_arguments)]
    fn new(
        capture_config_path_base64: impl Into<String>,
        raw_project_id: &str,
        raw_telemetry_frame_digest: Sha256Digest,
        raw_project_loading_frame_digest: Sha256Digest,
        config_path: WorkspaceRelativePath,
        telemetry_tokens: Vec<ResponseToken>,
        project_loading_tokens: Vec<ResponseToken>,
        telemetry_field: Vec<JsonPathSegment>,
        project_name_field: Vec<JsonPathSegment>,
    ) -> Result<Self, String> {
        let capture_config_path_base64 = capture_config_path_base64.into();
        let capture_config_path =
            canonical_capture_path(&capture_config_path_base64, &config_path)?;
        if raw_project_id.len() != 64
            || !raw_project_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("captured Tide projectId is not 64 lowercase hexadecimal bytes".to_owned());
        }
        let raw_project_id = Sha256Digest::parse(raw_project_id)?;
        if Sha256Digest::of(capture_config_path.as_bytes()) != raw_project_id {
            return Err(
                "captured Tide projectId does not hash the decoded absolute config path".to_owned(),
            );
        }
        if telemetry_field.is_empty() || project_name_field.is_empty() {
            return Err("captured Tide provenance fields must be nonempty".to_owned());
        }
        let owns_project_id = telemetry_tokens.iter().filter(|token| {
            token.field == telemetry_field
                && token.kind == ResponseTokenKind::ProjectId(config_path.clone())
        });
        if owns_project_id.count() != 1 {
            return Err(
                "captured Tide telemetry plan must own its exact projectId field once".to_owned(),
            );
        }
        let owns_project_name = project_loading_tokens.iter().filter(|token| {
            token.field == project_name_field
                && token.kind == ResponseTokenKind::RootPath(config_path.clone())
        });
        if owns_project_name.count() != 1 {
            return Err(
                "captured Tide loading plan must own its exact projectName field once".to_owned(),
            );
        }
        Ok(Self {
            capture_config_path_base64,
            raw_project_id,
            raw_telemetry_frame_digest,
            raw_project_loading_frame_digest,
            config_path,
            telemetry_tokens,
            project_loading_tokens,
            telemetry_field,
            project_name_field,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ingest(
        self,
        project_loading_template: Vec<u8>,
        expected_project_loading_digest: Sha256Digest,
        project_loading_delivery: DeliveryPlan,
        telemetry_template: Vec<u8>,
        expected_telemetry_digest: Sha256Digest,
        telemetry_delivery: DeliveryPlan,
    ) -> Result<CapturedProjectFrames, String> {
        let capture_config_path =
            canonical_capture_path(&self.capture_config_path_base64, &self.config_path)?;
        let suffix = format!("/{}", self.config_path.display());
        let capture_root = capture_config_path
            .strip_suffix(&suffix)
            .ok_or_else(|| "captured Tide config path lost its typed suffix".to_owned())?;
        let capture_root = if capture_root.is_empty() {
            "/"
        } else {
            capture_root
        };

        let project_loading = CapturedFrameTemplate::parse(project_loading_template)?;
        project_loading.validate_normalized_tokens(&self.project_loading_tokens)?;
        if json_field(&project_loading.body, &self.project_name_field).and_then(Value::as_str)
            != Some(format!("[ROOT]/{}", self.config_path.display()).as_str())
        {
            return Err("captured Tide projectName marker is missing or mismatched".to_owned());
        }
        project_loading.require_raw_digest(
            &self.project_loading_tokens,
            capture_root,
            self.raw_project_id,
            self.raw_project_loading_frame_digest,
        )?;
        let project_loading = project_loading.approve(
            expected_project_loading_digest,
            project_loading_delivery,
            self.project_loading_tokens,
        )?;

        let telemetry = CapturedFrameTemplate::parse(telemetry_template)?;
        telemetry.validate_normalized_tokens(&self.telemetry_tokens)?;
        if json_field(&telemetry.body, &self.telemetry_field).and_then(Value::as_str)
            != Some("[PROJECT-ID]")
        {
            return Err("captured Tide telemetry projectId marker is missing".to_owned());
        }
        if json_field(
            &telemetry.body,
            &[
                JsonPathSegment::Key("body"),
                JsonPathSegment::Key("telemetryEventName"),
            ],
        )
        .and_then(Value::as_str)
            != Some("projectInfo")
        {
            return Err("captured Tide telemetry evidence is not the projectInfo event".to_owned());
        }
        telemetry.require_raw_digest(
            &self.telemetry_tokens,
            capture_root,
            self.raw_project_id,
            self.raw_telemetry_frame_digest,
        )?;
        let telemetry = telemetry.approve(
            expected_telemetry_digest,
            telemetry_delivery,
            self.telemetry_tokens,
        )?;
        validate_captured_project_frame_roles(&project_loading, &telemetry)?;

        Ok(CapturedProjectFrames {
            project_loading,
            telemetry,
        })
    }
}

fn validate_captured_project_frame_roles(
    project_loading: &ApprovedFrame,
    telemetry: &ApprovedFrame,
) -> Result<(), String> {
    if project_loading.kind != TsFrameKind::ProjectLoadingStart {
        return Err("captured Tide project-loading evidence has the wrong frame kind".to_owned());
    }
    if telemetry.kind != TsFrameKind::Telemetry {
        return Err("captured Tide telemetry evidence has the wrong frame kind".to_owned());
    }
    Ok(())
}

fn canonical_capture_path(
    capture_config_path_base64: &str,
    config_path: &WorkspaceRelativePath,
) -> Result<String, String> {
    let bytes = BASE64
        .decode(capture_config_path_base64)
        .map_err(|error| format!("captured Tide config path is not base64: {error}"))?;
    if BASE64.encode(&bytes) != capture_config_path_base64 {
        return Err("captured Tide config path is not canonical base64".to_owned());
    }
    let path = std::str::from_utf8(&bytes)
        .map_err(|error| format!("captured Tide config path is not UTF-8: {error}"))?;
    if path.is_empty() || path.contains(['\0', '\n', '\r']) {
        return Err("captured Tide config path is empty or contains a forbidden byte".to_owned());
    }
    let parsed = Path::new(path);
    if !parsed.is_absolute()
        || parsed
            .components()
            .any(|component| !matches!(component, Component::RootDir | Component::Normal(_)))
    {
        return Err("captured Tide config path is not an absolute normal path".to_owned());
    }
    let normalized = parsed.components().collect::<PathBuf>();
    if normalized.to_str() != Some(path) {
        return Err("captured Tide config path is not lexically canonical".to_owned());
    }
    let suffix = format!("/{}", config_path.display());
    if !path.ends_with(&suffix) {
        return Err("captured Tide config path does not have the typed relative suffix".to_owned());
    }
    Ok(path.to_owned())
}

fn canonical_capture_absolute_path(encoded: &str, role: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(encoded)
        .map_err(|error| format!("captured Tide {role} is not base64: {error}"))?;
    if BASE64.encode(&bytes) != encoded {
        return Err(format!("captured Tide {role} is not canonical base64"));
    }
    let path = std::str::from_utf8(&bytes)
        .map_err(|error| format!("captured Tide {role} is not UTF-8: {error}"))?;
    if path.is_empty() || path.contains(['\0', '\n', '\r']) {
        return Err(format!(
            "captured Tide {role} is empty or contains a forbidden byte"
        ));
    }
    let parsed = Path::new(path);
    if !parsed.is_absolute()
        || parsed
            .components()
            .any(|component| !matches!(component, Component::RootDir | Component::Normal(_)))
        || parsed.components().collect::<PathBuf>().to_str() != Some(path)
    {
        return Err(format!(
            "captured Tide {role} is not an absolute lexically normal path"
        ));
    }
    Ok(path.to_owned())
}

fn key(segment: &JsonPathSegment, expected: &str) -> bool {
    matches!(segment, JsonPathSegment::Key(actual) if *actual == expected)
}

fn validate_bundle_capture_token_field(token: &ResponseToken) -> Result<(), String> {
    let field = token.field.as_slice();
    let approved = match &token.kind {
        ResponseTokenKind::TsserverBundledPath(_) => {
            (field.len() == 3
                && key(&field[0], "body")
                && key(&field[1], "fileNames")
                && matches!(field[2], JsonPathSegment::Index(index) if index < 47))
                || (field.len() == 2 && key(&field[0], "body") && key(&field[1], "file"))
                || (field.len() == 7
                    && key(&field[0], "body")
                    && key(&field[1], "diagnostics")
                    && matches!(field[2], JsonPathSegment::Index(_))
                    && key(&field[3], "relatedInformation")
                    && matches!(field[4], JsonPathSegment::Index(_))
                    && key(&field[5], "span")
                    && key(&field[6], "file"))
        }
        ResponseTokenKind::RootPath(_) => {
            (field.len() == 2 && key(&field[0], "body") && key(&field[1], "configFileName"))
                || (field.len() == 3
                    && key(&field[0], "body")
                    && key(&field[1], "fileNames")
                    && matches!(field[2], JsonPathSegment::Index(index) if (47..=49).contains(&index)))
        }
        _ => false,
    };
    if !approved {
        return Err(format!(
            "captured Tide bundle token uses an unapproved protocol field: {:?}",
            token.field,
        ));
    }
    Ok(())
}

fn json_field<'a>(value: &'a Value, field: &[JsonPathSegment]) -> Option<&'a Value> {
    let Some((head, tail)) = field.split_first() else {
        return Some(value);
    };
    match head {
        JsonPathSegment::Key(key) => json_field(value.get(*key)?, tail),
        JsonPathSegment::Index(index) => json_field(value.get(*index)?, tail),
    }
}

fn json_field_mut<'a>(value: &'a mut Value, field: &[JsonPathSegment]) -> Option<&'a mut Value> {
    let Some((head, tail)) = field.split_first() else {
        return Some(value);
    };
    match head {
        JsonPathSegment::Key(key) => json_field_mut(value.get_mut(*key)?, tail),
        JsonPathSegment::Index(index) => json_field_mut(value.get_mut(*index)?, tail),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedFrameTemplate {
    body: Value,
    trailing_newline: bool,
}

impl CapturedFrameTemplate {
    fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .ok_or_else(|| "captured Tide frame has no header terminator".to_owned())?;
        let header = std::str::from_utf8(&bytes[..header_end])
            .map_err(|error| format!("captured Tide frame header is not UTF-8: {error}"))?;
        let declared = header
            .strip_suffix("\r\n\r\n")
            .and_then(|line| line.strip_prefix("Content-Length: "))
            .filter(|length| !length.is_empty() && length.bytes().all(|byte| byte.is_ascii_digit()))
            .ok_or_else(|| "captured Tide frame has an invalid Content-Length header".to_owned())?
            .parse::<usize>()
            .map_err(|error| format!("captured Tide frame length is invalid: {error}"))?;
        let body_bytes = &bytes[header_end..];
        if declared != body_bytes.len() {
            return Err("captured Tide frame Content-Length mismatch".to_owned());
        }
        let (json_bytes, trailing_newline) = match body_bytes.strip_suffix(b"\n") {
            Some(json_bytes) if !json_bytes.ends_with(b"\r") => (json_bytes, true),
            _ => (body_bytes, false),
        };
        let body: Value = serde_json::from_slice(json_bytes)
            .map_err(|error| format!("captured Tide frame body is invalid JSON: {error}"))?;
        let canonical = serde_json::to_vec(&body)
            .map_err(|error| format!("captured Tide frame body cannot serialize: {error}"))?;
        if canonical != json_bytes {
            return Err(
                "captured Tide frame body is not key-order-preserving canonical JSON".to_owned(),
            );
        }
        Ok(Self {
            body,
            trailing_newline,
        })
    }

    fn validate_normalized_tokens(&self, tokens: &[ResponseToken]) -> Result<(), String> {
        if tokens.iter().enumerate().any(|(index, token)| {
            tokens[..index].iter().any(|prior| {
                prior.field == token.field
                    || token.field.starts_with(&prior.field)
                    || prior.field.starts_with(&token.field)
            })
        }) {
            return Err(
                "captured Tide frame has duplicate or overlapping response token fields".to_owned(),
            );
        }
        for token in tokens {
            token.validate(&self.body)?;
        }
        if reserved_response_token_count(&self.body)
            != tokens
                .iter()
                .filter(|token| token.owns_reserved_scalar())
                .count()
        {
            return Err(
                "captured Tide frame token plan does not own every reserved response token"
                    .to_owned(),
            );
        }
        Ok(())
    }

    fn require_raw_digest(
        &self,
        tokens: &[ResponseToken],
        capture_root: &str,
        raw_project_id: Sha256Digest,
        expected_digest: Sha256Digest,
    ) -> Result<(), String> {
        let mut raw_body = self.body.clone();
        for token in tokens {
            let field = json_field_mut(&mut raw_body, &token.field)
                .ok_or_else(|| "captured Tide raw token field disappeared".to_owned())?;
            if field.as_str() != Some(token.expected().as_str()) {
                return Err("captured Tide raw token field changed before expansion".to_owned());
            }
            let expanded = match &token.kind {
                ResponseTokenKind::RootPath(relative) => capture_owned_path(capture_root, relative),
                ResponseTokenKind::ProjectRoot => capture_root.to_owned(),
                ResponseTokenKind::TsserverPath => {
                    return Err(
                        "captured Tide project provenance cannot self-certify a tsserver path"
                            .to_owned(),
                    );
                }
                ResponseTokenKind::TsserverBundledPath(_) => {
                    return Err(
                        "captured Tide project provenance cannot self-certify a bundled tsserver path"
                            .to_owned(),
                    );
                }
                ResponseTokenKind::EmbeddedRootPath {
                    prefix,
                    path,
                    suffix,
                } => format!(
                    "{}{}{}",
                    prefix.as_str(),
                    capture_owned_path(capture_root, path),
                    suffix.as_str(),
                ),
                ResponseTokenKind::ProjectId(_) => raw_project_id.hex(),
            };
            *field = Value::String(expanded);
        }
        let raw = captured_frame_bytes(&raw_body, self.trailing_newline)?;
        if Sha256Digest::of(&raw) != expected_digest {
            return Err("captured Tide raw frame digest mismatch".to_owned());
        }
        Ok(())
    }

    fn approve(
        self,
        expected_digest: Sha256Digest,
        delivery: DeliveryPlan,
        tokens: Vec<ResponseToken>,
    ) -> Result<ApprovedFrame, String> {
        let normalized = captured_frame_bytes(&self.body, self.trailing_newline)?;
        if Sha256Digest::of(&normalized) != expected_digest {
            return Err("captured Tide normalized frame digest mismatch".to_owned());
        }
        if tokens
            .iter()
            .any(|token| matches!(token.kind, ResponseTokenKind::ProjectId(_)))
        {
            ApprovedFrame::new_capture_provenance(normalized, expected_digest, delivery, tokens)
        } else {
            ApprovedFrame::new(normalized, expected_digest, delivery, tokens)
        }
    }
}

fn capture_owned_path(capture_root: &str, relative: &WorkspaceRelativePath) -> String {
    if capture_root == "/" {
        format!("/{}", relative.display())
    } else {
        format!("{capture_root}/{}", relative.display())
    }
}

fn captured_frame_bytes(body: &Value, trailing_newline: bool) -> Result<Vec<u8>, String> {
    let mut body = serde_json::to_vec(body)
        .map_err(|error| format!("captured Tide frame body cannot serialize: {error}"))?;
    if trailing_newline {
        body.push(b'\n');
    }
    let mut frame = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    frame.extend_from_slice(&body);
    Ok(frame)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallbackPolicy {
    NotRegistered,
    Registered,
}

impl CallbackPolicy {
    const fn symbol(self) -> &'static str {
        match self {
            Self::NotRegistered => "not-registered",
            Self::Registered => "registered",
        }
    }
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                write!(output, "\\u{:04x}", character as u32)
                    .expect("writing to a String cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn json_path(path: &WorkspaceRelativePath) -> String {
    json_string(&format!("[ROOT]/{}", path.display()))
}

fn point_arguments(request: &PointRequest) -> String {
    format!(
        "{{\"file\":{},\"line\":{},\"offset\":{}}}",
        json_path(&request.file),
        request.point.line,
        request.point.offset,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryPlan {
    WholeFrame,
    SplitHeader { at: NonZeroUsize },
    SplitBody { at: NonZeroUsize },
    CoalescedWithNext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplayTermination {
    CleanEof,
    ClientKilled {
        ready_after: RequestOrdinal,
    },
    ExitAfter {
        request: RequestOrdinal,
        code: NonZeroI32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub(crate) fn parse(hex: &str) -> Result<Self, String> {
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("invalid SHA-256 digest: {hex:?}"));
        }
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|error| format!("invalid SHA-256 digest {hex:?}: {error}"))?;
        }
        Ok(Self(bytes))
    }

    fn hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
        }
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApprovedFrame {
    kind: TsFrameKind,
    owner: FrameOwner,
    frame_digest: Sha256Digest,
    bytes: Vec<u8>,
    header_bytes: NonZeroUsize,
    body_bytes: NonZeroUsize,
    delivery: DeliveryPlan,
    tokens: Vec<ResponseToken>,
    capture_provenance: FrameCaptureProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameCaptureProvenance {
    Ordinary,
    ProjectId,
    TsserverBundle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum FrameOwner {
    Response {
        request: RequestOrdinal,
        command: String,
    },
    RequestCompleted(RequestOrdinal),
    Asynchronous,
}

impl FrameOwner {
    fn json(&self) -> Value {
        match self {
            Self::Response { request, command } => serde_json::json!({
                "kind": "response",
                "request": request.get(),
                "command": command,
            }),
            Self::RequestCompleted(request) => serde_json::json!({
                "kind": "request-completed",
                "request": request.get(),
            }),
            Self::Asynchronous => serde_json::json!({ "kind": "asynchronous" }),
        }
    }

    fn elisp(&self) -> String {
        match self {
            Self::Response { request, command } => {
                format!("(:response {} {})", request.get(), json_string(command),)
            }
            Self::RequestCompleted(request) => {
                format!("(:request-completed {})", request.get())
            }
            Self::Asynchronous => "asynchronous".to_owned(),
        }
    }
}

impl ApprovedFrame {
    fn validate_capture_provenance(&self) -> Result<(), String> {
        let has_project_id = self
            .tokens
            .iter()
            .any(|token| matches!(token.kind, ResponseTokenKind::ProjectId(_)));
        let has_bundle = self
            .tokens
            .iter()
            .any(|token| matches!(token.kind, ResponseTokenKind::TsserverBundledPath(_)));
        let expected = if has_project_id {
            FrameCaptureProvenance::ProjectId
        } else if has_bundle {
            FrameCaptureProvenance::TsserverBundle
        } else {
            FrameCaptureProvenance::Ordinary
        };
        if self.capture_provenance != expected {
            return Err("Tide frame lost its private capture-provenance seal".to_owned());
        }
        Ok(())
    }

    pub(crate) fn new(
        bytes: Vec<u8>,
        expected_digest: Sha256Digest,
        delivery: DeliveryPlan,
        tokens: Vec<ResponseToken>,
    ) -> Result<Self, String> {
        if tokens.iter().any(|token| {
            matches!(
                token.kind,
                ResponseTokenKind::ProjectId(_) | ResponseTokenKind::TsserverBundledPath(_)
            )
        }) {
            return Err(
                "a Tide ProjectId or bundled-server frame requires raw capture provenance"
                    .to_owned(),
            );
        }
        Self::new_inner(
            bytes,
            expected_digest,
            delivery,
            tokens,
            FrameCaptureProvenance::Ordinary,
        )
    }

    fn new_capture_provenance(
        bytes: Vec<u8>,
        expected_digest: Sha256Digest,
        delivery: DeliveryPlan,
        tokens: Vec<ResponseToken>,
    ) -> Result<Self, String> {
        if tokens
            .iter()
            .filter(|token| matches!(token.kind, ResponseTokenKind::ProjectId(_)))
            .count()
            != 1
        {
            return Err(
                "captured Tide telemetry needs exactly one proven ProjectId token".to_owned(),
            );
        }
        if tokens
            .iter()
            .any(|token| matches!(token.kind, ResponseTokenKind::TsserverBundledPath(_)))
        {
            return Err("ProjectId evidence cannot approve a bundled-server token".to_owned());
        }
        Self::new_inner(
            bytes,
            expected_digest,
            delivery,
            tokens,
            FrameCaptureProvenance::ProjectId,
        )
    }

    fn new_bundle_capture_provenance(
        bytes: Vec<u8>,
        expected_digest: Sha256Digest,
        delivery: DeliveryPlan,
        tokens: Vec<ResponseToken>,
    ) -> Result<Self, String> {
        if !tokens
            .iter()
            .any(|token| matches!(token.kind, ResponseTokenKind::TsserverBundledPath(_)))
            || tokens.iter().any(|token| {
                !matches!(
                    token.kind,
                    ResponseTokenKind::TsserverBundledPath(_) | ResponseTokenKind::RootPath(_)
                )
            })
        {
            return Err(
                "bundled-server evidence needs bundled tokens and only approved mixed root tokens"
                    .to_owned(),
            );
        }
        Self::new_inner(
            bytes,
            expected_digest,
            delivery,
            tokens,
            FrameCaptureProvenance::TsserverBundle,
        )
    }

    fn new_inner(
        bytes: Vec<u8>,
        expected_digest: Sha256Digest,
        delivery: DeliveryPlan,
        tokens: Vec<ResponseToken>,
        capture_provenance: FrameCaptureProvenance,
    ) -> Result<Self, String> {
        let actual_digest = Sha256Digest::of(&bytes);
        if actual_digest != expected_digest {
            return Err(format!(
                "recorded Tide frame digest mismatch: expected {}, got {}",
                expected_digest.hex(),
                actual_digest.hex(),
            ));
        }
        let header_end = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .ok_or_else(|| "recorded Tide frame has no header terminator".to_owned())?;
        let header = std::str::from_utf8(&bytes[..header_end])
            .map_err(|error| format!("recorded Tide header is not UTF-8: {error}"))?;
        let header_line = header
            .strip_suffix("\r\n\r\n")
            .ok_or_else(|| "recorded Tide header has no exact CRLF terminator".to_owned())?;
        if !header_line.starts_with("Content-Length: ")
            || header_line.matches("Content-Length: ").count() != 1
            || header_line.contains(['\r', '\n'])
        {
            return Err(format!(
                "recorded Tide frame must have one exact Content-Length header: {header:?}"
            ));
        }
        let declared_body_bytes = header
            .split("\r\n")
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .ok_or_else(|| "recorded Tide frame has no Content-Length".to_owned())?
            .parse::<usize>()
            .map_err(|error| format!("recorded Tide Content-Length is invalid: {error}"))?;
        let actual_body_bytes = bytes.len() - header_end;
        if actual_body_bytes != declared_body_bytes {
            return Err(format!(
                "recorded Tide body length mismatch: declared {declared_body_bytes}, got {actual_body_bytes}"
            ));
        }
        let body: Value = serde_json::from_slice(&bytes[header_end..])
            .map_err(|error| format!("recorded Tide body is not valid JSON: {error}"))?;
        if tokens.iter().enumerate().any(|(index, token)| {
            tokens[..index].iter().any(|prior| {
                prior.field == token.field
                    || token.field.starts_with(&prior.field)
                    || prior.field.starts_with(&token.field)
            })
        }) {
            return Err(
                "recorded Tide frame has duplicate or overlapping response token fields".into(),
            );
        }
        for token in &tokens {
            token.validate(&body)?;
        }
        if reserved_response_token_count(&body)
            != tokens
                .iter()
                .filter(|token| token.owns_reserved_scalar())
                .count()
        {
            return Err(
                "recorded Tide frame token plan does not own every reserved response token".into(),
            );
        }
        let (kind, owner) = parse_frame_identity(&body)?;
        let header_bytes = NonZeroUsize::new(header_end)
            .ok_or_else(|| "a recorded Tide frame needs a nonempty header".to_owned())?;
        let body_bytes = NonZeroUsize::new(actual_body_bytes)
            .ok_or_else(|| "a recorded Tide frame needs a nonempty body".to_owned())?;
        header_bytes
            .get()
            .checked_add(body_bytes.get())
            .ok_or_else(|| "a recorded Tide frame length overflowed usize".to_owned())?;
        let frame = Self {
            kind,
            owner,
            frame_digest: actual_digest,
            bytes,
            header_bytes,
            body_bytes,
            delivery,
            tokens,
            capture_provenance,
        };
        match delivery {
            DeliveryPlan::SplitHeader { at } if at >= frame.header_bytes => Err(format!(
                "header split {} is outside a {}-byte recorded header",
                at, frame.header_bytes
            )),
            DeliveryPlan::SplitBody { at } if at >= frame.body_bytes => Err(format!(
                "body split {} is outside a {}-byte recorded body",
                at, frame.body_bytes
            )),
            _ => Ok(frame),
        }
    }

    const fn total_bytes(&self) -> usize {
        self.header_bytes.get() + self.body_bytes.get()
    }
}

fn parse_protocol_ordinal(value: &Value, field: &str) -> Result<RequestOrdinal, String> {
    let value = match value {
        Value::String(value) => value
            .parse::<usize>()
            .map_err(|error| format!("recorded Tide {field} is not an ordinal: {error}"))?,
        Value::Number(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("recorded Tide {field} is not a usize ordinal"))?,
        _ => return Err(format!("recorded Tide {field} is not a string or number")),
    };
    RequestOrdinal::new(value)
}

fn parse_frame_identity(body: &Value) -> Result<(TsFrameKind, FrameOwner), String> {
    let object = body
        .as_object()
        .ok_or_else(|| "recorded Tide frame body is not a JSON object".to_owned())?;
    if object.get("seq").and_then(Value::as_u64) != Some(0) {
        return Err("recorded Tide server frame must have numeric seq 0".to_owned());
    }
    match object.get("type").and_then(Value::as_str) {
        Some("response") => {
            let command = object
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| "recorded Tide response has no string command".to_owned())?;
            let request = parse_protocol_ordinal(
                object
                    .get("request_seq")
                    .ok_or_else(|| "recorded Tide response has no request_seq".to_owned())?,
                "response request_seq",
            )?;
            Ok((
                TsFrameKind::Response,
                FrameOwner::Response {
                    request,
                    command: command.to_owned(),
                },
            ))
        }
        Some("event") => {
            let event = object
                .get("event")
                .and_then(Value::as_str)
                .ok_or_else(|| "recorded Tide event has no string event name".to_owned())?;
            let (kind, owner) = match event {
                "syntaxDiag" => (
                    TsFrameKind::DiagnosticEvent(DiagnosticEventKind::Syntax),
                    FrameOwner::Asynchronous,
                ),
                "semanticDiag" => (
                    TsFrameKind::DiagnosticEvent(DiagnosticEventKind::Semantic),
                    FrameOwner::Asynchronous,
                ),
                "suggestionDiag" => (
                    TsFrameKind::DiagnosticEvent(DiagnosticEventKind::Suggestion),
                    FrameOwner::Asynchronous,
                ),
                "projectLoadingStart" => {
                    (TsFrameKind::ProjectLoadingStart, FrameOwner::Asynchronous)
                }
                "projectLoadingFinish" => {
                    (TsFrameKind::ProjectLoadingFinish, FrameOwner::Asynchronous)
                }
                "configFileDiag" => (TsFrameKind::ConfigFileDiagnostic, FrameOwner::Asynchronous),
                "telemetry" => (TsFrameKind::Telemetry, FrameOwner::Asynchronous),
                "requestCompleted" => {
                    let request = parse_protocol_ordinal(
                        object
                            .get("body")
                            .and_then(Value::as_object)
                            .and_then(|body| body.get("request_seq"))
                            .ok_or_else(|| {
                                "recorded Tide requestCompleted has no body.request_seq".to_owned()
                            })?,
                        "requestCompleted request_seq",
                    )?;
                    (
                        TsFrameKind::RequestCompleted,
                        FrameOwner::RequestCompleted(request),
                    )
                }
                other => return Err(format!("unmodelled recorded Tide event kind: {other:?}")),
            };
            Ok((kind, owner))
        }
        other => Err(format!("recorded Tide frame has invalid type: {other:?}")),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticEventKind {
    Syntax,
    Semantic,
    Suggestion,
}

impl DiagnosticEventKind {
    const fn protocol_name(self) -> &'static str {
        match self {
            Self::Syntax => "syntaxDiag",
            Self::Semantic => "semanticDiag",
            Self::Suggestion => "suggestionDiag",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TsFrameKind {
    Response,
    DiagnosticEvent(DiagnosticEventKind),
    ProjectLoadingStart,
    ProjectLoadingFinish,
    ConfigFileDiagnostic,
    Telemetry,
    RequestCompleted,
}

impl TsFrameKind {
    const fn symbol(self) -> &'static str {
        match self {
            Self::Response => "response",
            Self::DiagnosticEvent(kind) => kind.protocol_name(),
            Self::ProjectLoadingStart => "project-loading-start",
            Self::ProjectLoadingFinish => "project-loading-finish",
            Self::ConfigFileDiagnostic => "config-file-diagnostic",
            Self::Telemetry => "telemetry",
            Self::RequestCompleted => "request-completed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ApprovedOutput {
    NoFrames,
    Frames {
        frames: Vec<ApprovedFrame>,
        delivery_after: RequestOrdinal,
    },
}

impl ApprovedOutput {
    pub(crate) const fn no_frames() -> Self {
        Self::NoFrames
    }

    pub(crate) fn frames(
        owner: RequestOrdinal,
        frames: Vec<ApprovedFrame>,
    ) -> Result<Self, String> {
        Self::new_frames(owner, frames)
    }

    pub(crate) fn frames_delayed(
        delivery_after: RequestOrdinal,
        frames: Vec<ApprovedFrame>,
    ) -> Result<Self, String> {
        Self::new_frames(delivery_after, frames)
    }

    fn new_frames(
        delivery_after: RequestOrdinal,
        frames: Vec<ApprovedFrame>,
    ) -> Result<Self, String> {
        if frames.is_empty() {
            return Err("use ApprovedOutput::no_frames for a request with no output".to_owned());
        }
        for (index, frame) in frames.iter().enumerate() {
            if frame.delivery != DeliveryPlan::CoalescedWithNext {
                continue;
            }
            if let Some(tail) = frames.get(index + 1)
                && tail.delivery != DeliveryPlan::WholeFrame
            {
                return Err(
                    "a Tide coalesced tail must be one complete frame with no ignored delivery plan"
                        .into(),
                );
            }
        }
        Ok(Self::Frames {
            frames,
            delivery_after,
        })
    }

    fn frames_slice(&self) -> &[ApprovedFrame] {
        match self {
            Self::NoFrames => &[],
            Self::Frames { frames, .. } => frames,
        }
    }

    const fn delivery_after(&self) -> Option<RequestOrdinal> {
        match self {
            Self::NoFrames => None,
            Self::Frames { delivery_after, .. } => Some(*delivery_after),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordedExchange {
    ordinal: RequestOrdinal,
    request: TsRequest,
    callback_policy: CallbackPolicy,
    fixture_generation: FixtureGeneration,
    output: ApprovedOutput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerminalExchange {
    ordinal: RequestOrdinal,
    request: TsRequest,
    callback_policy: CallbackPolicy,
    fixture_generation: FixtureGeneration,
    approved_prefix: ApprovedOutput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReplayExchange {
    Healthy(RecordedExchange),
    TerminalFailure(TerminalExchange),
}

impl ReplayExchange {
    const fn ordinal(&self) -> RequestOrdinal {
        match self {
            Self::Healthy(exchange) => exchange.ordinal,
            Self::TerminalFailure(exchange) => exchange.ordinal,
        }
    }

    const fn request(&self) -> &TsRequest {
        match self {
            Self::Healthy(exchange) => &exchange.request,
            Self::TerminalFailure(exchange) => &exchange.request,
        }
    }

    const fn fixture_generation(&self) -> &FixtureGeneration {
        match self {
            Self::Healthy(exchange) => &exchange.fixture_generation,
            Self::TerminalFailure(exchange) => &exchange.fixture_generation,
        }
    }

    const fn output(&self) -> &ApprovedOutput {
        match self {
            Self::Healthy(exchange) => &exchange.output,
            Self::TerminalFailure(exchange) => &exchange.approved_prefix,
        }
    }

    const fn callback_policy(&self) -> CallbackPolicy {
        match self {
            Self::Healthy(exchange) => exchange.callback_policy,
            Self::TerminalFailure(exchange) => exchange.callback_policy,
        }
    }

    const fn outcome_symbol(&self) -> &'static str {
        match self {
            Self::Healthy(_) => "complete",
            Self::TerminalFailure(_) => "external-exit-before-completion",
        }
    }
}

impl From<RecordedExchange> for ReplayExchange {
    fn from(exchange: RecordedExchange) -> Self {
        Self::Healthy(exchange)
    }
}

impl From<TerminalExchange> for ReplayExchange {
    fn from(exchange: TerminalExchange) -> Self {
        Self::TerminalFailure(exchange)
    }
}

fn validate_output_owners(
    ordinal: RequestOrdinal,
    request: &TsRequest,
    output: &ApprovedOutput,
) -> Result<(usize, usize), String> {
    let mut response_count = 0_usize;
    let mut request_completed_count = 0_usize;
    for frame in output.frames_slice() {
        match &frame.owner {
            FrameOwner::Response {
                request: owner,
                command,
            } => {
                response_count += 1;
                if *owner != ordinal || command != request.command() {
                    return Err(format!(
                        "Tide response belongs to request {} {command:?}, not {} {:?}",
                        owner.get(),
                        ordinal.get(),
                        request.command(),
                    ));
                }
            }
            FrameOwner::RequestCompleted(owner) => {
                request_completed_count += 1;
                if *owner != ordinal {
                    return Err(format!(
                        "Tide requestCompleted belongs to request {}, not {}",
                        owner.get(),
                        ordinal.get(),
                    ));
                }
            }
            FrameOwner::Asynchronous => {}
        }
    }
    Ok((response_count, request_completed_count))
}

impl RecordedExchange {
    pub(crate) fn new(
        ordinal: RequestOrdinal,
        request: TsRequest,
        fixture_generation: FixtureGeneration,
        output: ApprovedOutput,
    ) -> Result<Self, String> {
        if output
            .delivery_after()
            .is_some_and(|boundary| boundary != ordinal)
        {
            return Err(format!(
                "Tide immediate output {} cannot use delayed boundary {}",
                ordinal.get(),
                output.delivery_after().unwrap().get(),
            ));
        }
        Self::new_validated(ordinal, request, fixture_generation, output)
    }

    pub(crate) fn new_delayed(
        ordinal: RequestOrdinal,
        request: TsRequest,
        fixture_generation: FixtureGeneration,
        output: ApprovedOutput,
    ) -> Result<Self, String> {
        let Some(boundary) = output.delivery_after() else {
            return Err("a deliberately delayed Tide exchange needs nonempty output".into());
        };
        if boundary <= ordinal {
            return Err(format!(
                "Tide delayed output {} needs a later boundary, got {}",
                ordinal.get(),
                boundary.get(),
            ));
        }
        Self::new_validated(ordinal, request, fixture_generation, output)
    }

    fn new_validated(
        ordinal: RequestOrdinal,
        request: TsRequest,
        fixture_generation: FixtureGeneration,
        output: ApprovedOutput,
    ) -> Result<Self, String> {
        let (response_count, request_completed_count) =
            validate_output_owners(ordinal, &request, &output)?;
        let expected_responses = request.wire_response_count();
        if response_count != expected_responses {
            return Err(format!(
                "Tide request {} {:?} needs {expected_responses} command responses, got {response_count}",
                ordinal.get(),
                request.command(),
            ));
        }
        let expected_completions = usize::from(request.requires_request_completed());
        if request_completed_count != expected_completions {
            return Err(format!(
                "Tide request {} {:?} needs {expected_completions} requestCompleted events, got {request_completed_count}",
                ordinal.get(),
                request.command(),
            ));
        }
        if request.requires_request_completed()
            && !matches!(
                output.frames_slice().last().map(|frame| &frame.owner),
                Some(FrameOwner::RequestCompleted(owner)) if *owner == ordinal
            )
        {
            return Err("Tide geterrForProject must end with its matching requestCompleted".into());
        }
        Ok(Self {
            ordinal,
            callback_policy: request.callback_policy(),
            request,
            fixture_generation,
            output,
        })
    }
}

impl TerminalExchange {
    pub(crate) fn new(
        ordinal: RequestOrdinal,
        request: TsRequest,
        fixture_generation: FixtureGeneration,
        approved_prefix: ApprovedOutput,
    ) -> Result<Self, String> {
        let (response_count, request_completed_count) =
            validate_output_owners(ordinal, &request, &approved_prefix)?;
        if response_count != 0 || request_completed_count != 0 {
            return Err(format!(
                "Tide terminal failure {} {:?} cannot include its normal completion frame",
                ordinal.get(),
                request.command(),
            ));
        }
        if request.wire_response_count() == 0 && !request.requires_request_completed() {
            return Err(format!(
                "Tide terminal failure {} {:?} has no required healthy completion to interrupt",
                ordinal.get(),
                request.command(),
            ));
        }
        Ok(Self {
            ordinal,
            callback_policy: request.callback_policy(),
            request,
            fixture_generation,
            approved_prefix,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplaySession {
    exchanges: Vec<ReplayExchange>,
    first_ordinal: RequestOrdinal,
    request_stream_digest: Sha256Digest,
    delivery_schedule_digest: Sha256Digest,
    termination: ReplayTermination,
}

impl ReplaySession {
    pub(crate) fn new(
        exchanges: Vec<ReplayExchange>,
        expected_request_stream_digest: Sha256Digest,
        expected_delivery_schedule_digest: Sha256Digest,
        termination: ReplayTermination,
    ) -> Result<Self, String> {
        if exchanges.is_empty() {
            return Err("a Tide replay session has no recorded exchanges".into());
        }
        let first_ordinal = exchanges[0].ordinal();
        let last_ordinal = first_ordinal
            .get()
            .checked_add(exchanges.len() - 1)
            .ok_or_else(|| "Tide request ordinal range overflowed usize".to_owned())?;
        for (index, exchange) in exchanges.iter().enumerate() {
            let expected = first_ordinal
                .get()
                .checked_add(index)
                .ok_or_else(|| "Tide request ordinal range overflowed usize".to_owned())?;
            if exchange.ordinal().get() != expected {
                return Err(format!(
                    "Tide replay session expected request ordinal {expected}, got {}",
                    exchange.ordinal().get()
                ));
            }
            if matches!(exchange, ReplayExchange::TerminalFailure(_)) && expected != last_ordinal {
                return Err(
                    "a Tide terminal-failure exchange must be the sole final exchange".into(),
                );
            }
            if let Some(delivery_after) = exchange.output().delivery_after()
                && (delivery_after < exchange.ordinal() || delivery_after.get() > last_ordinal)
            {
                return Err(format!(
                    "Tide output {} has invalid delivery boundary {} in session ending {}",
                    exchange.ordinal().get(),
                    delivery_after.get(),
                    last_ordinal,
                ));
            }
            let normalized_request = exchange.request().normalized_json(exchange.ordinal());
            let parsed_request: Value =
                serde_json::from_str(&normalized_request).map_err(|error| {
                    format!(
                        "typed Tide request {} is invalid JSON: {error}",
                        exchange.ordinal().get(),
                    )
                })?;
            let tokens = exchange.request().token_plan();
            if tokens.iter().enumerate().any(|(index, token)| {
                tokens[..index]
                    .iter()
                    .any(|prior| prior.field == token.field)
            }) {
                return Err(format!(
                    "typed Tide request {} has duplicate token fields",
                    exchange.ordinal().get(),
                ));
            }
            for token in &tokens {
                token.validate(&parsed_request)?;
            }
            if reserved_request_token_count(&parsed_request) != tokens.len() {
                return Err(format!(
                    "typed Tide request {} token plan does not own every reserved token",
                    exchange.ordinal().get(),
                ));
            }
        }
        let scheduled = exchanges
            .iter()
            .filter_map(|exchange| {
                exchange.output().delivery_after().map(|delivery_after| {
                    (
                        exchange.ordinal(),
                        delivery_after,
                        exchange.output().frames_slice(),
                    )
                })
            })
            .collect::<Vec<_>>();
        if scheduled.windows(2).any(|pair| pair[0].1 > pair[1].1) {
            return Err(
                "Tide output delivery boundaries must be nondecreasing in owner order".into(),
            );
        }
        let scheduled_frames = scheduled
            .iter()
            .flat_map(|(_, boundary, frames)| frames.iter().map(move |frame| (*boundary, frame)))
            .collect::<Vec<_>>();
        for (index, (boundary, frame)) in scheduled_frames.iter().enumerate() {
            if frame.delivery != DeliveryPlan::CoalescedWithNext {
                continue;
            }
            let Some((tail_boundary, tail)) = scheduled_frames.get(index + 1) else {
                return Err("a Tide coalesced frame has no following frame".into());
            };
            if tail_boundary != boundary {
                return Err(
                    "a Tide frame cannot coalesce across output delivery boundaries".into(),
                );
            }
            if tail.delivery != DeliveryPlan::WholeFrame {
                return Err(
                    "a Tide coalesced tail must be one complete frame with no ignored delivery plan"
                        .into(),
                );
            }
        }
        let mut schedule_bytes = Vec::new();
        for (owner, delivery_after, frames) in scheduled {
            let entry = serde_json::json!({
                "owner": owner.get(),
                "delivery_after": delivery_after.get(),
                "frames": frames.iter().map(|frame| serde_json::json!({
                    "sha256": frame.frame_digest.hex(),
                    "owner": frame.owner.json(),
                    "delivery": frame.delivery.json(),
                })).collect::<Vec<_>>(),
            });
            serde_json::to_writer(&mut schedule_bytes, &entry)
                .map_err(|error| format!("Tide delivery schedule cannot serialize: {error}"))?;
            schedule_bytes.push(b'\n');
        }
        let delivery_schedule_digest = Sha256Digest::of(&schedule_bytes);
        if delivery_schedule_digest != expected_delivery_schedule_digest {
            return Err(format!(
                "Tide delivery-schedule digest mismatch: expected {}, got {}",
                expected_delivery_schedule_digest.hex(),
                delivery_schedule_digest.hex(),
            ));
        }
        match termination {
            ReplayTermination::CleanEof => {
                if exchanges
                    .iter()
                    .any(|exchange| !matches!(exchange, ReplayExchange::Healthy(_)))
                {
                    return Err("a clean Tide replay cannot contain a terminal failure".into());
                }
            }
            ReplayTermination::ClientKilled { ready_after } => {
                if ready_after.get() != last_ordinal
                    || exchanges
                        .iter()
                        .any(|exchange| !matches!(exchange, ReplayExchange::Healthy(_)))
                {
                    return Err(format!(
                        "Tide client-killed READY {} must follow the final healthy exchange {}",
                        ready_after.get(),
                        last_ordinal,
                    ));
                }
            }
            ReplayTermination::ExitAfter { request, .. } => {
                if request.get() != last_ordinal
                    || !matches!(exchanges.last(), Some(ReplayExchange::TerminalFailure(_)))
                {
                    return Err(format!(
                        "Tide external exit {} must own the sole final terminal-failure exchange {}",
                        request.get(),
                        last_ordinal,
                    ));
                }
            }
        }
        let request_stream = exchanges
            .iter()
            .map(|exchange| {
                let mut request = exchange.request().normalized_json(exchange.ordinal());
                request.push('\n');
                request
            })
            .collect::<String>();
        let request_stream_digest = Sha256Digest::of(request_stream.as_bytes());
        if request_stream_digest != expected_request_stream_digest {
            return Err(format!(
                "Tide {:?} request-stream digest mismatch: expected {}, got {}",
                first_ordinal,
                expected_request_stream_digest.hex(),
                request_stream_digest.hex(),
            ));
        }
        Ok(Self {
            exchanges,
            first_ordinal,
            request_stream_digest,
            delivery_schedule_digest,
            termination,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TideReplay {
    scenario: TideScenario,
    fixtures: FixtureManifest,
    sessions: Vec<ReplaySession>,
}

impl TideReplay {
    pub(crate) fn new(
        scenario: TideScenario,
        fixtures: FixtureManifest,
        sessions: Vec<ReplaySession>,
    ) -> Result<Self, String> {
        if sessions.is_empty() {
            return Err(format!("Tide scenario {scenario:?} has no replay sessions"));
        }
        if sessions[0].first_ordinal.get() != 1 {
            return Err(format!(
                "Tide scenario {scenario:?} must begin at owned absolute request 1, got {}",
                sessions[0].first_ordinal.get(),
            ));
        }
        for pair in sessions.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            let expected = left
                .first_ordinal
                .get()
                .checked_add(left.exchanges.len())
                .ok_or_else(|| "Tide case-wide request sequence overflowed usize".to_owned())?;
            if right.first_ordinal.get() != expected {
                return Err(format!(
                    "Tide scenario {scenario:?} expected the next process at absolute request {expected}, got {}",
                    right.first_ordinal.get(),
                ));
            }
        }
        Ok(Self {
            scenario,
            fixtures,
            sessions,
        })
    }

    pub(crate) fn elisp_summary(&self) -> String {
        let session_summaries = self
            .sessions
            .iter()
            .map(|session| {
                let commands = session
                    .exchanges
                    .iter()
                    .map(|exchange| exchange.request().command())
                    .collect::<Vec<_>>()
                    .join(" ");
                let recordings = session
                    .exchanges
                    .iter()
                    .map(|exchange| {
                        let output = if exchange.output().frames_slice().is_empty() {
                            ":output none".to_owned()
                        } else {
                            let frames = exchange
                                .output()
                                .frames_slice()
                                .iter()
                                .map(|frame| {
                                    format!(
                                        "(:kind {} :owner {} :bytes {} :sha256 \"{}\" :delivery {})",
                                        frame.kind.symbol(),
                                        frame.owner.elisp(),
                                        frame.total_bytes(),
                                        frame.frame_digest.hex(),
                                        frame.delivery.elisp(),
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            format!(
                                ":output (:delivery-after {} :frames ({frames}))",
                                exchange.output().delivery_after().unwrap().get(),
                            )
                        };
                        format!(
                            "(:ordinal {} :outcome {} :callback {} {} :json {})",
                            exchange.ordinal().get(),
                            exchange.outcome_symbol(),
                            exchange.callback_policy().symbol(),
                            output,
                            json_string(&exchange.request().normalized_json(exchange.ordinal())),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let termination = termination_elisp(session.termination);
                let frame_count = session
                    .exchanges
                    .iter()
                    .map(|exchange| exchange.output().frames_slice().len())
                    .sum::<usize>();
                format!(
                    "(:first-ordinal {} :requests ({commands}) :request-count {} :frame-count {frame_count} :request-sha256 \"{}\" :recordings ({recordings}) :termination {termination})",
                    session.first_ordinal.get(),
                    session.exchanges.len(),
                    session.request_stream_digest.hex(),
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "'(:scenario {} :fixture-count {} :session-count {} :sessions ({session_summaries}))",
            self.scenario.symbol(),
            self.fixtures.0.len(),
            self.sessions.len(),
        )
    }

    pub(crate) fn artifacts(&self, runtime: &ReplayRuntimeIdentity) -> ReplayArtifacts {
        let mut frame_files = Vec::new();
        let sessions = self
            .sessions
            .iter()
            .enumerate()
            .map(|(session_index, session)| {
                let exchanges = session
                    .exchanges
                    .iter()
                    .map(|exchange| {
                        let frames = exchange
                            .output()
                            .frames_slice()
                            .iter()
                            .enumerate()
                            .map(|(index, frame)| {
                                frame
                                    .validate_capture_provenance()
                                    .expect("typed Tide frame retains its private provenance seal");
                                let path = format!(
                                    ".tide368/frames/{:02}-{:06}-{:03}.frame",
                                    session_index + 1,
                                    exchange.ordinal().get(),
                                    index + 1,
                                );
                                frame_files.push(ReplayArtifactFile {
                                    relative_path: ReplayArtifactPath::Owner(
                                        WorkspaceRelativePath::new(&path)
                                            .expect("typed Tide frame paths are canonical"),
                                    ),
                                    bytes: frame.bytes.clone(),
                                    digest: frame.frame_digest,
                                });
                                serde_json::json!({
                                    "path": path,
                                    "sha256": frame.frame_digest.hex(),
                                    "kind": frame.kind.symbol(),
                                    "owner": frame.owner.json(),
                                    "delivery": frame.delivery.json(),
                                    "tokens": frame.tokens.iter().map(ResponseToken::json).collect::<Vec<_>>(),
                                })
                            })
                            .collect::<Vec<_>>();
                        let tokens = exchange
                            .request()
                            .token_plan()
                            .iter()
                            .map(RequestToken::json)
                            .collect::<Vec<_>>();
                        let fixture_generation = exchange.fixture_generation().json();
                        serde_json::json!({
                            "ordinal": exchange.ordinal().get(),
                            "outcome": exchange.outcome_symbol(),
                            "callback_policy": exchange.callback_policy().symbol(),
                            "request": exchange.request().normalized_json(exchange.ordinal()),
                            "tokens": tokens,
                            "fixture_generation": fixture_generation,
                            "frames": frames,
                            "delivery_after": exchange.output().delivery_after().map(RequestOrdinal::get),
                        })
                    })
                    .collect::<Vec<_>>();
                let frame_count = session
                    .exchanges
                    .iter()
                    .map(|exchange| exchange.output().frames_slice().len())
                    .sum::<usize>();
                serde_json::json!({
                    "session": session_index + 1,
                    "first_ordinal": session.first_ordinal.get(),
                    "request_count": session.exchanges.len(),
                    "frame_count": frame_count,
                    "request_stream_sha256": session.request_stream_digest.hex(),
                    "delivery_schedule_sha256": session.delivery_schedule_digest.hex(),
                    "exchanges": exchanges,
                    "termination": termination_json(session.termination),
                })
            })
            .collect::<Vec<_>>();
        let fixtures = self
            .fixtures
            .0
            .iter()
            .map(|fixture| {
                serde_json::json!({
                    "path": fixture.path.display(),
                    "sha256": fixture.digest.hex(),
                })
            })
            .collect::<Vec<_>>();
        let config = serde_json::json!({
            "scenario": self.scenario.symbol(),
            "session_count": self.sessions.len(),
            "frame_count": frame_files.len(),
            "pinned_tsserver_sha256": PINNED_TSSERVER_SHA256,
            "interpreter": {
                "path": runtime.interpreter.utf8_path().expect("preflighted interpreter path is UTF-8"),
                "sha256": runtime.interpreter.digest.hex(),
            },
            "tsserver": {
                "path": runtime.tsserver.utf8_path().expect("preflighted tsserver path is UTF-8"),
                "sha256": runtime.tsserver.digest.hex(),
            },
            "tsserver_bundle": {
                "manifest_sha256": PINNED_TSSERVER_BUNDLE_MANIFEST_SHA256,
                "files": runtime.tsserver_bundle.iter().map(|(relative, digest)| {
                    serde_json::json!({
                        "path": relative.display(),
                        "sha256": digest.hex(),
                    })
                }).collect::<Vec<_>>(),
            },
            "adapter_body_sha256": Sha256Digest::of(TIDE_REPLAY_ADAPTER.as_bytes()).hex(),
            "environment": {
                "DISPLAY": ":0",
                "HOME": "[OWNER]/.tide368/home",
                "LANG": "C.UTF-8",
                "LC_ALL": "C.UTF-8",
                "LOGNAME": "tide368",
                "TIDE368_ADAPTER": "[OWNER]/.tide368/bin/tide-node",
                "TIDE368_ADAPTER_SHA256": "[ADAPTER-SHA256]",
                "TIDE368_CONFIG": "[OWNER]/.tide368/config.json",
                "TIDE368_CONFIG_SHA256": "[SELF-SHA256]",
                "TIDE368_HOSTINFO": "[RUNTIME-HOSTINFO]",
                "TIDE368_INTERPRETER": runtime.interpreter.utf8_path().expect("preflighted interpreter path is UTF-8"),
                "TIDE368_INTERPRETER_SHA256": runtime.interpreter.digest.hex(),
                "TIDE368_INVOCATION_LEDGER": "[OWNER]/.tide368/invocations",
                "TIDE368_MISS": "[OWNER]/.tide368/miss.jsonl",
                "TIDE368_OWNER": "[OWNER]",
                "TIDE368_ROOT": "[ROOT]",
                "TIDE368_TMP": "[OWNER]/.tide368/tmp",
                "TIDE368_TRACE": "[OWNER]/.tide368/trace.jsonl",
                "TIDE368_TSSERVER": runtime.tsserver.utf8_path().expect("preflighted tsserver path is UTF-8"),
                "TIDE368_TSSERVER_SHA256": PINNED_TSSERVER_SHA256,
                "TMPDIR": "[OWNER]/.tide368/tmp",
                "TZ": "UTC",
                "USER": "tide368",
            },
            "fixtures": fixtures,
            "sessions": sessions,
        });
        let config_bytes = serde_json::to_vec(&config)
            .expect("a typed Tide replay config always serializes as JSON");
        ReplayArtifacts {
            adapter_source: TIDE_REPLAY_ADAPTER.as_bytes().to_vec(),
            config_digest: Sha256Digest::of(&config_bytes),
            config_bytes,
            fixture_files: self
                .fixtures
                .0
                .iter()
                .map(|fixture| ReplayArtifactFile {
                    relative_path: ReplayArtifactPath::Project(fixture.path.clone()),
                    bytes: fixture.bytes.clone(),
                    digest: fixture.digest,
                })
                .collect(),
            frame_files,
        }
    }
}

fn termination_elisp(termination: ReplayTermination) -> String {
    match termination {
        ReplayTermination::CleanEof => "clean-eof".to_owned(),
        ReplayTermination::ClientKilled { ready_after } => {
            format!("(:client-killed :ready-after {})", ready_after.get())
        }
        ReplayTermination::ExitAfter { request, code } => {
            format!("(:exit-after {} :code {})", request.get(), code.get())
        }
    }
}

fn termination_json(termination: ReplayTermination) -> Value {
    match termination {
        ReplayTermination::CleanEof => serde_json::json!({ "kind": "clean-eof" }),
        ReplayTermination::ClientKilled { ready_after } => serde_json::json!({
            "kind": "client-killed",
            "ready_after": ready_after.get(),
        }),
        ReplayTermination::ExitAfter { request, code } => serde_json::json!({
            "kind": "exit-after",
            "request": request.get(),
            "code": code.get(),
        }),
    }
}

impl DeliveryPlan {
    fn elisp(self) -> String {
        match self {
            Self::WholeFrame => "whole-frame".to_owned(),
            Self::SplitHeader { at } => format!("(:split-header {})", at.get()),
            Self::SplitBody { at } => format!("(:split-body {})", at.get()),
            Self::CoalescedWithNext => "coalesced-with-next".to_owned(),
        }
    }

    fn json(self) -> Value {
        match self {
            Self::WholeFrame => serde_json::json!({ "mode": "whole" }),
            Self::SplitHeader { at } => {
                serde_json::json!({ "mode": "split-header", "at": at.get() })
            }
            Self::SplitBody { at } => {
                serde_json::json!({ "mode": "split-body", "at": at.get() })
            }
            Self::CoalescedWithNext => serde_json::json!({ "mode": "coalesced" }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReplayArtifactPath {
    Project(WorkspaceRelativePath),
    Owner(WorkspaceRelativePath),
}

impl ReplayArtifactPath {
    fn display(&self) -> &str {
        match self {
            Self::Project(path) | Self::Owner(path) => path.display(),
        }
    }

    const fn role(&self) -> &'static str {
        match self {
            Self::Project(_) => "project",
            Self::Owner(_) => "owner",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplayArtifactFile {
    relative_path: ReplayArtifactPath,
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReplayArtifacts {
    adapter_source: Vec<u8>,
    config_bytes: Vec<u8>,
    config_digest: Sha256Digest,
    fixture_files: Vec<ReplayArtifactFile>,
    frame_files: Vec<ReplayArtifactFile>,
}

impl ReplayArtifacts {
    pub(crate) fn elisp_plan(&self) -> String {
        fn files(entries: &[ReplayArtifactFile]) -> String {
            entries
                .iter()
                .map(|entry| {
                    format!(
                        "(:role {} :path {} :base64 {} :sha256 \"{}\")",
                        entry.relative_path.role(),
                        json_string(entry.relative_path.display()),
                        json_string(&BASE64.encode(&entry.bytes)),
                        entry.digest.hex(),
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        format!(
            concat!(
                "'(:adapter-base64 {} :adapter-sha256 \"{}\" ",
                ":config-base64 {} :config-sha256 \"{}\" ",
                ":fixtures ({}) :frames ({}))"
            ),
            json_string(&BASE64.encode(&self.adapter_source)),
            Sha256Digest::of(&self.adapter_source).hex(),
            json_string(&BASE64.encode(&self.config_bytes)),
            self.config_digest.hex(),
            files(&self.fixture_files),
            files(&self.frame_files),
        )
    }
}

const TIDE_REPLAY_ADAPTER: &str = r####"import fcntl
import hashlib
import json
import os
import re
import stat
import sys

REJECT_STATUS = 86
PINNED_TSSERVER_SHA256 = "708b584a9937448f5400b09817774823e6ae339000ddeabc0e7766dfa428793a"
PINNED_TSSERVER_BUNDLE_MANIFEST_SHA256 = "f95ede2ee0564044ca2d61127e75fcd4b3af5b260998823c108d3c59add400da"
ENV_KEYS = {
    "DISPLAY", "HOME", "LANG", "LC_ALL", "LOGNAME", "TIDE368_ADAPTER",
    "TIDE368_ADAPTER_SHA256", "TIDE368_CONFIG", "TIDE368_CONFIG_SHA256",
    "TIDE368_HOSTINFO", "TIDE368_INTERPRETER", "TIDE368_INTERPRETER_SHA256",
    "TIDE368_INVOCATION_LEDGER", "TIDE368_MISS", "TIDE368_OWNER",
    "TIDE368_ROOT", "TIDE368_TMP", "TIDE368_TRACE", "TIDE368_TSSERVER",
    "TIDE368_TSSERVER_SHA256", "TMPDIR", "TZ", "USER",
}
SAFE_MISS = None

def digest_bytes(value):
    return hashlib.sha256(value).hexdigest()

def read_bytes(path):
    with open(path, "rb") as stream:
        return stream.read()

def write_all_fd(fd, value):
    while value:
        written = os.write(fd, value)
        if written <= 0:
            raise BrokenPipeError("file descriptor accepted no replay bytes")
        value = value[written:]

def append_json(path, value):
    flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    fd = os.open(path, flags, 0o600)
    try:
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            raise OSError("ledger is not a regular file")
        encoded = (json.dumps(value, ensure_ascii=False, separators=(",", ":"), default=repr) + "\n").encode("utf-8")
        write_all_fd(fd, encoded)
        os.fsync(fd)
    finally:
        os.close(fd)

def reject(phase, detail):
    if SAFE_MISS is not None:
        try:
            append_json(SAFE_MISS, {"phase": phase, "detail": detail})
        except OSError:
            pass
    sys.stderr.write("UNRECORDED tide replay: " + phase + "\n")
    sys.stderr.flush()
    raise SystemExit(REJECT_STATUS)

def canonical_absolute(path, phase):
    if not isinstance(path, str) or not path or not os.path.isabs(path):
        reject(phase, path)
    if os.path.abspath(path) != path or os.path.normpath(path) != path:
        reject(phase + "-spelling", path)
    if os.path.realpath(path) != path:
        reject(phase + "-symlink", path)
    return path

def direct_directory(path, phase):
    path = canonical_absolute(path, phase)
    try:
        metadata = os.lstat(path)
    except OSError as error:
        reject(phase + "-missing", str(error))
    if not stat.S_ISDIR(metadata.st_mode) or os.path.islink(path):
        reject(phase + "-kind", path)
    return path

def direct_regular(path, owner=None, phase="file"):
    path = canonical_absolute(path, phase)
    try:
        metadata = os.lstat(path)
    except OSError as error:
        reject(phase + "-missing", str(error))
    if not stat.S_ISREG(metadata.st_mode) or os.path.islink(path):
        reject(phase + "-kind", path)
    if owner is not None:
        try:
            if os.path.commonpath((owner, path)) != owner:
                reject(phase + "-owner", path)
        except ValueError:
            reject(phase + "-owner", path)
    return path

def direct_output(path, expected, owner, phase):
    if path != expected:
        reject(phase + "-identity", path)
    path = canonical_absolute(path, phase)
    if os.path.lexists(path):
        direct_regular(path, owner, phase)
    return path

def owned_path(root, relative):
    if not isinstance(relative, str) or not relative or "\\" in relative:
        reject("relative-path", repr(relative))
    parts = relative.split("/")
    if any(not part or part in (".", "..") for part in parts):
        reject("relative-path", relative)
    path = os.path.join(root, *parts)
    if os.path.normpath(path) != path:
        reject("relative-path-spelling", relative)
    try:
        if os.path.commonpath((root, path)) != root or path == root:
            reject("relative-path-owner", relative)
    except ValueError:
        reject("relative-path-owner", relative)
    return path

def validate_tsserver_bundle(config, tsserver):
    plan = config.get("tsserver_bundle")
    if (not isinstance(plan, dict)
            or set(plan) != {"manifest_sha256", "files"}
            or plan.get("manifest_sha256") != PINNED_TSSERVER_BUNDLE_MANIFEST_SHA256
            or not isinstance(plan.get("files"), list)
            or len(plan["files"]) != 47):
        reject("tsserver-bundle-plan", plan)
    base = direct_directory(os.path.dirname(tsserver), "tsserver-bundle-directory")
    entries = {}
    manifest = bytearray()
    previous = None
    for entry in plan["files"]:
        if (not isinstance(entry, dict) or set(entry) != {"path", "sha256"}
                or not isinstance(entry.get("path"), str)
                or not isinstance(entry.get("sha256"), str)
                or re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]) is None):
            reject("tsserver-bundle-entry", entry)
        relative = entry["path"]
        if previous is not None and previous >= relative:
            reject("tsserver-bundle-order", relative)
        previous = relative
        path = direct_regular(owned_path(base, relative), base,
                              "tsserver-bundle-member")
        if os.path.dirname(path) != base:
            reject("tsserver-bundle-parent", path)
        if digest_bytes(read_bytes(path)) != entry["sha256"]:
            reject("tsserver-bundle-digest", relative)
        manifest.extend((relative + " " + entry["sha256"] + "\n").encode("utf-8"))
        entries[relative] = entry["sha256"]
    if digest_bytes(bytes(manifest)) != PINNED_TSSERVER_BUNDLE_MANIFEST_SHA256:
        reject("tsserver-bundle-manifest-digest", digest_bytes(bytes(manifest)))
    return base, entries

def tsserver_bundled_path(bundle, relative, phase):
    base, entries = bundle
    if not isinstance(relative, str) or relative not in entries:
        reject(phase + "-relative", relative)
    path = direct_regular(owned_path(base, relative), base, phase)
    if os.path.dirname(path) != base:
        reject(phase + "-parent", path)
    if digest_bytes(read_bytes(path)) != entries[relative]:
        reject(phase + "-digest", relative)
    return path

def require_environment_keys():
    actual = set(os.environ)
    if actual != ENV_KEYS:
        reject("environment-keys", {"missing": sorted(ENV_KEYS - actual), "extra": sorted(actual - ENV_KEYS)})

def load_config(config_path):
    config_bytes = read_bytes(config_path)
    config_digest = digest_bytes(config_bytes)
    if config_digest != os.environ["TIDE368_CONFIG_SHA256"]:
        reject("config-digest", config_path)
    try:
        config = json.loads(config_bytes)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        reject("config-json", str(error))
    if not isinstance(config, dict):
        reject("config-shape", type(config).__name__)
    return config, config_digest

def bootstrap():
    global SAFE_MISS
    require_environment_keys()
    owner = direct_directory(os.environ["TIDE368_OWNER"], "owner")
    control = direct_directory(os.path.join(owner, ".tide368"), "control")
    root = direct_directory(os.environ["TIDE368_ROOT"], "root")
    tmp_root = direct_directory(os.environ["TIDE368_TMP"], "tmp-root")
    home = direct_directory(os.environ["HOME"], "home")
    try:
        if (os.path.commonpath((owner, root)) != owner or root == owner
                or os.path.commonpath((control, root)) == control):
            reject("root-owner", root)
        if tmp_root != os.path.join(control, "tmp") or home != os.path.join(control, "home"):
            reject("control-directory", {"tmp": tmp_root, "home": home})
    except ValueError:
        reject("root-owner", root)
    config_path = direct_regular(os.environ["TIDE368_CONFIG"], control, "config")
    if config_path != os.path.join(control, "config.json"):
        reject("config-identity", config_path)
    trace = direct_output(os.environ["TIDE368_TRACE"], os.path.join(control, "trace.jsonl"), control, "trace")
    miss = direct_output(os.environ["TIDE368_MISS"], os.path.join(control, "miss.jsonl"), control, "miss")
    ledger = direct_regular(os.environ["TIDE368_INVOCATION_LEDGER"], control, "invocation-ledger")
    if ledger != os.path.join(control, "invocations"):
        reject("invocation-ledger-identity", ledger)
    SAFE_MISS = miss
    config, config_digest = load_config(config_path)
    return owner, control, root, tmp_root, trace, ledger, config, config_digest

def exact_environment(config, config_digest, owner, control, root, tmp_root):
    adapter = direct_regular(os.environ["TIDE368_ADAPTER"], control, "adapter")
    adapter_digest = digest_bytes(read_bytes(adapter))
    interpreter_plan = config.get("interpreter")
    tsserver_plan = config.get("tsserver")
    if (not isinstance(interpreter_plan, dict)
            or set(interpreter_plan) != {"path", "sha256"}
            or not isinstance(tsserver_plan, dict)
            or set(tsserver_plan) != {"path", "sha256"}):
        reject("executable-plan", {"interpreter": interpreter_plan, "tsserver": tsserver_plan})
    interpreter = direct_regular(interpreter_plan["path"], None, "interpreter")
    interpreter_digest = digest_bytes(read_bytes(interpreter))
    if interpreter_digest != interpreter_plan["sha256"]:
        reject("interpreter-plan-digest", interpreter)
    tsserver = direct_regular(tsserver_plan["path"], None, "tsserver")
    if digest_bytes(read_bytes(tsserver)) != tsserver_plan["sha256"]:
        reject("tsserver-plan-digest", tsserver)
    host_info = os.environ["TIDE368_HOSTINFO"]
    if not isinstance(host_info, str) or not host_info:
        reject("host-info-plan", host_info)
    templates = config.get("environment")
    if not isinstance(templates, dict) or set(templates) != ENV_KEYS:
        reject("environment-plan", templates)
    replacements = {
        "[OWNER]": owner,
        "[ROOT]": root,
        "[RUNTIME-HOSTINFO]": host_info,
        "[SELF-SHA256]": config_digest,
        "[ADAPTER-SHA256]": adapter_digest,
    }
    expected = {}
    for key, template in templates.items():
        if not isinstance(template, str):
            reject("environment-template", {"key": key, "value": template})
        placeholders = set(re.findall(r"\[[A-Z][A-Z0-9-]*\]", template))
        unknown = placeholders - set(replacements)
        if unknown:
            reject("environment-template-unknown", {"key": key, "tokens": sorted(unknown)})
        value = template
        for token, replacement in replacements.items():
            value = value.replace(token, replacement)
        if any(token in value for token in replacements):
            reject("environment-template-token", {"key": key, "value": value})
        expected[key] = value
    actual = {key: os.environ[key] for key in ENV_KEYS}
    if actual != expected:
        changed = {key: {"expected": expected.get(key), "actual": actual.get(key)} for key in sorted(ENV_KEYS) if expected.get(key) != actual.get(key)}
        reject("environment-values", changed)
    if expected["TIDE368_OWNER"] != owner or expected["TIDE368_ROOT"] != root or expected["TIDE368_TMP"] != tmp_root:
        reject("environment-roots", expected)
    if expected["TMPDIR"] != tmp_root or expected["HOME"] != os.path.join(control, "home"):
        reject("environment-directories", expected)
    return adapter, adapter_digest, interpreter, tsserver, host_info

def validate_executables(config, adapter, adapter_digest, interpreter, tsserver, root):
    if os.getcwd() != root:
        reject("cwd", os.getcwd())
    interpreter = direct_regular(interpreter, None, "interpreter-pre-launch")
    if digest_bytes(read_bytes(interpreter)) != os.environ["TIDE368_INTERPRETER_SHA256"]:
        reject("interpreter-digest", interpreter)
    if canonical_absolute(sys.executable, "sys-executable") != interpreter:
        reject("interpreter-identity", sys.executable)
    if canonical_absolute(sys.argv[0], "argv-zero") != adapter:
        reject("adapter-identity", sys.argv[0])
    adapter_bytes = read_bytes(adapter)
    if digest_bytes(adapter_bytes) != adapter_digest:
        reject("adapter-digest", adapter)
    shebang, separator, body = adapter_bytes.partition(b"\n")
    if not separator or shebang != b"#!" + interpreter.encode("utf-8"):
        reject("adapter-shebang", repr(shebang))
    if digest_bytes(body) != config.get("adapter_body_sha256"):
        reject("adapter-body-digest", adapter)
    tsserver = direct_regular(tsserver, None, "tsserver-pre-launch")
    if config.get("pinned_tsserver_sha256") != PINNED_TSSERVER_SHA256:
        reject("tsserver-config-pin", config.get("pinned_tsserver_sha256"))
    if os.environ["TIDE368_TSSERVER_SHA256"] != PINNED_TSSERVER_SHA256:
        reject("tsserver-environment-pin", os.environ["TIDE368_TSSERVER_SHA256"])
    if digest_bytes(read_bytes(tsserver)) != PINNED_TSSERVER_SHA256:
        reject("tsserver-digest", tsserver)
    if sys.argv[1:] != [tsserver, "--disableAutomaticTypingAcquisition"]:
        reject("argv", sys.argv[1:])
    return validate_tsserver_bundle(config, tsserver)

def validate_config(config, root, owner):
    expected_keys = {"scenario", "session_count", "frame_count", "pinned_tsserver_sha256", "adapter_body_sha256", "interpreter", "tsserver", "tsserver_bundle", "environment", "fixtures", "sessions"}
    if set(config) != expected_keys:
        reject("config-keys", sorted(config))
    fixtures = config.get("fixtures")
    sessions = config.get("sessions")
    if not isinstance(fixtures, list) or not fixtures or not isinstance(sessions, list) or not sessions:
        reject("config-cardinality", {"fixtures": fixtures, "sessions": sessions})
    if config.get("session_count") != len(sessions):
        reject("session-count", config.get("session_count"))
    seen_frames = set()
    total_frames = 0
    expected_first = 1
    for session_index, session in enumerate(sessions, 1):
        if not isinstance(session, dict) or session.get("session") != session_index:
            reject("session-index", session)
        exchanges = session.get("exchanges")
        if not isinstance(exchanges, list) or not exchanges or session.get("request_count") != len(exchanges):
            reject("session-exchanges", session_index)
        if session.get("first_ordinal") != expected_first:
            reject("session-first-ordinal", session)
        session_frames = 0
        prior_boundary = 0
        for local_index, exchange in enumerate(exchanges):
            expected_ordinal = expected_first + local_index
            if not isinstance(exchange, dict) or exchange.get("ordinal") != expected_ordinal:
                reject("exchange-ordinal", exchange)
            if exchange.get("outcome") not in ("complete", "external-exit-before-completion"):
                reject("exchange-outcome", exchange.get("outcome"))
            if exchange.get("callback_policy") not in ("registered", "not-registered"):
                reject("exchange-callback-policy", exchange.get("callback_policy"))
            if not isinstance(exchange.get("request"), str) or not isinstance(exchange.get("tokens"), list):
                reject("exchange-shape", exchange)
            frames = exchange.get("frames")
            if not isinstance(frames, list):
                reject("frames-shape", frames)
            delivery_after = exchange.get("delivery_after")
            if frames:
                if (not isinstance(delivery_after, int)
                        or delivery_after < expected_ordinal
                        or delivery_after > exchanges[-1]["ordinal"]
                        or delivery_after < prior_boundary):
                    reject("delivery-after", exchange)
                prior_boundary = delivery_after
            elif delivery_after is not None:
                reject("delivery-after-empty", exchange)
            for frame in frames:
                if not isinstance(frame, dict) or set(frame) != {"path", "sha256", "kind", "owner", "delivery", "tokens"}:
                    reject("frame-shape", frame)
                if not isinstance(frame["tokens"], list):
                    reject("frame-tokens", frame["tokens"])
                path = direct_regular(owned_path(owner, frame["path"]), owner, "frame")
                if path in seen_frames:
                    reject("duplicate-frame", path)
                seen_frames.add(path)
                if digest_bytes(read_bytes(path)) != frame.get("sha256"):
                    reject("frame-digest-launch", frame.get("path"))
                session_frames += 1
        schedule_entries = []
        for exchange in exchanges:
            if exchange["frames"]:
                schedule_entries.append({
                    "owner": exchange["ordinal"],
                    "delivery_after": exchange["delivery_after"],
                    "frames": [{
                        "sha256": frame["sha256"],
                        "owner": frame["owner"],
                        "delivery": frame["delivery"],
                    } for frame in exchange["frames"]],
                })
        schedule_bytes = b"".join(
            json.dumps(entry, ensure_ascii=False, separators=(",", ":"),
                       sort_keys=False).encode("utf-8") + b"\n"
            for entry in schedule_entries)
        if digest_bytes(schedule_bytes) != session.get("delivery_schedule_sha256"):
            reject("delivery-schedule-digest", session_index)
        scheduled_frames = [
            (exchange["delivery_after"], frame)
            for exchange in exchanges
            for frame in exchange["frames"]
        ]
        for frame_index, (boundary, frame) in enumerate(scheduled_frames):
            if frame.get("delivery") != {"mode": "coalesced"}:
                continue
            if frame_index + 1 >= len(scheduled_frames):
                reject("coalesced-terminal", frame)
            tail_boundary, tail = scheduled_frames[frame_index + 1]
            if tail_boundary != boundary:
                reject("coalesced-cross-boundary", {
                    "boundary": boundary, "tail_boundary": tail_boundary,
                })
            if tail.get("delivery") != {"mode": "whole"}:
                reject("coalesced-tail-delivery", tail.get("delivery"))
        termination = session.get("termination")
        if not isinstance(termination, dict):
            reject("termination-shape", termination)
        kind = termination.get("kind")
        last_ordinal = exchanges[-1]["ordinal"]
        outcomes = [exchange["outcome"] for exchange in exchanges]
        if kind == "clean-eof":
            if set(termination) != {"kind"} or any(outcome != "complete" for outcome in outcomes):
                reject("termination-clean", termination)
        elif kind == "client-killed":
            if (set(termination) != {"kind", "ready_after"}
                    or termination.get("ready_after") != last_ordinal
                    or any(outcome != "complete" for outcome in outcomes)):
                reject("termination-client-killed", termination)
        elif kind == "exit-after":
            if (set(termination) != {"kind", "request", "code"}
                    or termination.get("request") != last_ordinal
                    or not isinstance(termination.get("code"), int)
                    or termination["code"] <= 0
                    or outcomes[-1] != "external-exit-before-completion"
                    or any(outcome != "complete" for outcome in outcomes[:-1])):
                reject("termination-exit", termination)
        else:
            reject("termination-kind", termination)
        if session.get("frame_count") != session_frames:
            reject("session-frame-count", session_index)
        expected_first += len(exchanges)
        total_frames += session_frames
    if config.get("frame_count") != total_frames:
        reject("frame-count", total_frames)
    for fixture in fixtures:
        if not isinstance(fixture, dict) or set(fixture) != {"path", "sha256"}:
            reject("fixture-shape", fixture)
        owned_path(root, fixture["path"])

def read_trace(path):
    raw = read_bytes(path)
    if not raw:
        return []
    if not raw.endswith(b"\n"):
        reject("trace-ending", len(raw))
    records = []
    for index, line in enumerate(raw.splitlines(), 1):
        try:
            record = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            reject("trace-json", {"line": index, "error": str(error)})
        if not isinstance(record, dict):
            reject("trace-shape", record)
        records.append(record)
    return records

def expected_terminal_event(termination):
    kind = termination.get("kind") if isinstance(termination, dict) else None
    return {"clean-eof": "DONE", "client-killed": "READY", "exit-after": "EXPECTED_EXIT"}.get(kind)

def expected_emission(session, root, owner, bundle):
    state = {"frames": 0, "bytes": 0, "sha256": hashlib.sha256()}
    for exchange in session["exchanges"]:
        for frame in exchange["frames"]:
            value = expanded_frame(frame, root, owner, bundle)
            record_emitted(state, value)
    return state

def validate_previous_terminal(config, trace, previous_index, root, owner, bundle):
    session = config["sessions"][previous_index]
    records = read_trace(trace)
    if not records:
        reject("previous-terminal-missing", previous_index + 1)
    actual = records[-1]
    expected_event = expected_terminal_event(session.get("termination"))
    last_ordinal = session["exchanges"][-1]["ordinal"]
    expected = {
        "event": expected_event,
        "session": previous_index + 1,
        "request": last_ordinal,
        "requests": session["request_count"],
        "frames": session["frame_count"],
        "request_sha256": session["request_stream_sha256"],
    }
    terminal_keys = set(expected) | {"bytes", "emitted_sha256"}
    if expected_event == "EXPECTED_EXIT":
        terminal_keys.add("code")
    if set(actual) != terminal_keys:
        reject("previous-terminal-keys", sorted(actual))
    for key, value in expected.items():
        if actual.get(key) != value:
            reject("previous-terminal", {"key": key, "expected": value, "actual": actual.get(key)})
    emitted = expected_emission(session, root, owner, bundle)
    if actual.get("bytes") != emitted["bytes"]:
        reject("previous-terminal-bytes", actual)
    if actual.get("emitted_sha256") != emitted["sha256"].hexdigest():
        reject("previous-terminal-digest", actual)
    if expected_event == "EXPECTED_EXIT" and actual.get("code") != session["termination"].get("code"):
        reject("previous-terminal-code", actual)

def claim_session(config, ledger_path, trace, root, owner, bundle):
    flags = os.O_RDWR
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    fd = os.open(ledger_path, flags)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            reject("invocation-ledger-kind", ledger_path)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            reject("invocation-concurrent", ledger_path)
        os.lseek(fd, 0, os.SEEK_SET)
        raw = os.read(fd, 128)
        if not raw.endswith(b"\n") or not raw[:-1].isdigit() or (len(raw) > 2 and raw.startswith(b"0")):
            reject("invocation-ledger-content", repr(raw))
        index = int(raw[:-1])
        if index >= len(config["sessions"]):
            reject("invocation-overrun", index)
        trace_records = read_trace(trace)
        if index == 0 and trace_records:
            reject("invocation-trace-without-claim", trace_records[-1])
        if index:
            validate_previous_terminal(config, trace, index - 1, root, owner, bundle)
        else:
            validate_initial_fixtures(config, root)
        updated = str(index + 1).encode("ascii") + b"\n"
        os.lseek(fd, 0, os.SEEK_SET)
        os.ftruncate(fd, 0)
        write_all_fd(fd, updated)
        os.fsync(fd)
        return fd, index, config["sessions"][index]
    except BaseException:
        os.close(fd)
        raise

def validate_initial_fixtures(config, root):
    expected = set()
    for fixture in config["fixtures"]:
        path = direct_regular(owned_path(root, fixture["path"]), root, "initial-fixture")
        if digest_bytes(read_bytes(path)) != fixture["sha256"]:
            reject("initial-fixture-digest", fixture["path"])
        if fixture["path"] in expected:
            reject("initial-fixture-duplicate", fixture["path"])
        expected.add(fixture["path"])
    actual = set()
    for directory, directories, files in os.walk(root, followlinks=False):
        for name in directories:
            path = os.path.join(directory, name)
            if os.path.islink(path):
                reject("initial-fixture-directory-symlink", path)
        for name in files:
            path = direct_regular(os.path.join(directory, name), root, "initial-fixture-enumeration")
            actual.add(os.path.relpath(path, root).replace(os.sep, "/"))
    if actual != expected:
        reject("initial-fixture-set", {"expected": sorted(expected), "actual": sorted(actual)})

def validate_generation(exchange, root):
    generation = exchange.get("fixture_generation")
    if isinstance(generation, list) and generation:
        alternatives = [generation]
    elif (isinstance(generation, dict) and set(generation) == {"one_of"}
          and isinstance(generation.get("one_of"), list)
          and len(generation["one_of"]) >= 2
          and all(isinstance(value, list) and value
                  for value in generation["one_of"])):
        alternatives = generation["one_of"]
    else:
        reject("fixture-generation", generation)
    expected = []
    expected_paths = None
    for alternative in alternatives:
        expected_present = {}
        expected_missing = set()
        for fixture in alternative:
            if not isinstance(fixture, dict):
                reject("fixture-generation-shape", fixture)
            relative = fixture.get("path")
            owned_path(root, relative)
            state = fixture.get("state")
            if relative in expected_present or relative in expected_missing:
                reject("fixture-generation-duplicate", relative)
            if state == "present" and set(fixture) == {"path", "state", "sha256"}:
                if re.fullmatch(r"[0-9a-f]{64}", str(fixture.get("sha256"))) is None:
                    reject("fixture-generation-digest-shape", fixture)
                expected_present[relative] = fixture["sha256"]
            elif state == "missing" and set(fixture) == {"path", "state"}:
                expected_missing.add(relative)
            else:
                reject("fixture-generation-state", fixture)
        paths = set(expected_present) | expected_missing
        if expected_paths is None:
            expected_paths = paths
        elif paths != expected_paths:
            reject("fixture-generation-alternative-paths", sorted(paths))
        expected.append((expected_present, expected_missing))

    actual = None
    for _ in range(64):
        candidate = {}
        unstable = False
        for directory, directories, files in os.walk(root, followlinks=False):
            for name in directories:
                path = os.path.join(directory, name)
                if os.path.islink(path):
                    reject("fixture-directory-symlink", path)
            for name in files:
                path = os.path.join(directory, name)
                try:
                    metadata = os.lstat(path)
                    if not stat.S_ISREG(metadata.st_mode) or os.path.islink(path):
                        reject("fixture-enumeration-kind", path)
                    relative = os.path.relpath(path, root).replace(os.sep, "/")
                    candidate[relative] = digest_bytes(read_bytes(path))
                except FileNotFoundError:
                    unstable = True
                    break
            if unstable:
                break
        if not unstable:
            actual = candidate
            break
    if actual is None:
        reject("fixture-generation-unstable", sorted(expected_paths))
    if not any(
        set(actual) == set(present)
        and all(actual[path] == value for path, value in present.items())
        and all(path not in actual for path in missing)
        for present, missing in expected
    ):
        reject("fixture-generation-set", {
            "expected": [
                {"present": present, "missing": sorted(missing)}
                for present, missing in expected
            ],
            "actual": actual,
        })

def json_field(container, field, replacement=None):
    current = container
    for segment in field[:-1]:
        if isinstance(segment, str) and isinstance(current, dict) and segment in current:
            current = current[segment]
        elif isinstance(segment, int) and isinstance(current, list) and 0 <= segment < len(current):
            current = current[segment]
        else:
            reject("token-field", field)
    leaf = field[-1] if field else None
    if isinstance(leaf, str) and isinstance(current, dict) and leaf in current:
        value = current[leaf]
        if replacement is not None:
            current[leaf] = replacement
        return value
    if isinstance(leaf, int) and isinstance(current, list) and 0 <= leaf < len(current):
        value = current[leaf]
        if replacement is not None:
            current[leaf] = replacement
        return value
    reject("token-field", field)

def validate_tide_temp(actual, token, root, tmp_root, temp_paths):
    if not isinstance(actual, str):
        reject("tmp-path", actual)
    actual = direct_regular(actual, tmp_root, "tide-temp")
    basename = os.path.basename(actual)
    if os.path.dirname(actual) != tmp_root or not basename.startswith("tide") or len(basename) <= len("tide"):
        reject("tmp-owner", actual)
    source = token.get("source")
    source_path = direct_regular(owned_path(root, source), root, "tmp-source")
    if source_path == actual:
        reject("tmp-source-alias", actual)
    if digest_bytes(read_bytes(actual)) != token.get("sha256"):
        reject("tmp-digest", actual)
    prior = temp_paths.setdefault(source, actual)
    if prior != actual:
        reject("tmp-identity", {"source": source, "first": prior, "next": actual})
    for other_source, other_path in temp_paths.items():
        if other_source != source and other_path == actual:
            reject("tmp-slot-alias", {"source": source, "other": other_source})

def normalize_request(raw, exchange, root, tmp_root, host_info, temp_paths):
    try:
        request = json.loads(raw)
    except json.JSONDecodeError as error:
        reject("request-json", str(error))
    if not isinstance(request, dict):
        reject("request-object", type(request).__name__)
    canonical = json.dumps(request, ensure_ascii=False, separators=(",", ":"))
    if canonical != raw:
        reject("request-encoding", raw)
    try:
        expected = json.loads(exchange["request"])
    except (TypeError, json.JSONDecodeError) as error:
        reject("recorded-request-json", str(error))
    normalized = json.loads(raw)
    seen_fields = set()
    for token in exchange["tokens"]:
        if not isinstance(token, dict) or not isinstance(token.get("field"), list) or not token["field"]:
            reject("token-shape", token)
        field_key = json.dumps(token["field"], separators=(",", ":"))
        if field_key in seen_fields:
            reject("token-duplicate", token["field"])
        seen_fields.add(field_key)
        actual = json_field(normalized, token["field"])
        kind = token.get("kind")
        if kind == "root-path" and set(token) == {"field", "kind", "relative"}:
            expected_path = owned_path(root, token["relative"])
            if actual != expected_path:
                reject("request-root-path", {"field": token["field"], "actual": actual})
            replacement = "[ROOT]/" + token["relative"]
        elif kind == "host-info" and set(token) == {"field", "kind"}:
            if not isinstance(actual, str) or actual != host_info:
                reject("host-info", actual)
            replacement = "[HOSTINFO]"
        elif kind == "tide-temp" and set(token) == {"field", "kind", "source", "sha256"}:
            validate_tide_temp(actual, token, root, tmp_root, temp_paths)
            replacement = "[TIDE-TMP]"
        else:
            reject("token-kind", token)
        json_field(normalized, token["field"], replacement)
    normalized_raw = json.dumps(normalized, ensure_ascii=False, separators=(",", ":"))
    if normalized_raw != exchange["request"] or normalized != expected:
        reject("request", {"ordinal": exchange["ordinal"], "actual": normalized_raw})
    if root in normalized_raw or os.environ["TIDE368_TSSERVER"] in normalized_raw:
        reject("request-path-residue", normalized_raw)
    return normalized_raw

def reserved_response_tokens(value):
    if isinstance(value, str) and (value == "[ROOT]" or value.startswith("[ROOT]/") or value == "[TSSERVER]" or value.startswith("[TSSERVER-DIR]/") or value == "[PROJECT-ID]"):
        return 1
    if isinstance(value, list):
        return sum(reserved_response_tokens(item) for item in value)
    if isinstance(value, dict):
        return sum(reserved_response_tokens(item) for item in value.values())
    return 0

def expanded_frame(frame, root, owner, bundle):
    path = direct_regular(owned_path(owner, frame["path"]), owner, "frame-pre-emit")
    raw = read_bytes(path)
    if digest_bytes(raw) != frame["sha256"]:
        reject("frame-digest-pre-emit", frame["path"])
    if raw.count(b"\r\n\r\n") != 1:
        reject("frame-header-count", frame["path"])
    header, body = raw.split(b"\r\n\r\n", 1)
    prefix = b"Content-Length: "
    if not header.startswith(prefix) or not header[len(prefix):].isdigit() or int(header[len(prefix):]) != len(body):
        reject("frame-header", frame["path"])
    if body.endswith(b"\n"):
        if body.endswith(b"\r\n") or body.endswith(b"\n\n"):
            reject("frame-newline-pre-emit", frame["path"])
        json_body = body[:-1]
        trailing_newline = b"\n"
    else:
        json_body = body
        trailing_newline = b""
    try:
        parsed = json.loads(json_body)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        reject("frame-json-pre-emit", {"path": frame["path"], "error": str(error)})
    canonical = json.dumps(parsed, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if canonical != json_body:
        reject("frame-encoding-pre-emit", frame["path"])
    seen_fields = set()
    tokens = frame["tokens"]
    for token in tokens:
        if not isinstance(token, dict) or not isinstance(token.get("kind"), str) or not isinstance(token.get("field"), list) or not token["field"]:
            reject("frame-token-shape", token)
        field_key = json.dumps(token["field"], separators=(",", ":"))
        if field_key in seen_fields:
            reject("frame-token-duplicate", token["field"])
        seen_fields.add(field_key)
        actual = json_field(parsed, token["field"])
        kind = token["kind"]
        if kind == "root-path" and set(token) == {"field", "kind", "relative"}:
            expected = "[ROOT]/" + token["relative"]
            if actual != expected:
                reject("frame-token-value", token)
            replacement = owned_path(root, token["relative"])
        elif kind == "project-root" and set(token) == {"field", "kind"} and actual == "[ROOT]":
            replacement = root
        elif kind == "tsserver-path" and set(token) == {"field", "kind"} and actual == "[TSSERVER]":
            replacement = os.environ["TIDE368_TSSERVER"]
        elif kind == "tsserver-bundled-path" and set(token) == {"field", "kind", "relative"}:
            expected = "[TSSERVER-DIR]/" + token["relative"]
            if actual != expected:
                reject("frame-token-value", token)
            replacement = tsserver_bundled_path(
                bundle, token["relative"], "frame-tsserver-bundle")
        elif kind == "embedded-root-path" and set(token) == {"field", "kind", "prefix", "relative", "suffix"}:
            if not isinstance(token["prefix"], str) or not isinstance(token["suffix"], str):
                reject("frame-token-literal", token)
            expected = token["prefix"] + "[ROOT]/" + token["relative"] + token["suffix"]
            if actual != expected:
                reject("frame-token-value", token)
            replacement = token["prefix"] + owned_path(root, token["relative"]) + token["suffix"]
        elif kind == "project-id" and set(token) == {"field", "kind", "relative"} and actual == "[PROJECT-ID]":
            replacement = digest_bytes(owned_path(root, token["relative"]).encode("utf-8"))
        else:
            reject("frame-token-value", token)
        json_field(parsed, token["field"], replacement)
    if reserved_response_tokens(json.loads(json_body)) != sum(
        token["kind"] != "embedded-root-path" for token in tokens
    ):
        reject("frame-token-coverage", frame["path"])
    for token in tokens:
        expanded = json_field(parsed, token["field"])
        if token["kind"] == "root-path":
            expected_expanded = owned_path(root, token["relative"])
        elif token["kind"] == "project-root":
            expected_expanded = root
        elif token["kind"] == "embedded-root-path":
            expected_expanded = (
                token["prefix"] + owned_path(root, token["relative"]) + token["suffix"]
            )
        elif token["kind"] == "project-id":
            expected_expanded = digest_bytes(owned_path(root, token["relative"]).encode("utf-8"))
        elif token["kind"] == "tsserver-bundled-path":
            expected_expanded = tsserver_bundled_path(
                bundle, token["relative"], "frame-tsserver-bundle-recheck")
        else:
            expected_expanded = os.environ["TIDE368_TSSERVER"]
        if expanded != expected_expanded:
            reject("frame-token-expansion", token)
    body = json.dumps(parsed, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + trailing_newline
    return prefix + str(len(body)).encode("ascii") + b"\r\n\r\n" + body

def write_stdout(value):
    write_all_fd(sys.stdout.fileno(), value)

def record_emitted(state, value):
    state["frames"] += 1
    state["bytes"] += len(value)
    state["sha256"].update(value)

def trace_frame(trace, session_index, ordinal, delivery_after, frame, delivery, value):
    append_json(trace, {
        "event": "frame", "session": session_index, "request": ordinal,
        "delivery_after": delivery_after, "owner": frame["owner"],
        "sha256": frame["sha256"], "delivery": delivery,
        "emitted_bytes": len(value), "emitted_sha256": digest_bytes(value),
    })

def emit_frames(scheduled_frames, root, owner, bundle, trace, session_index, delivery_after, state):
    index = 0
    while index < len(scheduled_frames):
        ordinal, frame = scheduled_frames[index]
        value = expanded_frame(frame, root, owner, bundle)
        delivery = frame["delivery"]
        mode = delivery.get("mode") if isinstance(delivery, dict) else None
        if mode == "coalesced":
            if index + 1 >= len(scheduled_frames):
                reject("coalesced-terminal", ordinal)
            tail_ordinal, tail = scheduled_frames[index + 1]
            if tail.get("delivery") != {"mode": "whole"}:
                reject("coalesced-tail-delivery", tail.get("delivery"))
            tail_value = expanded_frame(tail, root, owner, bundle)
            write_stdout(value + tail_value)
            record_emitted(state, value)
            record_emitted(state, tail_value)
            trace_frame(trace, session_index, ordinal, delivery_after, frame, "coalesced", value)
            trace_frame(trace, session_index, tail_ordinal, delivery_after, tail, "coalesced-tail", tail_value)
            index += 2
            continue
        marker = value.index(b"\r\n\r\n")
        header_end = marker + 4
        if mode == "whole":
            chunks = (value,)
        elif mode == "split-header":
            at = delivery.get("at")
            if not isinstance(at, int) or at <= 0 or at >= header_end:
                reject("split-header", delivery)
            chunks = (value[:at], value[at:])
        elif mode == "split-body":
            at = delivery.get("at")
            body_length = len(value) - header_end
            if not isinstance(at, int) or at <= 0 or at >= body_length:
                reject("split-body", delivery)
            chunks = (value[:header_end + at], value[header_end + at:])
        else:
            reject("delivery", delivery)
        for chunk in chunks:
            write_stdout(chunk)
        record_emitted(state, value)
        trace_frame(trace, session_index, ordinal, delivery_after, frame, mode, value)
        index += 1

def emit_due_outputs(exchanges, boundary, root, owner, bundle, trace, session_index, state, emitted_owners):
    scheduled_frames = []
    for exchange in exchanges:
        if (exchange["frames"] and exchange["delivery_after"] == boundary
                and exchange["ordinal"] not in emitted_owners):
            scheduled_frames.extend(
                (exchange["ordinal"], frame) for frame in exchange["frames"])
            emitted_owners.add(exchange["ordinal"])
    if scheduled_frames:
        emit_frames(scheduled_frames, root, owner, bundle, trace, session_index,
                    boundary, state)

def completed_state(session, consumed, request_digest, emitted):
    if consumed != session["request_count"]:
        reject("missing-request", {"expected": session["request_count"], "actual": consumed})
    if request_digest.hexdigest() != session["request_stream_sha256"]:
        reject("request-stream-digest", request_digest.hexdigest())
    if emitted["frames"] != session["frame_count"]:
        reject("emitted-frame-count", emitted["frames"])
    return {
        "requests": consumed,
        "frames": emitted["frames"],
        "bytes": emitted["bytes"],
        "request_sha256": request_digest.hexdigest(),
        "emitted_sha256": emitted["sha256"].hexdigest(),
    }

def append_terminal(trace, event, session_index, request, state, code=None):
    record = {"event": event, "session": session_index, "request": request, **state}
    if code is not None:
        record["code"] = code
    append_json(trace, record)

def await_public_kill():
    while True:
        value = os.read(sys.stdin.fileno(), 1)
        if not value:
            reject("client-killed-eof", "public delete-process never arrived")
        reject("client-killed-extra-input", repr(value))

def main():
    owner, control, root, tmp_root, trace, ledger, config, config_digest = bootstrap()
    adapter, adapter_digest, interpreter, tsserver, host_info = exact_environment(config, config_digest, owner, control, root, tmp_root)
    bundle = validate_executables(config, adapter, adapter_digest, interpreter, tsserver, root)
    validate_config(config, root, owner)
    ledger_fd, session_zero_index, session = claim_session(
        config, ledger, trace, root, owner, bundle)
    session_index = session_zero_index + 1
    append_json(trace, {
        "event": "START", "session": session_index,
        "first_ordinal": session["first_ordinal"], "interpreter": "[INTERPRETER]",
        "interpreter_match": sys.executable == interpreter,
        "interpreter_sha256": digest_bytes(read_bytes(interpreter)),
        "tsserver": "[TSSERVER]", "tsserver_match": sys.argv[1] == tsserver,
        "tsserver_sha256": digest_bytes(read_bytes(tsserver)),
    })
    exchanges = session["exchanges"]
    termination = session["termination"]
    request_digest = hashlib.sha256()
    emitted = {"frames": 0, "bytes": 0, "sha256": hashlib.sha256()}
    temp_paths = {}
    emitted_owners = set()
    consumed = 0
    for line in sys.stdin.buffer:
        if not line.endswith(b"\n") or line.endswith(b"\r\n"):
            reject("request-newline", repr(line[-4:]))
        if consumed >= len(exchanges):
            reject("extra-request", consumed + 1)
        exchange = exchanges[consumed]
        ordinal = exchange["ordinal"]
        validate_generation(exchange, root)
        try:
            raw = line[:-1].decode("utf-8")
        except UnicodeDecodeError as error:
            reject("request-utf8", str(error))
        normalized = normalize_request(raw, exchange, root, tmp_root, host_info, temp_paths)
        request_digest.update(normalized.encode("utf-8") + b"\n")
        append_json(trace, {"event": "request", "session": session_index, "ordinal": ordinal, "json": normalized})
        consumed += 1
        emit_due_outputs(exchanges, ordinal, root, owner, bundle, trace, session_index,
                         emitted, emitted_owners)
        if termination.get("kind") == "exit-after" and termination.get("request") == ordinal:
            state = completed_state(session, consumed, request_digest, emitted)
            append_terminal(trace, "EXPECTED_EXIT", session_index, ordinal, state, termination.get("code"))
            sys.stderr.write("TIDE368 expected external exit\n")
            sys.stderr.flush()
            raise SystemExit(termination["code"])
        if termination.get("kind") == "client-killed" and termination.get("ready_after") == ordinal:
            state = completed_state(session, consumed, request_digest, emitted)
            append_terminal(trace, "READY", session_index, ordinal, state)
            await_public_kill()
    if termination.get("kind") != "clean-eof":
        reject("unexpected-eof", termination)
    state = completed_state(session, consumed, request_digest, emitted)
    append_terminal(trace, "DONE", session_index, exchanges[-1]["ordinal"], state)
    os.close(ledger_fd)

try:
    main()
except BrokenPipeError as error:
    reject("broken-pipe", str(error))
except Exception as error:
    reject("adapter-exception", {"type": type(error).__name__, "detail": str(error)})
"####;

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'eldoc)
(require 'flycheck)
(require 'imenu)
(require 'js)
(require 'json)
(require 'seq)
(require 'tide)
(require 'xref)

;; Unicode path conversion can lazily use either editor infrastructure buffer
;; depending on the exact conversion route.  Give the shared package baseline
;; stable instances to restore per case.
(get-buffer-create " *code-conversion-work*")
(get-buffer-create " *code-converting-work*")

(defconst tide368-test-spec-sha256
  "e343f8c0c995136dfefec9a7e79edf2b5668b847e69dfa730f276a47d450a7c8")
(defconst tide368-test-standards-sha256
  "2d555f93c7f713000b46ffa4c6bb587e504213864d0942b050aed5cdfb9c1a4f")
(defconst tide368-test-tsserver-bundle-manifest-sha256
  "f95ede2ee0564044ca2d61127e75fcd4b3af5b260998823c108d3c59add400da")

(defconst tide368-test-state-symbols
  '(process-environment exec-path temporary-file-directory
    enable-local-variables enable-local-eval enable-dir-local-variables
    create-lockfiles make-backup-files vc-handled-backends
    tide-node-executable tide-node-flags tide-tsserver-executable
    tide-tscompiler-executable
    tide-tsserver-flags tide-tsserver-process-environment
    tide-tsserver-start-method tide-project-cleanup-delay tide-default-mode
    tide-sync-request-timeout tide-native-json-parsing tide-recenter-after-jump
    tide-server-buffer-name tide-hl-identifier-idle-time tide-format-options
    tide-user-preferences tide-save-buffer-after-code-edit tide-enable-xref
    tide-post-code-edit-hook tide-sort-completions-by-kind
    tide-disable-suggestions tide-completion-setup-company-backend
    tide-completion-ignore-case tide-completion-show-source
    tide-completion-fuzzy tide-completion-detailed
    tide-completion-enable-autoimport-suggestions
    tide-navto-item-filter tide-jump-to-definition-reuse-window
    tide-imenu-flatten tide-allow-popup-select tide-always-show-documentation
    tide-server-max-response-length tide-tsserver-locator-function
    tide-jump-to-fallback tide-filter-out-warning-completions
    tide-request-counter tide-project-configs tide-servers
    tide-response-callbacks tide-tsserver-unsupported-commands
    tide-event-listeners tide--cleanup-timer tide--cleanup-kinds
    tide--hl-identifier-timer tide--current-hl-identifier-idle-time
    tide-xref--last-completion-table tide-lv-wnd
    xref--history global-mark-ring tag-mark-stack
    minibuffer-history file-name-history extended-command-history
    command-history kill-ring kill-ring-yank-pointer
    flycheck--last-buffer flycheck--project-error-store
    coding-system-for-read coding-system-for-write)
  "Mutable editor and Tide state restored after every rank-368 story.")

(defconst tide368-test-forbidden-external-functions
  '(call-process call-process-region process-file
    make-network-process open-network-stream
    url-retrieve url-retrieve-synchronously
    start-process-shell-command shell-command async-shell-command))

(defvar tide368-test-world nil)
(defvar tide368-test-external-advices nil)
(defvar tide368-test-process-events nil)
(defvar tide368-test-owned-processes nil)
(defvar tide368-test-parked-buffers nil)
(defvar tide368-test-approved-start-depth 0)
(defvar tide368-test-make-process-result nil)
(defvar tide368-test-planned-session nil)
(defvar tide368-test-planned-session-index nil)
(defvar tide368-test-public-phase nil)
(defvar tide368-test-public-delete-route nil)
(defvar tide368-test-public-delete-ledger nil)
(defvar tide368-test-process-terminals nil)
(defvar tide368-test-current-send-command nil)
(defvar tide368-test-callback-policy-ledger nil)

(defun tide368-test-variable-state (symbol)
  (if (boundp symbol)
      (let ((value (symbol-value symbol)))
        (list :bound t :value value
              :hash-test (and (hash-table-p value) (hash-table-test value))
              :hash-weakness
              (and (hash-table-p value) (hash-table-weakness value))
              :hash-size (and (hash-table-p value) (hash-table-size value))
              :snapshot
              (cond ((hash-table-p value)
                     (let* ((test (hash-table-test value))
                            (snapshot (make-hash-table
                                      :test (hash-table-test value)
                                      :size (max 1 (hash-table-size value))
                                      :rehash-size (hash-table-rehash-size value)
                                      :rehash-threshold
                                      (hash-table-rehash-threshold value))))
                       (maphash
                        (lambda (key item)
                          (puthash (if (memq test '(eq eql)) key (copy-tree key))
                                   (copy-tree item) snapshot))
                        value)
                       snapshot))
                    ((consp value) (copy-tree value))
                    ((stringp value) (copy-sequence value))
                    (t value))))
    '(:bound nil)))

(defun tide368-test-restore-variable (symbol state)
  (if (plist-get state :bound)
      (let ((value (plist-get state :value))
            (snapshot (plist-get state :snapshot)))
        ;; Configure fresh-binds every mutable subject collection.  Therefore
        ;; the ambient object and all of its aliases must be untouched; do not
        ;; manufacture restoration by mutating that original object in place.
        (unless
            (cond ((hash-table-p value)
                   (and (eq (hash-table-test value)
                            (plist-get state :hash-test))
                        (eq (hash-table-weakness value)
                            (plist-get state :hash-weakness))
                        (= (hash-table-size value)
                           (plist-get state :hash-size))
                        (equal (hash-table-rehash-size value)
                               (hash-table-rehash-size snapshot))
                        (equal (hash-table-rehash-threshold value)
                               (hash-table-rehash-threshold snapshot))
                        (tide368-test-hash-table-equal value snapshot)))
                  ((or (consp value) (stringp value)) (equal value snapshot))
                  (t t))
          (error "Tide mutated an ambient variable object: %S" symbol))
        (set symbol value))
    (makunbound symbol)))

(defun tide368-test-hash-table-equal (left right)
  (and (= (hash-table-count left) (hash-table-count right))
       (catch 'different
         (maphash
          (lambda (key item)
            (let ((sentinel (make-symbol "missing")))
              (unless (equal item (gethash key right sentinel))
                (throw 'different nil))))
          left)
         t)))

(defun tide368-test-variable-restored-p (symbol state)
  (if (plist-get state :bound)
      (and (boundp symbol) (eq (symbol-value symbol) (plist-get state :value))
           (let ((value (symbol-value symbol))
                 (snapshot (plist-get state :snapshot)))
             (cond ((hash-table-p value)
                   (and (eq (hash-table-test value)
                             (plist-get state :hash-test))
                         (eq (hash-table-weakness value)
                             (plist-get state :hash-weakness))
                         (= (hash-table-size value)
                            (plist-get state :hash-size))
                         (equal (hash-table-rehash-size value)
                                (hash-table-rehash-size snapshot))
                         (equal (hash-table-rehash-threshold value)
                                (hash-table-rehash-threshold snapshot))
                         (tide368-test-hash-table-equal value snapshot)))
                   ((consp value) (equal value snapshot))
                   ((stringp value) (equal value snapshot))
                   (t (eq value snapshot)))))
    (not (boundp symbol))))

(defun tide368-test-local-variable-state (symbol buffer)
  (with-current-buffer buffer
    (list :local (local-variable-p symbol)
          :value (and (boundp symbol) (symbol-value symbol)))))

(defun tide368-test-restore-local-variable (symbol buffer state)
  (unless (buffer-live-p buffer)
    (error "Tide baseline local-state buffer died: %S" buffer))
  (with-current-buffer buffer
    (if (plist-get state :local)
        (set (make-local-variable symbol) (plist-get state :value))
      (kill-local-variable symbol))))

(defun tide368-test-condition-state (condition)
  (list :symbol (car condition)
        :data (copy-tree (cdr condition))
        :message (copy-sequence (error-message-string condition))))

(defun tide368-test-normalize-string (value)
  (if (not (stringp value)) value
    (let ((normalized (substring-no-properties value)))
      (when tide368-test-world
        (dolist (entry `((,(plist-get tide368-test-world :root) . "[ROOT]/")
                         (,(directory-file-name
                            (plist-get tide368-test-world :root)) . "[ROOT]")
                         (,(plist-get tide368-test-world :owner) . "[OWNER]/")
                         (,(directory-file-name
                            (plist-get tide368-test-world :owner)) . "[OWNER]")
                         (,(plist-get tide368-test-world :adapter) . "[ADAPTER]")
                         (,(plist-get tide368-test-world :server) . "[TSSERVER]")
                         (,(and (plist-get tide368-test-world :server)
                                (file-name-directory
                                 (plist-get tide368-test-world :server)))
                          . "[TSSERVER-DIR]/")
                         (,(plist-get tide368-test-world :interpreter) . "[INTERPRETER]")))
          (when (car entry)
            (setq normalized
                  (replace-regexp-in-string
                   (regexp-quote (car entry)) (cdr entry) normalized t t)))))
      normalized)))

(defun tide368-test-attempt (phase thunk errors)
  (condition-case condition
      (progn (funcall thunk) errors)
    (t (cons (list phase (tide368-test-condition-state condition)) errors))))

(defun tide368-test-file-sha256 (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun tide368-test-bytes-sha256 (bytes)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert bytes)
    (secure-hash 'sha256 (current-buffer))))

(defun tide368-test-new-timers (timers-before idle-before)
  (seq-difference (append timer-list timer-idle-list)
                  (append timers-before idle-before) #'eq))

(defun tide368-test-buffer-content-state (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (let ((minimum (point-min)) (maximum (point-max)))
        (list :buffer buffer
              :text (save-restriction (widen) (buffer-string))
              :point (point) :mark (mark t) :active mark-active
              :modified (buffer-modified-p) :undo (copy-tree buffer-undo-list)
              :read-only buffer-read-only :min minimum :max maximum
              :mode major-mode :file buffer-file-name
              :coding buffer-file-coding-system
              :before-save (copy-sequence before-save-hook)
              :after-save (copy-sequence after-save-hook)
              :after-change (copy-sequence after-change-functions)
              :kill-hook (copy-sequence kill-buffer-hook)
              :overlays
              (mapcar (lambda (overlay)
                        (list overlay (overlay-start overlay) (overlay-end overlay)
                              (copy-tree (overlay-properties overlay))))
                      (save-restriction
                        (widen) (overlays-in (point-min) (point-max)))))))))

(defun tide368-test-restore-buffer-content (state)
  (when state
    (let ((buffer (plist-get state :buffer)))
      (unless (buffer-live-p buffer)
        (error "Tide baseline buffer died: %S" buffer))
      (with-current-buffer buffer
        (let ((inhibit-read-only t) (inhibit-modification-hooks t)
              (before-change-functions nil) (after-change-functions nil))
          (widen) (erase-buffer) (insert (plist-get state :text)))
        (let ((saved (plist-get state :overlays)))
          (dolist (overlay (save-restriction
                             (widen) (overlays-in (point-min) (point-max))))
            (unless (assq overlay saved) (delete-overlay overlay)))
          (dolist (entry saved)
            (let ((overlay (nth 0 entry)) (start (nth 1 entry))
                  (end (nth 2 entry)) (properties (copy-sequence (nth 3 entry))))
              (unless (overlayp overlay)
                (error "Tide baseline overlay died: %S" overlay))
              (move-overlay overlay start end buffer)
              (let ((existing (overlay-properties overlay)))
                (while existing
                  (overlay-put overlay (pop existing) nil)
                  (pop existing)))
              (while properties
                (overlay-put overlay (pop properties) (pop properties))))))
        (goto-char (min (plist-get state :point) (point-max)))
        (if (plist-get state :mark)
            (set-mark (min (plist-get state :mark) (point-max)))
          (set-marker (mark-marker) nil))
        (setq mark-active (plist-get state :active)
              buffer-undo-list (copy-tree (plist-get state :undo))
              buffer-read-only (plist-get state :read-only))
        (unless (and (eq major-mode (plist-get state :mode))
                     (equal buffer-file-name (plist-get state :file)))
          (error "Tide baseline buffer identity changed: %S" buffer))
        (setq buffer-file-coding-system (plist-get state :coding)
              before-save-hook (copy-sequence (plist-get state :before-save))
              after-save-hook (copy-sequence (plist-get state :after-save))
              after-change-functions
              (copy-sequence (plist-get state :after-change))
              kill-buffer-hook (copy-sequence (plist-get state :kill-hook)))
        (set-buffer-modified-p (plist-get state :modified))
        (narrow-to-region (plist-get state :min) (plist-get state :max))))))

(defun tide368-test-buffer-content-restored-p (state)
  (or (null state)
      (let ((buffer (plist-get state :buffer)))
        (and (buffer-live-p buffer)
             (with-current-buffer buffer
               (and (equal (save-restriction (widen) (buffer-string))
                           (plist-get state :text))
                    (= (point) (plist-get state :point))
                    (equal (mark t) (plist-get state :mark))
                    (eq mark-active (plist-get state :active))
                    (eq (buffer-modified-p) (plist-get state :modified))
                    (equal buffer-undo-list (plist-get state :undo))
                    (eq buffer-read-only (plist-get state :read-only))
                    (= (point-min) (plist-get state :min))
                    (= (point-max) (plist-get state :max))
                    (eq major-mode (plist-get state :mode))
                    (equal buffer-file-name (plist-get state :file))
                    (eq buffer-file-coding-system (plist-get state :coding))
                    (equal before-save-hook (plist-get state :before-save))
                    (equal after-save-hook (plist-get state :after-save))
                    (equal after-change-functions
                           (plist-get state :after-change))
                    (equal kill-buffer-hook (plist-get state :kill-hook))
                    (equal
                     (mapcar (lambda (overlay)
                               (list overlay (overlay-start overlay) (overlay-end overlay)
                                     (copy-tree (overlay-properties overlay))))
                             (save-restriction
                               (widen) (overlays-in (point-min) (point-max))))
                     (plist-get state :overlays))))))))

(defun tide368-test-output-buffer-name-p (name)
  (or (member name
              '("*tide-server*" "*tide-references*" "*tide-documentation*"
                "*tide-project-info*" "*tide-error*" "*Tide Server List*"
                "*Flycheck errors*" " *tide-LV*" " *eldoc*" "*Warnings*"
                "*Messages*"))
      (string-prefix-p "*tide-server*<" name)
      (and tide368-test-world
           (let* ((root (directory-file-name
                         (plist-get tide368-test-world :root)))
                  (project (concat (file-name-nondirectory root) "-"
                                   (substring (md5 root) 0 10))))
             (equal name (format "*%s-errors*" project))))))

(defun tide368-test-park-output-buffers ()
  (dolist (buffer (buffer-list))
    (let ((name (buffer-name buffer)))
      (when (and name (tide368-test-output-buffer-name-p name))
        (push (list :buffer buffer :name name
                    :state (tide368-test-buffer-content-state buffer))
              tide368-test-parked-buffers)
        (with-current-buffer buffer
          (rename-buffer
           (generate-new-buffer-name (format " *tide368 baseline %s*" name))
           t))))))

(defun tide368-test-restore-output-buffer (entry)
  (let ((buffer (plist-get entry :buffer)) (name (plist-get entry :name)))
    (unless (buffer-live-p buffer)
      (error "Tide parked output buffer died: %S" name))
    (when-let* ((replacement (get-buffer name)))
      (unless (eq replacement buffer)
        (error "Tide output replacement survived: %S" name)))
    ;; Emacs retains internal object identities for these editor-owned logs,
    ;; so renaming alone cannot prevent writes during the story.  Restore their
    ;; complete saved state before applying the same exact restoration gate.
    (when (member name '("*Messages*" "*Warnings*"))
      (tide368-test-restore-buffer-content (plist-get entry :state)))
    (unless (tide368-test-buffer-content-restored-p (plist-get entry :state))
      (error "Tide parked output buffer was mutated: %S" name))
    (with-current-buffer buffer (rename-buffer name t))))

(defun tide368-test-window-state ()
  (mapcar
   (lambda (window)
     (list :window window :buffer (window-buffer window)
           :edges (window-edges window) :pixel-edges (window-pixel-edges window)
           :point (window-point window) :start (window-start window)
           :hscroll (window-hscroll window) :vscroll (window-vscroll window t)
           :dedicated (window-dedicated-p window)
           ;; Clearing a parameter through GNU's public setter retains an
           ;; alist cell whose cdr is nil.  Such cells have no parameter value
           ;; and are intentionally ignored by window-configuration equality;
           ;; retain every non-nil structural parameter exactly.
           :parameters
           (copy-tree (seq-filter #'cdr (window-parameters window)))
           :margins (window-margins window) :fringes (window-fringes window)
           :scroll-bars (window-scroll-bars window)
           :prev (copy-tree (window-prev-buffers window))
           :next (copy-tree (window-next-buffers window))))
   (window-list nil t)))

(defun tide368-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry state)
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Tide baseline window died: %S" window))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-dedicated-p window (plist-get entry :dedicated))
      (dolist (parameter (window-parameters window))
        (set-window-parameter window (car parameter) nil))
      (dolist (parameter (plist-get entry :parameters))
        (set-window-parameter window (car parameter) (cdr parameter)))
      (let ((margins (plist-get entry :margins)))
        (set-window-margins window (car margins) (cdr margins)))
      (let ((fringes (plist-get entry :fringes)))
        (set-window-fringes window (nth 0 fringes) (nth 1 fringes)
                            (nth 2 fringes) (nth 3 fringes)))
      (let ((bars (plist-get entry :scroll-bars)))
        (set-window-scroll-bars window (nth 0 bars) (nth 2 bars)
                                (nth 3 bars) (nth 5 bars) (nth 6 bars)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t))))

(defun tide368-test-stabilize-batch-frame ()
  ;; A newly created GNU batch frame initially reports its sole ordinary
  ;; window at row zero.  The first real `set-window-configuration' moves that
  ;; window below the menu-bar row, and the row-zero configuration can then no
  ;; longer round-trip after a public `pop-to-buffer'.  Establish and prove the
  ;; editor's own fixed point before taking any per-case ownership snapshot.
  ;; This runs before Tide setup on both editors and does not alter subject
  ;; output; every later cleanup still has to restore this exact baseline.
  (set-window-configuration (current-window-configuration))
  (let ((configuration (current-window-configuration))
        (state (tide368-test-window-state)))
    (set-window-configuration configuration)
    (unless (and (compare-window-configurations
                  (current-window-configuration) configuration)
                 (equal (tide368-test-window-state) state))
      (error "Tide batch-frame baseline is not a fixed point: %S"
             (list :before state :after (tide368-test-window-state))))))

(defun tide368-test-owner-path (relative)
  (expand-file-name relative (plist-get tide368-test-world :owner)))

(defun tide368-test-project-path (relative)
  (expand-file-name relative (plist-get tide368-test-world :root)))

(defun tide368-test-direct-child-p (path parent)
  (let ((parent (file-name-as-directory (file-truename parent))))
    (and (file-name-absolute-p path)
         (equal (file-name-directory (directory-file-name path)) parent))))

(defun tide368-test-write-exclusive-bytes (path bytes expected-sha256 &optional mode)
  (unless (and (file-name-absolute-p path) (not (file-exists-p path)))
    (error "Tide refuses nonexclusive artifact path: %S" path))
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'no-conversion))
    (with-temp-buffer
      (set-buffer-multibyte nil)
      (insert bytes)
      (write-region (point-min) (point-max) path nil 'silent nil 'excl)))
  (when mode (set-file-modes path mode))
  (unless (and (file-regular-p path)
               (not (file-symlink-p path))
               (equal (tide368-test-file-sha256 path) expected-sha256))
    (error "Tide artifact materialization drifted: %S" path))
  path)

(defun tide368-test-json-value (key object)
  (alist-get key object nil nil #'string=))

(defun tide368-test-runtime-config (config-bytes)
  (json-parse-string
   (decode-coding-string config-bytes 'utf-8-unix)
   :object-type 'alist :array-type 'list :null-object nil
   :false-object :json-false))

(defun tide368-test-validate-tsserver-bundle (config server)
  (let* ((plan (tide368-test-json-value "tsserver_bundle" config))
         (files (tide368-test-json-value "files" plan))
         (manifest-sha256
          (tide368-test-json-value "manifest_sha256" plan))
         (base (directory-file-name (file-name-directory server)))
         (previous nil)
         (manifest "")
         validated)
    (unless (and (equal manifest-sha256
                        tide368-test-tsserver-bundle-manifest-sha256)
                 (listp files) (= (length files) 47)
                 (file-directory-p base) (not (file-symlink-p base))
                 (equal (file-truename base) base))
      (error "Tide tsserver bundle plan is invalid: %S" plan))
    (dolist (entry files)
      (let* ((relative (tide368-test-json-value "path" entry))
             (digest (tide368-test-json-value "sha256" entry))
             (path (and (stringp relative) (expand-file-name relative base))))
        (unless (and (stringp relative) (not (string-empty-p relative))
                     (equal relative (file-name-nondirectory relative))
                     (equal relative (file-relative-name path base))
                     (not (string-match-p "\\\\" relative))
                     (or (null previous) (string< previous relative))
                     (stringp digest)
                     (string-match-p "\\`[0-9a-f]\\{64\\}\\'" digest)
                     (file-regular-p path) (not (file-symlink-p path))
                     (equal (file-truename path) path)
                     (equal (directory-file-name (file-name-directory path)) base)
                     (equal (tide368-test-file-sha256 path) digest))
          (error "Tide tsserver bundle member is invalid: %S" entry))
        (setq previous relative
              manifest (concat manifest relative " " digest "\n"))
        (push (cons relative digest) validated)))
    (unless (equal (tide368-test-bytes-sha256
                    (encode-coding-string manifest 'utf-8-unix t))
                   tide368-test-tsserver-bundle-manifest-sha256)
      (error "Tide tsserver bundle manifest digest drifted"))
    (nreverse validated)))

(defun tide368-test-tsserver-bundled-path (relative)
  (let* ((server (plist-get tide368-test-world :server))
         (base (directory-file-name (file-name-directory server)))
         (entry (and (stringp relative)
                     (assoc-string relative
                                   (plist-get tide368-test-world
                                              :tsserver-bundle)
                                   nil)))
         (path (and entry (expand-file-name relative base))))
    (unless (and entry (equal relative (file-name-nondirectory relative))
                 (file-regular-p path) (not (file-symlink-p path))
                 (equal (file-truename path) path)
                 (equal (directory-file-name (file-name-directory path)) base)
                 (equal (tide368-test-file-sha256 path) (cdr entry)))
      (error "Tide tsserver bundle path drifted: %S" relative))
    path))

(defun tide368-test-allocate-world (scenario)
  (unless (memq scenario
                '(lifecycle navigation references diagnostics edits rename
                  failure-recovery))
    (error "Tide unknown scenario: %S" scenario))
  (let ((raw-workspace (getenv "NEOMACS_TEST_WORKSPACE_ROOT"))
        (raw-sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (dolist (entry `((:workspace . ,raw-workspace) (:sandbox . ,raw-sandbox)))
      (unless (and (stringp (cdr entry)) (file-name-absolute-p (cdr entry))
                   (file-directory-p (cdr entry)))
        (error "Tide unsafe oracle root: %S" entry)))
    (let* ((workspace (file-name-as-directory (file-truename raw-workspace)))
           (workspace-tmp
            (file-name-as-directory
             (file-truename (expand-file-name "tmp" workspace))))
           (sandbox (file-name-as-directory (file-truename raw-sandbox)))
           (owner (expand-file-name
                   (format "tide368-%s-owner-%s/" scenario (emacs-pid)) sandbox))
           (root (expand-file-name "project space 界/" owner))
           (control (expand-file-name ".tide368/" owner)))
      (unless (and (string-prefix-p workspace-tmp sandbox)
                   (tide368-test-direct-child-p owner sandbox)
                   (not (file-exists-p owner)))
        (error "Tide refuses owned root allocation: %S"
               (list workspace-tmp sandbox owner)))
      (make-directory owner nil)
      (condition-case condition
          (progn
            (dolist (directory
                     (list root control (expand-file-name "bin/" control)
                           (expand-file-name "frames/" control)
                           (expand-file-name "home/" control)
                           (expand-file-name "tmp/" control)))
              (make-directory directory t))
            (list :scenario scenario :workspace workspace :sandbox sandbox
                  :owner (file-name-as-directory (file-truename owner))
                  :root (file-name-as-directory (file-truename root))
                  :control (file-name-as-directory (file-truename control))))
        (t
         (when (file-exists-p owner) (delete-directory owner t))
         (signal (car condition) (cdr condition)))))))

(defun tide368-test-materialize-files (entries)
  (dolist (entry entries)
    (let* ((role (plist-get entry :role))
           (relative (plist-get entry :path))
           (base (pcase role
                   ('project (plist-get tide368-test-world :root))
                   ('owner (plist-get tide368-test-world :owner))
                   (_ (error "Tide unknown artifact role: %S" role))))
           (path (expand-file-name relative base))
           (bytes (base64-decode-string (plist-get entry :base64))))
      (unless (and (stringp relative)
                   (equal relative (file-relative-name path base))
                   (not (string-prefix-p "../" relative)))
        (error "Tide artifact escaped its role root: %S" entry))
      (tide368-test-write-exclusive-bytes
       path bytes (plist-get entry :sha256)))))

(defun tide368-test-materialize (artifacts)
  (let* ((adapter-body (base64-decode-string
                        (plist-get artifacts :adapter-base64)))
         (config-bytes (base64-decode-string
                        (plist-get artifacts :config-base64)))
         (config (tide368-test-runtime-config config-bytes))
         (interpreter-plan (tide368-test-json-value "interpreter" config))
         (server-plan (tide368-test-json-value "tsserver" config))
         (interpreter (tide368-test-json-value "path" interpreter-plan))
         (server (tide368-test-json-value "path" server-plan))
         (adapter (tide368-test-owner-path ".tide368/bin/tide-node"))
         (config-path (tide368-test-owner-path ".tide368/config.json"))
         (adapter-bytes
          (concat (encode-coding-string (format "#!%s\n" interpreter)
                                        'utf-8-unix t)
                  adapter-body))
         (adapter-sha256 (tide368-test-bytes-sha256 adapter-bytes)))
    (unless (equal (tide368-test-json-value "scenario" config)
                   (symbol-name (plist-get tide368-test-world :scenario)))
      (error "Tide scenario/artifact mismatch: %S"
             (list (plist-get tide368-test-world :scenario)
                   (tide368-test-json-value "scenario" config))))
    (unless (and (file-name-absolute-p interpreter)
                 (file-executable-p interpreter)
                 (not (file-symlink-p interpreter))
                 (equal (tide368-test-file-sha256 interpreter)
                        (tide368-test-json-value "sha256" interpreter-plan))
                 (file-name-absolute-p server) (file-regular-p server)
                 (not (file-symlink-p server))
                 (equal (tide368-test-file-sha256 server)
                        (tide368-test-json-value "sha256" server-plan)))
      (error "Tide runtime identities drifted before materialization"))
    (let ((bundle (tide368-test-validate-tsserver-bundle config server)))
      (setq tide368-test-world
            (plist-put tide368-test-world :tsserver-bundle bundle)))
    (tide368-test-materialize-files (plist-get artifacts :fixtures))
    (tide368-test-materialize-files (plist-get artifacts :frames))
    (tide368-test-write-exclusive-bytes
     config-path config-bytes (plist-get artifacts :config-sha256))
    (tide368-test-write-exclusive-bytes adapter adapter-bytes adapter-sha256 #o700)
    (dolist (entry '((".tide368/invocations" . "0\n")
                     (".tide368/trace.jsonl" . "")
                     (".tide368/miss.jsonl" . "")))
      (let ((bytes (encode-coding-string (cdr entry) 'utf-8-unix t)))
        (tide368-test-write-exclusive-bytes
         (tide368-test-owner-path (car entry)) bytes
         (tide368-test-bytes-sha256 bytes))))
    (setq tide368-test-world
          (append tide368-test-world
                  (list :adapter adapter :adapter-sha256 adapter-sha256
                        :config-data config
                        :config config-path
                        :config-sha256 (plist-get artifacts :config-sha256)
                        :interpreter interpreter :server server
                        :trace (tide368-test-owner-path ".tide368/trace.jsonl")
                        :miss (tide368-test-owner-path ".tide368/miss.jsonl")
                        :ledger (tide368-test-owner-path ".tide368/invocations")
                        :home (tide368-test-owner-path ".tide368/home/")
                        :tmp (tide368-test-owner-path ".tide368/tmp/"))))))

(defun tide368-test-exact-environment ()
  (let ((host-info (emacs-version)))
    (mapcar
     (lambda (entry) (format "%s=%s" (car entry) (cdr entry)))
     `(("DISPLAY" . ":0")
       ("HOME" . ,(directory-file-name
                     (plist-get tide368-test-world :home)))
       ("LANG" . "C.UTF-8") ("LC_ALL" . "C.UTF-8")
       ("LOGNAME" . "tide368")
       ("TIDE368_ADAPTER" . ,(plist-get tide368-test-world :adapter))
       ("TIDE368_ADAPTER_SHA256" . ,(plist-get tide368-test-world :adapter-sha256))
       ("TIDE368_CONFIG" . ,(plist-get tide368-test-world :config))
       ("TIDE368_CONFIG_SHA256" . ,(plist-get tide368-test-world :config-sha256))
       ("TIDE368_HOSTINFO" . ,host-info)
       ("TIDE368_INTERPRETER" . ,(plist-get tide368-test-world :interpreter))
       ("TIDE368_INTERPRETER_SHA256" .
        ,(tide368-test-file-sha256 (plist-get tide368-test-world :interpreter)))
       ("TIDE368_INVOCATION_LEDGER" . ,(plist-get tide368-test-world :ledger))
       ("TIDE368_MISS" . ,(plist-get tide368-test-world :miss))
       ("TIDE368_OWNER" . ,(directory-file-name
                             (plist-get tide368-test-world :owner)))
       ("TIDE368_ROOT" . ,(directory-file-name
                            (plist-get tide368-test-world :root)))
       ("TIDE368_TMP" . ,(directory-file-name
                           (plist-get tide368-test-world :tmp)))
       ("TIDE368_TRACE" . ,(plist-get tide368-test-world :trace))
       ("TIDE368_TSSERVER" . ,(plist-get tide368-test-world :server))
       ("TIDE368_TSSERVER_SHA256" .
        ,(tide368-test-file-sha256 (plist-get tide368-test-world :server)))
       ("TMPDIR" . ,(directory-file-name (plist-get tide368-test-world :tmp)))
       ("TZ" . "UTC") ("USER" . "tide368")))))

(defun tide368-test-configure ()
  (let ((environment (tide368-test-exact-environment)))
    (setq process-environment nil exec-path nil
          default-directory (plist-get tide368-test-world :root)
          temporary-file-directory (plist-get tide368-test-world :tmp)
          enable-local-variables nil enable-local-eval nil
          enable-dir-local-variables nil create-lockfiles nil
          make-backup-files nil
          vc-handled-backends nil
          tide-node-executable (plist-get tide368-test-world :adapter)
          tide-node-flags nil
          tide-tsserver-executable (plist-get tide368-test-world :server)
          tide-tscompiler-executable nil
          tide-tsserver-flags '("--disableAutomaticTypingAcquisition")
          tide-tsserver-process-environment environment
          tide-tsserver-start-method 'immediate tide-project-cleanup-delay nil
          tide-default-mode "JS" tide-sync-request-timeout 5
          tide-native-json-parsing t tide-recenter-after-jump nil
          tide-server-buffer-name "*tide-server*"
          tide-hl-identifier-idle-time 0.5
          tide-format-options '(:tabSize 2 :indentSize 2)
          tide-user-preferences
          '(:includeCompletionsForModuleExports t
            :includeCompletionsWithInsertText t
            :allowTextChangesInNewFiles t
            :generateReturnInDocTemplate t)
          tide-save-buffer-after-code-edit t tide-enable-xref t
          tide-post-code-edit-hook nil tide-sort-completions-by-kind nil
          tide-disable-suggestions nil tide-completion-setup-company-backend nil
          tide-completion-ignore-case nil tide-completion-show-source nil
          tide-completion-fuzzy nil tide-completion-detailed nil
          tide-completion-enable-autoimport-suggestions t
          tide-navto-item-filter #'tide-navto-item-filter-default
          tide-jump-to-definition-reuse-window t tide-imenu-flatten nil
          tide-allow-popup-select '(code-fix refactor)
          tide-always-show-documentation nil
          tide-server-max-response-length 102400
          tide-tsserver-locator-function
          #'tide-tsserver-locater-npmlocal-projectile-npmglobal
          tide-jump-to-fallback #'tide-jump-to-fallback-not-given
          tide-filter-out-warning-completions nil
          tide-request-counter 0
          tide-project-configs (make-hash-table :test 'equal)
          tide-servers (make-hash-table :test 'equal)
          tide-response-callbacks (make-hash-table :test 'equal)
          tide-tsserver-unsupported-commands (make-hash-table :test 'equal)
          tide-event-listeners (make-hash-table :test 'equal)
          tide--cleanup-timer nil tide--cleanup-kinds nil
          tide--hl-identifier-timer nil
          tide--current-hl-identifier-idle-time tide-hl-identifier-idle-time
          tide-xref--last-completion-table nil tide-lv-wnd nil
          xref--history (cons nil nil) global-mark-ring nil mark-ring nil
          tag-mark-stack nil minibuffer-history nil file-name-history nil
          extended-command-history nil command-history nil
          kill-ring nil kill-ring-yank-pointer nil flycheck--last-buffer nil
          flycheck--project-error-store (make-hash-table :test 'eq))
    (setq tide368-test-world
          (plist-put tide368-test-world :environment environment))))

(defun tide368-test-forbidden-external (symbol &rest _arguments)
  (error "Tide attempted forbidden external boundary: %S" symbol))

(defun tide368-test-start-file-process
    (original name buffer program &rest arguments)
  (let ((expected-program (plist-get tide368-test-world :adapter))
        (expected-arguments
         (list (plist-get tide368-test-world :server)
               "--disableAutomaticTypingAcquisition"))
        (expected-environment (plist-get tide368-test-world :environment)))
    (unless (and (equal name "tsserver") (buffer-live-p buffer)
                 (string-prefix-p tide-server-buffer-name (buffer-name buffer))
                 (equal program expected-program)
                 (equal arguments expected-arguments)
                 (equal default-directory (plist-get tide368-test-world :root))
                 (equal process-environment expected-environment)
                 (equal (tide368-test-file-sha256 program)
                        (plist-get tide368-test-world :adapter-sha256))
                 (equal (tide368-test-file-sha256 (car arguments))
                        (getenv-internal "TIDE368_TSSERVER_SHA256"
                                         expected-environment)))
      (error "Tide start-file-process launch drifted: %S"
             (list name (buffer-name buffer) program arguments
                   default-directory process-environment)))
    (let* ((tide368-test-make-process-result nil)
           (tide368-test-approved-start-depth
            (1+ tide368-test-approved-start-depth))
           (session-index (1+ (length tide368-test-owned-processes)))
           (sessions (tide368-test-json-value
                      "sessions" (plist-get tide368-test-world :config-data)))
           (session (nth (1- session-index) sessions))
           (process
            (progn
              (unless session
                (error "Tide attempted an unplanned replay session: %S"
                       session-index))
              (let ((tide368-test-planned-session session)
                    (tide368-test-planned-session-index session-index))
                (apply original name buffer program arguments)))))
      (unless (and (processp process) (eq process tide368-test-make-process-result)
                   (eq (process-buffer process) buffer)
                   (process-live-p process))
        (error "Tide start/make process identity drifted: %S" process))
      (push (list :name name :buffer (buffer-name buffer)
                  :program '[ADAPTER]
                  :arguments (list '[TSSERVER] (cadr arguments))
                  :cwd '[ROOT] :environment-count (length process-environment))
            tide368-test-process-events)
      process)))

(defun tide368-test-assert-current-server ()
  (let* ((process (tide-current-server))
         (buffer (and process (process-get process 'tide368-initial-buffer))))
    (unless (and (processp process) (process-live-p process)
                 (buffer-live-p buffer) (eq (process-buffer process) buffer)
                 (equal (process-command process)
                        (list (plist-get tide368-test-world :adapter)
                              (plist-get tide368-test-world :server)
                              "--disableAutomaticTypingAcquisition"))
                 (null (process-tty-name process))
                 (equal (process-coding-system process)
                        '(utf-8-unix . utf-8-unix))
                 (eq (process-filter process) #'tide-net-filter)
                 (eq (process-sentinel process) #'tide-net-sentinel)
                 (not (process-query-on-exit-flag process))
                 (equal (process-get process 'project-root)
                        (plist-get tide368-test-world :root))
                 (eq (gethash (process-get process 'project-name) tide-servers)
                     process))
      (error "Tide configured process contract drifted: %S"
             (and process
                  (list :status (process-status process)
                        :command (process-command process)
                        :buffer (and (process-buffer process)
                                     (buffer-name (process-buffer process)))
                        :coding (process-coding-system process)
                        :filter (process-filter process)
                        :sentinel (process-sentinel process)
                        :query (process-query-on-exit-flag process)))))
    (process-put process 'tide368-contract-validated t)
    process))

(defun tide368-test-tide-start-server (original &rest arguments)
  (prog1 (apply original arguments)
    (tide368-test-assert-current-server)))

(defun tide368-test-exchange-for-ordinal (ordinal)
  (cl-loop
   for session in (tide368-test-json-value
                   "sessions" (plist-get tide368-test-world :config-data))
   thereis
   (cl-find-if
    (lambda (exchange)
      (= (tide368-test-json-value "ordinal" exchange) ordinal))
    (tide368-test-json-value "exchanges" session))))

(defun tide368-test-tide-send-command (original name args &optional callback)
  ;; `tide-sync-buffer-contents' may recursively send reload before the outer
  ;; command allocates its id.  Carry the real call's arguments dynamically;
  ;; the exact allocator observer below pairs them with the actual ordinal.
  (let ((tide368-test-current-send-command
         (list :command name :callback callback)))
    (funcall original name args callback)))

(defun tide368-test-next-request-id (original &rest arguments)
  (unless tide368-test-current-send-command
    (error "Tide allocated a request outside tide-send-command"))
  (let* ((value (apply original arguments))
         (ordinal (and (stringp value)
                       (string-match-p "\\`[1-9][0-9]*\\'" value)
                       (string-to-number value)))
         (exchange (and ordinal
                        (tide368-test-exchange-for-ordinal ordinal)))
         (command (plist-get tide368-test-current-send-command :command))
         (callback (plist-get tide368-test-current-send-command :callback))
         (policy (and exchange
                      (tide368-test-json-value "callback_policy" exchange)))
         (request (and exchange
                       (tide368-test-runtime-config
                        (encode-coding-string
                         (tide368-test-json-value "request" exchange)
                         'utf-8-unix t))))
         (expected-command
          (and request (tide368-test-json-value "command" request)))
         (actual-policy (if callback "registered" "not-registered")))
    (unless (and ordinal exchange (stringp command)
                 (equal command expected-command)
                 (equal policy actual-policy))
      (error "Tide callback/request allocation drifted: %S"
             (list :ordinal ordinal :command command
                   :expected-command expected-command
                   :callback actual-policy :expected-callback policy)))
    (push (list :ordinal ordinal :command command
                :callback (intern actual-policy))
          tide368-test-callback-policy-ledger)
    value))

(defun tide368-test-make-process (original &rest arguments)
  (unless (= tide368-test-approved-start-depth 1)
    (error "Tide attempted an unapproved make-process: %S" arguments))
  (unless (and (equal (plist-get arguments :name) "tsserver")
               (buffer-live-p (plist-get arguments :buffer))
               (equal (plist-get arguments :command)
                      (list (plist-get tide368-test-world :adapter)
                            (plist-get tide368-test-world :server)
                            "--disableAutomaticTypingAcquisition")))
    (error "Tide nested make-process drifted: %S" arguments))
  (when tide368-test-make-process-result
    (error "Tide nested make-process ran more than once"))
  (let ((process (apply original arguments)))
    (unless (processp process)
      (error "Tide make-process returned a non-process: %S" process))
    ;; Claim immediately: every later validation failure still leaves cleanup
    ;; with the exact process object it must contain and reap.
    (process-put process 'tide368-session-index tide368-test-planned-session-index)
    (process-put process 'tide368-session-plan tide368-test-planned-session)
    (process-put process 'tide368-initial-buffer (plist-get arguments :buffer))
    (push process tide368-test-owned-processes)
    (setq tide368-test-make-process-result process)))

(defun tide368-test-delete-process (original process)
  (let ((owned (and (processp process)
                    (memq process tide368-test-owned-processes))))
    (when tide368-test-public-phase
      (unless (and owned
                   (memq tide368-test-public-delete-route
                         '(kill-server restart-server server-list-kill)))
        (error "Tide rejected an unscoped public delete: %S"
               (list :process process :owned owned
                     :route tide368-test-public-delete-route)))
      (unless (tide368-test-terminal-record
               (process-get process 'tide368-session-index))
        (error "Tide public delete preceded its READY ledger: %S" process))
      (process-put process 'tide368-public-delete t)
      (process-put process 'tide368-ready-trace-size
                   (file-attribute-size
                    (file-attributes (plist-get tide368-test-world :trace)))))
    (prog1 (funcall original process)
      (when tide368-test-public-phase
        (tide368-test-validate-public-kill process)
        (push (list :session (process-get process 'tide368-session-index)
                    :route tide368-test-public-delete-route)
              tide368-test-public-delete-ledger)))))

(defun tide368-test-validate-public-kill (process)
  (unless (and (processp process)
               (process-get process 'tide368-public-delete))
    (error "Tide process was not killed through a public route: %S" process))
  (tide368-test-wait-process-dead process)
  (unless (= (or (process-get process 'tide368-ready-trace-size) -1)
             (file-attribute-size
              (file-attributes (plist-get tide368-test-world :trace))))
    (error "Tide public kill changed the READY trace: %S" process))
  (process-put process 'tide368-public-kill-validated t)
  t)

(defun tide368-test-tide-kill-server (original &rest arguments)
  (let ((tide368-test-public-delete-route
         (or tide368-test-public-delete-route 'kill-server)))
    (apply original arguments)))

(defun tide368-test-tide-restart-server (original &rest arguments)
  ;; The nested public tide-kill-server observer preserves this outer route.
  (let ((tide368-test-public-delete-route 'restart-server))
    (apply original arguments)))

(defun tide368-test-server-list-kill-server (original &rest arguments)
  (let ((tide368-test-public-delete-route 'server-list-kill))
    (apply original arguments)))

(defun tide368-test-net-sentinel (original process message)
  (let ((record
         (list :session (process-get process 'tide368-session-index)
               :status (process-status process) :exit (process-exit-status process)
               :message (tide368-test-normalize-string message)
               :stderr
               (tide368-test-normalize-string
                (if (buffer-live-p (process-buffer process))
                    (with-current-buffer (process-buffer process)
                      (buffer-substring-no-properties (point-min) (point-max)))
                  "")))))
    (prog1 (funcall original process message)
      (push record tide368-test-process-terminals))))

(defun tide368-test-install-observers ()
  (dolist (symbol tide368-test-forbidden-external-functions)
    (let ((fn (apply-partially #'tide368-test-forbidden-external symbol)))
      (advice-add symbol :override fn)
      (push (cons symbol fn) tide368-test-external-advices)))
  (advice-add 'start-file-process :around #'tide368-test-start-file-process)
  (push (cons 'start-file-process #'tide368-test-start-file-process)
        tide368-test-external-advices)
  (advice-add 'make-process :around #'tide368-test-make-process)
  (push (cons 'make-process #'tide368-test-make-process)
        tide368-test-external-advices)
  (advice-add 'delete-process :around #'tide368-test-delete-process)
  (push (cons 'delete-process #'tide368-test-delete-process)
        tide368-test-external-advices)
  (advice-add 'tide-start-server :around #'tide368-test-tide-start-server)
  (push (cons 'tide-start-server #'tide368-test-tide-start-server)
        tide368-test-external-advices)
  (advice-add 'tide-send-command :around #'tide368-test-tide-send-command)
  (push (cons 'tide-send-command #'tide368-test-tide-send-command)
        tide368-test-external-advices)
  (advice-add 'tide-next-request-id :around #'tide368-test-next-request-id)
  (push (cons 'tide-next-request-id #'tide368-test-next-request-id)
        tide368-test-external-advices)
  (advice-add 'tide-kill-server :around #'tide368-test-tide-kill-server)
  (push (cons 'tide-kill-server #'tide368-test-tide-kill-server)
        tide368-test-external-advices)
  (advice-add 'tide-restart-server :around #'tide368-test-tide-restart-server)
  (push (cons 'tide-restart-server #'tide368-test-tide-restart-server)
        tide368-test-external-advices)
  (advice-add 'tide--server-list-kill-server :around
              #'tide368-test-server-list-kill-server)
  (push (cons 'tide--server-list-kill-server
              #'tide368-test-server-list-kill-server)
        tide368-test-external-advices)
  (advice-add 'tide-net-sentinel :around #'tide368-test-net-sentinel)
  (push (cons 'tide-net-sentinel #'tide368-test-net-sentinel)
        tide368-test-external-advices))

(defun tide368-test-remove-observers ()
  (let (survivors errors)
    (dolist (entry tide368-test-external-advices)
      (condition-case condition
          (progn
            (advice-remove (car entry) (cdr entry))
            (when (advice-member-p (cdr entry) (car entry))
              (push entry survivors)))
        (t (push (list entry (tide368-test-condition-state condition)) errors)
           (push entry survivors))))
    (setq tide368-test-external-advices survivors)
    (when (or survivors errors)
      (error "Tide observer cleanup failed: %S" (list survivors errors)))))

(defun tide368-test-read-json-lines (path)
  (let ((bytes (if (file-exists-p path)
                   (let ((coding-system-for-read 'utf-8-unix))
                     (with-temp-buffer
                       (insert-file-contents path)
                       (buffer-string)))
                 "")))
    (unless (or (string-empty-p bytes) (string-suffix-p "\n" bytes))
      (error "Tide JSON ledger lacks final newline: %S" path))
    (mapcar (lambda (line)
              (json-parse-string line :object-type 'alist :array-type 'list
                                 :null-object nil :false-object :json-false))
            (split-string bytes "\n" t))))

(defun tide368-test-json-hash-value (key object)
  (gethash key object))

(defun tide368-test-json-field (object field &optional replacement replace-p)
  (let ((current object))
    (dolist (segment (butlast field))
      (setq current
            (cond ((and (stringp segment) (hash-table-p current))
                   (let ((sentinel (make-symbol "missing")))
                     (let ((value (gethash segment current sentinel)))
                       (when (eq value sentinel)
                         (error "Tide missing response field: %S" field))
                       value)))
                  ((and (integerp segment) (arrayp current)
                        (<= 0 segment) (< segment (length current)))
                   (aref current segment))
                  ((and (stringp segment) (listp current))
                   (let ((entry (assq (intern segment) current)))
                     (unless entry
                       (error "Tide missing ordered response field: %S" field))
                     (cdr entry)))
                  (t (error "Tide invalid response field path: %S" field)))))
    (let ((leaf (car (last field))))
      (cond ((and (stringp leaf) (hash-table-p current))
             (let ((sentinel (make-symbol "missing")))
               (let ((value (gethash leaf current sentinel)))
                 (when (eq value sentinel)
                   (error "Tide missing response field leaf: %S" field))
                 (when replace-p (puthash leaf replacement current))
                 value)))
            ((and (integerp leaf) (arrayp current)
                  (<= 0 leaf) (< leaf (length current)))
             (prog1 (aref current leaf)
               (when replace-p (aset current leaf replacement))))
            ((and (stringp leaf) (listp current))
             (let ((entry (assq (intern leaf) current)))
               (unless entry
                 (error "Tide missing ordered response field leaf: %S" field))
               (prog1 (cdr entry)
                 (when replace-p (setcdr entry replacement)))))
            (t (error "Tide invalid response field leaf: %S" field))))))

(defun tide368-test-expanded-frame (frame)
  (let* ((path (tide368-test-owner-path
                (tide368-test-json-value "path" frame)))
         (raw (with-temp-buffer
                (set-buffer-multibyte nil)
                (insert-file-contents-literally path)
                (buffer-string)))
         (marker (string-match "\r\n\r\n" raw))
         (body (and marker (substring raw (+ marker 4))))
         (trailing-newline (and body (string-suffix-p "\n" body)))
         (json-bytes (and body
                          (if trailing-newline
                              (substring body 0 -1)
                            body)))
         (parsed (and json-bytes
                      (json-parse-string
                       (decode-coding-string json-bytes 'utf-8-unix)
                       :object-type 'alist :array-type 'array
                       :null-object :json-null :false-object :json-false)))
         (canonical-json
          (and parsed
               (encode-coding-string
                (json-serialize parsed :null-object :json-null
                                :false-object :json-false)
                'utf-8-unix t))))
    (unless (and marker
                 (not (string-match "\r\n\r\n" raw (+ marker 4)))
                 (string-match-p
                  (format "\\`Content-Length: %d\\'" (length body))
                  (substring raw 0 marker))
                 (equal canonical-json json-bytes)
                 (equal (tide368-test-bytes-sha256 raw)
                        (tide368-test-json-value "sha256" frame)))
      (error "Tide recorded frame drifted before terminal gate: %S" path))
    (dolist (token (tide368-test-json-value "tokens" frame))
      (let* ((field (tide368-test-json-value "field" token))
             (kind (tide368-test-json-value "kind" token))
             (actual (tide368-test-json-field parsed field))
             (replacement
              (pcase kind
                ("root-path"
                 (directory-file-name
                  (tide368-test-project-path
                   (tide368-test-json-value "relative" token))))
                ("project-root"
                 (directory-file-name (plist-get tide368-test-world :root)))
                ("embedded-root-path"
                 (concat
                  (tide368-test-json-value "prefix" token)
                  (directory-file-name
                   (tide368-test-project-path
                    (tide368-test-json-value "relative" token)))
                  (tide368-test-json-value "suffix" token)))
                ("project-id"
                 (tide368-test-bytes-sha256
                  (encode-coding-string
                   (directory-file-name
                    (tide368-test-project-path
                     (tide368-test-json-value "relative" token)))
                   'utf-8-unix t)))
                ("tsserver-bundled-path"
                 (let ((relative (tide368-test-json-value "relative" token)))
                   (unless (equal actual
                                  (concat "[TSSERVER-DIR]/" relative))
                     (error "Tide bundled response token drifted: %S" token))
                   (tide368-test-tsserver-bundled-path relative)))
                ("tsserver-path" (plist-get tide368-test-world :server))
                (_ (error "Tide invalid response token kind: %S" kind)))))
        (tide368-test-json-field parsed field replacement t)))
    (let* ((expanded-body
            (concat
             (encode-coding-string
              (json-serialize parsed :null-object :json-null
                              :false-object :json-false)
              'utf-8-unix t)
             (if trailing-newline "\n" "")))
           (header (encode-coding-string
                    (format "Content-Length: %d\r\n\r\n" (length expanded-body))
                    'us-ascii t)))
      (concat header expanded-body))))

(defun tide368-test-terminal-keys (record expected)
  (equal (sort (mapcar (lambda (entry)
                         (let ((key (car entry)))
                           (if (symbolp key) (symbol-name key) key)))
                       record)
               #'string<)
         (sort (copy-sequence expected) #'string<)))

(defun tide368-test-validate-trace ()
  (let* ((sessions (tide368-test-json-value
                    "sessions" (plist-get tide368-test-world :config-data)))
         (records (tide368-test-read-json-lines
                   (plist-get tide368-test-world :trace)))
         (cursor records))
    (cl-loop
     for session in sessions for session-index from 1 do
     (let* ((exchanges (tide368-test-json-value "exchanges" session))
            (termination (tide368-test-json-value "termination" session))
            (kind (tide368-test-json-value "kind" termination))
            (start (pop cursor)) (emitted "") (frame-count 0))
       (unless (and start
                    (tide368-test-terminal-keys
                     start '("event" "session" "first_ordinal"
                             "interpreter" "interpreter_match"
                             "interpreter_sha256" "tsserver" "tsserver_match"
                             "tsserver_sha256"))
                    (equal (tide368-test-json-value "event" start) "START")
                    (= (tide368-test-json-value "session" start) session-index)
                    (= (tide368-test-json-value "first_ordinal" start)
                       (tide368-test-json-value "first_ordinal" session))
                    (equal (tide368-test-json-value "interpreter" start)
                           "[INTERPRETER]")
                    (eq (tide368-test-json-value "interpreter_match" start) t)
                    (equal (tide368-test-json-value "interpreter_sha256" start)
                           (tide368-test-json-value
                            "sha256"
                            (tide368-test-json-value
                             "interpreter"
                             (plist-get tide368-test-world :config-data))))
                    (equal (tide368-test-json-value "tsserver" start)
                           "[TSSERVER]")
                    (eq (tide368-test-json-value "tsserver_match" start) t)
                    (equal (tide368-test-json-value "tsserver_sha256" start)
                           (tide368-test-json-value
                            "sha256"
                            (tide368-test-json-value
                             "tsserver"
                             (plist-get tide368-test-world :config-data)))))
         (error "Tide START trace drifted: %S" start))
       (dolist (exchange exchanges)
         (let ((ordinal (tide368-test-json-value "ordinal" exchange))
               (request (pop cursor)))
           (unless (and request
                        (tide368-test-terminal-keys
                         request '("event" "session" "ordinal" "json"))
                        (equal (tide368-test-json-value "event" request) "request")
                        (= (tide368-test-json-value "session" request) session-index)
                        (= (tide368-test-json-value "ordinal" request) ordinal)
                        (equal (tide368-test-json-value "json" request)
                               (tide368-test-json-value "request" exchange)))
             (error "Tide request trace drifted: %S" request))
           (let ((coalesced-next nil))
             (dolist (due exchanges)
               (when (and (tide368-test-json-value "delivery_after" due)
                          (= (tide368-test-json-value "delivery_after" due)
                             ordinal))
                 (let ((owner (tide368-test-json-value "ordinal" due)))
                   (dolist (frame (tide368-test-json-value "frames" due))
                     (let* ((value (tide368-test-expanded-frame frame))
                            (record (pop cursor))
                            (mode (tide368-test-json-value
                                   "mode" (tide368-test-json-value
                                           "delivery" frame)))
                            (delivery
                             (if coalesced-next "coalesced-tail"
                               (if (equal mode "coalesced") "coalesced" mode))))
                       (setq coalesced-next (equal mode "coalesced")
                             emitted (concat emitted value)
                             frame-count (1+ frame-count))
                       (unless
                           (and record
                                (tide368-test-terminal-keys
                                 record '("event" "session" "request" "sha256"
                                          "delivery_after" "owner" "delivery"
                                          "emitted_bytes" "emitted_sha256"))
                                (equal (tide368-test-json-value "event" record)
                                       "frame")
                                (= (tide368-test-json-value "session" record)
                                   session-index)
                                (= (tide368-test-json-value "request" record) owner)
                                (= (tide368-test-json-value
                                    "delivery_after" record) ordinal)
                                (equal (tide368-test-json-value "owner" record)
                                       (tide368-test-json-value "owner" frame))
                                (equal (tide368-test-json-value "sha256" record)
                                       (tide368-test-json-value "sha256" frame))
                                (equal (tide368-test-json-value "delivery" record)
                                       delivery)
                                (= (tide368-test-json-value
                                    "emitted_bytes" record) (length value))
                                (equal (tide368-test-json-value
                                        "emitted_sha256" record)
                                       (tide368-test-bytes-sha256 value)))
                         (let ((root (plist-get tide368-test-world :root)))
                           (error
                            "Tide frame trace drifted: %S"
                            (list :record record
                                  :expected-bytes (length value)
                                  :expected-sha256
                                  (tide368-test-bytes-sha256 value)
                                  :root root :root-length (length root)
                                  :root-bytes (string-bytes root)
                                  :root-multibyte
                                  (multibyte-string-p root)
                                  :value-multibyte
                                  (multibyte-string-p value))))))))))
             (when coalesced-next
               (error "Tide coalescing crossed delivery boundary %S" ordinal)))))
       (let* ((terminal (pop cursor))
              (event (tide368-test-terminal-event kind))
              (last (car (last exchanges)))
              (keys (append
                     '("event" "session" "request" "requests" "frames"
                               "bytes" "request_sha256" "emitted_sha256")
                     (and (equal kind "exit-after") '("code")))))
         (unless (and terminal
                      (tide368-test-terminal-keys terminal keys)
                      (equal (tide368-test-json-value "event" terminal) event)
                      (= (tide368-test-json-value "session" terminal) session-index)
                      (= (tide368-test-json-value "request" terminal)
                         (tide368-test-json-value "ordinal" last))
                      (= (tide368-test-json-value "requests" terminal)
                         (tide368-test-json-value "request_count" session))
                      (= (tide368-test-json-value "frames" terminal) frame-count)
                      (= (tide368-test-json-value "bytes" terminal) (length emitted))
                      (equal (tide368-test-json-value "request_sha256" terminal)
                             (tide368-test-json-value
                              "request_stream_sha256" session))
                      (equal (tide368-test-json-value "emitted_sha256" terminal)
                             (tide368-test-bytes-sha256 emitted))
                      (or (not (equal kind "exit-after"))
                          (= (tide368-test-json-value "code" terminal)
                             (tide368-test-json-value "code" termination))))
           (error "Tide terminal trace drifted: %S" terminal)))))
    (when cursor (error "Tide trace has trailing records: %S" cursor))
    t))

(defun tide368-test-session-terminal-kind (session)
  (tide368-test-json-value
   "kind" (tide368-test-json-value "termination" session)))

(defun tide368-test-terminal-event (kind)
  (pcase kind
    ("clean-eof" "DONE") ("client-killed" "READY")
    ("exit-after" "EXPECTED_EXIT")
    (_ (error "Tide unknown terminal kind: %S" kind))))

(defun tide368-test-terminal-record (session-index)
  (let* ((records (tide368-test-read-json-lines
                   (plist-get tide368-test-world :trace)))
         (session (nth (1- session-index)
                       (tide368-test-json-value
                        "sessions" (plist-get tide368-test-world :config-data))))
         (expected (tide368-test-terminal-event
                    (tide368-test-session-terminal-kind session))))
    (seq-find (lambda (record)
                (and (= (or (tide368-test-json-value "session" record) 0)
                        session-index)
                     (equal (tide368-test-json-value "event" record) expected)))
              records)))

(defun tide368-test-expected-process-terminal (session-index session)
  (let* ((termination (tide368-test-json-value "termination" session))
         (kind (tide368-test-json-value "kind" termination)))
    (pcase kind
      ("clean-eof"
       (list :session session-index :status 'exit :exit 0
             :message "finished\n" :stderr "\n"))
      ("client-killed"
       (list :session session-index :status 'signal :exit 9
             :message "killed\n" :stderr "\n"))
      ("exit-after"
       (let ((code (tide368-test-json-value "code" termination)))
         (list :session session-index :status 'exit :exit code
               :message (format "exited abnormally with code %d\n" code)
               :stderr "\nTIDE368 expected external exit\n")))
      (_ (error "Tide unknown process terminal kind: %S" kind)))))

(defun tide368-test-validate-process-terminals ()
  (let* ((sessions (tide368-test-json-value
                    "sessions" (plist-get tide368-test-world :config-data)))
         (actual (reverse tide368-test-process-terminals))
         (expected
          (cl-loop for session in sessions for index from 1
                   collect (tide368-test-expected-process-terminal index session))))
    (unless (and (= (length actual) (length sessions))
                 (equal (mapcar (lambda (record) (plist-get record :session))
                                actual)
                        (number-sequence 1 (length sessions)))
                 (equal actual expected))
      (error "Tide real sentinel terminal ledger drifted: %S"
             (list :expected expected :actual actual)))
    (copy-tree actual)))

(defun tide368-test-validate-public-delete-ledger ()
  (let* ((sessions (tide368-test-json-value
                    "sessions" (plist-get tide368-test-world :config-data)))
         (expected-sessions
          (cl-loop for session in sessions for index from 1
                   when (equal (tide368-test-session-terminal-kind session)
                               "client-killed")
                   collect index))
         (actual (reverse tide368-test-public-delete-ledger)))
    (unless (and (equal (mapcar (lambda (entry) (plist-get entry :session))
                                actual)
                        expected-sessions)
                 (cl-every
                  (lambda (entry)
                    (memq (plist-get entry :route)
                          '(kill-server restart-server server-list-kill)))
                  actual))
      (error "Tide public delete route ledger drifted: %S"
             (list :expected-sessions expected-sessions :actual actual)))
    t))

(defun tide368-test-validate-callback-policy-ledger ()
  (let* ((sessions (tide368-test-json-value
                    "sessions" (plist-get tide368-test-world :config-data)))
         (expected
          (cl-loop
           for session in sessions append
           (mapcar
            (lambda (exchange)
              (let* ((ordinal (tide368-test-json-value "ordinal" exchange))
                     (request
                      (tide368-test-runtime-config
                       (encode-coding-string
                        (tide368-test-json-value "request" exchange)
                        'utf-8-unix t))))
                (list :ordinal ordinal
                      :command (tide368-test-json-value "command" request)
                      :callback
                      (intern
                       (tide368-test-json-value
                        "callback_policy" exchange)))))
            (tide368-test-json-value "exchanges" session))))
         (actual (reverse tide368-test-callback-policy-ledger)))
    (unless (equal actual expected)
      (error "Tide callback policy ledger drifted: %S"
             (list :expected expected :actual actual)))
    (copy-tree actual)))

(defun tide368-test-wait-process-dead (process)
  (let ((deadline (+ (float-time) 5.0)) (stable 0) previous)
    (while (and (< (float-time) deadline) (< stable 3))
      (accept-process-output process 0.02)
      (let ((state
             (list (process-status process) (process-exit-status process)
                   (process-live-p process)
                   (and (buffer-live-p (process-buffer process))
                        (eq (get-buffer-process (process-buffer process)) process))
                   (gethash (process-get process 'project-name) tide-servers))))
        (if (and (not (process-live-p process)) (equal state previous))
            (setq stable (1+ stable))
          (setq stable 0 previous state))))
    (unless (and (not (process-live-p process)) (= stable 3))
      (error "Tide process did not settle: %S" process))))

(defun tide368-test-force-process-disposal (process)
  (when (processp process)
    (let (disposal-error)
      (condition-case condition
          (progn
            (set-process-query-on-exit-flag process nil)
            (when (process-live-p process) (delete-process process)))
        (t (setq disposal-error condition)))
      (let ((deadline (+ (float-time) 2.0)))
        (while (and (process-live-p process) (< (float-time) deadline))
          (accept-process-output process 0.02)))
      (when (process-live-p process)
        (error "Tide process survived forced disposal: %S" process))
      (when disposal-error
        (signal (car disposal-error) (cdr disposal-error))))))

(defun tide368-test-finish-process (process)
  (when (processp process)
    (let (finish-error disposal-error)
      (condition-case condition
          (let* ((session-index (process-get process 'tide368-session-index))
                 (session (process-get process 'tide368-session-plan))
                 (kind (tide368-test-session-terminal-kind session)))
            (pcase kind
              ("clean-eof"
               (when (process-live-p process) (process-send-eof process)))
              ("client-killed"
               (unless (tide368-test-terminal-record session-index)
                 (error "Tide public-kill session lacks READY: %S" session-index))
               (unless (and (process-get process 'tide368-public-kill-validated)
                            (not (process-live-p process)))
                 (error "Tide ClientKilled lacked public dead/trace-stable proof: %S"
                        session-index)))
              ("exit-after" nil))
            (tide368-test-wait-process-dead process)
            (unless (tide368-test-terminal-record session-index)
              (error "Tide session lacks typed terminal witness: %S" session-index)))
        (t (setq finish-error condition)))
      ;; A typed finish failure must never strand its child.  Disposal has its
      ;; own condition slot so it cannot silently replace the first failure.
      (condition-case condition
          (tide368-test-force-process-disposal process)
        (t (setq disposal-error condition)))
      (cond ((and finish-error disposal-error)
             (error "Tide finish and disposal both failed: %S"
                    (list (tide368-test-condition-state finish-error)
                          (tide368-test-condition-state disposal-error))))
            (finish-error (signal (car finish-error) (cdr finish-error)))
            (disposal-error
             (signal (car disposal-error) (cdr disposal-error)))))))

(defun tide368-test-disable-owned-modes (buffers-before)
  (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
    (when (tide368-test-owned-buffer-p buffer buffers-before)
      (with-current-buffer buffer
        (when (bound-and-true-p tide-hl-identifier-mode)
          (tide-hl-identifier-mode -1))
        (when (bound-and-true-p flycheck-mode) (flycheck-mode -1))
        (when (bound-and-true-p tide-mode)
          ;; All public close requests belong in the typed body transcript.
          ;; At teardown, disarm local integrations without generating a new
          ;; request after the terminal ledger has closed.
          (remove-hook 'after-save-hook 'tide-sync-buffer-contents t)
          (remove-hook 'after-save-hook 'tide-auto-compile-file t)
          (remove-hook 'after-change-functions 'tide-handle-change t)
          (remove-hook 'kill-buffer-hook 'tide-cleanup-buffer t)
          (remove-hook 'kill-buffer-hook 'tide-schedule-dead-projects-cleanup t)
          (remove-hook 'hack-local-variables-hook
                       'tide-configure-buffer-if-server-exists t)
          (remove-hook 'xref-backend-functions #'xref-tide-xref-backend t)
          (setq tide-mode nil))))))

(defun tide368-test-remove-owned-temp-files (buffers-before)
  (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
    (when (tide368-test-owned-buffer-p buffer buffers-before)
      (with-current-buffer buffer
        (when (and (boundp 'tide-buffer-tmp-file) tide-buffer-tmp-file)
          (let* ((path tide-buffer-tmp-file)
                 (tmp (file-name-as-directory
                       (file-truename (plist-get tide368-test-world :tmp)))))
            (unless (and (file-name-absolute-p path) (file-regular-p path)
                         (not (file-symlink-p path))
                         (equal (file-name-directory path) tmp)
                         (string-match-p "\\`tide.+\\'"
                                         (file-name-nondirectory path)))
              (error "Tide refuses temp cleanup: %S" path))
            (delete-file path)
            (setq tide-buffer-tmp-file nil)))))))

(defun tide368-test-allowed-new-buffer-p (buffer)
  (let ((name (buffer-name buffer)) (file (buffer-file-name buffer)))
    (or (and name (tide368-test-output-buffer-name-p name))
        (and name (string-prefix-p " *Minibuf-" name))
        (and file tide368-test-world
             (string-prefix-p (plist-get tide368-test-world :root)
                              (expand-file-name file)))
        (and file tide368-test-world
             (let* ((expanded (expand-file-name file))
                    (server (plist-get tide368-test-world :server))
                    (base (directory-file-name (file-name-directory server)))
                    (relative (file-relative-name expanded base)))
               (and (equal relative (file-name-nondirectory relative))
                    (condition-case nil
                        (equal expanded
                               (tide368-test-tsserver-bundled-path relative))
                      (error nil))))))))

(defun tide368-test-owned-buffer-p (buffer buffers-before)
  (and (buffer-live-p buffer)
       (not (memq buffer buffers-before))
       (tide368-test-allowed-new-buffer-p buffer)))

(defun tide368-test-validate-replay-cleanup ()
  (let* ((sessions (tide368-test-json-value
                    "sessions" (plist-get tide368-test-world :config-data)))
         (ledger-lines (with-temp-buffer
                         (insert-file-contents-literally
                          (plist-get tide368-test-world :ledger))
                         (buffer-string)))
         (tmp-files
          (seq-remove
           (lambda (path)
             (member (file-name-nondirectory (directory-file-name path))
                     '("." "..")))
           (directory-files (plist-get tide368-test-world :tmp) t nil t))))
    (unless (and (= (length tide368-test-owned-processes) (length sessions))
                 (cl-every
                  (lambda (pair)
                    (let ((process (car pair)) (index (cdr pair)))
                      (and (not (process-live-p process))
                           (= (process-get process 'tide368-session-index) index)
                           (process-get process 'tide368-contract-validated))))
                  (cl-mapcar #'cons (reverse tide368-test-owned-processes)
                             (number-sequence 1 (length sessions))))
                 (equal ledger-lines (format "%d\n" (length sessions)))
                 (string-empty-p
                  (with-temp-buffer
                    (insert-file-contents-literally
                     (plist-get tide368-test-world :miss))
                    (buffer-string)))
                 (null tmp-files)
                 (zerop (hash-table-count tide-response-callbacks))
                 (zerop (hash-table-count tide-event-listeners))
                 (zerop (hash-table-count tide-servers))
                 (zerop (hash-table-count tide-project-configs))
                 (zerop (hash-table-count tide-tsserver-unsupported-commands))
                 (tide368-test-validate-trace)
                 (tide368-test-validate-process-terminals)
                 (tide368-test-validate-callback-policy-ledger)
                 (tide368-test-validate-public-delete-ledger))
      (error "Tide replay cleanup ledger is not clean: %S"
             (list :sessions (length sessions)
                   :processes (length tide368-test-owned-processes)
                   :process-states
                   (mapcar
                    (lambda (process)
                      (list :live (process-live-p process)
                            :status (process-status process)
                            :session
                            (process-get process 'tide368-session-index)
                            :contract
                            (process-get process 'tide368-contract-validated)))
                    (reverse tide368-test-owned-processes))
                   :ledger ledger-lines :tmp tmp-files
                   :callbacks (hash-table-count tide-response-callbacks)
                   :listeners (hash-table-count tide-event-listeners)
                   :servers (hash-table-count tide-servers)
                   :configs (hash-table-count tide-project-configs)
                   :unsupported
                   (hash-table-count tide-tsserver-unsupported-commands))))
    (dotimes (index (length sessions))
      (unless (tide368-test-terminal-record (1+ index))
        (error "Tide missing terminal session record: %S" (1+ index))))))

(defun tide368-test-read-diagnostic-bytes (path)
  (if (and path (file-exists-p path))
      (let ((coding-system-for-read 'utf-8-unix))
        (with-temp-buffer
          (insert-file-contents path)
          (tide368-test-normalize-string (buffer-string))))
    ""))

(defun tide368-test-replay-diagnostics ()
  ;; Copy every external witness before output buffers and the owner disappear.
  ;; Failure reporting retains the raw normalized ledgers; successful snapshots
  ;; expose the independently validated real-sentinel ledger only.
  (list :trace (tide368-test-read-diagnostic-bytes
                (plist-get tide368-test-world :trace))
        :miss (tide368-test-read-diagnostic-bytes
               (plist-get tide368-test-world :miss))
        :terminals (reverse (copy-tree tide368-test-process-terminals))
        :callbacks (reverse (copy-tree tide368-test-callback-policy-ledger))
        :public-deletes
        (reverse (copy-tree tide368-test-public-delete-ledger))))

(defun tide368-test-kill-buffer (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (bound-and-true-p tide-mode)
        (remove-hook 'after-save-hook 'tide-sync-buffer-contents t)
        (remove-hook 'after-save-hook 'tide-auto-compile-file t)
        (remove-hook 'after-change-functions 'tide-handle-change t)
        (setq tide-mode nil))
      ;; These are post-baseline disposable buffers.  No teardown hook may
      ;; manufacture an unplanned request or prevent containment after a body
      ;; failure; ambient baseline buffers never enter this function.
      (setq kill-buffer-hook nil kill-buffer-query-functions nil)
      (set-buffer-modified-p nil))
    (unless (kill-buffer buffer)
      (error "Tide buffer resisted forced disposal: %S" buffer))
    (when (buffer-live-p buffer)
      (error "Tide buffer survived forced disposal: %S" buffer))))

(defun tide368-test-run (scenario typed-summary artifacts thunk)
  (tide368-test-stabilize-batch-frame)
  (let* ((tide368-test-world nil)
         (tide368-test-external-advices nil)
         (tide368-test-process-events nil)
         (tide368-test-owned-processes nil)
         (tide368-test-parked-buffers nil)
         (tide368-test-approved-start-depth 0)
         (tide368-test-public-delete-route nil)
         (tide368-test-public-delete-ledger nil)
         (tide368-test-process-terminals nil)
         (tide368-test-current-send-command nil)
         (tide368-test-callback-policy-ledger nil)
         (print-circle nil)
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (idle-before (copy-sequence timer-idle-list))
         (buffer-before (current-buffer))
         (window-before (selected-window))
         (configuration-before (current-window-configuration))
         (windows-before (tide368-test-window-state))
         (default-directory-before
          (tide368-test-local-variable-state 'default-directory buffer-before))
         (coding-before
          (tide368-test-local-variable-state
           'buffer-file-coding-system buffer-before))
         (mark-ring-before
          (tide368-test-local-variable-state 'mark-ring buffer-before))
         (code-conversions-before
          (mapcar
           (lambda (name)
             (tide368-test-buffer-content-state (get-buffer name)))
           '(" *code-conversion-work*" " *code-converting-work*")))
         (states-before
          (mapcar (lambda (symbol)
                    (cons symbol (tide368-test-variable-state symbol)))
                  tide368-test-state-symbols))
         body-value body-error cleanup-errors owner-gone replay-diagnostics)
    (unwind-protect
        (condition-case condition
            (progn
              (setq tide368-test-world (tide368-test-allocate-world scenario))
              (tide368-test-materialize artifacts)
              (tide368-test-park-output-buffers)
              (tide368-test-configure)
              (tide368-test-install-observers)
              (let ((tide368-test-public-phase t))
                (setq body-value (funcall thunk tide368-test-world))))
          (t (setq body-error (tide368-test-condition-state condition))))
      (setq cleanup-errors
            (tide368-test-attempt
             'disable-owned-modes
             (lambda () (tide368-test-disable-owned-modes buffers-before))
             cleanup-errors))
      (setq cleanup-errors
            (tide368-test-attempt
             'remove-owned-temp-files
             (lambda () (tide368-test-remove-owned-temp-files buffers-before))
             cleanup-errors))
      (let ((index 0))
        (dolist (timer (tide368-test-new-timers timers-before idle-before))
          (setq cleanup-errors
                (tide368-test-attempt
                 (list 'cancel-timer index)
                 (lambda () (cancel-timer timer)) cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (process (reverse tide368-test-owned-processes))
          (setq cleanup-errors
                (tide368-test-attempt
                 (list 'finish-owned-process index)
                 (lambda () (tide368-test-finish-process process))
                 cleanup-errors))
          (setq index (1+ index))))
      (setq cleanup-errors
            (tide368-test-attempt
             'capture-replay-diagnostics
             (lambda ()
               (setq replay-diagnostics (tide368-test-replay-diagnostics)))
             cleanup-errors))
      (setq cleanup-errors
            (tide368-test-attempt
             'validate-replay #'tide368-test-validate-replay-cleanup
             cleanup-errors))
      ;; First complete sweep: by now no subject work may still be live.
      (dotimes (pass 2)
        (let ((index 0))
          (dolist (timer (tide368-test-new-timers timers-before idle-before))
            (setq cleanup-errors
                  (tide368-test-attempt
                   (list 'cancel-timer pass index)
                   (lambda () (cancel-timer timer)) cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (process (seq-difference (process-list) processes-before #'eq))
            (setq cleanup-errors
                  (tide368-test-attempt
                   (list 'reap-process pass index)
                   (lambda ()
                     (let ((owned (memq process tide368-test-owned-processes)))
                       (tide368-test-force-process-disposal process)
                       (unless owned
                         (error "Unexpected Tide process was disposed: %S"
                                process))))
                   cleanup-errors))
            (setq index (1+ index))))
        (let ((index 0))
          (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
            (setq cleanup-errors
                  (tide368-test-attempt
                   (list 'kill-buffer pass index (buffer-name buffer))
                   (lambda ()
                     (let ((allowed (tide368-test-allowed-new-buffer-p buffer))
                           (name (buffer-name buffer)))
                       (tide368-test-kill-buffer buffer)
                       (unless allowed
                         (error "Unexpected Tide buffer was disposed: %S" name))))
                   cleanup-errors))
            (setq index (1+ index)))))
      (dolist (entry (reverse tide368-test-parked-buffers))
        (setq cleanup-errors
              (tide368-test-attempt
               (list 'restore-output-buffer (plist-get entry :name))
               (lambda () (tide368-test-restore-output-buffer entry))
               cleanup-errors)))
      (dolist (entry states-before)
        (setq cleanup-errors
              (tide368-test-attempt
               (list 'restore-variable (car entry))
               (lambda () (tide368-test-restore-variable (car entry) (cdr entry)))
               cleanup-errors)))
      (dolist (entry `((default-directory . ,default-directory-before)
                       (buffer-file-coding-system . ,coding-before)
                       (mark-ring . ,mark-ring-before)))
        (setq cleanup-errors
              (tide368-test-attempt
               (list 'restore-local-variable (car entry))
               (lambda ()
                 (tide368-test-restore-local-variable
                  (car entry) buffer-before (cdr entry)))
               cleanup-errors)))
      (setq cleanup-errors
            (tide368-test-attempt
             'restore-windows
             (lambda ()
               (tide368-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      ;; Restoration may schedule resources or mutate display state. Sweep and
      ;; restore once more before dropping external guards.
      (let ((index 0))
        (dolist (timer (tide368-test-new-timers timers-before idle-before))
          (setq cleanup-errors
                (tide368-test-attempt
                 (list 'restore-reaction-timer index)
                 (lambda () (cancel-timer timer)) cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (process (seq-difference (process-list) processes-before #'eq))
          (setq cleanup-errors
                (tide368-test-attempt
                 (list 'restore-reaction-process index)
                 (lambda ()
                   (let ((owned (memq process tide368-test-owned-processes)))
                     (tide368-test-force-process-disposal process)
                     (unless owned
                       (error "Tide restore process was disposed: %S" process))))
                 cleanup-errors))
          (setq index (1+ index))))
      (let ((index 0))
        (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
          (setq cleanup-errors
                (tide368-test-attempt
                 (list 'restore-reaction-buffer index (buffer-name buffer))
                 (lambda ()
                   (let ((allowed (tide368-test-allowed-new-buffer-p buffer))
                         (name (buffer-name buffer)))
                     (tide368-test-kill-buffer buffer)
                     (unless allowed
                       (error "Tide restore buffer was disposed: %S" name))))
                 cleanup-errors))
          (setq index (1+ index))))
      (setq cleanup-errors
            (tide368-test-attempt
             'restore-windows-after-second-sweep
             (lambda ()
               (tide368-test-restore-windows configuration-before windows-before)
               (select-window window-before) (set-buffer buffer-before))
             cleanup-errors))
      (setq cleanup-errors
            (tide368-test-attempt
             'remove-observers #'tide368-test-remove-observers cleanup-errors))
      (when tide368-test-world
        (setq cleanup-errors
              (tide368-test-attempt
               'delete-owner
               (lambda ()
                 (let* ((owner (plist-get tide368-test-world :owner))
                        (sandbox (plist-get tide368-test-world :sandbox))
                        (true-owner (and (file-exists-p owner)
                                         (file-name-as-directory
                                          (file-truename owner)))))
                   (when true-owner
                     (unless (and (tide368-test-direct-child-p owner sandbox)
                                  (string-prefix-p sandbox true-owner))
                       (error "Tide refuses owner deletion: %S"
                              (list sandbox owner true-owner)))
                     (delete-directory owner t))
                   (setq owner-gone (not (file-exists-p owner)))))
               cleanup-errors)))
        (setq cleanup-errors
              (tide368-test-attempt
               'restore-code-conversion-after-owner
               (lambda ()
                 (dolist (state code-conversions-before)
                   (tide368-test-restore-buffer-content state)))
               cleanup-errors)))
    (setq cleanup-errors (nreverse cleanup-errors))
    (let* ((variable-mismatches
            (delq nil
                  (mapcar
                   (lambda (entry)
                     (unless (tide368-test-variable-restored-p
                              (car entry) (cdr entry))
                       (car entry)))
                   states-before)))
           (cleanup
            (list :new-buffers (seq-difference (buffer-list) buffers-before #'eq)
                  :new-processes (seq-difference (process-list) processes-before #'eq)
                  :new-timers (tide368-test-new-timers timers-before idle-before)
                  :variables (null variable-mismatches)
                  :variable-mismatches variable-mismatches
                  :windows (equal (tide368-test-window-state) windows-before)
                  :window-drift
                  (let ((after (tide368-test-window-state)))
                    (unless (equal after windows-before)
                      (list :before windows-before :after after)))
                  :configuration
                  (compare-window-configurations
                   (current-window-configuration) configuration-before)
                  :buffer (eq (current-buffer) buffer-before)
                  :window (eq (selected-window) window-before)
                  :observers tide368-test-external-advices
                  :parked
                  (cl-every
                   (lambda (entry)
                     (and (eq (get-buffer (plist-get entry :name))
                              (plist-get entry :buffer))
                          (tide368-test-buffer-content-restored-p
                           (plist-get entry :state))))
                   tide368-test-parked-buffers)
                  :code-conversion
                  (cl-every #'tide368-test-buffer-content-restored-p
                            code-conversions-before)
                  :owner owner-gone :body-error body-error
                  :cleanup-errors cleanup-errors
                  :replay-diagnostics replay-diagnostics)))
      (unless (and (null (plist-get cleanup :new-buffers))
                   (null (plist-get cleanup :new-processes))
                   (null (plist-get cleanup :new-timers))
                   (plist-get cleanup :variables)
                   (plist-get cleanup :windows)
                   (plist-get cleanup :configuration)
                   (plist-get cleanup :buffer) (plist-get cleanup :window)
                   (null (plist-get cleanup :observers))
                   (plist-get cleanup :parked)
                   (plist-get cleanup :code-conversion)
                   (plist-get cleanup :owner)
                   (null body-error) (null cleanup-errors))
        (error "Tide workflow/cleanup failure: %S" cleanup))
      (list :result body-value :typed typed-summary
            :launches (nreverse tide368-test-process-events)
            :terminals (plist-get replay-diagnostics :terminals)
            :callbacks (plist-get replay-diagnostics :callbacks)
            :public-deletes (plist-get replay-diagnostics :public-deletes)
            :cleanup 'clean))))

(defun tide368-test-read-workflow-body (source)
  (unless (stringp source)
    (error "Tide workflow body source must be a string: %S" source))
  (with-temp-buffer
    (insert source)
    (goto-char (point-min))
    (check-parens)
    (goto-char (point-min))
    (let ((form (read (current-buffer))))
      (skip-chars-forward " \t\r\n")
      (unless (eobp)
        (error "Tide workflow body has trailing non-whitespace input at %d"
               (point)))
      (unless (and (consp form) (eq (car form) 'lambda))
        (error "Tide workflow body is not one lambda form: %S" form))
      form)))

(defun tide368-test-assert-workflow-body-reader-contract ()
  (let ((valid (tide368-test-read-workflow-body
                "  (lambda (world) (list world))\n\t")))
    (unless (and (consp valid) (eq (car valid) 'lambda))
      (error "Tide workflow body reader rejected its positive contract")))
  (dolist (invalid '("(lambda (world) world))"
                     "(lambda (world) world) nil"))
    (unless (let ((inhibit-message t)
                  (message-log-max nil))
              (condition-case nil
                  (progn (tide368-test-read-workflow-body invalid) nil)
                (error t)))
      (error "Tide workflow body reader accepted invalid input: %S" invalid))))

"####;

fn tide_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TIDE_MELPA_PIN, "tide.el")
        .expect("prepare exact shallow Tide source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare Tide's exact Dash dependency")
        .with_melpa_dependency(FLYCHECK_MELPA_PIN)
        .expect("prepare Tide's exact Flycheck dependency")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare Tide's exact s dependency")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn protocol_frame(body: &str, delivery: DeliveryPlan) -> ApprovedFrame {
    protocol_frame_with_tokens(body, delivery, Vec::new())
}

fn protocol_frame_with_tokens(
    body: &str,
    delivery: DeliveryPlan,
    tokens: Vec<ResponseToken>,
) -> ApprovedFrame {
    let bytes = format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes();
    let digest = Sha256Digest::of(&bytes);
    ApprovedFrame::new(bytes, digest, delivery, tokens).expect("valid typed Tide protocol frame")
}

fn assert_typed_contracts() {
    let preflight_runtime = ReplayRuntimeIdentity::preflight()
        .expect("preflight the exact Tide replay interpreter and bundled tsserver");
    assert_eq!(
        preflight_runtime.tsserver.digest.hex(),
        PINNED_TSSERVER_SHA256,
    );
    assert_eq!(preflight_runtime.tsserver_bundle.len(), 47);
    assert_eq!(
        validate_tsserver_bundle_manifest(PINNED_TSSERVER_BUNDLE).unwrap(),
        preflight_runtime.tsserver_bundle,
    );
    let mut reordered_bundle = PINNED_TSSERVER_BUNDLE.to_vec();
    reordered_bundle.swap(0, 1);
    assert!(validate_tsserver_bundle_manifest(&reordered_bundle).is_err());
    let mut shortened_bundle = PINNED_TSSERVER_BUNDLE.to_vec();
    shortened_bundle.pop();
    assert!(validate_tsserver_bundle_manifest(&shortened_bundle).is_err());
    let mut changed_bundle = PINNED_TSSERVER_BUNDLE.to_vec();
    changed_bundle[0].1 = "089c0703923150aa30673fa3de411346d727cc44a11c75d05d7cf9ef095daa22";
    assert!(validate_tsserver_bundle_manifest(&changed_bundle).is_err());
    for invalid in [
        "/lib.es5.d.ts",
        "../lib.es5.d.ts",
        "./lib.es5.d.ts",
        "lib.es5.d.ts/child",
        "lib\\es5.d.ts",
        "not-in-the-pinned-manifest.d.ts",
    ] {
        assert!(TsserverRelativePath::new(invalid).is_err(), "{invalid:?}");
    }
    assert!(RequestOrdinal::new(0).is_err());
    assert!(LineOffset::new(0, 1).is_err());
    assert!(LineOffset::new(1, 0).is_err());
    for invalid in [
        "/ambient/main.js",
        "../outside.js",
        "./src/main.js",
        "src/./main.js",
        "src//main.js",
        "src\\main.js",
    ] {
        assert!(WorkspaceRelativePath::new(invalid).is_err(), "{invalid:?}");
    }

    let before_move = FixtureGeneration::new(vec![
        FixtureExpectation::Present {
            path: WorkspaceRelativePath::new("src/old.js").unwrap(),
            digest: Sha256Digest::of(b"old"),
        },
        FixtureExpectation::Missing(WorkspaceRelativePath::new("src/new.js").unwrap()),
    ])
    .unwrap();
    let after_move = FixtureGeneration::new(vec![
        FixtureExpectation::Missing(WorkspaceRelativePath::new("src/old.js").unwrap()),
        FixtureExpectation::Present {
            path: WorkspaceRelativePath::new("src/new.js").unwrap(),
            digest: Sha256Digest::of(b"old"),
        },
    ])
    .unwrap();
    assert!(FixtureGeneration::one_of(vec![before_move.clone()]).is_err());
    assert!(
        FixtureGeneration::one_of(vec![
            before_move.clone(),
            FixtureGeneration::new(vec![FixtureExpectation::Missing(
                WorkspaceRelativePath::new("different.js").unwrap(),
            )])
            .unwrap(),
        ])
        .is_err()
    );
    assert!(FixtureGeneration::one_of(vec![before_move.clone(), before_move.clone()]).is_err());
    assert_eq!(
        FixtureGeneration::one_of(vec![before_move, after_move])
            .unwrap()
            .json()["one_of"]
            .as_array()
            .unwrap()
            .len(),
        2,
    );

    let workspace = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    assert!(OwnedAbsoluteRoot::new("relative", workspace.join("tmp/owned")).is_err());
    assert!(OwnedAbsoluteRoot::new(&workspace, "/tmp/ambient").is_err());
    assert!(OwnedAbsoluteRoot::new(&workspace, workspace.join("tmp")).is_err());
    assert!(OwnedAbsoluteRoot::new(&workspace, workspace.join("tmp/owned/../../escape")).is_err());
    let expected_root = workspace.join("tmp/tide-typed-contract");
    let owned_root = OwnedAbsoluteRoot::new(&workspace, &expected_root)
        .expect("a nonexistent repository tmp child is a valid Tide owner");
    assert_eq!(owned_root.0, expected_root);

    assert!(Sha256Digest::parse("not-a-digest").is_err());
    let server_digest =
        Sha256Digest::parse("708b584a9937448f5400b09817774823e6ae339000ddeabc0e7766dfa428793a")
            .expect("the pinned tsserver digest is valid");
    assert_eq!(
        server_digest.hex(),
        "708b584a9937448f5400b09817774823e6ae339000ddeabc0e7766dfa428793a"
    );

    let main = WorkspaceRelativePath::new("src/main.js").unwrap();
    let point = LineOffset::new(5, 22).unwrap();
    let configure = TsRequest::Configure(ConfigureRequest {
        file: main.clone(),
        host_info: HostInfoToken::normalized(),
        format: FormatOptions {
            tab_size: NonZeroUsize::new(2).unwrap(),
            indent_size: NonZeroUsize::new(2).unwrap(),
        },
        preferences: UserPreferences {
            include_module_exports: true,
            include_insert_text: true,
            allow_new_files: true,
            generate_return_in_doc_template: true,
        },
    });
    assert_eq!(
        TsRequest::Open(
            OpenRequest::manual(main.clone(), ScriptKind::JavaScript, String::new(),).unwrap()
        )
        .normalized_json(RequestOrdinal::new(1).unwrap()),
        r#"{"command":"open","seq":"1","arguments":{"file":"[ROOT]/src/main.js","scriptKindName":"JS","fileContent":""}}"#,
    );
    let config_path = WorkspaceRelativePath::new("jsconfig.json").unwrap();
    assert_eq!(
        TsRequest::Open(OpenRequest::inferred(config_path.clone()).unwrap())
            .normalized_json(RequestOrdinal::new(2).unwrap()),
        r#"{"command":"open","seq":"2","arguments":{"file":"[ROOT]/jsconfig.json"}}"#,
    );
    assert!(OpenRequest::inferred(main.clone()).is_err());
    assert!(OpenRequest::immediate(config_path, ScriptKind::JavaScript).is_err(),);
    assert_eq!(
        configure.normalized_json(RequestOrdinal::new(2).unwrap()),
        r#"{"command":"configure","seq":"2","arguments":{"hostInfo":"[HOSTINFO]","file":"[ROOT]/src/main.js","formatOptions":{"tabSize":2,"indentSize":2},"preferences":{"includeCompletionsForModuleExports":true,"includeCompletionsWithInsertText":true,"allowTextChangesInNewFiles":true,"generateReturnInDocTemplate":true}}}"#,
    );
    assert_eq!(
        TsRequest::NavTo(NavToRequest::new("add", main.clone(), false).unwrap())
            .normalized_json(RequestOrdinal::new(3).unwrap()),
        r#"{"command":"navto","seq":"3","arguments":{"file":"[ROOT]/src/main.js","searchValue":"add","maxResultCount":100,"currentFileOnly":false}}"#,
    );
    assert_eq!(
        TsRequest::ProjectErrors(ProjectErrorsRequest { file: main.clone() })
            .normalized_json(RequestOrdinal::new(4).unwrap()),
        r#"{"command":"geterrForProject","seq":"4","arguments":{"file":"[ROOT]/src/main.js","delay":0}}"#,
    );
    let peer = WorkspaceRelativePath::new("src/math.js").unwrap();
    let exact_requests = [
        (
            TsRequest::Open(OpenRequest::immediate(main.clone(), ScriptKind::JavaScript).unwrap()),
            r#"{"command":"open","seq":"5","arguments":{"file":"[ROOT]/src/main.js","scriptKindName":"JS"}}"#,
        ),
        (
            TsRequest::Status,
            r#"{"command":"status","seq":"6","arguments":null}"#,
        ),
        (
            TsRequest::ProjectInfo(ProjectInfoRequest {
                file: main.clone(),
                file_names: FileNameListRequest::Null,
            }),
            r#"{"command":"projectInfo","seq":"7","arguments":{"file":"[ROOT]/src/main.js","needFileNameList":null}}"#,
        ),
        (
            TsRequest::ProjectInfo(ProjectInfoRequest {
                file: main.clone(),
                file_names: FileNameListRequest::Include,
            }),
            r#"{"command":"projectInfo","seq":"8","arguments":{"file":"[ROOT]/src/main.js","needFileNameList":true}}"#,
        ),
        (
            TsRequest::QuickInfoFull(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"quickinfo-full","seq":"9","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::QuickInfo(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"quickinfo","seq":"10","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::SignatureHelp(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"signatureHelp","seq":"11","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::Definition(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"definition","seq":"12","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::NavTree(FileRequest { file: main.clone() }),
            r#"{"command":"navtree","seq":"13","arguments":{"file":"[ROOT]/src/main.js"}}"#,
        ),
        (
            TsRequest::References(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"references","seq":"14","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::Diagnostics(
                DiagnosticKind::Syntactic,
                FileRequest { file: main.clone() },
            ),
            r#"{"command":"syntacticDiagnosticsSync","seq":"15","arguments":{"file":"[ROOT]/src/main.js"}}"#,
        ),
        (
            TsRequest::Diagnostics(DiagnosticKind::Semantic, FileRequest { file: main.clone() }),
            r#"{"command":"semanticDiagnosticsSync","seq":"16","arguments":{"file":"[ROOT]/src/main.js"}}"#,
        ),
        (
            TsRequest::Diagnostics(
                DiagnosticKind::Suggestion,
                FileRequest { file: main.clone() },
            ),
            r#"{"command":"suggestionDiagnosticsSync","seq":"17","arguments":{"file":"[ROOT]/src/main.js"}}"#,
        ),
        (
            TsRequest::DocumentHighlights(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"documentHighlights","seq":"18","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22,"filesToSearch":["[ROOT]/src/main.js"]}}"#,
        ),
        (
            TsRequest::Format(
                RangeRequest::new(
                    main.clone(),
                    LineOffset::new(1, 1).unwrap(),
                    LineOffset::new(9, 58).unwrap(),
                )
                .unwrap(),
            ),
            r#"{"command":"format","seq":"19","arguments":{"file":"[ROOT]/src/main.js","line":1,"offset":1,"endLine":9,"endOffset":58}}"#,
        ),
        (
            TsRequest::OrganizeImports(FileRequest { file: main.clone() }),
            r#"{"command":"organizeImports","seq":"20","arguments":{"scope":{"type":"file","args":{"file":"[ROOT]/src/main.js"}}}}"#,
        ),
        (
            TsRequest::DocCommentTemplate(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"docCommentTemplate","seq":"21","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::Rename(PointRequest {
                file: main.clone(),
                point,
            }),
            r#"{"command":"rename","seq":"22","arguments":{"file":"[ROOT]/src/main.js","line":5,"offset":22}}"#,
        ),
        (
            TsRequest::FileRename(FileRenameRequest {
                old_file: peer,
                new_file: WorkspaceRelativePath::new("src/arithmetic 界.js").unwrap(),
            }),
            r#"{"command":"getEditsForFileRename","seq":"23","arguments":{"oldFilePath":"[ROOT]/src/math.js","newFilePath":"[ROOT]/src/arithmetic 界.js","file":"[ROOT]/src/math.js"}}"#,
        ),
        (
            TsRequest::Reload(ReloadRequest {
                file: main.clone(),
                temporary_file: TideTempFileToken::new(
                    main.clone(),
                    Sha256Digest::of(b"dirty source\n"),
                ),
            }),
            r#"{"command":"reload","seq":"24","arguments":{"file":"[ROOT]/src/main.js","tmpfile":"[TIDE-TMP]"}}"#,
        ),
        (
            TsRequest::Close(FileRequest { file: main.clone() }),
            r#"{"command":"close","seq":"25","arguments":{"file":"[ROOT]/src/main.js"}}"#,
        ),
    ];
    for (index, (request, expected)) in exact_requests.iter().enumerate() {
        assert_eq!(
            request.normalized_json(RequestOrdinal::new(index + 5).unwrap()),
            *expected,
        );
    }
    assert!(RangeRequest::new(main.clone(), LineOffset::new(9, 58).unwrap(), point).is_err());
    let contract_generation = FixtureGeneration::new(vec![FixtureExpectation::Present {
        path: WorkspaceRelativePath::new("jsconfig.json").unwrap(),
        digest: Sha256Digest::of(b"{}"),
    }])
    .unwrap();
    let reload_request = TsRequest::Reload(ReloadRequest {
        file: main.clone(),
        temporary_file: TideTempFileToken::new(main.clone(), Sha256Digest::of(b"dirty source\n")),
    });
    let reload_ack = protocol_frame(
        r#"{"seq":0,"type":"response","command":"reload","request_seq":"24","success":true}"#,
        DeliveryPlan::WholeFrame,
    );
    let reload_finished = protocol_frame(
        r#"{"seq":0,"type":"response","command":"reload","request_seq":"24","success":true,"body":{"reloadFinished":true}}"#,
        DeliveryPlan::WholeFrame,
    );
    let reload_exchange = RecordedExchange::new(
        RequestOrdinal::new(24).unwrap(),
        reload_request.clone(),
        contract_generation.clone(),
        ApprovedOutput::frames(
            RequestOrdinal::new(24).unwrap(),
            vec![reload_ack.clone(), reload_finished.clone()],
        )
        .unwrap(),
    )
    .expect("Tide retains both ignored reload responses in their exact wire order");
    assert_eq!(
        reload_exchange.callback_policy,
        CallbackPolicy::NotRegistered
    );
    assert_eq!(reload_exchange.output.frames_slice().len(), 2);
    assert!(
        RecordedExchange::new(
            RequestOrdinal::new(24).unwrap(),
            reload_request,
            contract_generation.clone(),
            ApprovedOutput::frames(RequestOrdinal::new(24).unwrap(), vec![reload_ack]).unwrap(),
        )
        .is_err()
    );
    let rename_generation = FixtureGeneration::new(vec![
        FixtureExpectation::Present {
            path: WorkspaceRelativePath::new("src/new-name.js").unwrap(),
            digest: Sha256Digest::of(b"renamed\n"),
        },
        FixtureExpectation::Missing(WorkspaceRelativePath::new("src/old-name.js").unwrap()),
    ])
    .unwrap();
    assert_eq!(rename_generation.0.len(), 1);
    assert_eq!(rename_generation.0[0].len(), 2);

    let response_body =
        r#"{"seq":0,"type":"response","command":"status","request_seq":"1","success":true}"#;
    let response_bytes = format!(
        "Content-Length: {}\r\n\r\n{response_body}",
        response_body.len()
    )
    .into_bytes();
    let response_digest = Sha256Digest::of(&response_bytes);
    assert!(
        ApprovedFrame::new(
            response_bytes.clone(),
            server_digest,
            DeliveryPlan::WholeFrame,
            Vec::new(),
        )
        .is_err()
    );
    let header_bytes = response_bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert!(
        ApprovedFrame::new(
            response_bytes.clone(),
            response_digest,
            DeliveryPlan::SplitHeader {
                at: NonZeroUsize::new(header_bytes).unwrap(),
            },
            Vec::new(),
        )
        .is_err()
    );
    assert!(
        ApprovedFrame::new(
            response_bytes.clone(),
            response_digest,
            DeliveryPlan::SplitBody {
                at: NonZeroUsize::new(response_body.len()).unwrap(),
            },
            Vec::new(),
        )
        .is_err()
    );
    assert!(ApprovedOutput::frames(RequestOrdinal::new(1).unwrap(), Vec::new()).is_err());
    let token_body = r#"{"seq":0,"type":"event","event":"telemetry","body":{"configFileName":"[ROOT]/jsconfig.json","display":"literal [ROOT]/jsconfig.json text"}}"#;
    let token_bytes =
        format!("Content-Length: {}\r\n\r\n{token_body}", token_body.len()).into_bytes();
    assert!(
        ApprovedFrame::new(
            token_bytes.clone(),
            Sha256Digest::of(&token_bytes),
            DeliveryPlan::WholeFrame,
            Vec::new(),
        )
        .is_err()
    );
    let owned_token = ResponseToken::root_path(
        vec![
            JsonPathSegment::Key("body"),
            JsonPathSegment::Key("configFileName"),
        ],
        WorkspaceRelativePath::new("jsconfig.json").unwrap(),
    );
    let token_frame =
        protocol_frame_with_tokens(token_body, DeliveryPlan::WholeFrame, vec![owned_token]);
    assert_eq!(token_frame.tokens.len(), 1);
    let bundled_body = r#"{"seq":0,"type":"event","event":"semanticDiag","body":{"file":"[TSSERVER-DIR]/lib.es5.d.ts","diagnostics":[]}}"#;
    let bundled_token = ResponseToken::tsserver_bundled(
        vec![JsonPathSegment::Key("body"), JsonPathSegment::Key("file")],
        TsserverRelativePath::new("lib.es5.d.ts").unwrap(),
    );
    assert!(matches!(
        bundled_token.kind,
        ResponseTokenKind::TsserverBundledPath(_)
    ));
    assert_eq!(
        bundled_token.json().get("kind").and_then(Value::as_str),
        Some("tsserver-bundled-path"),
    );
    let bundled_bytes = format!(
        "Content-Length: {}\r\n\r\n{bundled_body}",
        bundled_body.len()
    )
    .into_bytes();
    let bundled_digest = Sha256Digest::of(&bundled_bytes);
    assert!(
        ApprovedFrame::new(
            bundled_bytes.clone(),
            bundled_digest,
            DeliveryPlan::WholeFrame,
            Vec::new(),
        )
        .is_err()
    );
    assert!(
        ApprovedFrame::new(
            bundled_bytes.clone(),
            bundled_digest,
            DeliveryPlan::WholeFrame,
            vec![bundled_token],
        )
        .is_err()
    );
    let mismatched_bundle_token = ResponseToken::tsserver_bundled(
        vec![JsonPathSegment::Key("body"), JsonPathSegment::Key("file")],
        TsserverRelativePath::new("lib.dom.d.ts").unwrap(),
    );
    assert!(
        ApprovedFrame::new(
            bundled_bytes,
            bundled_digest,
            DeliveryPlan::WholeFrame,
            vec![mismatched_bundle_token],
        )
        .is_err()
    );
    let project_root_body =
        r#"{"seq":0,"type":"event","event":"telemetry","body":{"projectRoot":"[ROOT]"}}"#;
    let project_root_frame = protocol_frame_with_tokens(
        project_root_body,
        DeliveryPlan::WholeFrame,
        vec![ResponseToken::project_root(vec![
            JsonPathSegment::Key("body"),
            JsonPathSegment::Key("projectRoot"),
        ])],
    );
    assert!(matches!(
        project_root_frame.tokens[0].kind,
        ResponseTokenKind::ProjectRoot
    ));
    let embedded_body = r#"{"seq":0,"type":"event","event":"projectLoadingStart","body":{"reason":"Creating possible configured project for [ROOT]/src/main.js to open","documentation":"literal [ROOT]/src/main.js remains text"}}"#;
    let embedded_frame = protocol_frame_with_tokens(
        embedded_body,
        DeliveryPlan::WholeFrame,
        vec![ResponseToken::embedded_root_path(
            vec![JsonPathSegment::Key("body"), JsonPathSegment::Key("reason")],
            RecordedLiteral::new("Creating possible configured project for ").unwrap(),
            WorkspaceRelativePath::new("src/main.js").unwrap(),
            RecordedLiteral::new(" to open").unwrap(),
        )],
    );
    assert!(matches!(
        embedded_frame.tokens[0].kind,
        ResponseTokenKind::EmbeddedRootPath { .. }
    ));
    assert!(RecordedLiteral::new("literal [ROOT]/src/main.js").is_err());
    let capture_config = WorkspaceRelativePath::new("jsconfig.json").unwrap();
    let project_id_field = vec![
        JsonPathSegment::Key("body"),
        JsonPathSegment::Key("payload"),
        JsonPathSegment::Key("projectId"),
    ];
    let project_name_field = vec![
        JsonPathSegment::Key("body"),
        JsonPathSegment::Key("projectName"),
    ];
    let capture_loading_body = r#"{"seq":0,"type":"event","event":"projectLoadingStart","body":{"projectName":"[ROOT]/jsconfig.json","reason":"Creating possible configured project for [ROOT]/src/main.js to open"}}"#;
    let capture_telemetry_body = r#"{"seq":0,"type":"event","event":"telemetry","body":{"telemetryEventName":"projectInfo","payload":{"projectId":"[PROJECT-ID]","fileStats":{"js":2,"jsSize":721,"jsx":0,"jsxSize":0,"ts":0,"tsSize":0,"tsx":0,"tsxSize":0,"dts":47,"dtsSize":1744378,"deferred":0,"deferredSize":0},"compilerOptions":{"allowJs":true,"maxNodeModuleJsDepth":2,"allowSyntheticDefaultImports":true,"skipLibCheck":true,"noEmit":true,"checkJs":true,"strict":true,"target":"es2020","module":"commonjs"},"typeAcquisition":{"enable":true,"include":false,"exclude":false},"extends":false,"files":true,"include":false,"exclude":false,"compileOnSave":false,"configFileName":"jsconfig.json","projectType":"configured","languageServiceEnabled":true,"version":"5.1.3"}}}"#;
    let captured_template = |body: &str| {
        let body = format!("{body}\n");
        format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    };
    let project_loading_tokens = vec![
        ResponseToken::root_path(project_name_field.clone(), capture_config.clone()),
        ResponseToken::embedded_root_path(
            vec![JsonPathSegment::Key("body"), JsonPathSegment::Key("reason")],
            RecordedLiteral::new("Creating possible configured project for ").unwrap(),
            WorkspaceRelativePath::new("src/main.js").unwrap(),
            RecordedLiteral::new(" to open").unwrap(),
        ),
    ];
    let telemetry_tokens = vec![ResponseToken::project_id(
        project_id_field.clone(),
        capture_config.clone(),
    )];
    let capture_evidence = CaptureProjectIdEvidence::new(
        "L2hvbWUvZXhlYy9Qcm9qZWN0cy9naXRodWIuY29tL2V2YWwtZXhlYy9uZW9tYWNzLXdpbmRvd3MvdG1wL3RpZGUtc3R1ZHkvcmVhbC1qcy1wcm9qZWN0IHNwYWNlIOeVjC9qc2NvbmZpZy5qc29u",
        "9d39a531fe14c923fe80bcd59e0f68d7f975ba9c3e050c7f285c49a9b14bc288",
        Sha256Digest::parse(
            "5fa8b5422f0d4da50ddd93840cc0ab79968bb3efcd2238471d7e3f6a7ecde673",
        )
        .unwrap(),
        Sha256Digest::parse(
            "9c583758c2e1e0ce32857f2852a1c4542dbff2a46f4abda2106aca4c0169cc42",
        )
        .unwrap(),
        capture_config.clone(),
        telemetry_tokens,
        project_loading_tokens,
        project_id_field,
        project_name_field,
    )
    .unwrap();
    let captured_frames = capture_evidence
        .ingest(
            captured_template(capture_loading_body),
            Sha256Digest::parse("7ed52fae3a82d53595f3c758386d47e0845effbd7dacb794f965159ece2697b6")
                .unwrap(),
            DeliveryPlan::WholeFrame,
            captured_template(capture_telemetry_body),
            Sha256Digest::parse("8f43b6e505712e274f96c5789926db49953c9843ae8bf64a3dd6c9e95152fbce")
                .unwrap(),
            DeliveryPlan::WholeFrame,
        )
        .unwrap();
    assert!(matches!(
        captured_frames.project_loading.kind,
        TsFrameKind::ProjectLoadingStart
    ));
    assert!(matches!(
        captured_frames.telemetry.tokens[0].kind,
        ResponseTokenKind::ProjectId(_)
    ));
    assert!(captured_frames.project_loading.bytes.ends_with(b"\n"));
    assert!(captured_frames.telemetry.bytes.ends_with(b"\n"));
    assert!(
        validate_captured_project_frame_roles(
            &captured_frames.telemetry,
            &captured_frames.project_loading,
        )
        .is_err()
    );
    assert!(
        ApprovedFrame::new(
            captured_frames.telemetry.bytes.clone(),
            captured_frames.telemetry.frame_digest,
            DeliveryPlan::WholeFrame,
            captured_frames.telemetry.tokens.clone(),
        )
        .is_err()
    );
    assert!(CaptureProjectIdEvidence::new(
        "L2hvbWUvZXhlYy9ub3QtY2Fub25pY2FsLw==",
        "9d39a531fe14c923fe80bcd59e0f68d7f975ba9c3e050c7f285c49a9b14bc288",
        Sha256Digest::parse(
            "5fa8b5422f0d4da50ddd93840cc0ab79968bb3efcd2238471d7e3f6a7ecde673",
        )
        .unwrap(),
        Sha256Digest::parse(
            "9c583758c2e1e0ce32857f2852a1c4542dbff2a46f4abda2106aca4c0169cc42",
        )
        .unwrap(),
        capture_config,
        Vec::new(),
        Vec::new(),
        vec![JsonPathSegment::Key("projectId")],
        vec![JsonPathSegment::Key("projectName")],
    )
    .is_err());
    assert!(
        ApprovedFrame::new(
            token_frame.bytes.clone(),
            token_frame.frame_digest,
            DeliveryPlan::WholeFrame,
            vec![ResponseToken::tsserver(vec![
                JsonPathSegment::Key("body"),
                JsonPathSegment::Key("configFileName"),
            ])],
        )
        .is_err()
    );
    let response = protocol_frame(response_body, DeliveryPlan::WholeFrame);
    assert_eq!(response.bytes, response_bytes);
    assert!(
        RecordedExchange::new(
            RequestOrdinal::new(2).unwrap(),
            TsRequest::Status,
            contract_generation.clone(),
            ApprovedOutput::frames(RequestOrdinal::new(2).unwrap(), vec![response.clone()])
                .unwrap(),
        )
        .is_err()
    );
    assert!(
        RecordedExchange::new(
            RequestOrdinal::new(1).unwrap(),
            TsRequest::ProjectInfo(ProjectInfoRequest {
                file: main.clone(),
                file_names: FileNameListRequest::Null,
            }),
            contract_generation.clone(),
            ApprovedOutput::frames(RequestOrdinal::new(1).unwrap(), vec![response]).unwrap(),
        )
        .is_err()
    );
    assert!(
        RecordedExchange::new(
            RequestOrdinal::new(1).unwrap(),
            TsRequest::Status,
            contract_generation.clone(),
            ApprovedOutput::no_frames(),
        )
        .is_err()
    );

    let open_output = ApprovedOutput::frames(
        RequestOrdinal::new(1).unwrap(),
        vec![
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"projectLoadingStart","body":{}}"#,
                DeliveryPlan::CoalescedWithNext,
            ),
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"projectLoadingFinish","body":{}}"#,
                DeliveryPlan::WholeFrame,
            ),
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"telemetry","body":{}}"#,
                DeliveryPlan::SplitHeader {
                    at: NonZeroUsize::new(5).unwrap(),
                },
            ),
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"configFileDiag","body":{}}"#,
                DeliveryPlan::SplitBody {
                    at: NonZeroUsize::new(3).unwrap(),
                },
            ),
        ],
    )
    .unwrap();
    let configure_output = ApprovedOutput::frames(
        RequestOrdinal::new(2).unwrap(),
        vec![protocol_frame(
            r#"{"seq":0,"type":"response","command":"configure","request_seq":"2","success":true}"#,
            DeliveryPlan::WholeFrame,
        )],
    )
    .unwrap();
    let project_errors_output = ApprovedOutput::frames(
        RequestOrdinal::new(3).unwrap(),
        vec![
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"syntaxDiag","body":{}}"#,
                DeliveryPlan::WholeFrame,
            ),
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"semanticDiag","body":{}}"#,
                DeliveryPlan::WholeFrame,
            ),
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"suggestionDiag","body":{}}"#,
                DeliveryPlan::WholeFrame,
            ),
            protocol_frame(
                r#"{"seq":0,"type":"event","event":"requestCompleted","body":{"request_seq":"3"}}"#,
                DeliveryPlan::WholeFrame,
            ),
        ],
    )
    .unwrap();
    let exchanges = vec![
        RecordedExchange::new(
            RequestOrdinal::new(1).unwrap(),
            TsRequest::Open(OpenRequest::immediate(main.clone(), ScriptKind::JavaScript).unwrap()),
            contract_generation.clone(),
            open_output,
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            RequestOrdinal::new(2).unwrap(),
            configure,
            contract_generation.clone(),
            configure_output,
        )
        .unwrap()
        .into(),
        RecordedExchange::new(
            RequestOrdinal::new(3).unwrap(),
            TsRequest::ProjectErrors(ProjectErrorsRequest { file: main }),
            contract_generation.clone(),
            project_errors_output,
        )
        .unwrap()
        .into(),
    ];
    let fixture_bytes = b"{}".to_vec();
    let fixture_digest = Sha256Digest::of(&fixture_bytes);
    assert!(
        FixtureFile::new(
            WorkspaceRelativePath::new("jsconfig.json").unwrap(),
            fixture_bytes.clone(),
            server_digest,
        )
        .is_err()
    );
    assert!(FixtureManifest::new(Vec::new()).is_err());
    let fixtures = FixtureManifest::new(vec![
        FixtureFile::new(
            WorkspaceRelativePath::new("jsconfig.json").unwrap(),
            fixture_bytes,
            fixture_digest,
        )
        .unwrap(),
    ])
    .unwrap();
    assert_eq!(fixtures.generation(), contract_generation);
    let request_stream_digest =
        Sha256Digest::parse("a4d791ab36cba5ece07b447c06f4f9cf34cb1c16d52dd60efc5eb9e4f9fc3c80")
            .unwrap();
    let first_session = ReplaySession::new(
        exchanges,
        request_stream_digest,
        Sha256Digest::parse("6e8bd9267aced027b72f09d32f636385fbf9680244d91f95338dcd10a4002125")
            .unwrap(),
        ReplayTermination::CleanEof,
    )
    .expect("sequential Tide replay session");
    let replay = TideReplay::new(
        TideScenario::Lifecycle,
        fixtures.clone(),
        vec![first_session.clone()],
    )
    .expect("single-process Tide scenario plan");
    let summary = replay.elisp_summary();
    assert!(summary.contains(":request-count 3 :frame-count 9"));
    assert!(summary.contains(":kind request-completed"));
    assert!(summary.contains(":delivery coalesced-with-next"));
    let plan = replay.artifacts(&preflight_runtime).elisp_plan();
    assert!(plan.contains(":adapter-base64"));
    assert!(plan.contains(":config-sha256"));
    assert!(plan.contains(":fixtures"));
    assert!(plan.contains(":frames"));

    let status_frame = |request, delivery| {
        protocol_frame(
            &format!(
                "{{\"seq\":0,\"type\":\"response\",\"command\":\"status\",\"request_seq\":\"{request}\",\"success\":true}}"
            ),
            delivery,
        )
    };
    ReplaySession::new(
        vec![
            RecordedExchange::new_delayed(
                RequestOrdinal::new(1).unwrap(),
                TsRequest::Status,
                contract_generation.clone(),
                ApprovedOutput::frames_delayed(
                    RequestOrdinal::new(2).unwrap(),
                    vec![status_frame(1, DeliveryPlan::CoalescedWithNext)],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
            RecordedExchange::new(
                RequestOrdinal::new(2).unwrap(),
                TsRequest::Status,
                contract_generation.clone(),
                ApprovedOutput::frames(
                    RequestOrdinal::new(2).unwrap(),
                    vec![status_frame(2, DeliveryPlan::WholeFrame)],
                )
                .unwrap(),
            )
            .unwrap()
            .into(),
        ],
        Sha256Digest::parse("15bd6c91ec9560575c9be70e342f41e21271e84c04b2480fedcc41b8a0227ebc")
            .unwrap(),
        Sha256Digest::parse("df68ddbb25604a463839f54abe0bde21cee69461c7e92b71680ac374e59bd7ea")
            .unwrap(),
        ReplayTermination::CleanEof,
    )
    .expect("adjacent same-boundary Tide outputs may coalesce across request owners");
    assert!(
        ReplaySession::new(
            vec![
                RecordedExchange::new_delayed(
                    RequestOrdinal::new(1).unwrap(),
                    TsRequest::Status,
                    contract_generation.clone(),
                    ApprovedOutput::frames_delayed(
                        RequestOrdinal::new(2).unwrap(),
                        vec![status_frame(1, DeliveryPlan::CoalescedWithNext)],
                    )
                    .unwrap(),
                )
                .unwrap()
                .into(),
                RecordedExchange::new_delayed(
                    RequestOrdinal::new(2).unwrap(),
                    TsRequest::Status,
                    contract_generation.clone(),
                    ApprovedOutput::frames_delayed(
                        RequestOrdinal::new(3).unwrap(),
                        vec![status_frame(2, DeliveryPlan::WholeFrame)],
                    )
                    .unwrap(),
                )
                .unwrap()
                .into(),
                RecordedExchange::new(
                    RequestOrdinal::new(3).unwrap(),
                    TsRequest::Status,
                    contract_generation.clone(),
                    ApprovedOutput::frames(
                        RequestOrdinal::new(3).unwrap(),
                        vec![status_frame(3, DeliveryPlan::WholeFrame)],
                    )
                    .unwrap(),
                )
                .unwrap()
                .into(),
            ],
            Sha256Digest::parse(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )
            .unwrap(),
            Sha256Digest::parse(
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            )
            .unwrap(),
            ReplayTermination::CleanEof,
        )
        .is_err()
    );

    let fourth_response = protocol_frame(
        r#"{"seq":0,"type":"response","command":"status","request_seq":"4","success":true}"#,
        DeliveryPlan::WholeFrame,
    );
    let killed_session = ReplaySession::new(
        vec![
            RecordedExchange::new(
                RequestOrdinal::new(4).unwrap(),
                TsRequest::Status,
                contract_generation.clone(),
                ApprovedOutput::frames(RequestOrdinal::new(4).unwrap(), vec![fourth_response])
                    .unwrap(),
            )
            .unwrap()
            .into(),
        ],
        Sha256Digest::of(b"{\"command\":\"status\",\"seq\":\"4\",\"arguments\":null}\n"),
        Sha256Digest::parse("edf626686a4cf6bc567783757135e2debdcc8e4b8bc96a4e2ff83ee6e2cd0997")
            .unwrap(),
        ReplayTermination::ClientKilled {
            ready_after: RequestOrdinal::new(4).unwrap(),
        },
    )
    .unwrap();
    TideReplay::new(
        TideScenario::Lifecycle,
        fixtures.clone(),
        vec![first_session.clone(), killed_session.clone()],
    )
    .expect("case-wide request ordinals continue across public server restarts");

    let terminal_session = ReplaySession::new(
        vec![
            TerminalExchange::new(
                RequestOrdinal::new(5).unwrap(),
                TsRequest::Status,
                contract_generation,
                ApprovedOutput::no_frames(),
            )
            .unwrap()
            .into(),
        ],
        Sha256Digest::of(b"{\"command\":\"status\",\"seq\":\"5\",\"arguments\":null}\n"),
        Sha256Digest::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
            .unwrap(),
        ReplayTermination::ExitAfter {
            request: RequestOrdinal::new(5).unwrap(),
            code: NonZeroI32::new(87).unwrap(),
        },
    )
    .unwrap();
    TideReplay::new(
        TideScenario::FailureRecovery,
        fixtures.clone(),
        vec![first_session.clone(), killed_session, terminal_session],
    )
    .expect("typed Tide process sessions own healthy kill and terminal external exit semantics");

    assert!(
        TideReplay::new(
            TideScenario::FailureRecovery,
            fixtures,
            vec![first_session, ReplaySession { first_ordinal: RequestOrdinal::new(5).unwrap(), ..ReplaySession::new(
                vec![RecordedExchange::new(
                    RequestOrdinal::new(5).unwrap(),
                    TsRequest::Status,
                    FixtureGeneration::new(vec![FixtureExpectation::Missing(
                        WorkspaceRelativePath::new("missing.js").unwrap(),
                    )]).unwrap(),
                    ApprovedOutput::frames(RequestOrdinal::new(5).unwrap(), vec![protocol_frame(
                        r#"{"seq":0,"type":"response","command":"status","request_seq":"5","success":true}"#,
                        DeliveryPlan::WholeFrame,
                    )]).unwrap(),
                ).unwrap().into()],
                Sha256Digest::of(b"{\"command\":\"status\",\"seq\":\"5\",\"arguments\":null}\n"),
                Sha256Digest::parse(
                    "d91ed0dd705adf25ffd44cc7b5c2e72c29c9ea0fa68cb11ffe0dd7a36017ca22",
                ).unwrap(),
                ReplayTermination::CleanEof,
            ).unwrap() }],
        )
        .is_err()
    );
}

#[test]
fn tide_package_batch() {
    assert_typed_contracts();
    assert_oracle_batch_cases(
        tide_oracle(),
        "tide-package-batch",
        "tide_parity",
        &workflows::workflow_batch_cases(),
    );
}
