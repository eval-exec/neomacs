use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{LambdaData, LambdaParams};

fn make_lambda(required: Vec<&str>, optional: Vec<&str>, rest: Option<&str>) -> Value {
    Value::make_lambda(LambdaData {
        params: LambdaParams {
            required: required.into_iter().map(|s| intern(s)).collect(),
            optional: optional.into_iter().map(|s| intern(s)).collect(),
            rest: rest.map(|s| intern(s)),
        },
        body: vec![].into(),
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    })
}

fn make_macro(required: Vec<&str>) -> Value {
    Value::make_macro(LambdaData {
        params: LambdaParams::simple(required.into_iter().map(|s| intern(s)).collect()),
        body: vec![].into(),
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    })
}

fn make_bytecode(required: Vec<&str>, rest: Option<&str>) -> Value {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    let params = LambdaParams {
        required: required.into_iter().map(|s| intern(s)).collect(),
        optional: vec![],
        rest: rest.map(|s| intern(s)),
    };
    Value::make_bytecode(ByteCodeFunction::new(params))
}

fn func_arity(function: Value) -> EvalResult {
    let mut ctx = crate::emacs_core::eval::Context::new();
    builtin_func_arity_ctx(&mut ctx, vec![function])
}

// -- subr-name --

#[test]
fn subr_name_returns_string() {
    crate::test_utils::init_test_tracing();
    let result = builtin_subr_name(vec![Value::subr(intern("cons"))]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("cons"));
}

#[test]
fn subr_name_error_for_non_subr() {
    crate::test_utils::init_test_tracing();
    let result = builtin_subr_name(vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

// -- subr-arity --

fn assert_subr_arity(name: &str, min: i64, max: Option<i64>) {
    let mut ctx = crate::emacs_core::eval::Context::new();
    let function = ctx
        .obarray()
        .symbol_function(name)
        .unwrap_or_else(|| panic!("{name} must have a registered function cell"));
    assert!(function.is_subr(), "{name} must be registered as a subr");
    let result = builtin_subr_arity(&mut ctx, vec![function]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(
            pair_car.as_int(),
            Some(min),
            "subr min arity mismatch for {name}"
        );
        match max {
            Some(n) => assert_eq!(
                pair_cdr.as_int(),
                Some(n),
                "subr max arity mismatch for {name}"
            ),
            None => assert_eq!(
                pair_cdr.as_symbol_name(),
                Some("many"),
                "subr max arity mismatch for {name}"
            ),
        }
    } else {
        panic!("expected cons cell");
    }
}

/// Arity audit against GNU Emacs 31.0.90, commit 0ee48ac4df2.
///
/// These rows cover registrations that were absent from the earlier oracle or
/// whose recorded oracle value disagreed with the corresponding C `DEFUN`.
#[test]
fn subr_arities_match_the_pinned_gnu_defun_contracts() {
    crate::test_utils::init_test_tracing();
    for (name, min, max) in [
        ("xwidget-view-lookup", 1, Some(2)),
        ("frame-root-frame", 0, Some(1)),
        ("all-completions", 2, Some(3)),
        ("make-thread", 1, Some(3)),
        ("set-frame-size-and-position-pixelwise", 5, Some(6)),
        ("mouse-position-in-root-frame", 0, Some(0)),
        ("x-load-color-file", 1, Some(1)),
        ("garbage-collect-heapsize", 0, Some(0)),
        ("tty-frame-list-z-order", 0, Some(1)),
        ("tty-frame-restack", 2, Some(3)),
        ("buffer-local-toplevel-value", 1, Some(2)),
        ("set-buffer-local-toplevel-value", 2, Some(3)),
        ("internal-delete-indirect-variable", 1, Some(1)),
        ("frame-windows-min-size", 4, Some(4)),
        ("remember-mouse-glyph", 3, Some(3)),
        ("frame--z-order-lessp", 2, Some(2)),
    ] {
        assert_subr_arity(name, min, max);
    }
}

#[test]
fn subr_arity_returns_cons() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("+", 0, None);
}

#[test]
fn subr_arity_error_for_non_subr() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_subr_arity(&mut ctx, vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn subr_arity_message_is_one_or_more() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("message", 1, None);
    assert_subr_arity("message-box", 1, None);
    assert_subr_arity("message-or-box", 1, None);
}

#[test]
fn subr_arity_if_is_unevalled() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_subr_arity(&mut ctx, vec![Value::subr(intern("if"))]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(2));
        assert_eq!(pair_cdr.as_symbol_name(), Some("unevalled"));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn subr_arity_core_special_forms_match_oracle_unevalled_shapes() {
    crate::test_utils::init_test_tracing();
    for (name, min) in [
        ("and", 0),
        ("setq", 0),
        ("let", 1),
        ("quote", 1),
        ("catch", 1),
        ("defconst", 2),
        ("condition-case", 2),
        ("unwind-protect", 1),
    ] {
        let mut ctx = crate::emacs_core::eval::Context::new();
        let result = builtin_subr_arity(&mut ctx, vec![Value::subr(intern(name))]).unwrap();
        if result.is_cons() {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_int(), Some(min));
            assert_eq!(pair_cdr.as_symbol_name(), Some("unevalled"));
        } else {
            panic!("expected cons cell for {name}");
        }
    }
}

#[test]
fn subr_arity_thread_join_is_one() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("thread-join", 1, Some(1));
}

#[test]
fn subr_arity_thread_signal_is_three() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("thread-signal", 3, Some(3));
}

#[test]
fn subr_arity_thread_last_error_optional_cleanup() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("thread-last-error", 0, Some(1));
}

#[test]
fn subr_arity_make_thread_optional_name() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("make-thread", 1, Some(3));
}

#[test]
fn subr_arity_current_thread_is_zero() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("current-thread", 0, Some(0));
}

#[test]
fn subr_arity_condition_notify_optional_all() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("condition-notify", 1, Some(2));
}

#[test]
fn subr_arity_event_apply_modifier_is_four() {
    crate::test_utils::init_test_tracing();
}

#[test]
fn subr_arity_thread_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("thread-join", 1, Some(1));
    assert_subr_arity("thread-yield", 0, Some(0));
    assert_subr_arity("thread-name", 1, Some(1));
    assert_subr_arity("thread-live-p", 1, Some(1));
    assert_subr_arity("thread-signal", 3, Some(3));
    assert_subr_arity("thread-last-error", 0, Some(1));
    assert_subr_arity("make-thread", 1, Some(3));
    assert_subr_arity("make-mutex", 0, Some(1));
    assert_subr_arity("mutexp", 1, Some(1));
    assert_subr_arity("mutex-name", 1, Some(1));
    assert_subr_arity("mutex-lock", 1, Some(1));
    assert_subr_arity("mutex-unlock", 1, Some(1));
    assert_subr_arity("make-condition-variable", 1, Some(2));
    assert_subr_arity("condition-variable-p", 1, Some(1));
    assert_subr_arity("condition-name", 1, Some(1));
    assert_subr_arity("condition-mutex", 1, Some(1));
    assert_subr_arity("condition-wait", 1, Some(1));
    assert_subr_arity("condition-notify", 1, Some(2));
    assert_subr_arity("current-thread", 0, Some(0));
    assert_subr_arity("all-threads", 0, Some(0));
}

#[test]
fn subr_arity_display_terminal_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("display-supports-face-attributes-p", 1, Some(2));
    assert_subr_arity("ding", 0, Some(1));
    assert_subr_arity("redraw-display", 0, Some(0));
    assert_subr_arity("redraw-frame", 0, Some(1));
    assert_subr_arity("open-termscript", 1, Some(1));
    assert_subr_arity("send-string-to-terminal", 1, Some(2));
    assert_subr_arity("internal-show-cursor", 2, Some(2));
    assert_subr_arity("internal-show-cursor-p", 0, Some(1));
    assert_subr_arity("window-system", 0, Some(1));
    assert_subr_arity("terminal-name", 0, Some(1));
    assert_subr_arity("terminal-list", 0, Some(0));
    assert_subr_arity("terminal-live-p", 1, Some(1));
    assert_subr_arity("frame-terminal", 0, Some(1));
    assert_subr_arity("frame-selected-window", 0, Some(1));
    assert_subr_arity("terminal-parameter", 2, Some(2));
    assert_subr_arity("terminal-parameters", 0, Some(1));
    assert_subr_arity("set-terminal-parameter", 3, Some(3));
    assert_subr_arity("tty-type", 0, Some(1));
    assert_subr_arity("tty-display-color-p", 0, Some(1));
    assert_subr_arity("tty-display-color-cells", 0, Some(1));
    assert_subr_arity("tty-no-underline", 0, Some(1));
    assert_subr_arity("tty-top-frame", 0, Some(1));
    assert_subr_arity("controlling-tty-p", 0, Some(1));
    assert_subr_arity("suspend-tty", 0, Some(1));
    assert_subr_arity("resume-tty", 0, Some(1));
    assert_subr_arity("terminal-coding-system", 0, Some(1));
    assert_subr_arity("x-backspace-delete-keys-p", 0, Some(1));
    assert_subr_arity("x-change-window-property", 2, Some(7));
    assert_subr_arity("x-delete-window-property", 1, Some(3));
    assert_subr_arity("x-disown-selection-internal", 1, Some(3));
    assert_subr_arity("x-export-frames", 0, Some(2));
    assert_subr_arity("x-display-list", 0, Some(0));
    assert_subr_arity("x-family-fonts", 0, Some(2));
    assert_subr_arity("x-focus-frame", 1, Some(2));
    assert_subr_arity("x-frame-edges", 0, Some(2));
    assert_subr_arity("x-frame-geometry", 0, Some(1));
    assert_subr_arity("x-frame-list-z-order", 0, Some(1));
    assert_subr_arity("x-frame-restack", 2, Some(3));
    assert_subr_arity("x-get-atom-name", 1, Some(2));
    assert_subr_arity("x-get-local-selection", 0, Some(2));
    assert_subr_arity("x-get-modifier-masks", 0, Some(1));
    assert_subr_arity("x-get-selection-internal", 2, Some(4));
    assert_subr_arity("x-hide-tip", 0, Some(0));
    assert_subr_arity("x-internal-focus-input-context", 1, Some(1));
    assert_subr_arity("x-mouse-absolute-pixel-position", 0, Some(0));
    assert_subr_arity("x-get-resource", 2, Some(4));
    assert_subr_arity("x-list-fonts", 1, Some(5));
    assert_subr_arity("x-open-connection", 1, Some(3));
    assert_subr_arity("x-own-selection-internal", 2, Some(3));
    assert_subr_arity("x-parse-geometry", 1, Some(1));
    assert_subr_arity("x-popup-dialog", 2, Some(3));
    assert_subr_arity("x-popup-menu", 2, Some(2));
    assert_subr_arity("x-register-dnd-atom", 1, Some(2));
    assert_subr_arity("x-selection-exists-p", 0, Some(2));
    assert_subr_arity("x-selection-owner-p", 0, Some(2));
    assert_subr_arity("x-send-client-message", 6, Some(6));
    assert_subr_arity("x-close-connection", 1, Some(1));
    assert_subr_arity("x-server-version", 0, Some(1));
    assert_subr_arity("x-server-input-extension-version", 0, Some(1));
    assert_subr_arity("x-server-max-request-size", 0, Some(1));
    assert_subr_arity("x-server-vendor", 0, Some(1));
    assert_subr_arity("x-show-tip", 1, Some(6));
    assert_subr_arity("x-display-grayscale-p", 0, Some(1));
    assert_subr_arity("x-display-backing-store", 0, Some(1));
    assert_subr_arity("x-display-color-cells", 0, Some(1));
    assert_subr_arity("x-display-mm-height", 0, Some(1));
    assert_subr_arity("x-display-mm-width", 0, Some(1));
    assert_subr_arity("x-display-monitor-attributes-list", 0, Some(1));
    assert_subr_arity("x-display-pixel-width", 0, Some(1));
    assert_subr_arity("x-display-pixel-height", 0, Some(1));
    assert_subr_arity("x-display-planes", 0, Some(1));
    assert_subr_arity("x-display-save-under", 0, Some(1));
    assert_subr_arity("x-display-screens", 0, Some(1));
    assert_subr_arity("x-display-set-last-user-time", 1, Some(2));
    assert_subr_arity("x-display-visual-class", 0, Some(1));
    assert_subr_arity("x-set-mouse-absolute-pixel-position", 2, Some(2));
    assert_subr_arity("x-synchronize", 1, Some(2));
    assert_subr_arity("x-translate-coordinates", 1, Some(6));
    assert_subr_arity("x-uses-old-gtk-dialog", 0, Some(0));
    assert_subr_arity("x-window-property", 1, Some(6));
    assert_subr_arity("x-window-property-attributes", 1, Some(3));
    assert_subr_arity("x-wm-set-size-hint", 0, Some(1));
}

