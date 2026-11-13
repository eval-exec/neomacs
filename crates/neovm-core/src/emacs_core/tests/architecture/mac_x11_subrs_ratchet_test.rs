//! MAC-X11-SUBRS / MX-3 ratchet: GNU's mac/NS build must keep leaving X11-only
//! subrs void, and GNU's W32 build binds only the four w32fns.c DEFUNs of that
//! set, so Neo must register the other 32 on the X11 surface only.
//!
//! The authoritative set is the LIVE pinned GNU 31.1 NS build (a360712c9d27,
//! --with-ns --without-x), not the C object sets alone: a fboundp dump of all
//! 68 XOBJ DEFUN names (xterm.c xfns.c xmenu.c xselect.c xrdb.c xsmfns.c
//! xsettings.c gtkutil.c) on that build reports 41 void.  Two XOBJ DEFUNs are
//! nevertheless bound on NS by Lisp preload (x-begin-drag and x-file-dialog),
//! so the divergence is the 36 names below, which Neo registered
//! unconditionally while GNU NS leaves them void.
//!
//! MX-3 splits the 36 by GNU's W32 surface:
//! * four are w32 DEFUNs (x-change-window-property, x-delete-window-property,
//!   x-synchronize, x-window-property; w32fns.c), so they are bound on X11 and
//!   W32 and void on NS -> the `not-macos` class;
//! * the other 32 have no w32 DEFUN, and no Lisp file GNU loads for
//!   windows-nt binds them (the static table below, checked against
//!   loadup.el's windows-nt branch and term/w32-win.el's requires), so they
//!   are bound on the X11 surface only -> the `x11-only` class.
//!
//! The plan's cfg column is the single source of truth for registration
//! activity (HostCfgClass::is_active, the H3 snapshot and the T6 trace all
//! read it).  On Linux every one of the 36 stays active, so the registration
//! trace and the generated artifacts are unchanged there; the NS build drops
//! all 36 and the W32 build drops the 32.  x-begin-drag stays `always`,
//! because GNU's NS build also binds it through Lisp preload.

use std::path::{Path, PathBuf};

/// The 36 X11-only subrs that the live pinned GNU NS build leaves void and
/// that Neo registers.  (x-begin-drag is XOBJ-only but Lisp-bound on NS;
/// x-file-dialog is Lisp-bound there and Neo binds it through the dump-safe
/// Lisp preload, not this table.)
const NS_VOID_REGISTERED: &[&str] = &[
    "font-get-system-font",
    "font-get-system-normal-font",
    "handle-save-session",
    "tool-bar-get-system-style",
    "x-backspace-delete-keys-p",
    "x-change-window-property",
    "x-delete-window-property",
    "x-disown-selection-internal",
    "x-display-monitor-attributes-list",
    "x-display-set-last-user-time",
    "x-double-buffered-p",
    "x-export-frames",
    "x-frame-edges",
    "x-frame-geometry",
    "x-frame-list-z-order",
    "x-frame-restack",
    "x-get-atom-name",
    "x-get-local-selection",
    "x-get-modifier-masks",
    "x-get-selection-internal",
    "x-internal-focus-input-context",
    "x-menu-bar-open-internal",
    "x-mouse-absolute-pixel-position",
    "x-own-selection-internal",
    "x-register-dnd-atom",
    "x-selection-exists-p",
    "x-selection-owner-p",
    "x-send-client-message",
    "x-server-input-extension-version",
    "x-set-mouse-absolute-pixel-position",
    "x-synchronize",
    "x-translate-coordinates",
    "x-uses-old-gtk-dialog",
    "x-window-property",
    "x-window-property-attributes",
    "x-wm-set-size-hint",
];

/// The four names of the 36 that GNU's w32 backend defines as DEFUNs in
/// w32fns.c, so a W32 build binds them and `not-macos` is right for them.
const W32_C_DEFUNS: &[&str] = &[
    "x-change-window-property",
    "x-delete-window-property",
    "x-synchronize",
    "x-window-property",
];

