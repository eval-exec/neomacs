macro_rules! source_files {
    ($($path:literal),+ $(,)?) => {
        [$(($path, include_str!(concat!("../../", $path)))),+]
    };
}

#[test]
fn migrated_string_subsystems_do_not_call_generic_runtime_string_adapter_directly() {
    let forbidden = concat!("lisp_string", "_to_runtime_string(");
    for (path, source) in source_files![
        "commands/abbrev/mod.rs",
        "lisp/autoload/mod.rs",
        "editing/bookmark/mod.rs",
        "editing/buffer/mod.rs",
        "lisp/native/builtins/misc_eval.rs",
        "lisp/native/builtins/misc_pure.rs",
        "lisp/native/builtins/symbols.rs",
        "lisp/native/builtins/stubs.rs",
        "lisp/native/builtins_extra/mod.rs",
        "system/callproc/mod.rs",
        "text/charset/mod.rs",
        "text/coding/mod.rs",
        "editing/dired/mod.rs",
        "display/display/mod.rs",
        "editing/editfns/mod.rs",
        "runtime/errors/mod.rs",
        "system/fileio/mod.rs",
        "system/filelock/mod.rs",
        "lisp/native/fns/mod.rs",
        "display/font/mod.rs",
        "display/fontset/mod.rs",
        "text/format/mod.rs",
        "runtime/eval/mod.rs",
        "commands/interactive/mod.rs",
        "commands/keyboard/pure.rs",
        "commands/kmacro/mod.rs",
        "lisp/load/mod.rs",
        "lisp/lread/mod.rs",
        "editing/marker/mod.rs",
        "commands/minibuffer/mod.rs",
        "lisp/native/misc/mod.rs",
        "system/network/mod.rs",
        "system/process/mod.rs",
        "lisp/reader/mod.rs",
        "text/syntax/mod.rs",
        "text/textprop/mod.rs",
        "system/timefns/mod.rs",
        "system/timer/mod.rs",
        "editing/undo/mod.rs",
        "runtime/value_reader/mod.rs",
        "display/window_cmds/mod.rs",
        "display/xdisp/mod.rs",
    ] {
        assert!(
            !source.contains(forbidden),
            "{path} should use subsystem-local string helpers instead of the generic runtime-string adapter"
        );
    }
}

#[test]
fn semantic_string_subsystems_do_not_reintroduce_utf8_unwraps() {
    let forbidden = concat!("as_str", "().unwrap(");
    for (path, source) in source_files![
        "lisp/native/builtins/symbols.rs",
        "lisp/cl_lib/mod.rs",
        "text/search/mod.rs",
    ] {
        assert!(
            !source.contains(forbidden),
            "{path} should use LispString/runtime helpers instead of UTF-8 unwraps"
        );
    }
}

#[test]
fn live_treesit_paths_do_not_use_buffer_string_adapter() {
    let forbidden = concat!("buffer_", "string(");
    for (path, source) in
        source_files!["lisp/native/builtins/treesit.rs", "editing/editfns/mod.rs",]
    {
        assert!(
            !source.contains(forbidden),
            "{path} should use explicit buffer source helpers instead of buffer_string()"
        );
    }
}
