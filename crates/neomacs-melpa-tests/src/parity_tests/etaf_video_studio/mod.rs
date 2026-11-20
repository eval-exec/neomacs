//! Behavioural parity for the pinned `etaf-video-studio` CLI boundary.
//!
//! A small shell fixture written by this test stands in for `pi-skill`.  It
//! serves one canned project, logs every invocation, and reports a missing
//! project as a CLI error.  The probes drive the public open, state, selection
//! and close entry points, so no real project store, ffmpeg, network or GUI is
//! involved.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CLI_SCRIPT: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$NEOMACS_VS_LOG"
cmd="$1"; sub="$2"; dir="$3"
status='{"ok":true,"id":"video-1","title":"Probe","directory":"/tmp/neomacs-video-studio-probe","revision":2,"plan":{"version":1,"clips":[{"id":"clip-1","file":"/tmp/neomacs-video-studio-clip.mp4","in":0,"out":5,"speed":1,"gain":1}],"captions":[{"id":"cap-1","start":0,"end":2,"text":"hello"}]},"timeline":[],"duration":5,"undo":false,"redo":false,"history":[],"tasks":[]}'
case "$cmd $sub" in
 "video-project list") printf '%s\n' '{"ok":true,"projects":[]}' ;;
 "video-project status")
   if [ "$dir" = "/tmp/neomacs-video-studio-missing" ]; then
     printf '%s\n' '{"ok":false,"error":"notfound","message":"项目不存在"}'
     exit 3
   fi
   printf '%s\n' "$status" ;;
 *) printf '%s\n' "$status" ;;
esac
"#;

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-video-studio)

(defun neomacs-vs-wait (buffer)
  "Wait until BUFFER has no pending CLI request and settle the last reply."
  (with-current-buffer buffer
    (let ((end (+ (float-time) 10)))
      (while (and (< (float-time) end)
                  (> (plist-get (etaf-video-studio-state) :pending) 0))
        (accept-process-output nil 0.05)))
    (sleep-for 0.2)))

(defun neomacs-vs-facts ()
  "Return the studio's public state compactly."
  (let ((state (etaf-video-studio-state)))
    (list :project (plist-get state :project)
          :projects (plist-get state :projects)
          :clips (plist-get state :clips)
          :captions (plist-get state :captions)
          :pane (plist-get state :pane)
          :clip-index (plist-get state :clip-index)
          :pending (plist-get state :pending)
          :message (plist-get state :message))))

(defun neomacs-vs-lines (log)
  "Return the recorded CLI invocations in LOG."
  (when (file-exists-p log)
    (with-temp-buffer
      (insert-file-contents log)
      (split-string (buffer-string) "\n" t))))

(defun neomacs-vs-error (thunk)
  "Return THUNK's error as (TYPE MESSAGE)."
  (condition-case err
      (funcall thunk)
    (error (list (car err) (cadr err)))))
"####;

fn cli_path() -> PathBuf {
    let path = std::env::temp_dir().join("neomacs-video-studio-cli.sh");
    fs::write(&path, CLI_SCRIPT).expect("write the video-studio CLI fixture");
    let mut permissions = fs::metadata(&path)
        .expect("stat the CLI fixture")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("make the CLI fixture executable");
    path
}

