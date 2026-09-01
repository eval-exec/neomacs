//! GNU's post-image `init_*` sequence, transcribed.
//!
//! **The dump is written from inside `-l loadup`, so the image carries
//! mid-load state.**  `lisp/loadup.el` calls `dump-emacs-portable` while
//! loadup.el is itself still being loaded, so whatever a loader, a keyboard
//! or an allocator global happened to hold at that instant is what the image
//! carries.  GNU knows this and says so in `main` itself, at the line right
//! after the image is mapped in (src/emacs.c:1438-1442):
//!
//! ```text
//!   /* ... loading the pdumper file above assigns to those variables values
//!      from the dump stage, which might be incorrect ... */
//! ```
//!
//! GNU's answer is not to dump more carefully.  It is to run a fixed sequence
//! of `init_*` functions on EVERY startup, dumped image or not, each of which
//! re-establishes the facts its subsystem owns.  `load_pdump` is called at
//! src/emacs.c:1436; every `init_*` call below that line is part of that
//! sequence.  This port has exactly one place that runs at the same moment --
//! `finalize_cached_bootstrap_eval` (super::load) -- so it owes the same
//! sequence, and this module is that sequence written down.
//!
//! Entry 174 ported ONE of them (`init_lread`) after `load-in-progress` was
//! found wedged at `t` for the entire life of every session, and said plainly
//! that four functions read out of about thirty is a spot-check.  Entry 177
//! is the systematic screen: all 40 post-image call sites, each with its GNU
//! citation and each classified.
//!
//! ## Why this is a table and not a list of statements
//!
//! A hand-written `eval.set_variable(...)` per fact is invisible to any
//! check: nothing can tell a fact that was deliberately left alone from one
//! nobody thought of.  Ledger 173's law applies -- *a predicate over rows
//! that exist cannot see a row that was never written* -- so the guard has to
//! be able to fail on an EMPTY table.  Three things make that true here:
//!
//! 1. [`PostImageInit::ALL`] is declared with length [`PostImageInit::COUNT`],
//!    which is derived from the LAST discriminant.  Adding a variant without
//!    adding its row to `ALL` is a **compile error**, not an omission.
//! 2. [`PostImageInit::site`] is an exhaustive `match`, so a variant with no
//!    GNU citation and no classification is a **compile error** too.
//! 3. `post_image_init_test.rs` asserts the enumeration's SIZE and its
//!    emacs.c line ordering against the numbers screened from GNU's `main`.
//!    An empty table fails that test rather than passing it vacuously.

use super::eval::Context;
use crate::Value;

/// A compile-time constant GNU assigns to a Lisp variable in an `init_*`
/// body.
///
/// Only constants are representable.  A fact GNU DERIVES at startup -- from
/// `argv`, the environment, the filesystem or the terminal -- cannot be
/// written as a row, and must be declared in [`Establishes::Facts::derived`]
/// with the citation for where this port derives it instead.  That is the
/// point of the type: it refuses to let a derived fact masquerade as a reset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ResetValue {
    Nil,
    T,
    Fixnum(i64),
    Float(f64),
}

impl ResetValue {
    pub(crate) fn value(self) -> Value {
        match self {
            ResetValue::Nil => Value::NIL,
            ResetValue::T => Value::T,
            ResetValue::Fixnum(n) => Value::fixnum(n),
            ResetValue::Float(f) => Value::make_float(f),
        }
    }
}

/// One Lisp variable an `init_*` body assigns to a constant, with the GNU
/// line that assigns it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ResetRow {
    pub(crate) name: &'static str,
    pub(crate) value: ResetValue,
    /// `file:line` in the GNU tree for the assignment itself.
    pub(crate) gnu: &'static str,
}

/// A Lisp-visible fact GNU DERIVES at startup -- from `argv`, the
/// environment, the filesystem or the terminal -- rather than assigning a
/// constant.
///
/// Every derivation has to be classified, and the classification is what
/// makes "GNU establishes this here" checkable instead of a comment beside
/// the code: [`Derived::Ported`] carries the function
/// [`apply_post_image_init`] calls, so a ported derivation with no
/// implementation is unrepresentable.
#[derive(Clone, Copy)]
pub(crate) enum Derived {
    /// GNU derives it, and so does this port -- in the function stored here,
    /// which the sequence calls in `main`'s order.
    Ported {
        what: &'static str,
        gnu: &'static str,
        apply: fn(&mut Context),
    },
    /// GNU derives it here; this port establishes the same fact at another
    /// named place on the same startup path.
    Elsewhere {
        what: &'static str,
        gnu: &'static str,
        by: &'static str,
    },
    /// GNU derives it and this port has nothing to derive, with the reason.
    NotApplicable {
        what: &'static str,
        gnu: &'static str,
        why: &'static str,
    },
}

impl Derived {
    pub(crate) fn what(self) -> &'static str {
        match self {
            Derived::Ported { what, .. }
            | Derived::Elsewhere { what, .. }
            | Derived::NotApplicable { what, .. } => what,
        }
    }

    pub(crate) fn gnu(self) -> &'static str {
        match self {
            Derived::Ported { gnu, .. }
            | Derived::Elsewhere { gnu, .. }
            | Derived::NotApplicable { gnu, .. } => gnu,
        }
    }
}

