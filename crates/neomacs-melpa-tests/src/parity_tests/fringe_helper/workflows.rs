use expect_test::expect;

use super::ParityBatchCase;

fn convert_turns_visual_strings_into_fringe_bitmap_vectors() -> ParityBatchCase {
    ParityBatchCase::value(
        "convert_turns_visual_strings_into_fringe_bitmap_vectors",
        r####"
(list :diagonal
      (fringe-helper-convert
       "XX......"
       "..XX...."
       "....XX.."
       "......XX")
      :multiline
      (fringe-helper-convert "XX......\n..XX....\n....XX..\n......XX")
      :single (fringe-helper-convert "XXXXXXXX"))
"####,
        expect!["OK (:diagonal [192 48 12 3] :multiline [192 48 12 3] :single [255])"],
    )
}

fn define_insert_and_remove_manage_point_overlays() -> ParityBatchCase {
    ParityBatchCase::value(
        "define_insert_and_remove_manage_point_overlays",
        r####"
(with-temp-buffer
  (insert "alpha\nbeta\ngamma\n")
  (fringe-helper-define 'neomacs-fringe-helper-test-bitmap 'center
    "XX......"
    "..XX...."
    "....XX.."
    "......XX")
  (goto-char (point-min))
  (search-forward "beta")
  (beginning-of-line)
  (let* ((ov (fringe-helper-insert
              'neomacs-fringe-helper-test-bitmap (point)
              'left-fringe 'font-lock-warning-face))
         (before (neomacs-fringe-helper-test-overlay-shape ov)))
    (fringe-helper-remove ov)
    (list :before before
          :alive (overlay-buffer ov)
          :bitmap (and (fringe-bitmap-p 'neomacs-fringe-helper-test-bitmap) t))))
"####,
        expect![
            "OK (:before (:start 7 :end 7 :helper t :parent nil :display (left-fringe neomacs-fringe-helper-test-bitmap font-lock-warning-face)) :alive nil :bitmap t)"
        ],
    )
}

fn insert_region_covers_each_line_and_remove_clears_children() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_region_covers_each_line_and_remove_clears_children",
        r####"
(with-temp-buffer
  (insert "one\ntwo\nthree\n")
  (fringe-helper-define 'neomacs-fringe-helper-region-bitmap nil
    "XXXXXXXX"
    "XXXXXXXX")
  (let* ((parent (fringe-helper-insert-region
                  (point-min) (point-max)
                  'neomacs-fringe-helper-region-bitmap
                  'right-fringe
                  'font-lock-keyword-face))
         (children
          (cl-remove-if-not
           (lambda (ov)
             (eq (overlay-get ov 'fringe-helper-parent) parent))
           (overlays-in (point-min) (1+ (point-max)))))
         (before
          (list :parent (neomacs-fringe-helper-test-overlay-shape parent)
                :child-count (length children)
                :child-starts
                (sort (mapcar #'overlay-start children) #'<))))
    (fringe-helper-remove parent)
    (list :before before
          :remaining
          (cl-count-if
           (lambda (ov)
             (or (overlay-get ov 'fringe-helper)
                 (overlay-get ov 'fringe-helper-parent)))
           (overlays-in (point-min) (1+ (point-max)))))))
"####,
        expect![
            "OK (:before (:parent (:start 1 :end 15 :helper t :parent nil :display (right-fringe neomacs-fringe-helper-region-bitmap font-lock-keyword-face)) :child-count 2 :child-starts (5 9)) :remaining 0)"
        ],
    )
}

fn stock_library_bitmaps_load_once_and_reuse_symbols() -> ParityBatchCase {
    ParityBatchCase::value(
        "stock_library_bitmaps_load_once_and_reuse_symbols",
        r####"
;; Batch frames often report a nil fringe width; pin a usable size so the
;; stock library can size its patterns the way interactive frames do.
(set-frame-parameter nil 'left-fringe 8)
(set-frame-parameter nil 'right-fringe 8)
(let* ((first (fringe-lib-load fringe-lib-exclamation-mark 'left-fringe))
       (second (fringe-lib-load fringe-lib-exclamation-mark 'left-fringe))
       (question (fringe-lib-load fringe-lib-question-mark 'right-fringe))
       (slash (fringe-lib-load fringe-lib-slash))
       (wave (fringe-lib-load fringe-lib-wave)))
  (list :first first
        :second second
        :same (eq first second)
        :question question
        :slash slash
        :wave wave
        :bitmaps
        (mapcar (lambda (sym) (and (fringe-bitmap-p sym) t))
                (list first question slash wave))))
"####,
        expect![
            "OK (:first fringe-lib-exclamation-mark-5 :second fringe-lib-exclamation-mark-5 :same t :question fringe-lib-question-mark-5 :slash fringe-lib-slash-5 :wave fringe-lib-wave-0 :bitmaps (t t t t))"
        ],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        convert_turns_visual_strings_into_fringe_bitmap_vectors(),
        define_insert_and_remove_manage_point_overlays(),
        insert_region_covers_each_line_and_remove_clears_children(),
        stock_library_bitmaps_load_once_and_reuse_symbols(),
    ]
}
