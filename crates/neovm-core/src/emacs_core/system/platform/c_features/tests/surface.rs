//! Standing check for the `Fprovide` half of a build option.
//!
//! Ledger 189 measured what one `#ifdef` decides in GNU -- "the C variable
//! surface, the `Fprovide`, and which `loadup.el` branch runs" -- for the
//! window system.  Ledger 190 measured the subr surface.  This file owns the
//! `Fprovide` itself, for **every** feature GNU's `src/*.c` provides, because
//! `(featurep 'X)` is what GNU's own Lisp asks to decide whether a capability
//! is there, and a `t` this build cannot back is worse than an error: the
//! caller believes it.
//!
//! GNU's `lisp/net/tramp-gvfs.el:123` is the shape:
//!
//! ```elisp
//! (defconst tramp-gvfs-enabled
//!   (ignore-errors
//!     (and (featurep 'dbusbind)
//!          (tramp-compat-funcall 'dbus-get-unique-name :session)
//!          ...)))
//! ```
//!
//! -- the same "`featurep`/`fboundp` as the build test" ledger 190 found at
//! `lisp/t-mouse.el:49`, and `lisp/net/dbus.el` repeats it at seven sites as
//! `(or (featurep 'dbusbind) (signal 'dbus-error (list "Emacs not compiled
//! with dbus support")))`.  A build that answers `t` there does not gain a
//! capability; it loses GNU's own detection AND GNU's own honest error.
//!
//! Ledger 192.

use crate::emacs_core::c_features::{GnuGuard, HereDecision, gnu_c_features};
use crate::test_utils::runtime_startup_eval_one;

/// Every `Fprovide` GNU's C makes, and the seed, sorted.
///
/// Measured on GNU 31.0.90 (mirror `0ee48ac4df2`) with
///
/// ```text
/// grep -rh 'Fprovide (' src/*.c src/*.m | sed 's/.*Fprovide (//;s/,.*//' | sort -u
/// ```
///
/// -- **35 call sites, 29 distinct names** -- plus `emacs`, which is not an
/// `Fprovide` at all but `Vfeatures = list1 (Qemacs)` at `src/fns.c:6820`.
/// A name here with no row in [`gnu_c_features`] is a feature nobody decided
/// about, which is the hole ledger 192 found `dbusbind` sitting in.
///
/// **`src/*.m` is load-bearing in that command, and ledger 197 is why.**
/// Ledger 192 globbed `src/*.c` alone and reported "32 call sites, 26 distinct
/// names".  `src/nsterm.m` is Objective-C, so its three `Fprovide`s --
/// `ns` (`:11744`), `cocoa` (`:11757`), `gnustep` (`:11760`) -- were outside
/// the glob, and 32 + 3 = 35, 26 + 3 = 29 exactly.  All three are names
/// **neither** editor provides, so no diff of two binaries could ever have
/// found them missing: 190's blind spot one level further out, and 173's law
/// again -- a predicate over rows that exist cannot see a row never written.
///
/// `Fprovide` and the seed are the *only* two ways onto `features` in GNU's C:
/// `grep -rn Vfeatures src/*.c src/*.m src/*.h` finds assignment at
/// `src/fns.c:3751` (inside `Fprovide`), `src/fns.c:6820` (the seed) and
/// `src/eval.c:2438` (`unbind_to` restoring the autoload queue), and nowhere
/// else.  So this list is complete, not merely long.
const GNU_C_PROVIDES: &[&str] = &[
    "android",
    "cairo",
    "cocoa",
    "dbusbind",
    "dynamic-setting",
    "emacs",
    "font-render-setting",
    "gfilenotify",
    "gnustep",
    "gtk",
    "haiku",
    "inotify",
    "kqueue",
    "lcms2",
    "make-network-process",
    "motif",
    "move-toolbar",
    "multi-tty",
    "native-compile",
    "ns",
    "pgtk",
    "system-font-setting",
    "threads",
    "tty-child-frames",
    "w32",
    "w32notify",
    "x",
    "x-toolkit",
    "xinput2",
    "xwidget-internal",
];

/// The table has a row for every one of them, and no row for anything else.
///
/// This is the sweep ledger 192 was asked for, kept: a feature GNU provides
/// and this table has no opinion about cannot be audited, and a row for a name
/// GNU never provides is this port inventing a feature.
#[test]
fn the_table_covers_exactly_the_features_gnus_c_provides() {
    crate::test_utils::init_test_tracing();
    let mut rows: Vec<&str> = gnu_c_features().iter().map(|f| f.name).collect();
    rows.sort_unstable();
    assert_eq!(rows, GNU_C_PROVIDES);
}

