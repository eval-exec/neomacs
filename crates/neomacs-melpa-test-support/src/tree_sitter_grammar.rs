//! Pinned Tree-sitter grammars: explicit prefetch, cache-only test reads.
//!
//! A grammar fixture is pinned by repository, full revision, and the SHA-256
//! of the generated `parser.c` that revision must contain. [`TreeSitterGrammarCase::prefetch`]
//! (driven by `cargo xtask treesit-grammars`) fetches, hash-verifies, and
//! builds the grammar through GNU Emacs's native installer; tests then read
//! the cache through [`TreeSitterGrammarCase::prepare`], which verifies the
//! marker, the library, and the hash on every use and never touches the
//! network -- a cold cache is a loud error naming the prefetch command, not a
//! clone attempt that can flake on a TLS hiccup.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod matrix;
pub use matrix::{
    EXTRA_TREE_SITTER_GRAMMAR_PINS, ExtraTreeSitterGrammarPin, GNU31_TREE_SITTER_GRAMMARS,
    GrammarSourceDirectory, TreeSitterGrammarCase, extra_tree_sitter_grammar_pin,
};

use crate::{
    CommandError, EmacsRuntime, elisp_string, output_with_timeout, output_with_timeout_pinned,
};
use neomacs_test_env::{CanonicalEnvironment, PinnedCommand};

#[derive(Clone, Copy)]
struct GrammarSourceSpec<'a> {
    language: &'a str,
    repository: &'a str,
    revision: &'a str,
    source_directory: Option<&'a str>,
    source_sha256: &'a str,
}

impl GrammarSourceSpec<'_> {
    /// The generated parser source this pin describes: the hash of these
    /// bytes is what makes a revision's content a pin, and it is
    /// host-independent, unlike the compiled library.
    fn parser_source(&self, checkout: &Path) -> PathBuf {
        checkout
            .join(self.source_directory.unwrap_or("src"))
            .join("parser.c")
    }
}

struct GrammarCachePaths {
    root: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
    checkout: PathBuf,
    grammar_directory: PathBuf,
    ready_marker: PathBuf,
}

impl GrammarCachePaths {
    fn for_source(source: GrammarSourceSpec<'_>) -> Self {
        Self::below(&neomacs_infra::melpa_cache_root(), source)
    }

    /// The same layout below an arbitrary cache root, so the cache-only
    /// reader stays testable against a temporary directory.
    fn below(root: &Path, source: GrammarSourceSpec<'_>) -> Self {
        let root = root
            .join("tree-sitter-grammar-cache")
            .join(source.language)
            .join(source.revision);
        let home = root.join("home");
        Self {
            tmp: root.join("tmp"),
            checkout: root.join("source"),
            grammar_directory: home.join(".emacs.d/tree-sitter"),
            ready_marker: root.join("ready"),
            root,
            home,
        }
    }

    /// The package sandbox's canonical environment for this cache root.
    fn environment(&self) -> CanonicalEnvironment {
        neomacs_infra::packages::install::package_environment_with_runtime(
            &self.root,
            &self.home,
            &self.tmp,
            &self.root.join("xdg/runtime"),
        )
        .expect("tree-sitter grammar cache roots must be absolute")
    }

    /// A canonically pinned command for the editor that builds the grammar.
    fn command(&self, runtime: &EmacsRuntime) -> PinnedCommand {
        let mut command = runtime.command(&self.environment());
        command.current_dir(&self.root);
        command
    }

    /// Same deterministic environment for the git helper that checks the
    /// grammar source out: helper output is host-independent too, and the
    /// helper receives this cache root's one canonical environment instead
    /// of a second, rebuilt copy that can drift from it.
    fn configure_helper(&self, command: &mut Command) {
        neomacs_infra::packages::configure_helper_process_environment(
            command,
            &self.environment(),
            &self.root,
        );
    }
}

/// Fetch, hash-verify, and build one exact Tree-sitter grammar into the
/// shared cache below `<melpa cache root>/tree-sitter-grammar-cache`.
///
/// GNU Emacs performs the build through its native grammar installer so the
/// compiler and shared-library conventions match the host platform. The
/// fetched `parser.c` must hash to `source_sha256` before anything is
/// installed or published; the marker is written only after the built
/// library is in place. [`cached_tree_sitter_grammar`] reads the result.
pub fn prefetch_cached_tree_sitter_grammar(
    gnu_emacs: &EmacsRuntime,
    language: &str,
    repository: &str,
    revision: &str,
    source_sha256: &str,
) -> Result<PathBuf, String> {
    prefetch_cached_tree_sitter_grammar_with_source_directory(
        gnu_emacs,
        language,
        repository,
        revision,
        None,
        source_sha256,
    )
}

