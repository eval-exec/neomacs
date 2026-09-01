//! Standing check: **this build ships no `.el` that GNU would have compiled.**
//!
//! Ledger 202 gave this tree a refusal for bytecode *older* than its source.
//! That predicate is keyed on the `.elc`: `stale_lisp_bytecode` walks the
//! `.elc` files and asks each one for its `.el`.  A `.el` with **no** `.elc`
//! never enters the loop, so the third state of a Lisp tree -- source present,
//! bytecode absent -- had no observer at all.  Ledger 173's law, and this time
//! the row that was never written is the whole defect.
//!
//! # What the absent `.elc` costs, precisely
//!
//! `Fload` hands a `*.el` to `load-source-file-function`, which `loadup.el:143`
//! sets to `load-with-code-conversion` (`src/lread.c:1400-1418`).  That
//! function inserts the file into a temporary buffer with
//! `insert-file-contents` (`lisp/international/mule.el:294-336`), and
//! `Finsert_file_contents` assigns `Vlast_coding_system_used`
//! (`src/fileio.c:5172`).  A `*.elc` never reaches that path: `Fload` falls
//! through to `readevalloop` over a raw stdio stream and touches no coding
//! state.
//!
//! Measured on both editors, two fixtures with identical bytes, one with a
//! `.elc` beside it and one without (ledger 207 §1):
//!
//! ```text
//!                    compiled arm            source-only arm
//! GNU Emacs 31.0.90  sentinel -> sentinel    sentinel -> prefer-utf-8-unix
//! neomacs            sentinel -> sentinel    sentinel -> prefer-utf-8-unix
//! ```
//!
//! So a generated file that ships without its `.elc` silently rewrites the
//! coding state of whatever called `load`.  A peer session lost months to
//! exactly that: `lisp/emacs-lisp/cl-loaddefs.el` had no `.elc`, the
//! missing-lexical-binding warning pulls `warnings` -> `icons` -> `cl-lib` ->
//! `cl-loaddefs` *inside* the load being measured, and
//! `oracle_load_auto_detects_iso_2022_source_without_a_valid_cookie` read the
//! clobbered value.
//!
//! # GNU's rule, and why it is not a list
//!
//! See [`compile_main_rule`] for the citation.  In one line: GNU's
//! `compile-main` compiles every `.el` it globs unless that file's own text
//! says `no-byte-compile: t`, and the generator is what decides whether a
//! generated file carries the cookie.  So this is a scan over the tree, not a
//! table of names -- for the same reason ledger 197's `c_features` scan reads
//! `features` out of a live runtime instead of grepping for `provide`, and the
//! same reason ledger 206's recipe check runs awk and diffs the bytes on disk.
//!
//! Ledger 207.

#[path = "../../../../build_support/compile_main_rule.rs"]
mod compile_main_rule;

use compile_main_rule::{BytecodeCoverage, LispBytecodeCoverage};
use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
}

/// **The postcondition of GNU's `compile-main`, asserted over the real tree.**
///
/// Green before ledger 207 on a fully built checkout -- and said so, because a
/// guard presented as a fix when it was already green is the false green this
/// campaign keeps recording.  Its value is the trees where it is not green:
/// the peer's, where `cl-loaddefs.elc` was absent, and any tree left by
/// `cargo xtask fresh-build --no-byte-compile`, which used to delete the whole
/// generated loaddefs set's bytecode and recompile none of it (ledger 206
/// §9.2, fixed here by `BytecodePlan`).
#[test]
fn no_lisp_source_ships_without_the_bytecode_gnu_would_have_built_it() {
    let lisp_root = project_root().join("lisp");
    let coverage = LispBytecodeCoverage::scan(&lisp_root).expect("scan lisp/");

    assert!(
        coverage.examined > 1000,
        "the scan examined only {} .el files under {}; GNU's tree has ~1670 and \
         this one ~1684, so a smaller number means the walk failed and every \
         assertion below is vacuous",
        coverage.examined,
        lisp_root.display()
    );
    assert!(
        coverage.compiled > 1000,
        "{} .el examined but only {} have a .elc.  This is almost certainly a \
         worktree that has never been built: the generated Lisp artifacts and \
         every .elc are gitignored, so they do not travel with a checkout.  \
         Seed it from a built checkout (copy lisp/'s ignored files, then \
         `find lisp -name '*.elc' -exec touch {{}} +`) or run \
         `cargo xtask fresh-build --release`.  Ledger 207.",
        coverage.examined,
        coverage.compiled
    );
    assert_eq!(
        coverage.missing,
        Vec::<PathBuf>::new(),
        "these .el files have no .elc and do not exempt themselves with GNU's \
         `no-byte-compile: t' cookie, so GNU's lisp/Makefile.in compile-main \
         would have compiled every one of them.  Each will now be loaded by \
         `load-with-code-conversion', which assigns last-coding-system-used \
         (src/fileio.c:5172) inside whatever called `load'.\n{}",
        coverage.describe(&lisp_root)
    );
}

