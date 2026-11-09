//! resources_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::display::*;
use crate::emacs_core::display_host::GuiResourceQuery;
use neovm_value::value::Value;

#[test]
fn resource_name_normalization_updates_the_visible_buffer_local_binding() {
    let mut eval = crate::test_utils::new_context();
    eval.set_variable("initial-window-system", Value::symbol("neo"));
    assert_eq!(
        eval.eval_str(
            r#"
        (progn
          (set-default 'x-resource-name "global")
          (set-buffer (get-buffer-create "*resource-local*"))
          (make-local-variable 'x-resource-name)
          (setq x-resource-name "local.name")
          (x-get-resource "font" "Font")
          (list x-resource-name (default-value 'x-resource-name)))
    "#
        )
        .unwrap()
        .to_string(),
        r#"("local_name" "global")"#
    );
}