/// Every Lisp file GNU's windows-nt build loads **for its window system**:
/// the loadup.el windows-nt branch (loadup.el:323-340), the
/// `(if (fboundp 'x-create-frame) ...)` blocks that a W32 build takes
/// (w32fns.c:6241 defines x-create-frame; GNU's loadup.el:291-300, :363-366
/// plus Neo's dump-safe term/common-win preload), and
/// the file-backed `(require ...)` forms of term/w32-win.el:72-78 (`fontset`
/// is a built-in feature with no `.el` file).
///
/// The unconditional dump files need no row: they are the same files the NS
/// dump loads, and the live pinned NS fboundp dump already proves none of the
/// 36 is Lisp-bound there, so none can be Lisp-bound on W32 through them.
/// The W32-specific delta is this table, and MX-3's `x11-only` claim rests on
/// none of these binding one of the 32 in Lisp.
const W32_LOADED_LISP: &[&str] = &[
    // loadup.el windows-nt branch.
    "lisp/term/common-win.el",
    "lisp/w32-vars.el",
    "lisp/term/w32-win.el",
    "lisp/disp-table.el",
    "lisp/term/w32-nt.el",
    "lisp/w32-fns.el",
    "lisp/ls-lisp.el",
    "lisp/dos-w32.el",
    "lisp/touch-screen.el",
    // The two (fboundp 'x-create-frame) GUI blocks.
    "lisp/fringe.el",
    "lisp/emacs-lisp/regexp-opt.el",
    "lisp/image.el",
    "lisp/international/fontset.el",
    "lisp/dnd.el",
    "lisp/tool-bar.el",
    "lisp/mwheel.el",
    // term/w32-win.el direct requires that have a file.
    "lisp/frame.el",
    "lisp/mouse.el",
    "lisp/scroll-bar.el",
    "lisp/select.el",
    "lisp/menu-bar.el",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn canonical_rows() -> Vec<(String, String)> {
    let names_path = workspace_root().join("crates/neovm-eval/src/canonical_subr_names.tsv");
    let text = std::fs::read_to_string(&names_path).expect("canonical subr names");
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            (fields.len() == 5).then(|| (fields[0].to_owned(), fields[4].to_owned()))
        })
        .collect()
}

/// The 32 names bound on the X11 surface only: the NS-void set minus the four
/// w32 DEFUNs.
fn x11_only_names() -> Vec<&'static str> {
    NS_VOID_REGISTERED
        .iter()
        .copied()
        .filter(|name| !W32_C_DEFUNS.contains(name))
        .collect()
}

/// Whether `text` binds `name` as a Lisp function.  Over-approximates (a
/// comment mentioning a definition would count); that is the fail-closed
/// direction for the x11-only claim.
fn lisp_defines(text: &str, name: &str) -> bool {
    [
        format!("(defun {name}"),
        format!("(defsubst {name}"),
        format!("(defmacro {name}"),
        format!("(cl-defun {name}"),
        format!("(defalias '{name}"),
        format!("(defalias (quote {name}"),
    ]
    .iter()
    .any(|form| text.contains(form))
}

#[test]
fn x_only_gnu_subrs_carry_the_surface_class_of_their_gnu_hosts() {
    let rows = canonical_rows();
    let cfg_of = |name: &str| {
        rows.iter()
            .find(|(row_name, _)| row_name == name)
            .map(|(_, cfg)| cfg.as_str())
    };
    let mut problems = Vec::new();
    for name in x11_only_names() {
        match cfg_of(name) {
            Some("x11-only") => {}
            Some(other) => problems.push(format!("{name}: cfg {other:?}, expected \"x11-only\"")),
            None => problems.push(format!("{name}: missing from the canonical plan")),
        }
    }
    for name in W32_C_DEFUNS {
        match cfg_of(name) {
            Some("not-macos") => {}
            Some(other) => problems.push(format!(
                "{name}: cfg {other:?}, expected \"not-macos\" (w32fns.c DEFUN)"
            )),
            None => problems.push(format!("{name}: missing from the canonical plan")),
        }
    }
    assert_eq!(
        NS_VOID_REGISTERED.len(),
        36,
        "the live NS-void set Neo registers is 36; update this test (and the \
         ledger) deliberately if membership changes"
    );
    assert_eq!(
        W32_C_DEFUNS.len(),
        4,
        "exactly four of the 36 are w32fns.c DEFUNs; update with GNU evidence"
    );
    assert_eq!(
        x11_only_names().len(),
        32,
        "the other 32 are bound on the X11 surface only"
    );
    assert!(
        problems.is_empty(),
        "the live GNU surfaces decide these classes: 36 names are void on NS, \
         four of them are w32 DEFUNs and the other 32 are X11-only:\n  {}",
        problems.join("\n  ")
    );
}

