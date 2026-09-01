//! Standing check: this build cannot read bytecode older than its source and
//! report what it found as behaviour.
//!
//! Generated `.elc` files are gitignored, so they do not travel with a pull, a
//! merge or a fresh worktree, and `load` prefers a `.elc` over a newer `.el`.
//! A tree whose `.el` moved and whose `.elc` did not therefore runs the OLD
//! bytecode, and any test asserting on the compiled result reports the old
//! behaviour -- as a failure that looks exactly like a code defect.  It has
//! done so four times in this campaign, and cost a peer session a day.
//!
//! GNU has two defences and this port had neither in force:
//!
//! 1. **`lisp/loadup.el:110-116`** (Bug#17629) is one `(if dump-mode (progn
//!    ...))` with two effective statements -- `(setq inhibit-load-charset-map
//!    t)` and `(setq load-prefer-newer t)`.  The second is what keeps stale
//!    bytecode out of the dumped image entirely.  This port runs loadup with
//!    `dump-mode' nil (`load.rs`, `set_loadup_dump_mode(..., None)`) because
//!    Rust, not Lisp, does the dumping -- so the branch is dead and both
//!    statements have to be seeded from Rust.  Before ledger 202 only the
//!    first one was: `eval.set_variable("inhibit-load-charset-map", Value::T)`
//!    sat two lines below, and its sibling was left behind the dead `if`.
//!    Half a block was hoisted.
//!
//! 2. **`src/lread.c:1368-1398`** stats the `.el` beside every `.elc` `Fload`
//!    opens and messages `"Source file `%s' newer than byte-compiled file;
//!    using older file"`.  That string did not exist anywhere in this port, so
//!    the wrong answer was also a silent one.
//!
//! The type-level point is ledger 173's law in a new place.  A predicate over
//! rows that exist cannot see a row never written -- and here the rows DO
//! exist: `bootstrap_source_fingerprint` already stats every `.el` and `.elc`
//! under `lisp/` to build the memo key.  Nothing ever asked that table which
//! `.elc` no longer implement their `.el`.  The verdict costs no I/O; it was
//! simply never derived.
//!
//! Ledger 202.

use super::load::{
    LOADUP_DUMP_BRANCH_SEEDED_VARIABLES, StaleBytecodePolicy, seed_loadup_dump_branch_state,
    stale_lisp_bytecode,
};
use crate::emacs_core::value::Value;

