use super::*;
use winit::{
    event::{Ime, Modifiers, MouseScrollDelta, TouchPhase},
    keyboard::ModifiersState,
};

#[test]
fn startup_input_preserves_focus_modifier_and_ime_order() {
    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    let events = vec![
        WindowEvent::Focused(true),
        WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
        WindowEvent::Ime(Ime::Enabled),
        WindowEvent::Ime(Ime::Commit("A".into())),
        WindowEvent::ModifiersChanged(Modifiers::default()),
        WindowEvent::Focused(false),
    ];
    for event in &events {
        input.defer_input(event.clone()).unwrap();
    }
    let mut replayed = Vec::new();
    while let Some(event) = input.pop_front() {
        replayed.push(event);
    }
    assert_eq!(replayed, events);
    assert!(input.is_empty());
}

#[test]
fn startup_input_keeps_pointer_and_newer_keyboard_in_native_order() {
    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    input.defer_input(WindowEvent::Focused(true)).unwrap();
    let wheel = WindowEvent::MouseWheel {
        device_id: None,
        delta: MouseScrollDelta::LineDelta(0.0, -1.0),
        phase: TouchPhase::Moved,
    };
    input.defer_input(wheel.clone()).unwrap();
    let commit = WindowEvent::Ime(Ime::Commit("after wheel".into()));
    input.defer_input(commit.clone()).unwrap();

    assert_eq!(input.pop_front(), Some(WindowEvent::Focused(true)));
    // Input arriving after GPU completion cannot overtake the wheel.
    let release = WindowEvent::ModifiersChanged(Modifiers::default());
    input.defer_input(release.clone()).unwrap();
    assert_eq!(input.pop_front(), Some(wheel));
    assert_eq!(input.pop_front(), Some(commit));
    assert_eq!(input.pop_front(), Some(release));
    assert!(input.is_empty());
}

#[test]
fn startup_input_replays_default_quit_chord_before_first_scene() {
    use winit::{
        event::{ElementState, KeyEvent},
        keyboard::{Key, KeyCode, KeyLocation, PhysicalKey},
    };

    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    let events = vec![
        WindowEvent::Focused(true),
        WindowEvent::MouseWheel {
            device_id: None,
            delta: MouseScrollDelta::LineDelta(0.0, -1.0),
            phase: TouchPhase::Moved,
        },
        WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
        WindowEvent::KeyboardInput {
            device_id: None,
            event: KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_key: Key::Character("g".into()),
                text: Some("g".into()),
                location: KeyLocation::Standard,
                state: ElementState::Pressed,
                repeat: false,
                text_with_all_modifiers: Some("\u{7}".into()),
                key_without_modifiers: Key::Character("g".into()),
            },
            is_synthetic: false,
        },
        WindowEvent::ModifiersChanged(Modifiers::default()),
        WindowEvent::Focused(false),
    ];
    for event in &events {
        input.defer_input(event.clone()).unwrap();
    }

    // Native identity is ready; no scene has arrived. Ordinary pointer
    // delivery has an explicit Unpresented fallback, so it must not hold
    // the later Ctrl-G away from the input bridge's quit-request path.
    // Replaying the whole FIFO also cooks G after its actual Ctrl state
    // and preserves the subsequent release and focus transition.
    let mut replayed = Vec::new();
    while let Some(event) = input.pop_front() {
        replayed.push(event);
    }
    assert_eq!(replayed, events);
    assert!(input.is_empty());
}

#[test]
fn startup_input_keeps_lifecycle_events_synchronous() {
    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    let commit = WindowEvent::Ime(Ime::Commit("owned input".into()));
    input.defer_input(commit.clone()).unwrap();
    for event in [
        WindowEvent::CloseRequested,
        WindowEvent::Destroyed,
        WindowEvent::SurfaceResized(winit::dpi::PhysicalSize::new(0, 0)),
        WindowEvent::RedrawRequested,
    ] {
        assert_eq!(input.defer_input(event.clone()), Err(event));
    }
    assert_eq!(input.pop_front(), Some(commit));
    assert!(input.is_empty());
}