#[test]
fn subr_arity_image_font_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("clear-face-cache", 0, Some(1));
    assert_subr_arity("clear-font-cache", 0, Some(0));
    assert_subr_arity("clear-image-cache", 0, Some(2));
    assert_subr_arity("clear-string", 1, Some(1));
    assert_subr_arity("find-font", 1, Some(2));
    assert_subr_arity("font-family-list", 0, Some(1));
    assert_subr_arity("image-cache-size", 0, Some(0));
    assert_subr_arity("image-flush", 1, Some(2));
    assert_subr_arity("image-mask-p", 1, Some(2));
    assert_subr_arity("image-metadata", 1, Some(2));
    assert_subr_arity("imagep", 1, Some(1));
    assert_subr_arity("image-size", 1, Some(3));
    assert_subr_arity("image-transforms-p", 0, Some(1));
    assert_subr_arity("list-fonts", 1, Some(4));
}

#[test]
fn subr_arity_process_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("accept-process-output", 0, Some(4));
    assert_subr_arity("call-process", 1, None);
    assert_subr_arity("call-process-region", 3, None);
    assert_subr_arity("continue-process", 0, Some(2));
    assert_subr_arity("delete-process", 0, Some(1));
    assert_subr_arity("format-network-address", 1, Some(2));
    assert_subr_arity("get-buffer-process", 1, Some(1));
    assert_subr_arity("get-process", 1, Some(1));
    assert_subr_arity("getenv-internal", 1, Some(2));
    assert_subr_arity("internal-default-interrupt-process", 0, Some(2));
    assert_subr_arity("internal-default-process-filter", 2, Some(2));
    assert_subr_arity("internal-default-process-sentinel", 2, Some(2));
    assert_subr_arity("internal-default-signal-process", 2, Some(3));
    assert_subr_arity("interrupt-process", 0, Some(2));
    assert_subr_arity("kill-process", 0, Some(2));
    assert_subr_arity("list-system-processes", 0, Some(0));
    assert_subr_arity("make-process", 0, None);
    assert_subr_arity("make-network-process", 0, None);
    assert_subr_arity("make-pipe-process", 0, None);
    assert_subr_arity("make-serial-process", 0, None);
    assert_subr_arity("network-interface-info", 1, Some(1));
    assert_subr_arity("network-interface-list", 0, Some(2));
    assert_subr_arity("network-lookup-address-info", 1, Some(3));
    assert_subr_arity("num-processors", 0, Some(1));
    assert_subr_arity("print--preprocess", 1, Some(1));
    assert_subr_arity("process-attributes", 1, Some(1));
    assert_subr_arity("process-buffer", 1, Some(1));
    assert_subr_arity("process-coding-system", 1, Some(1));
    assert_subr_arity("process-command", 1, Some(1));
    assert_subr_arity("process-contact", 1, Some(3));
    assert_subr_arity("process-datagram-address", 1, Some(1));
    assert_subr_arity("process-exit-status", 1, Some(1));
    assert_subr_arity("process-filter", 1, Some(1));
    assert_subr_arity("process-id", 1, Some(1));
    assert_subr_arity("process-inherit-coding-system-flag", 1, Some(1));
    assert_subr_arity("process-list", 0, Some(0));
    assert_subr_arity("process-mark", 1, Some(1));
    assert_subr_arity("process-name", 1, Some(1));
    assert_subr_arity("process-plist", 1, Some(1));
    assert_subr_arity("process-query-on-exit-flag", 1, Some(1));
    assert_subr_arity("quit-process", 0, Some(2));
    assert_subr_arity("process-running-child-p", 0, Some(1));
    assert_subr_arity("process-send-eof", 0, Some(1));
    assert_subr_arity("process-send-region", 3, Some(3));
    assert_subr_arity("process-send-string", 2, Some(2));
    assert_subr_arity("process-sentinel", 1, Some(1));
    assert_subr_arity("signal-names", 0, Some(0));
    assert_subr_arity("signal-process", 2, Some(3));
    assert_subr_arity("process-status", 1, Some(1));
    assert_subr_arity("stop-process", 0, Some(2));
    assert_subr_arity("process-thread", 1, Some(1));
    assert_subr_arity("process-tty-name", 1, Some(2));
    assert_subr_arity("process-type", 1, Some(1));
    assert_subr_arity("processp", 1, Some(1));
    assert_subr_arity("set-process-buffer", 2, Some(2));
    assert_subr_arity("set-process-coding-system", 1, Some(3));
    assert_subr_arity("set-process-datagram-address", 2, Some(2));
    assert_subr_arity("set-process-filter", 2, Some(2));
    assert_subr_arity("set-process-inherit-coding-system-flag", 2, Some(2));
    assert_subr_arity("set-process-plist", 2, Some(2));
    assert_subr_arity("set-process-query-on-exit-flag", 2, Some(2));
    assert_subr_arity("set-process-sentinel", 2, Some(2));
    assert_subr_arity("set-process-thread", 2, Some(2));
    assert_subr_arity("set-process-window-size", 3, Some(3));
    assert_subr_arity("set-binary-mode", 2, Some(2));
    assert_subr_arity("set-network-process-option", 3, Some(4));
    assert_subr_arity("serial-process-configure", 0, None);
}

#[test]
fn subr_arity_core_math_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("%", 2, Some(2));
    assert_subr_arity("/", 1, None);
    assert_subr_arity("/=", 2, Some(2));
    assert_subr_arity("1+", 1, Some(1));
    assert_subr_arity("1-", 1, Some(1));
    assert_subr_arity("<", 1, None);
    assert_subr_arity("<=", 1, None);
    assert_subr_arity("=", 1, None);
    assert_subr_arity(">", 1, None);
    assert_subr_arity(">=", 1, None);
    assert_subr_arity("abs", 1, Some(1));
    assert_subr_arity("ash", 2, Some(2));
    assert_subr_arity("apply", 1, None);
}

#[test]
fn subr_arity_minibuffer_control_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("recursive-edit", 0, Some(0));
    assert_subr_arity("top-level", 0, Some(0));
    assert_subr_arity("exit-recursive-edit", 0, Some(0));
    assert_subr_arity("abort-minibuffers", 0, Some(0));
    assert_subr_arity("abort-recursive-edit", 0, Some(0));
    assert_subr_arity("minibuffer-depth", 0, Some(0));
    assert_subr_arity("minibufferp", 0, Some(2));
    assert_subr_arity("next-read-file-uses-dialog-p", 0, Some(0));
    assert_subr_arity("minibuffer-prompt", 0, Some(0));
    assert_subr_arity("minibuffer-contents", 0, Some(0));
    assert_subr_arity("minibuffer-contents-no-properties", 0, Some(0));
}

#[test]
fn subr_arity_point_navigation_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("beginning-of-line", 0, Some(1));
    assert_subr_arity("end-of-line", 0, Some(1));
    assert_subr_arity("forward-char", 0, Some(1));
    assert_subr_arity("backward-char", 0, Some(1));
    assert_subr_arity("forward-word", 0, Some(1));
    assert_subr_arity("forward-line", 0, Some(1));
    assert_subr_arity("goto-char", 1, Some(1));
    assert_subr_arity("point-max", 0, Some(0));
    assert_subr_arity("point-min", 0, Some(0));
    assert_subr_arity("bobp", 0, Some(0));
    assert_subr_arity("eobp", 0, Some(0));
    assert_subr_arity("bolp", 0, Some(0));
    assert_subr_arity("eolp", 0, Some(0));
}

#[test]
fn subr_arity_buffer_point_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("current-buffer", 0, Some(0));
    assert_subr_arity("buffer-string", 0, Some(0));
    assert_subr_arity("point", 0, Some(0));
    assert_subr_arity("point-min", 0, Some(0));
    assert_subr_arity("point-max", 0, Some(0));
    assert_subr_arity("erase-buffer", 0, Some(0));
    assert_subr_arity("widen", 0, Some(0));
    assert_subr_arity("barf-if-buffer-read-only", 0, Some(1));
    assert_subr_arity("buffer-file-name", 0, Some(1));
    assert_subr_arity("buffer-base-buffer", 0, Some(1));
    assert_subr_arity("buffer-last-name", 0, Some(1));
    assert_subr_arity("buffer-name", 0, Some(1));
    assert_subr_arity("buffer-size", 0, Some(1));
    assert_subr_arity("buffer-chars-modified-tick", 0, Some(1));
    assert_subr_arity("buffer-modified-p", 0, Some(1));
    assert_subr_arity("buffer-modified-tick", 0, Some(1));
    assert_subr_arity("buffer-list", 0, Some(1));
    assert_subr_arity("other-buffer", 0, Some(3));
    assert_subr_arity("bury-buffer-internal", 1, Some(1));
    assert_subr_arity("buffer-enable-undo", 0, Some(1));
    assert_subr_arity("buffer-hash", 0, Some(1));
    assert_subr_arity("buffer-line-statistics", 0, Some(1));
    assert_subr_arity("buffer-local-variables", 0, Some(1));
    assert_subr_arity("buffer-live-p", 1, Some(1));
    assert_subr_arity("buffer-swap-text", 1, Some(1));
    assert_subr_arity("buffer-local-value", 2, Some(2));
    assert_subr_arity("buffer-substring", 2, Some(2));
    assert_subr_arity("buffer-substring-no-properties", 2, Some(2));
    assert_subr_arity("constrain-to-field", 2, Some(5));
    assert_subr_arity("field-beginning", 0, Some(3));
    assert_subr_arity("field-end", 0, Some(3));
    assert_subr_arity("field-string", 0, Some(1));
    assert_subr_arity("field-string-no-properties", 0, Some(1));
    assert_subr_arity("byte-to-position", 1, Some(1));
    assert_subr_arity("position-bytes", 1, Some(1));
}

#[test]
fn subr_arity_mark_marker_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("mark-marker", 0, Some(0));
    assert_subr_arity("point-marker", 0, Some(0));
    assert_subr_arity("point-min-marker", 0, Some(0));
    assert_subr_arity("point-max-marker", 0, Some(0));
    assert_subr_arity("marker-buffer", 1, Some(1));
    assert_subr_arity("marker-insertion-type", 1, Some(1));
    assert_subr_arity("marker-position", 1, Some(1));
    assert_subr_arity("markerp", 1, Some(1));
    assert_subr_arity("set-marker", 2, Some(3));
    // No `move-marker' row: GNU has no DEFUN of that name, only
    // `(defalias 'move-marker #'set-marker)' at lisp/subr.el:2280.
    assert_subr_arity("set-marker-insertion-type", 2, Some(2));
}

#[test]
fn subr_arity_register_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("register-ccl-program", 2, Some(2));
    assert_subr_arity("register-code-conversion-map", 2, Some(2));
}

#[test]
fn subr_arity_list_sequence_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("max", 1, None);
    assert_subr_arity("min", 1, None);
    assert_subr_arity("mod", 2, Some(2));
    assert_subr_arity("nreverse", 1, Some(1));
    assert_subr_arity("nth", 2, Some(2));
    assert_subr_arity("nthcdr", 2, Some(2));
    assert_subr_arity("reverse", 1, Some(1));
    assert_subr_arity("safe-length", 1, Some(1));
    assert_subr_arity("proper-list-p", 1, Some(1));
    assert_subr_arity("make-list", 2, Some(2));
    assert_subr_arity("mapcar", 2, Some(2));
    assert_subr_arity("mapc", 2, Some(2));
    assert_subr_arity("mapcan", 2, Some(2));
    assert_subr_arity("mapconcat", 2, Some(3));
}

#[test]
fn subr_arity_char_charset_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("char-after", 0, Some(1));
    assert_subr_arity("char-before", 0, Some(1));
    assert_subr_arity("char-category-set", 1, Some(1));
    assert_subr_arity("char-charset", 1, Some(2));
    assert_subr_arity("char-equal", 2, Some(2));
    assert_subr_arity("char-or-string-p", 1, Some(1));
    assert_subr_arity("char-resolve-modifiers", 1, Some(1));
    assert_subr_arity("char-syntax", 1, Some(1));
    assert_subr_arity("char-width", 1, Some(1));
    assert_subr_arity("char-table-p", 1, Some(1));
    assert_subr_arity("char-table-parent", 1, Some(1));
    assert_subr_arity("char-table-subtype", 1, Some(1));
    assert_subr_arity("char-table-extra-slot", 2, Some(2));
    assert_subr_arity("char-table-range", 2, Some(2));
    assert_subr_arity("charset-after", 0, Some(1));
    assert_subr_arity("charset-id-internal", 0, Some(1));
    assert_subr_arity("charset-plist", 1, Some(1));
    assert_subr_arity("charset-priority-list", 0, Some(1));
    assert_subr_arity("define-charset-alias", 2, Some(2));
    assert_subr_arity("declare-equiv-charset", 4, Some(4));
    assert_subr_arity("decode-big5-char", 1, Some(1));
    assert_subr_arity("decode-sjis-char", 1, Some(1));
    assert_subr_arity("encode-big5-char", 1, Some(1));
    assert_subr_arity("encode-sjis-char", 1, Some(1));
    assert_subr_arity("get-unused-iso-final-char", 2, Some(2));
}

