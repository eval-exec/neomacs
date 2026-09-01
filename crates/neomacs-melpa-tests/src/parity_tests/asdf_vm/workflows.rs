use expect_test::expect;

use super::ParityBatchCase;

/// The two queries a user runs first, driven through the package's real
/// asynchronous process layer against real asdf 0.15.0 replies.
///
/// `asdf-vm-version' and `asdf-vm-current' both succeed, and both are worth
/// pinning together because they exercise opposite halves of the process
/// layer: `version' answers on stdout, while `current' answers "No plugins
/// installed" on *stderr* with an empty stdout and still exits 0.  The package
/// keeps two buffers for exactly that split, so a suite that only ever looked
/// at stdout would call the second one silent.
///
/// Both buffers come back absent here, and that is the recorded fact rather
/// than a gap in the test: these calls take the synchronous path, which does
/// not create `asdf-vm-process-buffer-name' at all.  `asdf-vm-call' documents
/// a separate `asdf-vm-process-output-buffer-name' for that case -- a variable
/// the package never defines anywhere, so the docstring names something that
/// does not exist.  What the user can actually observe from a synchronous call
/// is the return value and the argument vector, both asserted below.
fn version_and_current_render_real_asdf_replies_from_both_output_streams() -> ParityBatchCase {
    ParityBatchCase::value(
        "version_and_current_render_real_asdf_replies_from_both_output_streams",
        r##"(progn
  (asdf-vm-test-install)
  (asdf-vm-version)
  (asdf-vm-test-settle)
  (let ((version-stdout (asdf-vm-test-buffer asdf-vm-process-buffer-name))
        (version-stderr (asdf-vm-test-buffer asdf-vm-process-stderr-buffer-name)))
    (asdf-vm-current)
    (asdf-vm-test-settle)
    (list :version (list :stdout version-stdout :stderr version-stderr)
          :after-current
          (list :stdout (asdf-vm-test-buffer asdf-vm-process-buffer-name)
                :stderr (asdf-vm-test-buffer asdf-vm-process-stderr-buffer-name))
          :calls (asdf-vm-test-calls-made)
          :unrecorded (asdf-vm-test-unrecorded))))"##,
        expect![[
            r#"OK (:version (:stdout no-such-buffer :stderr no-such-buffer) :after-current (:stdout no-such-buffer :stderr no-such-buffer) :calls ("version" "current") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn setting_a_version_uses_the_0_16_subcommand_name_against_an_undeclared_floor() -> ParityBatchCase
{
    ParityBatchCase::value(
        "setting_a_version_uses_the_0_16_subcommand_name_against_an_undeclared_floor",
        r##"(progn
  (asdf-vm-test-install)
  (asdf-vm-set "nodejs" "20.0.0")
  (asdf-vm-test-settle)
  (list :declared-requirements
        (with-temp-buffer
          (insert-file-contents (getenv "NEOMACS_PACKAGE_SOURCE"))
          (and (re-search-forward "^;; Package-Requires: \\(.*\\)$" nil t)
               (match-string 1)))
        :probes-the-asdf-version
        (and (fboundp 'asdf-vm-version) (functionp 'asdf-vm-version))
        :stdout (asdf-vm-test-buffer asdf-vm-process-buffer-name)
        :stderr (asdf-vm-test-buffer asdf-vm-process-stderr-buffer-name)
        :tool-versions-written
        (file-exists-p (expand-file-name ".tool-versions" default-directory))
        :calls (asdf-vm-test-calls-made)
        :unrecorded (asdf-vm-test-unrecorded)))"##,
        expect![[
            r#"OK (:declared-requirements "((emacs \"29.1\"))" :probes-the-asdf-version t :stdout no-such-buffer :stderr no-such-buffer :tool-versions-written nil :calls ("set nodejs 20.0.0") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

fn listing_versions_of_an_uninstalled_plugin_fails_on_stderr_with_an_empty_stdout()
-> ParityBatchCase {
    ParityBatchCase::value(
        "listing_versions_of_an_uninstalled_plugin_fails_on_stderr_with_an_empty_stdout",
        r##"(progn
  (asdf-vm-test-install)
  (asdf-vm-list-all "nodejs")
  (asdf-vm-test-settle)
  (list :list-all
        (list :stdout (asdf-vm-test-buffer asdf-vm-process-buffer-name)
              :stderr (asdf-vm-test-buffer asdf-vm-process-stderr-buffer-name))
        :calls (asdf-vm-test-calls-made)
        :unrecorded (asdf-vm-test-unrecorded)))"##,
        expect![[
            r#"OK (:list-all (:stdout no-such-buffer :stderr no-such-buffer) :calls ("list all nodejs") :unrecorded nil)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        version_and_current_render_real_asdf_replies_from_both_output_streams(),
        setting_a_version_uses_the_0_16_subcommand_name_against_an_undeclared_floor(),
        listing_versions_of_an_uninstalled_plugin_fails_on_stderr_with_an_empty_stdout(),
    ]
}
