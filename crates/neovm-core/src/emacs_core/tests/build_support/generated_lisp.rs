//! Standing check: **one artifact, one generator.**
//!
//! `lisp/international/charscript.el` and `lisp/international/emoji-zwj.el`
//! are not source.  GNU builds them, in `admin/unidata/Makefile.in:110-123`,
//! by running its own awk scripts over its own Unicode data:
//!
//! ```make
//! ${unidir}/charscript.el: ${blocks_sources}
//! 	$(AM_V_GEN)$(AWK) -f ${blocks} ${blocks_sources} > $@
//! ${unidir}/emoji-zwj.el: ${zwj_sources}
//! 	$(AM_V_GEN)$(AWK) -f ${zwj} ${zwj_sources} > $@
//! ```
//!
//! There is exactly one recipe per file and no post-processing, so **what awk
//! prints is the file**.  Until ledger 206 this port had two producers for
//! those two names -- `cargo xtask fresh-build` ran GNU's awk, and
//! `neovm-core/build.rs` ran a hand-written Rust reimplementation of it over a
//! second, byte-identical copy of the same Unicode data in
//! `crates/neovm-core/unicode-data/`.  Whichever ran last decided the file.
//!
//! Two costs, and the second is the expensive one:
//!
//! 1. **A build-system defect.**  The two producers disagree on the bytes, so
//!    the first debug build after a release build (or the reverse) rewrote
//!    `emoji-zwj.el` and left `emoji-zwj.elc` behind it.  Ledger 203 §7.4
//!    recorded that; ledger 202's refusal is what caught it, by name, in
//!    2 seconds.
//! 2. **A GNU divergence.**  The reimplementation double-escaped the
//!    `\U0001F1E6`-style character escapes in the two hand-derived flag
//!    blocks, so the Elisp reader saw a literal backslash instead of a
//!    character.  The regexp GNU builds for regional-indicator flags is
//!    `"[\U0001F1E6-\U0001F1FF][\U0001F1E6-\U0001F1FF]"`, five characters per
//!    bracket; this port shipped the 23-character literal
//!    `"[\\U0001F1E6-\\U0001F1FF]"`, and the same for the UK subdivision tag
//!    sequence.  Country flags and UK flags therefore did not compose.
//!
//! The fix is not "port the awk more carefully" -- that is the reimplementation
//! the project's standing directive forbids, and it cannot even succeed here,
//! because GNU's `for (elt in ch)` emits its 150 entries in gawk's internal
//! hash order, which is not a thing another language can reproduce.  The fix
//! is that there is now ONE recipe table, [`AWK_GENERATED_UNICODE_LISP`],
//! included by `neovm-core/build.rs` and by `xtask` from the same file, and it
//! runs GNU's awk.
//!
//! These tests are the ledger-197 half: a table with no way to spell a second
//! producer only constrains the table.  So the first test below does not read
//! the table's intentions -- it **runs the recipe and compares the bytes on
//! disk**, which fails whatever crate the second producer lives in.
//!
//! Ledger 206.

#[path = "../../../../build_support/generated_lisp.rs"]
mod generated_lisp;

use generated_lisp::{AWK_GENERATED_UNICODE_LISP, AwkGeneratedLisp, GeneratedLispRoots};
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

/// This checkout, read and written by the same recipes the build scripts run.
fn roots() -> GeneratedLispRoots {
    GeneratedLispRoots::of_project(&project_root())
}

/// Anti-vacuity: a table that lost its rows would satisfy every loop below.
fn recipes() -> &'static [AwkGeneratedLisp] {
    assert!(
        AWK_GENERATED_UNICODE_LISP.len() >= 2,
        "the recipe table has {} rows; GNU's admin/unidata/Makefile.in has two \
         awk-generated Lisp targets, so a shorter table means rows were lost \
         and every check in this file is vacuous",
        AWK_GENERATED_UNICODE_LISP.len()
    );
    AWK_GENERATED_UNICODE_LISP
}

