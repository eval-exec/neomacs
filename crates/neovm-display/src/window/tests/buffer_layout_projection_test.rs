//! Redisplay scheduling omits only the effective fontification hook identity.
use crate::window::display::WindowDisplayContextExt;
use crate::window::{BufferLayoutInputState, FontificationHookIdentity};
use neovm_eval::emacs_core::eval::Context;

#[test]
fn redisplay_projection_skips_fontifier_identity_read() {
    let (eval, buffer) = crate::test_utils::upper_layer_helpers::buffer_layout_projection_setup(
        Context::new(crate::test_utils::unit_runtime()),
    );
    eval.buffer_layout_inputs(buffer).unwrap();
    neovm_value::intern::reset_intern_calls();
    let light: BufferLayoutInputState<()> = eval.redisplay_buffer_layout_inputs(buffer).unwrap();
    assert!(
        neovm_value::intern::intern_call_names()
            .iter()
            .all(|name| name != "fontification-functions"),
        "redisplay-only capture must not intern or fetch the hook"
    );
    neovm_value::intern::reset_intern_calls();
    let full: BufferLayoutInputState<FontificationHookIdentity> =
        eval.buffer_layout_inputs(buffer).unwrap();
    assert_eq!(
        neovm_value::intern::intern_call_names()
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
    let (mut eval, buffer) = crate::test_utils::upper_layer_helpers::buffer_layout_projection_setup(
        Context::new(crate::test_utils::unit_runtime()),
    );
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
