use super::*;
use neomacs_display_protocol::input_progress::{InputProgress, InputStream};
use winit::keyboard::KeyCode;

const KEY: PhysicalKey = PhysicalKey::Code(KeyCode::KeyP);

#[test]
fn slow_evaluator_has_at_most_one_unread_repeat_without_repeat_debt() {
    let mut repeats = KeyRepeats::default();
    let window = WindowId::from_raw(1);
    let stream = InputStream::default();
    let first = stream.issue().unwrap();
    repeats.queued(window, KEY, Some(first.receipt()));
    for _ in 0..1000 {
        assert!(!repeats.admit(window, KEY, true));
    }
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    progress.consumed(first);
    assert!(repeats.admit(window, KEY, true));
    let next = stream.issue().unwrap();
    repeats.queued(window, KEY, Some(next.receipt()));
    assert!(!repeats.admit(window, KEY, true));
    drop(command);
    assert!(!repeats.admit(window, KEY, true));
    progress.consumed(next);
    assert!(repeats.admit(window, KEY, true));
}

#[test]
fn release_repress_and_other_physical_typing_are_lossless() {
    let mut repeats = KeyRepeats::default();
    let window = WindowId::from_raw(1);
    let old = InputStream::default().issue().unwrap();
    repeats.queued(window, KEY, Some(old.receipt()));
    assert!(repeats.admit(window, PhysicalKey::Code(KeyCode::KeyX), false));
    assert!(repeats.admit(window, KEY, false));
    repeats.release(window, KEY);
    assert!(!repeats.admit(window, KEY, true));
    assert!(repeats.admit(window, KEY, false));
    let new = InputStream::default().issue().unwrap();
    repeats.queued(window, KEY, Some(new.receipt()));
    drop(old); // A late old delivery cannot settle the new press.
    assert!(!repeats.admit(window, KEY, true));
    drop(new);
    assert!(repeats.admit(window, KEY, true));
}

#[test]
fn focus_loss_retires_only_its_window_and_no_orphan_repeat_is_admitted() {
    let mut repeats = KeyRepeats::default();
    let one = WindowId::from_raw(1);
    let two = WindowId::from_raw(2);
    let first = InputStream::default().issue().unwrap();
    let second = InputStream::default().issue().unwrap();
    repeats.queued(one, KEY, Some(first.receipt()));
    repeats.queued(two, KEY, Some(second.receipt()));
    repeats.retire_window(one);
    drop(first);
    assert!(!repeats.admit(one, KEY, true));
    assert!(!repeats.admit(two, KEY, true));
    drop(second);
    assert!(repeats.admit(two, KEY, true));
    assert!(repeats.admit(one, KEY, false));
}
