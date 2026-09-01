use super::*;

// ---- EventKind discriminant values ----

#[test]
fn event_kind_discriminants() {
    assert_eq!(EventKind::KeyPress as u32, 1);
    assert_eq!(EventKind::KeyRelease as u32, 2);
    assert_eq!(EventKind::MousePress as u32, 3);
    assert_eq!(EventKind::MouseRelease as u32, 4);
    assert_eq!(EventKind::MouseMove as u32, 5);
    assert_eq!(EventKind::Scroll as u32, 6);
    assert_eq!(EventKind::Resize as u32, 7);
    assert_eq!(EventKind::CloseRequest as u32, 8);
    assert_eq!(EventKind::FocusIn as u32, 9);
    assert_eq!(EventKind::FocusOut as u32, 10);
    assert_eq!(EventKind::ImageDimensionsReady as u32, 11);
    assert_eq!(EventKind::TerminalExited as u32, 12);
    assert_eq!(EventKind::MenuSelection as u32, 13);
    assert_eq!(EventKind::FileDrop as u32, 14);
    assert_eq!(EventKind::TerminalTitleChanged as u32, 15);
}

// ---- FFI event kind constants match enum ----

#[test]
fn ffi_event_constants_match_enum() {
    assert_eq!(NEOMACS_EVENT_KEY_PRESS, EventKind::KeyPress as u32);
    assert_eq!(NEOMACS_EVENT_KEY_RELEASE, EventKind::KeyRelease as u32);
    assert_eq!(NEOMACS_EVENT_BUTTON_PRESS, EventKind::MousePress as u32);
    assert_eq!(NEOMACS_EVENT_BUTTON_RELEASE, EventKind::MouseRelease as u32);
    assert_eq!(NEOMACS_EVENT_MOUSE_MOVE, EventKind::MouseMove as u32);
    assert_eq!(NEOMACS_EVENT_SCROLL, EventKind::Scroll as u32);
    assert_eq!(NEOMACS_EVENT_RESIZE, EventKind::Resize as u32);
    assert_eq!(NEOMACS_EVENT_CLOSE, EventKind::CloseRequest as u32);
    assert_eq!(NEOMACS_EVENT_FOCUS_IN, EventKind::FocusIn as u32);
    assert_eq!(NEOMACS_EVENT_FOCUS_OUT, EventKind::FocusOut as u32);
    assert_eq!(
        NEOMACS_EVENT_IMAGE_DIMENSIONS_READY,
        EventKind::ImageDimensionsReady as u32
    );
    assert_eq!(
        NEOMACS_EVENT_TERMINAL_EXITED,
        EventKind::TerminalExited as u32
    );
    assert_eq!(
        NEOMACS_EVENT_MENU_SELECTION,
        EventKind::MenuSelection as u32
    );
    assert_eq!(NEOMACS_EVENT_FILE_DROP, EventKind::FileDrop as u32);
    assert_eq!(
        NEOMACS_EVENT_TERMINAL_TITLE_CHANGED,
        EventKind::TerminalTitleChanged as u32
    );
}

// ---- Modifier mask constants ----

#[test]
fn modifier_mask_values() {
    assert_eq!(NEOMACS_SHIFT_MASK, 1);
    assert_eq!(NEOMACS_CTRL_MASK, 2);
    assert_eq!(NEOMACS_META_MASK, 4);
    assert_eq!(NEOMACS_SUPER_MASK, 8);
}

#[test]
fn modifier_masks_are_distinct_bits() {
    // Each mask should be a single distinct bit (no overlap).
    let masks = [
        NEOMACS_SHIFT_MASK,
        NEOMACS_CTRL_MASK,
        NEOMACS_META_MASK,
        NEOMACS_SUPER_MASK,
    ];
    for i in 0..masks.len() {
        assert!(
            masks[i].is_power_of_two(),
            "mask {} is not a power of two",
            masks[i]
        );
        for j in (i + 1)..masks.len() {
            assert_eq!(
                masks[i] & masks[j],
                0,
                "masks {} and {} overlap",
                masks[i],
                masks[j]
            );
        }
    }
}

#[test]
fn modifier_masks_can_be_combined() {
    let ctrl_meta = NEOMACS_CTRL_MASK | NEOMACS_META_MASK;
    assert_eq!(ctrl_meta, 6);
    assert_ne!(ctrl_meta & NEOMACS_CTRL_MASK, 0);
    assert_ne!(ctrl_meta & NEOMACS_META_MASK, 0);
    assert_eq!(ctrl_meta & NEOMACS_SHIFT_MASK, 0);
    assert_eq!(ctrl_meta & NEOMACS_SUPER_MASK, 0);
}