/// **The file on disk is what the one recipe prints.**
///
/// This is a scan, not a list.  It does not ask which code claims to own the
/// artifact; it runs GNU's awk and diffs the result against the bytes a test
/// would actually load.  A second producer anywhere in the workspace -- a
/// build script, an xtask step, a helper someone adds next year -- fails here
/// and is named, exactly as `c_features_test::no_site_outside_the_table_decides_a_c_level_feature`
/// catches an out-of-table `provide` in any crate (ledger 197).
///
/// RED before ledger 206: `neovm-core/build.rs` had written its own bytes over
/// awk's, and the diff was the entry order of all 150 emoji entries plus three
/// blank lines plus the doubled backslashes of the two flag regexps.
#[test]
fn every_generated_unicode_lisp_file_is_byte_for_byte_what_gnus_awk_prints() {
    let roots = roots();
    for recipe in recipes() {
        let output = recipe.output_path(&roots);
        let printed = recipe
            .generate(&roots)
            .unwrap_or_else(|err| panic!("running GNU's recipe for {}: {err}", recipe.output));
        assert!(
            printed.len() > 1000,
            "GNU's awk printed only {} bytes for {}; an empty or truncated run \
             would make the comparison below meaningless",
            printed.len(),
            recipe.output
        );
        let on_disk = std::fs::read(&output).unwrap_or_else(|err| {
            panic!(
                "{} is missing ({err}); it is generated, not source, and \
                 nothing that boots an image can run without it",
                output.display()
            )
        });
        assert_eq!(
            describe_bytes(&on_disk),
            describe_bytes(&printed),
            "{} on disk is not what `{} -f {} ...` prints, so a SECOND producer \
             wrote it.  Whichever one ran last also invalidated the .elc beside \
             it.  Run `cargo xtask fresh-build --release`, and find the other \
             producer",
            output.display(),
            generated_lisp::AWK_PROGRAM,
            recipe.script,
        );
    }
}

/// A byte-count-and-hash summary, so a failure prints a diagnosis rather than
/// 128 KB of emoji sequences twice.
fn describe_bytes(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{} bytes, fnv1a {hash:016x}", bytes.len())
}

/// **Two producers for one artifact is unspellable in the table itself.**
///
/// The compile-time half of the rule.  A duplicate output name is the shape
/// the defect had, so the table may not carry one.
#[test]
fn no_generated_lisp_artifact_has_more_than_one_recipe() {
    let mut outputs: Vec<&str> = recipes().iter().map(|recipe| recipe.output).collect();
    outputs.sort_unstable();
    let mut deduped = outputs.clone();
    deduped.dedup();
    assert_eq!(
        outputs, deduped,
        "an output listed twice is two recipes for one artifact, which is the \
         defect ledger 206 deleted"
    );
}

/// Every recipe names inputs that exist, and names GNU's own script.
///
/// The inputs are `admin/unidata/`, GNU's directory, and no other: the second
/// copy under `crates/neovm-core/unicode-data/` was deleted with the reimplementation
/// that read it, because two copies of an input are the same defect one level
/// down.
#[test]
fn every_recipe_reads_gnus_own_script_and_data_and_nothing_else() {
    let root = project_root();
    let roots = roots();
    for recipe in recipes() {
        for dependency in recipe.dependencies(&roots) {
            assert!(
                dependency.is_file(),
                "recipe for {} names {}, which is not a file",
                recipe.output,
                dependency.display()
            );
            assert!(
                dependency.starts_with(root.join("admin").join("unidata")),
                "recipe for {} reads {}, outside GNU's own admin/unidata",
                recipe.output,
                dependency.display()
            );
        }
    }
    assert!(
        !root.join("crates/neovm-core").join("unicode-data").exists(),
        "crates/neovm-core/unicode-data is a second copy of admin/unidata's inputs; it \
         existed only for the Rust reimplementation ledger 206 deleted"
    );
}

/// The recipe is a function of its inputs, so `write_if_changed` really can
/// leave the mtime alone.
///
/// If awk's output were unstable between runs, every build would rewrite the
/// `.el` and invalidate the `.elc` beside it -- which is the defect, restored
/// by a different route.  gawk's `for (elt in ch)` order is unspecified by
/// POSIX but deterministic for a given implementation and insertion sequence;
/// this is the check that says so out loud rather than assuming it.
#[test]
fn the_recipes_print_the_same_bytes_twice() {
    let roots = roots();
    for recipe in recipes() {
        let first = recipe.generate(&roots).expect("first run");
        let second = recipe.generate(&roots).expect("second run");
        assert_eq!(
            describe_bytes(&first),
            describe_bytes(&second),
            "{} is not reproducible, so every build would rewrite it",
            recipe.output
        );
    }
}