/// A scratch tree for one test, under the repository's own `tmp/`.
///
/// The project rule is "never `/tmp`" -- the shared temp dir has no space and
/// destroys failure evidence.  `load_test.rs` still reaches for
/// `std::env::temp_dir()`; this does not.
struct Fixture {
    root: std::path::PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let root = std::path::PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
            .join("tmp")
            .join("stale-bytecode-tests")
            .join(format!("{name}-{unique}"));
        std::fs::create_dir_all(&root).expect("create fixture root");
        Self { root }
    }

    /// Write `stem.el` and `stem.elc` with the given contents and give the
    /// `.elc` an mtime OFFSET seconds away from the `.el`'s.  A negative
    /// offset is the defect: bytecode older than the source it came from.
    fn pair(&self, stem: &str, source: &str, compiled: &str, offset_secs: i64) {
        let el = self.root.join(format!("{stem}.el"));
        let elc = self.root.join(format!("{stem}.elc"));
        std::fs::write(&el, source).expect("write source");
        std::fs::write(&elc, format!(";ELC\n;;; Compiled\n\n\n{compiled}"))
            .expect("write compiled");
        let base = std::fs::metadata(&el)
            .expect("stat source")
            .modified()
            .expect("source mtime");
        let shifted = if offset_secs >= 0 {
            base + std::time::Duration::from_secs(offset_secs as u64)
        } else {
            base - std::time::Duration::from_secs(offset_secs.unsigned_abs())
        };
        std::fs::File::options()
            .write(true)
            .open(&elc)
            .expect("reopen compiled")
            .set_modified(shifted)
            .expect("set compiled mtime");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The stat table the bootstrap already builds can name every stale artifact.
///
/// `bootstrap_source_fingerprint` walks `lisp/` and stats every `.el` and
/// `.elc` to build its memo key (`load.rs`, `bootstrap_source_stats`).  Until
/// ledger 202 that table answered exactly one question -- "has anything
/// changed?" -- and threw away the one that mattered: "which of these `.elc`
/// no longer implement their `.el`?"  Both are predicates over the same rows.
///
/// The four shapes below are the whole space:
///
/// | shape | can it go stale? |
/// | --- | --- |
/// | `.elc` newer than its `.el` | no -- current |
/// | `.elc` OLDER than its `.el` | **yes -- this is the defect** |
/// | `.elc` with no `.el` at all | no -- nothing to compare |
/// | `.el` with no `.elc` | no -- loads from source |
///
/// Measured on the checkout this was written against: 1651 `.elc` under
/// `lisp/`, and **all 1651** have an `.el` sibling, so every one of them can
/// go stale.  The 33 `.el` with no `.elc` are GNU's own generated set
/// (`uni-*.el`, `charprop.el`, `ldefs-boot.el`, `loadup.el`, `subdirs.el`,
/// `cus-load.el`, `finder-inf.el`, `leim-list.el`, `theme-loaddefs.el`,
/// `org-version.el`) -- *not* `simple.el` and `window.el`, which are
/// compiled like everything else.
///
/// Ledger 202.  RED before the fix: `stale_lisp_bytecode` did not exist.
#[test]
fn the_bootstrap_stat_table_names_every_stale_artifact_it_already_stats() {
    crate::test_utils::init_test_tracing();
    let fixture = Fixture::new("census");
    fixture.pair(
        "current",
        "(defvar probe 'new)\n",
        "(defvar probe 'new)\n",
        60,
    );
    fixture.pair(
        "stale",
        "(defvar probe 'new)\n",
        "(defvar probe 'old)\n",
        -60,
    );
    std::fs::write(
        fixture.root.join("orphan.elc"),
        ";ELC\n\n\n(defvar orphan t)\n",
    )
    .expect("write orphan");
    std::fs::write(
        fixture.root.join("sourceonly.el"),
        "(defvar sourceonly t)\n",
    )
    .expect("write source-only");

    let stale = stale_lisp_bytecode(&fixture.root);
    let names = stale
        .iter()
        .map(|entry| {
            entry
                .source
                .file_name()
                .expect("named source")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["stale.el".to_string()],
        "only the .elc older than its .el is stale; a newer .elc, an orphan \
         .elc and a source-only .el are all fine"
    );
    // The verdict has to carry both mtimes, because "the mtimes" is the whole
    // diagnosis a reader needs and re-deriving it later is what nobody does.
    let entry = &stale[0];
    assert!(
        entry.source_mtime > entry.compiled_mtime,
        "a stale entry must carry the two mtimes that prove it stale, got \
         source={:?} compiled={:?}",
        entry.source_mtime,
        entry.compiled_mtime
    );
}

/// `load` says so when it reads bytecode older than its source.
///
/// GNU `Fload` (`src/lread.c:1368-1398`) stats the `.el` beside the `.elc` it
/// just opened and, when the source is newer, messages
///
/// ```text
/// Source file `%s' newer than byte-compiled file; using older file
/// ```
///
/// This port emitted nothing: `grep -rn "newer than byte-compiled" --include
/// '*.rs' .` returned no hits at all before ledger 202.  So the port's `load`
/// made the same choice GNU makes and kept the reason to itself, which is the
/// difference between a five-minute diagnosis and the four this cost.
///
/// Ledger 202.  RED before the fix: `current-message` was nil.
#[test]
fn loading_bytecode_older_than_its_source_says_so_the_way_gnu_does() {
    crate::test_utils::init_test_tracing();
    let fixture = Fixture::new("warns");
    fixture.pair(
        "probe",
        "(defvar l202-probe 'from-source)\n",
        "(defvar l202-probe 'from-bytecode)\n",
        -60,
    );

    let mut eval = super::eval::Context::new();
    super::load::load_file(&mut eval, &fixture.root.join("probe.elc")).expect("load fixture");

    // `*Messages*` and not `(current-message)`: this build faithfully takes
    // GNU's batch branch, where `message3` logs to `*Messages*` and prints to
    // stderr but never calls `set_message`, so the echo area -- and therefore
    // `current-message` -- stays empty (`xdisp.c` `message3_frame_nolog`,
    // `FRAME_INITIAL_P`).  `message_dolog` runs unconditionally.
    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("a stale .elc load must have logged something");
    let message = eval
        .buffers
        .get(messages_id)
        .expect("*Messages* live")
        .buffer_string();
    assert!(
        message.contains("newer than byte-compiled file; using older file"),
        "GNU src/lread.c:1379 names the source file it is ignoring; this build \
         said {message:?}"
    );
    assert!(
        message.contains("probe.el"),
        "the message must name the file, which is the whole diagnosis; got \
         {message:?}"
    );
}

/// The image build seeds EVERY statement of `loadup.el`'s dump branch.
///
/// `lisp/loadup.el:110-116` is one conditional with two effective statements:
///
/// ```elisp
/// (if dump-mode
///     (progn
///       (setq inhibit-load-charset-map t)
///       (defvar load--prefer-newer load-prefer-newer)
///       (setq load-prefer-newer t)))
/// ```
///
/// This port runs loadup with `dump-mode' nil, so the branch never fires and
/// both statements must be seeded from Rust.  Before ledger 202 only
/// `inhibit-load-charset-map` was, and the image could therefore be built out
/// of bytecode that no longer implemented its source -- which is what turned
/// the peer session's 33 stale `.elc` into three red tests on a clean
/// `origin/main`.
///
/// This is a structural guard, not a value check: it pins the two statements
/// TOGETHER so the next person to hoist one cannot leave the other behind.
///
/// Ledger 202.  RED before the fix: `load-prefer-newer` was nil.
#[test]
fn the_image_build_seeds_every_statement_of_loadups_dump_branch() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::eval::Context::new();
    seed_loadup_dump_branch_state(&mut eval);

    for name in LOADUP_DUMP_BRANCH_SEEDED_VARIABLES {
        assert_eq!(
            eval.obarray().symbol_value(name),
            Some(&Value::T),
            "loadup.el:110-116 sets {name} while building the image, and the \
             dump-mode branch that would do it is dead in this port"
        );
    }
    assert!(
        LOADUP_DUMP_BRANCH_SEEDED_VARIABLES.contains(&"load-prefer-newer"),
        "load-prefer-newer is the statement that keeps stale bytecode out of \
         the image (Bug#17629); dropping it from the list would restore the \
         defect silently"
    );
    // loadup.el:492-496 restores the user-visible value from this temporary,
    // and that block is guarded by `boundp' ALONE -- not by dump-mode -- so
    // seeding the temporary is what lets GNU's own Lisp do the restore rather
    // than a Rust copy of it.
    assert_eq!(
        eval.obarray().symbol_value("load--prefer-newer"),
        Some(&Value::NIL),
        "loadup.el:115 saves the pre-dump value so :493 can put it back"
    );
}

/// After loadup, the image answers about `load-prefer-newer` exactly as GNU's
/// does.
///
/// Measured against GNU Emacs 31.0.90, `emacs -Q --batch`:
///
/// ```text
/// (:value nil :standard nil :temp-bound nil)
/// ```
///
/// `:standard nil` and `:temp-bound nil` are `loadup.el:492-496` having run --
/// GNU's dump always sets `load--prefer-newer`, so that restore block always
/// fires and always `makunbound`s the temporary.  Seeding the temporary makes
/// this port take the same path, so the fix is not observable from Lisp: the
/// image is built with the option on and ships with it off, which is GNU's
/// arrangement exactly.
///
/// Ledger 202.
#[test]
fn the_built_image_ships_load_prefer_newer_off_exactly_as_gnu_does() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        "(list load-prefer-newer
               (get 'load-prefer-newer 'standard-value)
               (boundp 'load--prefer-newer))",
    );
    assert_eq!(result, "OK (nil nil nil)");
}

