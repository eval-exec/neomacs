use expect_test::expect;

use super::ParityBatchCase;

fn agent_recall_loads_the_pinned_package_with_its_real_agent_shell_dependency() -> ParityBatchCase {
    ParityBatchCase::value(
        "agent_recall_loads_the_pinned_package_with_its_real_agent_shell_dependency",
        r##"(let ((descriptor
                                (cadr
                                 (assq
                                  'agent-recall
                                  package-alist))))
                           (list
                            (package-desc-name descriptor)
                            (package-version-join
                             (package-desc-version descriptor))
                            (package-desc-reqs descriptor)
                            (featurep
                             'agent-recall)
                            (featurep
                             'agent-shell)
                            (let ((agent-shell-descriptor
                                   (cadr
                                    (assq
                                     'agent-shell
                                     package-alist))))
                              (and
                               agent-shell-descriptor
                               (version-list-<=
                                '(0 1 0)
                                (package-desc-version
                                 agent-shell-descriptor))))
                            (file-name-base
                             (symbol-file
                              'agent-shell-subscribe-to
                              'defun))
                            (mapcar
                             (lambda (command)
                               (list
                                command
                                (commandp command)))
                             '(agent-recall-reindex
                               agent-recall-search
                               agent-recall-browse
                               agent-recall-resume
                               agent-recall-stats
                               agent-recall-backfill))))"##,
        expect![[
            r#"OK (agent-recall "20260710.1707" ((emacs (29 1)) (agent-shell (0 1 0))) t t t "agent-shell" ((agent-recall-reindex t) (agent-recall-search t) (agent-recall-browse t) (agent-recall-resume t) (agent-recall-stats t) (agent-recall-backfill t)))"#
        ]],
    )
}

fn agent_recall_autoloads_the_user_entry_points_without_loading_the_main_source() -> ParityBatchCase
{
    ParityBatchCase::value(
        "agent_recall_autoloads_the_user_entry_points_without_loading_the_main_source",
        r##"(list
                           (featurep
                            'agent-recall)
                           (mapcar
                            (lambda (command)
                              (let ((definition
                                     (symbol-function command)))
                                (list
                                 command
                                 (autoloadp definition)
                                 (nth 1 definition)
                                 (commandp command))))
                            '(agent-recall-reindex
                              agent-recall-invalidate-cache
                              agent-recall-search
                              agent-recall-search-live
                              agent-recall-browse
                              agent-recall-clean-view
                              agent-recall-resume
                              agent-recall-stats
                              agent-recall-track-sessions
                              agent-recall-backfill)))"##,
        expect![[
            r#"OK (nil ((agent-recall-reindex t "agent-recall" t) (agent-recall-invalidate-cache t "agent-recall" t) (agent-recall-search t "agent-recall" t) (agent-recall-search-live t "agent-recall" t) (agent-recall-browse t "agent-recall" t) (agent-recall-clean-view nil nil nil) (agent-recall-resume t "agent-recall" t) (agent-recall-stats t "agent-recall" t) (agent-recall-track-sessions t "agent-recall" nil) (agent-recall-backfill t "agent-recall" t)))"#
        ]],
    )
}

pub(super) fn smoke_agent_recall_batch_cases() -> Vec<ParityBatchCase> {
    vec![agent_recall_loads_the_pinned_package_with_its_real_agent_shell_dependency()]
}

pub(super) fn smoke_agent_recall_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![agent_recall_autoloads_the_user_entry_points_without_loading_the_main_source()]
}