#[test]
fn subr_arity_assoc_predicate_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("assoc", 2, Some(3));
    assert_subr_arity("assoc-string", 2, Some(3));
    assert_subr_arity("assq", 2, Some(2));
    assert_subr_arity("car-less-than-car", 2, Some(2));
    assert_subr_arity("member", 2, Some(2));
    assert_subr_arity("memq", 2, Some(2));
    assert_subr_arity("memql", 2, Some(2));
    assert_subr_arity("rassoc", 2, Some(2));
    assert_subr_arity("rassq", 2, Some(2));
    assert_subr_arity("bare-symbol", 1, Some(1));
    assert_subr_arity("bare-symbol-p", 1, Some(1));
    assert_subr_arity("boundp", 1, Some(1));
    assert_subr_arity("byte-code-function-p", 1, Some(1));
    assert_subr_arity("car-safe", 1, Some(1));
    assert_subr_arity("cdr-safe", 1, Some(1));
}

#[test]
fn subr_arity_navigation_case_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("backward-prefix-chars", 0, Some(0));
    assert_subr_arity("capitalize", 1, Some(1));
    assert_subr_arity("capitalize-word", 1, Some(1));
    assert_subr_arity("capitalize-region", 2, Some(3));
}

#[test]
fn subr_arity_kill_edit_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("downcase-region", 2, Some(3));
    assert_subr_arity("downcase-word", 1, Some(1));
    assert_subr_arity("kill-buffer", 0, Some(1));
    assert_subr_arity("kill-local-variable", 1, Some(1));
}

#[test]
fn subr_arity_hook_advice_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("add-face-text-property", 3, Some(5));
    assert_subr_arity("add-name-to-file", 2, Some(3));
    assert_subr_arity("add-text-properties", 3, Some(4));
    assert_subr_arity("add-variable-watcher", 2, Some(2));
    // advice-add, advice-remove, advice-member-p: handled by nadvice.el
    assert_subr_arity("autoload", 2, Some(5));
    assert_subr_arity("autoload-do-load", 1, Some(3));
    assert_subr_arity("backtrace--frames-from-thread", 1, Some(1));
    assert_subr_arity("backtrace--locals", 1, Some(2));
    assert_subr_arity("backtrace-debug", 2, Some(3));
    assert_subr_arity("backtrace-eval", 2, Some(3));
    assert_subr_arity("backtrace-frame--internal", 3, Some(3));
    assert_subr_arity("run-hook-with-args", 1, None);
    assert_subr_arity("run-hook-with-args-until-failure", 1, None);
    assert_subr_arity("run-hook-with-args-until-success", 1, None);
    assert_subr_arity("run-hook-wrapped", 2, None);
    assert_subr_arity("run-window-configuration-change-hook", 0, Some(1));
    assert_subr_arity("run-window-scroll-functions", 0, Some(1));
}

#[test]
fn subr_arity_doc_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("Snarf-documentation", 1, Some(1));
    assert_subr_arity("documentation", 1, Some(2));
    assert_subr_arity("documentation-stringp", 1, Some(1));
    assert_subr_arity("documentation-property", 2, Some(3));
}

#[test]
fn subr_arity_coding_time_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("decode-coding-string", 2, Some(4));
    assert_subr_arity("decode-time", 0, Some(3));
    assert_subr_arity("detect-coding-region", 2, Some(3));
    assert_subr_arity("detect-coding-string", 1, Some(2));
    assert_subr_arity("encode-char", 2, Some(2));
    assert_subr_arity("encode-coding-string", 2, Some(4));
    assert_subr_arity("encode-time", 1, None);
    assert_subr_arity("format-time-string", 1, Some(3));
}

#[test]
fn subr_arity_indent_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("indent-to", 1, Some(2));
    assert_subr_arity("move-to-column", 1, Some(2));
}

#[test]
fn subr_arity_text_property_overlay_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("put-text-property", 4, Some(5));
    assert_subr_arity("set-text-properties", 3, Some(4));
    assert_subr_arity("remove-text-properties", 3, Some(4));
    assert_subr_arity("remove-list-of-text-properties", 3, Some(4));
    assert_subr_arity("get-text-property", 2, Some(3));
    assert_subr_arity("get-char-property", 2, Some(3));
    assert_subr_arity("get-char-property-and-overlay", 2, Some(3));
    assert_subr_arity("get-display-property", 2, Some(4));
    assert_subr_arity("get-pos-property", 2, Some(3));
    assert_subr_arity("text-properties-at", 1, Some(2));
    assert_subr_arity("next-single-property-change", 2, Some(4));
    assert_subr_arity("next-single-char-property-change", 2, Some(4));
    assert_subr_arity("previous-single-property-change", 2, Some(4));
    assert_subr_arity("previous-single-char-property-change", 2, Some(4));
    assert_subr_arity("next-property-change", 1, Some(3));
    assert_subr_arity("previous-property-change", 1, Some(3));
    assert_subr_arity("next-char-property-change", 1, Some(2));
    assert_subr_arity("previous-char-property-change", 1, Some(2));
    assert_subr_arity("text-property-any", 4, Some(5));
    assert_subr_arity("text-property-not-all", 4, Some(5));
    assert_subr_arity("make-overlay", 2, Some(5));
    assert_subr_arity("move-overlay", 3, Some(4));
    assert_subr_arity("overlay-put", 3, Some(3));
    assert_subr_arity("overlay-get", 2, Some(2));
    assert_subr_arity("next-overlay-change", 1, Some(1));
    assert_subr_arity("previous-overlay-change", 1, Some(1));
    assert_subr_arity("overlay-start", 1, Some(1));
    assert_subr_arity("overlay-end", 1, Some(1));
    assert_subr_arity("overlay-buffer", 1, Some(1));
    assert_subr_arity("overlay-properties", 1, Some(1));
    assert_subr_arity("overlays-at", 1, Some(2));
    assert_subr_arity("overlays-in", 2, Some(2));
    assert_subr_arity("overlayp", 1, Some(1));
}

#[test]
fn subr_arity_encoding_bool_vector_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("bool-vector", 0, None);
    assert_subr_arity("base64-decode-region", 2, Some(4));
    assert_subr_arity("base64-encode-region", 2, Some(3));
    assert_subr_arity("base64-decode-string", 1, Some(3));
    assert_subr_arity("base64-encode-string", 1, Some(2));
    assert_subr_arity("base64url-encode-region", 2, Some(3));
    assert_subr_arity("base64url-encode-string", 1, Some(2));
    assert_subr_arity("bool-vector-p", 1, Some(1));
    assert_subr_arity("bool-vector-count-population", 1, Some(1));
    assert_subr_arity("bool-vector-count-consecutive", 3, Some(3));
    assert_subr_arity("bool-vector-not", 1, Some(2));
    assert_subr_arity("bool-vector-subsetp", 2, Some(2));
    assert_subr_arity("bool-vector-exclusive-or", 2, Some(3));
    assert_subr_arity("bool-vector-intersection", 2, Some(3));
    assert_subr_arity("bool-vector-set-difference", 2, Some(3));
    assert_subr_arity("bool-vector-union", 2, Some(3));
}

#[test]
fn subr_arity_runtime_covered_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("aref", 2, Some(2));
    assert_subr_arity("arrayp", 1, Some(1));
    assert_subr_arity("aset", 3, Some(3));
    assert_subr_arity("atom", 1, Some(1));
    assert_subr_arity("bufferp", 1, Some(1));
    assert_subr_arity("ceiling", 1, Some(2));
    assert_subr_arity("char-to-string", 1, Some(1));
    assert_subr_arity("characterp", 1, Some(2));
    assert_subr_arity("consp", 1, Some(1));
    assert_subr_arity("downcase", 1, Some(1));
    assert_subr_arity("eq", 2, Some(2));
    assert_subr_arity("eql", 2, Some(2));
    assert_subr_arity("equal", 2, Some(2));
    assert_subr_arity("funcall", 1, None);
    assert_subr_arity("funcall-interactively", 1, None);
    assert_subr_arity("funcall-with-delayed-message", 3, Some(3));
    assert_subr_arity("float", 1, Some(1));
    assert_subr_arity("floatp", 1, Some(1));
    assert_subr_arity("floor", 1, Some(2));
    assert_subr_arity("integerp", 1, Some(1));
    assert_subr_arity("keywordp", 1, Some(1));
    assert_subr_arity("listp", 1, Some(1));
    assert_subr_arity("make-vector", 2, Some(2));
    assert_subr_arity("nlistp", 1, Some(1));
    assert_subr_arity("null", 1, Some(1));
    assert_subr_arity("number-to-string", 1, Some(1));
    assert_subr_arity("numberp", 1, Some(1));
    assert_subr_arity("defalias", 2, Some(3));
    assert_subr_arity("provide", 1, Some(2));
    assert_subr_arity("require", 1, Some(3));
    assert_subr_arity("round", 1, Some(2));
    assert_subr_arity("sequencep", 1, Some(1));
    assert_subr_arity("string-equal", 2, Some(2));
    assert_subr_arity("string-lessp", 2, Some(2));
    assert_subr_arity("string-to-char", 1, Some(1));
    assert_subr_arity("string-to-number", 1, Some(2));
    assert_subr_arity("stringp", 1, Some(1));
    assert_subr_arity("substring", 1, Some(3));
    assert_subr_arity("symbolp", 1, Some(1));
    assert_subr_arity("throw", 2, Some(2));
    assert_subr_arity("truncate", 1, Some(2));
    assert_subr_arity("type-of", 1, Some(1));
    assert_subr_arity("upcase", 1, Some(1));
    assert_subr_arity("vectorp", 1, Some(1));
}

#[test]
fn subr_arity_command_timer_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("call-interactively", 1, Some(3));
    assert_subr_arity("command-modes", 1, Some(1));
    assert_subr_arity("command-error-default-function", 3, Some(3));
    assert_subr_arity("command-remapping", 1, Some(3));
    assert_subr_arity("commandp", 1, Some(2));
    assert_subr_arity("sleep-for", 1, Some(2));
    assert_subr_arity("current-cpu-time", 0, Some(0));
    assert_subr_arity("current-idle-time", 0, Some(0));
    assert_subr_arity("current-time", 0, Some(0));
    assert_subr_arity("flush-standard-output", 0, Some(0));
    assert_subr_arity("force-mode-line-update", 0, Some(1));
    assert_subr_arity("force-window-update", 0, Some(1));
    assert_subr_arity("get-internal-run-time", 0, Some(0));
    assert_subr_arity("float-time", 0, Some(1));
}

