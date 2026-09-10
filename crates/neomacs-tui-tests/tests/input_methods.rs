//! Input-method composition through the real command loop, compared with GNU.

use crate::support::*;
use neomacs_tui_tests::TuiSession;
use std::time::Duration;

fn boot_tex_pair() -> (TuiSession, TuiSession) {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn (erase-buffer) (fundamental-mode) (set-input-method "TeX") (message "TeX ready"))"##,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.last().is_some_and(|row| row.contains("TeX ready"))
    });
    for (name, editor) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            editor
                .text_grid()
                .last()
                .is_some_and(|row| row.contains("TeX ready")),
            "{name} must activate TeX before testing input:\n{}",
            editor.text_grid().join("\n")
        );
    }
    (gnu, neo)
}

/// GNU Quail displays the pending `-` with its composition face while reading
/// another key: `--` and `---` can still become an en dash or an em dash.
#[test]
fn tex_pending_hyphen_is_visible_while_reading_translation() {
    let (mut gnu, mut neo) = boot_tex_pair();

    send_both(&mut gnu, &mut neo, "-");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.get(1).is_some_and(|row| row.trim() == "-")
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));

    assert_eq!(
        gnu.text_grid()[1].trim(),
        "-",
        "GNU must display composition"
    );
    assert_pair_exact_display(
        "tex_pending_hyphen_is_visible_while_reading_translation",
        &gnu,
        &neo,
    );
}

/// The space ends Quail's nested read and must be replayed as a separate
/// command, not prepended to the translated hyphen's key sequence.
#[test]
fn tex_hyphen_commits_before_following_text() {
    let (mut gnu, mut neo) = boot_tex_pair();

    send_both(&mut gnu, &mut neo, "-");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    send_both(&mut gnu, &mut neo, "SPC x");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.get(1).is_some_and(|row| row.trim() == "- x")
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_eq!(gnu.text_grid()[1].trim(), "- x");
    assert_pair_exact_display("tex_hyphen_commits_before_following_text", &gnu, &neo);
}

fn assert_tex_committed_text(label: &str, keys: &str, expected: &str) {
    let (mut gnu, mut neo) = boot_tex_pair();
    send_both(&mut gnu, &mut neo, keys);
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.get(1).is_some_and(|row| row.trim() == expected)
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_eq!(
        gnu.text_grid()[1].trim(),
        expected,
        "GNU translation for {keys}"
    );
    assert_pair_exact_display(label, &gnu, &neo);
}

#[test]
fn tex_two_hyphens_translate_to_en_dash() {
    assert_tex_committed_text("tex_two_hyphens_translate_to_en_dash", "- - SPC x", "– x");
}

#[test]
fn tex_three_hyphens_translate_to_em_dash() {
    assert_tex_committed_text(
        "tex_three_hyphens_translate_to_em_dash",
        "- - - SPC x",
        "— x",
    );
}

#[test]
fn tex_backslash_pi_translates_to_greek_letter() {
    assert_tex_committed_text(
        "tex_backslash_pi_translates_to_greek_letter",
        r##"\ p i SPC x"##,
        "π x",
    );
}

#[test]
fn tex_nonmatching_letter_preserves_hyphen_and_replays_letter() {
    assert_tex_committed_text(
        "tex_nonmatching_letter_preserves_hyphen_and_replays_letter",
        "- x",
        "-x",
    );
}

#[test]
fn tex_backspace_aborts_pending_translation() {
    assert_tex_committed_text("tex_backspace_aborts_pending_translation", "- DEL x", "x");
}

