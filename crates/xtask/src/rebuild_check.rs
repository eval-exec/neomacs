//! Read-only staleness query for the Lisp tree (`fresh-build --check`).
//!
//! This is the GNU `make -q` analog: it answers "what would a byte-preserving
//! build recompile?" and builds nothing.  The predicates are the ones the
//! pipeline itself uses (`bytecode_needs_rebuild`, the compile-main dependency
//! closure, and `update-subdirs`' content comparison), so the report cannot
//! drift from the stages it describes.
//!
//! GNU models the answer with make's timestamp prerequisites
//! (`lisp/Makefile.in`'s `.el.elc` suffix rule, `leim/Makefile.in`'s
//! `leim-list.el` prerequisites, `admin/grammars/Makefile.in`'s grammar
//! rules).  The port additionally treats "a dependency's bytecode is newer
//! than mine" as staleness (`compile_main_dependency_bytecode_newer`) and
//! propagates a rebuild to dependents.  This module applies that model across
//! all four bytecode stages, in pipeline order:
//!
//! 1. `COMPILE_FIRST`,
//! 2. the `loadup.el` preloaded set (including the characters/Unicode deps),
//! 3. `MAIN_FIRST`,
//! 4. the rest of `compile-main`.
//!
//! A source that belongs to more than one list is attributed to the earliest
//! stage, because that stage compiles it first.
//!
//! Generated Lisp outputs that `fresh-build` currently regenerates
//! unconditionally (leim, semantic grammars, loaddefs, custom/finder data) are
//! NOT evaluated here; only `subdirs.el` is, because its content comparison is
//! exact and already implemented.  The report says so.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::{
    FreshBuildOptions, Result, bytecode_needs_rebuild, bytecode_needs_rebuild_with_dependencies,
    compile_main_dependency_bytecode_newer, compile_main_needs_rebuild, compile_main_sources,
    lisp_dirs_for_subdirs_update, parse_compile_first_sources, parse_compile_main_dependencies,
    parse_main_first_sources, parse_preloaded_lisp_sources, pipeline_paths,
    preloaded_characters_dependency_sources, update_subdirs_contents, update_subdirs_expression,
};

/// How many paths one stage prints before folding the rest into a count.
const MAX_LISTED: usize = 10;

#[derive(Debug)]
pub(crate) struct StageReport {
    pub(crate) label: &'static str,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) total: usize,
}

impl StageReport {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            files: Vec::new(),
            total: 0,
        }
    }

    fn print(&self, repo_root: &Path) {
        println!(
            "  {:<14} {:>4} / {:<4} need byte-compilation",
            self.label,
            self.files.len(),
            self.total
        );
        for file in self.files.iter().take(MAX_LISTED) {
            println!("    - {}", relative(repo_root, file));
        }
        if self.files.len() > MAX_LISTED {
            println!("    ... {} more", self.files.len() - MAX_LISTED);
        }
    }
}

#[derive(Debug)]
pub(crate) struct RebuildCheck {
    pub(crate) compile_first: StageReport,
    pub(crate) preloaded: StageReport,
    pub(crate) main_first: StageReport,
    pub(crate) compile_main: StageReport,
    pub(crate) subdirs_changed: Vec<PathBuf>,
    pub(crate) subdirs_total: usize,
}

impl RebuildCheck {
    pub(crate) fn has_work(&self) -> bool {
        !self.subdirs_changed.is_empty()
            || [
                &self.compile_first,
                &self.preloaded,
                &self.main_first,
                &self.compile_main,
            ]
            .iter()
            .any(|stage| !stage.files.is_empty())
    }

    fn bytecode_work(&self) -> usize {
        [
            &self.compile_first,
            &self.preloaded,
            &self.main_first,
            &self.compile_main,
        ]
        .iter()
        .map(|stage| stage.files.len())
        .sum()
    }

    pub(crate) fn print(&self, repo_root: &Path) {
        println!("fresh-build check (read-only; a byte-preserving build would do):");
        for stage in [
            &self.compile_first,
            &self.preloaded,
            &self.main_first,
            &self.compile_main,
        ] {
            stage.print(repo_root);
        }
        println!(
            "  subdirs.el     {:>4} / {:<4} would change",
            self.subdirs_changed.len(),
            self.subdirs_total
        );
        for dir in self.subdirs_changed.iter().take(MAX_LISTED) {
            println!("    - {}", relative(repo_root, dir));
        }
        if self.subdirs_changed.len() > MAX_LISTED {
            println!("    ... {} more", self.subdirs_changed.len() - MAX_LISTED);
        }

        if self.has_work() {
            println!(
                "work needed: {} .el file(s), {} subdirs.el file(s)",
                self.bytecode_work(),
                self.subdirs_changed.len()
            );
        } else {
            println!("up to date: nothing to rebuild");
        }
        println!(
            "note: leim, semantic grammars, loaddefs and custom/finder data are \
             regenerated unconditionally by fresh-build; --check does not \
             evaluate them."
        );
    }
}