/// No feature is advertised without something behind it.
///
/// GNU cannot advertise a capability it did not build, because one `#ifdef`
/// compiles both the `Fprovide` and the implementation it stands for.  Here the
/// two are separate pieces of Rust, so the rule is a test: a provided row must
/// carry a citation of the code that answers for it.
#[test]
fn every_provided_feature_names_what_backs_it() {
    crate::test_utils::init_test_tracing();
    let mut provided = 0usize;
    for row in gnu_c_features() {
        if !row.here.provided() {
            continue;
        }
        provided += 1;
        match row.here {
            HereDecision::UnconditionalInGnu => assert_eq!(
                row.gnu_guard,
                GnuGuard::Unconditional,
                "{} is provided here on the grounds that GNU provides it in \
                 every build, but its own row says GNU guards it",
                row.name
            ),
            HereDecision::Implemented { by, .. } => assert!(
                by.len() > 20 && by.contains(".rs"),
                "{} claims an implementation but does not cite one: {by:?}",
                row.name
            ),
            HereDecision::DetectedAtBuildTime { cfg, .. } => assert!(
                cfg.contains("build.rs"),
                "{} claims a build-time probe but does not cite one: {cfg:?}",
                row.name
            ),
            HereDecision::NotBuilt { .. } => unreachable!("filtered above"),
        }
    }
    assert!(
        provided >= 5,
        "only {provided} features provided; the filter is eating rows"
    );
}

/// Every row this build does NOT provide says why, and the reason is about a
/// capability rather than about the list.
#[test]
fn every_absent_feature_says_why() {
    crate::test_utils::init_test_tracing();
    for row in gnu_c_features() {
        if let HereDecision::NotBuilt { because } = row.here {
            assert!(
                because.len() > 30,
                "{} is absent with no reason worth reading: {because:?}",
                row.name
            );
        }
    }
}

/// Every row cites a `file:line` in GNU's `src/`.
///
/// `.m` as well as `.c`, and ledger 197 is why: this assertion carried ledger
/// 192's "GNU's C is `src/*.c`" assumption too, so it would have *rejected* the
/// three correct `src/nsterm.m` rows the same glob had already dropped.  The
/// wrong premise sat in two places -- the glob that built the list and the
/// predicate that validates the rows -- and the second would have argued the
/// first was right.
#[test]
fn every_row_cites_gnus_own_site() {
    crate::test_utils::init_test_tracing();
    for row in gnu_c_features() {
        assert!(
            row.gnu_site.starts_with("src/")
                && (row.gnu_site.contains(".c:") || row.gnu_site.contains(".m:")),
            "{} cites {:?}, which is not a src/*.c or src/*.m line",
            row.name,
            row.gnu_site
        );
    }
}

/// The three Objective-C rows are present, and each is absent from this build.
///
/// The regression pin for ledger 197's finding.  These are the only rows whose
/// `gnu_site` is a `.m` file, so a future re-narrowing of either the
/// enumeration glob or `every_row_cites_gnus_own_site` drops exactly these and
/// this test names them.
#[test]
fn the_nsterm_objective_c_rows_are_in_the_table() {
    crate::test_utils::init_test_tracing();
    for (name, site) in [
        ("ns", "src/nsterm.m:11744"),
        ("cocoa", "src/nsterm.m:11757"),
        ("gnustep", "src/nsterm.m:11760"),
    ] {
        let row = gnu_c_features()
            .into_iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("the table lost its {name} row"));
        assert_eq!(row.gnu_site, site, "{name} cites the wrong line");
        assert!(
            !row.here.provided(),
            "{name} is a NeXTstep window-system feature and this build has no NS terminal"
        );
    }
}

/// `dbusbind` is absent, and the row says the reason is the missing transport.
///
/// The regression pin for ledger 192: putting the name back requires editing
/// this row, and the only variants that provide it demand a citation.
#[test]
fn dbusbind_is_absent_and_its_row_names_the_missing_transport() {
    crate::test_utils::init_test_tracing();
    let row = gnu_c_features()
        .into_iter()
        .find(|f| f.name == "dbusbind")
        .expect("the table has a dbusbind row");
    assert_eq!(row.gnu_guard, GnuGuard::BuildOption("HAVE_DBUS"));
    assert!(!row.here.provided());
    let HereDecision::NotBuilt { because } = row.here else {
        panic!("dbusbind is provided again: {:?}", row.here);
    };
    assert!(because.contains("no D-Bus transport"), "{because:?}");
}

