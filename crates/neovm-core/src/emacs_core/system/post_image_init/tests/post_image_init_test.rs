//! Ledger 177 -- the systematic screen of GNU's post-image `init_*` sequence.
//!
//! Ledger 173's law is the design constraint here: *a predicate over rows
//! that exist cannot see a row that was never written.*  So every test below
//! is written to be RED on an EMPTY table, never vacuously green:
//!
//! * the enumeration tests assert absolute COUNTS taken from GNU's `main`,
//!   so an empty `ALL` fails on `0 != 40` rather than passing over nothing;
//! * the constant tests name each variable and its value literally, so a
//!   deleted row fails on a lookup rather than on a shrunken iteration;
//! * the runtime tests read the variable out of a finalized image, so a table
//!   that is merely well-formed but never applied still fails.

use super::{CallGuard, Derived, Establishes, PostImageInit, ResetValue, apply_post_image_init};
use crate::Value;
use crate::emacs_core::value::list_to_vec;

/// A Lisp string as UTF-8, for comparing paths in assertions.
fn text(v: Value) -> String {
    v.as_utf8_str().unwrap_or_default().to_string()
}

/// GNU `main` (src/emacs.c:1321-2638), screened at 31.0.90 (0ee48ac4df20).
///
/// `load_pdump` is called at :1436, so every `init_*` call BELOW that line
/// runs with the dumped image already in memory.  Counted from the source:
/// 57 `init_*` call sites in `main`, of which 1 is above :1436 (`init_heap`,
/// itself WINDOWSNT-only) and 16 sit in the `if (!initialized)` block
/// (:1957-2013) that only `temacs` reaches.  That leaves 40.
const GNU_POST_IMAGE_CALL_SITES: usize = 40;
/// Of the 40: 25 run on every GNU/Linux startup...
const GNU_UNCONDITIONAL: usize = 25;
/// ...4 are behind a build option this GNU/Linux build has (HAVE_MODULES,
/// HAVE_DBUS, HAVE_X_WINDOWS, HAVE_WINDOW_SYSTEM)...
const GNU_BUILD_OPTION: usize = 4;
/// ...and 11 are behind a platform macro no GNU/Linux build defines
/// (MSDOS x2, WINDOWSNT x2, HAVE_HAIKU, HAVE_W32NOTIFY, HAVE_ANDROID x5).
const GNU_PLATFORM_ONLY: usize = 11;
/// src/emacs.c:1436 -- `initial_emacs_executable = load_pdump (...)`.
const GNU_LOAD_PDUMP_LINE: u32 = 1436;

#[test]
fn post_image_init_enumerates_every_gnu_call_site() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        PostImageInit::ALL.len(),
        GNU_POST_IMAGE_CALL_SITES,
        "GNU's `main' makes {GNU_POST_IMAGE_CALL_SITES} init_* calls below \
         load_pdump (src/emacs.c:{GNU_LOAD_PDUMP_LINE}); this table has {}. \
         A screen that enumerates fewer is a spot-check.",
        PostImageInit::ALL.len()
    );
    assert_eq!(PostImageInit::ALL.len(), PostImageInit::COUNT);

    let mut unconditional = 0;
    let mut build_option = 0;
    let mut platform = 0;
    for site in PostImageInit::ALL {
        match site.site().guard {
            CallGuard::Unconditional => unconditional += 1,
            CallGuard::BuildOption(_) => build_option += 1,
            CallGuard::Platform(_) => platform += 1,
        }
    }
    assert_eq!(unconditional, GNU_UNCONDITIONAL, "unconditional call sites");
    assert_eq!(build_option, GNU_BUILD_OPTION, "build-option call sites");
    assert_eq!(platform, GNU_PLATFORM_ONLY, "platform-only call sites");
}