/// With `load-prefer-newer` on, an exact mtime tie keeps the `.elc`.
///
/// GNU `openp` swaps its saved candidate only for a STRICTLY newer one --
/// `if (timespec_cmp (mtime, save_mtime) <= 0) emacs_close (fd);`
/// (`src/lread.c:1991`) -- and `.elc` precedes `.el` in `load-suffixes`, so a
/// tie resolves to the bytecode.  This port used `Iterator::max_by_key`, which
/// documents the opposite ("the last element is returned"), and so resolved a
/// tie to source.
///
/// The inversion was unreachable while nothing switched `load-prefer-newer' on.
/// Ledger 202 switches it on for every image build, which is what made it
/// reachable and what found it: one-second filesystem timestamp granularity is
/// still common, and a byte-compile that finishes inside the same second as the
/// source write is an ordinary event.
///
/// Ledger 202.  RED before the fix: the `.el` was chosen.
#[test]
fn an_mtime_tie_under_prefer_newer_keeps_the_bytecode_as_gnu_does() {
    crate::test_utils::init_test_tracing();
    let fixture = Fixture::new("tie");
    fixture.pair(
        "tied",
        "(defvar probe 'source)\n",
        "(defvar probe 'bytecode)\n",
        0,
    );

    let load_path = vec![crate::heap_types::LispString::from_utf8(
        fixture.root.to_string_lossy().as_ref(),
    )];
    let chosen = super::load::find_file_in_load_path_with_flags(
        "tied", &load_path, false, false, /* prefer_newer */ true,
    )
    .expect("the fixture is on the load path");
    assert_eq!(
        chosen.extension().and_then(|e| e.to_str()),
        Some("elc"),
        "GNU keeps the earlier suffix on a tie; this build chose {}",
        chosen.display()
    );
}