/// [`prefetch_cached_tree_sitter_grammar`] for a grammar whose generated
/// sources live below a repository subdirectory.
pub fn prefetch_cached_tree_sitter_grammar_from_subdirectory(
    gnu_emacs: &EmacsRuntime,
    language: &str,
    repository: &str,
    revision: &str,
    source_directory: &str,
    source_sha256: &str,
) -> Result<PathBuf, String> {
    prefetch_cached_tree_sitter_grammar_with_source_directory(
        gnu_emacs,
        language,
        repository,
        revision,
        Some(source_directory),
        source_sha256,
    )
}

fn prefetch_cached_tree_sitter_grammar_with_source_directory(
    gnu_emacs: &EmacsRuntime,
    language: &str,
    repository: &str,
    revision: &str,
    source_directory: Option<&str>,
    source_sha256: &str,
) -> Result<PathBuf, String> {
    let source = GrammarSourceSpec {
        language,
        repository,
        revision,
        source_directory,
        source_sha256,
    };
    validate_grammar_source(source)?;
    let paths = GrammarCachePaths::for_source(source);

    fs::create_dir_all(&paths.root).map_err(|error| {
        format!(
            "failed to create Tree-sitter grammar cache root {}: {error}",
            paths.root.display()
        )
    })?;
    let lock_path = paths.root.join("prepare.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "failed to open Tree-sitter grammar cache lock {}: {error}",
                lock_path.display()
            )
        })?;
    fs4::FileExt::lock(&lock).map_err(|error| {
        format!(
            "failed to lock Tree-sitter grammar cache {}: {error}",
            paths.root.display()
        )
    })?;

    let source_directory_marker = source_directory.unwrap_or("");
    let expected_marker =
        format!("{language}\t{repository}\t{revision}\t{source_directory_marker}\n");
    if cache_entry_matches(&paths, source) {
        return Ok(paths.grammar_directory);
    }

    remove_incomplete_cache(&paths)?;
    for directory in [
        paths.home.join(".emacs.d"),
        paths.tmp.clone(),
        paths.root.join("xdg/config"),
        paths.root.join("xdg/cache"),
        paths.root.join("xdg/data"),
        paths.root.join("xdg/state"),
    ] {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create Tree-sitter grammar cache directory {}: {error}",
                directory.display()
            )
        })?;
    }

    prepare_grammar_checkout(gnu_emacs, source, &paths)?;
    verify_parser_source_hash(source, &paths)?;
    install_grammar(gnu_emacs, source, &paths)?;

    let marker_tmp = paths.root.join(format!("ready.{}.tmp", std::process::id()));
    fs::write(&marker_tmp, &expected_marker).map_err(|error| {
        format!(
            "failed to write Tree-sitter grammar cache marker {}: {error}",
            marker_tmp.display()
        )
    })?;
    fs::rename(&marker_tmp, &paths.ready_marker).map_err(|error| {
        format!(
            "failed to publish Tree-sitter grammar cache marker {}: {error}",
            paths.ready_marker.display()
        )
    })?;
    Ok(paths.grammar_directory)
}

/// Read an already-prefetched grammar and verify its pin.
///
/// This cache-only path checks the ready marker, the built library, and the
/// SHA-256 of the cached `parser.c` on every use, and never launches git,
/// GNU Emacs, or a network request. A missing or stale entry is an error
/// naming the prefetch command, so a cold cache fails loudly instead of
/// silently skipping or flaking on a clone.
pub fn cached_tree_sitter_grammar(
    language: &str,
    revision: &str,
    source_directory: Option<&str>,
    source_sha256: &str,
) -> Result<PathBuf, String> {
    cached_tree_sitter_grammar_in(
        &neomacs_infra::melpa_cache_root(),
        language,
        revision,
        source_directory,
        source_sha256,
    )
}