/// **The flag regexps carry GNU's character escapes, not doubled ones.**
///
/// The behavioural half, pinned on the artifact rather than on the image so it
/// is readable next to its cause.  In Elisp source `"\U0001F1E6"` is one
/// character (the Elisp reader's 8-digit Unicode escape) and `"\\U0001F1E6"`
/// is ten -- a backslash followed by nine literal characters.  GNU's
/// `emoji-zwj.awk` writes `\\U` in an awk string literal, which awk prints as
/// a SINGLE backslash; the Rust reimplementation wrote `\\\\U` in a Rust
/// string literal, which is two.  Measured against GNU Emacs 31.0.90:
///
/// ```text
/// (read "\"[\\U0001F1E6-\\U0001F1FF]\"")     => "[🇦-🇿]"                    length 5
/// (read "\"[\\\\U0001F1E6-\\\\U0001F1FF]\"") => "[\\U0001F1E6-\\U0001F1FF]"  length 23
/// ```
///
/// RED before ledger 206 on the shipped `target/release/neomacs`, whose
/// `composition-function-table` entry for `?\N{U+1F1E6}` was the 23-character
/// literal.
///
/// It reads the file **on disk** and not awk's stdout, deliberately.  Asking
/// awk would be green before the fix as well as after -- awk was always right;
/// what was wrong is the file the image is built from.  A guard that is green
/// before the fix is the false green this campaign keeps recording.
#[test]
fn the_hand_derived_flag_regexps_use_gnus_single_backslash_escapes() {
    let roots = roots();
    let recipe = recipes()
        .iter()
        .find(|recipe| recipe.output.ends_with("emoji-zwj.el"))
        .expect("emoji-zwj.el has a recipe");
    let output = recipe.output_path(&roots);
    let text = std::fs::read_to_string(&output)
        .unwrap_or_else(|err| panic!("read {}: {err}", output.display()));
    assert!(
        text.len() > 1000,
        "{} is {} bytes, so the checks below would be vacuous",
        output.display(),
        text.len()
    );

    assert!(
        text.contains(r#"(vector "[\U0001F1E6-\U0001F1FF][\U0001F1E6-\U0001F1FF]""#),
        "the regional-indicator flag regexp is not GNU's; without single \
         backslashes the Elisp reader builds a bracket of literal backslashes \
         and country flags stop composing"
    );
    assert!(
        !text.contains(r#"[\\U0001F1E6"#),
        "a doubled backslash is back in the flag regexp"
    );
    assert!(
        text.contains(r#"(vector "\U0001F3F4\U000E0067\U000E0062\\(?:"#),
        "the UK subdivision tag-sequence regexp is not GNU's: the character \
         escapes take one backslash and only the shy group takes two"
    );
}

/// A generated file is never source, and the tree must say so.
///
/// Both outputs are gitignored -- which is exactly why they do not travel with
/// a pull and why staleness is invisible (ledger 202 §1).  If one were ever
/// committed, `git` would hand every checkout a fourth producer.
#[test]
fn no_generated_unicode_lisp_file_is_tracked_in_git() {
    let root = project_root();
    let roots = roots();
    for recipe in recipes() {
        let output = recipe.output_path(&roots);
        let relative = output
            .strip_prefix(&root)
            .expect("output under the workspace root");
        let tracked = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("ls-files")
            .arg("--error-unmatch")
            .arg(relative)
            .output()
            .expect("run git ls-files");
        assert!(
            !tracked.status.success(),
            "{} is tracked in git, so a checkout is a producer too",
            relative.display()
        );
    }
}

/// Every path helper agrees on where the artifact lives.
#[test]
fn output_paths_land_under_lisp_international() {
    let root = project_root();
    let roots = roots();
    for recipe in recipes() {
        let output = recipe.output_path(&roots);
        assert!(
            output.starts_with(root.join("lisp")),
            "{} is generated outside lisp/",
            output.display()
        );
        assert_eq!(
            output.extension().and_then(|e| e.to_str()),
            Some("el"),
            "{} is not a Lisp source file",
            output.display()
        );
        assert!(
            Path::new(recipe.output).is_relative(),
            "recipe outputs are relative to lisp/, so they cannot escape it"
        );
    }
}