/// The in-process test harness refuses to build an image from a stale tree.
///
/// This is the mechanism that covers all four bites at once, and it is a
/// build-integrity precondition rather than a Lisp semantics change: `cargo
/// xtask fresh-build` opens by DELETING every generated `.elc`
/// (`crates/xtask/src/main.rs`, `remove_stale_lisp_bytecode`) and recompiles, so
/// tests run through it can never see a stale artifact -- while a bare `cargo
/// nextest run` compiles nothing at all and happily reads whatever is on disk.
/// That asymmetry is the defect; this closes it by making the second path
/// notice what the first prevents.
///
/// A user's neomacs must NOT refuse to start over a stale `.elc` -- GNU warns
/// and carries on -- so the strict arm is scoped to the test harness, and the
/// two arms are a type rather than a boolean so that neither can be reached by
/// accident.
///
/// Ledger 202.  RED before the fix: the policy type did not exist.
#[test]
fn the_test_harness_refuses_a_stale_tree_and_a_user_build_only_warns() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        StaleBytecodePolicy::for_test_harness(),
        StaleBytecodePolicy::Refuse,
        "a test that asserts on compiled behaviour must not be allowed to read \
         bytecode that does not implement the checked-out source"
    );
    assert_eq!(
        StaleBytecodePolicy::for_user_runtime(),
        StaleBytecodePolicy::Warn,
        "GNU src/lread.c:1379 warns and loads it anyway; a shipped editor that \
         refused to start over one stale .elc would be a worse trade"
    );

    let fixture = Fixture::new("refusal");
    fixture.pair(
        "stale",
        "(defvar probe 'new)\n",
        "(defvar probe 'old)\n",
        -60,
    );
    let report = StaleBytecodePolicy::Refuse
        .report(&stale_lisp_bytecode(&fixture.root))
        .expect("a stale tree under Refuse must produce a refusal");
    assert!(
        report.contains("stale.el"),
        "the refusal has to name the files, or it is no better than the \
         behaviour difference it replaces; got {report}"
    );
    assert!(
        report.contains("cargo xtask fresh-build"),
        "and it has to name the command that fixes them; got {report}"
    );
    assert!(
        StaleBytecodePolicy::Warn
            .report(&stale_lisp_bytecode(&fixture.root))
            .is_none(),
        "Warn never refuses"
    );
}