/// The derived list is exactly the one this build had before ledger 192, less
/// `dbusbind`, in GNU's own order.
///
/// `features` reads newest-provided first (`src/fns.c:3751` conses), so the
/// order is a fact about `main`'s `syms_of_*` sequence and is observable from
/// Lisp.  This pins that the table's row order reproduces it.
#[test]
fn the_derived_list_keeps_gnus_relative_order() {
    crate::test_utils::init_test_tracing();
    let names = crate::emacs_core::c_features::initial_feature_names();
    let mut expected = vec!["threads"];
    if cfg!(neomacs_have_wkwebview) {
        // macOS has the native WKWebView backend; GNU's table puts
        // xwidget-internal immediately after threads.
        expected.push("xwidget-internal");
    }
    // GNU calls `syms_of_inotify` then `syms_of_kqueue` (src/emacs.c:2465,
    // :2469), and `features` reads newest-provided first, so kqueue's slot is
    // immediately before inotify's; a build provides at most one of the two.
    if cfg!(target_os = "macos") {
        expected.push("kqueue");
    }
    if cfg!(target_os = "linux") {
        expected.push("inotify");
    }
    if cfg!(neomacs_have_lcms2) {
        expected.push("lcms2");
    }
    expected.extend([
        "multi-tty",
        "make-network-process",
        "tty-child-frames",
        "emacs",
    ]);
    assert_eq!(names, expected);
}

/// GNU's `syms_of_dbusbind` is the whole of `src/dbusbind.c` behind one
/// `#ifdef HAVE_DBUS` (`src/dbusbind.c:21`, `:2178`), called from
/// `src/emacs.c:2477-2479` behind the same one.  A build without the option
/// therefore has none of the six subrs, none of the nine `DEFVAR`s, no
/// `dbus-error` conditions (`src/dbusbind.c:2013-2017`) and none of
/// `keyboard.c`'s four `#ifdef HAVE_DBUS` sites.
///
/// This port links no libdbus and has no D-Bus transport, so it is in that
/// configuration and every one of those answers must be the absent one.
///
/// The last element is the whole of `while-no-input-ignore-events` rather than
/// a `memq`, because GNU builds it in one function
/// (`init_while_no_input_ignore_events`, `src/keyboard.c:13315-13336`) whose
/// eleven-name base list and trailing `sleep-event` are guarded by nothing:
/// asking only about `dbus-event` would have left the two names that were
/// missing from that base list here unmeasured, which is how this pin found
/// them.
#[test]
fn without_a_dbus_transport_the_whole_dbusbind_surface_is_absent() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one(
        "(list
           (featurep 'dbusbind)
           (mapcar #'fboundp '(dbus--init-bus dbus-get-unique-name
                               dbus-message-internal dbus--fd-open
                               dbus--fd-close dbus--registered-fds))
           (mapcar #'boundp '(dbus-compiled-version dbus-runtime-version
                              dbus-message-type-invalid
                              dbus-message-type-method-call
                              dbus-message-type-method-return
                              dbus-message-type-error
                              dbus-message-type-signal
                              dbus-registered-objects-table
                              dbus-debug))
           (get 'dbus-error 'error-conditions)
           (lookup-key special-event-map [dbus-event])
           while-no-input-ignore-events)",
    );
    assert_eq!(
        result,
        "OK (nil (nil nil nil nil nil nil) \
         (nil nil nil nil nil nil nil nil nil) nil nil \
         (sleep-event thread-event file-notify select-window help-echo \
         move-frame iconify-frame make-frame-visible focus-in focus-out \
         config-changed-event selection-request monitors-changed \
         toolkit-theme-changed))",
        "GNU without HAVE_DBUS declares none of this; a value invented here is \
         believed by every `(featurep 'dbusbind)' caller in GNU's own Lisp"
    );
}