// ---- NeomacsInputEvent default ----

#[test]
fn input_event_default_all_zeroed() {
    let evt = NeomacsInputEvent::default();
    assert_eq!(evt.kind, 0);
    assert_eq!(evt.window_id, 0);
    assert_eq!(evt.timestamp, 0);
    assert_eq!(evt.x, 0);
    assert_eq!(evt.y, 0);
    assert_eq!(evt.keycode, 0);
    assert_eq!(evt.keysym, 0);
    assert_eq!(evt.modifiers, 0);
    assert_eq!(evt.button, 0);
    assert_eq!(evt.scroll_delta_x, 0.0);
    assert_eq!(evt.scroll_delta_y, 0.0);
    assert_eq!(evt.pixel_precise, 0);
    assert_eq!(evt.width, 0);
    assert_eq!(evt.height, 0);
    assert_eq!(evt.target_frame_id, 0);
}

// ---- NeomacsInputEvent field mutation ----

#[test]
fn input_event_field_mutation() {
    let mut evt = NeomacsInputEvent::default();
    evt.kind = NEOMACS_EVENT_KEY_PRESS;
    evt.window_id = 42;
    evt.timestamp = 123456789;
    evt.x = -100;
    evt.y = 200;
    evt.keycode = 65; // 'A'
    evt.keysym = 0x61; // 'a'
    evt.modifiers = NEOMACS_CTRL_MASK | NEOMACS_SHIFT_MASK;
    evt.button = 1;
    evt.scroll_delta_x = 1.5;
    evt.scroll_delta_y = -2.5;
    evt.pixel_precise = 1;
    evt.width = 1920;
    evt.height = 1080;
    evt.target_frame_id = 0xDEAD_BEEF;

    assert_eq!(evt.kind, 1);
    assert_eq!(evt.window_id, 42);
    assert_eq!(evt.timestamp, 123456789);
    assert_eq!(evt.x, -100);
    assert_eq!(evt.y, 200);
    assert_eq!(evt.keycode, 65);
    assert_eq!(evt.keysym, 0x61);
    assert_eq!(evt.modifiers, 3); // SHIFT | CTRL = 1 | 2 = 3
    assert_eq!(evt.button, 1);
    assert_eq!(evt.scroll_delta_x, 1.5);
    assert_eq!(evt.scroll_delta_y, -2.5);
    assert_eq!(evt.pixel_precise, 1);
    assert_eq!(evt.width, 1920);
    assert_eq!(evt.height, 1080);
    assert_eq!(evt.target_frame_id, 0xDEAD_BEEF);
}

// ---- EventKind traits ----

#[test]
fn event_kind_clone_and_copy() {
    let a = EventKind::Scroll;
    let b = a; // Copy
    let c = a.clone(); // Clone
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn event_kind_equality() {
    assert_eq!(EventKind::KeyPress, EventKind::KeyPress);
    assert_ne!(EventKind::KeyPress, EventKind::KeyRelease);
}

#[test]
fn event_kind_debug_format() {
    let dbg = format!("{:?}", EventKind::CloseRequest);
    assert_eq!(dbg, "CloseRequest");
}

// ---- NeomacsInputEvent clone ----

#[test]
fn input_event_clone() {
    let mut evt = NeomacsInputEvent::default();
    evt.kind = NEOMACS_EVENT_SCROLL;
    evt.scroll_delta_y = -3.0;
    evt.modifiers = NEOMACS_META_MASK;

    let cloned = evt.clone();
    assert_eq!(cloned.kind, evt.kind);
    assert_eq!(cloned.scroll_delta_y, evt.scroll_delta_y);
    assert_eq!(cloned.modifiers, evt.modifiers);
}

// ---- FFI layout: repr(C) struct size sanity ----

#[test]
fn input_event_struct_size_is_reasonable() {
    // The struct has 18 fields of various sizes. Ensure it's at least
    // as large as the sum of field sizes and not absurdly large.
    let size = std::mem::size_of::<NeomacsInputEvent>();
    // Minimum: 4+4+8+4+4+4+4+4+4+4+4+4+4+4+8+4+4+4 = 80 bytes
    assert!(size >= 80, "struct too small: {}", size);
    // Should not exceed a generous upper bound (padding included)
    assert!(size <= 136, "struct unexpectedly large: {}", size);
}

#[test]
fn event_kind_repr_u32_size() {
    assert_eq!(std::mem::size_of::<EventKind>(), std::mem::size_of::<u32>());
}
