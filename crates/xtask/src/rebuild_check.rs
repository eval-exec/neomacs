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
//! Every stage's predicate is evaluated for a source that belongs to more than
//! one list, and the source is reported under the earliest stage that would
//! compile it.  A COMPILE_FIRST file whose own mtime is current but whose
//! dependency bytecode is newer is therefore reported under compile-main: that
//! is the pass whose dependency rule selects it.
//!
//! Generated Lisp outputs that `fresh-build` currently regenerates
//! unconditionally (leim, semantic grammars, loaddefs, custom/finder data) are
//! NOT evaluated here; only `subdirs.el` is, because its content comparison is
//! exact and already implemented.  The report says so.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use strum::IntoEnumIterator;

use crate::{
    FreshBuildOptions, Result, bytecode_needs_rebuild, bytecode_needs_rebuild_with_dependencies,
    compile_main_dependency_bytecode_newer, compile_main_needs_rebuild, compile_main_sources,
    lisp_dirs_for_subdirs_update, parse_compile_first_sources, parse_compile_main_dependencies,
    parse_main_first_sources, parse_preloaded_lisp_sources, pipeline_paths,
    preloaded_characters_dependency_sources, update_subdirs_contents, update_subdirs_expression,
};

/// How many paths one stage prints before folding the rest into a count.
const MAX_LISTED: usize = 10;

/// The bytecode stages, in the order the pipeline compiles them.
///
/// `EnumIter` supplies the pipeline order without a second, hand-maintained
/// list; `Display` carries the report labels as per-variant attributes; the
/// explicit `repr` discriminants are the array slots (a new variant cannot
/// silently shift an existing one).
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumIter, strum::Display)]
enum Stage {
    #[strum(to_string = "COMPILE_FIRST")]
    CompileFirst = 0,
    #[strum(to_string = "preloaded")]
    Preloaded = 1,
    #[strum(to_string = "MAIN_FIRST")]
    MainFirst = 2,
    #[strum(to_string = "compile-main")]
    CompileMain = 3,
}

impl Stage {
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug)]
pub(crate) struct StageReport {
    pub(crate) stage: Stage,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) total: usize,
}

impl StageReport {
    fn new(stage: Stage) -> Self {
        Self {
            stage,
            files: Vec::new(),
            total: 0,
        }
    }

    fn print(&self, repo_root: &Path) {
        println!(
            "  {:<14} {:>4} / {:<4} need byte-compilation",
            self.stage,
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

    // Candidate membership per stage, in pipeline order.  A COMPILE_FIRST or
    // preloaded source is also a compile-main candidate, so this is NOT a
    // partition: `total` reports how many sources each stage considers.
    let mut preloaded_set: BTreeSet<PathBuf> = preloaded_sources.into_iter().collect();
    preloaded_set.extend(unicode_deps.iter().cloned());
    let membership: [BTreeSet<PathBuf>; 4] = [
        compile_first.into_iter().collect(),
        preloaded_set,
        main_first_set,
        general.into_iter().collect(),
    ];

    // Evaluate every stage's predicate.  A source is reported under the
    // earliest stage that would compile it.  This matters for the review
    // case where COMPILE_FIRST sees a current mtime but compile-main's
    // dependency rule (newer dependency bytecode) still selects the source:
    // the pipeline compiles it in compile-main, so that is where it belongs.
    let mut selected: BTreeMap<PathBuf, Stage> = BTreeMap::new();
    for source in membership.iter().flatten() {
        if selected.contains_key(source) {
            continue;
        }
        for stage in Stage::iter() {
            if !membership[stage.index()].contains(source) {
                continue;
            }
            if stage_needs(stage, source, &characters, &unicode_deps, &dependencies) {
                selected.insert(source.clone(), stage);
                break;
            }
        }
    }

    // A rebuilt file's dependents are out of date too.  This mirrors
    // compile-main's rebuild closure: only general candidates participate,
    // and the dependent is compiled by compile-main (the earlier stages do
    // not consult dependencies, except characters').  Iterate to a fixpoint
    // because the graph can be deep.
    loop {
        let mut changed = false;
        for (source, deps) in &edges {
            if selected.contains_key(source)
                || !membership[Stage::CompileMain.index()].contains(source)
            {
                continue;
            }
            if deps.iter().any(|dep| selected.contains_key(dep)) {
                selected.insert(source.clone(), Stage::CompileMain);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut stages = [
        StageReport::new(Stage::CompileFirst),
        StageReport::new(Stage::Preloaded),
        StageReport::new(Stage::MainFirst),
        StageReport::new(Stage::CompileMain),
    ];
    for (stage, sources) in Stage::iter().zip(membership.iter()) {
        stages[stage.index()].total = sources.len();
    }
    for (source, stage) in selected {
        stages[stage.index()].files.push(source);
    }
    for stage in &mut stages {
        stage.files.sort();
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

/// The pipeline's staleness predicate for one stage, in exhaustive form.
fn stage_needs(
    stage: Stage,
    source: &Path,
    characters: &Path,
    unicode_deps: &[PathBuf],
    dependencies: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
) -> bool {
    match stage {
        Stage::CompileFirst => bytecode_needs_rebuild(source),
        Stage::Preloaded if source == characters => {
            bytecode_needs_rebuild_with_dependencies(source, unicode_deps)
        }
        Stage::Preloaded => bytecode_needs_rebuild(source),
        Stage::MainFirst => compile_main_needs_rebuild(source),
        Stage::CompileMain => {
            compile_main_needs_rebuild(source)
                || compile_main_dependency_bytecode_newer(source, dependencies)
        }
    }
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