/// Quail rereads its prefix and terminating key. Neither reread may duplicate
/// physical input in the macro; playback must translate the original keys.
#[test]
fn tex_keyboard_macro_records_physical_keys_once_and_replays() {
    let (mut gnu, mut neo) = boot_tex_pair();
    send_both(&mut gnu, &mut neo, "C-x (");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "- - SPC x");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.get(1).is_some_and(|row| row.trim() == "– x")
    });
    send_both(&mut gnu, &mut neo, "C-x )");
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(message "macro:%S" (append last-kbd-macro nil))"##,
    );
    let expected = "macro:(45 45 32 120)";
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.iter().any(|row| row.contains(expected))
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert!(
        gnu.text_grid().iter().any(|row| row.contains(expected)),
        "{}",
        gnu.text_grid().join("\n")
    );
    assert_pair_exact_display("tex_keyboard_macro_recorded_keys", &gnu, &neo);

    send_both(&mut gnu, &mut neo, "C-x e");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.get(1).is_some_and(|row| row.trim() == "– x– x")
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_eq!(gnu.text_grid()[1].trim(), "– x– x");
    assert_pair_exact_display("tex_keyboard_macro_replayed", &gnu, &neo);
}

/// Queue metadata controls recording independently of key delivery. These
/// results come from GNU keyboard.c's three unread queues, not from Quail's
/// implementation: the input method deliberately maps `-` to `.`.
#[test]
fn unread_input_events_preserve_translation_and_history_policy() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(let ((input-method-function (lambda (key) (list (1+ key)))))
          (message "history:%S"
           (append
            (mapcar
             (lambda (entry)
               (let ((unread-command-events (list entry))
                     (before num-nonmacro-input-events))
                 (list (read-key-sequence-vector nil)
                       (let ((history (recent-keys))
                             (count (- num-nonmacro-input-events before)))
                         (append (seq-subseq history (- (length history) count)) nil))
                       last-input-event)))
             (list 45 (cons t 45) (cons 'no-record 45)))
            (mapcar
             (lambda (queue)
               (let ((unread-input-method-events nil)
                     (unread-post-input-method-events nil)
                     (before num-nonmacro-input-events))
                 (set queue (list 45))
                 (list (read-key-sequence-vector nil)
                       (let ((history (recent-keys))
                             (count (- num-nonmacro-input-events before)))
                         (append (seq-subseq history (- (length history) count)) nil))
                       last-input-event)))
             '(unread-input-method-events unread-post-input-method-events)))))"##,
    );
    let expected =
        "history:(([46] (46) 46) ([46] (46) 46) ([46] nil 46) ([46] (46) 46) ([45] nil 45))";
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.iter().any(|row| row.contains(expected))
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert!(
        gnu.text_grid().iter().any(|row| row.contains(expected)),
        "GNU input history contract:\n{}",
        gnu.text_grid().join("\n")
    );
    assert_pair_exact_display(
        "unread_input_events_preserve_translation_and_history_policy",
        &gnu,
        &neo,
    );
}

/// Physical input enters history before the input method, but GNU does not
/// publish it as last-input-event until the method has returned its result.
#[test]
fn physical_input_history_does_not_publish_last_input_event_early() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (erase-buffer) (fundamental-mode)
          (defvar neomacs-test-input-observation nil)
          (setq-local input-method-function
           (lambda (key)
             (setq neomacs-test-input-observation
              (list key last-input-event
                    (aref (recent-keys) (1- (length (recent-keys))))))
             (list (1+ key))))
          (setq last-input-event 777)
          (message "Input observer ready"))"##,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.last()
            .is_some_and(|row| row.contains("Input observer ready"))
    });
    send_both(&mut gnu, &mut neo, "a");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.get(1).is_some_and(|row| row.trim() == "b")
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_eq!(gnu.text_grid()[1].trim(), "b");
    assert_pair_exact_display("physical_input_translated_text", &gnu, &neo);
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(message "physical:%S" neomacs-test-input-observation)"##,
    );
    let expected = "physical:(97 777 97)";
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.iter().any(|row| row.contains(expected))
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert!(
        gnu.text_grid().iter().any(|row| row.contains(expected)),
        "{}",
        gnu.text_grid().join("\n")
    );
    assert_pair_exact_display("physical_input_history_publication", &gnu, &neo);
}
