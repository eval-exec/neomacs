use crate::{CachedMelpaOracle, YOUTUBE_SUB_EXTRACTOR_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YOUTUBE_SUB_EXTRACTOR_TEST_PRELUDE: &str = r##"
(require 'youtube-sub-extractor)

(defun neomacs-melpa-youtube-sub-extractor--write-executable (path body)
  (with-temp-file path
    (insert "#!/bin/sh\nset -eu\n")
    (insert body))
  (set-file-modes path #o700)
  path)

(defun neomacs-melpa-youtube-sub-extractor--file-string (path)
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun neomacs-melpa-youtube-sub-extractor--overlay-state ()
  (mapcar
   (lambda (overlay)
     (let ((before-string (overlay-get overlay 'before-string)))
       (list (overlay-start overlay)
             (overlay-end overlay)
             (substring-no-properties before-string)
             (get-text-property 0 'display before-string))))
   (sort (overlays-in (point-min) (point-max))
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))
"##;

fn youtube_sub_extractor_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YOUTUBE_SUB_EXTRACTOR_MELPA_PIN, "youtube-sub-extractor.el")
        .expect("prepare pinned youtube-sub-extractor source below ./tmp")
        .with_prelude(YOUTUBE_SUB_EXTRACTOR_TEST_PRELUDE)
}

#[test]
fn youtube_sub_extractor_package_batch() {
    assert_oracle_batch_cases(
        youtube_sub_extractor_oracle(),
        "youtube_sub_extractor_package_batch",
        "youtube_sub_extractor_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
