use super::*;
use neomacs_display_protocol::presentation_origin::BufferModiff;

fn window(id: i64, selected: bool) -> WindowInfo {
    WindowInfo {
        window_id: DisplayWindowId::new(id),
        buffer_id: id as u64 + 100,
        window_start: 1,
        window_end: 100,
        buffer_size: 1000,
        buffer_modiff: BufferModiff::new(1),
        bounds: Rect::new(id as f32 * 100.0, 0.0, 100.0, 600.0),
        geometry: neomacs_display_protocol::PresentedWindowGeometry::default(),
        line_number_field: None,
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::from("scratch"),
        buffer_file_name: String::new(),
        modified: false,
    }
}

fn minibuffer(selected: bool) -> WindowInfo {
    let mut info = window(9, selected);
    info.is_minibuffer = true;
    info
}

#[test]
fn the_selection_moving_between_text_windows_is_observed() {
    let before = [window(1, true), window(2, false)];
    let after = [window(1, false), window(2, true)];
    assert_eq!(
        observe_selection(&before, &after),
        Some(SelectionObservation {
            window: DisplayWindowId::new(2),
            bounds: Rect::new(200.0, 0.0, 100.0, 600.0),
        })
    );
}

#[test]
fn the_same_selected_window_is_not_a_switch() {
    let before = [window(1, true), window(2, false)];
    let after = [window(1, true), window(2, false)];
    assert_eq!(observe_selection(&before, &after), None);
}

#[test]
fn selecting_the_minibuffer_is_not_a_window_switch() {
    // Every `M-x` selects the minibuffer. Treating that as a switch would fire
    // the effect on nearly every command.
    let before = [window(1, true), minibuffer(false)];
    let after = [window(1, false), minibuffer(true)];
    assert_eq!(observe_selection(&before, &after), None);
}

#[test]
fn returning_from_the_minibuffer_to_the_same_window_is_not_a_switch() {
    let before = [window(1, false), minibuffer(true)];
    let after = [window(1, true), minibuffer(false)];
    assert_eq!(
        observe_selection(&before, &after),
        None,
        "the previous side has no selected text window to have moved from"
    );
}

#[test]
fn the_first_install_observes_nothing() {
    // No previous presentation means no selection to have moved from.
    let after = [window(1, true)];
    assert_eq!(observe_selection(&[], &after), None);
}

#[test]
fn a_presentation_with_no_selected_text_window_observes_nothing() {
    let before = [window(1, true)];
    let after = [window(1, false), window(2, false)];
    assert_eq!(observe_selection(&before, &after), None);
}
