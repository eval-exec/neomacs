//! Redisplay scheduling omits only the effective fontification hook identity.
use crate::buffer::BufferId;
use crate::emacs_core::{Context, Value};
use crate::window::{BufferLayoutInputState, FontificationHookIdentity};

fn setup() -> (Context, BufferId) {
    let mut eval = Context::new();
    eval.eval_str("(setq fontification-functions nil)").unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    (eval, buffer)
}

#[test]
fn redisplay_projection_skips_fontifier_identity_read() {
    let (eval, buffer) = setup();
    eval.buffer_layout_inputs(buffer).unwrap();
    crate::emacs_core::intern::reset_intern_calls();
    let light: BufferLayoutInputState<()> = eval.redisplay_buffer_layout_inputs(buffer).unwrap();
    assert!(
        crate::emacs_core::intern::intern_call_names()
            .iter()
            .all(|name| name != "fontification-functions"),
        "redisplay-only capture must not intern or fetch the hook"
    );
    crate::emacs_core::intern::reset_intern_calls();
    let full: BufferLayoutInputState<FontificationHookIdentity> =
        eval.buffer_layout_inputs(buffer).unwrap();
    assert_eq!(
        crate::emacs_core::intern::intern_call_names()
            .iter()
            .filter(|name| name.as_str() == "fontification-functions")
            .count(),
        0,
        "full freshness reads the effective hook by its pre-resolved identity"
    );
    assert_eq!(light.id, full.id);
    assert_eq!(light.prefixes, full.prefixes);
    assert_eq!(light.invisibility, full.invisibility);
    assert_eq!(light.display_table, full.display_table);
}

#[test]
fn hook_replacement_changes_only_full_buffer_projection() {
    let (mut eval, buffer) = setup();
    let light_before = eval.redisplay_buffer_layout_inputs(buffer).unwrap();
    let full_before = eval.buffer_layout_inputs(buffer).unwrap();
    eval.eval_str("(setq fontification-functions (list 'ignore))")
        .unwrap();
    let light_after = eval.redisplay_buffer_layout_inputs(buffer).unwrap();
    let full_after = eval.buffer_layout_inputs(buffer).unwrap();
    assert_eq!(light_before, light_after);
    assert_ne!(full_before, full_after);
    assert_ne!(
        full_before.fontification_functions,
        full_after.fontification_functions
    );
}

#[test]
fn both_projections_keep_owned_prefix_mutations_without_running_when() {
    let (mut eval, buffer) = setup();
    eval.eval_str(
        r#"(progn
          (setq ttykey-projection-prefix (copy-sequence "ab")
                ttykey-projection-when-calls 0
                line-prefix ttykey-projection-prefix)
          (put-text-property
           0 1 'display
           '(when (progn
                    (setq ttykey-projection-when-calls
                          (1+ ttykey-projection-when-calls))
                    t) . "x")
           ttykey-projection-prefix))"#,
    )
    .unwrap();
    let light_before = eval.redisplay_buffer_layout_inputs(buffer).unwrap();
    let full_before = eval.buffer_layout_inputs(buffer).unwrap();
    let light_debug = format!("{light_before:?}");
    let full_debug = format!("{full_before:?}");
    eval.eval_str("(aset ttykey-projection-prefix 0 ?z)")
        .unwrap();
    let light_after = eval.redisplay_buffer_layout_inputs(buffer).unwrap();
    let full_after = eval.buffer_layout_inputs(buffer).unwrap();
    assert_ne!(light_before, light_after);
    assert_ne!(full_before, full_after);
    assert_eq!(light_after.prefixes, full_after.prefixes);
    assert_eq!(format!("{light_before:?}"), light_debug);
    assert_eq!(format!("{full_before:?}"), full_debug);
    assert_eq!(
        eval.eval_str("ttykey-projection-when-calls").unwrap(),
        Value::fixnum(0)
    );
}
