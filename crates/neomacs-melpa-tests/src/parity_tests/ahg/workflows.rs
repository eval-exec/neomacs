use expect_test::expect;

use super::ParityBatchCase;

/// Opening the status view of a repository with real working-tree changes.
///
/// `hg status' reports one modified file and one untracked file, and aHg also
/// asks for `hg summary' to build the header.  Both answers are replayed from
/// real Mercurial 7.1, which matters most for `summary': its real output leads
/// with `parent:' and carries `update:' and `phases:' lines, and a header
/// parser keyed on line order would happily pass against a differently ordered
/// invention.
///
/// The exact argument vectors are asserted beside the rendered buffer, because
/// the buffer alone cannot say whether aHg asked Mercurial the right question.
fn the_status_view_renders_mercurials_own_status_and_summary_for_a_dirty_tree() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_status_view_renders_mercurials_own_status_and_summary_for_a_dirty_tree",
        r##"(progn
  (ahg-test-install-hg)
  (let* ((root (ahg-test-repo "repoA"))
         (default-directory root))
    (ahg-status)
    (ahg-test-settle 15)
    (list :status (ahg-test-buffer "*hg status:")
          :mode (ahg-test-buffer-mode "*hg status:")
          :calls (ahg-test-calls)
          :unrecorded (ahg-test-unrecorded))))"##,
        expect![[
            r#"OK (:status "hg status for [ORACLE-SANDBOX]/repoA/\n\n M docs/guide.md\n ? notes.todo\n\n-------------------------------------------------------------------------------\nparent: 2:60eb783c89a0 tip\n Ship release safely\ncommit: 1 modified, 1 unknown\nupdate: (current)\nphases: 3 draft\n" :mode ahg-status-mode :calls ("repoA: --config ui.report_untrusted=0 status" "repoA: --config ui.report_untrusted=0 summary") :unrecorded nil)"#
        ]],
    )
}

fn both_log_views_are_parsed_from_mercurials_own_template_and_style_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "both_log_views_are_parsed_from_mercurials_own_template_and_style_output",
        r##"(progn
  (ahg-test-install-hg)
  (let* ((root (ahg-test-repo "repoA"))
         (default-directory root)
         (observed nil))
    (ahg-short-log "0" "2")
    (ahg-test-settle 15)
    (push (list :short-log (ahg-test-buffer "*hg log (summary):")
                :mode (ahg-test-buffer-mode "*hg log (summary):"))
          observed)
    (ahg-log "0" "2")
    (ahg-test-settle 15)
    (push (list :detailed-log (ahg-test-buffer "*hg log (details):")
                :mode (ahg-test-buffer-mode "*hg log (details):"))
          observed)
    (push (list :calls (ahg-test-calls)
                :unrecorded (ahg-test-unrecorded))
          observed)
    (nreverse observed)))"##,
        expect![[
            r#"OK ((:short-log "hg log (summary) for [ORACLE-SANDBOX]/repoA/\n\n--------------------------------------------------------------------------------\n    Rev |    Date    |  Author  | Summary\n--------------------------------------------------------------------------------\n      0 | 2023-11-14 |    grace | Bootstrap repository                          \n      1 | 2023-11-15 |    grace | Add rollback procedure                        \n      2*| 2023-11-16 |      ada | Ship release safely                           \n--------------------------------------------------------------------------------\n" :mode ahg-short-log-mode) (:detailed-log "hg log for [ORACLE-SANDBOX]/repoA/\n\nchangeset:   0:84d4a1540886\nphase:       draft\nuser:        Grace Hopper <grace@example.test>\ndate:        Tue Nov 14 22:13:20 2023 +0000\nfiles:       docs/guide.md\n             src/main.el\ndescription:\nBootstrap repository\n\n\nchangeset:   1:9eb7836204d1\nphase:       draft\nuser:        Grace Hopper <grace@example.test>\ndate:        Wed Nov 15 22:13:20 2023 +0000\nfiles:       src/main.el\ndescription:\nAdd rollback procedure\n\n\nchangeset:   2:60eb783c89a0\nphase:       draft\ntag:         tip\nuser:        Ada Lovelace <ada@example.test>\ndate:        Thu Nov 16 22:13:20 2023 +0000\nfiles:       src/main.el\ndescription:\nShip release safely\n\n\n" :mode ahg-log-mode) (:calls ("repoA: --config ui.report_untrusted=0 log -r 0:2 --template {rev} {date|shortdate} {author|user} {desc|firstline}\\n" "repoA: --config ui.report_untrusted=0 log -r . --template {rev} " "repoA: --config ui.report_untrusted=0 log -r 0:2 --style .hg/ahg-log-style-map") :unrecorded nil))"#
        ]],
    )
    .fresh_process()
}

