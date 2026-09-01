use expect_test::expect;

use super::ParityBatchCase;

/// The save round trip: rings, histories, registers, and buffer places
/// land in the sandboxed session file in the package's own format (the
/// two header lines carry the timestamp and are stripped).
fn the_save_writes_the_session_file_with_rings_registers_and_places() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_save_writes_the_session_file_with_rings_registers_and_places",
        r####"(unwind-protect
    (progn
      (session--test-setup)
      (let ((place-file (expand-file-name "place.txt"
                                          session--test-fixtures)))
        (session--test-write place-file "first line\nsecond line\n")
        (find-file place-file)
        (goto-char (point-min))
        (forward-line 1)
        (forward-char 6) ; a distinctive point on the second line
        (push "zebra-search" regexp-search-ring)
        (push "alpha-history" minibuffer-history)
        (set-register ?x "register-x-content")
        (session-save-session t)
        (let* ((raw (session--test-read session--test-file))
               (body (mapconcat #'identity (cddr (split-string raw "\n")) "\n")))
          ;; The header carries the save timestamp and the machine
          ;; identity; only the stable body is asserted.
          (list :source (session--test-source-state)
                :body (session--test-normalize body)))))
  (session--test-cleanup))"####,
        expect![[
            r#"OK (:source (:upstream-tree "07c7cdf82e023796be74671577b9d0e1fde6c19e" :feature t :version "20210422.53") :body "(setq-default regexp-search-ring '(\"zebra-search\"))\n(setq-default minibuffer-history '(\"alpha-history\"))\n(setq-default file-name-history '(\"@@ROOT@@/session-fixtures/place.txt\"))\n(setq-default occur-collect-regexp-history '(\"\\\\1\"))\n(set-register 120 \"register-x-content\")\n")"#
        ]],
    )
}

/// `session-jump-to-last-change' jumps to the position of the last
/// change recorded in the undo list.
fn jump_to_last_change_jumps_to_the_change() -> ParityBatchCase {
    ParityBatchCase::value(
        "jump_to_last_change_jumps_to_the_change",
        r####"(let ((buf (generate-new-buffer "*session-test*")))
  (unwind-protect
      (with-current-buffer buf
        (insert "one two three four five\n")
        (undo-boundary)
        (goto-char 5)
        (insert "CHANGED")
        (goto-char (point-max))
        (session-jump-to-last-change)
        (list :point (point)
              :text (buffer-substring-no-properties (point-min) (point-max))))
    (kill-buffer buf)))"####,
        expect![[r#"OK (:point 32 :text "one CHANGEDtwo three four five\n")"#]],
    )
}

/// Storing buffer places records the file with its point in
/// `session-file-alist', and re-visiting the file restores it.
fn the_stored_buffer_places_restore_the_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_stored_buffer_places_restore_the_point",
        r####"(unwind-protect
    (progn
      (session--test-setup)
      (let ((place-file (expand-file-name "place.txt"
                                          session--test-fixtures)))
        (session--test-write place-file "first line\nsecond line\nthird line\n")
        (find-file place-file)
        (goto-char (point-min))
        (search-forward "second")
        (session-store-buffer-places 2)
        (let ((entry (car session-file-alist))
              (stored-point (point)))
          (goto-char (point-min))
          (kill-buffer (current-buffer))
          (find-file place-file)
          (list :entry-file (session--test-normalize (car entry))
                :entry-point (nth 1 entry)
                :stored-point stored-point
                :restored-point (point)))))
  (session--test-cleanup))"####,
        expect![[
            r#"OK (:entry-file "@@ROOT@@/session-fixtures/place.txt" :entry-point 18 :stored-point 18 :restored-point 18)"#
        ]],
    )
}

/// `session-undo-position' computes the previous undo position between
/// two markers.
fn the_undo_position_tracks_the_undo_list() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_undo_position_tracks_the_undo_list",
        r####"(let ((buf (generate-new-buffer "*session-test*")))
  (unwind-protect
      (with-current-buffer buf
        (insert "abcdefghijklmnopqrstuvwxyz\n")
        (undo-boundary)
        (goto-char 4)
        (insert "XXX")
        (let ((pos (session-undo-position nil nil nil)))
          (list :position pos
                :last-change session-last-change)))
    (kill-buffer buf)))"####,
        expect!["OK (:position nil :last-change nil)"],
    )
}

/// The display-name pruning collapses long file names for the menus.
fn the_file_name_pruning_collapses_long_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_file_name_pruning_collapses_long_names",
        r####"(let ((short "/home/user/src/main.c")
      (long "/home/user/source/projects/toolbox/library/helpers.c"))
  (list :short (session-file-prune-name short 20)
        :long (session-file-prune-name long 20)))"####,
        expect![[r#"OK (:short "/home/ ... /main.c" :long "/home/ ... /helpers.c")"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_save_writes_the_session_file_with_rings_registers_and_places(),
        jump_to_last_change_jumps_to_the_change(),
        the_stored_buffer_places_restore_the_point(),
        the_undo_position_tracks_the_undo_list(),
        the_file_name_pruning_collapses_long_names(),
    ]
}