/// **This process refuses, because nothing announced it a shipped editor.**
///
/// Ledger 202 decided the arm with `cfg!(test)`, which Rust sets only for the
/// crate under compilation as a test.  So the refusal was live here, in
/// `neovm-core`'s own 482 in-process tests, and dark in the 62 that live in
/// `neomacs-bin` and the 13 in `neomacs-layout-engine` -- both of which link
/// this crate as an ordinary dependency, where `cfg!(test)` is false.
///
/// Reproduced in ledger 206 on one deliberately staled tree (a single stale
/// `lisp/international/emoji-zwj.elc`, manufactured by the second generator
/// that ledger deleted):
///
/// | crate | test | result |
/// | --- | --- | --- |
/// | `neovm-core` | `the_gui_terminal_layer_adds_documentation_and_never_rewrites_it` | REFUSED in 2.0 s, naming the file and both mtimes |
/// | `neomacs` | `startup::tests::bootstrap_gui_frame_uses_gnu_cursor_and_pointer_color_defaults` | **1 passed** in 9.4 s, silently |
///
/// The fix is not a wider `cfg!`.  The question is about a PROCESS and
/// `cfg!(test)` answers about a compilation unit, so the default is inverted:
/// refuse unless `announce_shipped_editor_process` was called, and only
/// `neomacs`'s `main` calls it.  This test and its two peers in the other
/// crates observe the verdict rather than argue about it.
///
/// Green before ledger 206 in THIS crate -- said plainly, because a
/// green-before test presented as a guard is the false green this campaign
/// keeps recording.  Its RED peers are the tests of the same name in
/// `crates/neomacs/src/main_test.rs` and
/// `crates/neomacs-layout-engine/src/gui_chrome_test.rs`.
#[test]
fn a_process_that_never_announced_itself_a_shipped_editor_refuses() {
    crate::test_utils::init_test_tracing();
    let expected = match std::env::var_os(super::load::ALLOW_STALE_BYTECODE_ENV) {
        Some(value) if !value.is_empty() => StaleBytecodePolicy::Warn,
        _ => StaleBytecodePolicy::Refuse,
    };
    assert_eq!(
        StaleBytecodePolicy::for_this_process(),
        expected,
        "no test binary in any crate may read bytecode that does not implement \
         the checked-out source"
    );
}

