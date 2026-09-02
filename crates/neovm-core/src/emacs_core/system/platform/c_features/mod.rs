//! Every feature GNU's C `Fprovide`s, and what decides it here.
//!
//! # Why this is a table and not a `vec!` of names
//!
//! In GNU, a C-level feature is not a name someone chose to list.  It is a
//! *consequence*: `configure` sets a preprocessor symbol, the `#ifdef` compiles
//! (or does not compile) the `syms_of_*` that contains the `Fprovide`, and the
//! same `#ifdef` compiles the implementation the feature advertises.  One
//! switch decides both, so GNU cannot advertise a capability it did not build.
//!
//! This port had the two halves separated: the implementations lived behind
//! real conditions -- `cfg(neomacs_have_lcms2)` from a `build.rs` `pkg-config`
//! probe, `notify`-crate watches, real OS threads -- while the advertisement
//! was a hand-written `vec!["threads", "dbusbind", "inotify"]`.  Ledger 192
//! measured the result: **`dbusbind` was advertised with no D-Bus transport
//! behind it at all**, and GNU's own Lisp uses `(featurep 'dbusbind)` as *the*
//! build test (`lisp/net/tramp-gvfs.el:123`, `lisp/net/tramp-archive.el:124`,
//! `lisp/net/dbus.el` at seven sites, `lisp/net/secrets.el:793`,
//! `lisp/system-sleep.el:208`, `lisp/system-taskbar.el:224`,
//! `lisp/gnus/gnus-dbus.el:45`, `lisp/ps-samp.el:258`).
//!
//! So the list is derived from the table below, in which every row states what
//! decides the feature in GNU **and** what backs it here.  A row cannot be
//! provided without naming an implementation, because [`HereDecision`] has no
//! variant that says yes without one.  That is the compile-time half of the
//! rule; `c_features_test.rs` is the run-time half.
//!
//! # The order is GNU's, and it is observable
//!
//! `features` is built by consing (`src/fns.c:3751`), so the list reads
//! newest-provided first, and the order is a fact about `main`'s `syms_of_*`
//! sequence in `src/emacs.c`.  [`gnu_c_features`] is written in that observed
//! order -- `syms_of_threads` (`emacs.c:2490`) is last to provide and first in
//! the list, `Vfeatures = list1 (Qemacs)` (`src/fns.c:6820`) is the seed and
//! last in the list -- so filtering it preserves GNU's relative order for free.
//!
//! Ledger 192.

/// What decides, in GNU's C, whether the `Fprovide` runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GnuGuard {
    /// GNU runs it in every build.  Parity is not optional for these.
    Unconditional,
    /// A `configure` option, spelled as the preprocessor symbol it defines.
    BuildOption(&'static str),
}