#[test]
fn post_image_init_is_in_gnu_main_order_and_below_load_pdump() {
    crate::test_utils::init_test_tracing();

    let mut previous = GNU_LOAD_PDUMP_LINE;
    for site in PostImageInit::ALL {
        let s = site.site();
        assert!(
            s.call_line > GNU_LOAD_PDUMP_LINE,
            "{} is at src/emacs.c:{} which is ABOVE load_pdump at :{}: it runs \
             before the image exists and cannot be carrying dumped state",
            s.c_name,
            s.call_line,
            GNU_LOAD_PDUMP_LINE
        );
        assert!(
            s.call_line > previous,
            "{} (src/emacs.c:{}) is out of `main' order after :{}. The table's \
             order IS the dependency order -- init_charset needs the \
             data-directory init_callproc derives.",
            s.c_name,
            s.call_line,
            previous
        );
        previous = s.call_line;
        assert!(!s.c_name.is_empty() && s.c_name.starts_with("init_"));
        assert!(
            s.body.starts_with("src/"),
            "{} must cite the GNU file its body lives in",
            s.c_name
        );
    }
    assert_eq!(
        PostImageInit::StandardFds.site().call_line,
        1460,
        "the first post-image init_* call is init_standard_fds"
    );
    assert_eq!(
        PostImageInit::SfntfontAndroid.site().call_line,
        2567,
        "the last init_* call in `main' is init_sfntfont_android"
    );
}

#[test]
fn post_image_init_screened_sites_carry_their_evidence() {
    crate::test_utils::init_test_tracing();

    // A site that establishes nothing must SAY WHY.  An empty string here
    // would be a silent skip wearing a classification.
    let mut screened_empty = 0;
    let mut not_in_build = 0;
    let mut os_dispositions = 0;
    for site in PostImageInit::ALL {
        match site.site().establishes {
            Establishes::NoLispVisibleState(why) => {
                assert!(
                    why.len() > 40,
                    "{} claims no Lisp-visible state without evidence",
                    site.site().c_name
                );
                screened_empty += 1;
            }
            Establishes::NotInThisBuild(why) => {
                assert!(!why.is_empty());
                not_in_build += 1;
            }
            // `init_signals` alone.  Ledger 184 moved it out of the screened
            // set: its body really does assign no V-prefixed global, and it
            // really does decide whether the editor survives a user signal,
            // so both facts are now carried instead of one.
            Establishes::OsDispositions {
                no_lisp_state,
                installs,
            } => {
                assert!(
                    no_lisp_state.len() > 40,
                    "{} claims no Lisp-visible state without evidence",
                    site.site().c_name
                );
                if cfg!(unix) {
                    assert!(
                        !installs.is_empty(),
                        "{} claims to install dispositions but names none",
                        site.site().c_name
                    );
                } else {
                    assert!(
                        installs.is_empty(),
                        "{} has no supported signal dispositions on this platform",
                        site.site().c_name
                    );
                }
                os_dispositions += 1;
            }
            Establishes::Facts { constants, derived } => {
                assert!(
                    !constants.is_empty() || !derived.is_empty(),
                    "{} claims to establish facts but names none",
                    site.site().c_name
                );
                for row in constants {
                    assert!(
                        row.gnu.starts_with("src/"),
                        "{}: row `{}' must cite the GNU line that assigns it",
                        site.site().c_name,
                        row.name
                    );
                }
            }
        }
    }
    assert_eq!(
        not_in_build, GNU_PLATFORM_ONLY,
        "every platform-only call site is classified NotInThisBuild"
    );
    // init_standard_fds, init_random, init_module_assertions, init_atimer,
    // init_dbusbind, init_xterm, init_xdisp, init_fringe.  `init_signals`
    // used to be the ninth; ledger 184 reclassified it, because the
    // dispositions it installs are the difference between GNU's `rc=0` and
    // this port's `rc=140` on a `kill -USR2`.
    assert_eq!(
        screened_empty, 8,
        "eight reachable call sites were read and establish no Lisp-visible \
         state; that is a result, not a gap"
    );
    assert_eq!(
        os_dispositions, 1,
        "init_signals is the only call site in `main' that establishes an OS \
         disposition this port has to install"
    );
}

