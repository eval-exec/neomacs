use super::*;

#[test]
fn default_quit_preserves_bare_key_and_tty_rules() {
    for (key, expected) in [
        (KeyEvent::char_with_mods('g', Modifiers::ctrl()), true),
        (KeyEvent::char('g'), false),
        (KeyEvent::char_with_mods('p', Modifiers::ctrl()), false),
        (
            KeyEvent::named_with_mods(NamedKey::Escape, Modifiers::ctrl()),
            false,
        ),
        (
            KeyEvent::char_with_mods(
                'g',
                Modifiers {
                    ctrl: true,
                    meta: true,
                    ..Modifiers::none()
                },
            ),
            false,
        ),
        (
            KeyEvent::char_with_mods(
                'g',
                Modifiers {
                    ctrl: true,
                    super_: true,
                    ..Modifiers::none()
                },
            ),
            false,
        ),
        (
            KeyEvent::char_with_mods(
                'g',
                Modifiers {
                    ctrl: true,
                    hyper: true,
                    ..Modifiers::none()
                },
            ),
            false,
        ),
    ] {
        assert_eq!(InputEvent::key_press(key).requests_default_quit(), expected);
    }
    for byte in [0x07, b'g', 0x00, 0x1b] {
        let expected = byte == 0x07;
        for event in [
            InputEvent::raw_tty_bytes(vec![b'a', byte, b'b'], 42),
            InputEvent::TtyByte {
                byte,
                target: TtyInputTarget::SelectedFrame,
            },
            InputEvent::TtyCharacter {
                character: crate::emacs_core::emacs_char::EmacsChar::from_char(char::from(byte)),
                target: TtyInputTarget::SelectedFrame,
            },
        ] {
            assert_eq!(event.requests_default_quit(), expected);
        }
    }
    assert!(!InputEvent::LayoutInvalidated.requests_default_quit());
}
#[test]
fn default_quit_inspects_wrappers_without_consuming_input() {
    // The optional recorder is process-global. Enable it in a fresh test
    // process rather than mutating environment or relying on suite order.
    if std::env::var_os("NEOMACS_INPUT_LATENCY_FILE").is_none() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "keyboard::tests::repeat_backpressure::default_quit_inspects_wrappers_without_consuming_input",
                "--nocapture",
            ])
            .env("NEOMACS_INPUT_LATENCY_FILE", file.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use neomacs_display_protocol::{input_latency, input_progress::InputDelivery};
    let token = input_latency::received(
        42,
        "key",
        input_latency::PlatformTimestamp {
            clock_id: 0,
            nanoseconds: 0,
        },
    )
    .unwrap();
    for (key, expected) in [
        (KeyEvent::char_with_mods('g', Modifiers::ctrl()), true),
        (KeyEvent::char('g'), false),
        (KeyEvent::char_with_mods('p', Modifiers::ctrl()), false),
        (
            KeyEvent::char_with_mods(
                'g',
                Modifiers {
                    ctrl: true,
                    meta: true,
                    ..Modifiers::none()
                },
            ),
            false,
        ),
    ] {
        // Bare, tracked, observed, and both orders of nested wrappers.
        for nesting in 0..5 {
            let delivery = InputDelivery::for_read();
            let receipt = delivery.receipt();
            let mut event = InputEvent::key_press(key.clone());
            if matches!(nesting, 2 | 3) {
                event = InputEvent::Observed {
                    token,
                    event: Box::new(event),
                };
            }
            if matches!(nesting, 1 | 3 | 4) {
                event = InputEvent::Tracked {
                    receipt: delivery.clone(),
                    event: Box::new(event),
                };
            }
            if nesting == 4 {
                event = InputEvent::Observed {
                    token,
                    event: Box::new(event),
                };
            }
            assert_eq!(
                event.requests_default_quit(),
                expected,
                "nesting={nesting}, key={key:?}"
            );
            assert!(
                !receipt.consumed_or_cancelled(),
                "inspection must not read or cancel input"
            );
            drop(event);
            assert!(
                !receipt.consumed_or_cancelled(),
                "the retained delivery still owns input"
            );
            drop(delivery);
            assert!(
                receipt.cancelled(),
                "inspection must preserve delivery ownership"
            );
        }
    }
    // The ordinary evaluator reader still recursively consumes each delivery
    // and returns the original C-g event, independently of immediate quit.
    let outer = InputDelivery::for_read();
    let inner = InputDelivery::for_read();
    let receipts = [outer.receipt(), inner.receipt()];
    let event = InputEvent::Tracked {
        receipt: outer,
        event: Box::new(InputEvent::Observed {
            token,
            event: Box::new(InputEvent::Tracked {
                receipt: inner,
                event: Box::new(InputEvent::key_press(KeyEvent::char_with_mods(
                    'g',
                    Modifiers::ctrl(),
                ))),
            }),
        }),
    };
    assert!(event.requests_default_quit());
    assert!(
        receipts
            .iter()
            .all(|receipt| !receipt.consumed_or_cancelled())
    );
    let mut eval = crate::emacs_core::Context::new();
    let value = eval
        .handle_read_char_input_event(event, TtyInputDecoding::KeyboardCodingSystem)
        .unwrap()
        .unwrap();
    assert_eq!(value.as_fixnum(), Some(7));
    assert!(
        receipts
            .iter()
            .all(|receipt| receipt.consumed_or_cancelled() && !receipt.cancelled())
    );
}