/// MX-3 evidence: GNU's W32 build has no C DEFUN for the 32 (only the four
/// w32fns.c names above), so a Lisp definition in the windows-nt dump would
/// be the only other way they could be bound there.  None is.
#[test]
fn w32_dump_lisp_binds_none_of_the_x11_only_subrs() {
    let root = workspace_root();
    let mut problems = Vec::new();
    for file in W32_LOADED_LISP {
        let path = root.join(file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            problems.push(format!("{file}: missing or unreadable"));
            continue;
        };
        for name in x11_only_names() {
            if lisp_defines(&text, name) {
                problems.push(format!("{file}: binds {name} in Lisp"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "a Lisp binding in the windows-nt dump would make `x11-only` \
         under-register on W32:\n  {}",
        problems.join("\n  ")
    );
}

/// The balanced parenthesized form that starts at `start` (whose first byte
/// is `(`).  The loadup window-system forms carry no parens inside a string
/// or comment, and an over-long or short scan fails the equality below in the
/// fail-closed direction.
fn balanced_form<'a>(text: &'a str, start: usize) -> &'a str {
    let mut depth = 0usize;
    for (offset, ch) in text[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &text[start..start + offset + ch.len_utf8()];
                }
            }
            _ => {}
        }
    }
    &text[start..]
}

/// Every `(load "NAME")` call inside `form`, as `lisp/NAME.el`.  The parser
/// matches the call textually: a load line may carry the enclosing forms'
/// closing parens, so a whole-line prefix/suffix test would miss it.
fn loaded_files(form: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(offset) = form[from..].find("(load \"") {
        let start = from + offset + "(load \"".len();
        let Some(close) = form[start..].find('"') else {
            break;
        };
        let name = &form[start..start + close];
        let after = &form[start + close + 1..];
        if after.trim_start().starts_with(')') {
            out.push(format!("lisp/{name}.el"));
        }
        from = start + close + 1;
    }
    out
}