/// [`cached_tree_sitter_grammar`] below an arbitrary cache root.
pub(crate) fn cached_tree_sitter_grammar_in(
    cache_root: &Path,
    language: &str,
    revision: &str,
    source_directory: Option<&str>,
    source_sha256: &str,
) -> Result<PathBuf, String> {
    let source = GrammarSourceSpec {
        language,
        repository: "",
        revision,
        source_directory,
        source_sha256,
    };
    let paths = GrammarCachePaths::below(cache_root, source);
    let entry_present = grammar_library_exists(&paths.grammar_directory, source.language)
        && marker_names_this_pin(&paths, source);
    if !entry_present {
        return Err(format!(
            "pinned Tree-sitter grammar cache entry for `{language}` at revision {revision} is \
             missing or stale below {}; run `cargo xtask treesit-grammars` to prefetch it",
            neomacs_infra::melpa_cache_root().display()
        ));
    }
    // The entry claims to be this pin; hold it to the pin's bytes. A hash
    // mismatch means the cache holds different content than the pin
    // describes -- a louder diagnosis than "missing", and one a bare
    // re-prefetch will heal by removing the stale entry first.
    verify_parser_source_hash(source, &paths)?;
    Ok(paths.grammar_directory)
}

/// A cache entry is reusable only when its ready marker names exactly this
/// pin's language, revision, and source directory, the built library is
/// present, and the cached `parser.c` still hashes to the pin -- the cached
/// bytes are what the pin describes.
///
/// The marker's repository field is provenance, not identity: the cache
/// directory is already keyed by language and revision, and the `parser.c`
/// hash is what the content is verified against.
fn cache_entry_matches(paths: &GrammarCachePaths, source: GrammarSourceSpec<'_>) -> bool {
    grammar_library_exists(&paths.grammar_directory, source.language)
        && marker_names_this_pin(paths, source)
        && verify_parser_source_hash(source, paths).is_ok()
}

/// The ready marker names exactly this pin's language, revision, and source
/// directory. Only the line terminator is trimmed, never tabs: the final
/// field is the source directory, which is legitimately the empty string for
/// grammars whose `parser.c` sits at the repository root's `src/`.
fn marker_names_this_pin(paths: &GrammarCachePaths, source: GrammarSourceSpec<'_>) -> bool {
    fs::read_to_string(&paths.ready_marker).is_ok_and(|contents| {
        let mut fields = contents.trim_end_matches('\n').split('\t');
        matches!(fields.next(), Some(language) if language == source.language)
            && fields.next().is_some()
            && matches!(fields.next(), Some(revision) if revision == source.revision)
            && fields.next() == Some(source.source_directory.unwrap_or(""))
            && fields.next().is_none()
    })
}

/// The cached `parser.c` is the pinned bytes.
fn verify_parser_source_hash(
    source: GrammarSourceSpec<'_>,
    paths: &GrammarCachePaths,
) -> Result<(), String> {
    let parser_source = source.parser_source(&paths.checkout);
    let bytes = fs::read(&parser_source).map_err(|error| {
        format!(
            "failed to read generated parser source {}: {error}",
            parser_source.display()
        )
    })?;
    let actual = neomacs_infra::inventory::sha256_hex(&bytes);
    if actual == source.source_sha256 {
        Ok(())
    } else {
        Err(format!(
            "Tree-sitter grammar `{}` at revision {} has parser.c SHA-256 {actual}, expected {} \
             -- the cache no longer holds the pinned bytes",
            source.language, source.revision, source.source_sha256
        ))
    }
}

fn validate_grammar_source(source: GrammarSourceSpec<'_>) -> Result<(), String> {
    let GrammarSourceSpec {
        language,
        repository,
        revision,
        source_directory,
        source_sha256,
    } = source;
    let source_directory_is_safe = source_directory.is_none_or(|directory| {
        let path = Path::new(directory);
        !directory.is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_)))
    });
    if language.is_empty()
        || !language
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        || !repository.starts_with("https://github.com/")
        || revision.len() != 40
        || !revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || !source_directory_is_safe
        || source_sha256.len() != 64
        || !source_sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!(
            "cached Tree-sitter grammar requires a safe language, GitHub repository, full \
             revision, optional source directory, and parser.c SHA-256, got `{language}` \
             `{repository}` `{revision}` `{source_directory:?}` `{source_sha256}`"
        ));
    }
    Ok(())
}

fn remove_incomplete_cache(paths: &GrammarCachePaths) -> Result<(), String> {
    for directory in [&paths.home, &paths.tmp, &paths.checkout] {
        if directory.exists() {
            fs::remove_dir_all(directory).map_err(|error| {
                format!(
                    "failed to remove incomplete Tree-sitter grammar cache {}: {error}",
                    directory.display()
                )
            })?;
        }
    }
    if paths.ready_marker.exists() {
        fs::remove_file(&paths.ready_marker).map_err(|error| {
            format!(
                "failed to remove invalid Tree-sitter grammar marker {}: {error}",
                paths.ready_marker.display()
            )
        })?;
    }
    Ok(())
}