/// Every DERIVED fact is classified, and every classification carries its GNU
/// citation.  `Derived::Ported` cannot exist without an implementation -- the
/// function pointer is a field, not an `Option` -- so the only thing left to
/// pin is the count the screen found.
#[test]
fn post_image_init_derivations_are_classified_and_cited() {
    crate::test_utils::init_test_tracing();

    let (mut ported, mut elsewhere, mut not_applicable) = (0, 0, 0);
    for site in PostImageInit::ALL {
        let Establishes::Facts { derived, .. } = site.site().establishes else {
            continue;
        };
        for fact in derived {
            assert!(
                fact.gnu().starts_with("src/"),
                "{}: derived fact `{}' must cite the GNU lines that establish it",
                site.site().c_name,
                fact.what()
            );
            assert!(fact.what().len() > 20);
            match fact {
                Derived::Ported { .. } => ported += 1,
                Derived::Elsewhere { by, .. } => {
                    assert!(!by.is_empty());
                    elsewhere += 1;
                }
                Derived::NotApplicable { why, .. } => {
                    assert!(!why.is_empty());
                    not_applicable += 1;
                }
            }
        }
    }
    // exec-path/exec-directory, shell-file-name, charset-map-path, font-log.
    assert_eq!(
        ported, 4,
        "four derivations are performed by this sequence itself"
    );
    assert_eq!(
        elsewhere, 21,
        "twenty-one are performed elsewhere on this port's startup path"
    );
    // shared-game-score-directory.
    assert_eq!(not_applicable, 1, "one has nothing here to derive");
}

/// The five rows entry 174 shipped, named literally so deleting the table
/// fails this test instead of shrinking an iteration nobody counts.
#[test]
fn post_image_init_keeps_gnu_init_lread_constants() {
    crate::test_utils::init_test_tracing();

    let rows = PostImageInit::Lread.constants();
    let expected = [
        ("values", ResetValue::Nil),
        ("load-in-progress", ResetValue::Nil),
        ("load-file-name", ResetValue::Nil),
        ("load-true-file-name", ResetValue::Nil),
        ("standard-input", ResetValue::T),
    ];
    assert_eq!(rows.len(), expected.len(), "GNU src/lread.c:5522-5527");
    for (row, (name, value)) in rows.iter().zip(expected) {
        assert_eq!(row.name, name);
        assert_eq!(row.value, value, "GNU resets `{name}'");
    }
}

/// The constants ledger 177's screen ADDED, likewise named literally.
#[test]
fn post_image_init_covers_the_constants_the_screen_found() {
    crate::test_utils::init_test_tracing();

    let named = |site: PostImageInit, name: &str| -> Option<ResetValue> {
        site.constants()
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.value)
    };

    assert_eq!(
        named(PostImageInit::Alloc, "gcs-done"),
        Some(ResetValue::Fixnum(0)),
        "GNU src/alloc.c:7392"
    );
    assert_eq!(
        named(PostImageInit::Alloc, "gc-elapsed"),
        Some(ResetValue::Float(0.0)),
        "GNU src/alloc.c:7391"
    );
    assert_eq!(
        named(PostImageInit::Bignum, "integer-width"),
        Some(ResetValue::Fixnum(65536)),
        "GNU src/bignum.c:55"
    );
    assert_eq!(
        named(PostImageInit::Eval, "quit-flag"),
        Some(ResetValue::Nil),
        "GNU src/eval.c:247"
    );
    assert_eq!(
        named(PostImageInit::Eval, "debug-on-next-call"),
        Some(ResetValue::Nil),
        "GNU src/eval.c:248"
    );
    assert_eq!(
        named(PostImageInit::Macros, "executing-kbd-macro"),
        Some(ResetValue::Nil),
        "GNU src/macros.c:395"
    );
    assert_eq!(
        named(PostImageInit::Keyboard, "unread-command-events"),
        Some(ResetValue::Nil),
        "GNU src/keyboard.c:13206"
    );
    assert_eq!(
        named(PostImageInit::Keyboard, "defining-kbd-macro"),
        Some(ResetValue::Nil),
        "GNU src/keyboard.c:13120 via init_kboard"
    );
    assert_eq!(
        named(PostImageInit::ProcessEmacs, "internal--daemon-sockname"),
        Some(ResetValue::Nil),
        "GNU src/process.c:8761"
    );
    assert_eq!(
        PostImageInit::Keyboard.constants().len(),
        15,
        "init_keyboard plus the init_kboard it calls assign 15 Lisp variables \
         to constants.  `window-system' is the SIXTEENTH assignment there and \
         is deliberately NOT one: init_kboard sets it to its TYPE argument, \
         and this port's GUI startup path assigns it after this sequence runs, \
         so a constant row would clobber a live window system."
    );
}