/// **Every exemption is the file's own, not this code's.**
///
/// The scan is only trustworthy if `exempt` means "the bytes say
/// `no-byte-compile: t`" and never "the scan decided to skip it".  There is no
/// allow-list to check, so this re-reads each exempt file and re-derives the
/// verdict, and it names the count so a scan that started exempting everything
/// is visible rather than quietly green.
///
/// Green before ledger 207.
#[test]
fn every_uncompiled_lisp_source_exempts_itself_in_its_own_text() {
    let lisp_root = project_root().join("lisp");
    let coverage = LispBytecodeCoverage::scan(&lisp_root).expect("scan lisp/");

    assert!(
        !coverage.exempt.is_empty(),
        "not one .el under {} exempts itself with `no-byte-compile: t'.  GNU's \
         tree has 33 -- lisp/loadup.el, lisp/ldefs-boot.el, \
         lisp/theme-loaddefs.el, lisp/subdirs.el, the 22 international/uni-*.el \
         tables and the rest -- so zero means the cookie scan is broken and the \
         guard above is checking nothing",
        lisp_root.display()
    );
    assert!(
        coverage.exempt.len() < coverage.examined / 4,
        "{} of {} .el files claim `no-byte-compile: t'.  GNU's proportion is 33 \
         of 1673; anything near a quarter of the tree means the marker match \
         became too loose and is exempting files GNU compiles",
        coverage.exempt.len(),
        coverage.examined
    );

    for source in &coverage.exempt {
        assert!(
            compile_main_rule::source_declares_no_byte_compile(source).expect("re-read source"),
            "{} was reported exempt but its text does not match GNU's \
             `^;.*[^a-zA-Z]no-byte-compile: *t'",
            source.display()
        );
        assert!(
            !source.with_extension("elc").is_file(),
            "{} is reported exempt, but a .elc exists beside it -- the exempt \
             bucket is for files with NO bytecode, and GNU's `test ! -f \
             $${{el}}c &&' short-circuit means a file that has one is compiled \
             again whatever its own text says",
            source.display()
        );
    }
}

/// **Sensitivity: the scan does report a missing `.elc`.**
///
/// The two guards above are green on a built tree, which on its own proves
/// nothing about whether they can go red.  This builds the three states by
/// hand and checks the scan sorts them, so "always returns no missing files"
/// cannot satisfy it.  Ledger 191's method, applied to a scan rather than a
/// binary.
#[test]
fn the_scan_separates_compiled_exempt_and_missing_bytecode() {
    let root = project_root()
        .join("tmp")
        .join("compile-main-rule-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos()
        ));
    std::fs::create_dir_all(root.join("sub")).expect("create fixture");

    std::fs::write(root.join("compiled.el"), ";;; compiled.el\n").expect("write");
    std::fs::write(root.join("compiled.elc"), ";ELC\n").expect("write");
    std::fs::write(
        root.join("exempt.el"),
        ";;; exempt.el\n;; Local Variables:\n;; no-byte-compile: t\n;; End:\n",
    )
    .expect("write");
    // The defect, in the shape it actually shipped: a generated loaddefs file
    // whose trailer carries `no-update-autoloads' and `no-native-compile' but
    // NOT `no-byte-compile', exactly as GNU's own cl-loaddefs.el does.
    std::fs::write(
        root.join("sub/cl-loaddefs.el"),
        ";;; cl-loaddefs.el --- automatically extracted autoloads\n\
         (provide 'cl-loaddefs)\n\
         ;; Local Variables:\n\
         ;; version-control: never\n\
         ;; no-update-autoloads: t\n\
         ;; no-native-compile: t\n\
         ;; End:\n",
    )
    .expect("write");

    let coverage = LispBytecodeCoverage::scan(&root).expect("scan fixture");

    assert_eq!(coverage.examined, 3);
    assert_eq!(coverage.compiled, 1);
    assert_eq!(coverage.exempt, vec![root.join("exempt.el")]);
    assert_eq!(coverage.missing, vec![root.join("sub/cl-loaddefs.el")]);

    // And the per-file verdict agrees with the scan, so a caller that asks one
    // file and a caller that asks the tree cannot disagree.
    assert_eq!(
        BytecodeCoverage::of(&root.join("sub/cl-loaddefs.el")).expect("verdict"),
        BytecodeCoverage::MissingBytecode
    );
    assert_eq!(
        BytecodeCoverage::of(&root.join("exempt.el")).expect("verdict"),
        BytecodeCoverage::ExemptBySourceCookie
    );

    let described = coverage.describe(&root);
    assert!(
        described.contains("cl-loaddefs.el"),
        "the failure message must name the file; got {described}"
    );

    std::fs::remove_dir_all(&root).ok();
}
