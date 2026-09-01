use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{
    CommandError, EmacsRuntime, configure_process_environment, elisp_string, output_with_timeout,
    workspace_root,
};

#[derive(Clone, Copy)]
struct GrammarSourceSpec<'a> {
    language: &'a str,
    repository: &'a str,
    revision: &'a str,
    source_directory: Option<&'a str>,
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
        let root = workspace_root()
            .join("tmp/melpa/tree-sitter-grammar-cache")
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

    fn configure(&self, command: &mut Command) {
        configure_process_environment(command, &self.root, &self.home, &self.tmp);
    }
}

/// Build one exact Tree-sitter grammar into a cross-process cache below
/// `<workspace>/tmp/melpa/tree-sitter-grammar-cache`.
///
/// GNU Emacs performs the build through its native grammar installer so the
/// compiler and shared-library conventions match the host platform. The
/// returned directory can be added to `treesit-extra-load-path` by any editor
/// adapter using this shared preparation module.
pub fn prepare_cached_tree_sitter_grammar(
    gnu_emacs: &EmacsRuntime,
    language: &str,
    repository: &str,
    revision: &str,
) -> Result<PathBuf, String> {
    prepare_cached_tree_sitter_grammar_with_source_directory(
        gnu_emacs, language, repository, revision, None,
    )
}

/// Build one exact Tree-sitter grammar whose generated sources live below a
/// repository subdirectory.
pub fn prepare_cached_tree_sitter_grammar_from_subdirectory(
    gnu_emacs: &EmacsRuntime,
    language: &str,
    repository: &str,
    revision: &str,
    source_directory: &str,
) -> Result<PathBuf, String> {
    prepare_cached_tree_sitter_grammar_with_source_directory(
        gnu_emacs,
        language,
        repository,
        revision,
        Some(source_directory),
    )
}

fn prepare_cached_tree_sitter_grammar_with_source_directory(
    gnu_emacs: &EmacsRuntime,
    language: &str,
    repository: &str,
    revision: &str,
    source_directory: Option<&str>,
) -> Result<PathBuf, String> {
    let source = GrammarSourceSpec {
        language,
        repository,
        revision,
        source_directory,
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
    let cache_is_ready = grammar_library_exists(&paths.grammar_directory, language)
        && fs::read_to_string(&paths.ready_marker)
            .is_ok_and(|contents| contents == expected_marker);
    if cache_is_ready {
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

fn validate_grammar_source(source: GrammarSourceSpec<'_>) -> Result<(), String> {
    let GrammarSourceSpec {
        language,
        repository,
        revision,
        source_directory,
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
    {
        return Err(format!(
            "cached Tree-sitter grammar requires a safe language, GitHub repository, full revision, and optional source directory, got `{language}` `{repository}` `{revision}` `{source_directory:?}`"
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
        paths.configure(&mut command);
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
    let mut command = gnu_emacs.command();
    paths.configure(&mut command);
    command.args(["--batch", "--quick", "--eval", &form]);
    let output =
        output_with_timeout(&mut command, gnu_emacs.timeout).map_err(|error| match error {
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
        })?;
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