fn prepare_grammar_checkout(
    gnu_emacs: &EmacsRuntime,
    source: GrammarSourceSpec<'_>,
    paths: &GrammarCachePaths,
) -> Result<(), String> {
    let source_arg = paths.checkout.to_string_lossy().into_owned();
    let run_git = |arguments: &[&str]| -> Result<(), String> {
        let mut command = Command::new("git");
        paths.configure_helper(&mut command);
        command.args(arguments);
        let output =
            output_with_timeout(&mut command, gnu_emacs.timeout).map_err(|error| match error {
                CommandError::Launch(error) => format!(
                    "failed to launch git for cached Tree-sitter grammar `{}`: {error}",
                    source.language
                ),
                CommandError::TimedOut(_) => format!(
                    "git timed out while preparing cached Tree-sitter grammar `{}`",
                    source.language
                ),
                CommandError::Capture(error) => format!(
                    "failed to capture git output for cached Tree-sitter grammar `{}`: {error}",
                    source.language
                ),
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git failed while preparing cached Tree-sitter grammar `{}`\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
                source.language,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    };
    run_git(&["init", "--quiet", &source_arg])?;
    run_git(&[
        "-C",
        &source_arg,
        "remote",
        "add",
        "origin",
        source.repository,
    ])?;
    run_git(&[
        "-C",
        &source_arg,
        "fetch",
        "--quiet",
        "--depth",
        "1",
        "origin",
        source.revision,
    ])?;
    run_git(&[
        "-C",
        &source_arg,
        "checkout",
        "--quiet",
        "--detach",
        "FETCH_HEAD",
    ])
}

fn install_grammar(
    gnu_emacs: &EmacsRuntime,
    source: GrammarSourceSpec<'_>,
    paths: &GrammarCachePaths,
) -> Result<(), String> {
    let source_arg = paths.checkout.to_string_lossy().into_owned();
    let source_string = elisp_string(&source_arg);
    let grammar_recipe = match source.source_directory {
        Some(directory) => {
            let directory = elisp_string(directory);
            format!("({} {source_string} nil {directory})", source.language)
        }
        None => format!("({} {source_string})", source.language),
    };
    let form = format!(
        r##"(progn
               (require 'treesit)
               (setq user-emacs-directory
                     (file-name-as-directory
                      (expand-file-name ".emacs.d" (getenv "HOME")))
                     treesit-language-source-alist
                     '({grammar_recipe}))
               (treesit-install-language-grammar '{language})
               (unless (treesit-language-available-p '{language})
                 (error "Installed Tree-sitter grammar is unavailable: %s"
                        '{language}))
               (princ "NEOMACS-TREESIT-GRAMMAR-CACHE:ready"))"##,
        language = source.language,
    );
    let mut command = paths.command(gnu_emacs);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output =
        output_with_timeout_pinned(&mut command, gnu_emacs.timeout).map_err(
            |error| match error {
                CommandError::Launch(error) => format!(
                    "failed to launch {} for cached Tree-sitter grammar `{}` in {}: {error}",
                    gnu_emacs.name,
                    source.language,
                    paths.root.display()
                ),
                CommandError::TimedOut(_) => format!(
                    "{} cached Tree-sitter grammar `{}` timed out after {:?} in {}",
                    gnu_emacs.name,
                    source.language,
                    gnu_emacs.timeout,
                    paths.root.display()
                ),
                CommandError::Capture(error) => format!(
                    "failed to capture {} cached Tree-sitter grammar `{}` output: {error}",
                    gnu_emacs.name, source.language
                ),
            },
        )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success()
        || !stdout.contains("NEOMACS-TREESIT-GRAMMAR-CACHE:ready")
        || !grammar_library_exists(&paths.grammar_directory, source.language)
    {
        return Err(format!(
            "failed to prepare cached Tree-sitter grammar {} at {} below {}\nstatus: {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            source.language,
            source.revision,
            paths.root.display(),
            output.status.code()
        ));
    }
    Ok(())
}

fn grammar_library_exists(grammar_dir: &Path, language: &str) -> bool {
    let stem = format!("tree-sitter-{language}");
    fs::read_dir(grammar_dir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|file_type| file_type.is_file())
                && entry.file_name().to_string_lossy().contains(&stem)
        })
    })
}

#[cfg(test)]
mod tests;
