//! Redisplay scheduling omits only the effective fontification hook identity.
//!
//! Re-homed from neovm-display (display upper-layer re-home): the test needs
//! the core upper layers that the display unit runtime refuses.
use crate::window::BufferLayoutInputState;
use crate::window::display::WindowDisplayContextExt;
use neovm_display::test_support::authority::buffer_layout_projection_setup_for_test as buffer_layout_projection_setup;
use neovm_edit::buffer::BufferId;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::data::layout_input::FontificationHookIdentity;
use neovm_value::value::Value;

#[test]
fn both_projections_keep_owned_prefix_mutations_without_running_when() {
    let (mut eval, buffer) = buffer_layout_projection_setup(crate::test_utils::new_context());
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