/// The static table must be exactly the window-system Lisp set GNU's
/// windows-nt build loads: the loadup.el windows-nt branch, the
/// `(if (fboundp 'x-create-frame))` blocks a W32 build takes (w32fns.c:6241
/// defines x-create-frame; GNU's two plus Neo's common-win preload), and the
/// file-backed requires of term/w32-win.el.
/// A new file in any of those places fails this test until the x11-only
/// evidence is re-derived, and a stale table row fails it too.
#[test]
fn w32_lisp_table_covers_every_window_system_load() {
    let root = workspace_root();
    let loadup = std::fs::read_to_string(root.join("lisp/loadup.el")).expect("loadup.el");
    let mut expected = std::collections::BTreeSet::new();

    let branch_start = loadup
        .find("(if (or (eq system-type 'windows-nt)")
        .expect("loadup.el windows-nt branch");
    expected.extend(loaded_files(balanced_form(&loadup, branch_start)));

    // Neo's loadup preloads touch-screen unconditionally at GNU's relative
    // position ("Every GUI-capable GNU build (X, pgtk, w32 and android)
    // preloads touch-screen from its window-system branch here"), so it is
    // part of the windows-nt set even though Neo's own W32 branch no longer
    // names it.
    if loadup
        .lines()
        .any(|line| line.trim() == "(load \"touch-screen\")")
    {
        expected.insert("lisp/touch-screen.el".to_owned());
    }

    let mut from = 0;
    let mut gui_blocks = 0;
    while let Some(offset) = loadup[from..].find("(if (fboundp 'x-create-frame)") {
        let start = from + offset;
        expected.extend(loaded_files(balanced_form(&loadup, start)));
        gui_blocks += 1;
        from = start + 1;
    }
    // GNU has two; Neo's dump-safe preload adds a third `(load
    // "term/common-win")` under the same predicate (DIVERGENCES 179), which
    // is already part of the set.
    assert_eq!(
        gui_blocks, 3,
        "loadup.el has three x-create-frame GUI blocks; a W32 build takes all"
    );

    let w32_win =
        std::fs::read_to_string(root.join("lisp/term/w32-win.el")).expect("term/w32-win.el");
    let mut requires = 0;
    for line in w32_win.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("(require '")
            && let Some(name) = name.split(')').next()
        {
            let file = format!("lisp/{name}.el");
            if root.join(&file).is_file() {
                expected.insert(file);
                requires += 1;
            }
        }
    }
    assert!(
        requires >= 6,
        "term/w32-win.el's file-backed requires must be parsed (got {requires})"
    );

    let table: std::collections::BTreeSet<String> = W32_LOADED_LISP
        .iter()
        .map(|file| (*file).to_owned())
        .collect();
    let missing: Vec<_> = expected.difference(&table).cloned().collect();
    let stale: Vec<_> = table.difference(&expected).cloned().collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "W32_LOADED_LISP must be exactly the window-system Lisp set GNU's \
         windows-nt build loads:\n  missing from the table: {missing:?}\n  \
         table rows with no loader: {stale:?}"
    );
}

/// GNU's NS build binds x-begin-drag through Lisp preload even though the
/// DEFUN lives only in xfns.c, so gating it would create a new NS divergence.
#[test]
fn x_begin_drag_stays_registered_on_every_surface() {
    let rows = canonical_rows();
    let cfg = rows
        .iter()
        .find(|(name, _)| name == "x-begin-drag")
        .map(|(_, cfg)| cfg.as_str());
    assert_eq!(
        cfg,
        Some("always"),
        "x-begin-drag is fboundp on the live GNU NS build (Lisp preload); it \
         must stay registered on every surface"
    );
}

#[test]
fn host_cfg_surface_classes_classify_the_ns_and_w32_surfaces() {
    use neovm_eval::host_cfg::HostCfgClass;
    assert_eq!(
        HostCfgClass::from_label("not-macos"),
        Some(HostCfgClass::NotMacos)
    );
    assert_eq!(
        HostCfgClass::from_label("x11-only"),
        Some(HostCfgClass::X11Only)
    );
    assert!(
        HostCfgClass::vocabulary_round_trips(),
        "the plan/snapshot cfg vocabulary must round-trip through label/from_label"
    );
    assert_eq!(
        HostCfgClass::NotMacos.is_active(),
        !cfg!(target_os = "macos"),
        "not-macos must be inactive exactly on the macOS/NS surface"
    );
    assert_eq!(
        HostCfgClass::X11Only.is_active(),
        !cfg!(windows) && !cfg!(target_os = "macos"),
        "x11-only must be active exactly on the X11 surface"
    );
    assert!(
        !(HostCfgClass::NotMacos.is_active() && HostCfgClass::Macos.is_active()),
        "the not-macos surface class must exclude every macOS/NS build"
    );
    assert!(
        !(HostCfgClass::X11Only.is_active() && HostCfgClass::Macos.is_active()),
        "the x11-only surface class must exclude every macOS/NS build"
    );
    assert!(
        !(HostCfgClass::X11Only.is_active() && HostCfgClass::Windows.is_active()),
        "the x11-only surface class must exclude every Windows build"
    );
}