/// Why this build does, or does not, make the same `Fprovide`.
///
/// There is deliberately **no** variant meaning "yes, because the list used to
/// say so".  Every provided row names the code that answers for the feature,
/// which is the thing ledger 192 found missing for `dbusbind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HereDecision {
    /// GNU provides it in every build, so this port must too.
    UnconditionalInGnu,
    /// The capability is implemented here; `by` names where.
    Implemented { by: &'static str },
    /// A `build.rs` probe decides it, exactly as `configure` decides GNU's --
    /// the one shape in this port that already matched GNU before ledger 192.
    DetectedAtBuildTime { cfg: &'static str, present: bool },
    /// This build does not have the capability, so it does not advertise it.
    /// That is not a gap: it is what GNU's own build without the option leaves,
    /// and it is what GNU's Lisp is written to detect.
    NotBuilt { because: &'static str },
}

impl HereDecision {
    /// Whether this build puts the feature on `features`.
    pub(crate) const fn provided(self) -> bool {
        match self {
            Self::UnconditionalInGnu | Self::Implemented { .. } => true,
            Self::DetectedAtBuildTime { present, .. } => present,
            Self::NotBuilt { .. } => false,
        }
    }
}

/// One `Fprovide` in GNU's `src/*.c`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GnuCFeature {
    pub(crate) name: &'static str,
    /// `file:line` of the `Fprovide` (or of the seed, for `emacs`).
    pub(crate) gnu_site: &'static str,
    pub(crate) gnu_guard: GnuGuard,
    pub(crate) here: HereDecision,
}

/// GNU 31.0.90 (`0ee48ac4df2`) makes **35** `Fprovide` calls in `src/*.c` and
/// `src/*.m`, naming **29** distinct features, plus the `emacs` seed: **30
/// rows**.  A name provided from more than one window-system backend cites the
/// backend a GNU/Linux build would use.
///
/// Ledger 192 wrote 32/26/27 here, from a `src/*.c` glob that could not see
/// `src/nsterm.m`.  Ledger 197 re-measured over `src/*.c src/*.m` and added the
/// three Objective-C rows; 32 + 3 = 35 and 26 + 3 = 29 exactly.
pub(crate) fn gnu_c_features() -> [GnuCFeature; 30] {
    use GnuGuard::{BuildOption, Unconditional};
    use HereDecision::{DetectedAtBuildTime, Implemented, NotBuilt, UnconditionalInGnu};

    /// This port has no X, GTK, PGTK, W32, NS, Haiku or Android terminal: it
    /// has its own `neo` backend.  Ledger 189 measured that whole branch and
    /// declined it with the cost named; these rows record the consequence for
    /// `features` rather than re-deciding it.
    const NO_GNU_WINDOW_SYSTEM: &str = "this port has no X/GTK/PGTK/W32/NS/Haiku/Android terminal -- ledger 189 \
         measured GNU's seventh-window-system branch and declined it";

    [
        GnuCFeature {
            name: "threads",
            gnu_site: "src/thread.c:1293",
            gnu_guard: BuildOption("THREADS_ENABLED"),
            here: Implemented {
                by: "crates/neovm-core/src/emacs_core/runtime/threads/mod.rs -- real OS threads, \
                     make-thread/make-mutex/condition-variable",
            },
        },
        GnuCFeature {
            name: "xwidget-internal",
            gnu_site: "src/xwidget.c:4003",
            gnu_guard: BuildOption("HAVE_XWIDGETS"),
            // macOS has a native inline web view: `neomacs-display-runtime/
            // src/backend/wkwebview' places a real WKWebView over the GPU
            // surface, using GNU's own placement algorithm from
            // `src/xwidget.c'.  What `xwidget-internal' advertises to
            // `lisp/xwidget.el' is that the xwidget layer is there to be used,
            // and on macOS it now is: `xwidget-webkit-browse-url' works end to
            // end on the primary frame.
            //
            // It is NOT complete GNU xwidget compatibility, and this row does
            // not claim to be.  Tracked in issue 300: no views in secondary
            // top-level frames; `xwidget-webkit-estimated-load-progress' is
            // dispatched rather than measured, because nothing produces
            // `InputEvent::WebKitLoadFinished' and there is no
            // `WKNavigationDelegate' or KVO; `xwidget-webkit-execute-script'
            // signals on its optional FUN, having no result channel back to
            // the Lisp thread; and keyboard focus is not handed off in either
            // direction.  The flag is advisory in any case -- `xwidget.el'
            // does not `require' this feature, its
            // `(require 'xwidget-internal)' being commented out at line 32 --
            // so what it decides is whether a configuration reaches for the
            // layer at all, which is the question macOS can now answer yes to.
            //
            // Linux keeps the old answer: its WPE path renders through
            // dma-buf and ledger 190's 13 missing subrs are still open there.
            here: DetectedAtBuildTime {
                cfg: "neomacs_have_wkwebview -- crates/neovm-core/build.rs detect_wkwebview(): \
                      macOS AND the `webview' feature, which compiles \
                      crates/neomacs-webview/src/platform/macos (a real WKWebView placed \
                      with GNU's own algorithm from src/xwidget.c); see issue 300",
                present: cfg!(neomacs_have_wkwebview),
            },
        },
        GnuCFeature {
            name: "w32notify",
            gnu_site: "src/w32notify.c:739",
            gnu_guard: BuildOption("HAVE_W32NOTIFY"),
            here: if cfg!(target_os = "windows") {
                Implemented {
                    by: "crates/neovm-core/src/emacs_core/lisp/native/builtins/file_notify/platform/windows -- \
                         w32notify-add-watch/-rm-watch/-valid-p over an explicit \
                         ReadDirectoryChangesW adapter, with GNU-compatible event shapes",
                }
            } else {
                NotBuilt {
                    because: "GNU's ReadDirectoryChangesW backend is Windows-only",
                }
            },
        },
        GnuCFeature {
            name: "dbusbind",
            gnu_site: "src/dbusbind.c:2175",
            gnu_guard: BuildOption("HAVE_DBUS"),
            here: NotBuilt {
                because: "there is no D-Bus transport here at all -- no libdbus, no \
                          connection, no serial numbers.  GNU's `configure.ac:3921-3942' \
                          sets HAVE_DBUS only when `dbus-1 >= 1.0' links, and the whole \
                          of `src/dbusbind.c' is inside that `#ifdef'.  Ledger 192 \
                          deleted the three fabricating subrs that stood here",
            },
        },
        GnuCFeature {
            name: "gfilenotify",
            gnu_site: "src/gfilenotify.c:335",
            gnu_guard: BuildOption("HAVE_GFILENOTIFY"),
            here: NotBuilt {
                because: "file notification here uses the native inotify/kqueue/\
                          ReadDirectoryChangesW adapters; GNU's gfilenotify is the GIO backend and a build \
                          picks exactly one (`configure.ac' --with-file-notification)",
            },
        },
        GnuCFeature {
            name: "kqueue",
            gnu_site: "src/kqueue.c:545",
            gnu_guard: BuildOption("HAVE_KQUEUE"),
            here: if cfg!(target_os = "macos") {
                Implemented {
                    by: "crates/neovm-core/src/emacs_core/lisp/native/builtins/file_notify -- \
                         kqueue-add-watch/-rm-watch/-valid-p over `rustix' typed kqueue \
                         vnode flags plus GNU-style directory snapshot diffs; GNU's macOS default is \
                         --with-file-notification=kqueue, so this is the feature \
                         `filenotify.el' expects to find there",
                }
            } else {
                NotBuilt {
                    because: "GNU's BSD/macOS file-notification backend; on GNU/Linux \
                              the `inotify' row is the one that answers here",
                }
            },
        },
        GnuCFeature {
            name: "inotify",
            gnu_site: "src/inotify.c:589",
            gnu_guard: BuildOption("HAVE_INOTIFY"),
            here: if cfg!(target_os = "linux") {
                Implemented {
                    by: "crates/neovm-core/src/emacs_core/lisp/native/builtins/file_notify/platform/linux -- \
                         direct typed inotify masks through the `inotify' crate; \
                         inotify-add-watch/-rm-watch/-valid-p",
                }
            } else {
                NotBuilt {
                    because: "GNU provides `inotify' only where the Linux syscall is \
                              (`configure.ac' --with-file-notification=inotify)",
                }
            },
        },
        GnuCFeature {
            name: "android",
            gnu_site: "src/androidterm.c:6984",
            gnu_guard: BuildOption("HAVE_ANDROID"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "haiku",
            gnu_site: "src/haikuterm.c:4884",
            gnu_guard: BuildOption("HAVE_HAIKU"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "pgtk",
            gnu_site: "src/pgtkterm.c:7502",
            gnu_guard: BuildOption("HAVE_PGTK"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        // `syms_of_nsterm` is `src/emacs.c:2422`, between `syms_of_pgtkterm`
        // (`:2430`) and `syms_of_w32term` (`:2400`) in `features` order.  These
        // three rows were missing until ledger 197: ledger 192 enumerated GNU's
        // `Fprovide`s with a `src/*.c` glob, and `nsterm.m` is Objective-C.
        // All three are names neither editor provides, so no comparison of two
        // binaries could have found them -- only re-reading GNU's source could.
        GnuCFeature {
            name: "cocoa",
            gnu_site: "src/nsterm.m:11757",
            gnu_guard: BuildOption("NS_IMPL_COCOA"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "gnustep",
            gnu_site: "src/nsterm.m:11760",
            gnu_guard: BuildOption("!NS_IMPL_COCOA (the #else arm of the same #ifdef)"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "ns",
            gnu_site: "src/nsterm.m:11744",
            gnu_guard: BuildOption("HAVE_NS"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "w32",
            gnu_site: "src/w32term.c:8396",
            gnu_guard: BuildOption("HAVE_NTGUI"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "lcms2",
            gnu_site: "src/lcms.c:601",
            gnu_guard: BuildOption("HAVE_LCMS2"),
            here: DetectedAtBuildTime {
                cfg: "neomacs_have_lcms2 -- neovm-core/build.rs:65-86 probes lcms2 with \
                      pkg-config exactly as `configure.ac' does, and \
                      builtins/lcms/mod.rs dlopens liblcms2 for the eight subrs",
                present: cfg!(neomacs_have_lcms2),
            },
        },
        GnuCFeature {
            name: "dynamic-setting",
            gnu_site: "src/xsettings.c:1417",
            gnu_guard: BuildOption("HAVE_X_WINDOWS/HAVE_PGTK/HAVE_HAIKU/HAVE_ANDROID"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "system-font-setting",
            gnu_site: "src/xsettings.c:1409",
            gnu_guard: BuildOption("(USE_CAIRO|HAVE_XFT) && (HAVE_GCONF|HAVE_GSETTINGS)"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "font-render-setting",
            gnu_site: "src/xsettings.c:1407",
            gnu_guard: BuildOption("USE_CAIRO || HAVE_XFT"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "cairo",
            gnu_site: "src/xfns.c:10553",
            gnu_guard: BuildOption("USE_CAIRO"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "gtk",
            gnu_site: "src/xfns.c:10540",
            gnu_guard: BuildOption("USE_GTK"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "motif",
            gnu_site: "src/xfns.c:10526",
            gnu_guard: BuildOption("USE_MOTIF"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "x-toolkit",
            gnu_site: "src/xfns.c:10524",
            gnu_guard: BuildOption("USE_X_TOOLKIT"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "xinput2",
            gnu_site: "src/xfns.c:10520",
            gnu_guard: BuildOption("HAVE_XINPUT2"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "x",
            gnu_site: "src/xfns.c:10498",
            gnu_guard: BuildOption("HAVE_X_WINDOWS"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "multi-tty",
            gnu_site: "src/terminal.c:722",
            gnu_guard: Unconditional,
            here: UnconditionalInGnu,
        },
        GnuCFeature {
            name: "move-toolbar",
            gnu_site: "src/frame.c:7889",
            gnu_guard: BuildOption("HAVE_WINDOW_SYSTEM && (!HAVE_EXT_TOOL_BAR || USE_GTK)"),
            here: NotBuilt {
                because: NO_GNU_WINDOW_SYSTEM,
            },
        },
        GnuCFeature {
            name: "make-network-process",
            gnu_site: "src/process.c:9094",
            gnu_guard: BuildOption("subprocesses"),
            here: Implemented {
                by: "crates/neovm-core/src/emacs_core/system/process/mod.rs -- real sockets, and \
                     `make_network_process_subfeatures' supplies GNU's SUBFEATURES \
                     list rather than nil",
            },
        },
        GnuCFeature {
            name: "tty-child-frames",
            gnu_site: "src/dispnew.c:7578",
            gnu_guard: Unconditional,
            here: UnconditionalInGnu,
        },
        GnuCFeature {
            name: "native-compile",
            gnu_site: "src/comp.c:5825",
            gnu_guard: BuildOption("HAVE_NATIVE_COMP"),
            here: NotBuilt {
                because: "no libgccjit native compiler; ledger 190 deleted the thirteen \
                          `comp--'/`native-elisp-load' subrs that stood without one, and \
                          the reference GNU (--with-native-compilation=no) agrees",
            },
        },
        GnuCFeature {
            name: "emacs",
            gnu_site: "src/fns.c:6820",
            gnu_guard: Unconditional,
            here: UnconditionalInGnu,
        },
    ]
}

/// The C-level features this build advertises, newest-provided first, which is
/// the order `features` reads in.
pub(crate) fn initial_feature_names() -> Vec<&'static str> {
    gnu_c_features()
        .into_iter()
        .filter(|feature| feature.here.provided())
        .map(|feature| feature.name)
        .collect()
}
