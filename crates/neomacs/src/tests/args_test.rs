//! Unit tests for the `args` module — the GNU `argmatch` port.
//!
//! Each test here cites the GNU `emacs.c` line it reproduces, so the
//! parity intent stays explicit.

use super::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn exact_short_no_value() {
    // emacs.c:1696 — argmatch(argv, argc, "-nw", "--no-window-system", 6, NULL, &skip_args)
    let mut idx = 0;
    let argv = args(&["neomacs", "-nw"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 1);
}

#[test]
fn exact_long_no_value() {
    let mut idx = 0;
    let argv = args(&["neomacs", "--no-window-system"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 1);
}

#[test]
fn long_prefix_meets_minlen() {
    // GNU's `argmatch` uses strncmp(arg, lstr, arglen) — i.e. the user's
    // argument must be a prefix of the long form. So `--no-w` (6 chars)
    // matches `--no-window-system` because the first 6 characters of
    // both are identical and 6 >= minlen.
    let mut idx = 0;
    let argv = args(&["neomacs", "--no-w"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 1);
}

#[test]
fn long_form_distinct_aliases_need_separate_calls() {
    // GNU emacs.c:1697 declares "--no-windows" via a separate argmatch
    // call: the strings "--no-windows" and "--no-window-system" diverge
    // at character 11 ('s' vs '-'), so a single argmatch with lstr =
    // "--no-window-system" cannot match a user typing "--no-windows".
    // GNU works around this with TWO consecutive argmatch calls in
    // emacs.c:1696-1697; we test both halves of that pair.
    let mut idx = 0;
    let argv = args(&["neomacs", "--no-windows"]);
    // Call 1: lstr = "--no-window-system" — must NOT match.
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::NoMatch,
    );
    assert_eq!(idx, 0);
    // Call 2: lstr = "--no-windows" — matches.
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-windows"), 6, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 1);
}

#[test]
fn long_prefix_below_minlen_does_not_match() {
    // "--no" is 4 chars, below the 6-char minlen for --no-window-system.
    let mut idx = 0;
    let argv = args(&["neomacs", "--no"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::NoMatch,
    );
    assert_eq!(idx, 0);
}

#[test]
fn long_with_value_inline_eq() {
    // --temacs=pdump form
    let mut idx = 0;
    let argv = args(&["neomacs", "--temacs=pdump"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-temacs", Some("--temacs"), 8, true),
        ArgMatch::Value("pdump".to_string()),
    );
    assert_eq!(idx, 1);
}

#[test]
fn long_with_value_separate_token() {
    let mut idx = 0;
    let argv = args(&["neomacs", "--chdir", "/tmp"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-chdir", Some("--chdir"), 4, true),
        ArgMatch::Value("/tmp".to_string()),
    );
    assert_eq!(idx, 2);
}

#[test]
fn short_with_value_separate_token() {
    let mut idx = 0;
    let argv = args(&["neomacs", "-t", "/dev/pts/3"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-t", Some("--terminal"), 4, true),
        ArgMatch::Value("/dev/pts/3".to_string()),
    );
    assert_eq!(idx, 2);
}

#[test]
fn missing_value_at_eof_signals_explicitly() {
    // GNU returns 0 here; we return MissingValue so the caller
    // can produce "neomacs: option `--chdir' requires an argument".
    let mut idx = 0;
    let argv = args(&["neomacs", "--chdir"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-chdir", Some("--chdir"), 4, true),
        ArgMatch::MissingValue,
    );
    assert_eq!(idx, 0);
}

#[test]
fn no_match_leaves_idx_untouched() {
    let mut idx = 0;
    let argv = args(&["neomacs", "--unrelated"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::NoMatch,
    );
    assert_eq!(idx, 0);
}

#[test]
fn empty_argv_returns_no_match() {
    let mut idx = 0;
    let argv = args(&["neomacs"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::NoMatch,
    );
    assert_eq!(idx, 0);
}

#[test]
fn long_form_only_short_unset() {
    // Some GNU entries have lstr = NULL (e.g. "-display" alone) and a
    // distinct short string. We model that with `lstr = None`; only the
    // exact short match path is exercised.
    let mut idx = 0;
    let argv = args(&["neomacs", "-display"]);
    assert_eq!(
        argmatch(&argv, &mut idx, "-display", None, 0, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 1);
}

#[test]
fn idx_threading_walks_argv_one_match_at_a_time() {
    // Multiple flags on the same argv are handled by repeated calls
    // sharing the same `idx` cursor — the parser pattern in main.rs.
    let argv = args(&["neomacs", "-nw", "--batch"]);
    let mut idx = 0;

    assert_eq!(
        argmatch(&argv, &mut idx, "-nw", Some("--no-window-system"), 6, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 1);

    assert_eq!(
        argmatch(&argv, &mut idx, "-batch", Some("--batch"), 5, false),
        ArgMatch::Bare,
    );
    assert_eq!(idx, 2);
}

// ---------- sort_args ----------

#[test]
fn sort_args_keeps_program_name_at_index_zero() {
    // GNU emacs.c:2895 — `new[0] = argv[0];` always.
    let mut argv = args(&["neomacs"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs"]);
}

#[test]
fn sort_args_brings_high_priority_options_before_files() {
    // The headline behavior: a file name typed before a high-priority
    // option still ends up after the option in the sorted result.
    // GNU's `--no-splash` has priority 3; plain file args have priority 0.
    let mut argv = args(&["neomacs", "file.txt", "--no-splash"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs", "--no-splash", "file.txt"]);
}

#[test]
fn sort_args_respects_priority_ordering_among_options() {
    // -nw priority 110 > --no-splash priority 3 > -L priority 0.
    let mut argv = args(&["neomacs", "-L", "/lib", "--no-splash", "-nw"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs", "-nw", "--no-splash", "-L", "/lib"]);
}

#[test]
fn sort_args_keeps_option_value_pairs_glued() {
    // -L /lib must travel together; -nw must come first.
    let mut argv = args(&["neomacs", "-L", "/usr/lib", "-nw"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs", "-nw", "-L", "/usr/lib"]);
}

#[test]
fn sort_args_dedupes_zero_arg_duplicate_options() {
    // GNU emacs.c:2920 — duplicate zero-arg options collapse.
    let mut argv = args(&["neomacs", "-nw", "-nw"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs", "-nw"]);
}

#[test]
fn sort_args_does_not_dedupe_value_taking_options() {
    // -L appears twice with different values — both must survive.
    let mut argv = args(&["neomacs", "-L", "/a", "-L", "/b"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs", "-L", "/a", "-L", "/b"]);
}

#[test]
fn sort_args_stable_within_equal_priority() {
    // -bg, -fg, -bd all share priority 10. Their relative order must
    // be preserved. GNU's stable scan is "first in argv wins".
    let mut argv = args(&["neomacs", "-bd", "blue", "-fg", "white", "-bg", "black"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(
        argv,
        vec!["neomacs", "-bd", "blue", "-fg", "white", "-bg", "black",]
    );
}

#[test]
fn sort_args_long_form_inline_value_collapses_nargs() {
    // --temacs=pdump must be treated as a 1-token slot (nargs=0 in the
    // sort) — GNU emacs.c:2876-2877 sets options[from] = 0 when an
    // equals sign is present.
    let mut argv = args(&["neomacs", "file.el", "--temacs=pdump"]);
    sort_args(&mut argv).unwrap();
    // --temacs has priority 1, file.el has priority 0.
    assert_eq!(argv, vec!["neomacs", "--temacs=pdump", "file.el"]);
}

#[test]
fn sort_args_double_dash_terminator_pins_remaining_args_to_end() {
    // Everything after `--` gets priority -100; -nw has 110.
    let mut argv = args(&["neomacs", "--", "literal-arg", "-nw"]);
    sort_args(&mut argv).unwrap();
    // -nw is INSIDE the post-`--` region so it stays where it was.
    // The `--` terminator sticks to the front of the post-region.
    assert_eq!(argv, vec!["neomacs", "--", "literal-arg", "-nw"]);
}

#[test]
fn sort_args_kill_sinks_to_the_end() {
    // `-kill` has priority -10, lower than file-name args at 0.
    let mut argv = args(&["neomacs", "-kill", "file.txt"]);
    sort_args(&mut argv).unwrap();
    assert_eq!(argv, vec!["neomacs", "file.txt", "-kill"]);
}

#[test]
fn sort_args_missing_value_returns_error() {
    // `-L` declared as nargs=1; supplying it at the end of argv must
    // surface a GNU-shaped error message.
    let mut argv = args(&["neomacs", "-L"]);
    let err = sort_args(&mut argv).unwrap_err();
    assert!(err.contains("'-L'"), "error message: {err}");
    assert!(err.contains("requires an argument"));
}

#[test]
fn sort_args_unknown_flag_treated_as_plain_arg() {
    // Unknown flags drop to priority 0 like plain file-name args, so
    // they neither get reordered relative to other plain args nor
    // crash the parser.
    let mut argv = args(&["neomacs", "--no-splash", "--unknown", "file.txt"]);
    sort_args(&mut argv).unwrap();
    // --no-splash (priority 3) jumps ahead; --unknown and file.txt
    // keep their relative order at priority 0.
    assert_eq!(
        argv,
        vec!["neomacs", "--no-splash", "--unknown", "file.txt"]
    );
}