/// Compute the staleness report.  Reads the tree; writes nothing.
pub(crate) fn check(options: &FreshBuildOptions) -> Result<RebuildCheck> {
    let paths = pipeline_paths(options);
    let lisp_root = &paths.lisp_root;
    let makefile_in = &paths.makefile_in;

    let compile_first = parse_compile_first_sources(makefile_in, lisp_root, options.native_comp)?;
    let characters = lisp_root.join("international/characters.el");
    let preloaded_sources = parse_preloaded_lisp_sources(&lisp_root.join("loadup.el"), lisp_root)?;
    let unicode_deps = if preloaded_sources.contains(&characters) {
        preloaded_characters_dependency_sources(lisp_root)
    } else {
        Vec::new()
    };
    let main_first = parse_main_first_sources(makefile_in, lisp_root)?;
    let mut general = compile_main_sources(lisp_root)?;
    let main_first_set: BTreeSet<PathBuf> = main_first.iter().cloned().collect();
    general.retain(|source| !main_first_set.contains(source));

    // `edges` adds the GNU characters.elc prerequisites to the makefile
    // dependencies; `dependencies` stays the makefile map the compile-main
    // predicate itself uses (`dependency bytecode newer`).
    let dependencies = parse_compile_main_dependencies(makefile_in, lisp_root)?;
    let mut edges = dependencies.clone();
    let characters_is_preloaded = preloaded_sources.contains(&characters);
    if characters_is_preloaded {
        edges
            .entry(characters.clone())
            .or_default()
            .extend(unicode_deps.iter().cloned());
    }

    // Stage membership, earliest first.  A source listed in more than one
    // stage (COMPILE_FIRST files are also compile-main candidates) belongs to
    // the stage that compiles it first.
    let compile_first_set: BTreeSet<PathBuf> = compile_first.into_iter().collect();
    let mut preloaded_set: BTreeSet<PathBuf> = preloaded_sources.into_iter().collect();
    preloaded_set.extend(unicode_deps.iter().cloned());
    let general_set: BTreeSet<PathBuf> = general.into_iter().collect();

    let stage_of = |source: &PathBuf| -> Option<usize> {
        if compile_first_set.contains(source) {
            Some(0)
        } else if preloaded_set.contains(source) {
            Some(1)
        } else if main_first_set.contains(source) {
            Some(2)
        } else if general_set.contains(source) {
            Some(3)
        } else {
            None
        }
    };

    let mut membership: [BTreeSet<PathBuf>; 4] = [
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    ];
    for source in compile_first_set
        .iter()
        .chain(preloaded_set.iter())
        .chain(main_first_set.iter())
        .chain(general_set.iter())
    {
        if let Some(index) = stage_of(source) {
            membership[index].insert(source.clone());
        }
    }

    let mut rebuild: BTreeSet<PathBuf> = BTreeSet::new();
    for source in membership.iter().flatten() {
        let needs = match stage_of(source) {
            Some(0) => bytecode_needs_rebuild(source),
            Some(1) if *source == characters => {
                bytecode_needs_rebuild_with_dependencies(source, &unicode_deps)
            }
            Some(1) => bytecode_needs_rebuild(source),
            Some(2) => compile_main_needs_rebuild(source),
            Some(3) => {
                compile_main_needs_rebuild(source)
                    || compile_main_dependency_bytecode_newer(source, &dependencies)
            }
            Some(_) | None => false,
        };
        if needs {
            rebuild.insert(source.clone());
        }
    }

    // A rebuilt file's dependents are out of date too, exactly as make's
    // prerequisite rule propagates to the dependent target.  Iterate to a
    // fixpoint because the dependency graph can be deep.
    loop {
        let mut changed = false;
        for (source, deps) in &edges {
            if rebuild.contains(source) {
                continue;
            }
            let is_known = membership.iter().any(|stage| stage.contains(source));
            if is_known && deps.iter().any(|dep| rebuild.contains(dep)) {
                rebuild.insert(source.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut stages = [
        StageReport::new("COMPILE_FIRST"),
        StageReport::new("preloaded"),
        StageReport::new("MAIN_FIRST"),
        StageReport::new("compile-main"),
    ];
    for (index, sources) in membership.iter().enumerate() {
        stages[index].total = sources.len();
        stages[index].files = sources
            .iter()
            .filter(|source| rebuild.contains(*source))
            .cloned()
            .collect();
        stages[index].files.sort();
    }
    let [compile_first, preloaded, main_first, compile_main] = stages;

    let mut subdirs_changed = Vec::new();
    let dirs = lisp_dirs_for_subdirs_update(lisp_root)?;
    for dir in &dirs {
        if subdirs_would_change(dir)? {
            subdirs_changed.push(dir.clone());
        }
    }

    Ok(RebuildCheck {
        compile_first,
        preloaded,
        main_first,
        compile_main,
        subdirs_changed,
        subdirs_total: dirs.len(),
    })
}

/// Whether `update-subdirs` would rewrite or remove `dir/subdirs.el`.
///
/// Mirrors `update_subdirs_file` without writing the `subdirs.el~` staging
/// file: an empty expression means the file must be absent, otherwise the
/// file must exist with exactly the generated bytes.
fn subdirs_would_change(dir: &Path) -> Result<bool> {
    let expression = update_subdirs_expression(dir)?;
    let target = dir.join("subdirs.el");
    if expression.is_empty() {
        return Ok(target.exists());
    }
    let expected = update_subdirs_contents(&expression);
    match fs::read(&target) {
        Ok(existing) => Ok(existing != expected.as_bytes()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(true),
        Err(err) => Err(format!("read {}: {err}", target.display()).into()),
    }
}

fn relative(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}
