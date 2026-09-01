use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnedGitWorktree(PathBuf);

impl OwnedGitWorktree {
    pub(crate) fn from_canonical_roots(
        workspace_root: PathBuf,
        discovered_git_root: &Path,
        workspace_manifests_are_tracked: bool,
    ) -> Option<Self> {
        (workspace_manifests_are_tracked && workspace_root == discovered_git_root)
            .then_some(Self(workspace_root))
    }

    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CargoProfileName<'a>(&'a str);

impl<'a> CargoProfileName<'a> {
    pub(crate) fn from_build_inputs(
        out_dir: &'a Path,
        explicitly_selected_profile: Option<&'a str>,
    ) -> Option<Self> {
        let artifact_directory = out_dir
            .ancestors()
            .nth(3)?
            .file_name()?
            .to_str()
            .filter(|name| !name.is_empty())?;

        if let Some(profile) = explicitly_selected_profile.filter(|profile| !profile.is_empty()) {
            let expected_artifact_directory = match profile {
                "dev" | "test" => "debug",
                "release" | "bench" => "release",
                custom => custom,
            };
            return (expected_artifact_directory == artifact_directory).then_some(Self(profile));
        }

        // Cargo exposes only the shared artifact directory for its built-in
        // aliases: dev/test use `debug`, and release/bench use `release`.
        // Never guess between them. Neomacs' build driver supplies the exact
        // selection; direct Cargo invocations without that hint report the
        // honest class. Custom profiles keep their exact directory/name.
        Some(Self(match artifact_directory {
            "debug" => "dev/test",
            "release" => "release/bench",
            custom => custom,
        }))
    }

    pub(crate) const fn as_str(self) -> &'a str {
        self.0
    }
}

pub(crate) fn tracked_top_level_entries(
    nul_separated_paths: &[u8],
) -> Result<BTreeSet<&str>, std::str::Utf8Error> {
    nul_separated_paths
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(std::str::from_utf8)
        .map(|path| {
            path.map(|path| {
                path.split_once('/')
                    .map_or(path, |(top_level, _)| top_level)
            })
        })
        .collect()
}
