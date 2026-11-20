//! Behavioural parity for the pinned `etaf-ascii-video` player boundary.
//!
//! These probes cover the public validation and lifecycle boundary that runs
//! before any decoder starts: source normalization, render-mode and playback
//! speed checks, unreadable files, non-HTTP URLs, and the user errors every
//! player command reports outside a player buffer.  Frame decoding itself is
//! an ffmpeg/native-runtime contract with timing-dependent output, so it is
//! not pinned here.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-ascii-video)

(defun neomacs-ascii-video-error (thunk)
  "Return THUNK's error as (TYPE MESSAGE)."
  (condition-case err
      (funcall thunk)
    (error (list (car err) (cadr err)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-ascii-video", "0.1.1"), "etaf-ascii-video.el")
        .expect("prepare pinned etaf-ascii-video source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_ascii_video_behavior_batch() {
    let cases = vec![
        // The player rejects an unknown render mode, a non-positive or
        // non-numeric speed, an unreadable file, a non-HTTP URL and an
        // unsupported source before it starts a decoder.
        ParityBatchCase::value(
            "open_validation",
            r####"
(list
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open '(:demo t :mode mono2))))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open '(:demo t :speed 0))))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open '(:demo t :speed -1))))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open '(:demo t :speed "fast"))))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open-file "/tmp/neomacs-definitely-missing.mp4")))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open-url "ftp://example.com/x.mp4")))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open 42)))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-open '(:bogus 1)))))
"####,
            expect![[
                r#"OK ((user-error "Unknown etaf-ascii-video render mode: mono2 (expected mono or color)") (user-error "Playback speed must be a positive number: 0") (user-error "Playback speed must be a positive number: -1") (user-error "Playback speed must be a positive number: \"fast\"") (user-error "Video file is not readable: \"/tmp/neomacs-definitely-missing.mp4\"") (user-error "Video URL must be http or https: \"ftp://example.com/x.mp4\"") (user-error "Unsupported etaf-video source: 42") (user-error "Unsupported etaf-video source: (:bogus 1)"))"#
            ]],
        ),
        // Source normalization publishes the demo shape, the HTTP audio and
        // proxy targets, and rejects invalid kinds and non-HTTP sidecars.
        ParityBatchCase::value(
            "source_normalization",
            r####"
(list
 (etaf-video-source-info '(:kind demo))
 (etaf-video-source-info
  '(:kind url :url "http://example.com/x.mp4"
    :audio-url "https://example.com/a.mp3"
    :proxy "http://proxy:8080"))
 (neomacs-ascii-video-error
  (lambda () (etaf-video-source-info '(:kind bogus))))
 (neomacs-ascii-video-error
  (lambda () (etaf-video-source-info
              '(:kind file :file "/tmp/neomacs-definitely-missing.mp4"))))
 (neomacs-ascii-video-error
  (lambda () (etaf-video-source-info
              '(:kind url :url "http://example.com/x.mp4"
                :audio-url "ftp://example.com/a.mp3"))))
 (neomacs-ascii-video-error
  (lambda () (etaf-video-source-info
              '(:kind url :url "http://example.com/x.mp4"
                :proxy "ftp://proxy")))))
"####,
            expect![[
                r#"OK ((:kind demo :file nil :url nil :target nil :audio-url nil :audio-target nil :headers nil :proxy nil :width nil :height nil :duration nil :fps nil :audio nil) (:kind url :file nil :url "http://example.com/x.mp4" :target "http://example.com/x.mp4" :audio-url "https://example.com/a.mp3" :audio-target "https://example.com/a.mp3" :headers nil :proxy "http://proxy:8080" :width nil :height nil :duration nil :fps nil :audio t) (user-error "Unsupported etaf-video source kind: bogus") (user-error "Video file is not readable: \"/tmp/neomacs-definitely-missing.mp4\"") (user-error "Video audio URL must be http or https: \"ftp://example.com/a.mp3\"") (user-error "Video proxy must be an http or https URL: \"ftp://proxy\""))"#
            ]],
        ),
        // Every player command reports the same user error outside a player
        // buffer instead of touching the current buffer.
        ParityBatchCase::value(
            "operations_outside_player",
            r####"
(list
 (neomacs-ascii-video-error #'etaf-ascii-video-close)
 (neomacs-ascii-video-error #'etaf-ascii-video-toggle-playback)
 (neomacs-ascii-video-error #'etaf-ascii-video-step)
 (neomacs-ascii-video-error (lambda () (etaf-ascii-video-seek 1)))
 (neomacs-ascii-video-error (lambda () (etaf-ascii-video-set-speed 2)))
 (neomacs-ascii-video-error (lambda () (etaf-ascii-video-set-color t)))
 (neomacs-ascii-video-error
  (lambda () (etaf-ascii-video-set-render-mode 'color)))
 (neomacs-ascii-video-error (lambda () (etaf-ascii-video-set-grid 10 5)))
 (neomacs-ascii-video-error #'etaf-ascii-video-fit)
 (neomacs-ascii-video-error #'etaf-ascii-video-state))
"####,
            expect![[
                r#"OK ((user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer") (user-error "Not an etaf-ascii-video buffer"))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "etaf-ascii-video-behavior",
        "etaf-ascii-video",
        &cases,
    );
}
