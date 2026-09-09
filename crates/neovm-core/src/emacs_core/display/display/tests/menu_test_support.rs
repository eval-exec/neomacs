use super::*;

pub(super) fn publish(
    eval: &mut Context,
    source: &str,
) -> Result<Vec<crate::emacs_core::PopupMenuEntry>, Flow> {
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(eval);
    eval.frames
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));
    let menu = eval.eval_str(source).expect("menu fixture");
    tx.send(crate::keyboard::InputEvent::MenuSelection {
        index: -1,
        token: None,
    })
    .unwrap();
    super::super::builtin_x_popup_menu(
        eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )?;
    let shown = shown.lock().unwrap();
    Ok(shown
        .first()
        .map(|request| request.entries.clone())
        .unwrap_or_default())
}
