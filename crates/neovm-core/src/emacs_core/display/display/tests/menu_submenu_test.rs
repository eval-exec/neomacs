//! Symbol-backed keymaps must retain submenu presentation semantics.

use super::*;

#[test]
fn gui_menu_symbolic_keymap_preserves_submenu_indicator() {
    let mut eval = crate::emacs_core::Context::new();
    let menu = eval
        .eval_str(
            r#"
        (progn
          (fset 'facemenu-probe '(keymap (child menu-item "Child" ignore)))
          '(keymap (text-properties menu-item "Text Properties" facemenu-probe)))
    "#,
        )
        .unwrap();
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));
    tx.send(crate::keyboard::InputEvent::MenuSelection {
        index: 1,
        token: None,
    })
    .unwrap();
    let result = super::super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .expect("select submenu child");
    let shown = shown.lock().unwrap();
    let entries = &shown[0].entries;
    assert_eq!(entries[0].label, "Text Properties");
    assert!(
        entries[0].submenu(),
        "symbol-backed keymap must publish submenu=true so the painter draws its triangle: {entries:?}"
    );
    assert_eq!(entries.len(), 2, "submenu must also contain the child item");
    assert_eq!(
        list_to_vec(&result).unwrap(),
        vec![Value::symbol("text-properties"), Value::symbol("child")]
    );
}
