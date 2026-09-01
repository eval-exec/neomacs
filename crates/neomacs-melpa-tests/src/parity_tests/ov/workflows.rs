use expect_test::expect;

use super::ParityBatchCase;

/// Creating overlays by line, match, regexp, and region; reading their
/// strings and lengths back.
fn creating_overlays_by_line_match_regexp_and_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "creating_overlays_by_line_match_regexp_and_region",
        r####"(with-temp-buffer
  (ov297-test-setup)
  (let ((line-ov (ov-line))
        (match-ovs (ov-match "line"))
        (regexp-ovs (ov-regexp "beta\\|gamma"))
        (region-ov (save-excursion
                     (goto-char (point-min))
                     (forward-char 2)
                     (let ((beg (point)))
                       (end-of-line)
                       (set-mark beg)
                       (setq transient-mark-mode t)
                       (ov-region)))))
    (list :source (ov297-test-source-state)
          :line (list (ov-string line-ov) (ov-length line-ov))
          :match-count (length match-ovs)
          :match-first (ov-string (car match-ovs))
          :regexp-count (length regexp-ovs)
          :regexp-first (ov-string (car regexp-ovs))
          :region (ov-string region-ov))))"####,
        expect![[
            r#"OK (:source (:upstream-tree "2b6bdc185bd29de48a90f2bccaf098428c923fc1" :feature t :version "20230522.1117") :line ("beta line\n" 10) :match-count 4 :match-first "line" :regexp-count 2 :regexp-first "gamma" :region "pha line")"#
        ]],
    )
}

/// `ov-set' sets properties on overlays in bulk (by overlay, list, or
/// regexp re-match) and `ov-all'/`ov-at'/`ov-in' read them back.
fn setting_and_querying_overlay_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "setting_and_querying_overlay_properties",
        r####"(with-temp-buffer
  (ov297-test-setup)
  (let ((ov (ov-line)))
    (ov-set ov 'face 'region)
    (let ((by-single (ov297-test-overlay-state)))
      (ov-set (ov-match "line") 'face 'highlight)
      (let ((by-regexp (ov297-test-overlay-state))
            (at (progn
                  (goto-char (point-min))
                  (forward-line 2)
                  (and (ov-at) t)))
            (in-count (length (ov-in 'face)))
            (all-count (length (ov-all))))
        (list :by-single by-single
              :by-regexp by-regexp
              :at at
              :in-count in-count
              :all-count all-count)))))"####,
        expect![
            "OK (:by-single ((12 22 region)) :by-regexp ((7 11 highlight) (12 22 region) (17 21 highlight) (28 32 highlight) (39 43 highlight)) :at nil :in-count 5 :all-count 5)"
        ],
    )
}

/// Navigation: `ov-next'/`ov-prev' move point between property-matching
/// overlays, and `ov-forwards'/`ov-backwards' report neighbors.
fn navigating_between_overlays() -> ParityBatchCase {
    ParityBatchCase::value(
        "navigating_between_overlays",
        r####"(with-temp-buffer
  (ov297-test-setup)
  (ov-set (ov-line) 'face 'region)
  (goto-char (point-min))
  (forward-line 2)
  (ov-set (ov-line) 'face 'region)
  (goto-char (point-min))
  (let ((first (list :point (point)
                     :next (progn
                             (ov-next 'face)
                             (point)))))
    (let ((second (list :point (point)
                        :next (progn
                                (ov-next 'face)
                                (point)))))
      (list :first first
            :second second
            :prev (progn (ov-prev 'face) (point))
            :forwards (and (ov-forwards) t)
            :backwards (progn
                         (goto-char (point-max))
                         (and (ov-backwards) t))))))"####,
        expect![
            "OK (:first (:point 1 :next 1) :second (:point 1 :next 1) :prev 1 :forwards t :backwards t)"
        ],
    )
}

/// Clearing: `ov-clear' removes all overlays, the property-filtered form
/// removes only matching ones, and the region form removes those inside
/// a range.
fn clearing_overlays_by_all_property_and_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "clearing_overlays_by_all_property_and_region",
        r####"(with-temp-buffer
  (ov297-test-setup)
  (ov-set (ov-line) 'face 'region)
  (goto-char (point-min))
  (forward-line 2)
  (ov-set (ov-line) 'face 'region)
  (goto-char (point-min))
  (forward-line 3)
  (ov-set (ov-line) 'invisible t)
  (let ((before (ov297-test-overlay-state))
        (by-prop (progn
                   (ov-clear 'face)
                   (ov297-test-overlay-state))))
    (ov-clear)
    (list :before before
          :by-prop by-prop
          :after-all (ov297-test-overlay-state))))"####,
        expect![
            "OK (:before ((12 22 region) (22 33 region) (33 44 nil)) :by-prop ((33 44 nil)) :after-all nil)"
        ],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        creating_overlays_by_line_match_regexp_and_region(),
        setting_and_querying_overlay_properties(),
        navigating_between_overlays(),
        clearing_overlays_by_all_property_and_region(),
    ]
}