/// The table has to be APPLIED, not merely well-formed.  This drives it over
/// a context whose variables are all wrong and checks every row landed.
#[test]
fn apply_post_image_init_assigns_every_constant_row() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // Poison every row with a value GNU's reset must overwrite.  The poison
    // has to respect the variable's own type: `gcs-done' is int-forwarded and
    // REFUSES a string outright, which is itself worth knowing -- a reset that
    // "worked" only because the write was rejected would be a false green.
    let poison = |row_value: ResetValue| -> Value {
        match row_value {
            ResetValue::Nil | ResetValue::T => Value::string("dumped-mid-loadup"),
            ResetValue::Fixnum(n) => Value::fixnum(n.wrapping_add(4321)),
            ResetValue::Float(f) => Value::make_float(f + 17.5),
        }
    };
    let mut rows = 0;
    for site in PostImageInit::ALL {
        for row in site.constants() {
            eval.set_variable(row.name, poison(row.value));
            rows += 1;
        }
    }
    assert_eq!(
        rows, 27,
        "the screen found 27 constant rows across the sequence"
    );

    apply_post_image_init(&mut eval);

    for site in PostImageInit::ALL {
        for row in site.constants() {
            let got = eval.visible_variable_value_or_nil(row.name);
            assert_ne!(
                got,
                poison(row.value),
                "`{}' was never reset ({})",
                row.name,
                row.gnu
            );
            match row.value {
                ResetValue::Nil => assert_eq!(got, Value::NIL, "`{}' ({})", row.name, row.gnu),
                ResetValue::T => assert_eq!(got, Value::T, "`{}' ({})", row.name, row.gnu),
                ResetValue::Fixnum(n) => {
                    assert_eq!(got.as_fixnum(), Some(n), "`{}' ({})", row.name, row.gnu)
                }
                ResetValue::Float(f) => {
                    assert_eq!(got.as_float(), Some(f), "`{}' ({})", row.name, row.gnu)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The two divergences the screen found, on a finalized runtime image.
// ---------------------------------------------------------------------------

fn finalized_runtime() -> crate::emacs_core::eval::Context {
    crate::emacs_core::load::create_bootstrap_evaluator_cached().expect("cached bootstrap")
}

/// GNU `init_alloc` (src/alloc.c:7391-7392) zeroes `gc-elapsed` and `gcs-done`
/// AFTER the image is loaded, so a dumped image does not hand the session
/// loadup's collection count.
///
/// This is the row the screen's own NORMALIZATION nearly hid.  The probe
/// compared "the counter was reset" as `gcs-done < 100`, and both editors
/// answered `t` -- but GNU answers exactly `0` and this port answered `9`,
/// which was visible only in the raw row printed beside it.  The fix came out
/// of reading src/alloc.c rather than out of the comparison, which is the
/// argument for reading every body BEFORE running the screen.
#[test]
fn runtime_gc_counters_are_zeroed_like_gnu_init_alloc() {
    crate::test_utils::init_test_tracing();
    let eval = finalized_runtime();

    assert_eq!(
        eval.visible_variable_value_or_nil("gcs-done").as_fixnum(),
        Some(0),
        "GNU src/alloc.c:7392 -- the session must not inherit loadup's GC count"
    );
    assert_eq!(
        eval.visible_variable_value_or_nil("gc-elapsed").as_float(),
        Some(0.0),
        "GNU src/alloc.c:7391"
    );
}

/// GNU `init_callproc_1` (src/callproc.c:1960-1963) builds `exec-path` as
/// `$PATH` followed by the EMACSPATH/PATH_EXEC list whose CAR becomes
/// `exec-directory`, so the last element of `exec-path` IS `exec-directory`:
///
/// ```c
///   Vexec_path = decode_env_path ("EMACSPATH", PATH_EXEC, 0);   /* :1960 */
///   Vexec_directory = Ffile_name_as_directory (Fcar (Vexec_path));
///   Vexec_path = nconc2 (decode_env_path ("PATH", NULL, 0), Vexec_path);
/// ```
///
/// This port set `exec-path` from `$PATH` alone and `exec-directory` from the
/// running executable's directory, and never joined them -- so nothing Emacs
/// ships alongside its own binary was findable.  `(executable-find
/// "neomacsclient")' answered nil where GNU's `(executable-find "etags")'
/// answers the lib-src path.
#[test]
fn runtime_exec_path_ends_with_exec_directory_like_gnu_init_callproc() {
    crate::test_utils::init_test_tracing();
    let eval = finalized_runtime();

    let exec_directory = eval.visible_variable_value_or_nil("exec-directory");
    let exec_path = eval.visible_variable_value_or_nil("exec-path");
    let entries: Vec<Value> = list_to_vec(&exec_path).unwrap_or_default();
    assert!(
        !entries.is_empty(),
        "exec-path must not be empty; GNU always has at least PATH_EXEC"
    );

    let last = *entries.last().expect("non-empty");
    let as_dir = |v: Value| -> String {
        let s = text(v);
        if s.ends_with('/') { s } else { format!("{s}/") }
    };
    assert_eq!(
        as_dir(last),
        as_dir(exec_directory),
        "GNU src/callproc.c:1960-1963 leaves `exec-directory' as the tail of \
         `exec-path'; here exec-path ends at {last:?} while exec-directory is \
         {exec_directory:?}"
    );
}

/// GNU `init_charset` (src/charset.c:2303-2327) sets `charset-map-path` to
/// the single directory `<data-directory>/charsets`, and exits(1) rather than
/// continuing when that directory is not accessible.  This port left the
/// variable at the nil its DEFVAR carries, so no charset map file was ever
/// findable through it -- and `ensure_startup_compat_variables' could not
/// have fixed it, because that table only assigns a default when the variable
/// is UNSET, which after a dump it never is.
#[test]
fn runtime_charset_map_path_is_data_directory_charsets_like_gnu_init_charset() {
    crate::test_utils::init_test_tracing();
    let eval = finalized_runtime();

    let data_directory = text(eval.visible_variable_value_or_nil("data-directory"));
    assert!(!data_directory.is_empty(), "data-directory is a string");
    let charset_map_path = eval.visible_variable_value_or_nil("charset-map-path");
    let entries: Vec<String> = list_to_vec(&charset_map_path)
        .unwrap_or_default()
        .into_iter()
        .map(text)
        .collect();

    let expected = format!("{}charsets", data_directory);
    assert_eq!(
        entries,
        vec![expected.clone()],
        "GNU src/charset.c:2306,2326 -- charset-map-path is exactly \
         ({expected:?})"
    );
    assert!(
        std::path::Path::new(&expected).is_dir(),
        "GNU exit(1)s when {expected:?} is not an accessible directory \
         (src/charset.c:2307-2324)"
    );
}