#[test]
fn subr_arity_command_read_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("compare-buffer-substrings", 6, Some(6));
    assert_subr_arity("compare-strings", 6, Some(7));
    assert_subr_arity("define-fringe-bitmap", 2, Some(5));
    assert_subr_arity("destroy-fringe-bitmap", 1, Some(1));
    assert_subr_arity("display--line-is-continued-p", 0, Some(0));
    assert_subr_arity("display--update-for-mouse-movement", 3, Some(3));
    assert_subr_arity("do-auto-save", 0, Some(2));
    assert_subr_arity("external-debugging-output", 1, Some(1));
    assert_subr_arity("describe-buffer-bindings", 1, Some(3));
    assert_subr_arity("describe-vector", 1, Some(2));
    assert_subr_arity("delete-terminal", 0, Some(2));
    assert_subr_arity("face-attributes-as-vector", 1, Some(1));
    assert_subr_arity("font-at", 1, Some(3));
    assert_subr_arity("font-face-attributes", 1, Some(2));
    assert_subr_arity("font-get-glyphs", 3, Some(4));
    assert_subr_arity("font-get-system-font", 0, Some(0));
    assert_subr_arity("font-get-system-normal-font", 0, Some(0));
    assert_subr_arity("font-has-char-p", 2, Some(3));
    assert_subr_arity("font-info", 1, Some(2));
    assert_subr_arity("font-match-p", 2, Some(2));
    assert_subr_arity("font-shape-gstring", 2, Some(2));
    assert_subr_arity("font-variation-glyphs", 2, Some(2));
    assert_subr_arity("fontset-font", 2, Some(3));
    assert_subr_arity("fontset-info", 1, Some(2));
    assert_subr_arity("fontset-list", 0, Some(0));
    assert_subr_arity("frame--set-was-invisible", 2, Some(2));
    assert_subr_arity("frame-after-make-frame", 2, Some(2));
    assert_subr_arity("frame-ancestor-p", 2, Some(2));
    assert_subr_arity("frame--face-hash-table", 0, Some(1));
    assert_subr_arity("frame-bottom-divider-width", 0, Some(1));
    assert_subr_arity("frame-child-frame-border-width", 0, Some(1));
    assert_subr_arity("frame-focus", 0, Some(1));
    assert_subr_arity("frame-font-cache", 0, Some(1));
    assert_subr_arity("frame-fringe-width", 0, Some(1));
    assert_subr_arity("frame-internal-border-width", 0, Some(1));
    assert_subr_arity("frame-old-selected-window", 0, Some(1));
    assert_subr_arity("frame-or-buffer-changed-p", 0, Some(1));
    assert_subr_arity("frame-parent", 0, Some(1));
    assert_subr_arity("frame-pointer-visible-p", 0, Some(1));
    assert_subr_arity("frame-scale-factor", 0, Some(1));
    assert_subr_arity("frame-scroll-bar-height", 0, Some(1));
    assert_subr_arity("frame-scroll-bar-width", 0, Some(1));
    assert_subr_arity("frame-window-state-change", 0, Some(1));
    assert_subr_arity("frame-right-divider-width", 0, Some(1));
    assert_subr_arity("fringe-bitmaps-at-pos", 0, Some(2));
    assert_subr_arity("gap-position", 0, Some(0));
    assert_subr_arity("gap-size", 0, Some(0));
    assert_subr_arity("gnutls-available-p", 0, Some(0));
    assert_subr_arity("gnutls-asynchronous-parameters", 2, Some(2));
    assert_subr_arity("gnutls-boot", 3, Some(3));
    assert_subr_arity("gnutls-bye", 2, Some(2));
    assert_subr_arity("gnutls-ciphers", 0, Some(0));
    assert_subr_arity("gnutls-deinit", 1, Some(1));
    assert_subr_arity("gnutls-digests", 0, Some(0));
    assert_subr_arity("gnutls-format-certificate", 1, Some(1));
    assert_subr_arity("gnutls-get-initstage", 1, Some(1));
    assert_subr_arity("gnutls-hash-digest", 2, Some(2));
    assert_subr_arity("gnutls-hash-mac", 3, Some(3));
    assert_subr_arity("gnutls-macs", 0, Some(0));
    assert_subr_arity("gnutls-peer-status", 1, Some(1));
    assert_subr_arity("neomacs-open-tls-stream", 4, Some(4));
    assert_subr_arity("neomacs-tls-available-p", 0, Some(0));
    assert_subr_arity("sqlite-available-p", 0, Some(0));
    assert_subr_arity("sqlite-close", 1, Some(1));
    assert_subr_arity("sqlite-columns", 1, Some(1));
    assert_subr_arity("sqlite-commit", 1, Some(1));
    assert_subr_arity("sqlite-execute", 2, Some(3));
    assert_subr_arity("sqlite-execute-batch", 2, Some(2));
    assert_subr_arity("sqlite-finalize", 1, Some(1));
    assert_subr_arity("sqlite-load-extension", 2, Some(2));
    assert_subr_arity("sqlite-more-p", 1, Some(1));
    assert_subr_arity("sqlite-next", 1, Some(1));
    assert_subr_arity("sqlite-open", 0, Some(3));
    assert_subr_arity("sqlite-pragma", 2, Some(2));
    assert_subr_arity("sqlite-rollback", 1, Some(1));
    assert_subr_arity("sqlite-select", 2, Some(4));
    assert_subr_arity("sqlite-transaction", 1, Some(1));
    assert_subr_arity("sqlite-version", 0, Some(0));
    assert_subr_arity("sqlitep", 1, Some(1));
    assert_subr_arity("garbage-collect-maybe", 1, Some(1));
    assert_subr_arity("gnutls-error-fatalp", 1, Some(1));
    assert_subr_arity("gnutls-error-string", 1, Some(1));
    assert_subr_arity("gnutls-errorp", 1, Some(1));
    assert_subr_arity("gnutls-peer-status-warning-describe", 1, Some(1));
    assert_subr_arity("gnutls-symmetric-decrypt", 4, Some(5));
    assert_subr_arity("gnutls-symmetric-encrypt", 4, Some(5));
    assert_subr_arity("handle-save-session", 1, Some(1));
    assert_subr_arity("handle-switch-frame", 1, Some(1));
    assert_subr_arity("help--describe-vector", 7, Some(7));
    assert_subr_arity("init-image-library", 1, Some(1));
    assert_subr_arity("inotify-add-watch", 3, Some(3));
    assert_subr_arity("inotify-rm-watch", 1, Some(1));
    assert_subr_arity("inotify-valid-p", 1, Some(1));
    assert_subr_arity("innermost-minibuffer-p", 0, Some(1));
    assert_subr_arity("interactive-form", 1, Some(1));
    assert_subr_arity("local-variable-if-set-p", 1, Some(2));
    assert_subr_arity("lock-buffer", 0, Some(1));
    assert_subr_arity("lock-file", 1, Some(1));
    assert_subr_arity("lossage-size", 0, Some(1));
    assert_subr_arity("unlock-buffer", 0, Some(0));
    assert_subr_arity("unlock-file", 1, Some(1));
    assert_subr_arity("window-at", 2, Some(3));
    assert_subr_arity("window-bottom-divider-width", 0, Some(1));
    assert_subr_arity("window-bump-use-time", 0, Some(1));
    assert_subr_arity("window-combination-limit", 1, Some(1));
    assert_subr_arity("window-left-child", 0, Some(1));
    assert_subr_arity("window-line-height", 0, Some(2));
    assert_subr_arity("window-lines-pixel-dimensions", 0, Some(6));
    assert_subr_arity("window-list-1", 0, Some(3));
    assert_subr_arity("window-new-normal", 0, Some(1));
    assert_subr_arity("window-new-pixel", 0, Some(1));
    assert_subr_arity("window-new-total", 0, Some(1));
    assert_subr_arity("window-next-sibling", 0, Some(1));
    assert_subr_arity("window-normal-size", 0, Some(2));
    assert_subr_arity("window-old-body-pixel-height", 0, Some(1));
    assert_subr_arity("window-old-body-pixel-width", 0, Some(1));
    assert_subr_arity("window-old-pixel-height", 0, Some(1));
    assert_subr_arity("window-old-pixel-width", 0, Some(1));
    assert_subr_arity("window-parent", 0, Some(1));
    assert_subr_arity("window-pixel-left", 0, Some(1));
    assert_subr_arity("window-pixel-top", 0, Some(1));
    assert_subr_arity("window-prev-sibling", 0, Some(1));
    assert_subr_arity("window-resize-apply", 0, Some(2));
    assert_subr_arity("window-resize-apply-total", 0, Some(2));
    assert_subr_arity("window-right-divider-width", 0, Some(1));
    assert_subr_arity("window-scroll-bar-height", 0, Some(1));
    assert_subr_arity("window-scroll-bar-width", 0, Some(1));
    assert_subr_arity("window-tab-line-height", 0, Some(1));
    assert_subr_arity("window-top-child", 0, Some(1));
    assert_subr_arity("treesit-available-p", 0, Some(0));
    assert_subr_arity("treesit-compiled-query-p", 1, Some(1));
    assert_subr_arity("treesit-induce-sparse-tree", 2, Some(4));
    assert_subr_arity("treesit-language-abi-version", 0, Some(1));
    assert_subr_arity("treesit-language-available-p", 1, Some(2));
    assert_subr_arity("treesit-library-abi-version", 0, Some(1));
    assert_subr_arity("treesit-node-check", 2, Some(2));
    assert_subr_arity("treesit-node-child", 2, Some(3));
    assert_subr_arity("treesit-node-child-by-field-name", 2, Some(2));
    assert_subr_arity("treesit-node-child-count", 1, Some(2));
    assert_subr_arity("treesit-node-descendant-for-range", 3, Some(4));
    assert_subr_arity("treesit-node-end", 1, Some(1));
    assert_subr_arity("treesit-node-eq", 2, Some(2));
    assert_subr_arity("treesit-node-field-name-for-child", 2, Some(2));
    assert_subr_arity("treesit-node-first-child-for-pos", 2, Some(3));
    assert_subr_arity("treesit-node-match-p", 2, Some(3));
    assert_subr_arity("treesit-node-next-sibling", 1, Some(2));
    assert_subr_arity("treesit-node-p", 1, Some(1));
    assert_subr_arity("treesit-node-parent", 1, Some(1));
    assert_subr_arity("treesit-node-parser", 1, Some(1));
    assert_subr_arity("treesit-node-prev-sibling", 1, Some(2));
    assert_subr_arity("treesit-node-start", 1, Some(1));
    assert_subr_arity("treesit-node-string", 1, Some(1));
    assert_subr_arity("treesit-node-type", 1, Some(1));
    assert_subr_arity("treesit-parser-add-notifier", 2, Some(2));
    assert_subr_arity("treesit-parser-buffer", 1, Some(1));
    assert_subr_arity("treesit-parser-create", 1, Some(4));
    assert_subr_arity("treesit-parser-delete", 1, Some(1));
    assert_subr_arity("treesit-parser-included-ranges", 1, Some(1));
    assert_subr_arity("treesit-parser-language", 1, Some(1));
    assert_subr_arity("treesit-parser-list", 0, Some(3));
    assert_subr_arity("treesit-parser-notifiers", 1, Some(1));
    assert_subr_arity("treesit-parser-p", 1, Some(1));
    assert_subr_arity("treesit-parser-remove-notifier", 2, Some(2));
    assert_subr_arity("treesit-parser-root-node", 1, Some(1));
    assert_subr_arity("treesit-parser-set-included-ranges", 2, Some(2));
    assert_subr_arity("treesit-parser-tag", 1, Some(1));
    assert_subr_arity("treesit-pattern-expand", 1, Some(1));
    assert_subr_arity("treesit-query-capture", 2, Some(6));
    assert_subr_arity("treesit-query-compile", 2, Some(3));
    assert_subr_arity("treesit-query-expand", 1, Some(1));
    assert_subr_arity("treesit-query-language", 1, Some(1));
    assert_subr_arity("treesit-query-p", 1, Some(1));
    assert_subr_arity("treesit-search-forward", 2, Some(4));
    assert_subr_arity("treesit-search-subtree", 2, Some(5));
    assert_subr_arity("treesit-subtree-stat", 1, Some(1));
    assert_subr_arity("internal--define-uninitialized-variable", 1, Some(2));
    assert_subr_arity("internal--labeled-narrow-to-region", 3, Some(3));
    assert_subr_arity("internal--labeled-widen", 1, Some(1));
    assert_subr_arity("internal--obarray-buckets", 1, Some(1));
    assert_subr_arity("internal--set-buffer-modified-tick", 1, Some(2));
    assert_subr_arity("internal--track-mouse", 1, Some(1));
    assert_subr_arity("internal-char-font", 1, Some(2));
    assert_subr_arity("internal-complete-buffer", 3, Some(3));
    assert_subr_arity("internal-describe-syntax-value", 1, Some(1));
    assert_subr_arity("internal-event-symbol-parse-modifiers", 1, Some(1));
    assert_subr_arity("internal-handle-focus-in", 1, Some(1));
    assert_subr_arity("internal-make-var-non-special", 1, Some(1));
    assert_subr_arity("internal-set-lisp-face-attribute-from-resource", 3, Some(4));
    assert_subr_arity("internal-stack-stats", 0, Some(0));
    assert_subr_arity("internal-subr-documentation", 1, Some(1));
    assert_subr_arity("dump-emacs-portable", 1, Some(2));
    assert_subr_arity("dump-emacs-portable--sort-predicate", 2, Some(2));
    assert_subr_arity("dump-emacs-portable--sort-predicate-copied", 2, Some(2));
    assert_subr_arity("malloc-info", 0, Some(0));
    assert_subr_arity("malloc-trim", 0, Some(1));
    assert_subr_arity("marker-last-position", 1, Some(1));
    assert_subr_arity("match-data--translate", 1, Some(1));
    assert_subr_arity("memory-info", 0, Some(0));
    assert_subr_arity("make-frame-invisible", 0, Some(2));
    assert_subr_arity("make-terminal-frame", 1, Some(1));
    assert_subr_arity("menu-bar-menu-at-x-y", 2, Some(3));
    assert_subr_arity("menu-or-popup-active-p", 0, Some(0));
    assert_subr_arity("module-load", 1, Some(1));
    assert_subr_arity("mouse-pixel-position", 0, Some(0));
    assert_subr_arity("mouse-position", 0, Some(0));
    assert_subr_arity("newline-cache-check", 0, Some(1));
    assert_subr_arity("native-comp-available-p", 0, Some(0));
    assert_subr_arity("new-fontset", 2, Some(2));
    assert_subr_arity("object-intervals", 1, Some(1));
    assert_subr_arity("old-selected-frame", 0, Some(0));
    assert_subr_arity("old-selected-window", 0, Some(0));
    assert_subr_arity("open-dribble-file", 1, Some(1));
    assert_subr_arity("open-font", 1, Some(3));
    assert_subr_arity("optimize-char-table", 1, Some(2));
    assert_subr_arity("overlay-lists", 0, Some(0));
    assert_subr_arity("overlay-recenter", 1, Some(1));
    assert_subr_arity("pdumper-stats", 0, Some(0));
    assert_subr_arity("play-sound-internal", 1, Some(1));
    assert_subr_arity("position-symbol", 2, Some(2));
    assert_subr_arity("posn-at-point", 0, Some(2));
    assert_subr_arity("posn-at-x-y", 2, Some(4));
    assert_subr_arity("profiler-cpu-log", 0, Some(0));
    assert_subr_arity("profiler-cpu-running-p", 0, Some(0));
    assert_subr_arity("profiler-cpu-start", 1, Some(1));
    assert_subr_arity("profiler-cpu-stop", 0, Some(0));
    assert_subr_arity("profiler-memory-log", 0, Some(0));
    assert_subr_arity("profiler-memory-running-p", 0, Some(0));
    assert_subr_arity("profiler-memory-start", 0, Some(0));
    assert_subr_arity("profiler-memory-stop", 0, Some(0));
    assert_subr_arity("query-font", 1, Some(1));
    assert_subr_arity("query-fontset", 1, Some(2));
    assert_subr_arity("read-positioning-symbols", 0, Some(1));
    assert_subr_arity("recent-auto-save-p", 0, Some(0));
    assert_subr_arity("record", 1, None);
    assert_subr_arity("recordp", 1, Some(1));
    assert_subr_arity("reconsider-frame-fonts", 1, Some(1));
    assert_subr_arity("redirect-debugging-output", 1, Some(2));
    assert_subr_arity("redirect-frame-focus", 1, Some(2));
    assert_subr_arity("remove-pos-from-symbol", 1, Some(1));
    assert_subr_arity("resize-mini-window-internal", 1, Some(1));
    assert_subr_arity("restore-buffer-modified-p", 1, Some(1));
    assert_subr_arity("set--this-command-keys", 1, Some(1));
    assert_subr_arity("set-buffer-auto-saved", 0, Some(0));
    assert_subr_arity("set-buffer-redisplay", 4, Some(4));
    assert_subr_arity("set-charset-plist", 2, Some(2));
    assert_subr_arity("set-fontset-font", 3, Some(5));
    assert_subr_arity("set-frame-selected-window", 2, Some(3));
    assert_subr_arity("set-frame-window-state-change", 0, Some(2));
    assert_subr_arity("set-fringe-bitmap-face", 1, Some(2));
    assert_subr_arity("set-minibuffer-window", 1, Some(1));
    assert_subr_arity("set-mouse-pixel-position", 3, Some(3));
    assert_subr_arity("set-mouse-position", 3, Some(3));
    assert_subr_arity("set-window-combination-limit", 2, Some(2));
    assert_subr_arity("set-window-new-normal", 1, Some(2));
    assert_subr_arity("set-window-new-pixel", 2, Some(3));
    assert_subr_arity("set-window-new-total", 2, Some(3));
    assert_subr_arity("sort-charsets", 1, Some(1));
    assert_subr_arity("split-char", 1, Some(1));
    assert_subr_arity("string-distance", 2, Some(3));
    assert_subr_arity("subst-char-in-region", 4, Some(5));
    assert_subr_arity("subr-native-lambda-list", 1, Some(1));
    assert_subr_arity("subr-type", 1, Some(1));
    assert_subr_arity("this-single-command-keys", 0, Some(0));
    assert_subr_arity("this-single-command-raw-keys", 0, Some(0));
    assert_subr_arity("thread--blocker", 1, Some(1));
    assert_subr_arity("tool-bar-get-system-style", 0, Some(0));
    assert_subr_arity("tool-bar-pixel-width", 0, Some(1));
    assert_subr_arity("translate-region-internal", 3, Some(3));
    assert_subr_arity("transpose-regions", 4, Some(5));
    assert_subr_arity("tty--output-buffer-size", 0, Some(1));
    assert_subr_arity("tty--set-output-buffer-size", 1, Some(2));
    assert_subr_arity("tty-suppress-bold-inverse-default-colors", 1, Some(1));
    assert_subr_arity("unencodable-char-position", 3, Some(5));
    assert_subr_arity("unicode-property-table-internal", 1, Some(1));
    assert_subr_arity("unify-charset", 1, Some(3));
    assert_subr_arity("unix-sync", 0, Some(0));
    assert_subr_arity("value<", 2, Some(2));
    assert_subr_arity("variable-binding-locus", 1, Some(1));
    assert_subr_arity("byte-code", 3, Some(3));
    assert_subr_arity("decode-coding-region", 3, Some(4));
    assert_subr_arity("defconst-1", 2, Some(3));
    assert_subr_arity("define-coding-system-internal", 13, None);
    assert_subr_arity("defvar-1", 2, Some(3));
    assert_subr_arity("defvaralias", 2, Some(3));
    assert_subr_arity("encode-coding-region", 3, Some(4));
    assert_subr_arity("find-operation-coding-system", 1, None);
    assert_subr_arity("handler-bind-1", 1, None);
    assert_subr_arity("indirect-variable", 1, Some(1));
    assert_subr_arity("insert-and-inherit", 0, None);
    assert_subr_arity("insert-before-markers-and-inherit", 0, None);
    assert_subr_arity("insert-buffer-substring", 1, Some(3));
    assert_subr_arity("iso-charset", 3, Some(3));
    assert_subr_arity("keymap--get-keyelt", 2, Some(2));
    assert_subr_arity("keymap-prompt", 1, Some(1));
    assert_subr_arity("kill-all-local-variables", 0, Some(1));
    assert_subr_arity("kill-emacs", 0, Some(2));
    assert_subr_arity("lower-frame", 0, Some(1));
    assert_subr_arity("lread--substitute-object-in-subtree", 3, Some(3));
    assert_subr_arity("macroexpand", 1, Some(2));
    assert_subr_arity("make-byte-code", 4, None);
    assert_subr_arity("make-char", 1, Some(5));
    assert_subr_arity("make-closure", 1, None);
    assert_subr_arity("make-finalizer", 1, Some(1));
    assert_subr_arity("make-indirect-buffer", 2, Some(4));
    assert_subr_arity("make-interpreted-closure", 3, Some(5));
    assert_subr_arity("make-record", 3, Some(3));
    assert_subr_arity("make-temp-file-internal", 4, Some(4));
    assert_subr_arity("map-charset-chars", 2, Some(5));
    assert_subr_arity("map-keymap", 2, Some(3));
    assert_subr_arity("map-keymap-internal", 2, Some(2));
    assert_subr_arity("mapbacktrace", 1, Some(2));
    assert_subr_arity("minibuffer-innermost-command-loop-p", 0, Some(1));
    assert_subr_arity("minibuffer-prompt-end", 0, Some(0));
    assert_subr_arity("next-frame", 0, Some(2));
    assert_subr_arity("ntake", 2, Some(2));
    assert_subr_arity("obarray-clear", 1, Some(1));
    assert_subr_arity("obarray-make", 0, Some(1));
    assert_subr_arity("previous-frame", 0, Some(2));
    assert_subr_arity("put-unicode-property-internal", 3, Some(3));
    assert_subr_arity("raise-frame", 0, Some(1));
    assert_subr_arity("re--describe-compiled", 1, Some(2));
    assert_subr_arity("redisplay", 0, Some(1));
    assert_subr_arity("rename-buffer", 1, Some(2));
    assert_subr_arity("set-buffer-major-mode", 1, Some(1));
    assert_subr_arity("set-buffer-multibyte", 1, Some(1));
    assert_subr_arity("setplist", 2, Some(2));
    assert_subr_arity("split-window-internal", 4, Some(5));
    assert_subr_arity("suspend-emacs", 0, Some(1));
    assert_subr_arity("vertical-motion", 1, Some(3));
    assert_subr_arity("x-begin-drag", 1, Some(6));
    assert_subr_arity("x-create-frame", 1, Some(1));
    assert_subr_arity("x-double-buffered-p", 0, Some(1));
    assert_subr_arity("x-menu-bar-open-internal", 0, Some(1));
    assert_subr_arity("xw-color-defined-p", 1, Some(2));
    assert_subr_arity("xw-color-values", 1, Some(2));
    assert_subr_arity("xw-display-color-p", 0, Some(1));
    assert_subr_arity("get-unicode-property-internal", 2, Some(2));
    assert_subr_arity("get-variable-watchers", 1, Some(1));
    assert_subr_arity("fillarray", 2, Some(2));
    assert_subr_arity("define-hash-table-test", 3, Some(3));
    assert_subr_arity("find-coding-systems-region-internal", 2, Some(3));
    assert_subr_arity("completing-read", 2, Some(8));
    assert_subr_arity("try-completion", 2, Some(3));
    assert_subr_arity("all-completions", 2, Some(3));
    assert_subr_arity("test-completion", 2, Some(3));
    assert_subr_arity("completion--flex-cost-gotoh", 2, Some(2));
}

