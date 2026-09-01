use expect_test::expect;

use super::ParityBatchCase;

fn format_switches_reflects_sort_reverse_group_and_time() -> ParityBatchCase {
    ParityBatchCase::value(
        "format_switches_reflects_sort_reverse_group_and_time",
        r####"
(neomacs-dired-quick-sort-test-reset)
(let ((default (dired-quick-sort--format-switches)))
  (setq dired-quick-sort-sort-by-last "size"
        dired-quick-sort-reverse-last ?y
        dired-quick-sort-group-directories-last ?y
        dired-quick-sort-time-last "atime")
  (list :default default
        :custom (dired-quick-sort--format-switches)))
"####,
        expect![[
            r#"OK (:default "-al --sort=version   " :custom "-al --sort=size -r --group-directories-first --time=atime")"#
        ]],
    )
}

fn dired_quick_sort_updates_last_used_criteria() -> ParityBatchCase {
    ParityBatchCase::value(
        "dired_quick_sort_updates_last_used_criteria",
        r####"
(neomacs-dired-quick-sort-test-reset)
(let (calls)
  (cl-letf (((symbol-function 'dired-sort-other)
             (lambda (switches &optional _revert)
               (push switches calls)
               switches)))
    (dired-quick-sort "time" ?y ?n "ctime")
    (list :calls (nreverse calls)
          :sort-by dired-quick-sort-sort-by-last
          :reverse dired-quick-sort-reverse-last
          :group dired-quick-sort-group-directories-last
          :time dired-quick-sort-time-last
          :switches (dired-quick-sort--format-switches))))
"####,
        expect![[
            r#"OK (:calls ("-al --sort=time -r  --time=ctime") :sort-by "time" :reverse 121 :group 110 :time "ctime" :switches "-al --sort=time -r  --time=ctime")"#
        ]],
    )
}

fn nil_arguments_reuse_previous_settings() -> ParityBatchCase {
    ParityBatchCase::value(
        "nil_arguments_reuse_previous_settings",
        r####"
(neomacs-dired-quick-sort-test-reset)
(cl-letf (((symbol-function 'dired-sort-other)
           (lambda (switches &optional _revert) switches)))
  (dired-quick-sort "extension" ?y ?y "status")
  (let ((after (list dired-quick-sort-sort-by-last
                     dired-quick-sort-reverse-last
                     dired-quick-sort-group-directories-last
                     dired-quick-sort-time-last)))
    (dired-quick-sort nil nil nil nil)
    (list :after after
          :reused (list dired-quick-sort-sort-by-last
                        dired-quick-sort-reverse-last
                        dired-quick-sort-group-directories-last
                        dired-quick-sort-time-last)
          :switches (dired-quick-sort--format-switches))))
"####,
        expect![[
            r#"OK (:after ("extension" 121 121 "status") :reused ("extension" 121 121 "status") :switches "-al --sort=extension -r --group-directories-first --time=status")"#
        ]],
    )
}

fn setup_binds_key_and_hook_when_ls_program_is_enabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_binds_key_and_hook_when_ls_program_is_enabled",
        r####"
(let ((ls-lisp-use-insert-directory-program t)
      (dired-mode-map (make-sparse-keymap))
      (dired-mode-hook nil)
      warnings)
  (cl-letf (((symbol-function 'display-warning)
             (lambda (&rest args) (push args warnings)))
            ((symbol-function 'message)
             (lambda (&rest args) (push (cons 'message args) warnings))))
    (dired-quick-sort-setup)
    (list :key (lookup-key dired-mode-map (kbd "S"))
          :hook (and (memq 'dired-quick-sort-set-switches dired-mode-hook) t)
          :warnings warnings)))
"####,
        expect!["OK (:key dired-quick-sort-transient :hook t :warnings nil)"],
    )
}

fn setup_warns_when_ls_lisp_program_is_disabled() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_warns_when_ls_lisp_program_is_disabled",
        r####"
(let ((ls-lisp-use-insert-directory-program nil)
      (dired-quick-sort-suppress-setup-warning nil)
      (dired-mode-map (make-sparse-keymap))
      (dired-mode-hook nil)
      warnings)
  (cl-letf (((symbol-function 'display-warning)
             (lambda (type message &rest _)
               (push (list type (substring-no-properties message)) warnings))))
    (dired-quick-sort-setup)
    (list :key (lookup-key dired-mode-map (kbd "S"))
          :hook (and (memq 'dired-quick-sort-set-switches dired-mode-hook) t)
          :warnings warnings)))
"####,
        expect![[
            r#"OK (:key nil :hook nil :warnings ((dired-quick-sort "`ls-lisp-use-insert-directory-program' is nil. The package `dired-quick-sort'\nwill not work and thus is not set up by `dired-quick-sort-setup'. Set it to t to\nsuppress this warning. Alternatively, set\n`dired-quick-sort-suppress-setup-warning' to suppress warning and skip setup\nsilently.")))"#
        ]],
    )
}

fn savehist_tracks_last_used_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "savehist_tracks_last_used_variables",
        r####"
(list :sort-by (memq 'dired-quick-sort-sort-by-last savehist-additional-variables)
      :reverse (memq 'dired-quick-sort-reverse-last savehist-additional-variables)
      :group (memq 'dired-quick-sort-group-directories-last
                   savehist-additional-variables)
      :time (memq 'dired-quick-sort-time-last savehist-additional-variables)
      :prefix (commandp 'dired-quick-sort-transient))
"####,
        expect![
            "OK (:sort-by #1=(dired-quick-sort-sort-by-last) :reverse #2=(dired-quick-sort-reverse-last . #1#) :group #3=(dired-quick-sort-group-directories-last . #2#) :time (dired-quick-sort-time-last . #3#) :prefix t)"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        format_switches_reflects_sort_reverse_group_and_time(),
        dired_quick_sort_updates_last_used_criteria(),
        nil_arguments_reuse_previous_settings(),
        setup_binds_key_and_hook_when_ls_program_is_enabled(),
        setup_warns_when_ls_lisp_program_is_disabled(),
        savehist_tracks_last_used_variables(),
    ]
}
