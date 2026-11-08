//! MAC-X11-SUBRS ratchet: GNU's mac/NS build must keep leaving X11-only subrs
//! void, and Neo must not register them there.
//!
//! The authoritative set is the LIVE pinned GNU 31.1 NS build (a360712c9d27,
//! --with-ns --without-x), not the C object sets alone: a fboundp dump of all
//! 68 XOBJ DEFUN names (xterm.c xfns.c xmenu.c xselect.c xrdb.c xsmfns.c
//! xsettings.c gtkutil.c) on that build reports 41 void.  Two XOBJ DEFUNs are
//! nevertheless bound on NS by Lisp preload (x-begin-drag and x-file-dialog),
//! and x-file-dialog is not registered by Neo at all — so the divergence is
//! the 36 names below, which Neo used to register unconditionally while GNU NS
//! leaves them void.
//!
//! The plan's cfg column is the single source of truth for registration
//! activity (HostCfgClass::is_active, the H3 snapshot and the T6 trace all
//! read it).  Those 36 rows carry the `not-macos` class: active on Linux and
//! Windows (Neo's previous behaviour there is unchanged) and inactive on the
//! macOS/NS surface.  x-begin-drag must stay `always`, because GNU's NS build
//! also binds it through Lisp preload.
//!
//! On Linux every one of these rows stays active, so the registration trace
//! and the generated artifacts are unchanged there; the NS build drops all 36.

use std::path::{Path, PathBuf};

/// The 36 X11-only subrs that the live pinned GNU NS build leaves void and
/// that Neo registers.  (x-begin-drag is XOBJ-only but Lisp-bound on NS;
/// x-file-dialog is Lisp-bound there and unregistered by Neo.)
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

#[test]
fn x_only_gnu_subrs_never_register_on_the_ns_surface() {
    let rows = canonical_rows();
    let cfg_of = |name: &str| {
        rows.iter()
            .find(|(row_name, _)| row_name == name)
            .map(|(_, cfg)| cfg.as_str())
    };
    let mut problems = Vec::new();
    for name in NS_VOID_REGISTERED {
        match cfg_of(name) {
            Some("not-macos") => {}
            Some(other) => problems.push(format!("{name}: cfg {other:?}, expected \"not-macos\"")),
            None => problems.push(format!("{name}: missing from the canonical plan")),
        }
    }
    assert!(
        problems.is_empty(),
        "the live GNU NS build leaves these X-only subrs void, so Neo must not \
         register them on the NS surface:\n  {}",
        problems.join("\n  ")
    );
    assert_eq!(
        NS_VOID_REGISTERED.len(),
        36,
        "the live NS-void set Neo registers is 36; update this test (and the \
         ledger) deliberately if membership changes"
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
fn host_cfg_surface_classes_classify_the_ns_surface() {
    use neovm_eval::host_cfg::HostCfgClass;
    assert_eq!(
        HostCfgClass::from_label("not-macos"),
        Some(HostCfgClass::NotMacos)
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
    assert!(
        !(HostCfgClass::NotMacos.is_active() && HostCfgClass::Macos.is_active()),
        "the not-macos surface class must exclude every macOS/NS build"
    );
}