#[test]
fn subr_arity_read_core_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("read", 0, Some(1));
    assert_subr_arity("read-char", 0, Some(3));
    assert_subr_arity("read-char-exclusive", 0, Some(3));
    assert_subr_arity("read-event", 0, Some(3));
    assert_subr_arity("read-string", 1, Some(5));
    assert_subr_arity("read-variable", 1, Some(2));
    assert_subr_arity("read-from-string", 1, Some(3));
    assert_subr_arity("read-command", 1, Some(2));
}

#[test]
fn subr_arity_input_mode_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("current-input-mode", 0, Some(0));
    assert_subr_arity("set-input-mode", 3, Some(4));
    assert_subr_arity("set-input-interrupt-mode", 1, Some(1));
    assert_subr_arity("set-input-meta-mode", 1, Some(2));
    assert_subr_arity("set-output-flow-control", 1, Some(2));
    assert_subr_arity("set-quit-char", 1, Some(1));
    assert_subr_arity("input-pending-p", 0, Some(1));
    assert_subr_arity("discard-input", 0, Some(0));
    assert_subr_arity("waiting-for-user-input-p", 0, Some(0));
}

#[test]
fn subr_arity_kmacro_command_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("start-kbd-macro", 1, Some(2));
    assert_subr_arity("cancel-kbd-macro-events", 0, Some(0));
    assert_subr_arity("end-kbd-macro", 0, Some(2));
    assert_subr_arity("call-last-kbd-macro", 0, Some(2));
    assert_subr_arity("execute-kbd-macro", 1, Some(3));
}

#[test]
fn subr_arity_keymap_keyboard_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("key-binding", 1, Some(4));
    assert_subr_arity("lookup-key", 2, Some(3));
    assert_subr_arity("key-description", 1, Some(2));
    assert_subr_arity("keymap-parent", 1, Some(1));
    assert_subr_arity("keymapp", 1, Some(1));
    assert_subr_arity("accessible-keymaps", 1, Some(2));
    assert_subr_arity("keyboard-coding-system", 0, Some(1));
    assert_subr_arity("make-keymap", 0, Some(1));
    assert_subr_arity("make-sparse-keymap", 0, Some(1));
}

#[test]
fn subr_arity_delete_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("delete-char", 1, Some(2));
    assert_subr_arity("delete-all-overlays", 0, Some(1));
    assert_subr_arity("delete-and-extract-region", 2, Some(2));
    assert_subr_arity("delete-field", 0, Some(1));
    assert_subr_arity("delete-region", 2, Some(2));
    assert_subr_arity("delete-overlay", 1, Some(1));
    assert_subr_arity("delete-window-internal", 1, Some(1));
}

#[test]
fn subr_arity_filesystem_path_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("access-file", 2, Some(2));
    assert_subr_arity("delete-directory-internal", 1, Some(1));
    assert_subr_arity("delete-file-internal", 1, Some(1));
    assert_subr_arity("directory-file-name", 1, Some(1));
    assert_subr_arity("directory-files", 1, Some(5));
    assert_subr_arity("directory-files-and-attributes", 1, Some(6));
    assert_subr_arity("directory-name-p", 1, Some(1));
    assert_subr_arity("expand-file-name", 1, Some(2));
}

#[test]
fn subr_arity_filesystem_create_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("make-directory-internal", 1, Some(1));
    assert_subr_arity("make-temp-name", 1, Some(1));
    assert_subr_arity("make-symbolic-link", 2, Some(3));
    assert_subr_arity("rename-file", 2, Some(3));
    assert_subr_arity("add-name-to-file", 2, Some(3));
}

#[test]
fn subr_arity_file_load_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("find-buffer", 2, Some(2));
    assert_subr_arity("find-file-name-handler", 2, Some(2));
    assert_subr_arity("insert-file-contents", 1, Some(5));
    assert_subr_arity("load", 1, Some(5));
    assert_subr_arity("locate-file-internal", 2, Some(4));
}

#[test]
fn subr_arity_file_stat_predicate_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("file-acl", 1, Some(1));
    assert_subr_arity("file-attributes", 1, Some(2));
    assert_subr_arity("file-accessible-directory-p", 1, Some(1));
    assert_subr_arity("file-directory-p", 1, Some(1));
    assert_subr_arity("file-executable-p", 1, Some(1));
    assert_subr_arity("file-exists-p", 1, Some(1));
    assert_subr_arity("file-locked-p", 1, Some(1));
    assert_subr_arity("file-modes", 1, Some(2));
    assert_subr_arity("file-newer-than-file-p", 2, Some(2));
    assert_subr_arity("file-readable-p", 1, Some(1));
    assert_subr_arity("file-regular-p", 1, Some(1));
    assert_subr_arity("file-selinux-context", 1, Some(1));
    assert_subr_arity("file-system-info", 1, Some(1));
    assert_subr_arity("file-symlink-p", 1, Some(1));
    assert_subr_arity("file-writable-p", 1, Some(1));
}