/// What a screened `init_*` body establishes, at the level of Lisp.
#[derive(Clone, Copy)]
pub(crate) enum Establishes {
    /// The body establishes Lisp-visible state.
    Facts {
        /// Variables assigned to compile-time constants.  This port owes the
        /// same assignment on every startup, and [`apply_post_image_init`]
        /// performs them.
        constants: &'static [ResetRow],
        /// Facts the same body establishes that a constant table cannot
        /// carry, each classified by whether this port derives them too.
        derived: &'static [Derived],
    },
    /// Screened and empty: the body assigns no Lisp-visible state at all.
    /// The string is the evidence for that claim, not an apology for it.
    NoLispVisibleState(&'static str),
    /// The body assigns no Lisp *value*, but it establishes OS-level
    /// dispositions that decide whether Lisp runs at all.
    ///
    /// `init_signals` is the only site of this shape, and ledger 184 is why
    /// it needs its own classification: reading it for V-prefixed assignments
    /// gives the right answer to the wrong question.  Its
    /// `add_user_signal (SIGUSR1, "sigusr1")` tail installs a `sigaction`,
    /// and without that install the kernel's default disposition for both
    /// user signals is `Term` -- so "no Lisp-visible state" was true about
    /// globals and silent about the editor being killed.
    OsDispositions {
        /// The evidence that no Lisp value is written, kept from the
        /// `NoLispVisibleState` screen this site used to carry.
        no_lisp_state: &'static str,
        /// The dispositions this port installs here, in GNU's own order.
        installs: &'static [crate::emacs_core::os_signal::HandledSignal],
    },
    /// The call site is compiled out of a GNU/Linux build.  Screened for
    /// completeness so the enumeration is the whole of GNU's `main`, not the
    /// convenient part of it.
    NotInThisBuild(&'static str),
}

/// Whether GNU's `main` reaches the call site at all in this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallGuard {
    /// Runs on every startup.
    Unconditional,
    /// Runs when the named build option is configured in; present in the
    /// GNU/Linux build this port is compared against.
    BuildOption(&'static str),
    /// Guarded by a platform macro that is never defined on GNU/Linux.
    Platform(&'static str),
}

/// One `init_*` call site below `load_pdump` in GNU's `main`.
#[derive(Clone, Copy)]
pub(crate) struct PostImageInitSite {
    /// The C function's name, exactly as `main` spells it.
    pub(crate) c_name: &'static str,
    /// Line of the CALL in `src/emacs.c`.  Strictly increasing across
    /// [`PostImageInit::ALL`]; the test pins that.
    pub(crate) call_line: u32,
    /// `file:first-last` for the function BODY in the GNU tree.
    pub(crate) body: &'static str,
    pub(crate) guard: CallGuard,
    pub(crate) establishes: Establishes,
}

/// Every `init_*` call GNU's `main` makes AFTER `load_pdump`
/// (src/emacs.c:1436) has put the dumped image in memory.
///
/// The 16 calls in the `if (!initialized)` block (src/emacs.c:1957-2013) are
/// deliberately absent: that block runs only in `temacs`, which by definition
/// has no image to carry stale state out of.  `init_heap` (src/emacs.c:1402)
/// is absent for the other reason -- it runs BEFORE the image is mapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum PostImageInit {
    StandardFds = 0,
    Signals,
    Alloc,
    Bignum,
    Threads,
    Eval,
    Random,
    Xfaces,
    ModuleAssertions,
    Dosfns,
    EnvironmentMsdos,
    Atimer,
    EnvironmentW32,
    Ntproc,
    Buffer,
    Callproc1,
    Cmdargs,
    Callproc,
    Fileio,
    Lread,
    HaikuSelect,
    Charset,
    Timefns,
    Editfns,
    Dbusbind,
    Xterm,
    ProcessEmacs,
    Keyboard,
    Display,
    Crit,
    Xdisp,
    Fringe,
    Macros,
    Window,
    Font,
    AndroidMenu,
    AndroidFont,
    AndroidSelect,
    Sfntfont,
    SfntfontAndroid,
}

impl PostImageInit {
    /// Derived from the LAST discriminant, so it cannot drift from the enum.
    pub(crate) const COUNT: usize = Self::SfntfontAndroid as usize + 1;

    /// The whole sequence, in `main`'s order.
    ///
    /// The declared LENGTH is [`Self::COUNT`], so adding a variant without
    /// adding it here does not compile.  That is the only mechanism in this
    /// module that can catch a row nobody wrote.
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::StandardFds,
        Self::Signals,
        Self::Alloc,
        Self::Bignum,
        Self::Threads,
        Self::Eval,
        Self::Random,
        Self::Xfaces,
        Self::ModuleAssertions,
        Self::Dosfns,
        Self::EnvironmentMsdos,
        Self::Atimer,
        Self::EnvironmentW32,
        Self::Ntproc,
        Self::Buffer,
        Self::Callproc1,
        Self::Cmdargs,
        Self::Callproc,
        Self::Fileio,
        Self::Lread,
        Self::HaikuSelect,
        Self::Charset,
        Self::Timefns,
        Self::Editfns,
        Self::Dbusbind,
        Self::Xterm,
        Self::ProcessEmacs,
        Self::Keyboard,
        Self::Display,
        Self::Crit,
        Self::Xdisp,
        Self::Fringe,
        Self::Macros,
        Self::Window,
        Self::Font,
        Self::AndroidMenu,
        Self::AndroidFont,
        Self::AndroidSelect,
        Self::Sfntfont,
        Self::SfntfontAndroid,
    ];

    /// The GNU citation and the classification, one exhaustive `match` so a
    /// new variant cannot be added without reading GNU's body for it.
    pub(crate) const fn site(self) -> PostImageInitSite {
        match self {
            // ---------------------------------------------------------------
            Self::StandardFds => PostImageInitSite {
                c_name: "init_standard_fds",
                call_line: 1460,
                body: "src/sysdep.c:246-263",
                guard: CallGuard::Unconditional,
                establishes: Establishes::NoLispVisibleState(
                    "force_open()s fds 0/1/2 the wrong way round so ordinary I/O \
                     fails loudly, and line-buffers stderr.  No Lisp object is \
                     touched.",
                ),
            },
            Self::Signals => PostImageInitSite {
                c_name: "init_signals",
                call_line: 1951,
                body: "src/sysdep.c:2020-2118",
                guard: CallGuard::Unconditional,
                establishes: Establishes::OsDispositions {
                    no_lisp_state: "sigaction/sigemptyset only; the body contains no assignment \
                                    to any V-prefixed Lisp global.",
                    installs: crate::emacs_core::os_signal::supported_signals(),
                },
            },
            Self::Alloc => PostImageInitSite {
                c_name: "init_alloc",
                call_line: 2015,
                body: "src/alloc.c:7389-7393",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[
                        ResetRow {
                            name: "gc-elapsed",
                            value: ResetValue::Float(0.0),
                            gnu: "src/alloc.c:7391",
                        },
                        ResetRow {
                            name: "gcs-done",
                            value: ResetValue::Fixnum(0),
                            gnu: "src/alloc.c:7392",
                        },
                    ],
                    derived: &[],
                },
            },
            Self::Bignum => PostImageInitSite {
                c_name: "init_bignum",
                call_line: 2016,
                body: "src/bignum.c:52-69",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[ResetRow {
                        name: "integer-width",
                        value: ResetValue::Fixnum(1 << 16),
                        gnu: "src/bignum.c:55",
                    }],
                    derived: &[],
                },
            },
            Self::Threads => PostImageInitSite {
                c_name: "init_threads",
                call_line: 2017,
                body: "src/thread.c:1246-1259",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "current_thread becomes &main_thread, so `(all-threads)' \
                               is the single unnamed main thread",
                        gnu: "src/thread.c:1251",
                        by: "Context construction -- this port has one Lisp thread \
                             by construction, so there is no second one to select \
                             away from",
                    }],
                },
            },
            Self::Eval => PostImageInitSite {
                c_name: "init_eval",
                call_line: 2018,
                body: "src/eval.c:234-252",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[
                        ResetRow {
                            name: "quit-flag",
                            value: ResetValue::Nil,
                            gnu: "src/eval.c:247",
                        },
                        ResetRow {
                            name: "debug-on-next-call",
                            value: ResetValue::Nil,
                            gnu: "src/eval.c:248",
                        },
                    ],
                    derived: &[Derived::Elsewhere {
                        what: "specpdl_ptr, handlerlist, lisp_eval_depth and \
                               when_entered_debugger -- C state with no DEFVAR",
                        gnu: "src/eval.c:236-251",
                        by: "eval::Context::new_inner and \
                             Context::clear_top_level_eval_state",
                    }],
                },
            },
            Self::Random => PostImageInitSite {
                c_name: "init_random",
                call_line: 2020,
                body: "src/sysdep.c:2225-2249",
                guard: CallGuard::Unconditional,
                establishes: Establishes::NoLispVisibleState(
                    "seeds the PRNG from getrandom().  Two startups are MEANT to \
                     disagree, so `(random)' is not a fact that can be compared \
                     between editors at all.",
                ),
            },
            Self::Xfaces => PostImageInitSite {
                c_name: "init_xfaces",
                call_line: 2021,
                body: "src/xfaces.c:7393-7444",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "rebuilds lface_id_to_name[] FROM the dumped \
                               `face--new-frame-defaults', so face ids SURVIVE the \
                               dump rather than being reset -- the one site in the \
                               sequence that preserves dumped state on purpose",
                        gnu: "src/xfaces.c:7398-7421",
                        by: "xfaces::restore_created_faces_from_table, called from \
                             finalize_cached_bootstrap_eval",
                    }],
                },
            },
            Self::ModuleAssertions => PostImageInitSite {
                c_name: "init_module_assertions",
                call_line: 2049,
                body: "src/emacs-module.c:1677-1680",
                guard: CallGuard::BuildOption("HAVE_MODULES"),
                establishes: Establishes::NoLispVisibleState(
                    "stores the --module-assertions command-line flag in a static \
                     C bool that has no DEFVAR.",
                ),
            },
            Self::Dosfns => PostImageInitSite {
                c_name: "init_dosfns",
                call_line: 2152,
                body: "src/dosfns.c",
                guard: CallGuard::Platform("MSDOS"),
                establishes: Establishes::NotInThisBuild("MSDOS is not defined on GNU/Linux."),
            },
            Self::EnvironmentMsdos => PostImageInitSite {
                c_name: "init_environment",
                call_line: 2155,
                body: "src/msdos.c",
                guard: CallGuard::Platform("MSDOS"),
                establishes: Establishes::NotInThisBuild("MSDOS is not defined on GNU/Linux."),
            },
            Self::Atimer => PostImageInitSite {
                c_name: "init_atimer",
                call_line: 2180,
                body: "src/atimer.c:596-627",
                guard: CallGuard::Unconditional,
                establishes: Establishes::NoLispVisibleState(
                    "creates the timerfd/POSIX alarm timer, empties the C atimer \
                     chain and installs the SIGALRM handler.  The chain has no \
                     DEFVAR; `timer-list' is Lisp-side and untouched here.",
                ),
            },
            Self::EnvironmentW32 => PostImageInitSite {
                c_name: "init_environment",
                call_line: 2195,
                body: "src/w32.c",
                guard: CallGuard::Platform("WINDOWSNT"),
                establishes: Establishes::NotInThisBuild("WINDOWSNT is not defined on GNU/Linux."),
            },
            Self::Ntproc => PostImageInitSite {
                c_name: "init_ntproc",
                call_line: 2196,
                body: "src/w32proc.c",
                guard: CallGuard::Platform("WINDOWSNT"),
                establishes: Establishes::NotInThisBuild("WINDOWSNT is not defined on GNU/Linux."),
            },
            Self::Buffer => PostImageInitSite {
                c_name: "init_buffer",
                call_line: 2200,
                body: "src/buffer.c:4933-4984",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "creates and selects `*scratch*' and sets its \
                               `default-directory' to the PROCESS cwd as a directory \
                               name, `/:'-quoted when a file-name handler claims it; \
                               minibuffer 0 copies the same value",
                        gnu: "src/buffer.c:4938,4960-4983",
                        by: "finalize_cached_bootstrap_eval's default-directory block",
                    }],
                },
            },
            Self::Callproc1 => PostImageInitSite {
                c_name: "init_callproc_1",
                call_line: 2203,
                body: "src/callproc.c:1950-1964",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[
                        Derived::Ported {
                            what: "exec-path is $PATH FOLLOWED BY the EMACSPATH / \
                                   PATH_EXEC list whose car becomes exec-directory, \
                                   so the last element of exec-path IS exec-directory",
                            gnu: "src/callproc.c:1960-1963",
                            apply: derive_exec_path_and_exec_directory,
                        },
                        Derived::Elsewhere {
                            what: "data-directory and doc-directory from EMACSDATA / \
                                   EMACSDOC and the configured epaths defaults",
                            gnu: "src/callproc.c:1952-1956",
                            by: "finalize_cached_bootstrap_eval's etc/ block",
                        },
                    ],
                },
            },
            Self::Cmdargs => PostImageInitSite {
                c_name: "init_cmdargs",
                call_line: 2206,
                body: "src/emacs.c:489-656",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "invocation-name and invocation-directory from argv[0], \
                               PATH-searched when argv[0] carries no directory; \
                               installation-directory by walking up for a lib-src+etc \
                               pair; command-line-args from argv",
                        gnu: "src/emacs.c:518-549,551-641,643-653",
                        by: "the neomacs-bin startup path and \
                             finalize_cached_bootstrap_eval's \
                             installation-directory block",
                    }],
                },
            },
            Self::Callproc => PostImageInitSite {
                c_name: "init_callproc",
                call_line: 2218,
                body: "src/callproc.c:1969-2067",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[
                        Derived::Ported {
                            what: "shell-file-name from $SHELL, else /bin/sh",
                            gnu: "src/callproc.c:2038-2044",
                            apply: derive_shell_file_name,
                        },
                        Derived::NotApplicable {
                            what: "shared-game-score-directory from PATH_GAME when \
                                   accessible, else nil",
                            gnu: "src/callproc.c:2046-2066",
                            why: "this port configures no PATH_GAME, so GNU's own \
                                  answer on this machine is nil as well -- measured, \
                                  not assumed",
                        },
                        Derived::Elsewhere {
                            what: "exec-path/exec-directory/doc-directory/\
                                   data-directory re-derived against \
                                   installation-directory when running uninstalled",
                            gnu: "src/callproc.c:1976-2025",
                            by: "derive_exec_path_and_exec_directory, whose \
                                 default comes from emacs_core::path_exec: an \
                                 installed tree's archlib when one is staged, and \
                                 otherwise the running binary's own directory as \
                                 this port's lib-src",
                        },
                    ],
                },
            },
            Self::Fileio => PostImageInitSite {
                c_name: "init_fileio",
                call_line: 2219,
                body: "src/fileio.c:6691-6697",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "caches the process umask in the static `realmask', \
                               which is the only thing `default-file-modes' reads \
                               (fileio.c:3730-3736)",
                        gnu: "src/fileio.c:6693",
                        by: "fileio's default-file-modes, which reads the live umask \
                             on every call rather than a cached copy",
                    }],
                },
            },
            Self::Lread => PostImageInitSite {
                c_name: "init_lread",
                call_line: 2220,
                body: "src/lread.c:5458-5529",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[
                        ResetRow {
                            name: "values",
                            value: ResetValue::Nil,
                            gnu: "src/lread.c:5522",
                        },
                        ResetRow {
                            name: "load-in-progress",
                            value: ResetValue::Nil,
                            gnu: "src/lread.c:5524",
                        },
                        ResetRow {
                            name: "load-file-name",
                            value: ResetValue::Nil,
                            gnu: "src/lread.c:5525",
                        },
                        ResetRow {
                            name: "load-true-file-name",
                            value: ResetValue::Nil,
                            gnu: "src/lread.c:5526",
                        },
                        ResetRow {
                            name: "standard-input",
                            value: ResetValue::T,
                            gnu: "src/lread.c:5527",
                        },
                    ],
                    derived: &[
                        Derived::Elsewhere {
                            what: "load-path from EMACSLOADPATH or \
                                   load_path_default(), plus the site-lisp directories",
                            gnu: "src/lread.c:5460-5520",
                            by: "finalize_cached_bootstrap_eval's \
                                 runtime_load_path_entries block",
                        },
                        Derived::Elsewhere {
                            what: "Vloads_in_progress is a STATIC C variable, not a \
                                   Lisp variable (lread.c:237), so the STACK is its \
                                   counterpart rather than a table row",
                            gnu: "src/lread.c:5528",
                            by: "clear_runtime_loader_state, which empties \
                                 loads_in_progress and require_stack",
                        },
                    ],
                },
            },
            Self::HaikuSelect => PostImageInitSite {
                c_name: "init_haiku_select",
                call_line: 2521,
                body: "src/haikuselect.c",
                guard: CallGuard::Platform("HAVE_HAIKU"),
                establishes: Establishes::NotInThisBuild("HAVE_HAIKU is not a GNU/Linux build."),
            },
            Self::Charset => PostImageInitSite {
                c_name: "init_charset",
                call_line: 2524,
                body: "src/charset.c:2303-2327",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Ported {
                        what: "charset-map-path = (list <data-directory>/charsets)",
                        gnu: "src/charset.c:2306,2326",
                        apply: derive_charset_map_path,
                    }],
                },
            },
            Self::Timefns => PostImageInitSite {
                c_name: "init_timefns",
                call_line: 2527,
                body: "src/timefns.c:343-350",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "tzlookup(getenv TZ, or `wall', true) fixes the local \
                               zone before any thread starts; visible through \
                               `(current-time-zone)'",
                        gnu: "src/timefns.c:345-349",
                        by: "timefns, which resolves the zone from $TZ on each call \
                             rather than latching it once",
                    }],
                },
            },
            Self::Editfns => PostImageInitSite {
                c_name: "init_editfns",
                call_line: 2529,
                body: "src/editfns.c:82-147",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "system-name; user-real-login-name from \
                               getpwuid(getuid()); user-login-name from LOGNAME/USER \
                               else geteuid(); user-full-name from the gecos field or \
                               $NAME; operating-system-release from uname",
                        gnu: "src/editfns.c:90,99,104-116,128-134,140",
                        by: "runtime_identity::install, called from \
                             ensure_startup_compat_variables",
                    }],
                },
            },
            Self::Dbusbind => PostImageInitSite {
                c_name: "init_dbusbind",
                call_line: 2533,
                body: "src/dbusbind.c:1987-1991",
                guard: CallGuard::BuildOption("HAVE_DBUS"),
                establishes: Establishes::NoLispVisibleState(
                    "the body is one xputenv(\"DBUS_FATAL_WARNINGS=0\"), and that is \
                     INVISIBLE to Lisp: `set_initial_environment' (src/emacs.c:2177) \
                     has already snapshotted `environ' into `process-environment', \
                     and `getenv-internal' searches only that list.  Measured -- a \
                     GNU build whose system-configuration-features names DBUS still \
                     answers nil to (getenv \"DBUS_FATAL_WARNINGS\").",
                ),
            },
            Self::Xterm => PostImageInitSite {
                c_name: "init_xterm",
                call_line: 2537,
                body: "src/xterm.c:32365-32381",
                guard: CallGuard::BuildOption("HAVE_X_WINDOWS"),
                establishes: Establishes::NoLispVisibleState(
                    "the whole body is inside `#ifndef HAVE_XINPUT2' and \
                     `#ifdef HAVE_X_I18N'; in an XI2 build it is empty, and \
                     otherwise it only sets GDK_CORE_DEVICE_EVENTS or registers a \
                     C text-conversion interface.",
                ),
            },
            Self::ProcessEmacs => PostImageInitSite {
                c_name: "init_process_emacs",
                call_line: 2543,
                body: "src/process.c:8695-8793",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[ResetRow {
                        name: "internal--daemon-sockname",
                        value: ResetValue::Nil,
                        gnu: "src/process.c:8751,8761",
                    }],
                    derived: &[Derived::Elsewhere {
                        what: "Vprocess_alist = Qnil is NOT a DEFVAR; its Lisp \
                               surface is `(process-list)'.  deleted_pid_list, \
                               chan_process[] and the coding-system arrays are C state",
                        gnu: "src/process.c:8778-8786",
                        by: "the process table, which a freshly loaded image starts \
                             empty because no child was dumped with it",
                    }],
                },
            },
            Self::Keyboard => PostImageInitSite {
                c_name: "init_keyboard",
                call_line: 2545,
                body: "src/keyboard.c:13201-13284 (calls init_kboard, 13104-13130)",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[
                        ResetRow {
                            name: "unread-command-events",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13206",
                        },
                        ResetRow {
                            name: "track-mouse",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13212",
                        },
                        ResetRow {
                            name: "last-event-device",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13219",
                        },
                        ResetRow {
                            name: "last-event-frame",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13223-13224",
                        },
                        // init_kboard, on the current kboard, after wipe_kboard.
                        ResetRow {
                            name: "overriding-terminal-local-map",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13106",
                        },
                        ResetRow {
                            name: "last-command",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13107",
                        },
                        ResetRow {
                            name: "real-last-command",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13108",
                        },
                        ResetRow {
                            name: "keyboard-translate-table",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13109",
                        },
                        ResetRow {
                            name: "last-repeatable-command",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13110",
                        },
                        ResetRow {
                            name: "prefix-arg",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13111",
                        },
                        ResetRow {
                            name: "last-prefix-arg",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13112",
                        },
                        ResetRow {
                            name: "defining-kbd-macro",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13120",
                        },
                        ResetRow {
                            name: "last-kbd-macro",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13121",
                        },
                        ResetRow {
                            name: "system-key-alist",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13123",
                        },
                        ResetRow {
                            name: "default-minibuffer-frame",
                            value: ResetValue::Nil,
                            gnu: "src/keyboard.c:13129",
                        },
                    ],
                    derived: &[
                        Derived::Elsewhere {
                            what: "input-decode-map and local-function-key-map are \
                                   FRESH sparse keymaps, the latter re-parented on \
                                   `function-key-map'.  A fresh cons is not a \
                                   constant, so these are built rather than tabled",
                            gnu: "src/keyboard.c:13126-13128",
                            by: "keyboard::pure's kboard initialisation; MEASURED \
                                 equal to GNU's in --batch, own bindings empty and \
                                 parent `function-key-map'",
                        },
                        Derived::Elsewhere {
                            what: "recent_keys is emptied by total_keys = 0 and \
                                   recent_keys_index = 0, which is what \
                                   `(recent-keys)' reads",
                            gnu: "src/keyboard.c:13208-13209",
                            by: "the keyboard ring, which a loaded image starts empty",
                        },
                        Derived::Elsewhere {
                            what: "window-system, which init_kboard sets to its TYPE \
                                   argument -- Qnil for the initial kboard",
                            gnu: "src/keyboard.c:13125,13231",
                            by: "restore_cached_runtime_window_system_surface, which \
                                 runs LATER on this port's startup path and would be \
                                 clobbered by a constant row here",
                        },
                        Derived::Elsewhere {
                            what: "command_loop_level = -1 and quit_char = C-g have \
                                   no DEFVAR; `(recursion-depth)' is derived from the \
                                   former",
                            gnu: "src/keyboard.c:13204-13205",
                            by: "the command loop's own depth counter",
                        },
                    ],
                },
            },
            Self::Display => PostImageInitSite {
                c_name: "init_display",
                call_line: 2546,
                body: "src/dispnew.c:7413-7422",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[
                        Derived::Elsewhere {
                            what: "in --batch the ENTIRE body is `if \
                                   (dumped_with_pdumper_p ()) init_faces_initial ();', \
                                   which sets the initial frame's fg/bg to \
                                   FACE_TTY_DEFAULT_* and calls \
                                   `tty-set-up-initial-frame-faces' (dispnew.c:7175-7184)",
                            gnu: "src/dispnew.c:7415-7419",
                            by: "the face table restored by \
                                 xfaces::restore_created_faces_from_table; MEASURED \
                                 equal at unspecified-fg / unspecified-bg",
                        },
                        Derived::Elsewhere {
                            what: "`initial-window-system' is assigned ONLY on the \
                                   interactive arm, so under --batch it keeps \
                                   whatever the dump carried -- GNU relies on that \
                                   value being nil in the dump rather than resetting it",
                            gnu: "src/dispnew.c:7208",
                            by: "restore_cached_runtime_window_system_surface",
                        },
                    ],
                },
            },
            Self::Crit => PostImageInitSite {
                c_name: "init_crit",
                call_line: 2549,
                body: "src/w32.c",
                guard: CallGuard::Platform("HAVE_W32NOTIFY"),
                establishes: Establishes::NotInThisBuild(
                    "HAVE_W32NOTIFY is a Windows-only build option.",
                ),
            },
            Self::Xdisp => PostImageInitSite {
                c_name: "init_xdisp",
                call_line: 2551,
                body: "src/xdisp.c:39327-39376",
                guard: CallGuard::Unconditional,
                establishes: Establishes::NoLispVisibleState(
                    "assigns no Lisp variable: this_line_start_pos, \
                     echo_area_window, the mode-line scratch buffer and \
                     help_echo_showing_p are C state, and the window-geometry arm \
                     (39333-39363) is guarded by `if (!noninteractive)'.",
                ),
            },
            Self::Fringe => PostImageInitSite {
                c_name: "init_fringe",
                call_line: 2553,
                body: "src/fringe.c:1810-1818",
                guard: CallGuard::BuildOption("HAVE_WINDOW_SYSTEM"),
                establishes: Establishes::NoLispVisibleState(
                    "allocates the fringe_bitmaps[] and fringe_faces[] C arrays; \
                     `fringe-bitmaps' the Lisp variable is built by \
                     syms_of_fringe, not here.",
                ),
            },
            Self::Macros => PostImageInitSite {
                c_name: "init_macros",
                call_line: 2555,
                body: "src/macros.c:393-397",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[ResetRow {
                        name: "executing-kbd-macro",
                        value: ResetValue::Nil,
                        gnu: "src/macros.c:395",
                    }],
                    derived: &[Derived::Elsewhere {
                        what: "the C `executing_kbd_macro' shadow has no DEFVAR of \
                               its own",
                        gnu: "src/macros.c:396",
                        by: "the single Lisp variable above -- this port has no \
                             second C-side copy to diverge from it",
                    }],
                },
            },
            Self::Window => PostImageInitSite {
                c_name: "init_window",
                call_line: 2556,
                body: "src/window.c:9159-9162",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Elsewhere {
                        what: "Vwindow_list (window.c:94) is the C-side CACHE behind \
                               `window-list' and has no DEFVAR; clearing it forces \
                               the next `(window-list)' to be rebuilt from the live \
                               frames instead of the dumped ones",
                        gnu: "src/window.c:9161",
                        by: "window_cmds, which walks the live frame tree on every \
                             call and keeps no cache to go stale",
                    }],
                },
            },
            Self::Font => PostImageInitSite {
                c_name: "init_font",
                call_line: 2557,
                body: "src/font.c:6088-6091",
                guard: CallGuard::Unconditional,
                establishes: Establishes::Facts {
                    constants: &[],
                    derived: &[Derived::Ported {
                        what: "font-log = t unless EMACS_FONT_LOG is set, in which \
                               case nil -- environment-dependent, so a derivation and \
                               not a constant.  Note the polarity: t means logging is OFF",
                        gnu: "src/font.c:6090",
                        apply: derive_font_log,
                    }],
                },
            },
            Self::AndroidMenu => PostImageInitSite {
                c_name: "init_androidmenu",
                call_line: 2560,
                body: "src/androidmenu.c",
                guard: CallGuard::Platform("HAVE_ANDROID"),
                establishes: Establishes::NotInThisBuild("HAVE_ANDROID is not a GNU/Linux build."),
            },
            Self::AndroidFont => PostImageInitSite {
                c_name: "init_androidfont",
                call_line: 2564,
                body: "src/androidfont.c",
                guard: CallGuard::Platform("HAVE_ANDROID && !ANDROID_STUBIFY"),
                establishes: Establishes::NotInThisBuild("HAVE_ANDROID is not a GNU/Linux build."),
            },
            Self::AndroidSelect => PostImageInitSite {
                c_name: "init_androidselect",
                call_line: 2565,
                body: "src/androidselect.c",
                guard: CallGuard::Platform("HAVE_ANDROID && !ANDROID_STUBIFY"),
                establishes: Establishes::NotInThisBuild("HAVE_ANDROID is not a GNU/Linux build."),
            },
            Self::Sfntfont => PostImageInitSite {
                c_name: "init_sfntfont",
                call_line: 2566,
                body: "src/sfntfont.c",
                guard: CallGuard::Platform("HAVE_ANDROID && !ANDROID_STUBIFY"),
                establishes: Establishes::NotInThisBuild("HAVE_ANDROID is not a GNU/Linux build."),
            },
            Self::SfntfontAndroid => PostImageInitSite {
                c_name: "init_sfntfont_android",
                call_line: 2567,
                body: "src/sfntfont-android.c",
                guard: CallGuard::Platform("HAVE_ANDROID && !ANDROID_STUBIFY"),
                establishes: Establishes::NotInThisBuild("HAVE_ANDROID is not a GNU/Linux build."),
            },
        }
    }

    /// The constant rows this site owes on every startup.
    pub(crate) const fn constants(self) -> &'static [ResetRow] {
        match self.site().establishes {
            Establishes::Facts { constants, .. } => constants,
            Establishes::NoLispVisibleState(_)
            | Establishes::NotInThisBuild(_)
            | Establishes::OsDispositions { .. } => &[],
        }
    }
}

