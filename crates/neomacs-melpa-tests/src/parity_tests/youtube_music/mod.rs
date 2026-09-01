use crate::{CachedMelpaOracle, YOUTUBE_MUSIC_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YOUTUBE_MUSIC_TEST_PRELUDE: &str = r##"
(require 'youtube-music)

(defvar neomacs-melpa-youtube-music--response-buffers nil)
(defvar neomacs-melpa-youtube-music--ipc-payloads nil)
(defvar neomacs-melpa-youtube-music--ipc-playlist-response nil)

(defun neomacs-melpa-youtube-music--response-buffer (json)
  (let ((buffer (generate-new-buffer " *youtube-music-parity-http*")))
    (push buffer neomacs-melpa-youtube-music--response-buffers)
    (with-current-buffer buffer
      (insert "HTTP/1.1 200 OK\nContent-Type: application/json; charset=utf-8\n\n")
      (insert json)
      (goto-char (point-min)))
    buffer))

(defun neomacs-melpa-youtube-music--cleanup-response-buffers ()
  (dolist (buffer neomacs-melpa-youtube-music--response-buffers)
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))

(defun neomacs-melpa-youtube-music--hash-keys (table)
  (let (keys)
    (maphash (lambda (key _value) (push key keys)) table)
    (sort keys #'string<)))

(defun neomacs-melpa-youtube-music--process-live-p (process)
  (memq process '(neomacs-melpa-youtube-music--mpv
                  neomacs-melpa-youtube-music--ipc)))

(defun neomacs-melpa-youtube-music--process-send-string (_process payload)
  (let* ((message (json-parse-string payload :object-type 'plist))
         (request-id (plist-get message :request_id))
         (command (append (plist-get message :command) nil))
         (data
          (cond
           ((equal command '("get_property" "playlist"))
            (vconcat neomacs-melpa-youtube-music--ipc-playlist-response))
           ((equal command '("get_property" "playlist-pos")) 1)
           (t nil))))
    (push (list request-id command)
          neomacs-melpa-youtube-music--ipc-payloads)
    (youtube-music--ipc-filter
     youtube-music--ipc-process
     (concat
      (json-encode `((request_id . ,request-id)
                     (error . "success")
                     (data . ,data)))
      "\n"))))
"##;

fn youtube_music_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YOUTUBE_MUSIC_MELPA_PIN, "youtube-music.el")
        .expect("prepare pinned youtube-music source below ./tmp")
        .with_prelude(YOUTUBE_MUSIC_TEST_PRELUDE)
}

#[test]
fn youtube_music_package_batch() {
    assert_oracle_batch_cases(
        youtube_music_oracle(),
        "youtube_music_package_batch",
        "youtube_music_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
