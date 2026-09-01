//! Argv-handling primitives, ported from GNU Emacs `src/emacs.c`.
//!
//! This module mirrors the C-side of GNU's command-line parsing so the
//! Rust binary can stay bug-compatible with `emacs` for the small set of
//! flags that must be observed before the Lisp evaluator runs. Flags
//! that GNU forwards to `lisp/startup.el` are also forwarded here — see
//! `parse_startup_options` in `main.rs` for the consume vs. forward
//! boundary, and `drafts/argv-parity-audit.md` for the full ground-truth
//! table.
//!
//! Currently provides:
//!
//! - [`argmatch`] — ports `emacs.c:686-738`. The single primitive used to
//!   recognize a flag at a given index in argv, in either short
//!   (`-nw`) or long-prefix (`--no-window-system` or `--no-windows`)
//!   form, with both `--opt VAL` and `--opt=VAL` value forms.
//! - [`STANDARD_ARGS`] — direct port of GNU's `standard_args[]` table at
//!   `emacs.c:2646-2766`. Used by [`sort_args`] to assign priorities to
//!   each argv element.
//! - [`sort_args`] — ports `emacs.c:2796-2945`. Reorders argv so that
//!   higher-priority options come first while keeping option/value pairs
//!   together and deduping zero-arg duplicates. Called once near the top
//!   of `parse_startup_options` so the rest of the parser (and Lisp's
//!   `command-line` / `command-line-1`) sees argv in GNU's canonical
//!   order regardless of how the user typed it.

/// Result of one [`argmatch`] call against the next position in argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArgMatch {
    /// No match at this position. The caller's index has not been
    /// advanced and the args are unchanged.
    NoMatch,
    /// Matched a no-value flag. The caller's index has been advanced by
    /// one position past the flag.
    Bare,
    /// Matched a value-taking flag. The caller's index has been advanced
    /// by one or two positions depending on whether the value was inline
    /// (`--opt=VAL`) or in the next argv slot (`--opt VAL` / `-o VAL`).
    Value(String),
    /// The flag matched in long form but the value-taking variant
    /// could not find a value. Mirrors GNU's "Option requires an
    /// argument" failure for the value form. The caller's index is
    /// unchanged.
    MissingValue,
}

/// Mirror of `argmatch` from GNU `src/emacs.c:685-738`.
///
/// Tests `args[*idx + 1]` (the slot **after** the current cursor) against
/// either `sstr` (short form like `-nw`) or `lstr` (long form like
/// `--no-window-system`). The long form matches a prefix of the user's
/// argument so long as the prefix is at least `minlen` characters; this
/// is what lets `emacs --no-windows` distinguish between
/// `--no-window-system` and `--no-windows` even though both share a
/// prefix.
///
/// `*idx` is the GNU `*skipptr` — the index of the last consumed entry
/// in argv. The very first call uses `idx = 0` (so we look at argv[1]),
/// and each successful match advances it past the consumed entry.
///
/// `takes_value = true` corresponds to GNU's `valptr != NULL` branch.
/// On success, the value is returned via [`ArgMatch::Value`]. On a long
/// match where the value is missing, [`ArgMatch::MissingValue`] is
/// returned (GNU returns 0 in that branch — we surface a richer signal
/// so the caller can produce a useful error message).
///
/// **Index semantics**: `*idx` mirrors GNU's `*skipptr`, which points to
/// the last *consumed* slot. So `args[*idx + 1]` is the slot under
/// consideration, and a successful match advances `*idx` by 1 (no
/// value), 1 (`--opt=VAL`), or 2 (`--opt VAL` / `-o VAL`).
pub(crate) fn argmatch(
    args: &[String],
    idx: &mut usize,
    sstr: &str,
    lstr: Option<&str>,
    minlen: usize,
    takes_value: bool,
) -> ArgMatch {
    // GNU's `if (argc <= *skipptr + 1) return 0;` — give up if there is
    // no slot at the cursor position.
    if args.len() <= *idx + 1 {
        return ArgMatch::NoMatch;
    }
    let arg = &args[*idx + 1];

    // Exact short-form match.
    if arg == sstr {
        if takes_value {
            // GNU: *valptr = argv[*skipptr+2]; *skipptr += 2;
            if let Some(value) = args.get(*idx + 2) {
                let value = value.clone();
                *idx += 2;
                return ArgMatch::Value(value);
            }
            return ArgMatch::MissingValue;
        }
        *idx += 1;
        return ArgMatch::Bare;
    }

    // Long-form prefix match. GNU computes `arglen` as the index of an
    // optional `=` in the user's argument (only when `valptr != NULL`),
    // otherwise the full string length. Then it checks
    //   arglen >= minlen && strncmp(arg, lstr, arglen) == 0
    let Some(lstr) = lstr else {
        return ArgMatch::NoMatch;
    };
    let eq_pos = if takes_value { arg.find('=') } else { None };
    let arglen = eq_pos.unwrap_or(arg.len());
    if arglen < minlen {
        return ArgMatch::NoMatch;
    }
    if !lstr.starts_with(&arg[..arglen]) {
        return ArgMatch::NoMatch;
    }

    if !takes_value {
        *idx += 1;
        return ArgMatch::Bare;
    }

    if let Some(eq_pos) = eq_pos {
        // GNU: *valptr = p+1; *skipptr += 1;
        let value = arg[eq_pos + 1..].to_string();
        *idx += 1;
        return ArgMatch::Value(value);
    }

    // GNU: *valptr = argv[*skipptr+2]; *skipptr += 2;
    if let Some(value) = args.get(*idx + 2) {
        let value = value.clone();
        *idx += 2;
        return ArgMatch::Value(value);
    }
    ArgMatch::MissingValue
}