// ---------------------------------------------------------------------------
// The derivations this port performs, one per `Derived::Ported` row above.
// ---------------------------------------------------------------------------

/// Turn a directory spelling into GNU's `file-name-as-directory` form.
fn as_directory(mut name: String) -> String {
    if !name.ends_with('/') {
        name.push('/');
    }
    name
}

/// GNU `init_callproc_1`, src/callproc.c:1960-1963:
///
/// ```c
///   Vexec_path = decode_env_path ("EMACSPATH", PATH_EXEC, 0);
///   Vexec_directory = Ffile_name_as_directory (Fcar (Vexec_path));
///   Vexec_path = nconc2 (decode_env_path ("PATH", NULL, 0), Vexec_path);
/// ```
///
/// The shape that matters is the LAST line: `exec-path` is `$PATH` with the
/// EMACSPATH/PATH_EXEC list appended, so its final element is always the same
/// directory `exec-directory` names.  `init_callproc` (src/callproc.c:1984-1991)
/// then swaps that tail for `<installation-directory>/lib-src` when Emacs is
/// running uninstalled.
///
/// The default this port substitutes for GNU's compile-time `PATH_EXEC` is
/// [`super::path_exec::resolve`]: an installed tree's private archlib when
/// one is staged, and otherwise the directory the running binary lives in.
/// That fallback is GNU's own uninstalled branch --
/// `<installation-directory>/lib-src` -- because a cargo build tree's
/// `lib-src` is `target/<profile>`, where `neomacsclient` is written.
/// Setting `exec-directory` to it while leaving `exec-path` at `$PATH` alone
/// -- which is what this did before ledger 177 -- meant nothing shipped
/// beside the binary was findable, and `(executable-find "neomacsclient")`
/// answered nil where GNU's `(executable-find "etags")` answers the lib-src
/// path.
fn derive_exec_path_and_exec_directory(eval: &mut Context) {
    // GNU: decode_env_path ("EMACSPATH", PATH_EXEC, 0) -- the environment
    // variable wins, and the configured default stands in when it is unset.
    let mut exec_tail = super::load::exec_path_dirs_from_os(std::env::var_os("EMACSPATH"));
    if exec_tail.is_empty()
        && let Some(path_exec) = super::path_exec::resolve()
    {
        exec_tail.push(super::fileio::host_path_to_lisp_file_name_string(
            path_exec.dir(),
        ));
    }
    if exec_tail.is_empty() {
        return;
    }

    // GNU: Ffile_name_as_directory (Fcar (Vexec_path)) -- with the trailing
    // slash, unlike the exec-path entry it is taken from.
    eval.set_variable(
        "exec-directory",
        Value::unibyte_string(as_directory(exec_tail[0].clone())),
    );

    // GNU: nconc2 (decode_env_path ("PATH", NULL, 0), Vexec_path).
    let mut entries: Vec<Value> = super::load::exec_path_dirs_from_env()
        .into_iter()
        .map(Value::unibyte_string)
        .collect();
    entries.extend(exec_tail.into_iter().map(Value::unibyte_string));
    eval.set_variable("exec-path", Value::list(entries));
}