fn oracle() -> CachedMelpaOracle {
    let script = cli_path();
    let prelude = format!(
        "{}\n(defconst neomacs-vs-cli \"{}\")\n",
        PRELUDE,
        script.display()
    );
    CachedMelpaOracle::new(("etaf-video-studio", "0.1.1"), "etaf-video-studio.el")
        .expect("prepare pinned etaf-video-studio source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(prelude)
}

#[test]
fn etaf_video_studio_behavior_batch() {
    let cases = vec![
        // Opening a project runs the list/status protocol, adopts the
        // project, selects its first clip, and reports the settled state.
        ParityBatchCase::value(
            "open_project_state",
            r####"
(let* ((log (make-temp-file "neomacs-vs-log-"))
       (process-environment
        (cons (concat "NEOMACS_VS_LOG=" log) process-environment))
       (etaf-video-studio-executable neomacs-vs-cli)
       (buffer (etaf-video-studio-open "/tmp/neomacs-video-studio-probe")))
  (unwind-protect
      (progn
        (neomacs-vs-wait buffer)
        (let ((opened (with-current-buffer buffer (neomacs-vs-facts))))
          (with-current-buffer buffer
            (etaf-video-studio-select-clip 0))
          (list opened
                (with-current-buffer buffer (neomacs-vs-facts))
                (neomacs-vs-lines log))))
    (with-current-buffer buffer
      (ignore-errors (etaf-video-studio-close)))))
"####,
            expect![[r#"OK ((:project (:id "video-1" :title "Probe" :revision 2 :directory "/tmp/neomacs-video-XXXXXX-probe" :undo nil :redo nil) :projects 0 :clips 1 :captions 1 :pane properties :clip-index 0 :pending 0 :message "已刷新。") (:project (:id "video-1" :title "Probe" :revision 2 :directory "/tmp/neomacs-video-XXXXXX-probe" :undo nil :redo nil) :projects 0 :clips 1 :captions 1 :pane properties :clip-index 0 :pending 0 :message "素材文件不存在：/tmp/neomacs-video-XXXXXX-clip.mp4") ("video-project list --json" "video-project status /tmp/neomacs-video-XXXXXX-probe --json" "video-project list --json" "video-project status /tmp/neomacs-video-XXXXXX-probe --json"))"#]],
        ),
        // Closing releases the buffer; state, close and refresh then report
        // the documented out-of-studio error.
        ParityBatchCase::value(
            "close_and_outside_errors",
            r####"
(let* ((log (make-temp-file "neomacs-vs-log-"))
       (process-environment
        (cons (concat "NEOMACS_VS_LOG=" log) process-environment))
       (etaf-video-studio-executable neomacs-vs-cli)
       (buffer (etaf-video-studio-open)))
  (neomacs-vs-wait buffer)
  (with-current-buffer buffer
    (etaf-video-studio-close))
  (list (buffer-live-p buffer)
        (neomacs-vs-error #'etaf-video-studio-state)
        (neomacs-vs-error #'etaf-video-studio-close)
        (neomacs-vs-error #'etaf-video-studio-refresh)))
"####,
            expect![[
                r#"OK (nil (user-error "Not an etaf-video-studio buffer") (user-error "Not an etaf-video-studio buffer") (user-error "Not an etaf-video-studio buffer"))"#
            ]],
        ),
        // A CLI error reply is surfaced as the studio status message and no
        // project is adopted.
        ParityBatchCase::value(
            "missing_project_report",
            r####"
(let* ((log (make-temp-file "neomacs-vs-log-"))
       (process-environment
        (cons (concat "NEOMACS_VS_LOG=" log) process-environment))
       (etaf-video-studio-executable neomacs-vs-cli)
       (buffer (etaf-video-studio-open "/tmp/neomacs-video-studio-missing")))
  (unwind-protect
      (progn
        (neomacs-vs-wait buffer)
        (list (with-current-buffer buffer (neomacs-vs-facts))
              (neomacs-vs-lines log)))
    (with-current-buffer buffer
      (ignore-errors (etaf-video-studio-close)))))
"####,
            expect![[r#"OK ((:project nil :projects 0 :clips 0 :captions 0 :pane projects :clip-index nil :pending 0 :message "失败 [notfound] 项目不存在") ("video-project list --json" "video-project status /tmp/neomacs-video-XXXXXX-missing --json"))"#]],
        ),
        // A missing executable is reported without signalling, leaving the
        // studio empty and usable.
        ParityBatchCase::value(
            "missing_executable",
            r####"
(let ((etaf-video-studio-executable
       "/tmp/neomacs-video-studio-does-not-exist"))
  (let ((buffer (etaf-video-studio-open)))
    (unwind-protect
        (progn
          (neomacs-vs-wait buffer)
          (with-current-buffer buffer (neomacs-vs-facts)))
      (with-current-buffer buffer
        (ignore-errors (etaf-video-studio-close))))))
"####,
            expect![[r#"OK (:project nil :projects 0 :clips 0 :captions 0 :pane projects :clip-index nil :pending 0 :message "失败 [deps] 找不到 /tmp/neomacs-video-XXXXXX-does-not-exist；请先安装或设置 `etaf-video-studio-executable' · pi-skill video-project check")"#]],
        ),
        // Without a project the studio lists projects; an out-of-range clip
        // selection is ignored and a blank title is rejected locally.
        ParityBatchCase::value(
            "no_project_and_validation",
            r####"
(let* ((log (make-temp-file "neomacs-vs-log-"))
       (process-environment
        (cons (concat "NEOMACS_VS_LOG=" log) process-environment))
       (etaf-video-studio-executable neomacs-vs-cli)
       (buffer (etaf-video-studio-open)))
  (unwind-protect
      (with-current-buffer buffer
        (neomacs-vs-wait buffer)
        (let ((opened (neomacs-vs-facts)))
          (etaf-video-studio-select-clip 99)
          (list opened
                (neomacs-vs-facts)
                (neomacs-vs-error
                 (lambda () (etaf-video-studio-create-project "   ")))
                (neomacs-vs-lines log))))
    (with-current-buffer buffer
      (ignore-errors (etaf-video-studio-close)))))
"####,
            expect![[
                r#"OK ((:project nil :projects 0 :clips 0 :captions 0 :pane projects :clip-index nil :pending 0 :message "找到 0 个项目。") (:project nil :projects 0 :clips 0 :captions 0 :pane projects :clip-index nil :pending 0 :message "找到 0 个项目。") (user-error "项目标题不能为空") ("video-project list --json"))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "etaf-video-studio-behavior",
        "etaf-video-studio",
        &cases,
    );
}