fn the_diff_view_renders_mercurials_git_style_diff_of_the_working_tree() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_diff_view_renders_mercurials_git_style_diff_of_the_working_tree",
        r##"(progn
  (ahg-test-install-hg)
  (let* ((root (ahg-test-repo "repoA"))
         (default-directory root))
    (ahg-diff)
    (ahg-test-settle 15)
    (list :diff (ahg-test-buffer "*aHg-diff*")
          :mode (ahg-test-buffer-mode "*aHg-diff*")
          :calls (ahg-test-calls)
          :unrecorded (ahg-test-unrecorded))))"##,
        expect![[
            r#"OK (:diff "diff --git a/docs/guide.md b/docs/guide.md\n--- a/docs/guide.md\n+++ b/docs/guide.md\n@@ -1,3 +1,4 @@\n # Release guide\n \n Deploy after review.\n+Rollback if monitoring fails.\n" :mode ahg-diff-mode :calls ("repoA: --config ui.report_untrusted=0 diff --git" "repoA: --config ui.report_untrusted=0 log -r . --template {node|short} ") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn annotate_renders_mercurials_column_aligned_blame_for_a_real_source_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "annotate_renders_mercurials_column_aligned_blame_for_a_real_source_file",
        r##"(progn
  (ahg-test-install-hg)
  (let* ((root (ahg-test-repo "repoA"))
         (default-directory root)
         (source (expand-file-name "src/main.el" root))
         (buffer (let ((enable-dir-local-variables nil))
                   (find-file-noselect source)))
         (bare (list :word-at-point (fboundp 'word-at-point)
                     :thing-at-point (fboundp 'thing-at-point)
                     :ahg-requires-thingatpt (featurep 'thingatpt))))
    (require 'thingatpt)
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (ahg-annotate)
    (ahg-test-settle 15)
    (list :in-a-bare-session bare
          :annotate (ahg-test-buffer "*hg annotate:")
          :source-still-current (buffer-name buffer)
          :calls (ahg-test-calls)
          :unrecorded (ahg-test-unrecorded))))"##,
        expect![[
            r#"OK (:in-a-bare-session (:word-at-point nil :thing-at-point t :ahg-requires-thingatpt nil) :annotate "  ada 2 2023-11-16:1: (defun deploy-release ()\n  ada 2 2023-11-16:2:   (message \"release ready\"))\ngrace 1 2023-11-15:3: \ngrace 1 2023-11-15:4: (defun rollback-release ()\ngrace 1 2023-11-15:5:   (message \"rollback ready\"))\n" :source-still-current "main.el" :calls ("repoA: --config ui.report_untrusted=0 log --template {rev} {desc|firstline}\\n src/main.el" "repoA: --config ui.report_untrusted=0 annotate -undql src/main.el") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn the_patch_queue_view_shows_the_real_guard_that_kept_a_patch_unapplied() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_patch_queue_view_shows_the_real_guard_that_kept_a_patch_unapplied",
        r##"(progn
  (ahg-test-install-hg)
  (let* ((root (ahg-test-repo "repoB" t))
         (default-directory root))
    (ahg-mq-list-patches)
    (ahg-test-settle 15)
    (list :patches (ahg-test-buffer "*aHg mq patches for:")
          :mode (ahg-test-buffer-mode "*aHg mq patches for:")
          :calls (ahg-test-calls)
          :unrecorded (ahg-test-unrecorded))))"##,
        expect![[
            r#"OK (:patches "mq patch queue for [ORACLE-SANDBOX]/repoB/\n\n--------------------------------------------------------------------------------\n Index | App | Patch (Guards)\n--------------------------------------------------------------------------------\n     0 |     | release-candidate (+linux -windows)                              \n     1 |     | cleanup                                                          \n--------------------------------------------------------------------------------\n" :mode ahg-mq-patches-mode :calls ("repoB: --config ui.report_untrusted=0 qseries" "repoB: --config ui.report_untrusted=0 qapplied" "repoB: --config ui.report_untrusted=0 qguard -l") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_status_view_renders_mercurials_own_status_and_summary_for_a_dirty_tree(),
        both_log_views_are_parsed_from_mercurials_own_template_and_style_output(),
        the_diff_view_renders_mercurials_git_style_diff_of_the_working_tree(),
        annotate_renders_mercurials_column_aligned_blame_for_a_real_source_file(),
        the_patch_queue_view_shows_the_real_guard_that_kept_a_patch_unapplied(),
    ]
}