/// GNU `init_callproc`, src/callproc.c:2038-2044.
///
/// A release image dumped on a build machine carries that machine's `$SHELL`;
/// this re-derives it from the runtime environment on every startup, exactly
/// as GNU does.
fn derive_shell_file_name(eval: &mut Context) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    eval.set_variable("shell-file-name", Value::unibyte_string(shell));
}

/// GNU `init_charset`, src/charset.c:2303-2327:
///
/// ```c
///   tempdir = Fexpand_file_name (build_string ("charsets"), Vdata_directory);
///   if (! file_accessible_directory_p (tempdir)) { ... exit (1); }
///   Vcharset_map_path = list1 (tempdir);
/// ```
///
/// Note that `expand-file-name` leaves NO trailing slash, so the single entry
/// is `<data-directory>charsets` rather than a directory name.  This port left
/// the variable at the nil its DEFVAR carries; `ensure_startup_compat_variables`
/// could not have covered it, because that table only assigns when a variable
/// is UNSET, and after a dump nothing is unset.
///
/// GNU `exit (1)`s when the directory is missing.  A library that ends the
/// process out of a startup helper is not a behaviour worth porting, so this
/// warns and still publishes the path GNU would have published.
fn derive_charset_map_path(eval: &mut Context) {
    let Some(data_directory) = eval
        .visible_variable_value_or_nil("data-directory")
        .as_utf8_str()
    else {
        return;
    };
    let charsets = format!("{}charsets", as_directory(data_directory.to_string()));
    if !std::path::Path::new(&charsets).is_dir() {
        tracing::warn!(
            "charset map directory {charsets} is not accessible; GNU init_charset \
             (src/charset.c:2307-2324) exits(1) here"
        );
    }
    eval.set_variable(
        "charset-map-path",
        Value::list(vec![Value::unibyte_string(charsets)]),
    );
}