/// One row of GNU's `standard_args[]` priority table.
///
/// Mirrors `struct standard_args` at `emacs.c:2638-2644`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StandardArg {
    /// Short / canonical form, e.g. "-nw". Always set in GNU; we keep
    /// the same invariant.
    pub(crate) name: &'static str,
    /// Long form, e.g. "--no-window-system". `None` corresponds to GNU
    /// rows where `longname` is the literal `0`/`NULL`, used for
    /// alternate spellings like `-display` (no `--display` alias).
    pub(crate) longname: Option<&'static str>,
    /// Higher numbers come first after [`sort_args`]. Negative
    /// priorities (e.g. `-kill`) sink to the end.
    pub(crate) priority: i32,
    /// Number of value tokens this option owns. Used to keep
    /// option/value pairs glued together during the sort.
    pub(crate) nargs: u8,
}

/// Direct port of GNU `standard_args[]` from `emacs.c:2646-2766`.
///
/// Each row is reproduced verbatim from GNU except for rows whose
/// subsystems we still do not implement (`-seccomp`, NS-only flags,
/// `-module-assertions`). Their absence does not change the relative
/// ordering of the rows we do support.
///
/// **Do not reorder this table.** Some priority values are deliberately
/// shared (e.g. all GUI display flags share priority 10) and `sort_args`
/// uses array order as the tiebreaker — exactly mirroring GNU's stable
/// sort behavior.
pub(crate) static STANDARD_ARGS: &[StandardArg] = &[
    StandardArg {
        name: "-version",
        longname: Some("--version"),
        priority: 150,
        nargs: 0,
    },
    StandardArg {
        name: "-fingerprint",
        longname: Some("--fingerprint"),
        priority: 140,
        nargs: 0,
    },
    StandardArg {
        name: "-chdir",
        longname: Some("--chdir"),
        priority: 130,
        nargs: 1,
    },
    StandardArg {
        name: "-t",
        longname: Some("--terminal"),
        priority: 120,
        nargs: 1,
    },
    StandardArg {
        name: "-nw",
        longname: Some("--no-window-system"),
        priority: 110,
        nargs: 0,
    },
    StandardArg {
        name: "-nw",
        longname: Some("--no-windows"),
        priority: 110,
        nargs: 0,
    },
    StandardArg {
        name: "-batch",
        longname: Some("--batch"),
        priority: 100,
        nargs: 0,
    },
    StandardArg {
        name: "-script",
        longname: Some("--script"),
        priority: 100,
        nargs: 1,
    },
    // -daemon family omitted (no daemon mode).
    StandardArg {
        name: "-help",
        longname: Some("--help"),
        priority: 90,
        nargs: 0,
    },
    StandardArg {
        name: "-nl",
        longname: Some("--no-loadup"),
        priority: 70,
        nargs: 0,
    },
    StandardArg {
        name: "-nsl",
        longname: Some("--no-site-lisp"),
        priority: 65,
        nargs: 0,
    },
    StandardArg {
        name: "-no-build-details",
        longname: Some("--no-build-details"),
        priority: 63,
        nargs: 0,
    },
    StandardArg {
        name: "-module-assertions",
        longname: Some("--module-assertions"),
        priority: 52,
        nargs: 0,
    },
    StandardArg {
        name: "-d",
        longname: Some("--display"),
        priority: 60,
        nargs: 1,
    },
    StandardArg {
        name: "-display",
        longname: None,
        priority: 60,
        nargs: 1,
    },
    // Now for the options handled in `command-line` (startup.el).
    StandardArg {
        name: "-Q",
        longname: Some("--quick"),
        priority: 55,
        nargs: 0,
    },
    StandardArg {
        name: "-quick",
        longname: None,
        priority: 55,
        nargs: 0,
    },
    StandardArg {
        name: "-x",
        longname: None,
        priority: 55,
        nargs: 0,
    },
    StandardArg {
        name: "-q",
        longname: Some("--no-init-file"),
        priority: 50,
        nargs: 0,
    },
    StandardArg {
        name: "-no-init-file",
        longname: None,
        priority: 50,
        nargs: 0,
    },
    StandardArg {
        name: "-init-directory",
        longname: Some("--init-directory"),
        priority: 30,
        nargs: 1,
    },
    StandardArg {
        name: "-no-x-resources",
        longname: Some("--no-x-resources"),
        priority: 40,
        nargs: 0,
    },
    StandardArg {
        name: "-no-site-file",
        longname: Some("--no-site-file"),
        priority: 40,
        nargs: 0,
    },
    StandardArg {
        name: "-no-comp-spawn",
        longname: Some("--no-comp-spawn"),
        priority: 60,
        nargs: 0,
    },
    StandardArg {
        name: "-u",
        longname: Some("--user"),
        priority: 30,
        nargs: 1,
    },
    StandardArg {
        name: "-user",
        longname: None,
        priority: 30,
        nargs: 1,
    },
    StandardArg {
        name: "-debug-init",
        longname: Some("--debug-init"),
        priority: 20,
        nargs: 0,
    },
    StandardArg {
        name: "-iconic",
        longname: Some("--iconic"),
        priority: 15,
        nargs: 0,
    },
    StandardArg {
        name: "-D",
        longname: Some("--basic-display"),
        priority: 12,
        nargs: 0,
    },
    StandardArg {
        name: "-basic-display",
        longname: None,
        priority: 12,
        nargs: 0,
    },
    StandardArg {
        name: "-nbc",
        longname: Some("--no-blinking-cursor"),
        priority: 12,
        nargs: 0,
    },
    // Now for the options handled in `command-line-1' (startup.el).
    StandardArg {
        name: "-nbi",
        longname: Some("--no-bitmap-icon"),
        priority: 10,
        nargs: 0,
    },
    StandardArg {
        name: "-bg",
        longname: Some("--background-color"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-background",
        longname: None,
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-fg",
        longname: Some("--foreground-color"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-foreground",
        longname: None,
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-bd",
        longname: Some("--border-color"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-bw",
        longname: Some("--border-width"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-ib",
        longname: Some("--internal-border"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-ms",
        longname: Some("--mouse-color"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-cr",
        longname: Some("--cursor-color"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-fn",
        longname: Some("--font"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-font",
        longname: None,
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-fs",
        longname: Some("--fullscreen"),
        priority: 10,
        nargs: 0,
    },
    StandardArg {
        name: "-fw",
        longname: Some("--fullwidth"),
        priority: 10,
        nargs: 0,
    },
    StandardArg {
        name: "-fh",
        longname: Some("--fullheight"),
        priority: 10,
        nargs: 0,
    },
    StandardArg {
        name: "-mm",
        longname: Some("--maximized"),
        priority: 10,
        nargs: 0,
    },
    StandardArg {
        name: "-g",
        longname: Some("--geometry"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-geometry",
        longname: None,
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-T",
        longname: Some("--title"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-title",
        longname: None,
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-name",
        longname: Some("--name"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-xrm",
        longname: Some("--xrm"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-parent-id",
        longname: Some("--parent-id"),
        priority: 10,
        nargs: 1,
    },
    StandardArg {
        name: "-r",
        longname: Some("--reverse-video"),
        priority: 5,
        nargs: 0,
    },
    StandardArg {
        name: "-rv",
        longname: None,
        priority: 5,
        nargs: 0,
    },
    StandardArg {
        name: "-reverse",
        longname: None,
        priority: 5,
        nargs: 0,
    },
    StandardArg {
        name: "-hb",
        longname: Some("--horizontal-scroll-bars"),
        priority: 5,
        nargs: 0,
    },
    StandardArg {
        name: "-vb",
        longname: Some("--vertical-scroll-bars"),
        priority: 5,
        nargs: 0,
    },
    StandardArg {
        name: "-color",
        longname: Some("--color"),
        priority: 5,
        nargs: 0,
    },
    StandardArg {
        name: "-no-splash",
        longname: Some("--no-splash"),
        priority: 3,
        nargs: 0,
    },
    StandardArg {
        name: "-no-desktop",
        longname: Some("--no-desktop"),
        priority: 3,
        nargs: 0,
    },
    // Just above the file-name args, to get them out of our way without
    // mixing them with file names.
    StandardArg {
        name: "-temacs",
        longname: Some("--temacs"),
        priority: 1,
        nargs: 1,
    },
    StandardArg {
        name: "-dump-file",
        longname: Some("--dump-file"),
        priority: 1,
        nargs: 1,
    },
    // -seccomp omitted.
    // NS-only flags omitted.
    // These have the same priority as ordinary file name args, so they
    // are not reordered with respect to those.
    StandardArg {
        name: "-L",
        longname: Some("--directory"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-directory",
        longname: None,
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-l",
        longname: Some("--load"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-load",
        longname: None,
        priority: 0,
        nargs: 1,
    },
    // GNU comment: "no longname, because using --scriptload confuses sort_args"
    StandardArg {
        name: "-scriptload",
        longname: None,
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-f",
        longname: Some("--funcall"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-funcall",
        longname: None,
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-eval",
        longname: Some("--eval"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-execute",
        longname: Some("--execute"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-find-file",
        longname: Some("--find-file"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-visit",
        longname: Some("--visit"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-file",
        longname: Some("--file"),
        priority: 0,
        nargs: 1,
    },
    StandardArg {
        name: "-insert",
        longname: Some("--insert"),
        priority: 0,
        nargs: 1,
    },
    // Process after ordinary file name args and the like.
    StandardArg {
        name: "-kill",
        longname: Some("--kill"),
        priority: -10,
        nargs: 0,
    },
];

/// Reorder argv so that the highest priority options come first, mirroring
/// GNU `sort_args` at `emacs.c:2796-2945`.
///
/// Behavior:
///
/// 1. Each argv slot is categorized as either an option (matched against
///    [`STANDARD_ARGS`] by exact short or unambiguous long-prefix) or a
///    plain non-option argument. Plain args get priority 0.
/// 2. The slot at index 0 (the program name) is left untouched.
/// 3. After encountering `--`, all remaining slots get priority `-100`
///    so they sink to the end while preserving their relative order.
/// 4. The result is a stable sort by descending priority: equal
///    priorities keep their original relative order.
/// 5. Option/value pairs are kept together (a value-taking option's
///    nargs slots travel with it).
/// 6. If a zero-arg option appears more than once, only the first
///    occurrence is kept.
///
/// On error (an option declares nargs > 0 but its value slot is past
/// the end of argv), this function leaves argv unchanged and returns an
/// error string in the same form GNU's `fatal()` would print:
/// `Option '<flag>' requires an argument`. The caller turns it into the
/// usual `Result<_, String>` exit path.
pub(crate) fn sort_args(argv: &mut Vec<String>) -> Result<(), String> {
    let argc = argv.len();
    if argc <= 1 {
        return Ok(());
    }

    // Per-slot metadata mirroring GNU's `options[]` and `priority[]`.
    //
    // GNU uses `int options[from]` where -1 means "non-option", 0 means
    // "no-arg option", N means "option that owns the next N slots".
    // We use the same triple meaning here.
    let mut options: Vec<i32> = vec![-1; argc];
    let mut priority: Vec<i32> = vec![0; argc];

    let mut from = 1usize;
    while from < argc {
        let arg = argv[from].as_str();
        if !arg.starts_with('-') {
            from += 1;
            continue;
        }

        // GNU emacs.c:2823-2832 — `--` terminator: leave it and
        // everything after at the end of the sorted result.
        if arg == "--" {
            for slot in priority.iter_mut().take(argc).skip(from) {
                *slot = -100;
            }
            for slot in options.iter_mut().take(argc).skip(from) {
                *slot = -1;
            }
            break;
        }

        // GNU emacs.c:2836-2845 — exact match against the short
        // (canonical) name of any STANDARD_ARGS row.
        let mut matched = false;
        for entry in STANDARD_ARGS {
            if entry.name == arg {
                options[from] = i32::from(entry.nargs);
                priority[from] = entry.priority;
                if from + entry.nargs as usize >= argc {
                    return Err(format!("Option '{arg}' requires an argument"));
                }
                from += 1 + entry.nargs as usize;
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        // GNU emacs.c:2850-2891 — long-prefix match against any
        // STANDARD_ARGS row whose `longname` is set, with --opt=VAL
        // collapsing to nargs=0 (the value rides on the same slot).
        if arg.starts_with("--") {
            let (this_arg_for_match, has_eq) = match arg.find('=') {
                Some(eq_pos) => (&arg[..eq_pos], true),
                None => (arg, false),
            };

            let mut match_idx: Option<usize> = None;
            let mut multiple = false;
            for (i, entry) in STANDARD_ARGS.iter().enumerate() {
                let Some(longname) = entry.longname else {
                    continue;
                };
                if longname.starts_with(this_arg_for_match) {
                    if match_idx.is_none() {
                        match_idx = Some(i);
                    } else {
                        multiple = true;
                    }
                }
            }

            if let Some(i) = match_idx
                && !multiple
            {
                let entry = STANDARD_ARGS[i];
                let nargs = if has_eq { 0 } else { i32::from(entry.nargs) };
                options[from] = nargs;
                priority[from] = entry.priority;
                if from + nargs as usize >= argc {
                    return Err(format!("Option '{arg}' requires an argument"));
                }
                from += 1 + nargs as usize;
                continue;
            }
            // GNU just warns on ambiguous prefix; we silently leave it
            // as a non-option (priority 0) — same effective behavior.
        }

        from += 1;
    }

    // GNU emacs.c:2894-2934 — sort by descending priority into a
    // freshly built `new[]`. Within equal priority, the original order
    // is preserved (`best = first slot found at best_priority`).
    let mut new: Vec<Option<String>> = Vec::with_capacity(argc);
    new.push(Some(argv[0].clone())); // GNU keeps argv[0] in place.
    let mut consumed: Vec<bool> = vec![false; argc];
    consumed[0] = true;
    let mut incoming_used = 1usize;

    while incoming_used < argc {
        let mut best: Option<usize> = None;
        let mut best_priority: i32 = i32::MIN;

        let mut from = 1usize;
        while from < argc {
            if !consumed[from] && priority[from] > best_priority {
                best_priority = priority[from];
                best = Some(from);
            }
            // Skip option arguments — they ride with the option slot.
            if options[from] > 0 {
                from += 1 + options[from] as usize;
            } else {
                from += 1;
            }
        }

        let best = best.expect("sort_args: best slot not found despite incoming_used < argc");

        // Drop a duplicate zero-arg option (GNU emacs.c:2920-2926).
        let dup = options[best] == 0
            && new
                .last()
                .and_then(|s| s.as_deref())
                .map(|prev| prev == argv[best].as_str())
                .unwrap_or(false);
        if !dup {
            new.push(Some(argv[best].clone()));
            // GNU emacs.c:2924 — `for (i = 0; i < options[best]; i++)`.
            // GNU's `options[best]` is a signed int, so the loop simply
            // does not run for negative values (plain non-option slots
            // have options == -1). Use the same guard explicitly.
            let nargs_to_copy = options[best].max(0) as usize;
            for i in 0..nargs_to_copy {
                new.push(Some(argv[best + 1 + i].clone()));
            }
        }

        let span = 1 + options[best].max(0) as usize;
        incoming_used += span;
        for i in 0..span {
            consumed[best + i] = true;
        }
    }

    // GNU pads `new` with `NULL`s to argc; we just push None and then
    // collapse to the kept entries when writing back.
    let kept: Vec<String> = new.into_iter().flatten().collect();
    *argv = kept;
    Ok(())
}

#[cfg(test)]
#[path = "args_test.rs"]
mod args_test;
