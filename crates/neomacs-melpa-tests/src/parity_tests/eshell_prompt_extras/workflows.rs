use expect_test::expect;

use super::ParityBatchCase;

fn abbrev_dir_name_and_trim_newline_are_stable() -> ParityBatchCase {
    ParityBatchCase::value(
        "abbrev_dir_name_and_trim_newline_are_stable",
        r####"
(list :home (epe-abbrev-dir-name (expand-file-name "~"))
      :leaf (epe-abbrev-dir-name "/tmp/project/src")
      :root (epe-abbrev-dir-name "/")
      :trim (epe-trim-newline "hello\n")
      :no-trim (epe-trim-newline "hello"))
"####,
        expect![[r#"OK (:home "~" :leaf "src" :root "/" :trim "hello" :no-trim "hello")"#]],
    )
}

fn fish_path_shortens_long_directory_chains() -> ParityBatchCase {
    ParityBatchCase::value(
        "fish_path_shortens_long_directory_chains",
        r####"
(let* ((long "/home/user/projects/neomacs/src/parity_tests/eshell")
       (short (epe-fish-path long 12))
       (full (epe-fish-path long 200)))
  (list :short short
        :full full
        :short-shorter (< (length short) (length full))
        :ends-with-eshell
        (and (string-match-p "eshell\\'" short) t)
        :starts-abbreviated
        (and (string-match-p "\\`[^/]/" short) t)))
"####,
        expect![[
            r#"OK (:short "/h/u/p/n/s/p/eshell" :full "/home/user/projects/neomacs/src/parity_tests/eshell" :short-shorter t :ends-with-eshell t :starts-abbreviated nil)"#
        ]],
    )
}

fn status_formatter_and_min_duration_gate() -> ParityBatchCase {
    ParityBatchCase::value(
        "status_formatter_and_min_duration_gate",
        r####"
(let* ((ts (encode-time 0 30 12 7 8 2026))
       (formatted (epe-status-formatter ts 2.5))
       (epe-status--last-command-time (time-subtract (current-time) 3))
       (shown (epe-status #'epe-status-formatter 1))
       (epe-status--last-command-time (time-subtract (current-time) 0.1))
       (hidden (epe-status #'epe-status-formatter 1)))
  (list :formatted formatted
        :has-status-tag (and (string-match-p "STATUS" formatted) t)
        :has-duration (and (string-match-p "2\\.500s" formatted) t)
        :shown-nonempty (and (stringp shown) (> (length shown) 0) t)
        :hidden-empty (and (or (null hidden) (string-empty-p hidden)) t)))
"####,
        expect![[
            r##"OK (:formatted "#[STATUS] End time 2026-08-07 12:30:00, duration 2.500s\n" :has-status-tag t :has-duration t :shown-nonempty t :hidden-empty t)"##
        ]],
    )
}

fn user_name_and_remote_p_are_local_in_sandbox() -> ParityBatchCase {
    ParityBatchCase::value(
        "user_name_and_remote_p_are_local_in_sandbox",
        r####"
;; `epe-date-time' returns today, so the literal value cannot be asserted --
;; it was pinned as "2026-08-08" and both editors started failing the moment
;; the date rolled over, agreeing with each other and disagreeing only with
;; the snapshot. The format is what this case can actually pin, and
;; `:date-matches' already does; the raw value only ever added rot.
(list :remote (epe-remote-p)
      :user (epe-user-name)
      :user-string (and (stringp (epe-user-name)) t)
      :date-matches (and (string-match-p "^[0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\}$"
                                         (epe-date-time "%Y-%m-%d"))
                         t))
"####,
        expect![[r#"OK (:remote nil :user "melpa-test" :user-string t :date-matches t)"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        abbrev_dir_name_and_trim_newline_are_stable(),
        fish_path_shortens_long_directory_chains(),
        status_formatter_and_min_duration_gate(),
        user_name_and_remote_p_are_local_in_sandbox(),
    ]
}