/// The anti-vacuity half: the same probe run against features this build
/// really does have must answer `t`, or the test above is passing because
/// `runtime_startup_eval_one` returned nothing useful.
#[test]
fn the_features_this_build_really_has_still_answer_t() {
    crate::test_utils::init_test_tracing();
    // The file-notification feature is selected with the native backend,
    // exactly one per build.
    let file_notification_probe = if cfg!(target_os = "macos") {
        "(and (featurep 'kqueue) (fboundp 'kqueue-add-watch))"
    } else if cfg!(target_os = "windows") {
        "(and (featurep 'w32notify) (fboundp 'w32notify-add-watch))"
    } else {
        "(and (featurep 'inotify) (fboundp 'inotify-add-watch))"
    };
    let result = runtime_startup_eval_one(&format!(
        "(list (featurep 'emacs)
               (featurep 'multi-tty)
               (featurep 'make-network-process)
               (featurep 'tty-child-frames)
               (and (featurep 'threads) (fboundp 'make-thread))
               {file_notification_probe})",
    ));
    assert_eq!(result, "OK (t t t t t t)");
}

/// The Lisp-visible consequence of the kqueue row: `filenotify.el` picks its
/// backend from `featurep` alone (lisp/filenotify.el:36-41), and with no
/// backend `file-notify-add-watch` signals `("No file notification package
/// available")` (lisp/filenotify.el:456-457) -- which is what every
/// `workspace/didChangeWatchedFiles` registration (nixd through lsp-mode or
/// eglot) hit on macOS while this build advertised no file-notification
/// feature there.
#[cfg(target_os = "macos")]
#[test]
fn filenotify_selects_kqueue_on_macos() {
    crate::test_utils::init_test_tracing();
    let result = runtime_startup_eval_one("(progn (require 'filenotify) file-notify--library)");
    assert_eq!(result, "OK kqueue");
}

/// **The table is the only thing that decides a C-level feature.**
///
/// This is a scan, not a list: it boots a runtime, reads `features`, and
/// compares its intersection with GNU's 30 C-level names against the table's
/// own verdict, row by row.  A second site anywhere in the workspace that puts
/// one of those names on `features` -- or takes one off -- fails here and is
/// named, whatever crate it lives in.
///
/// Ledger 197 found two such sites and deleted them, because both defeated the
/// property ledger 192 built [`HereDecision`] for.  A reason enum with no
/// "yes, because the list says so" variant only makes an unbacked claim hard to
/// spell *in the table*; it says nothing about a `provide` somewhere else, and
/// there were two:
///
/// * `crates/neovm-core/src/emacs_core/lisp/native/builtins/mod.rs` provided `inotify` behind
///   `INOTIFY_FEATURE_AVAILABLE`, a `const bool = true` with no `cfg` on it,
///   while the table's `inotify` row is `cfg!(target_os = "linux")`.  On any
///   non-Linux target the table said absent and that line advertised it anyway
///   -- a live contradiction, invisible here because this host is Linux.
/// * `crates/neomacs/src/frame_layout.rs` provided `tty-child-frames` on live-TTY
///   startup, which the table already provides unconditionally, so the call was
///   a dedupe no-op.  Its function was `pub` with exactly one caller, so
///   ledger 186's rule applies: rustc could never have reported it dead.
///
/// The anti-vacuity halves matter as much as the assertion.  `features` after
/// loadup is over a hundred names, so a runtime that failed to boot is a red
/// rather than two satisfied empty-set comparisons.
#[test]
fn no_site_outside_the_table_decides_a_c_level_feature() {
    crate::test_utils::init_test_tracing();

    let names: Vec<&str> = gnu_c_features().iter().map(|f| f.name).collect();
    let probe = format!(
        "(list (length features)
               (delq nil (mapcar (lambda (f) (and (featurep f) f)) '({}))))",
        names.join(" ")
    );
    let result = runtime_startup_eval_one(&probe);

    let expected_present: Vec<&str> = gnu_c_features()
        .iter()
        .filter(|f| f.here.provided())
        .map(|f| f.name)
        .collect();
    assert!(
        expected_present.len() >= 5,
        "the table provides only {} features; the filter is eating rows",
        expected_present.len()
    );

    let body = result
        .strip_prefix("OK (")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("probe did not evaluate: {result}"));
    let (total, present) = body
        .split_once(" (")
        .unwrap_or_else(|| panic!("probe shape changed: {result}"));
    let total: usize = total
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("`(length features)' was not a number: {result}"));
    assert!(
        total > 50,
        "`features' has only {total} entries, so this runtime did not finish \
         loadup and the comparison below would be vacuous"
    );

    let present: Vec<&str> = present.trim_end_matches(')').split_whitespace().collect();
    assert_eq!(
        present, expected_present,
        "the C-level features this runtime advertises are not the ones \
         `gnu_c_features' decided.  Extra names come from a `provide' outside \
         the table; missing names mean a provided row never reached `features'"
    );
}

/// `xwidget-internal` must follow the backend that is really compiled in.
///
/// The WKWebView adapter lives behind `neomacs-webview`'s `webview` feature,
/// which `neomacs` forwards to `neovm-core/webview`; `build.rs` sets
/// `neomacs_have_wkwebview` only when that feature is on AND the target is
/// macOS.  Before this pin the cfg was set on every macOS build, so a binary
/// with no backend still told `lisp/xwidget.el` the layer was there, and
/// `xwidget-webkit-browse-url` failed at creation with `NotBuilt`.
#[test]
fn wkwebview_cfg_follows_the_webview_feature_on_macos() {
    assert_eq!(
        cfg!(neomacs_have_wkwebview),
        cfg!(all(target_os = "macos", feature = "webview")),
        "neomacs_have_wkwebview must be exactly `webview` feature AND macOS"
    );
}