/// GNU `init_font`, src/font.c:6090: `Vfont_log = egetenv ("EMACS_FONT_LOG") ?
/// Qnil : Qt`.  The polarity is the trap -- `t` means logging is OFF.
fn derive_font_log(eval: &mut Context) {
    let value = if std::env::var_os("EMACS_FONT_LOG").is_some() {
        Value::NIL
    } else {
        Value::T
    };
    eval.set_variable("font-log", value);
}

/// Re-establish, on the loaded image, everything GNU's post-image `init_*`
/// sequence establishes: every constant row, then every derivation this port
/// carries, walked in GNU `main`'s order.
///
/// **The order is load-bearing**, and it is free: because [`PostImageInit::ALL`]
/// is in `main` order, `init_callproc_1`'s `data-directory` is in place before
/// `init_charset` reads it, for the same reason it is in GNU.
///
/// Called from `finalize_cached_bootstrap_eval`, this port's single equivalent
/// of the stretch of GNU's `main` below `load_pdump`.
pub(crate) fn apply_post_image_init(eval: &mut Context) {
    for site in PostImageInit::ALL {
        // GNU's `init_signals` is a call in `main` like any other, so its
        // port belongs in this walk and not in a startup path only one front
        // end reaches.  `install` is idempotent, exactly as `add_user_signal`
        // is (src/keyboard.c:8470-8473).
        if let Establishes::OsDispositions { installs, .. } = site.site().establishes {
            let report = crate::emacs_core::os_signal::install();
            debug_assert_eq!(
                report.installed_count(),
                installs.len(),
                "{} did not install every disposition it declares",
                site.site().c_name
            );
        }
        if let Establishes::Facts { constants, derived } = site.site().establishes {
            for row in constants {
                eval.set_variable(row.name, row.value.value());
            }
            for fact in derived {
                if let Derived::Ported { apply, .. } = fact {
                    apply(eval);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod post_image_init_test;