/// **Only the shipped editor's `main` announces itself.**
///
/// The ledger-197 shape: a type that makes the bad state hard to spell in ONE
/// place says nothing about a second site somewhere else, and there were two
/// such sites when 197 went looking.  Here the bad state is "a harness that
/// opted out of the refusal", the only way to reach it is
/// `announce_shipped_editor_process`, and this scans every `.rs` in the
/// workspace for a call.
///
/// It is a source scan and not a runtime one, and the reason is worth stating:
/// the announcement is a fact about a process, so a library test can observe
/// only its OWN process's verdict -- which is what
/// `a_process_that_never_announced_itself_a_shipped_editor_refuses` and its two
/// peers do, one per crate that boots an image in-process.  Neither half is
/// sufficient alone.
#[test]
fn only_the_shipped_editors_main_announces_itself() {
    crate::test_utils::init_test_tracing();

    const CALL: &str = "announce_shipped_editor_process()";
    // The one program allowed to opt out, and the files that may merely name
    // the function: its definition, and this scan.
    const ALLOWED_CALLER: &str = "crates/neomacs/src/main.rs";
    const ALLOWED_MENTIONS: &[&str] = &[
        "crates/neovm-core/src/emacs_core/lisp/load/mod.rs",
        "crates/neovm-core/src/emacs_core/lisp/load/tests/stale_bytecode.rs",
    ];

    let root = std::path::PathBuf::from(env!("CARGO_WORKSPACE_DIR"));

    let mut scanned = 0usize;
    let mut callers: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                // `target/` is build output, `tmp/` is this project's scratch
                // tree, and neither is source.
                if name == "target" || name == "tmp" || name == ".git" || name == "node_modules" {
                    continue;
                }
                // A subdirectory with its own `.git` (a file for linked
                // worktrees, a directory for clones) is a nested checkout of
                // some commit, not this workspace's source.
                if path.join(".git").exists() {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !name.ends_with(".rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            scanned += 1;
            if !text.contains(CALL) {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .expect("scanned under the workspace root")
                .to_string_lossy()
                .replace('\\', "/");
            if ALLOWED_MENTIONS.contains(&relative.as_str()) {
                continue;
            }
            callers.push(relative);
        }
    }

    assert!(
        scanned > 500,
        "the scan read only {scanned} Rust files, so it did not reach the \
         workspace and finding nothing would prove nothing"
    );
    callers.sort();
    assert_eq!(
        callers,
        vec![ALLOWED_CALLER.to_string()],
        "the only program that may opt out of the stale-bytecode refusal is the \
         shipped editor, because GNU's answer for a shipped editor is a warning \
         (src/lread.c:1379).  Any other caller is a harness that would read \
         bytecode not implementing its source"
    );
}

/// **An announced process warns, an unannounced one refuses, and the
/// announcement really flips the fact.**
///
/// The two halves of `for_this_process` checked separately: the decision, with
/// no global read, and the store the shipped editor's `main` performs.
///
/// GNU's answer for a shipped editor is a message and a load
/// (`src/lread.c:1379`) -- an editor that refused to start over one stale
/// `.elc` would be a worse trade, and a divergence.  GNU's answer for a
/// harness is `make`, which cannot leave a stale `.elc` behind at all; a
/// refusal is this port's stand-in, because `cargo nextest run` compiles no
/// Lisp.
#[test]
fn announcing_a_shipped_editor_downgrades_the_refusal_to_gnus_warning() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        StaleBytecodePolicy::for_announcement(true),
        StaleBytecodePolicy::Warn,
        "a shipped editor must do what GNU does: name the file and start"
    );
    assert_eq!(
        StaleBytecodePolicy::for_announcement(false),
        StaleBytecodePolicy::for_test_harness(),
        "anything that has not announced itself is treated as a harness"
    );

    // And the store the editor's `main` performs really is what
    // `for_this_process' reads.  Withdrawn again immediately so that a
    // `cargo test' run -- which shares one process across tests, unlike the
    // nextest this project mandates -- cannot be left order-dependent.
    assert_eq!(
        StaleBytecodePolicy::for_this_process(),
        StaleBytecodePolicy::for_announcement(false),
        "this test binary is not a shipped editor"
    );
    super::load::announce_shipped_editor_process();
    let announced = StaleBytecodePolicy::for_this_process();
    super::load::withdraw_shipped_editor_announcement();
    assert_eq!(
        announced,
        StaleBytecodePolicy::Warn,
        "`announce_shipped_editor_process' did not reach `for_this_process'"
    );
    assert_eq!(
        StaleBytecodePolicy::for_this_process(),
        StaleBytecodePolicy::for_announcement(false),
        "the withdrawal did not restore the default"
    );
}
