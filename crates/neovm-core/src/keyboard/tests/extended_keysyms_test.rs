//! #398: named frontend input must remain a bindable Lisp event.

use super::*;

// GNU keyboard.c's function-key names and the toolkit's XF86 names, kept
// literal so a frontend/decoder renaming cannot rewrite this expectation.
const REPORTED_KEYS: [(u32, &str); 31] = [
    (0xffca, "f13"),
    (0xffcb, "f14"),
    (0xffcc, "f15"),
    (0xffcd, "f16"),
    (0xffce, "f17"),
    (0xffcf, "f18"),
    (0xffd0, "f19"),
    (0xffd1, "f20"),
    (0xffd2, "f21"),
    (0xffd3, "f22"),
    (0xffd4, "f23"),
    (0xffd5, "f24"),
    (0xffd6, "f25"),
    (0xffd7, "f26"),
    (0xffd8, "f27"),
    (0xffd9, "f28"),
    (0xffda, "f29"),
    (0xffdb, "f30"),
    (0xffdc, "f31"),
    (0xffdd, "f32"),
    (0xffde, "f33"),
    (0xffdf, "f34"),
    (0xffe0, "f35"),
    (0xff65, "undo"),
    (0xff66, "redo"),
    (0xff67, "menu"),
    (0x1008ff26, "XF86Back"),
    (0x1008ff27, "XF86Forward"),
    (0x1008ff57, "XF86Copy"),
    (0x1008ff58, "XF86Cut"),
    (0x1008ff6d, "XF86Paste"),
];

#[test]
fn extended_keysym_transport_reaches_the_bound_command() {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn
           (defvar issue398-recorded nil)
           (defvar issue398-sequence nil)
           (defun issue398-record-key ()
             (interactive)
             (push (list last-command-event (this-command-keys-vector)) issue398-recorded))
           (switch-to-buffer (get-buffer-create \"*issue398-bindings*\"))
           (use-local-map (make-sparse-keymap)))",
    )
    .unwrap();
    let frame = eval.frame_manager().selected_frame().unwrap().id.0;
    let (sender, receiver) = crossbeam_channel::unbounded();
    eval.input_rx = Some(receiver);
    for (keysym, name) in REPORTED_KEYS {
        eval.eval_str(&format!(
            "(define-key (current-local-map) [{name}] #'issue398-record-key)"
        ))
        .unwrap();
        let event =
            render_key_transport_to_input_event(FrontendKey::Keysym(keysym), 0, true, frame)
                .unwrap_or_else(|| panic!("{name}: named input must not be dropped"));
        sender.send(event).unwrap();
        // Read the actual queued frontend event and resolve its public Lisp
        // binding before replaying exactly that sequence in the command loop.
        let read = eval
            .eval_str("(setq issue398-sequence (read-key-sequence nil))")
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(
            crate::emacs_core::print_value(&read),
            format!("[{name}]"),
            "{name}: frontend channel must deliver the symbolic sequence"
        );
        let binding = eval.eval_str("(key-binding issue398-sequence)").unwrap();
        assert_eq!(
            crate::emacs_core::print_value(&binding),
            "issue398-record-key"
        );
        eval.eval_str("(execute-kbd-macro issue398-sequence)")
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let recorded = eval.eval_str("(car issue398-recorded)").unwrap();
        assert_eq!(
            crate::emacs_core::print_value(&recorded),
            format!("({name} [{name}])"),
            "{name}: transport must invoke the independently named binding"
        );
    }
}

#[test]
fn extended_keysym_transport_retains_command_modifiers() {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn
           (defvar issue398-modified nil)
           (defvar issue398-sequence nil)
           (defun issue398-modified-command ()
             (interactive) (setq issue398-modified last-command-event))
           (switch-to-buffer (get-buffer-create \"*issue398-bindings*\"))
           (use-local-map (make-sparse-keymap))
           (define-key (current-local-map) [C-S-f13] #'issue398-modified-command))",
    )
    .unwrap();
    let frame = eval.frame_manager().selected_frame().unwrap().id.0;
    let (sender, receiver) = crossbeam_channel::unbounded();
    eval.input_rx = Some(receiver);
    sender
        .send(
            render_key_transport_to_input_event(
                FrontendKey::Keysym(0xffca),
                RENDER_CTRL_MASK | RENDER_SHIFT_MASK,
                true,
                frame,
            )
            .unwrap(),
        )
        .unwrap();
    let read = eval
        .eval_str("(setq issue398-sequence (read-key-sequence nil))")
        .unwrap();
    assert_eq!(crate::emacs_core::print_value(&read), "[C-S-f13]");
    let binding = eval.eval_str("(key-binding issue398-sequence)").unwrap();
    assert_eq!(
        crate::emacs_core::print_value(&binding),
        "issue398-modified-command"
    );
    eval.eval_str("(execute-kbd-macro issue398-sequence)")
        .unwrap();
    let recorded = eval.eval_str("issue398-modified").unwrap();
    assert_eq!(crate::emacs_core::print_value(&recorded), "C-S-f13");
}