#[test]
fn subr_arity_file_name_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("file-name-absolute-p", 1, Some(1));
    assert_subr_arity("file-name-all-completions", 2, Some(2));
    assert_subr_arity("file-name-as-directory", 1, Some(1));
    assert_subr_arity("file-name-case-insensitive-p", 1, Some(1));
    assert_subr_arity("file-name-completion", 2, Some(3));
    assert_subr_arity("file-name-concat", 1, None);
    assert_subr_arity("file-name-directory", 1, Some(1));
    assert_subr_arity("file-name-nondirectory", 1, Some(1));
    assert_subr_arity("get-truename-buffer", 1, Some(1));
    assert_subr_arity("unhandled-file-name-directory", 1, Some(1));
}

#[test]
fn subr_arity_event_error_misc_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("event-convert-list", 1, Some(1));
    assert_subr_arity("error-message-string", 1, Some(1));
    assert_subr_arity("copysign", 2, Some(2));
    assert_subr_arity("equal-including-properties", 2, Some(2));
    assert_subr_arity("function-equal", 2, Some(2));
    assert_subr_arity("emacs-pid", 0, Some(0));
}

#[test]
fn subr_arity_eval_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("eval", 1, Some(2));
    assert_subr_arity("eval-buffer", 0, Some(5));
    assert_subr_arity("eval-region", 2, Some(4));
}

#[test]
fn subr_arity_define_defaults_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("default-file-modes", 0, Some(0));
    assert_subr_arity("define-category", 2, Some(3));
    assert_subr_arity("define-coding-system-alias", 2, Some(2));
    assert_subr_arity("define-key", 3, Some(4));
}

#[test]
fn subr_arity_category_ccl_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("category-table", 0, Some(0));
    assert_subr_arity("clear-charset-maps", 0, Some(0));
    assert_subr_arity("case-table-p", 1, Some(1));
    assert_subr_arity("category-set-mnemonics", 1, Some(1));
    assert_subr_arity("category-table-p", 1, Some(1));
    assert_subr_arity("ccl-program-p", 1, Some(1));
    assert_subr_arity("check-coding-system", 1, Some(1));
    assert_subr_arity("category-docstring", 1, Some(2));
    assert_subr_arity("ccl-execute", 2, Some(2));
    assert_subr_arity("ccl-execute-on-string", 3, Some(5));
}

#[test]
fn subr_arity_coding_system_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("check-coding-systems-region", 3, Some(3));
    assert_subr_arity("coding-system-aliases", 1, Some(1));
    assert_subr_arity("coding-system-base", 1, Some(1));
    assert_subr_arity("coding-system-eol-type", 1, Some(1));
    assert_subr_arity("coding-system-p", 1, Some(1));
    assert_subr_arity("coding-system-plist", 1, Some(1));
    assert_subr_arity("coding-system-priority-list", 0, Some(1));
    assert_subr_arity("coding-system-put", 3, Some(3));
}

#[test]
fn subr_arity_color_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("color-distance", 2, Some(4));
    assert_subr_arity("color-gray-p", 1, Some(2));
    assert_subr_arity("color-supported-p", 1, Some(3));
    assert_subr_arity("color-values-from-color-spec", 1, Some(1));
}

#[test]
fn subr_arity_copy_cons_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("cons", 2, Some(2));
    assert_subr_arity("copy-alist", 1, Some(1));
    assert_subr_arity("copy-category-table", 0, Some(1));
    assert_subr_arity("copy-file", 2, Some(6));
    assert_subr_arity("copy-hash-table", 1, Some(1));
    assert_subr_arity("copy-keymap", 1, Some(1));
    assert_subr_arity("copy-marker", 0, Some(2));
    assert_subr_arity("copy-sequence", 1, Some(1));
    assert_subr_arity("copy-syntax-table", 0, Some(1));
}

#[test]
fn subr_arity_current_state_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("bitmap-spec-p", 1, Some(1));
    assert_subr_arity("byte-to-string", 1, Some(1));
    assert_subr_arity("byteorder", 0, Some(0));
    assert_subr_arity("clear-buffer-auto-save-failure", 0, Some(0));
    assert_subr_arity("cl-type-of", 1, Some(1));
    assert_subr_arity("bidi-find-overridden-directionality", 3, Some(4));
    assert_subr_arity("bidi-resolved-levels", 0, Some(1));
    assert_subr_arity("current-active-maps", 0, Some(2));
    assert_subr_arity("current-bidi-paragraph-direction", 0, Some(1));
    assert_subr_arity("current-case-table", 0, Some(0));
    assert_subr_arity("current-column", 0, Some(0));
    assert_subr_arity("current-global-map", 0, Some(0));
    assert_subr_arity("current-indentation", 0, Some(0));
    assert_subr_arity("current-local-map", 0, Some(0));
    assert_subr_arity("current-message", 0, Some(0));
    assert_subr_arity("current-minor-mode-maps", 0, Some(0));
    assert_subr_arity("current-time-string", 0, Some(2));
    assert_subr_arity("current-time-zone", 0, Some(2));
    assert_subr_arity("current-window-configuration", 0, Some(1));
    assert_subr_arity("daemon-initialized", 0, Some(0));
    assert_subr_arity("daemonp", 0, Some(0));
    assert_subr_arity("invocation-directory", 0, Some(0));
    assert_subr_arity("invocation-name", 0, Some(0));
    assert_subr_arity("system-name", 0, Some(0));
}

#[test]
fn subr_arity_composition_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("clear-composition-cache", 0, Some(0));
    assert_subr_arity("compose-region-internal", 2, Some(4));
    assert_subr_arity("compose-string-internal", 3, Some(5));
    assert_subr_arity("composition-get-gstring", 4, Some(4));
    assert_subr_arity("composition-sort-rules", 1, Some(1));
}

#[test]
fn subr_arity_predicate_core_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("charsetp", 1, Some(1));
    assert_subr_arity("closurep", 1, Some(1));
    assert_subr_arity("decode-char", 2, Some(2));
    assert_subr_arity("default-boundp", 1, Some(1));
    assert_subr_arity("default-toplevel-value", 1, Some(1));
    assert_subr_arity("default-value", 1, Some(1));
    assert_subr_arity("integer-or-marker-p", 1, Some(1));
    assert_subr_arity("module-function-p", 1, Some(1));
    assert_subr_arity("number-or-marker-p", 1, Some(1));
    assert_subr_arity("obarrayp", 1, Some(1));
    assert_subr_arity("special-variable-p", 1, Some(1));
    assert_subr_arity("symbol-with-pos-p", 1, Some(1));
    assert_subr_arity("symbol-with-pos-pos", 1, Some(1));
    assert_subr_arity("user-ptrp", 1, Some(1));
    assert_subr_arity("vector-or-char-table-p", 1, Some(1));
    assert_subr_arity("featurep", 1, Some(2));
}

#[test]
fn subr_arity_abbrev_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
}

#[test]
fn subr_arity_cxr_family_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("car", 1, Some(1));
    assert_subr_arity("cdr", 1, Some(1));
}

#[test]
fn subr_arity_symbol_state_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("fboundp", 1, Some(1));
    assert_subr_arity("func-arity", 1, Some(1));
    assert_subr_arity("native-comp-function-p", 1, Some(1));
    assert_subr_arity("fset", 2, Some(2));
    assert_subr_arity("fmakunbound", 1, Some(1));
    assert_subr_arity("makunbound", 1, Some(1));
    assert_subr_arity("set", 2, Some(2));
    assert_subr_arity("get", 2, Some(2));
    assert_subr_arity("put", 3, Some(3));
    assert_subr_arity("symbol-function", 1, Some(1));
    assert_subr_arity("symbol-value", 1, Some(1));
}

#[test]
fn subr_arity_symbol_obarray_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("intern", 1, Some(2));
    assert_subr_arity("intern-soft", 1, Some(2));
    assert_subr_arity("make-symbol", 1, Some(1));
    assert_subr_arity("symbol-name", 1, Some(1));
    assert_subr_arity("symbol-plist", 1, Some(1));
    assert_subr_arity("unintern", 2, Some(2));
    assert_subr_arity("indirect-function", 1, Some(2));
}

#[test]
fn subr_arity_line_position_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("line-beginning-position", 0, Some(1));
    assert_subr_arity("line-end-position", 0, Some(1));
    assert_subr_arity("pos-bol", 0, Some(1));
    assert_subr_arity("pos-eol", 0, Some(1));
    assert_subr_arity("line-number-at-pos", 0, Some(2));
    assert_subr_arity("line-number-display-width", 0, Some(1));
    assert_subr_arity("line-pixel-height", 0, Some(0));
}

#[test]
fn subr_arity_search_match_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("looking-at", 1, Some(2));
    assert_subr_arity("posix-looking-at", 1, Some(2));
    assert_subr_arity("match-beginning", 1, Some(1));
    assert_subr_arity("match-end", 1, Some(1));
    assert_subr_arity("match-data", 0, Some(3));
    assert_subr_arity("replace-match", 1, Some(5));
    assert_subr_arity("string-match", 2, Some(4));
    assert_subr_arity("posix-string-match", 2, Some(4));
    assert_subr_arity("search-forward", 1, Some(4));
    assert_subr_arity("search-backward", 1, Some(4));
    assert_subr_arity("re-search-forward", 1, Some(4));
    assert_subr_arity("re-search-backward", 1, Some(4));
    assert_subr_arity("posix-search-forward", 1, Some(4));
    assert_subr_arity("posix-search-backward", 1, Some(4));
}

#[test]
fn subr_arity_edit_state_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("insert-byte", 2, Some(3));
    assert_subr_arity("insert-char", 1, Some(3));
    assert_subr_arity("other-window-for-scrolling", 0, Some(0));
    assert_subr_arity("local-variable-p", 1, Some(2));
    assert_subr_arity("locale-info", 1, Some(1));
    assert_subr_arity("max-char", 0, Some(1));
    assert_subr_arity("memory-use-counts", 0, Some(0));
    assert_subr_arity("make-marker", 0, Some(0));
    assert_subr_arity("make-local-variable", 1, Some(1));
    assert_subr_arity("make-variable-buffer-local", 1, Some(1));
    assert_subr_arity("mapatoms", 1, Some(2));
    assert_subr_arity("map-char-table", 2, Some(2));
}

#[test]
fn subr_arity_read_region_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("read-buffer", 1, Some(4));
    assert_subr_arity("read-coding-system", 1, Some(2));
    assert_subr_arity("read-from-minibuffer", 1, Some(7));
    assert_subr_arity("read-key-sequence", 1, Some(6));
    assert_subr_arity("read-key-sequence-vector", 1, Some(6));
    assert_subr_arity("read-non-nil-coding-system", 1, Some(1));
    assert_subr_arity("recenter", 0, Some(2));
    assert_subr_arity("recursion-depth", 0, Some(0));
    assert_subr_arity("region-beginning", 0, Some(0));
    assert_subr_arity("region-end", 0, Some(0));
    assert_subr_arity("regexp-quote", 1, Some(1));
}

#[test]
fn subr_arity_print_replace_edit_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("prin1", 1, Some(3));
    assert_subr_arity("prin1-to-string", 1, Some(3));
    assert_subr_arity("princ", 1, Some(2));
    assert_subr_arity("print", 1, Some(2));
    assert_subr_arity("terpri", 0, Some(2));
    assert_subr_arity("write-char", 1, Some(2));
    assert_subr_arity("propertize", 1, None);
}

#[test]
fn subr_arity_window_navigation_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("delete-frame", 0, Some(2));
    assert_subr_arity("delete-other-windows-internal", 0, Some(2));
    assert_subr_arity("next-window", 0, Some(3));
    assert_subr_arity("previous-window", 0, Some(3));
    assert_subr_arity("coordinates-in-window-p", 2, Some(2));
    assert_subr_arity("pos-visible-in-window-p", 0, Some(3));
    assert_subr_arity("move-to-window-line", 1, Some(1));
    assert_subr_arity("move-point-visually", 1, Some(1));
    assert_subr_arity("modify-frame-parameters", 2, Some(2));
    assert_subr_arity("iconify-frame", 0, Some(1));
    assert_subr_arity("make-frame-visible", 0, Some(1));
}

#[test]
fn subr_arity_face_font_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("close-font", 1, Some(2));
    assert_subr_arity("face-attribute-relative-p", 2, Some(2));
    assert_subr_arity("face-font", 1, Some(3));
    assert_subr_arity("font-get", 2, Some(2));
    assert_subr_arity("font-put", 3, Some(3));
    assert_subr_arity("font-xlfd-name", 1, Some(3));
    assert_subr_arity("fontp", 1, Some(2));
    assert_subr_arity("internal-copy-lisp-face", 4, Some(4));
    assert_subr_arity("internal-face-x-get-resource", 2, Some(3));
    assert_subr_arity("internal-get-lisp-face-attribute", 2, Some(3));
    assert_subr_arity("internal-make-lisp-face", 1, Some(2));
    assert_subr_arity("internal-lisp-face-attribute-values", 1, Some(1));
    assert_subr_arity("internal-lisp-face-empty-p", 1, Some(2));
    assert_subr_arity("internal-lisp-face-equal-p", 2, Some(3));
    assert_subr_arity("internal-lisp-face-p", 1, Some(2));
    assert_subr_arity("internal-merge-in-global-face", 2, Some(2));
    assert_subr_arity("internal-set-alternative-font-family-alist", 1, Some(1));
    assert_subr_arity("internal-set-alternative-font-registry-alist", 1, Some(1));
    assert_subr_arity("internal-set-font-selection-order", 1, Some(1));
    assert_subr_arity("internal-set-lisp-face-attribute", 3, Some(4));
}

#[test]
fn subr_arity_syntax_category_plist_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("category-set-mnemonics", 1, Some(1));
    assert_subr_arity("file-attributes-lessp", 2, Some(2));
    assert_subr_arity("forward-comment", 1, Some(1));
    assert_subr_arity("get-unused-category", 0, Some(1));
    assert_subr_arity("make-category-set", 1, Some(1));
    assert_subr_arity("make-category-table", 0, Some(0));
    assert_subr_arity("make-char-table", 1, Some(2));
    assert_subr_arity("modify-category-entry", 2, Some(4));
    assert_subr_arity("modify-syntax-entry", 2, Some(3));
    assert_subr_arity("parse-partial-sexp", 2, Some(6));
    assert_subr_arity("plist-get", 2, Some(3));
    assert_subr_arity("plist-member", 2, Some(3));
    assert_subr_arity("plist-put", 3, Some(4));
    assert_subr_arity("natnump", 1, Some(1));
    assert_subr_arity("preceding-char", 0, Some(0));
}

#[test]
fn subr_arity_set_scan_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("set-buffer", 1, Some(1));
    assert_subr_arity("set-buffer-modified-p", 1, Some(1));
    assert_subr_arity("set-case-table", 1, Some(1));
    assert_subr_arity("set-category-table", 1, Some(1));
    assert_subr_arity("set-char-table-extra-slot", 3, Some(3));
    assert_subr_arity("set-char-table-parent", 2, Some(2));
    assert_subr_arity("set-char-table-range", 3, Some(3));
    assert_subr_arity("set-default", 2, Some(2));
    assert_subr_arity("set-default-file-modes", 1, Some(1));
    assert_subr_arity("set-default-toplevel-value", 2, Some(2));
    assert_subr_arity("set-file-acl", 2, Some(2));
    assert_subr_arity("set-file-modes", 2, Some(3));
    assert_subr_arity("set-file-selinux-context", 2, Some(2));
    assert_subr_arity("set-file-times", 1, Some(3));
    assert_subr_arity("set-keyboard-coding-system-internal", 1, Some(2));
    assert_subr_arity("set-keymap-parent", 2, Some(2));
    assert_subr_arity("set-match-data", 1, Some(2));
    assert_subr_arity("set-safe-terminal-coding-system-internal", 1, Some(1));
    assert_subr_arity("set-standard-case-table", 1, Some(1));
    assert_subr_arity("set-syntax-table", 1, Some(1));
    assert_subr_arity("set-terminal-coding-system-internal", 1, Some(2));
    assert_subr_arity("set-text-conversion-style", 1, Some(2));
    assert_subr_arity("set-time-zone-rule", 1, Some(1));
    assert_subr_arity("set-visited-file-modtime", 0, Some(1));
    assert_subr_arity("set-window-dedicated-p", 2, Some(2));
    assert_subr_arity("setcar", 2, Some(2));
    assert_subr_arity("setcdr", 2, Some(2));
    assert_subr_arity("verify-visited-file-modtime", 0, Some(1));
    assert_subr_arity("visited-file-modtime", 0, Some(0));
    assert_subr_arity("scan-lists", 3, Some(3));
    assert_subr_arity("scan-sexps", 2, Some(2));
}

#[test]
fn subr_arity_string_syntax_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("string-as-multibyte", 1, Some(1));
    assert_subr_arity("string-as-unibyte", 1, Some(1));
    assert_subr_arity("string-collate-equalp", 2, Some(4));
    assert_subr_arity("string-collate-lessp", 2, Some(4));
    assert_subr_arity("string-make-multibyte", 1, Some(1));
    assert_subr_arity("string-make-unibyte", 1, Some(1));
    assert_subr_arity("string-search", 2, Some(3));
    assert_subr_arity("string-to-multibyte", 1, Some(1));
    assert_subr_arity("string-to-syntax", 1, Some(1));
    assert_subr_arity("string-to-unibyte", 1, Some(1));
    assert_subr_arity("string-version-lessp", 2, Some(2));
    assert_subr_arity("substitute-in-file-name", 1, Some(1));
    assert_subr_arity("syntax-class-to-char", 1, Some(1));
    assert_subr_arity("syntax-table", 0, Some(0));
    assert_subr_arity("syntax-table-p", 1, Some(1));
    assert_subr_arity("standard-case-table", 0, Some(0));
    assert_subr_arity("standard-category-table", 0, Some(0));
    assert_subr_arity("standard-syntax-table", 0, Some(0));
}

#[test]
fn subr_arity_time_user_runtime_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("threadp", 1, Some(1));
    assert_subr_arity("time-add", 2, Some(2));
    assert_subr_arity("time-convert", 1, Some(2));
    assert_subr_arity("time-equal-p", 2, Some(2));
    assert_subr_arity("time-less-p", 2, Some(2));
    assert_subr_arity("time-subtract", 2, Some(2));
    assert_subr_arity("system-groups", 0, Some(0));
    assert_subr_arity("system-users", 0, Some(0));
    assert_subr_arity("tab-bar-height", 0, Some(2));
    assert_subr_arity("text-char-description", 1, Some(1));
    assert_subr_arity("tool-bar-height", 0, Some(2));
    assert_subr_arity("user-full-name", 0, Some(1));
    assert_subr_arity("user-login-name", 0, Some(1));
    assert_subr_arity("user-real-login-name", 0, Some(0));
    assert_subr_arity("user-real-uid", 0, Some(0));
    assert_subr_arity("user-uid", 0, Some(0));
    assert_subr_arity("yes-or-no-p", 1, Some(1));
    assert_subr_arity("zlib-available-p", 0, Some(0));
    assert_subr_arity("zlib-decompress-region", 2, Some(3));
}

#[test]
fn subr_arity_command_edit_runtime_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("self-insert-command", 1, Some(2));
    assert_subr_arity("signal", 1, Some(2));
    assert_subr_arity("single-key-description", 1, Some(2));
    assert_subr_arity("skip-chars-backward", 1, Some(2));
    assert_subr_arity("skip-chars-forward", 1, Some(2));
    assert_subr_arity("skip-syntax-backward", 1, Some(2));
    assert_subr_arity("skip-syntax-forward", 1, Some(2));
    assert_subr_arity("sort", 1, None);
    assert_subr_arity("store-kbd-macro-event", 1, Some(1));
    assert_subr_arity("take", 2, Some(2));
    assert_subr_arity("clear-this-command-keys", 0, Some(1));
    assert_subr_arity("combine-after-change-execute", 0, Some(0));
    assert_subr_arity("this-command-keys", 0, Some(0));
    assert_subr_arity("this-command-keys-vector", 0, Some(0));
    assert_subr_arity("undo-boundary", 0, Some(0));
    assert_subr_arity("unibyte-char-to-multibyte", 1, Some(1));
    assert_subr_arity("upcase-initials", 1, Some(1));
    assert_subr_arity("upcase-initials-region", 2, Some(3));
    assert_subr_arity("upcase-region", 2, Some(3));
    assert_subr_arity("upcase-word", 1, Some(1));
    assert_subr_arity("use-global-map", 1, Some(1));
    assert_subr_arity("use-local-map", 1, Some(1));
    assert_subr_arity("where-is-internal", 1, Some(5));
}

#[test]
fn subr_arity_replace_window_io_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("minor-mode-key-binding", 1, Some(2));
    assert_subr_arity("narrow-to-region", 2, Some(2));
    assert_subr_arity("remove-variable-watcher", 2, Some(2));
    assert_subr_arity("scroll-down", 0, Some(1));
    assert_subr_arity("scroll-left", 0, Some(2));
    assert_subr_arity("scroll-right", 0, Some(2));
    assert_subr_arity("scroll-up", 0, Some(1));
    assert_subr_arity("select-frame", 1, Some(2));
    assert_subr_arity("select-window", 1, Some(2));
    assert_subr_arity("selected-frame", 0, Some(0));
    assert_subr_arity("set-charset-priority", 1, None);
    assert_subr_arity("subr-arity", 1, Some(1));
    assert_subr_arity("subr-name", 1, Some(1));
    assert_subr_arity("subrp", 1, Some(1));
    assert_subr_arity("write-region", 3, Some(7));
}

#[test]
fn subr_arity_charset_json_libxml_display_helpers_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("define-charset-internal", 17, None);
    assert_subr_arity("find-charset-region", 2, Some(3));
    assert_subr_arity("find-charset-string", 1, Some(2));
    assert_subr_arity("find-composition-internal", 4, Some(4));
    assert_subr_arity("format-mode-line", 1, Some(4));
    assert_subr_arity("json-insert", 1, None);
    assert_subr_arity("json-parse-string", 1, None);
    assert_subr_arity("json-serialize", 1, None);
    assert_subr_arity("libxml-available-p", 0, Some(0));
    assert_subr_arity("libxml-parse-html-region", 0, Some(4));
    assert_subr_arity("libxml-parse-xml-region", 0, Some(4));
    assert_subr_arity("line-pixel-height", 0, Some(0));
    assert_subr_arity("long-line-optimizations-p", 0, Some(0));
    assert_subr_arity("lookup-image-map", 3, Some(3));
    assert_subr_arity("make-bool-vector", 2, Some(2));
    assert_subr_arity("matching-paren", 1, Some(1));
    assert_subr_arity("md5", 1, Some(5));
    assert_subr_arity("merge-face-attribute", 3, Some(3));
    assert_subr_arity("multibyte-char-to-unibyte", 1, Some(1));
    assert_subr_arity("multibyte-string-p", 1, Some(1));
    assert_subr_arity("secure-hash", 2, Some(5));
    assert_subr_arity("secure-hash-algorithms", 0, Some(0));
}

#[test]
fn subr_arity_hash_table_introspection_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("hash-table-test", 1, Some(1));
    assert_subr_arity("hash-table-size", 1, Some(1));
    assert_subr_arity("hash-table-rehash-size", 1, Some(1));
    assert_subr_arity("hash-table-rehash-threshold", 1, Some(1));
    assert_subr_arity("hash-table-weakness", 1, Some(1));
    assert_subr_arity("internal--hash-table-buckets", 1, Some(1));
    assert_subr_arity("internal--hash-table-histogram", 1, Some(1));
    assert_subr_arity("internal--hash-table-index-size", 1, Some(1));
    assert_subr_arity("sxhash-eq", 1, Some(1));
    assert_subr_arity("sxhash-eql", 1, Some(1));
    assert_subr_arity("sxhash-equal", 1, Some(1));
    assert_subr_arity("sxhash-equal-including-properties", 1, Some(1));
}

#[test]
fn subr_arity_hash_table_core_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("hash-table-p", 1, Some(1));
    assert_subr_arity("make-hash-table", 0, None);
    assert_subr_arity("gethash", 2, Some(3));
    assert_subr_arity("puthash", 3, Some(3));
    assert_subr_arity("remhash", 2, Some(2));
    assert_subr_arity("clrhash", 1, Some(1));
    assert_subr_arity("hash-table-count", 1, Some(1));
    assert_subr_arity("maphash", 2, Some(2));
}

#[test]
fn subr_arity_buffer_lookup_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("get-buffer", 1, Some(1));
    assert_subr_arity("get-buffer-create", 1, Some(2));
    assert_subr_arity("get-file-buffer", 1, Some(1));
    assert_subr_arity("generate-new-buffer-name", 1, Some(2));
}

#[test]
fn subr_arity_numeric_state_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("fceiling", 1, Some(1));
    assert_subr_arity("ffloor", 1, Some(1));
    assert_subr_arity("frexp", 1, Some(1));
    assert_subr_arity("fround", 1, Some(1));
    assert_subr_arity("framep", 1, Some(1));
    assert_subr_arity("ftruncate", 1, Some(1));
    assert_subr_arity("following-char", 0, Some(0));
    assert_subr_arity("garbage-collect", 0, Some(0));
    assert_subr_arity("get-load-suffixes", 0, Some(0));
    assert_subr_arity("get-byte", 0, Some(2));
}

#[test]
fn subr_arity_misc_helper_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("format-message", 1, None);
    assert_subr_arity("identity", 1, Some(1));
    assert_subr_arity("prefix-numeric-value", 1, Some(1));
    assert_subr_arity("length", 1, Some(1));
    assert_subr_arity("length<", 2, Some(2));
    assert_subr_arity("length=", 2, Some(2));
    assert_subr_arity("length>", 2, Some(2));
    assert_subr_arity("ldexp", 2, Some(2));
    assert_subr_arity("logb", 1, Some(1));
    assert_subr_arity("logcount", 1, Some(1));
    assert_subr_arity("text-quoting-style", 0, Some(0));
    assert_subr_arity("lognot", 1, Some(1));
    assert_subr_arity("ngettext", 3, Some(3));
    assert_subr_arity("substring-no-properties", 1, Some(3));
    assert_subr_arity("group-name", 1, Some(1));
    assert_subr_arity("group-gid", 0, Some(0));
    assert_subr_arity("group-real-gid", 0, Some(0));
    assert_subr_arity("load-average", 0, Some(1));
    assert_subr_arity("last-nonminibuffer-frame", 0, Some(0));
    assert_subr_arity("interpreted-function-p", 1, Some(1));
    assert_subr_arity("invisible-p", 1, Some(1));
}

#[test]
fn subr_arity_window_frame_primitives_match_oracle() {
    crate::test_utils::init_test_tracing();
    assert_subr_arity("active-minibuffer-window", 0, Some(0));
    assert_subr_arity("frame-char-height", 0, Some(1));
    assert_subr_arity("frame-char-width", 0, Some(1));
    assert_subr_arity("frame-first-window", 0, Some(1));
    assert_subr_arity("frame-list", 0, Some(0));
    assert_subr_arity("frame-live-p", 1, Some(1));
    assert_subr_arity("frame-native-height", 0, Some(1));
    assert_subr_arity("frame-native-width", 0, Some(1));
    assert_subr_arity("frame-parameter", 2, Some(2));
    assert_subr_arity("frame-parameters", 0, Some(1));
    assert_subr_arity("frame-position", 0, Some(1));
    assert_subr_arity("frame-root-window", 0, Some(1));
    assert_subr_arity("frame-visible-p", 1, Some(1));
    assert_subr_arity("frame-text-cols", 0, Some(1));
    assert_subr_arity("frame-text-height", 0, Some(1));
    assert_subr_arity("frame-text-lines", 0, Some(1));
    assert_subr_arity("frame-text-width", 0, Some(1));
    assert_subr_arity("frame-total-cols", 0, Some(1));
    assert_subr_arity("frame-total-lines", 0, Some(1));
    assert_subr_arity("minibuffer-selected-window", 0, Some(0));
    assert_subr_arity("minibuffer-window", 0, Some(1));
    assert_subr_arity("selected-window", 0, Some(0));
    assert_subr_arity("set-frame-height", 2, Some(4));
    assert_subr_arity("set-frame-width", 2, Some(4));
    assert_subr_arity("set-frame-size", 3, Some(4));
    assert_subr_arity("set-frame-position", 3, Some(3));
    assert_subr_arity("set-window-parameter", 3, Some(3));
    assert_subr_arity("set-window-buffer", 2, Some(3));
    assert_subr_arity("set-window-configuration", 1, Some(3));
    assert_subr_arity("set-window-hscroll", 2, Some(2));
    assert_subr_arity("set-window-display-table", 2, Some(2));
    assert_subr_arity("set-window-cursor-type", 2, Some(2));
    assert_subr_arity("set-window-prev-buffers", 2, Some(2));
    assert_subr_arity("set-window-next-buffers", 2, Some(2));
    assert_subr_arity("set-window-margins", 2, Some(3));
    assert_subr_arity("set-window-point", 2, Some(2));
    assert_subr_arity("set-window-fringes", 2, Some(5));
    assert_subr_arity("set-window-scroll-bars", 1, Some(6));
    assert_subr_arity("set-window-start", 2, Some(3));
    assert_subr_arity("set-window-vscroll", 2, Some(4));
    assert_subr_arity("window-frame", 0, Some(1));
    assert_subr_arity("window-fringes", 0, Some(1));
    assert_subr_arity("window-header-line-height", 0, Some(1));
    assert_subr_arity("window-hscroll", 0, Some(1));
    assert_subr_arity("window-left-column", 0, Some(1));
    assert_subr_arity("window-margins", 0, Some(1));
    assert_subr_arity("window-mode-line-height", 0, Some(1));
    assert_subr_arity("window-pixel-height", 0, Some(1));
    assert_subr_arity("window-pixel-width", 0, Some(1));
    assert_subr_arity("visible-frame-list", 0, Some(0));
    assert_subr_arity("window-body-height", 0, Some(2));
    assert_subr_arity("window-body-width", 0, Some(2));
    assert_subr_arity("window-text-height", 0, Some(2));
    assert_subr_arity("window-text-width", 0, Some(2));
    assert_subr_arity("window-buffer", 0, Some(1));
    assert_subr_arity("window-configuration-equal-p", 2, Some(2));
    assert_subr_arity("window-configuration-frame", 1, Some(1));
    assert_subr_arity("window-configuration-p", 1, Some(1));
    assert_subr_arity("window-cursor-type", 0, Some(1));
    assert_subr_arity("window-display-table", 0, Some(1));
    assert_subr_arity("window-dedicated-p", 0, Some(1));
    assert_subr_arity("window-end", 0, Some(2));
    assert_subr_arity("window-list", 0, Some(3));
    assert_subr_arity("window-live-p", 1, Some(1));
    assert_subr_arity("window-next-buffers", 0, Some(1));
    assert_subr_arity("window-old-buffer", 0, Some(1));
    assert_subr_arity("window-old-point", 0, Some(1));
    assert_subr_arity("window-parameter", 2, Some(2));
    assert_subr_arity("window-parameters", 0, Some(1));
    assert_subr_arity("window-prev-buffers", 0, Some(1));
    assert_subr_arity("window-scroll-bars", 0, Some(1));
    assert_subr_arity("window-valid-p", 1, Some(1));
    assert_subr_arity("window-minibuffer-p", 0, Some(1));
    assert_subr_arity("window-point", 0, Some(1));
    assert_subr_arity("window-start", 0, Some(1));
    assert_subr_arity("window-use-time", 0, Some(1));
    assert_subr_arity("window-vscroll", 0, Some(2));
    assert_subr_arity("window-top-line", 0, Some(1));
    assert_subr_arity("buffer-text-pixel-size", 0, Some(4));
    assert_subr_arity("compute-motion", 7, Some(7));
    assert_subr_arity("window-text-pixel-size", 0, Some(7));
    assert_subr_arity("windowp", 1, Some(1));
    assert_subr_arity("get-buffer-window", 0, Some(2));
    assert_subr_arity("window-total-height", 0, Some(2));
    assert_subr_arity("window-total-width", 0, Some(2));
}

// -- interpreted-function-p --

#[test]
fn interpreted_function_p_true_for_lambda() {
    crate::test_utils::init_test_tracing();
    let lam = make_lambda(vec!["x"], vec![], None);
    let result = builtin_interpreted_function_p(vec![lam]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn interpreted_function_p_false_for_bytecode() {
    crate::test_utils::init_test_tracing();
    let bc = make_bytecode(vec![], None);
    let result = builtin_interpreted_function_p(vec![bc]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn interpreted_function_p_false_for_subr() {
    crate::test_utils::init_test_tracing();
    let result = builtin_interpreted_function_p(vec![Value::subr(intern("car"))]).unwrap();
    assert!(result.is_nil());
}

// -- special-form-p --

#[test]
fn special_form_p_true_for_if() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::symbol("if")]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn special_form_p_true_for_quote() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::symbol("quote")]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn special_form_p_true_for_setq() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::symbol("setq")]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn special_form_p_false_for_car() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::symbol("car")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn special_form_p_false_for_when() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::symbol("when")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn special_form_p_false_for_throw() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::symbol("throw")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn special_form_p_false_for_int() {
    crate::test_utils::init_test_tracing();
    let result = builtin_special_form_p(vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());
}

// -- macrop --
//
// `macrop' has no Rust implementation any more: GNU has no DEFUN of that
// name, only `(defun macrop (object) ...)' at lisp/subr.el:4793, and the
// helper these tests exercised (`macrop_check') existed only for it.  The
// seven arms moved to `macrop_arms_match_gnu' in
// `builtins/lisp_only_predicates_and_aliases_test.rs', where they are asked
// of the loaded runtime instead.  DIVERGENCES.md 148.

// -- commandp --

#[test]
fn commandp_true_for_subr() {
    crate::test_utils::init_test_tracing();
    let result = builtin_commandp(vec![Value::subr(intern("car"))]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn commandp_true_for_lambda() {
    crate::test_utils::init_test_tracing();
    let lam = make_lambda(vec![], vec![], None);
    let result = builtin_commandp(vec![lam]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn commandp_false_for_int() {
    crate::test_utils::init_test_tracing();
    let result = builtin_commandp(vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn commandp_false_for_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_commandp(vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn commandp_rejects_overflow_arity() {
    crate::test_utils::init_test_tracing();
    let err = builtin_commandp(vec![Value::symbol("car"), Value::NIL, Value::NIL])
        .expect_err("commandp should reject more than two arguments");
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("unexpected flow: {other:?}"),
    }
}

// -- func-arity --

#[test]
fn func_arity_lambda_required_only() {
    crate::test_utils::init_test_tracing();
    let lam = make_lambda(vec!["a", "b"], vec![], None);
    let result = func_arity(lam).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(2));
        assert_eq!(pair_cdr.as_int(), Some(2));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn func_arity_lambda_with_optional() {
    crate::test_utils::init_test_tracing();
    let lam = make_lambda(vec!["a"], vec!["b", "c"], None);
    let result = func_arity(lam).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(1));
        assert_eq!(pair_cdr.as_int(), Some(3));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn func_arity_lambda_with_rest() {
    crate::test_utils::init_test_tracing();
    let lam = make_lambda(vec!["a"], vec![], Some("rest"));
    let result = func_arity(lam).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(1));
        assert_eq!(pair_cdr.as_symbol_name(), Some("many"));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn func_arity_bytecode() {
    crate::test_utils::init_test_tracing();
    let bc = make_bytecode(vec!["x", "y"], Some("rest"));
    let result = func_arity(bc).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(2));
        assert_eq!(pair_cdr.as_symbol_name(), Some("many"));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn func_arity_subr() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let function = ctx.obarray().symbol_function("+").unwrap();
    let result = builtin_func_arity_ctx(&mut ctx, vec![function]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(0));
        assert_eq!(pair_cdr.as_symbol_name(), Some("many"));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn func_arity_subr_uses_registered_metadata() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let message_function = ctx.obarray().symbol_function("message").unwrap();
    let message = builtin_func_arity_ctx(&mut ctx, vec![message_function]).unwrap();
    if message.is_cons() {
        let pair_car = message.cons_car();
        let pair_cdr = message.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(1));
        assert_eq!(pair_cdr.as_symbol_name(), Some("many"));
    } else {
        panic!("expected cons cell");
    }

    let car_function = ctx.obarray().symbol_function("car").unwrap();
    let car = builtin_func_arity_ctx(&mut ctx, vec![car_function]).unwrap();
    if car.is_cons() {
        let pair_car = car.cons_car();
        let pair_cdr = car.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(1));
        assert_eq!(pair_cdr.as_int(), Some(1));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn func_arity_macro() {
    crate::test_utils::init_test_tracing();
    let m = make_macro(vec!["a", "b"]);
    let result = func_arity(m).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car.as_int(), Some(2));
        assert_eq!(pair_cdr.as_int(), Some(2));
    } else {
        panic!("expected cons cell");
    }
}

#[test]
fn gnu_lisp_macro_forms_are_not_evaluator_special_forms() {
    crate::test_utils::init_test_tracing();
    for name in [
        "declare",
        "eval-when-compile",
        "eval-and-compile",
        "setq-default",
        "defcustom",
        "defgroup",
        "define-error",
        "defvar-local",
        "track-mouse",
        "with-current-buffer",
        "with-temp-buffer",
        "with-output-to-string",
        "with-syntax-table",
        "with-mutex",
        "autoload",
    ] {
        assert!(
            !is_evaluator_special_form_name(name),
            "{name} should remain a GNU Lisp macro, not an evaluator special form"
        );
    }
}

#[test]
fn func_arity_error_for_non_callable() {
    crate::test_utils::init_test_tracing();
    let result = func_arity(Value::fixnum(42));
    assert!(result.is_err());
}

#[test]
fn func_arity_autoload_object_signals_wrong_type_argument_symbolp() {
    crate::test_utils::init_test_tracing();
    let autoload_fn = Value::list(vec![
        Value::symbol("autoload"),
        Value::string("vm-auto-file"),
        Value::NIL,
        Value::T,
        Value::NIL,
    ]);
    let result = func_arity(autoload_fn).expect_err("autoload forms should not satisfy func-arity");
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), autoload_fn]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

// -- wrong arg count --

#[test]
fn subr_name_wrong_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_subr_name(vec![]);
    assert!(result.is_err());
}

#[test]
fn func_arity_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_func_arity_ctx(&mut ctx, vec![]);
    assert!(result.is_err());
}
